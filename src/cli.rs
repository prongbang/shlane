//! Command line surface.

use crate::config;
use crate::error::Result;
use crate::runtime;
use clap::{Parser, Subcommand};
use std::env;

#[derive(Parser)]
#[command(name = "shlane")]
#[command(version)]
#[command(about = "A fastlane-like tool written in Rust", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a lane defined in the config file
    Run {
        #[arg(help = "Name of the lane to execute")]
        name: String,

        #[arg(help = "Key-value parameters", trailing_var_arg = true)]
        params: Vec<String>,
    },
}

pub fn dispatch(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Run { name, params } => {
            let workdir = env::current_dir().unwrap_or_else(|_| ".".into());
            let config = config::load_from_dir(&workdir)?;
            let params = runtime::parse_params(params);
            runtime::run_lane(&config, &name, params, &workdir)
        }
    }
}
