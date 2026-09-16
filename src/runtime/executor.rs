//! Lane execution.

use super::context::{Frame, SharedFrame};
use super::interpolate::{interpolate, interpolate_plain, Vars};
use super::shell;
use crate::config::model::{Config, Lane, ParamSpec, Step, StepKind};
use crate::config::validate;
use crate::error::{Result, ShlaneError};
use crate::script;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How deeply `lane:` steps may nest before shlane gives up.
const MAX_DEPTH: usize = 16;

#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    pub dry_run: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Ok,
    Skipped,
    Failed,
}

impl Status {
    fn symbol(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Skipped => "skipped",
            Self::Failed => "FAILED",
        }
    }
}

struct Record {
    lane: String,
    label: String,
    status: Status,
    duration: Duration,
}

pub struct Runner<'a> {
    config: &'a Config,
    root: PathBuf,
    options: Options,
    frame: SharedFrame,
    engine: rhai::Engine,
    scope: rhai::Scope<'static>,
    records: Vec<Record>,
    depth: usize,
}

/// Run a lane and print a summary of what happened.
pub fn run_lane(
    config: &Config,
    root: &Path,
    lane_name: &str,
    params: BTreeMap<String, String>,
    options: Options,
) -> Result<()> {
    let problems = validate::check(config);
    if !problems.is_empty() {
        return Err(ShlaneError::ConfigProblems {
            path: root.join("shlane.yaml"),
            problems,
        });
    }

    let lane = config
        .lanes
        .get(lane_name)
        .ok_or_else(|| ShlaneError::LaneNotFound {
            name: lane_name.to_string(),
            available: config.public_lane_names(),
        })?;

    if lane.private {
        return Err(ShlaneError::LanePrivate {
            name: lane_name.to_string(),
            available: config.public_lane_names(),
        });
    }

    let mut runner = Runner::new(config, root, options);
    let outcome = runner.run(lane_name, params);
    runner.print_summary();

    if outcome.is_ok() {
        println!("\nLane '{lane_name}' completed successfully!");
    }
    outcome
}

impl<'a> Runner<'a> {
    fn new(config: &'a Config, root: &Path, options: Options) -> Self {
        let frame: SharedFrame = Rc::new(RefCell::new(Frame {
            lane: String::new(),
            params: BTreeMap::new(),
            env: config.env.clone(),
            workdir: root.to_path_buf(),
            dry_run: options.dry_run,
        }));
        let engine = script::engine::build(&frame);
        Self {
            config,
            root: root.to_path_buf(),
            options,
            frame,
            engine,
            scope: rhai::Scope::new(),
            records: Vec::new(),
            depth: 0,
        }
    }

    fn run(&mut self, lane_name: &str, params: BTreeMap<String, String>) -> Result<()> {
        if let Some(shared) = &self.config.script {
            println!("Loading shared script...");
            let source = shared.clone();
            script::load_shared(&mut self.engine, &mut self.scope, &source).map_err(|message| {
                ShlaneError::Script {
                    lane: lane_name.to_string(),
                    phase: "shared",
                    message,
                }
            })?;
        }

        // The lane's frame is entered before `before_all` so the global hooks
        // can see ${shlane.lane} and the lane's parameters.
        let lane = match self.config.lanes.get(lane_name) {
            Some(lane) => lane,
            None => {
                return Err(ShlaneError::LaneNotFound {
                    name: lane_name.to_string(),
                    available: self.config.public_lane_names(),
                })
            }
        };
        let params = resolve_params(lane_name, lane, params)?;
        let previous = self.enter(lane_name, params);

        let result = self
            .run_steps("before_all", lane_name, &self.config.before_all)
            .and_then(|()| self.run_lane_body(lane_name, lane))
            .and_then(|()| self.run_steps("after_all", lane_name, &self.config.after_all));

        if result.is_err() && !self.config.error.is_empty() {
            println!("\nRunning error hooks...");
            // A failing error hook must not replace the failure that caused it.
            if let Err(err) = self.run_steps("error", lane_name, &self.config.error) {
                eprintln!("warning: an error hook itself failed: {err}");
            }
        }

        *self.frame.borrow_mut() = previous;
        result
    }

    fn run_lane_inner(&mut self, lane_name: &str, params: BTreeMap<String, String>) -> Result<()> {
        if self.depth >= MAX_DEPTH {
            return Err(ShlaneError::Script {
                lane: lane_name.to_string(),
                phase: "lane",
                message: format!("lanes nested more than {MAX_DEPTH} deep"),
            });
        }

        let lane = self
            .config
            .lanes
            .get(lane_name)
            .ok_or_else(|| ShlaneError::LaneNotFound {
                name: lane_name.to_string(),
                available: self.config.lane_names(),
            })?;

        let params = resolve_params(lane_name, lane, params)?;
        let previous = self.enter(lane_name, params);

        let result = self.run_lane_body(lane_name, lane);

        *self.frame.borrow_mut() = previous;
        result
    }

    fn run_lane_body(&mut self, lane_name: &str, lane: &Lane) -> Result<()> {
        if self.depth > 0 {
            println!("\n-> lane '{lane_name}'");
        }

        self.run_steps("before hook", lane_name, &lane.before)?;
        self.run_steps("step", lane_name, &lane.steps)?;

        if let Some(source) = &lane.script {
            println!("Running Rhai script for lane '{lane_name}':");
            let started = Instant::now();
            let source = source.clone();
            let result = script::eval(&self.engine, &mut self.scope, &source);
            let duration = started.elapsed();
            match result {
                Ok(()) => self.record(lane_name, "script", Status::Ok, duration),
                Err(message) => {
                    self.record(lane_name, "script", Status::Failed, duration);
                    return Err(ShlaneError::Script {
                        lane: lane_name.to_string(),
                        phase: "lane",
                        message,
                    });
                }
            }
        }

        self.run_steps("after hook", lane_name, &lane.after)?;
        Ok(())
    }

    fn run_steps(&mut self, phase: &'static str, lane_name: &str, steps: &[Step]) -> Result<()> {
        for (index, step) in steps.iter().enumerate() {
            self.run_step(phase, lane_name, index + 1, step)?;
        }
        Ok(())
    }

    fn run_step(
        &mut self,
        phase: &'static str,
        lane_name: &str,
        index: usize,
        step: &Step,
    ) -> Result<()> {
        let label = step.label();

        if let Some(condition) = &step.condition {
            let keep =
                script::condition(&self.engine, &mut self.scope, condition).map_err(|message| {
                    ShlaneError::Script {
                        lane: lane_name.to_string(),
                        phase: "if",
                        message: format!("{condition}: {message}"),
                    }
                })?;
            if !keep {
                println!("Skipping {phase} '{label}' ({condition} is false)");
                self.record(lane_name, &label, Status::Skipped, Duration::ZERO);
                return Ok(());
            }
        }

        let started = Instant::now();
        let mut attempt = 0;
        let result = loop {
            attempt += 1;
            let result = self.execute(phase, lane_name, index, step, &label);
            match result {
                Ok(()) => break Ok(()),
                Err(err) if attempt <= step.retry => {
                    println!("Attempt {attempt} failed ({err}); retrying...");
                }
                Err(err) => break Err(err),
            }
        };
        let duration = started.elapsed();

        match result {
            Ok(()) => {
                self.record(lane_name, &label, Status::Ok, duration);
                Ok(())
            }
            Err(err) if step.continue_on_error => {
                eprintln!("warning: {err} (continuing, continue_on_error is set)");
                self.record(lane_name, &label, Status::Failed, duration);
                Ok(())
            }
            Err(err) => {
                self.record(lane_name, &label, Status::Failed, duration);
                Err(err)
            }
        }
    }

    fn execute(
        &mut self,
        phase: &'static str,
        lane_name: &str,
        index: usize,
        step: &Step,
        label: &str,
    ) -> Result<()> {
        match &step.kind {
            StepKind::Run(command) => self.execute_command(phase, lane_name, index, step, command, label),
            StepKind::Script(source) => {
                if self.options.dry_run {
                    println!("Would run script: {label}");
                    return Ok(());
                }
                let source = source.clone();
                script::eval(&self.engine, &mut self.scope, &source).map_err(|message| {
                    ShlaneError::Script {
                        lane: lane_name.to_string(),
                        phase: "step",
                        message,
                    }
                })
            }
            StepKind::Lane { name, with } => {
                let with = self.interpolate_map(with)?;
                self.depth += 1;
                let result = self.run_lane_inner(name, with);
                self.depth -= 1;
                result
            }
            StepKind::Action(name) => Err(ShlaneError::Script {
                lane: lane_name.to_string(),
                phase: "step",
                message: format!(
                    "action '{name}' is not implemented yet (planned for M3, see docs/plan/06-actions-core.md)"
                ),
            }),
        }
    }

    fn execute_command(
        &mut self,
        phase: &'static str,
        lane_name: &str,
        index: usize,
        step: &Step,
        command: &str,
        label: &str,
    ) -> Result<()> {
        let command = {
            let frame = self.frame.borrow();
            let meta = frame.meta();
            interpolate(
                command,
                &Vars {
                    params: &frame.params,
                    env: &frame.env,
                    meta: &meta,
                },
            )?
        };

        let env = self.step_env(&step.env)?;
        let workdir = self.step_workdir(step.workdir.as_deref())?;

        if self.options.dry_run {
            println!("Would run: {command}");
            return Ok(());
        }

        println!("Running: {command}");
        let outcome = shell::run(&command, &env, &workdir, step.timeout)?;

        if outcome.timed_out {
            return Err(ShlaneError::StepTimedOut {
                lane: lane_name.to_string(),
                step: label.to_string(),
                seconds: step.timeout.unwrap_or_default().as_secs(),
                command,
            });
        }
        if outcome.success {
            return Ok(());
        }
        Err(ShlaneError::StepFailed {
            lane: lane_name.to_string(),
            phase,
            index,
            command,
            code: outcome.code,
        })
    }

    /// Step-level `env:` is layered on top of the lane's environment.
    fn step_env(&self, overrides: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
        let frame = self.frame.borrow();
        let mut env = frame.env.clone();
        if overrides.is_empty() {
            return Ok(env);
        }
        let meta = frame.meta();
        let vars = Vars {
            params: &frame.params,
            env: &frame.env,
            meta: &meta,
        };
        for (key, value) in overrides {
            env.insert(key.clone(), interpolate_plain(value, &vars)?);
        }
        Ok(env)
    }

    fn step_workdir(&self, workdir: Option<&str>) -> Result<PathBuf> {
        let frame = self.frame.borrow();
        let Some(workdir) = workdir else {
            return Ok(frame.workdir.clone());
        };
        let meta = frame.meta();
        let vars = Vars {
            params: &frame.params,
            env: &frame.env,
            meta: &meta,
        };
        let rendered = interpolate_plain(workdir, &vars)?;
        Ok(frame.workdir.join(rendered))
    }

    fn interpolate_map(&self, map: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
        let frame = self.frame.borrow();
        let meta = frame.meta();
        let vars = Vars {
            params: &frame.params,
            env: &frame.env,
            meta: &meta,
        };
        map.iter()
            .map(|(key, value)| Ok((key.clone(), interpolate_plain(value, &vars)?)))
            .collect()
    }

    /// Install a new frame for `lane_name`, returning the one it replaced.
    fn enter(&mut self, lane_name: &str, params: BTreeMap<String, String>) -> Frame {
        let mut frame = self.frame.borrow_mut();
        let previous = Frame {
            lane: frame.lane.clone(),
            params: frame.params.clone(),
            env: frame.env.clone(),
            workdir: frame.workdir.clone(),
            dry_run: frame.dry_run,
        };
        frame.lane = lane_name.to_string();
        frame.params = params;
        frame.workdir = self.root.clone();
        previous
    }

    fn record(&mut self, lane: &str, label: &str, status: Status, duration: Duration) {
        self.records.push(Record {
            lane: lane.to_string(),
            label: label.to_string(),
            status,
            duration,
        });
    }

    fn print_summary(&self) {
        if self.records.is_empty() {
            return;
        }

        let total: Duration = self.records.iter().map(|record| record.duration).sum();
        let lane_width = self
            .records
            .iter()
            .map(|record| record.lane.chars().count())
            .max()
            .unwrap_or(4)
            .max(4);
        let label_width = self
            .records
            .iter()
            .map(|record| record.label.chars().count())
            .max()
            .unwrap_or(4)
            .clamp(4, 48);

        println!("\nSummary");
        println!(
            "  {:>3}  {:<lane_width$}  {:<label_width$}  {:<8}  {:>8}",
            "#", "lane", "step", "result", "time"
        );
        for (index, record) in self.records.iter().enumerate() {
            println!(
                "  {:>3}  {:<lane_width$}  {:<label_width$}  {:<8}  {:>8}",
                index + 1,
                record.lane,
                truncate(&record.label, label_width),
                record.status.symbol(),
                format_duration(record.duration),
            );
        }
        println!(
            "  {:>3}  {:<lane_width$}  {:<label_width$}  {:<8}  {:>8}",
            "",
            "",
            "",
            "total",
            format_duration(total),
        );
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

fn format_duration(duration: Duration) -> String {
    let seconds = duration.as_secs_f64();
    if seconds >= 60.0 {
        let minutes = duration.as_secs() / 60;
        let rest = seconds - (minutes * 60) as f64;
        return format!("{minutes}m {rest:.0}s");
    }
    format!("{seconds:.1}s")
}

/// Apply declared defaults and check what the caller passed.
fn resolve_params(
    lane_name: &str,
    lane: &Lane,
    given: BTreeMap<String, String>,
) -> Result<BTreeMap<String, String>> {
    let mut resolved = given;

    for (name, spec) in &lane.params {
        match resolved.get(name) {
            Some(value) => check_param(lane_name, name, spec, value)?,
            None => {
                if let Some(default) = &spec.default {
                    resolved.insert(name.clone(), default.clone());
                } else if spec.required {
                    return Err(ShlaneError::ParamInvalid {
                        lane: lane_name.to_string(),
                        param: name.clone(),
                        message: "is required but was not given".to_string(),
                    });
                }
            }
        }
    }

    Ok(resolved)
}

fn check_param(lane_name: &str, name: &str, spec: &ParamSpec, value: &str) -> Result<()> {
    if let Err(message) = spec.param_type.check(value) {
        return Err(ShlaneError::ParamInvalid {
            lane: lane_name.to_string(),
            param: name.to_string(),
            message,
        });
    }
    if let Some(values) = &spec.values {
        if !values.iter().any(|allowed| allowed == value) {
            return Err(ShlaneError::ParamInvalid {
                lane: lane_name.to_string(),
                param: name.to_string(),
                message: format!("must be one of {}, got '{value}'", values.join(", ")),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_durations() {
        assert_eq!(format_duration(Duration::from_millis(1500)), "1.5s");
        assert_eq!(format_duration(Duration::from_secs(75)), "1m 15s");
    }

    #[test]
    fn truncates_long_labels() {
        assert_eq!(truncate("abcdef", 4), "abc…");
        assert_eq!(truncate("abc", 4), "abc");
    }
}
