//! Lane execution.

use super::context::{Frame, Outputs, SharedCleanups, SharedFrame, SharedOutputs};
use super::interpolate::{interpolate, interpolate_plain, Vars};
use super::secrets::{Secrets, SharedSecrets};
use super::shell::{self, Spawn};
use super::ui::{Ui, Verbosity};
use super::{env as environment, signals};
use crate::config::model::{Config, Lane, ParamSpec, Step, StepKind};
use crate::config::validate;
use crate::error::{Result, ShlaneError};
use crate::script;
use crate::script::builtins::Runtime;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

/// How deeply `lane:` steps may nest before shlane gives up.
const MAX_DEPTH: usize = 16;

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub dry_run: bool,
    /// Where to write machine-readable results.
    pub reports: Vec<crate::report::Target>,
    pub verbosity: Verbosity,
    pub json: bool,
    /// Selects `.env.<profile>` (`docs/plan/10-secrets-and-env.md`).
    pub profile: Option<String>,
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

pub struct Runner {
    config: Rc<Config>,
    registry: Rc<crate::actions::Registry>,
    root: PathBuf,
    options: Options,
    frame: SharedFrame,
    outputs: SharedOutputs,
    cleanups: SharedCleanups,
    secrets: SharedSecrets,
    ui: Rc<Ui>,
    engine: rhai::Engine,
    scope: rhai::Scope<'static>,
    /// Shared so a lane called from a script lands in the same summary as one
    /// called by a `lane:` step.
    records: Rc<RefCell<Vec<Record>>>,
    /// Shared for the same reason, and because it is what stops a lane that
    /// calls itself from recursing until the stack runs out.
    depth: Rc<Cell<usize>>,
}

/// Everything a nested lane call needs, without borrowing the runner that is
/// currently executing.
///
/// `call_lane()` is a Rhai builtin, and the engine it runs in belongs to the
/// runner, so the builtin cannot be handed `&mut Runner`. It builds a second
/// runner instead, sharing the frame, outputs, secrets and summary, which is
/// exactly what a `lane:` step does by recursing.
struct LaneCall {
    config: Rc<Config>,
    registry: Rc<crate::actions::Registry>,
    root: PathBuf,
    options: Options,
    frame: SharedFrame,
    outputs: SharedOutputs,
    cleanups: SharedCleanups,
    secrets: SharedSecrets,
    ui: Rc<Ui>,
    records: Rc<RefCell<Vec<Record>>>,
    depth: Rc<Cell<usize>>,
}

impl crate::script::builtins::LaneCaller for LaneCall {
    fn call(&self, name: &str, params: BTreeMap<String, String>) -> Result<()> {
        let mut runner = Runner::sharing(self);
        self.depth.set(self.depth.get() + 1);
        let result = runner.run_lane_inner(name, params);
        self.depth.set(self.depth.get().saturating_sub(1));
        result
    }
}

/// Run a lane and print a summary of what happened.
pub fn run_lane(
    config: Rc<Config>,
    root: &Path,
    lane_name: &str,
    params: BTreeMap<String, String>,
    options: Options,
) -> Result<()> {
    let registry = Rc::new(crate::actions::Registry::builtins().with_plugins(
        crate::plugin::actions(crate::plugin::load_all(&config, root)?),
    ));

    let problems = validate::check(&config, &registry);
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

    signals::install();

    let reports = options.reports.clone();
    let mut runner = Runner::new(config.clone(), root, options, registry)?;
    let outcome = runner.run(lane_name, params);
    runner.print_summary();

    // Written whether the lane passed or failed: a report that only appears on
    // success is no use to the CI job that needs to explain the failure.
    for target in &reports {
        let steps = runner.step_reports();
        if let Err(err) = crate::report::write(target, lane_name, &steps, outcome.is_err()) {
            runner
                .ui
                .warn(&format!("could not write {}: {err}", target.path.display()));
        } else {
            runner
                .ui
                .detail(&format!("Wrote {}", target.path.display()));
        }
    }

    if outcome.is_ok() {
        runner
            .ui
            .say(&format!("\nLane '{lane_name}' completed successfully!"));
        runner.ui.event(&[
            ("type", "lane_finished"),
            ("lane", lane_name),
            ("result", "ok"),
        ]);
    } else {
        runner.ui.event(&[
            ("type", "lane_finished"),
            ("lane", lane_name),
            ("result", "failed"),
        ]);
    }
    outcome
}

impl Runner {
    fn new(
        config: Rc<Config>,
        root: &Path,
        options: Options,
        registry: Rc<crate::actions::Registry>,
    ) -> Result<Self> {
        let mut secret_registry = Secrets::new();
        let env = environment::build(
            &config,
            root,
            options.profile.as_deref(),
            &mut secret_registry,
        )?;

        // Values the config explicitly marks secret, once the environment they
        // refer to is known.
        let empty = BTreeMap::new();
        for pattern in &config.secrets {
            let vars = Vars {
                params: &empty,
                env: &env,
                meta: &empty,
                outputs: &empty,
                dry_run: false,
            };
            if let Ok(value) = interpolate_plain(pattern, &vars) {
                secret_registry.add(&value);
            }
        }

        let secrets: SharedSecrets = Rc::new(RefCell::new(secret_registry));
        let ui = Rc::new(Ui::new(options.verbosity, options.json, secrets.clone()));

        let frame: SharedFrame = Rc::new(RefCell::new(Frame {
            lane: String::new(),
            params: BTreeMap::new(),
            env,
            workdir: root.to_path_buf(),
            dry_run: options.dry_run,
        }));
        let outputs: SharedOutputs = Rc::new(RefCell::new(Outputs::default()));
        let cleanups: SharedCleanups = Rc::new(RefCell::new(Vec::new()));

        let records: Rc<RefCell<Vec<Record>>> = Rc::new(RefCell::new(Vec::new()));
        let depth = Rc::new(Cell::new(0));

        let caller = Rc::new(LaneCall {
            config: config.clone(),
            registry: registry.clone(),
            root: root.to_path_buf(),
            options: options.clone(),
            frame: frame.clone(),
            outputs: outputs.clone(),
            cleanups: cleanups.clone(),
            secrets: secrets.clone(),
            ui: ui.clone(),
            records: records.clone(),
            depth: depth.clone(),
        });

        let engine = script::engine::build(&Runtime {
            frame: frame.clone(),
            outputs: outputs.clone(),
            secrets: secrets.clone(),
            ui: ui.clone(),
            cleanups: cleanups.clone(),
            // Weak: the registry can hold a Rhai plugin, whose engine holds
            // this runtime, so a strong handle here would be a cycle that
            // never frees.
            registry: Rc::downgrade(&registry),
            lane_caller: Some(caller),
            depth: depth.clone(),
        });

        Ok(Self {
            config,
            registry,
            root: root.to_path_buf(),
            options,
            frame,
            outputs,
            cleanups,
            secrets,
            ui,
            engine,
            scope: rhai::Scope::new(),
            records,
            depth,
        })
    }

    /// A second runner over the same run: same frame, outputs, secrets and
    /// summary, its own Rhai engine and scope.
    fn sharing(call: &LaneCall) -> Self {
        let caller = Rc::new(LaneCall {
            config: call.config.clone(),
            registry: call.registry.clone(),
            root: call.root.clone(),
            options: call.options.clone(),
            frame: call.frame.clone(),
            outputs: call.outputs.clone(),
            cleanups: call.cleanups.clone(),
            secrets: call.secrets.clone(),
            ui: call.ui.clone(),
            records: call.records.clone(),
            depth: call.depth.clone(),
        });

        let engine = script::engine::build(&Runtime {
            frame: call.frame.clone(),
            outputs: call.outputs.clone(),
            secrets: call.secrets.clone(),
            ui: call.ui.clone(),
            cleanups: call.cleanups.clone(),
            registry: Rc::downgrade(&call.registry),
            lane_caller: Some(caller),
            depth: call.depth.clone(),
        });

        Self {
            config: call.config.clone(),
            registry: call.registry.clone(),
            root: call.root.clone(),
            options: call.options.clone(),
            frame: call.frame.clone(),
            outputs: call.outputs.clone(),
            cleanups: call.cleanups.clone(),
            secrets: call.secrets.clone(),
            ui: call.ui.clone(),
            engine,
            scope: rhai::Scope::new(),
            records: call.records.clone(),
            depth: call.depth.clone(),
        }
    }

    fn run(&mut self, lane_name: &str, params: BTreeMap<String, String>) -> Result<()> {
        self.ui
            .event(&[("type", "lane_started"), ("lane", lane_name)]);

        // A local handle, so the borrow of the lane below is on this Rc and
        // not on `self`, which the steps need mutably.
        let config = self.config.clone();

        if let Some(shared) = &config.script {
            self.ui.detail("Loading shared script...");
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
        let lane = match config.lanes.get(lane_name) {
            Some(lane) => lane,
            None => {
                return Err(ShlaneError::LaneNotFound {
                    name: lane_name.to_string(),
                    available: config.public_lane_names(),
                })
            }
        };
        let params = resolve_params(lane_name, lane, params)?;
        let previous = self.enter(lane_name, params);
        self.lane_env(lane)?;

        let result = self
            .run_steps("before_all", lane_name, &config.before_all)
            .and_then(|()| self.run_lane_body(lane_name, lane))
            .and_then(|()| self.run_steps("after_all", lane_name, &config.after_all));

        if result.is_err() && !config.error.is_empty() {
            self.ui.say("\nRunning error hooks...");
            // A failing error hook must not replace the failure that caused it.
            if let Err(err) = self.run_steps("error", lane_name, &config.error) {
                self.ui.warn(&format!("an error hook itself failed: {err}"));
            }
        }

        self.run_cleanups();

        *self.frame.borrow_mut() = previous;
        result
    }

    /// Undo what actions asked to have undone, most recent first.
    ///
    /// A cleanup that fails is reported and the next one still runs: leaving a
    /// keychain behind because an unrelated cleanup failed is how a CI machine
    /// ends up with forty of them.
    fn run_cleanups(&mut self) {
        let pending: Vec<_> = self.cleanups.borrow_mut().drain(..).rev().collect();
        if pending.is_empty() {
            return;
        }

        self.ui.say("\nCleaning up...");
        let env = self.frame.borrow().env.clone();
        let workdir = self.frame.borrow().workdir.clone();
        let secrets = self.secrets.borrow().clone();

        for cleanup in pending {
            self.ui.detail(&cleanup.what);
            if self.options.dry_run {
                self.ui.say(&format!("Would run: {}", cleanup.command));
                continue;
            }
            let outcome = crate::runtime::shell::run(crate::runtime::shell::Spawn {
                command: &cleanup.command,
                env: &env,
                workdir: &workdir,
                timeout: None,
                quiet: true,
                secrets: &secrets,
            });
            match outcome {
                Ok(outcome) if !outcome.success => self.ui.warn(&format!(
                    "could not clean up {}: exit code {}",
                    cleanup.what,
                    outcome.code.unwrap_or(-1)
                )),
                Err(err) => self
                    .ui
                    .warn(&format!("could not clean up {}: {err}", cleanup.what)),
                Ok(_) => {}
            }
        }
    }

    fn run_lane_inner(&mut self, lane_name: &str, params: BTreeMap<String, String>) -> Result<()> {
        if self.depth.get() >= MAX_DEPTH {
            return Err(ShlaneError::Script {
                lane: lane_name.to_string(),
                phase: "lane",
                message: format!("lanes nested more than {MAX_DEPTH} deep"),
            });
        }

        let config = self.config.clone();
        let lane = config
            .lanes
            .get(lane_name)
            .ok_or_else(|| ShlaneError::LaneNotFound {
                name: lane_name.to_string(),
                available: config.lane_names(),
            })?;

        let params = resolve_params(lane_name, lane, params)?;
        let previous = self.enter(lane_name, params);
        let lane_env = self.lane_env(lane);

        let result = lane_env.and_then(|()| self.run_lane_body(lane_name, lane));

        *self.frame.borrow_mut() = previous;
        result
    }

    fn run_lane_body(&mut self, lane_name: &str, lane: &Lane) -> Result<()> {
        if self.depth.get() > 0 {
            self.ui.say(&format!("\n-> lane '{lane_name}'"));
            self.ui
                .event(&[("type", "lane_started"), ("lane", lane_name)]);
        }

        if self.ui.is_verbose() {
            let frame = self.frame.borrow();
            for (key, value) in &frame.params {
                self.ui.detail(&format!("  param {key}={value}"));
            }
        }

        self.run_steps("before hook", lane_name, &lane.before)?;
        self.run_steps("step", lane_name, &lane.steps)?;

        if let Some(source) = &lane.script {
            self.ui
                .say(&format!("Running Rhai script for lane '{lane_name}':"));
            let started = Instant::now();
            let source = source.clone();
            let result = script::eval(&self.engine, &mut self.scope, &source);
            let duration = started.elapsed();
            match result {
                Ok(()) => self.record(lane_name, "script", Status::Ok, duration),
                Err(failure) => {
                    self.record(lane_name, "script", Status::Failed, duration);
                    return Err(script_error(lane_name, "lane", failure));
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
                self.ui.say(&format!(
                    "Skipping {phase} '{label}' ({condition} is false)"
                ));
                self.ui.event(&[
                    ("type", "step_skipped"),
                    ("lane", lane_name),
                    ("step", &label),
                ]);
                self.record(lane_name, &label, Status::Skipped, Duration::ZERO);
                return Ok(());
            }
        }

        self.ui.event(&[
            ("type", "step_started"),
            ("lane", lane_name),
            ("step", &label),
        ]);

        let started = Instant::now();
        let mut attempt = 0;
        let result = loop {
            attempt += 1;
            let result = self.execute(phase, lane_name, index, step, &label);
            match result {
                Ok(()) => break Ok(()),
                Err(err) if attempt <= step.retry => {
                    self.ui
                        .say(&format!("Attempt {attempt} failed ({err}); retrying..."));
                }
                Err(err) => break Err(err),
            }
        };
        let duration = started.elapsed();

        self.ui.event(&[
            ("type", "step_finished"),
            ("lane", lane_name),
            ("step", &label),
            ("result", if result.is_ok() { "ok" } else { "failed" }),
        ]);

        match result {
            Ok(()) => {
                self.record(lane_name, &label, Status::Ok, duration);
                Ok(())
            }
            Err(err) if matches!(err, ShlaneError::Interrupted { .. }) => {
                self.record(lane_name, &label, Status::Failed, duration);
                Err(err)
            }
            Err(err) if step.continue_on_error => {
                self.ui
                    .warn(&format!("{err} (continuing, continue_on_error is set)"));
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
            StepKind::Run(command) => {
                self.execute_command(phase, lane_name, index, step, command, label)
            }
            StepKind::Script(source) => {
                if self.options.dry_run {
                    self.ui.say(&format!("Would run script: {label}"));
                    return Ok(());
                }
                let source = source.clone();
                script::eval(&self.engine, &mut self.scope, &source)
                    .map_err(|failure| script_error(lane_name, "step", failure))
            }
            StepKind::Lane { name, with } => {
                let with = self.interpolate_map(with)?;
                self.depth.set(self.depth.get() + 1);
                let result = self.run_lane_inner(name, with);
                self.depth.set(self.depth.get().saturating_sub(1));
                result
            }
            StepKind::Action { name, with } => self.execute_action(step, name, with),
        }
    }

    /// Run an action step: resolve its arguments, hand it a context, and store
    /// what it produced under the step's id.
    fn execute_action(
        &mut self,
        step: &Step,
        name: &str,
        with: &BTreeMap<String, String>,
    ) -> Result<()> {
        let registry = self.registry.clone();
        let Some(action) = registry.find(name) else {
            return Err(ShlaneError::Action {
                action: name.to_string(),
                message: format!("no such action (try: {})", registry.names().join(", ")),
            });
        };

        // An argument the action runs as a shell command is escaped like a
        // `run:` step. Everything else is substituted literally: an action
        // decides what its own argument means, and quoting a file path would
        // put quotes in the path.
        let shell_args: Vec<String> = action
            .schema()
            .into_iter()
            .filter(|spec| spec.shell)
            .map(|spec| spec.name)
            .collect();
        let provided = self.interpolate_args(with, &shell_args)?;
        let args = crate::actions::with_defaults(action, &provided);

        // Arguments the action declares as sensitive are hidden from here on.
        for spec in action.schema() {
            if spec.sensitive {
                if let Some(value) = args.get(&spec.name) {
                    self.secrets.borrow_mut().add(value);
                }
            }
        }

        let env = self.step_env(&step.env)?;
        let workdir = self.step_workdir(step.workdir.as_deref())?;
        let (lane, dry_run) = {
            let frame = self.frame.borrow();
            (frame.lane.clone(), frame.dry_run)
        };

        let mut ctx = crate::actions::context::ActionContext {
            lane,
            env: &env,
            workdir,
            dry_run,
            ui: self.ui.clone(),
            secrets: self.secrets.clone(),
            frame: self.frame.clone(),
            outputs: self.outputs.clone(),
            cleanups: self.cleanups.clone(),
            registry: Rc::downgrade(&self.registry),
            depth: self.depth.clone(),
        };

        let output = action.run(&mut ctx, &args)?;

        if let Some(id) = &step.id {
            let mut outputs = self.outputs.borrow_mut();
            for (key, value) in output.0 {
                outputs.set(id, &key, value);
            }
        }

        Ok(())
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
        let command = self.with_vars(|vars| interpolate(command, vars))?;

        let env = self.step_env(&step.env)?;
        let workdir = self.step_workdir(step.workdir.as_deref())?;

        if self.options.dry_run {
            self.ui.say(&format!("Would run: {command}"));
            return Ok(());
        }

        self.ui.say(&format!("Running: {command}"));
        if self.ui.is_verbose() {
            self.ui.detail(&format!("  in {}", workdir.display()));
        }

        let outcome = {
            let secrets = self.secrets.borrow().clone();
            shell::run(Spawn {
                command: &command,
                env: &env,
                workdir: &workdir,
                timeout: step.timeout,
                quiet: false,
                secrets: &secrets,
            })?
        };

        if let Some(id) = &step.id {
            let mut outputs = self.outputs.borrow_mut();
            outputs.set(id, "stdout", outcome.stdout.trim_end());
            outputs.set(id, "stderr", outcome.stderr.trim_end());
            outputs.set(id, "code", outcome.code.unwrap_or(-1).to_string());
        }

        if outcome.interrupted {
            return Err(ShlaneError::Interrupted {
                lane: lane_name.to_string(),
                step: label.to_string(),
            });
        }

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

    /// Layer the lane's own `env:` on top of what it inherited.
    fn lane_env(&self, lane: &Lane) -> Result<()> {
        if lane.env.is_empty() {
            return Ok(());
        }
        let rendered = self.with_vars(|vars| {
            lane.env
                .iter()
                .map(|(key, value)| Ok((key.clone(), interpolate_plain(value, vars)?)))
                .collect::<Result<BTreeMap<String, String>>>()
        })?;

        let mut frame = self.frame.borrow_mut();
        for (key, value) in rendered {
            if crate::runtime::secrets::is_sensitive_name(&key) {
                self.secrets.borrow_mut().add(&value);
            }
            frame.env.insert(key, value);
        }
        Ok(())
    }

    /// Run `body` with everything `${...}` can resolve against.
    fn with_vars<T>(&self, body: impl FnOnce(&Vars<'_>) -> Result<T>) -> Result<T> {
        let frame = self.frame.borrow();
        let meta = frame.meta();
        let outputs = self.outputs.borrow().flatten();
        body(&Vars {
            params: &frame.params,
            env: &frame.env,
            meta: &meta,
            outputs: &outputs,
            dry_run: frame.dry_run,
        })
    }

    /// Step-level `env:` is layered on top of the lane's environment.
    fn step_env(&self, overrides: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
        let mut env = self.frame.borrow().env.clone();
        if overrides.is_empty() {
            return Ok(env);
        }
        let rendered = self.with_vars(|vars| {
            overrides
                .iter()
                .map(|(key, value)| Ok((key.clone(), interpolate_plain(value, vars)?)))
                .collect::<Result<BTreeMap<String, String>>>()
        })?;

        for (key, value) in rendered {
            if crate::runtime::secrets::is_sensitive_name(&key) {
                self.secrets.borrow_mut().add(&value);
            }
            env.insert(key, value);
        }
        Ok(env)
    }

    fn step_workdir(&self, workdir: Option<&str>) -> Result<PathBuf> {
        let Some(workdir) = workdir else {
            return Ok(self.frame.borrow().workdir.clone());
        };
        let rendered = self.with_vars(|vars| interpolate_plain(workdir, vars))?;
        Ok(self.frame.borrow().workdir.join(rendered))
    }

    /// Like [`interpolate_map`](Self::interpolate_map), escaping the arguments
    /// named in `shell_args` the way a `run:` command is escaped.
    fn interpolate_args(
        &self,
        map: &BTreeMap<String, String>,
        shell_args: &[String],
    ) -> Result<BTreeMap<String, String>> {
        self.with_vars(|vars| {
            map.iter()
                .map(|(key, value)| {
                    let rendered = if shell_args.contains(key) {
                        interpolate(value, vars)?
                    } else {
                        interpolate_plain(value, vars)?
                    };
                    Ok((key.clone(), rendered))
                })
                .collect()
        })
    }

    fn interpolate_map(&self, map: &BTreeMap<String, String>) -> Result<BTreeMap<String, String>> {
        self.with_vars(|vars| {
            map.iter()
                .map(|(key, value)| Ok((key.clone(), interpolate_plain(value, vars)?)))
                .collect()
        })
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
        self.records.borrow_mut().push(Record {
            lane: lane.to_string(),
            label: label.to_string(),
            status,
            duration,
        });
    }

    /// The run's steps, for `--report`.
    fn step_reports(&self) -> Vec<crate::report::StepReport> {
        self.records
            .borrow()
            .iter()
            .map(|record| crate::report::StepReport {
                lane: record.lane.clone(),
                step: record.label.clone(),
                status: match record.status {
                    Status::Ok => crate::report::Status::Ok,
                    Status::Skipped => crate::report::Status::Skipped,
                    Status::Failed => crate::report::Status::Failed,
                },
                duration: record.duration,
            })
            .collect()
    }

    fn print_summary(&self) {
        if self.records.borrow().is_empty() {
            return;
        }

        let records = self.records.borrow();
        let total: Duration = records.iter().map(|record| record.duration).sum();
        let lane_width = records
            .iter()
            .map(|record| record.lane.chars().count())
            .max()
            .unwrap_or(4)
            .max(4);
        let label_width = records
            .iter()
            .map(|record| record.label.chars().count())
            .max()
            .unwrap_or(4)
            .clamp(4, 48);

        // Through the UI, so --quiet and --json suppress it and secrets in a
        // step's name are masked.
        self.ui.say("\nSummary");
        self.ui.say(&format!(
            "  {:>3}  {:<lane_width$}  {:<label_width$}  {:<8}  {:>8}",
            "#", "lane", "step", "result", "time"
        ));
        for (index, record) in records.iter().enumerate() {
            self.ui.say(&format!(
                "  {:>3}  {:<lane_width$}  {:<label_width$}  {:<8}  {:>8}",
                index + 1,
                record.lane,
                truncate(&record.label, label_width),
                record.status.symbol(),
                format_duration(record.duration),
            ));
        }
        self.ui.say(&format!(
            "  {:>3}  {:<lane_width$}  {:<label_width$}  {:<8}  {:>8}",
            "",
            "",
            "",
            "total",
            format_duration(total),
        ));
    }
}

/// Turn a script failure into an error.
///
/// A failure raised by a builtin -- an action that failed, a lane that could
/// not be called -- is already a shlane error, and is passed through whole. Only
/// a mistake in the script itself gets wrapped, so a lane that calls itself
/// reports the nesting limit rather than one wrapper per level with the reason
/// buried at the end.
fn script_error(lane: &str, phase: &'static str, failure: script::Failure) -> ShlaneError {
    match failure {
        script::Failure::Shlane(inner) => inner,
        script::Failure::Script(message) => ShlaneError::Script {
            lane: lane.to_string(),
            phase,
            message,
        },
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
