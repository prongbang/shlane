//! shlane — a fastlane-like automation tool.
//!
//! `main` stays thin on purpose: parse arguments, dispatch, turn an error into
//! a message and an exit code. See `docs/plan/02-architecture.md`.

#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]

mod actions;
mod cli;
mod config;
mod error;
mod migrate;
mod plugin;
mod report;
mod runtime;
mod script;

use clap::Parser;
use std::process::ExitCode;

/// Die quietly when a pipe closes, the way every other command-line tool does.
///
/// Rust ignores `SIGPIPE` so that writes return `EPIPE`, but `println!` turns
/// that into a panic -- so `shlane list | head` printed a backtrace.
#[cfg(unix)]
fn restore_sigpipe() {
    // Safety: resetting a signal to its default disposition, before any threads
    // exist.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
}

#[cfg(not(unix))]
fn restore_sigpipe() {}

fn main() -> ExitCode {
    restore_sigpipe();

    match cli::dispatch(cli::Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(err.exit_code() as u8)
        }
    }
}
