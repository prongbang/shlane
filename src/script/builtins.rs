//! Functions available to Rhai scripts.
//!
//! The signatures match v0.1.0 so existing scripts keep working. The richer
//! API — `run()` returning stdout, `set_output()`, `call_lane()` — is M2
//! (`docs/plan/05-scripting-rhai.md`).
//!
//! Every builtin reads the current [`Frame`], so a nested `lane:` call sees its
//! own parameters rather than the ones the outermost lane was started with.

use crate::runtime::context::SharedFrame;
use crate::runtime::shell;
use rhai::Engine;

pub fn register(engine: &mut Engine, frame: &SharedFrame) {
    register_param(engine, frame.clone());
    register_env(engine, frame.clone());
    register_run(engine, frame.clone());
}

fn register_param(engine: &mut Engine, frame: SharedFrame) {
    engine.register_fn("param", move |key: &str| -> String {
        frame.borrow().params.get(key).cloned().unwrap_or_default()
    });
}

/// `env()` reads the lane's environment, not the process environment, so it
/// agrees with what commands in the same lane actually see.
fn register_env(engine: &mut Engine, frame: SharedFrame) {
    engine.register_fn("env", move |key: &str| -> String {
        frame
            .borrow()
            .env
            .get(key)
            .cloned()
            .or_else(|| std::env::var(key).ok())
            .unwrap_or_default()
    });
}

fn register_run(engine: &mut Engine, frame: SharedFrame) {
    engine.register_fn("run", move |cmd: &str| -> i64 {
        let frame = frame.borrow();
        if frame.dry_run {
            println!("Would execute: {cmd}");
            return 0;
        }
        println!("Executing: {cmd}");
        match shell::run(cmd, &frame.env, &frame.workdir, None) {
            Ok(outcome) => outcome.code.unwrap_or(-1).into(),
            Err(err) => {
                eprintln!("error: {err}");
                -1
            }
        }
    });
}
