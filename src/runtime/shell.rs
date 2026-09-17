//! Running shell commands.
//!
//! Output is piped rather than inherited so that it can be masked before it
//! reaches the terminal and handed back to the caller. It is still streamed
//! line by line: a CI job that prints nothing for ten minutes gets killed.

use super::secrets::Secrets;
use super::signals;
use crate::error::{Result, ShlaneError};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const SHELL: &str = "sh";

/// How often a timed step is checked for completion.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// How long a timed-out step gets to shut down before it is killed outright.
const GRACE: Duration = Duration::from_millis(500);

pub struct Outcome {
    pub code: Option<i32>,
    pub success: bool,
    pub timed_out: bool,
    pub interrupted: bool,
    pub stdout: String,
    pub stderr: String,
}

/// Everything needed to run one command.
pub struct Spawn<'a> {
    pub command: &'a str,
    pub env: &'a BTreeMap<String, String>,
    pub workdir: &'a Path,
    pub timeout: Option<Duration>,
    /// Collect the output without echoing it (used by `capture()` in scripts).
    pub quiet: bool,
    pub secrets: &'a Secrets,
}

pub fn run(spawn: Spawn<'_>) -> Result<Outcome> {
    let mut builder = Command::new(SHELL);
    builder
        .arg("-c")
        .arg(spawn.command)
        .current_dir(spawn.workdir)
        .envs(spawn.env)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Every step gets its own process group, so a timeout or a Ctrl-C can stop
    // everything the command started rather than just the shell that started
    // it. Killing `sh` alone leaves the real work -- a `sleep`, a build --
    // running, still holding the pipes shlane is reading, so shlane then waits
    // for the very process it thought it had stopped.
    //
    // The cost is that the terminal no longer delivers Ctrl-C to the step
    // directly; shlane's own handler forwards it (see `signals`).
    #[cfg(unix)]
    let own_group = true;
    #[cfg(not(unix))]
    let own_group = false;

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        builder.process_group(0);
    }

    let mut child = builder
        .spawn()
        .map_err(|source| ShlaneError::ShellUnavailable {
            shell: SHELL.to_string(),
            source,
        })?;

    signals::register_child(child.id() as i32, own_group);

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let out_reader = reader(stdout, spawn.secrets.clone(), spawn.quiet, false);
    let err_reader = reader(stderr, spawn.secrets.clone(), spawn.quiet, true);

    let timed_out = match spawn.timeout {
        None => {
            child
                .wait()
                .map_err(|source| ShlaneError::ShellUnavailable {
                    shell: SHELL.to_string(),
                    source,
                })?;
            false
        }
        Some(timeout) => wait_with_timeout(&mut child, timeout)?,
    };

    let status = child
        .wait()
        .map_err(|source| ShlaneError::ShellUnavailable {
            shell: SHELL.to_string(),
            source,
        })?;

    signals::clear_child();

    let stdout = out_reader.join().unwrap_or_default();
    let stderr = err_reader.join().unwrap_or_default();

    Ok(Outcome {
        code: status.code(),
        success: status.success() && !timed_out,
        timed_out,
        interrupted: signals::interrupted(),
        stdout,
        stderr,
    })
}

/// Read a child stream line by line, masking secrets before anything is shown.
fn reader(
    stream: Option<impl Read + Send + 'static>,
    secrets: Secrets,
    quiet: bool,
    is_stderr: bool,
) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let Some(stream) = stream else {
            return String::new();
        };
        let mut collected = String::new();
        for line in BufReader::new(stream).lines() {
            let Ok(line) = line else { break };
            let line = secrets.mask(&line);
            collected.push_str(&line);
            collected.push('\n');
            if quiet {
                continue;
            }
            if is_stderr {
                let mut handle = std::io::stderr().lock();
                let _ = writeln!(handle, "{line}");
            } else {
                let mut handle = std::io::stdout().lock();
                let _ = writeln!(handle, "{line}");
            }
        }
        collected
    })
}

/// Returns true when the deadline was reached and the child was stopped.
fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<bool> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(false),
            Ok(None) => {}
            Err(source) => {
                return Err(ShlaneError::ShellUnavailable {
                    shell: SHELL.to_string(),
                    source,
                })
            }
        }

        if signals::interrupted() {
            stop(child);
            return Ok(false);
        }

        if Instant::now() >= deadline {
            stop(child);
            return Ok(true);
        }

        thread::sleep(POLL_INTERVAL);
    }
}

/// Stop a step: ask its process group to quit, then insist.
fn stop(child: &mut Child) {
    #[cfg(unix)]
    {
        let group = child.id() as i32;
        // Safety: killpg only signals; an already-dead group returns ESRCH.
        unsafe {
            libc::killpg(group, libc::SIGTERM);
        }

        let deadline = Instant::now() + GRACE;
        while Instant::now() < deadline {
            if matches!(child.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(POLL_INTERVAL);
        }

        unsafe {
            libc::killpg(group, libc::SIGKILL);
        }
    }

    let _ = child.kill();
}
