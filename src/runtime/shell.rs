//! Running shell commands.
//!
//! Unlike v0.1.0 this never calls `process::exit`: the outcome goes back to
//! the executor, which decides what to do and still gets to run cleanup.

use crate::error::{Result, ShlaneError};
use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Command, Stdio};
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
}

pub fn run(
    command: &str,
    env: &BTreeMap<String, String>,
    workdir: &Path,
    timeout: Option<Duration>,
) -> Result<Outcome> {
    let mut builder = Command::new(SHELL);
    builder
        .arg("-c")
        .arg(command)
        .current_dir(workdir)
        .envs(env)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    // A timed step gets its own process group, so the timeout can stop
    // everything the command started rather than just the shell that started
    // it -- killing `sh` alone leaves the real work (a `sleep`, a build) running
    // and holding the terminal.
    //
    // The cost is that Ctrl-C, which the terminal delivers to the foreground
    // group, no longer reaches a timed step. Handling that properly needs a
    // signal handler (docs/plan/04-cli-ux.md), so untimed steps -- the common
    // case -- are deliberately left in shlane's own group.
    #[cfg(unix)]
    if timeout.is_some() {
        use std::os::unix::process::CommandExt as _;
        builder.process_group(0);
    }

    let mut child = builder
        .spawn()
        .map_err(|source| ShlaneError::ShellUnavailable {
            shell: SHELL.to_string(),
            source,
        })?;

    let Some(timeout) = timeout else {
        let status = child
            .wait()
            .map_err(|source| ShlaneError::ShellUnavailable {
                shell: SHELL.to_string(),
                source,
            })?;
        return Ok(Outcome {
            code: status.code(),
            success: status.success(),
            timed_out: false,
        });
    };

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(Outcome {
                    code: status.code(),
                    success: status.success(),
                    timed_out: false,
                })
            }
            Ok(None) => {}
            Err(source) => {
                return Err(ShlaneError::ShellUnavailable {
                    shell: SHELL.to_string(),
                    source,
                })
            }
        }

        if Instant::now() >= deadline {
            stop(&mut child);
            return Ok(Outcome {
                code: None,
                success: false,
                timed_out: true,
            });
        }

        thread::sleep(POLL_INTERVAL);
    }
}

/// Stop a timed-out step: ask its process group to quit, then insist.
fn stop(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        let group = child.id() as i32;
        // Safety: `killpg` only signals; an already-dead group returns ESRCH,
        // which is why the result is ignored.
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
    let _ = child.wait();
}
