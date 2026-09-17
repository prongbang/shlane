//! Fetching a plugin from a git host.
//!
//! Never during a run. A plugin runs with the same permissions as shlane, on
//! the machine holding the signing keys, so installing one is something a
//! person asks for and a lockfile then holds to (`docs/plan/09-plugins.md`).

use super::source::{self, Source};
use super::{Manifest, MANIFEST};
use crate::config::model::{Config, PluginRef};
use crate::error::{Result, ShlaneError};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where fetched plugins live, relative to the config.
pub const DIRECTORY: &str = ".shlane/plugins";

pub struct Outcome {
    pub name: String,
    pub directory: PathBuf,
    pub floating: bool,
    pub already_present: bool,
}

/// Directory a fetched plugin is installed into.
pub fn directory_for(root: &Path, name: &str) -> PathBuf {
    root.join(DIRECTORY).join(name)
}

/// Fetch every plugin the config declares with a `source:`.
pub fn install_all(config: &Config, root: &Path, force: bool) -> Result<Vec<Outcome>> {
    let mut outcomes = Vec::new();

    for reference in &config.plugins {
        // A `path:` plugin is already where it needs to be.
        if reference.path.is_some() {
            continue;
        }
        let Some(spec) = &reference.source else {
            continue;
        };
        outcomes.push(install_one(reference, spec, root, force)?);
    }

    Ok(outcomes)
}

fn install_one(reference: &PluginRef, spec: &str, root: &Path, force: bool) -> Result<Outcome> {
    let problem = |message: String| ShlaneError::ConfigProblems {
        path: root.join("shlane.yaml"),
        problems: vec![message],
    };

    let source = source::parse(spec)
        .map_err(|message| problem(format!("plugin '{}': {message}", reference.name)))?;

    let directory = directory_for(root, &reference.name);
    if directory.join(MANIFEST).is_file() && !force {
        return Ok(Outcome {
            name: reference.name.clone(),
            directory,
            floating: source.is_floating(),
            already_present: true,
        });
    }

    // Fetch into a scratch directory first: a half-finished clone left where a
    // plugin is expected would be run on the next lane.
    let staging = root
        .join(DIRECTORY)
        .join(format!(".{}.fetching", reference.name));
    let _ = fs::remove_dir_all(&staging);
    if let Some(parent) = staging.parent() {
        fs::create_dir_all(parent).map_err(|source| ShlaneError::ConfigUnreadable {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    clone(&source, &staging).map_err(|message| {
        let _ = fs::remove_dir_all(&staging);
        problem(format!("plugin '{}': {message}", reference.name))
    })?;

    let result = check_manifest(&staging, &reference.name);
    if let Err(message) = result {
        let _ = fs::remove_dir_all(&staging);
        return Err(problem(format!("plugin '{}': {message}", reference.name)));
    }

    // The repository's own history is not wanted, and keeping it invites
    // someone to `git pull` a plugin into place without a checksum check.
    let _ = fs::remove_dir_all(staging.join(".git"));

    let _ = fs::remove_dir_all(&directory);
    fs::rename(&staging, &directory).map_err(|source| ShlaneError::ConfigUnreadable {
        path: directory.clone(),
        source,
    })?;

    Ok(Outcome {
        name: reference.name.clone(),
        directory,
        floating: source.is_floating(),
        already_present: false,
    })
}

fn clone(source: &Source, into: &Path) -> std::result::Result<(), String> {
    let mut command = Command::new("git");
    command.arg("clone").arg("--depth").arg("1").arg("--quiet");
    if let Some(reference) = &source.reference {
        command.arg("--branch").arg(reference);
    }
    command.arg(&source.url).arg(into);

    let output = command
        .output()
        .map_err(|err| format!("could not run git: {err}"))?;

    if output.status.success() {
        return Ok(());
    }

    let detail = String::from_utf8_lossy(&output.stderr);
    let detail = detail.trim();
    match &source.reference {
        Some(reference) => Err(format!(
            "could not fetch {} at {reference}: {detail}",
            source.url
        )),
        None => Err(format!("could not fetch {}: {detail}", source.url)),
    }
}

/// A fetched directory has to be a plugin, and the one that was asked for.
fn check_manifest(directory: &Path, expected: &str) -> std::result::Result<(), String> {
    let path = directory.join(MANIFEST);
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("the repository has no {MANIFEST}: {err}"))?;

    let manifest: Manifest =
        serde_yaml::from_str(&text).map_err(|err| format!("{MANIFEST} is invalid: {err}"))?;

    if manifest.name != expected {
        return Err(format!(
            "the config calls this plugin '{expected}' but the repository's manifest says '{}'",
            manifest.name
        ));
    }

    if manifest.protocol != super::protocol::VERSION {
        return Err(format!(
            "it speaks protocol {} but this shlane speaks {}",
            manifest.protocol,
            super::protocol::VERSION
        ));
    }

    let (_, entry) = manifest.entry()?;
    if !directory.join(entry).is_file() {
        return Err(format!(
            "its manifest points at {entry}, which the repository does not contain"
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installs_under_the_config() {
        assert_eq!(
            directory_for(Path::new("/repo"), "line-notify"),
            Path::new("/repo/.shlane/plugins/line-notify")
        );
    }

    #[test]
    fn a_fetched_directory_must_be_the_plugin_that_was_asked_for() {
        let root = std::env::temp_dir().join(format!("shlane-install-{}", std::process::id()));
        fs::create_dir_all(&root).expect("creatable");
        fs::write(root.join("notify.sh"), "#!/bin/sh\n").expect("writable");

        fs::write(
            root.join(MANIFEST),
            "name: other-plugin\nprotocol: 1\nexecutable: notify.sh\nactions: []\n",
        )
        .expect("writable");
        let error = check_manifest(&root, "line-notify").expect_err("names differ");
        assert!(error.contains("manifest says 'other-plugin'"), "{error}");

        fs::write(
            root.join(MANIFEST),
            "name: line-notify\nprotocol: 99\nexecutable: notify.sh\nactions: []\n",
        )
        .expect("writable");
        let error = check_manifest(&root, "line-notify").expect_err("protocol differs");
        assert!(error.contains("protocol 99"), "{error}");

        fs::write(
            root.join(MANIFEST),
            "name: line-notify\nprotocol: 1\nexecutable: missing.sh\nactions: []\n",
        )
        .expect("writable");
        let error = check_manifest(&root, "line-notify").expect_err("executable missing");
        assert!(error.contains("does not contain"), "{error}");

        fs::write(
            root.join(MANIFEST),
            "name: line-notify\nprotocol: 1\nexecutable: notify.sh\nactions: []\n",
        )
        .expect("writable");
        assert!(check_manifest(&root, "line-notify").is_ok());

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_directory_without_a_manifest_is_not_a_plugin() {
        let root = std::env::temp_dir().join(format!("shlane-empty-{}", std::process::id()));
        fs::create_dir_all(&root).expect("creatable");
        let error = check_manifest(&root, "anything").expect_err("no manifest");
        assert!(error.contains(MANIFEST), "{error}");
        let _ = fs::remove_dir_all(&root);
    }
}
