pub mod builtins;
pub mod engine;

use rhai::{Engine, Module, Scope};

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

/// Evaluate a `if:` condition.
///
/// Conditions are Rhai expressions rather than `${...}` templates: shell
/// quoting rules do not apply inside Rhai, so `param("x") == "y"` is both
/// unambiguous and impossible to mis-quote.
pub fn condition(engine: &Engine, scope: &mut Scope<'_>, source: &str) -> Result<bool, String> {
    engine
        .eval_expression_with_scope::<bool>(scope, source.trim())
        .map_err(|err| match err.to_string() {
            message if message.contains("Output type incorrect") => {
                format!("`if` must evaluate to true or false: {message}")
            }
            message => message,
        })
}

/// Evaluate the config's shared script and publish its functions to every lane.
///
/// Running the source is not enough: Rhai keeps function definitions in the
/// compiled AST, not in the scope, so in v0.1.0 a lane calling a shared function
/// failed with "Function not found" -- including the `greet()` call in the
/// shipped example. The functions are lifted into a module and registered on the
/// engine, while top-level statements run exactly once, here.
pub fn load_shared(engine: &mut Engine, scope: &mut Scope<'_>, source: &str) -> Result<(), String> {
    let ast = engine.compile(source).map_err(|err| err.to_string())?;

    engine
        .run_ast_with_scope(scope, &ast)
        .map_err(|err| err.to_string())?;

    // Statements have already run; keep only the function definitions so
    // evaluating the module does not run them a second time.
    let mut functions = ast.clone();
    functions.clear_statements();

    let module =
        Module::eval_ast_as_new(Scope::new(), &functions, engine).map_err(|err| err.to_string())?;
    engine.register_global_module(module.into());

    Ok(())
}
