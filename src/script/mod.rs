pub mod builtins;
pub mod engine;

use rhai::{Engine, Scope};

/// Evaluate a script for its side effects.
///
/// Two differences from v0.1.0:
///
/// * a failing script aborts the lane, instead of being printed and ignored --
///   which used to let a lane report success after its script had failed;
/// * the script's trailing value is discarded. `eval_with_scope::<()>` rejected
///   any script ending in an expression, so a lane ending in `run("...")` failed
///   with "Output type incorrect" even though the command had run fine.
pub fn eval(engine: &Engine, scope: &mut Scope<'_>, source: &str) -> Result<(), String> {
    engine
        .run_with_scope(scope, source)
        .map_err(|err| err.to_string())
}
