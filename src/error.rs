//! Error types and the exit-code contract.
//!
//! Exit codes follow `docs/plan/04-cli-ux.md`.

use std::fmt;
use std::io;
use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, ShlaneError>;

pub mod exit_code {
    /// A lane ran but something in it failed.
    pub const LANE_FAILED: i32 = 1;
    /// The config could not be parsed, or a value in it could not be resolved.
    pub const CONFIG_INVALID: i32 = 2;
    /// The config file or the requested lane does not exist.
    pub const NOT_FOUND: i32 = 3;
    /// A tool shlane needs is not installed.
    pub const TOOL_MISSING: i32 = 5;
}

#[derive(Debug)]
pub enum ShlaneError {
    ConfigNotFound {
        path: PathBuf,
    },
    ConfigUnreadable {
        path: PathBuf,
        source: io::Error,
    },
    ConfigInvalid {
        path: PathBuf,
        location: Option<(usize, usize)>,
        message: String,
    },
    LaneNotFound {
        name: String,
        available: Vec<String>,
    },
    UndefinedVariable {
        name: String,
        source_text: String,
    },
    UnterminatedVariable {
        source_text: String,
    },
    StepFailed {
        lane: String,
        phase: &'static str,
        index: usize,
        command: String,
        code: Option<i32>,
    },
    ShellUnavailable {
        shell: String,
        source: io::Error,
    },
    Script {
        lane: String,
        phase: &'static str,
        message: String,
    },
}

impl ShlaneError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::ConfigNotFound { .. }
            | Self::ConfigUnreadable { .. }
            | Self::LaneNotFound { .. } => exit_code::NOT_FOUND,
            Self::ConfigInvalid { .. }
            | Self::UndefinedVariable { .. }
            | Self::UnterminatedVariable { .. } => exit_code::CONFIG_INVALID,
            Self::ShellUnavailable { .. } => exit_code::TOOL_MISSING,
            Self::StepFailed { .. } | Self::Script { .. } => exit_code::LANE_FAILED,
        }
    }
}

impl fmt::Display for ShlaneError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigNotFound { path } => write!(
                f,
                "no config file found at {}\n  hint: create a shlane.yaml, or run shlane from the directory that contains it",
                path.display()
            ),
            Self::ConfigUnreadable { path, source } => {
                write!(f, "cannot read {}: {source}", path.display())
            }
            Self::ConfigInvalid {
                path,
                location,
                message,
            } => match location {
                Some((line, column)) => write!(
                    f,
                    "invalid config at {}:{line}:{column}: {message}",
                    path.display()
                ),
                None => write!(f, "invalid config at {}: {message}", path.display()),
            },
            Self::LaneNotFound { name, available } => {
                write!(f, "lane '{name}' not found")?;
                if available.is_empty() {
                    write!(f, "\n  no lanes are defined in this config")
                } else {
                    write!(f, "\n  available lanes:")?;
                    for lane in available {
                        write!(f, "\n    - {lane}")?;
                    }
                    Ok(())
                }
            }
            Self::UndefinedVariable { name, source_text } => write!(
                f,
                "undefined variable '${{{name}}}' in: {source_text}\n  hint: pass it on the command line (shlane run <lane> {name}=<value>) or define it under env:",
            ),
            Self::UnterminatedVariable { source_text } => write!(
                f,
                "unterminated '${{' in: {source_text}\n  hint: write '$${{' if you meant a literal dollar-brace",
            ),
            Self::StepFailed {
                lane,
                phase,
                index,
                command,
                code,
            } => {
                write!(f, "lane '{lane}': {phase} #{index} failed")?;
                match code {
                    Some(code) => write!(f, " with exit code {code}")?,
                    None => write!(f, " (terminated by a signal)")?,
                }
                write!(f, "\n  command: {command}")
            }
            Self::ShellUnavailable { shell, source } => write!(
                f,
                "cannot execute commands: failed to start '{shell}': {source}"
            ),
            Self::Script {
                lane,
                phase,
                message,
            } => write!(f, "lane '{lane}': {phase} script failed: {message}"),
        }
    }
}

impl std::error::Error for ShlaneError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ConfigUnreadable { source, .. } | Self::ShellUnavailable { source, .. } => {
                Some(source)
            }
            _ => None,
        }
    }
}
