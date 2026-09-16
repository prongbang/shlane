//! Lane execution.
//!
//! The order is the one v0.1.0 established — before, steps, script, after.
//! Folding all of these into a single ordered `steps:` list is an M1 change
//! (`docs/plan/03-config-schema.md`).

use super::context::Context;
use super::interpolate::interpolate;
use super::shell;
use crate::config::model::{Config, Lane};
use crate::error::{Result, ShlaneError};
use crate::script;
use std::collections::BTreeMap;
use std::path::Path;

pub fn run_lane(
    config: &Config,
    lane_name: &str,
    params: BTreeMap<String, String>,
    workdir: &Path,
) -> Result<()> {
    let lane = config
        .lanes
        .get(lane_name)
        .ok_or_else(|| ShlaneError::LaneNotFound {
            name: lane_name.to_string(),
            available: config.lane_names(),
        })?;

    let ctx = Context::new(lane_name, params, config.env.clone(), workdir);
    let engine = script::engine::build(&ctx);
    let mut scope = rhai::Scope::new();

    if let Some(shared) = &config.script {
        println!("Loading shared script...");
        script::eval(&engine, &mut scope, shared).map_err(|message| ShlaneError::Script {
            lane: ctx.lane.clone(),
            phase: "shared",
            message,
        })?;
    }

    run_commands(&ctx, "before hook", lane.before.as_deref())?;
    run_steps(&ctx, lane)?;

    if let Some(lane_script) = &lane.script {
        println!("Running Rhai script for lane '{}':", ctx.lane);
        script::eval(&engine, &mut scope, lane_script).map_err(|message| ShlaneError::Script {
            lane: ctx.lane.clone(),
            phase: "lane",
            message,
        })?;
    }

    run_commands(&ctx, "after hook", lane.after.as_deref())?;

    println!("Lane '{}' completed successfully!", ctx.lane);
    Ok(())
}

fn run_steps(ctx: &Context, lane: &Lane) -> Result<()> {
    let Some(steps) = lane.steps.as_deref() else {
        return Ok(());
    };
    if steps.is_empty() {
        return Ok(());
    }
    println!("Running steps...");
    for (index, step) in steps.iter().enumerate() {
        execute(ctx, "step", index + 1, &step.run)?;
    }
    Ok(())
}

fn run_commands(ctx: &Context, phase: &'static str, commands: Option<&[String]>) -> Result<()> {
    let Some(commands) = commands else {
        return Ok(());
    };
    if commands.is_empty() {
        return Ok(());
    }
    println!("Running {phase}s...");
    for (index, command) in commands.iter().enumerate() {
        execute(ctx, phase, index + 1, command)?;
    }
    Ok(())
}

fn execute(ctx: &Context, phase: &'static str, index: usize, raw: &str) -> Result<()> {
    let command = interpolate(raw, &ctx.vars())?;
    println!("Running: {command}");

    let outcome = shell::run(&command, &ctx.env, &ctx.workdir)?;
    if outcome.success {
        return Ok(());
    }

    Err(ShlaneError::StepFailed {
        lane: ctx.lane.clone(),
        phase,
        index,
        command,
        code: outcome.code,
    })
}
