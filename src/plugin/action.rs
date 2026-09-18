//! Running a plugin's action.

use super::protocol::{self, Event};
use super::ManifestAction;
use crate::actions::context::ActionContext;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::{Result, ShlaneError};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Turn what a manifest declares into the schema every action exposes.
pub fn schema_from(declared: &ManifestAction) -> Vec<ArgSpec> {
    declared
        .args
        .iter()
        .map(|arg| {
            let mut spec = ArgSpec::new(
                arg.name.clone(),
                arg.description.clone().unwrap_or_default(),
            );
            if arg.required {
                spec = spec.required();
            }
            if let Some(default) = &arg.default {
                spec = spec.default(default.clone());
            }
            if arg.sensitive {
                spec = spec.sensitive();
            }
            spec
        })
        .collect()
}

pub struct PluginAction {
    declared: ManifestAction,
    plugin: String,
    executable: PathBuf,
}

impl PluginAction {
    pub fn new(declared: ManifestAction, plugin: String, executable: PathBuf) -> Self {
        Self {
            declared,
            plugin,
            executable,
        }
    }
}

impl Action for PluginAction {
    fn name(&self) -> &str {
        &self.declared.name
    }

    fn description(&self) -> &str {
        self.declared
            .description
            .as_deref()
            .unwrap_or("(from a plugin)")
    }

    fn schema(&self) -> Vec<ArgSpec> {
        schema_from(&self.declared)
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let values: BTreeMap<String, String> = self
            .schema()
            .iter()
            .filter_map(|spec| {
                args.get(&spec.name)
                    .map(|value| (spec.name.clone(), value.to_string()))
            })
            .collect();

        let request = protocol::request(
            if ctx.dry_run { "dry_run" } else { "run" },
            self.name(),
            &values,
            &ctx.lane,
            &ctx.workdir().display().to_string(),
            ctx.dry_run,
        );

        ctx.ui
            .detail(&format!("plugin {} <- {request}", self.plugin));

        let mut child = Command::new(&self.executable)
            .current_dir(ctx.workdir())
            .envs(ctx.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|err| {
                ctx.error(
                    self.name(),
                    format!("cannot start {}: {err}", self.executable.display()),
                )
            })?;

        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(request.as_bytes());
            let _ = stdin.write_all(b"\n");
        }

        let output = child.wait_with_output().map_err(|err| {
            ctx.error(
                self.name(),
                format!("plugin '{}' failed: {err}", self.plugin),
            )
        })?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let (events, ignored) = protocol::parse_events(&stdout);

        // A plugin's stderr is its own diagnostics, not shlane's.
        for line in stderr.lines().filter(|line| !line.trim().is_empty()) {
            ctx.ui.warn(&format!("{}: {line}", self.plugin));
        }
        for line in ignored {
            ctx.ui.detail(&format!("{}: {line}", self.plugin));
        }

        let mut result = None;
        for event in events {
            match event {
                Event::Log { level, message } => match level.as_str() {
                    "error" => ctx.ui.error(&message),
                    "warn" | "warning" => ctx.ui.warn(&message),
                    _ => ctx.ui.say(&message),
                },
                // Registered before anything else is printed, so a value the
                // plugin discovered at runtime is masked from here on.
                Event::Secret { value } => ctx.mark_secret(&value),
                Event::Result {
                    ok,
                    message,
                    outputs,
                } => result = Some((ok.unwrap_or(true), message, outputs)),
                Event::Describe { .. } => {}
            }
        }

        let Some((ok, message, outputs)) = result else {
            return Err(ctx.error(
                self.name(),
                format!(
                    "plugin '{}' exited with code {} without reporting a result",
                    self.plugin,
                    output.status.code().unwrap_or(-1)
                ),
            ));
        };

        if !ok || !output.status.success() {
            return Err(ShlaneError::Action {
                action: self.name().to_string(),
                message: message.unwrap_or_else(|| {
                    format!(
                        "plugin '{}' reported a failure (exit code {})",
                        self.plugin,
                        output.status.code().unwrap_or(-1)
                    )
                }),
            });
        }

        let mut action_output = ActionOutput::new();
        for (key, value) in outputs {
            action_output = action_output.with(&key, value);
        }
        Ok(action_output)
    }
}
