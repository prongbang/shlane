//! `shlane plugin list|lock|verify`.

use crate::config::loader::Discovered;
use crate::error::{Result, ShlaneError};
use crate::plugin::{self, protocol, Loaded};
use std::io::Write;
use std::process::Stdio;

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
        println!(
            "  runs      {} ({})",
            plugin.entry.display(),
            plugin.kind.as_str()
        );
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

/// Fetch the plugins the config declares with a `source:`.
pub fn install(found: &Discovered, force: bool) -> Result<()> {
    let outcomes = plugin::install::install_all(&found.config, &found.root, force)?;
    if outcomes.is_empty() {
        println!("no plugins to fetch (a `path:` plugin is already where it needs to be)");
        return Ok(());
    }

    let locked = plugin::read_lockfile(&found.root)?;
    let mut mismatched = Vec::new();

    for outcome in &outcomes {
        if outcome.already_present {
            println!(
                "{} is already installed (--force to fetch it again)",
                outcome.name
            );
            continue;
        }
        println!(
            "Installed {} into {}",
            outcome.name,
            outcome.directory.display()
        );
        if outcome.floating {
            println!(
                "  warning: nothing pins this plugin; add @<tag> to the source so a moved tag cannot change what runs"
            );
        }
    }

    // Anything already in the lockfile has to still match: a tag can be moved
    // after the fact, and that is exactly what the lockfile is for.
    let plugins = plugin::load_all_unverified(&found.config, &found.root)?;
    for installed in &plugins {
        let actual = installed.checksum()?;
        match locked.get(&installed.manifest.name) {
            Some(expected) if expected != &actual => mismatched.push(format!(
                "plugin '{}' does not match the lockfile
    expected sha256:{expected}
    found    sha256:{actual}",
                installed.manifest.name
            )),
            Some(_) => println!("{} matches the lockfile", installed.manifest.name),
            None => println!(
                "{} is not in the lockfile yet: sha256:{actual}",
                installed.manifest.name
            ),
        }
    }

    if !mismatched.is_empty() {
        return Err(ShlaneError::ConfigProblems {
            path: found.root.join(plugin::LOCKFILE),
            problems: mismatched,
        });
    }

    if locked.is_empty() {
        println!("\nRun `shlane plugin lock` and commit the lockfile.");
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
        // A Rhai plugin has no process to ask; checking that it compiles and
        // defines what it promises is the same question.
        if plugin.kind == plugin::Kind::Rhai {
            problems.extend(verify_rhai(plugin));
            continue;
        }

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

/// Compile a Rhai plugin and check it defines a function per declared action.
fn verify_rhai(plugin: &Loaded) -> Vec<String> {
    let engine = rhai::Engine::new();
    let ast = match plugin::rhai_action::compile(&engine, &plugin.entry) {
        Ok(ast) => ast,
        Err(message) => return vec![format!("{}: {message}", plugin.manifest.name)],
    };

    let defined = plugin::rhai_action::function_names(&ast);
    plugin
        .manifest
        .actions
        .iter()
        .filter(|declared| !defined.contains(&declared.name))
        .map(|declared| {
            format!(
                "{}: the manifest declares '{}' but the script defines no such function (it defines: {})",
                plugin.manifest.name,
                declared.name,
                if defined.is_empty() {
                    "nothing".to_string()
                } else {
                    defined.join(", ")
                }
            )
        })
        .collect()
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

    // The same spawn rule a plugin's action uses: on Windows a script cannot be
    // executed directly, so it goes through the POSIX shell. `verify` starting
    // a plugin differently from the way a lane starts it would check something
    // other than what runs.
    let mut child = plugin::action::spawner(&plugin.entry, &std::collections::BTreeMap::new())
        .map_err(|err| err.to_string())?
        .current_dir(&plugin.directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("cannot start {}: {err}", plugin.entry.display()))?;

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

/// Fetch a plugin, declare it in the config, and record its checksum.
///
/// Deliberately separate from `shlane run`: installing a plugin puts somebody
/// else's code on the machine holding the signing keys, so it happens when a
/// person asks for it and never as a side effect of a build.
pub fn add(found: &Discovered, spec: &str) -> Result<()> {
    let problem = |message: String| ShlaneError::ConfigProblems {
        path: found.path.clone(),
        problems: vec![message],
    };

    let source = plugin::source::parse(spec).map_err(problem)?;

    // Fetched into a scratch directory first, because the plugin's real name
    // comes from its manifest and not from the URL it was written as.
    let staging = found.root.join(plugin::install::DIRECTORY).join(".adding");
    let _ = std::fs::remove_dir_all(&staging);
    if let Some(parent) = staging.parent() {
        std::fs::create_dir_all(parent).map_err(|source| ShlaneError::ConfigUnreadable {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    plugin::install::clone(&source, &staging).map_err(problem)?;

    let result = add_fetched(found, spec, &source, &staging);
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

fn add_fetched(
    found: &Discovered,
    spec: &str,
    source: &plugin::source::Source,
    staging: &std::path::Path,
) -> Result<()> {
    let problem = |message: String| ShlaneError::ConfigProblems {
        path: found.path.clone(),
        problems: vec![message],
    };

    let manifest_path = staging.join(plugin::MANIFEST);
    let text = std::fs::read_to_string(&manifest_path).map_err(|_| {
        problem(format!(
            "what was fetched has no {} in it, so it is not a shlane plugin",
            plugin::MANIFEST
        ))
    })?;
    let manifest: plugin::Manifest = serde_yaml::from_str(&text)
        .map_err(|err| problem(format!("{} is not valid: {err}", plugin::MANIFEST)))?;

    if found
        .config
        .plugins
        .iter()
        .any(|declared| declared.name == manifest.name)
    {
        return Err(problem(format!(
            "'{}' is already declared in this config",
            manifest.name
        )));
    }

    let directory = plugin::install::directory_for(&found.root, &manifest.name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::rename(staging, &directory).map_err(|source| ShlaneError::ConfigUnreadable {
        path: directory.clone(),
        source,
    })?;

    let config_text =
        std::fs::read_to_string(&found.path).map_err(|source| ShlaneError::ConfigUnreadable {
            path: found.path.clone(),
            source,
        })?;
    let updated = plugin::declare::insert(&config_text, &manifest.name, spec).map_err(problem)?;
    std::fs::write(&found.path, updated).map_err(|source| ShlaneError::ConfigUnreadable {
        path: found.path.clone(),
        source,
    })?;

    println!(
        "Added {} {} to {}",
        manifest.name,
        manifest.version.as_deref().unwrap_or("(no version)"),
        found.path.display()
    );
    println!("  installed into {}", directory.display());
    if source.is_floating() {
        println!(
            "  warning: nothing pins this plugin; add @<tag> to the source so a moved tag cannot change what runs"
        );
    }

    // Reloaded from disk: the config in hand predates the entry just written.
    let reloaded = crate::config::loader::open(&found.path)?;
    let plugins = plugin::load_all_unverified(&reloaded.config, &reloaded.root)?;
    let path = plugin::write_lockfile(&reloaded.root, &plugins)?;
    println!("  recorded in {}", path.display());
    println!("\nCommit both: a plugin runs with the same permissions as shlane itself.");
    Ok(())
}

/// Undeclare a plugin and delete what was fetched for it.
pub fn remove(found: &Discovered, name: &str, force: bool) -> Result<()> {
    let problem = |message: String| ShlaneError::ConfigProblems {
        path: found.path.clone(),
        problems: vec![message],
    };

    let declared = found
        .config
        .plugins
        .iter()
        .find(|declared| declared.name == name)
        .ok_or_else(|| {
            problem(format!(
                "'{name}' is not declared in this config (declared: {})",
                if found.config.plugins.is_empty() {
                    "none".to_string()
                } else {
                    found
                        .config
                        .plugins
                        .iter()
                        .map(|plugin| plugin.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            ))
        })?;
    let was_fetched = declared.path.is_none();

    // Removing a plugin a lane still calls leaves a config that no longer
    // validates, and the failure surfaces later as "no such action" with
    // nothing to connect it back to this command.
    let still_used = steps_using(found, name)?;
    if !still_used.is_empty() && !force {
        return Err(problem(format!(
            "'{name}' still provides actions this config uses:\n    {}\n  remove those steps first, or pass --force",
            still_used.join("\n    ")
        )));
    }

    let config_text =
        std::fs::read_to_string(&found.path).map_err(|source| ShlaneError::ConfigUnreadable {
            path: found.path.clone(),
            source,
        })?;
    let updated = plugin::declare::remove(&config_text, name).map_err(problem)?;
    std::fs::write(&found.path, updated).map_err(|source| ShlaneError::ConfigUnreadable {
        path: found.path.clone(),
        source,
    })?;
    println!("Removed {name} from {}", found.path.display());

    // Only what shlane fetched is deleted. A `path:` plugin is the user's own
    // directory, and removing a declaration is not permission to delete it.
    if was_fetched {
        let directory = plugin::install::directory_for(&found.root, name);
        if directory.is_dir() {
            std::fs::remove_dir_all(&directory).map_err(|source| {
                ShlaneError::ConfigUnreadable {
                    path: directory.clone(),
                    source,
                }
            })?;
            println!("  deleted {}", directory.display());
        }
    } else {
        println!("  left {} alone: it is not shlane's to delete", name);
    }

    let reloaded = crate::config::loader::open(&found.path)?;
    let plugins = plugin::load_all_unverified(&reloaded.config, &reloaded.root)?;
    let path = plugin::write_lockfile(&reloaded.root, &plugins)?;
    println!("  updated {}", path.display());

    if !still_used.is_empty() {
        println!(
            "\nwarning: this config still calls {} action(s) that are now gone; `shlane validate` will say where",
            still_used.len()
        );
    }
    Ok(())
}

/// Where the config still calls an action this plugin provides, as
/// `lane: action`.
fn steps_using(found: &Discovered, name: &str) -> Result<Vec<String>> {
    use crate::config::model::StepKind;

    let loaded = plugin::load_all_unverified(&found.config, &found.root)?;
    let Some(plugin) = loaded.iter().find(|loaded| loaded.manifest.name == name) else {
        // Declared but never fetched: there is nothing it could be providing.
        return Ok(Vec::new());
    };
    let provides: Vec<&str> = plugin
        .manifest
        .actions
        .iter()
        .map(|action| action.name.as_str())
        .collect();

    let mut used = Vec::new();
    let mut scan = |where_: &str, steps: &[crate::config::model::Step]| {
        for step in steps {
            if let StepKind::Action { name, .. } = &step.kind {
                if provides.contains(&name.as_str()) {
                    used.push(format!("{where_}: {name}"));
                }
            }
        }
    };

    scan("before_all", &found.config.before_all);
    scan("after_all", &found.config.after_all);
    scan("error", &found.config.error);
    for (lane_name, lane) in &found.config.lanes {
        scan(lane_name, &lane.before);
        scan(lane_name, &lane.steps);
        scan(lane_name, &lane.after);
    }

    used.sort();
    used.dedup();
    Ok(used)
}
