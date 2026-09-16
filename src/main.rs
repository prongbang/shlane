//! shlane — a fastlane-like automation tool.
//!
//! `main` stays thin on purpose: parse arguments, dispatch, turn an error into
//! a message and an exit code. See `docs/plan/02-architecture.md`.

#![cfg_attr(not(test), warn(clippy::unwrap_used, clippy::expect_used))]

mod cli;
mod config;
mod error;
mod runtime;
mod script;

use clap::Parser;
use std::process::ExitCode;

fn main() -> ExitCode {
    match cli::dispatch(cli::Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::from(err.exit_code() as u8)
        }
    }
}
