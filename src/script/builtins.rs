//! Functions available to Rhai scripts.
//!
//! The signatures match v0.1.0 so existing scripts keep working. The richer
//! API — `run()` returning stdout, `set_output()`, `call_lane()` — is M2
//! (`docs/plan/05-scripting-rhai.md`).

use crate::runtime::shell;
use crate::runtime::Context;
use rhai::Engine;
use std::collections::BTreeMap;
use std::path::PathBuf;

pub fn register(engine: &mut Engine, ctx: &Context) {
    register_param(engine, ctx.params.clone());
    register_env(engine, ctx.env.clone());
    register_run(engine, ctx.env.clone(), ctx.workdir.clone());
}

fn register_param(engine: &mut Engine, params: BTreeMap<String, String>) {
    engine.register_fn("param", move |key: &str| -> String {
        params.get(key).cloned().unwrap_or_default()
    });
}

/// `env()` reads the lane's environment, not the process environment, so it
/// agrees with what commands in the same lane actually see.
fn register_env(engine: &mut Engine, env: BTreeMap<String, String>) {
    engine.register_fn("env", move |key: &str| -> String {
        env.get(key)
            .cloned()
            .or_else(|| std::env::var(key).ok())
            .unwrap_or_default()
    });
}

fn register_run(engine: &mut Engine, env: BTreeMap<String, String>, workdir: PathBuf) {
    engine.register_fn("run", move |cmd: &str| -> i64 {
        println!("Executing: {cmd}");
        match shell::run(cmd, &env, &workdir) {
            Ok(outcome) => outcome.code.unwrap_or(-1).into(),
            Err(err) => {
                eprintln!("error: {err}");
                -1
            }
        }
    });
}
