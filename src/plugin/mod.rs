//! Loading plugins (`docs/plan/09-plugins.md`).
//!
//! A plugin is a directory with a `shlane-plugin.yaml` manifest and an
//! executable. It can be written in anything; it speaks the JSON protocol in
//! [`protocol`].
//!
//! Only local paths are resolved today. Fetching a plugin from a git host means
//! running someone else's code on the machine that holds the signing keys, so
//! it waits for the lockfile-verified installer the plan describes rather than
//! being half-done here.

pub mod action;
pub mod protocol;

use crate::actions::Action;
use crate::config::model::{Config, PluginRef};
use crate::error::{Result, ShlaneError};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "shlane-plugin.yaml";
pub const LOCKFILE: &str = "shlane-plugins.lock";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    pub protocol: u32,
    /// Path to the executable, relative to the plugin directory.
    pub executable: String,
    #[serde(default)]
    pub actions: Vec<ManifestAction>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ManifestAction {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub args: Vec<ManifestArg>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(deny_unknown_fields)]
pub struct ManifestArg {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub sensitive: bool,
}

/// A plugin that has been found and checked.
pub struct Loaded {
    pub manifest: Manifest,
    pub directory: PathBuf,
    pub executable: PathBuf,
}

impl Loaded {
    /// SHA-256 of the executable, for the lockfile.
    pub fn checksum(&self) -> Result<String> {
        let bytes = fs::read(&self.executable).map_err(|source| ShlaneError::ConfigUnreadable {
            path: self.executable.clone(),
            source,
        })?;
        let digest = ring::digest::digest(&ring::digest::SHA256, &bytes);
        Ok(digest
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }
}

/// Read every plugin the config asks for.
pub fn load_all(config: &Config, root: &Path) -> Result<Vec<Loaded>> {
    let expected = read_lockfile(root)?;
    let mut loaded = Vec::new();

    for reference in &config.plugins {
        let plugin = load_one(reference, root)?;

        // A plugin runs with full permissions on the machine that holds the
        // signing keys, so a recorded checksum has to match.
        if let Some(expected) = expected.get(&plugin.manifest.name) {
            let actual = plugin.checksum()?;
            if &actual != expected {
                return Err(ShlaneError::ConfigProblems {
                    path: root.join(LOCKFILE),
                    problems: vec![format!(
                        "plugin '{}' does not match the lockfile\n    expected sha256:{expected}\n    found    sha256:{actual}\n    run `shlane plugin lock` if the change is expected",
                        plugin.manifest.name
                    )],
                });
            }
        }

        loaded.push(plugin);
    }

    Ok(loaded)
}

fn load_one(reference: &PluginRef, root: &Path) -> Result<Loaded> {
    let problems = |message: String| ShlaneError::ConfigProblems {
        path: root.join("shlane.yaml"),
        problems: vec![message],
    };

    let Some(path) = &reference.path else {
        let named = match &reference.source {
            Some(source) => format!(" (`source: {source}`)"),
            None => String::new(),
        };
        return Err(problems(format!(
            "plugin '{}'{named} has no `path:`. Fetching a plugin from a git host is not implemented yet (docs/plan/09-plugins.md): a plugin runs with full permissions on the machine holding the signing keys, so it waits for the lockfile-verified installer. Vendor it and point `path:` at the directory.",
            reference.name
        )));
    };

    let directory = root.join(path);
    let manifest_path = directory.join(MANIFEST);
    let text = fs::read_to_string(&manifest_path).map_err(|source| {
        problems(format!(
            "plugin '{}': cannot read {}: {source}",
            reference.name,
            manifest_path.display()
        ))
    })?;

    let manifest: Manifest = serde_yaml::from_str(&text).map_err(|err| {
        problems(format!(
            "plugin '{}': {} is invalid: {err}",
            reference.name, MANIFEST
        ))
    })?;

    if manifest.protocol != protocol::VERSION {
        return Err(problems(format!(
            "plugin '{}' speaks protocol {} but this shlane speaks {}",
            manifest.name,
            manifest.protocol,
            protocol::VERSION
        )));
    }

    if manifest.name != reference.name {
        return Err(problems(format!(
            "the config calls this plugin '{}' but its manifest says '{}'",
            reference.name, manifest.name
        )));
    }

    let executable = directory.join(&manifest.executable);
    if !executable.is_file() {
        return Err(problems(format!(
            "plugin '{}': {} does not exist",
            manifest.name,
            executable.display()
        )));
    }

    Ok(Loaded {
        manifest,
        directory,
        executable,
    })
}

/// Turn loaded plugins into actions the registry can hold.
pub fn actions(loaded: Vec<Loaded>) -> Vec<Box<dyn Action>> {
    let mut actions: Vec<Box<dyn Action>> = Vec::new();
    for plugin in loaded {
        for declared in &plugin.manifest.actions {
            actions.push(Box::new(action::PluginAction::new(
                declared.clone(),
                plugin.manifest.name.clone(),
                plugin.executable.clone(),
            )));
        }
    }
    actions
}

/// `name sha256` per line.
pub fn read_lockfile(root: &Path) -> Result<std::collections::BTreeMap<String, String>> {
    let path = root.join(LOCKFILE);
    if !path.is_file() {
        return Ok(std::collections::BTreeMap::new());
    }
    let text = fs::read_to_string(&path).map_err(|source| ShlaneError::ConfigUnreadable {
        path: path.clone(),
        source,
    })?;
    Ok(parse_lockfile(&text))
}

pub fn parse_lockfile(text: &str) -> std::collections::BTreeMap<String, String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| line.split_once(char::is_whitespace))
        .map(|(name, checksum)| {
            (
                name.trim().to_string(),
                checksum.trim().trim_start_matches("sha256:").to_string(),
            )
        })
        .collect()
}

pub fn write_lockfile(root: &Path, plugins: &[Loaded]) -> Result<PathBuf> {
    let path = root.join(LOCKFILE);
    let mut text = String::from("# Written by `shlane plugin lock`. Commit this file.\n");
    for plugin in plugins {
        text.push_str(&format!(
            "{} sha256:{}\n",
            plugin.manifest.name,
            plugin.checksum()?
        ));
    }
    fs::write(&path, text).map_err(|source| ShlaneError::ConfigUnreadable {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_lockfile() {
        let locked = parse_lockfile("# a comment\nline-notify sha256:abc123\n\nother  def456\n");
        assert_eq!(
            locked.get("line-notify").map(String::as_str),
            Some("abc123")
        );
        assert_eq!(locked.get("other").map(String::as_str), Some("def456"));
        assert_eq!(locked.len(), 2);
    }

    #[test]
    fn a_manifest_parses() {
        let manifest: Manifest = serde_yaml::from_str(
            "name: line-notify\nversion: 0.1.0\nprotocol: 1\nexecutable: bin/notify\nactions:\n  - name: notify_line\n    args:\n      - name: token\n        required: true\n        sensitive: true\n",
        )
        .expect("valid");
        assert_eq!(manifest.name, "line-notify");
        assert_eq!(manifest.actions.len(), 1);
        assert!(manifest.actions[0].args[0].sensitive);
    }

    #[test]
    fn a_misspelled_manifest_key_is_rejected() {
        let result: std::result::Result<Manifest, _> =
            serde_yaml::from_str("name: x\nprotocol: 1\nexecutible: bin/notify\n");
        assert!(result.is_err());
    }
}
