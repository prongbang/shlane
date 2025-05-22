use clap::{Parser, Subcommand};
use rhai::{Engine, Scope};
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::process::{Command, Stdio};

#[derive(Parser)]
#[command(name = "shlane")]
#[command(about = "A fastlane-like tool written in Rust", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a lane defined in config file
    Run {
        #[arg(help = "Name of the lane to execute")]
        name: String,

        #[arg(help = "Key-value parameters", trailing_var_arg = true)]
        params: Vec<String>,
    },
}

#[derive(Debug, Deserialize)]
struct Config {
    env: Option<HashMap<String, String>>,
    script: Option<String>,
    lanes: HashMap<String, Lane>,
}

#[derive(Debug, Deserialize)]
struct Lane {
    before: Option<Vec<String>>,
    steps: Option<Vec<Step>>,
    after: Option<Vec<String>>,
    script: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Step {
    run: String,
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Run { name, params } => {
            let mut args = HashMap::new();
            for param in params {
                if let Some((k, v)) = param.split_once('=') {
                    args.insert(k.to_string(), v.to_string());
                }
            }
            run_lane(&name, args);
        }
    }
}

fn interpolate(input: &str, params: &HashMap<String, String>) -> String {
    let mut result = input.to_string();
    for (k, v) in params {
        let pattern = format!("${{{}}}", k);
        result = result.replace(&pattern, v);
    }
    result
}

fn run_lane(name: &str, params: HashMap<String, String>) {
    let config_str = fs::read_to_string("shlane.yaml").expect("Failed to read shlane.yaml");
    let config: Config = serde_yaml::from_str(&config_str).expect("Invalid YAML format");

    // Set environment variables from config
    if let Some(envs) = config.env {
        for (key, value) in envs {
            env::set_var(key, value);
        }
    }

    if let Some(lane) = config.lanes.get(name) {
        // New engine and scope for shared script
        let mut engine = Engine::new();
        let mut scope = Scope::new();

        // Registration builtin
        register_builtin_functions(&mut engine, &params);

        // Load shared script if needed
        if let Some(shared_script) = &config.script {
            println!("Loading shared script...");
            if let Err(e) = engine.eval_with_scope::<()>(&mut scope, shared_script) {
                eprintln!("Error in shared script: {}", e);
                return;
            }
        }

        // Run before hooks
        if let Some(before_hooks) = &lane.before {
            println!("Running before hooks...");
            for hook in before_hooks {
                run_shell_command(&interpolate(hook, &params));
            }
        }

        // Run steps
        if let Some(steps) = &lane.steps {
            println!("Running steps...");
            for step in steps {
                let interpolated = interpolate(&step.run, &params);
                println!("Running: {}", interpolated);
                run_shell_command(&interpolated);
            }
        }

        // Run lane script
        if let Some(lane_script) = &lane.script {
            println!("Running Rhai script for lane '{}':", name);

            // Use engine and scope
            if let Err(err) = engine.eval_with_scope::<()>(&mut scope, lane_script) {
                eprintln!("Rhai script error: {}", err);
            }
        }

        // Run after hooks
        if let Some(after_hooks) = &lane.after {
            println!("Running after hooks...");
            for hook in after_hooks {
                run_shell_command(&interpolate(hook, &params));
            }
        }

        println!("Lane '{}' completed successfully!", name);
    } else {
        eprintln!("Lane '{}' not found in shlane.yaml", name);

        // List lanes existing่
        if !config.lanes.is_empty() {
            eprintln!("Available lanes:");
            for lane_name in config.lanes.keys() {
                eprintln!("  - {}", lane_name);
            }
        }
    }
}

fn run_shell_command(command: &str) {
    let status = Command::new("sh")
        .arg("-c")
        .arg(command)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .expect("Failed to execute command");

    if !status.success() {
        eprintln!("Command failed with status: {}", status);
        std::process::exit(1);
    }
}

fn register_builtin_functions(engine: &mut Engine, params: &HashMap<String, String>) {
    // Register param() - for reference to parameters
    let param_map = params.clone();
    engine.register_fn("param", move |key: &str| -> String {
        param_map.get(key).cloned().unwrap_or_default()
    });

    // Register env() - for reference to environment variables
    engine.register_fn("env", |key: &str| -> String {
        env::var(key).unwrap_or_default()
    });

    // Register run() - for run shell commands from Rhai script
    engine.register_fn("run", |cmd: &str| -> i32 {
        println!("Executing: {}", cmd);
        let status = Command::new("sh")
            .arg("-c")
            .arg(cmd)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status();

        match status {
            Ok(status) => status.code().unwrap_or(-1),
            Err(_) => -1,
        }
    });

    // Register print() - for debug/logging
    engine.register_fn("print", |msg: &str| {
        println!("{}", msg);
    });
}
