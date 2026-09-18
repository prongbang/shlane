//! Running a plugin's action.

use super::protocol::{self, Event};
use super::ManifestAction;
use crate::actions::context::ActionContext;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::{Result, ShlaneError};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// How to start a plugin's entry point.
///
/// On unix the file is executed directly and its shebang decides what runs it.
/// Windows has no shebang handling, so a `notify.sh` fails with "%1 is not a
/// valid Win32 application" -- which is what a real plugin written as a script
/// would hit, not only the tests. Steps already run in the POSIX shell that
/// comes with Git for Windows, so anything that is not a native executable goes
/// through that same shell.
fn spawner(executable: &Path, env: &BTreeMap<String, String>) -> Result<Command> {
    if native_executable(executable) {
        return Ok(Command::new(executable));
    }

    let shell = crate::runtime::shell::shell(env)?;
    let mut command = Command::new(shell);
    command.arg("-c").arg(shell_invocation(executable));
    Ok(command)
}

/// Whether the OS can execute this file on its own.
fn native_executable(executable: &Path) -> bool {
    if !cfg!(windows) {
        // A shebang covers every script here.
        return true;
    }
    let extension = executable
        .extension()
        .map(|extension| extension.to_string_lossy().to_ascii_lowercase());
    matches!(
        extension.as_deref(),
        Some("exe" | "com" | "bat" | "cmd") | None
    )
}

/// The command line handed to the POSIX shell.
///
/// Backslashes become forward slashes: the shell that ships with Git for
/// Windows takes `C:/path/to/notify.sh`, while inside quotes a backslash is an
/// escape character rather than a separator.
fn shell_invocation(executable: &Path) -> String {
    let path = executable.display().to_string().replace('\\', "/");
    format!("'{}'", path.replace('\'', "'\\''"))
}

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

        let mut child = spawner(&self.executable, ctx.env)?
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_script_needs_a_shell_only_where_the_os_cannot_run_it() {
        // On unix a shebang covers every script, so nothing is wrapped.
        assert_eq!(
            native_executable(Path::new("plugins/demo/notify.sh")),
            !cfg!(windows)
        );
    }

    #[test]
    fn a_windows_executable_is_started_directly() {
        for name in ["notify.exe", "notify.BAT", "notify.cmd", "notify"] {
            assert!(
                native_executable(Path::new(name)),
                "{name} should not need a shell"
            );
        }
    }

    #[test]
    fn a_windows_path_reaches_the_shell_in_a_form_it_accepts() {
        let invocation = shell_invocation(Path::new(r"C:\Users\runner\plugins\notify.sh"));
        assert_eq!(invocation, "'C:/Users/runner/plugins/notify.sh'");
        assert!(
            !invocation.contains('\\'),
            "a backslash would be read as an escape: {invocation}"
        );
    }

    #[test]
    fn a_quote_in_the_path_cannot_end_the_quoting() {
        let invocation = shell_invocation(Path::new("/tmp/it's here/notify.sh"));
        assert_eq!(invocation, r"'/tmp/it'\''s here/notify.sh'");
    }
}
