//! Command line surface.

mod actions;
mod init;
mod list;
mod migrate;
mod plugins;
mod show_env;

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
enum PluginCommands {
    /// Show every plugin, its actions and its checksum
    List,
    /// Record each plugin's checksum in shlane-plugins.lock
    Lock,
    /// Ask each plugin to describe itself and compare with its manifest
    Verify,
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

        /// Write results to a file, e.g. junit:reports/shlane.xml (repeatable)
        #[arg(long, value_name = "FORMAT:PATH")]
        report: Vec<String>,
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

    /// Show the environment a lane would run with
    Env {
        /// Include everything inherited from the process, not just this config
        #[arg(long)]
        all: bool,
    },

    /// Convert a Fastfile into a shlane.yaml
    Migrate {
        /// The Fastfile to read; found automatically by default
        #[arg(long, value_name = "PATH")]
        fastfile: Option<PathBuf>,

        /// Where to write the result
        #[arg(long, value_name = "PATH")]
        out: Option<PathBuf>,

        /// Overwrite an existing file
        #[arg(long)]
        force: bool,
    },

    /// Inspect the plugins this config loads
    Plugin {
        #[command(subcommand)]
        command: PluginCommands,
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
            report,
        } => {
            let reports = report
                .iter()
                .map(|spec| crate::report::parse(spec))
                .collect::<std::result::Result<Vec<_>, String>>()
                .map_err(|message| ShlaneError::ConfigProblems {
                    path: PathBuf::from("--report"),
                    problems: vec![message],
                })?;

            let found = load(file.as_deref(), &base)?;
            let params = runtime::parse_params(params);
            runtime::run_lane(
                &found.config,
                &found.root,
                &name,
                params,
                runtime::Options {
                    dry_run,
                    reports,
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
            let registry = registry_for(&found)?;
            let problems = validate::check(&found.config, &registry);
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
        Commands::Action { command } => {
            // Plugins only load when there is a config; `shlane action list`
            // still has to work outside a project.
            let registry = match load(file.as_deref(), &base) {
                Ok(found) => registry_for(&found)?,
                Err(_) => crate::actions::Registry::builtins(),
            };
            match command {
                ActionCommands::List => {
                    actions::list(&registry);
                    Ok(())
                }
                ActionCommands::Show { name } => actions::show(&registry, &name),
            }
        }
        Commands::Plugin { command } => {
            let found = load(file.as_deref(), &base)?;
            match command {
                PluginCommands::List => plugins::list(&found),
                PluginCommands::Lock => plugins::lock(&found),
                PluginCommands::Verify => plugins::verify(&found),
            }
        }
        Commands::Env { all } => {
            let found = load(file.as_deref(), &base)?;
            show_env::show(&found, profile.as_deref(), all)
        }
        Commands::Migrate {
            fastfile,
            out,
            force,
        } => migrate::run(&base, fastfile.as_deref(), out.as_deref(), force),
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

/// Built-in actions plus whatever the config's `plugins:` bring in.
fn registry_for(found: &Discovered) -> Result<crate::actions::Registry> {
    let loaded = crate::plugin::load_all(&found.config, &found.root)?;
    Ok(crate::actions::Registry::builtins().with_plugins(crate::plugin::actions(loaded)))
}

fn load(file: Option<&std::path::Path>, base: &std::path::Path) -> Result<Discovered> {
    match file {
        Some(path) => loader::open(path),
        None => loader::discover(base),
    }
}
