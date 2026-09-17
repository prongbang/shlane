//! Functions available to Rhai scripts (`docs/plan/05-scripting-rhai.md`).
//!
//! Every builtin reads the current [`Frame`](crate::runtime::context::Frame),
//! so a nested `lane:` call sees its own parameters rather than the ones the
//! outermost lane was started with.

use crate::runtime::context::{SharedFrame, SharedOutputs};
use crate::runtime::secrets::SharedSecrets;
use crate::runtime::shell::{self, Spawn};
use crate::runtime::ui::Ui;
use rhai::{Engine, EvalAltResult};
use std::rc::Rc;

/// What the builtins need to reach.
#[derive(Clone)]
pub struct Runtime {
    pub frame: SharedFrame,
    pub outputs: SharedOutputs,
    pub secrets: SharedSecrets,
    pub ui: Rc<Ui>,
    /// `None` inside a Rhai plugin: the registry holds the plugin, so handing
    /// the plugin the registry back would be a cycle. Such a script gets every
    /// other builtin.
    pub registry: Option<Rc<crate::actions::Registry>>,
}

/// What a command did. Returned by `run()`, `try_run()`.
#[derive(Debug, Clone, Default)]
pub struct CmdResult {
    pub code: i64,
    pub stdout: String,
    pub stderr: String,
    pub success: bool,
}

type Fallible<T> = Result<T, Box<EvalAltResult>>;

pub fn register(engine: &mut Engine, runtime: &Runtime) {
    register_result_type(engine);
    register_params(engine, runtime.clone());
    register_env(engine, runtime.clone());
    register_commands(engine, runtime.clone());
    register_outputs(engine, runtime.clone());
    // Registered even inside a Rhai plugin, where it cannot work: "Function not
    // found: action" tells nobody why.
    register_actions(engine, runtime.clone());
    register_ci(engine, runtime.clone());
    register_ui(engine, runtime.clone());
}

fn register_result_type(engine: &mut Engine) {
    engine
        .register_type_with_name::<CmdResult>("CmdResult")
        .register_get("code", |result: &mut CmdResult| result.code)
        .register_get("stdout", |result: &mut CmdResult| result.stdout.clone())
        .register_get("stderr", |result: &mut CmdResult| result.stderr.clone())
        .register_get("success", |result: &mut CmdResult| result.success)
        .register_fn("to_string", |result: &mut CmdResult| {
            format!(
                "CmdResult(code: {}, success: {})",
                result.code, result.success
            )
        });
}

fn register_params(engine: &mut Engine, runtime: Runtime) {
    let frame = runtime.frame.clone();
    engine.register_fn("param", move |key: &str| -> String {
        frame.borrow().params.get(key).cloned().unwrap_or_default()
    });

    let frame = runtime.frame.clone();
    engine.register_fn("param_or", move |key: &str, fallback: &str| -> String {
        frame
            .borrow()
            .params
            .get(key)
            .cloned()
            .unwrap_or_else(|| fallback.to_string())
    });

    let frame = runtime.frame;
    engine.register_fn("has_param", move |key: &str| -> bool {
        frame.borrow().params.contains_key(key)
    });
}

/// `env()` reads the lane's environment, not the process environment, so it
/// agrees with what commands in the same lane actually see.
fn register_env(engine: &mut Engine, runtime: Runtime) {
    let frame = runtime.frame.clone();
    engine.register_fn("env", move |key: &str| -> String {
        frame.borrow().env.get(key).cloned().unwrap_or_default()
    });

    let frame = runtime.frame;
    let secrets = runtime.secrets;
    engine.register_fn("set_env", move |key: &str, value: &str| {
        if crate::runtime::secrets::is_sensitive_name(key) {
            secrets.borrow_mut().add(value);
        }
        frame
            .borrow_mut()
            .env
            .insert(key.to_string(), value.to_string());
    });
}

fn register_commands(engine: &mut Engine, runtime: Runtime) {
    // `skip_on_dry_run` marks a command that changes something. A read runs
    // even under --dry-run: a dry run that invents results reports problems
    // that do not exist and hides the ones that do, which is why `capture()`
    // returning nothing would quietly turn "v2.1.0" into "v".
    let execute = move |runtime: &Runtime,
                        command: &str,
                        quiet: bool,
                        skip_on_dry_run: bool|
          -> Fallible<CmdResult> {
        // Copy what is needed and drop the borrow: the command may take
        // minutes, and a builtin it calls may want the frame too.
        let (env, workdir, dry_run) = {
            let frame = runtime.frame.borrow();
            (frame.env.clone(), frame.workdir.clone(), frame.dry_run)
        };

        if dry_run && skip_on_dry_run {
            runtime.ui.say(&format!("Would execute: {command}"));
            return Ok(CmdResult {
                code: 0,
                success: true,
                ..CmdResult::default()
            });
        }

        if !quiet {
            runtime.ui.say(&format!("Executing: {command}"));
        }

        let outcome = shell::run(Spawn {
            command,
            env: &env,
            workdir: &workdir,
            timeout: None,
            quiet,
            secrets: &runtime.secrets.borrow().clone(),
        })
        .map_err(|err| EvalAltResult::ErrorSystem("command".into(), Box::new(err)))?;

        Ok(CmdResult {
            code: outcome.code.unwrap_or(-1).into(),
            stdout: outcome.stdout,
            stderr: outcome.stderr,
            success: outcome.success,
        })
    };

    let inner = runtime.clone();
    let run_fn = execute;
    engine.register_fn("run", move |command: &str| -> Fallible<CmdResult> {
        let result = run_fn(&inner, command, false, true)?;
        if !result.success {
            // Unlike v0.1.0, a failed command stops the script instead of
            // letting the next line run on a broken state.
            return Err(format!("command failed with exit code {}: {command}", result.code).into());
        }
        Ok(result)
    });

    let inner = runtime.clone();
    engine.register_fn("try_run", move |command: &str| -> Fallible<CmdResult> {
        run_fn(&inner, command, false, true)
    });

    let inner = runtime;
    engine.register_fn("capture", move |command: &str| -> Fallible<String> {
        let result = run_fn(&inner, command, true, false)?;
        if !result.success {
            return Err(format!("command failed with exit code {}: {command}", result.code).into());
        }
        Ok(result.stdout.trim_end().to_string())
    });
}

fn register_outputs(engine: &mut Engine, runtime: Runtime) {
    let outputs = runtime.outputs.clone();
    let frame = runtime.frame.clone();
    engine.register_fn("set_output", move |key: &str, value: &str| {
        let lane = frame.borrow().lane.clone();
        outputs.borrow_mut().set(&lane, key, value);
    });

    let outputs = runtime.outputs;
    engine.register_fn("output", move |id: &str, key: &str| -> String {
        outputs.borrow().get(id, key).cloned().unwrap_or_default()
    });
}

/// `action("git_tag", #{ name: "v1.0.0" })`, so a script can reach the same
/// actions a step can.
fn register_actions(engine: &mut Engine, runtime: Runtime) {
    let inner = runtime.clone();
    engine.register_fn(
        "action",
        move |name: &str, args: rhai::Map| -> Fallible<rhai::Map> {
            run_action(&inner, name, args)
        },
    );

    let inner = runtime;
    engine.register_fn("action", move |name: &str| -> Fallible<rhai::Map> {
        run_action(&inner, name, rhai::Map::new())
    });
}

fn run_action(runtime: &Runtime, name: &str, args: rhai::Map) -> Fallible<rhai::Map> {
    let Some(registry) = runtime.registry.clone() else {
        return Err(
            "action() is not available inside a Rhai plugin: the registry holds the plugin, so a plugin cannot be handed it back. Use run() or capture(), or write the plugin as an executable."
                .into(),
        );
    };
    let Some(action) = registry.find(name) else {
        return Err(format!(
            "no such action '{name}' (try: {})",
            registry.names().join(", ")
        )
        .into());
    };

    let provided: std::collections::BTreeMap<String, String> = args
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();

    let problems = crate::actions::check_args(action, &provided);
    if !problems.is_empty() {
        return Err(problems.join("; ").into());
    }

    let args = crate::actions::with_defaults(action, &provided);
    for spec in action.schema() {
        if spec.sensitive {
            if let Some(value) = args.get(&spec.name) {
                runtime.secrets.borrow_mut().add(value);
            }
        }
    }

    // Copy what the action needs and drop the borrow: it may run for minutes.
    let (lane, env, workdir, dry_run) = {
        let frame = runtime.frame.borrow();
        (
            frame.lane.clone(),
            frame.env.clone(),
            frame.workdir.clone(),
            frame.dry_run,
        )
    };

    let mut ctx = crate::actions::context::ActionContext {
        lane,
        env: &env,
        workdir,
        dry_run,
        ui: runtime.ui.clone(),
        secrets: runtime.secrets.clone(),
        frame: runtime.frame.clone(),
        outputs: runtime.outputs.clone(),
    };

    let output = action
        .run(&mut ctx, &args)
        .map_err(|err| EvalAltResult::ErrorSystem("action".into(), Box::new(err)))?;

    Ok(output
        .0
        .into_iter()
        .map(|(key, value)| (key.into(), rhai::Dynamic::from(value)))
        .collect())
}

/// Enough to branch on in a condition: `if: is_ci()`.
fn register_ci(engine: &mut Engine, runtime: Runtime) {
    let frame = runtime.frame.clone();
    engine.register_fn("is_ci", move || -> bool {
        crate::runtime::ci::detect(&frame.borrow().env).is_some()
    });

    let frame = runtime.frame;
    engine.register_fn("ci_provider", move || -> String {
        crate::runtime::ci::detect(&frame.borrow().env)
            .map(|provider| provider.as_str().to_string())
            .unwrap_or_default()
    });
}

fn register_ui(engine: &mut Engine, runtime: Runtime) {
    let ui = runtime.ui.clone();
    engine.register_fn("ui_message", move |text: &str| ui.say(text));

    let ui = runtime.ui.clone();
    engine.register_fn("ui_success", move |text: &str| ui.say(&format!("✔ {text}")));

    let ui = runtime.ui.clone();
    engine.register_fn("ui_error", move |text: &str| ui.error(text));

    let secrets = runtime.secrets;
    engine.register_fn("secret", move |value: &str| {
        secrets.borrow_mut().add(value);
    });
}
