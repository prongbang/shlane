//! What an action is given to work with.

use crate::error::{Result, ShlaneError};
use crate::runtime::context::{Cleanup, SharedCleanups, SharedFrame, SharedOutputs};
use crate::runtime::secrets::SharedSecrets;
use crate::runtime::shell::{self, Spawn};
use crate::runtime::ui::Ui;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub struct ActionContext<'a> {
    pub lane: String,
    pub env: &'a BTreeMap<String, String>,
    pub workdir: PathBuf,
    pub dry_run: bool,
    pub ui: Rc<Ui>,
    pub secrets: SharedSecrets,
    /// The running lane's state, so a plugin written in Rhai can reach the same
    /// builtins a lane's own script has.
    pub frame: SharedFrame,
    pub outputs: SharedOutputs,
    /// Commands to run once the run is over, whatever its result.
    pub cleanups: SharedCleanups,
    /// So a Rhai plugin's `action()` can reach the same registry. Weak: the
    /// registry holds the plugin, and a strong handle back would be a cycle.
    pub registry: std::rc::Weak<crate::actions::Registry>,
    /// Shared nesting depth, so a plugin calling its own action is bounded.
    pub depth: Rc<std::cell::Cell<usize>>,
}

impl ActionContext<'_> {
    pub fn workdir(&self) -> &Path {
        &self.workdir
    }

    /// Ask for a command to be run once the run is over, whether it passed or
    /// failed.
    ///
    /// Registered rather than run in a `Drop`: a cleanup is a real command that
    /// can fail and has something to say about it, and the lane's error hooks
    /// have to see the machine as the failure left it.
    pub fn on_finish(&self, what: impl Into<String>, command: impl Into<String>) {
        self.cleanups.borrow_mut().push(Cleanup {
            what: what.into(),
            command: command.into(),
        });
    }

    /// Hide a value wherever it appears in the output.
    pub fn mark_secret(&self, value: &str) {
        self.secrets.borrow_mut().add(value);
    }

    /// Run a command that changes something. Skipped by `--dry-run`.
    pub fn sh(&self, command: &str) -> Result<shell::Outcome> {
        self.spawn_with(command, false, true, &BTreeMap::new())
    }

    /// Like [`sh`](Self::sh), with extra environment for this command only.
    ///
    /// Used to keep secrets off the command line, where `ps` and CI logs can
    /// see them.
    pub fn sh_with_env(
        &self,
        command: &str,
        extra: &BTreeMap<String, String>,
    ) -> Result<shell::Outcome> {
        self.spawn_with(command, false, true, extra)
    }

    /// Like [`require`](Self::require), with extra environment.
    pub fn require_with_env(
        &self,
        command: &str,
        extra: &BTreeMap<String, String>,
    ) -> Result<shell::Outcome> {
        let outcome = self.sh_with_env(command, extra)?;
        if !outcome.success {
            return Err(self.failed(command, &outcome));
        }
        Ok(outcome)
    }

    /// Run a command and fail the action if it does not succeed.
    pub fn require(&self, command: &str) -> Result<shell::Outcome> {
        let outcome = self.sh(command)?;
        if !outcome.success {
            return Err(self.failed(command, &outcome));
        }
        Ok(outcome)
    }

    /// Read something: the command runs even under `--dry-run`.
    ///
    /// A dry run that invents results is worse than useless -- it reports
    /// problems that do not exist and hides the ones that do -- so reads
    /// happen for real and only changes are skipped. An action whose decisions
    /// depend on a change it just skipped has to handle `dry_run` itself.
    pub fn capture(&self, command: &str) -> Result<String> {
        let outcome = self.spawn_with(command, true, false, &BTreeMap::new())?;
        if !outcome.success {
            return Err(self.failed(command, &outcome));
        }
        Ok(outcome.stdout.trim_end().to_string())
    }

    /// Read something that is allowed to fail (no tags yet, not a repository).
    /// Runs even under `--dry-run`, like [`capture`](Self::capture).
    pub fn probe(&self, command: &str) -> Result<shell::Outcome> {
        self.spawn_with(command, true, false, &BTreeMap::new())
    }

    fn spawn_with(
        &self,
        command: &str,
        quiet: bool,
        skip_on_dry_run: bool,
        extra: &BTreeMap<String, String>,
    ) -> Result<shell::Outcome> {
        if self.dry_run && skip_on_dry_run {
            self.ui.say(&format!("Would run: {command}"));
            return Ok(shell::Outcome {
                code: Some(0),
                success: true,
                timed_out: false,
                interrupted: false,
                stdout: String::new(),
                stderr: String::new(),
            });
        }

        let secrets = self.secrets.borrow().clone();
        let mut env = self.env.clone();
        env.extend(extra.iter().map(|(k, v)| (k.clone(), v.clone())));

        shell::run(Spawn {
            command,
            env: &env,
            workdir: &self.workdir,
            timeout: None,
            quiet,
            secrets: &secrets,
        })
    }

    fn failed(&self, command: &str, outcome: &shell::Outcome) -> ShlaneError {
        let detail = outcome.stderr.trim();
        let detail = if detail.is_empty() {
            String::new()
        } else {
            format!(": {detail}")
        };
        ShlaneError::Action {
            action: self.lane.clone(),
            message: format!(
                "command failed with exit code {}{detail}\n  command: {command}",
                outcome.code.unwrap_or(-1)
            ),
        }
    }

    /// Build an error for an action that could not do its job.
    pub fn error(&self, action: &str, message: impl Into<String>) -> ShlaneError {
        ShlaneError::Action {
            action: action.to_string(),
            message: message.into(),
        }
    }
}
