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
    let execute = move |runtime: &Runtime, command: &str, quiet: bool| -> Fallible<CmdResult> {
        // Copy what is needed and drop the borrow: the command may take
        // minutes, and a builtin it calls may want the frame too.
        let (env, workdir, dry_run) = {
            let frame = runtime.frame.borrow();
            (frame.env.clone(), frame.workdir.clone(), frame.dry_run)
        };

        if dry_run {
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
        let result = run_fn(&inner, command, false)?;
        if !result.success {
            // Unlike v0.1.0, a failed command stops the script instead of
            // letting the next line run on a broken state.
            return Err(format!("command failed with exit code {}: {command}", result.code).into());
        }
        Ok(result)
    });

    let inner = runtime.clone();
    engine.register_fn("try_run", move |command: &str| -> Fallible<CmdResult> {
        run_fn(&inner, command, false)
    });

    let inner = runtime;
    engine.register_fn("capture", move |command: &str| -> Fallible<String> {
        let result = run_fn(&inner, command, true)?;
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
