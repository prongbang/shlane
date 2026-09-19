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

/// The shell on a unix machine.
const POSIX_SHELL: &str = "sh";

/// Where Git for Windows puts the POSIX shell it ships.
#[cfg(windows)]
const GIT_BASH: &[&str] = &[
    r"C:\Program Files\Git\bin\bash.exe",
    r"C:\Program Files (x86)\Git\bin\bash.exe",
    r"C:\Program Files\Git\usr\bin\sh.exe",
];

/// How often a timed step is checked for completion.
const POLL_INTERVAL: Duration = Duration::from_millis(25);

/// How long a timed-out step gets to shut down before it is killed outright.
/// Only used where there is a process group to signal.
#[cfg(unix)]
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

/// Which program runs a step's command.
///
/// POSIX everywhere, Windows included. Every value substituted into a `run:`
/// is escaped by POSIX rules (`runtime::interpolate`), and handing those to
/// `cmd.exe`, which quotes differently, would turn careful escaping back into
/// the command injection it exists to prevent. Windows users have a POSIX
/// shell already: it comes with Git for Windows.
///
/// `SHLANE_SHELL` overrides it, from the config's `env:` or the process.
pub fn shell(env: &BTreeMap<String, String>) -> Result<String> {
    if let Some(configured) = env
        .get("SHLANE_SHELL")
        .cloned()
        .or_else(|| std::env::var("SHLANE_SHELL").ok())
        .filter(|value| !value.is_empty())
    {
        return Ok(configured);
    }

    #[cfg(not(windows))]
    {
        Ok(POSIX_SHELL.to_string())
    }

    #[cfg(windows)]
    {
        if let Some(found) = which("bash.exe").or_else(|| which("sh.exe")) {
            return Ok(found.display().to_string());
        }
        for candidate in GIT_BASH {
            if Path::new(candidate).is_file() {
                return Ok((*candidate).to_string());
            }
        }
        Err(ShlaneError::ShellUnavailable {
            shell: "bash".to_string(),
            source: std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "shlane runs steps in a POSIX shell, and there is none on PATH. Install Git for Windows, which ships one, or point SHLANE_SHELL at the shell you want.",
            ),
        })
    }
}

/// The first `program` on PATH, as a full path, skipping the WSL launcher.
///
/// The full path matters: handed a bare `bash.exe`, Windows resolves it with
/// its own search order, which puts the system directory ahead of PATH. That
/// directory holds `bash.exe`, the launcher for the Windows Subsystem for
/// Linux, so a machine with no WSL distribution installed ran every step
/// through something that exits 1 with "has no installed distributions"
/// instead of through the shell that was actually found.
#[cfg(windows)]
fn which(program: &str) -> Option<std::path::PathBuf> {
    let path = std::env::var_os("PATH")?;
    let system_root = std::env::var_os("SystemRoot").map(std::path::PathBuf::from);

    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| {
            candidate.is_file() && !is_wsl_launcher(candidate, system_root.as_deref())
        })
}

/// Whether this is the WSL launcher rather than a POSIX shell.
///
/// It lives in the Windows system directory, which is the one place a real
/// POSIX shell never does.
///
/// Compiled for the tests everywhere, because the rule is about the shape of a
/// path and is worth checking on the machines this is written on, but only
/// called on Windows.
#[cfg(any(windows, test))]
fn is_wsl_launcher(candidate: &Path, system_root: Option<&Path>) -> bool {
    let Some(system_root) = system_root else {
        return false;
    };

    // Compared as normalised strings rather than with `Path::parent`, so the
    // rule holds anywhere: off Windows a backslash is an ordinary character
    // and `parent()` would see one long file name.
    let normalise = |path: &Path| {
        path.to_string_lossy()
            .to_ascii_lowercase()
            .replace('\\', "/")
    };
    let candidate = normalise(candidate);
    let root = normalise(system_root).trim_end_matches('/').to_string();

    ["system32", "sysnative"].iter().any(|directory| {
        candidate
            .strip_prefix(&format!("{root}/{directory}/"))
            .is_some_and(|rest| !rest.contains('/'))
    })
}

pub fn run(spawn: Spawn<'_>) -> Result<Outcome> {
    let shell = shell(spawn.env)?;
    let mut builder = Command::new(&shell);
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
            shell: shell.clone(),
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
                    shell: shell.clone(),
                    source,
                })?;
            false
        }
        Some(timeout) => wait_with_timeout(&mut child, timeout)?,
    };

    let status = child
        .wait()
        .map_err(|source| ShlaneError::ShellUnavailable {
            shell: shell.clone(),
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
                    shell: POSIX_SHELL.to_string(),
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
    #[cfg(windows)]
    {
        // Windows has no process group to signal, and killing the shell alone
        // is not enough: what it started inherits the pipes shlane is reading,
        // so the joins below wait for the very step that was just stopped. A
        // `timeout: 2s` on `sleep 10` then reported the timeout after ten
        // seconds. taskkill takes the whole tree; it ships with Windows, and a
        // tree that has already gone just makes it exit non-zero.
        let _ = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }

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

#[cfg(test)]
mod tests {
    use super::*;

    /// These run everywhere: the rule is about a path, not about the platform,
    /// and a Windows-only test would leave it uncovered on the machines where
    /// the code is usually written.
    #[test]
    fn the_wsl_launcher_is_recognised_wherever_windows_keeps_it() {
        let system_root = Path::new(r"C:\Windows");
        for launcher in [
            r"C:\Windows\System32\bash.exe",
            r"C:\Windows\system32\bash.exe",
            r"C:\WINDOWS\System32\bash.exe",
            r"C:\Windows\Sysnative\bash.exe",
        ] {
            assert!(
                is_wsl_launcher(Path::new(launcher), Some(system_root)),
                "{launcher} is the WSL launcher, not a POSIX shell"
            );
        }
    }

    #[test]
    fn a_real_shell_is_not_mistaken_for_the_launcher() {
        let system_root = Path::new(r"C:\Windows");
        for shell in [
            r"C:\Program Files\Git\bin\bash.exe",
            r"C:\Program Files\Git\usr\bin\sh.exe",
            r"C:\tools\msys64\usr\bin\bash.exe",
            // Nested deeper than the system directory itself.
            r"C:\Windows\System32\config\bash.exe",
            "/bin/sh",
        ] {
            assert!(
                !is_wsl_launcher(Path::new(shell), Some(system_root)),
                "{shell} is a real shell"
            );
        }
    }

    #[test]
    fn without_a_system_root_nothing_is_excluded() {
        // Better to run a shell that might be the launcher than to refuse to
        // find any shell at all.
        assert!(!is_wsl_launcher(
            Path::new(r"C:\Windows\System32\bash.exe"),
            None
        ));
    }
}
