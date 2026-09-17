//! `shlane plugin list|lock|verify`.

use crate::config::loader::Discovered;
use crate::error::{Result, ShlaneError};
use crate::plugin::{self, protocol, Loaded};
use std::io::Write;
use std::process::{Command, Stdio};

pub fn list(found: &Discovered) -> Result<()> {
    let plugins = plugin::load_all(&found.config, &found.root)?;
    if plugins.is_empty() {
        println!("no plugins in {}", found.path.display());
        return Ok(());
    }

    let locked = plugin::read_lockfile(&found.root)?;

    for plugin in &plugins {
        let version = plugin.manifest.version.as_deref().unwrap_or("?");
        println!("{} {version}", plugin.manifest.name);
        println!("  path      {}", plugin.directory.display());
        println!("  runs      {}", plugin.executable.display());
        println!("  checksum  sha256:{}", plugin.checksum()?);
        println!(
            "  locked    {}",
            if locked.contains_key(&plugin.manifest.name) {
                "yes"
            } else {
                "no — run `shlane plugin lock`"
            }
        );
        let actions: Vec<&str> = plugin
            .manifest
            .actions
            .iter()
            .map(|action| action.name.as_str())
            .collect();
        println!("  actions   {}\n", actions.join(", "));
    }

    Ok(())
}

pub fn lock(found: &Discovered) -> Result<()> {
    let plugins = plugin::load_all(&found.config, &found.root)?;
    let path = plugin::write_lockfile(&found.root, &plugins)?;
    println!("Wrote {} ({} plugin(s))", path.display(), plugins.len());
    println!("Commit it: a plugin runs with the same permissions as shlane itself.");
    Ok(())
}

/// Ask each plugin to describe itself, and check the answer against what its
/// manifest claims. A manifest that has drifted from the executable is how a
/// step ends up passing arguments nothing reads.
pub fn verify(found: &Discovered) -> Result<()> {
    let plugins = plugin::load_all(&found.config, &found.root)?;
    let mut problems = Vec::new();

    for plugin in &plugins {
        for declared in &plugin.manifest.actions {
            match describe(plugin, &declared.name) {
                Ok((description, reported)) => {
                    let name = &plugin.manifest.name;

                    if declared.description.is_none() {
                        if let Some(description) = description {
                            problems.push(format!(
                                "{name}: '{}' describes itself as \"{description}\" but the manifest gives no description",
                                declared.name
                            ));
                        }
                    }

                    for arg in &reported {
                        let Some(manifest_arg) = declared
                            .args
                            .iter()
                            .find(|candidate| candidate.name == arg.name)
                        else {
                            problems.push(format!(
                                "{name}: '{}' reports an argument '{}' the manifest does not list",
                                declared.name, arg.name
                            ));
                            continue;
                        };

                        // A mismatch here is how a step ends up passing an
                        // argument nothing reads, or a secret that is not masked.
                        if manifest_arg.required != arg.required {
                            problems.push(format!(
                                "{name}: '{}' argument '{}' is required={} in the manifest and required={} in the plugin",
                                declared.name, arg.name, manifest_arg.required, arg.required
                            ));
                        }
                        if manifest_arg.sensitive != arg.sensitive {
                            problems.push(format!(
                                "{name}: '{}' argument '{}' is sensitive={} in the manifest and sensitive={} in the plugin -- a value only the manifest marks is the one that leaks",
                                declared.name, arg.name, manifest_arg.sensitive, arg.sensitive
                            ));
                        }
                        if manifest_arg.default != arg.default {
                            problems.push(format!(
                                "{name}: '{}' argument '{}' has different defaults in the manifest and the plugin",
                                declared.name, arg.name
                            ));
                        }
                        if manifest_arg.description.is_none() && arg.description.is_some() {
                            problems.push(format!(
                                "{name}: '{}' argument '{}' has a description in the plugin but not in the manifest",
                                declared.name, arg.name
                            ));
                        }
                    }

                    for manifest_arg in &declared.args {
                        if !reported.iter().any(|arg| arg.name == manifest_arg.name) {
                            problems.push(format!(
                                "{name}: the manifest lists '{}' for '{}' but the plugin does not report it",
                                manifest_arg.name, declared.name
                            ));
                        }
                    }
                }
                Err(message) => problems.push(format!("{}: {message}", plugin.manifest.name)),
            }
        }
    }

    if problems.is_empty() {
        println!("{} plugin(s) agree with their manifests", plugins.len());
        return Ok(());
    }

    Err(ShlaneError::ConfigProblems {
        path: found.root.join(plugin::MANIFEST),
        problems,
    })
}

type Described = (Option<String>, Vec<protocol::DescribedArg>);

fn describe(plugin: &Loaded, action: &str) -> std::result::Result<Described, String> {
    let request = protocol::request(
        "describe",
        action,
        &std::collections::BTreeMap::new(),
        "",
        &plugin.directory.display().to_string(),
        false,
    );

    let mut child = Command::new(&plugin.executable)
        .current_dir(&plugin.directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("cannot start {}: {err}", plugin.executable.display()))?;

    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(request.as_bytes());
        let _ = stdin.write_all(b"\n");
    }

    let output = child
        .wait_with_output()
        .map_err(|err| format!("{action}: {err}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let (events, _) = protocol::parse_events(&stdout);

    events
        .into_iter()
        .find_map(|event| match event {
            protocol::Event::Describe { description, args } => Some((description, args)),
            _ => None,
        })
        .ok_or_else(|| format!("'{action}' did not answer `describe`"))
}
