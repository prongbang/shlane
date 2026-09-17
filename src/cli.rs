//! Command line surface.

mod actions;
mod init;
mod list;

use crate::config::loader::{self, Discovered};
use crate::config::validate;
use crate::error::{Result, ShlaneError};
use crate::runtime;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use std::env;
use std::io;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "shlane")]
#[command(version)]
#[command(about = "A fastlane-like tool written in Rust", long_about = None)]
pub struct Cli {
    /// Use this config file instead of searching for one
    #[arg(short = 'f', long, global = true, value_name = "PATH")]
    file: Option<PathBuf>,

    /// Work from this directory
    #[arg(short = 'C', long, global = true, value_name = "DIR")]
    cwd: Option<PathBuf>,

    /// Show more detail
    #[arg(short = 'v', long, global = true, conflicts_with = "quiet")]
    verbose: bool,

    /// Only report errors
    #[arg(short = 'q', long, global = true)]
    quiet: bool,

    /// Emit one JSON event per line instead of human-readable output
    #[arg(long, global = true)]
    json: bool,

    /// Select `.env.<profile>`
    #[arg(long, global = true, value_name = "PROFILE")]
    env: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum ActionCommands {
    /// List every action
    List,
    /// Show one action's arguments
    Show {
        #[arg(help = "Action name")]
        name: String,
    },
}

#[derive(Subcommand)]
enum Commands {
    /// Run a lane defined in the config file
    Run {
        #[arg(help = "Name of the lane to execute")]
        name: String,

        /// Key-value parameters, e.g. target=production
        ///
        /// Not a trailing_var_arg: that swallowed flags written after the lane
        /// name, so `shlane run beta target=x --dry-run` silently ignored the
        /// flag. Parameters are always `key=value`, so they never look like one.
        #[arg(value_name = "KEY=VALUE")]
        params: Vec<String>,

        /// Print what would run without running it
        #[arg(long)]
        dry_run: bool,
    },

    /// List the lanes in the config file
    #[command(alias = "lanes")]
    List,

    /// Check the config file without running anything
    Validate,

    /// Write a starter config file for this project
    Init {
        /// Overwrite an existing config file
        #[arg(long)]
        force: bool,
    },

    /// Show the built-in actions
    Action {
        #[command(subcommand)]
        command: ActionCommands,
    },

    /// Print a shell completion script
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
}

pub fn dispatch(cli: Cli) -> Result<()> {
    let base = match &cli.cwd {
        Some(dir) => dir.clone(),
        None => env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    let file = cli.file.clone();
    let verbosity = if cli.quiet {
        runtime::Verbosity::Quiet
    } else if cli.verbose {
        runtime::Verbosity::Verbose
    } else {
        runtime::Verbosity::Normal
    };
    let profile = cli.env.clone();
    let json = cli.json;

    match cli.command {
        Commands::Run {
            name,
            params,
            dry_run,
        } => {
            let found = load(file.as_deref(), &base)?;
            let params = runtime::parse_params(params);
            runtime::run_lane(
                &found.config,
                &found.root,
                &name,
                params,
                runtime::Options {
                    dry_run,
                    verbosity,
                    json,
                    profile,
                },
            )
        }
        Commands::List => {
            let found = load(file.as_deref(), &base)?;
            list::print(&found.config, &found.path);
            Ok(())
        }
        Commands::Validate => {
            let found = load(file.as_deref(), &base)?;
            let problems = validate::check(&found.config);
            if problems.is_empty() {
                let lanes = found.config.lanes.len();
                println!("{} is valid ({lanes} lane(s))", found.path.display());
                return Ok(());
            }
            Err(ShlaneError::ConfigProblems {
                path: found.path,
                problems,
            })
        }
        Commands::Action { command } => match command {
            ActionCommands::List => {
                actions::list();
                Ok(())
            }
            ActionCommands::Show { name } => actions::show(&name),
        },
        Commands::Init { force } => init::write(&base, force),
        Commands::Completions { shell } => {
            clap_complete::generate(
                shell,
                &mut Cli::command(),
                "shlane",
                &mut io::stdout().lock(),
            );
            Ok(())
        }
    }
}

fn load(file: Option<&std::path::Path>, base: &std::path::Path) -> Result<Discovered> {
    match file {
        Some(path) => loader::open(path),
        None => loader::discover(base),
    }
}
