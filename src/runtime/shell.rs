//! Running shell commands.
//!
//! Unlike v0.1.0 this never calls `process::exit`: the outcome goes back to
//! the executor, which decides what to do and still gets to run cleanup.

use crate::error::{Result, ShlaneError};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};

const SHELL: &str = "sh";

pub struct Outcome {
    pub code: Option<i32>,
    pub success: bool,
}

pub fn run(command: &str, env: &BTreeMap<String, String>, workdir: &Path) -> Result<Outcome> {
    let status = Command::new(SHELL)
        .arg("-c")
        .arg(command)
        .current_dir(workdir)
        .envs(env)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|source| ShlaneError::ShellUnavailable {
            shell: SHELL.to_string(),
            source,
        })?;

    Ok(Outcome {
        code: status.code(),
        success: status.success(),
    })
}
