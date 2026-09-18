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
pub fn eval(engine: &Engine, scope: &mut Scope<'_>, source: &str) -> Result<(), Failure> {
    engine.run_with_scope(scope, source).map_err(Failure::from)
}

/// Why a script stopped.
///
/// A builtin that calls an action or another lane fails with a real
/// [`ShlaneError`](crate::error::ShlaneError) inside it. Stringifying that at
/// every level turns a lane calling itself into a screen of
/// "step script failed: lane: step script failed: ..." with the actual reason
/// at the end, so the original error is carried out whole instead.
pub enum Failure {
    /// An error from shlane itself, raised by a builtin.
    Shlane(crate::error::ShlaneError),
    /// A mistake in the script: a syntax error, a missing function, a type.
    Script(String),
}

impl From<Box<rhai::EvalAltResult>> for Failure {
    fn from(err: Box<rhai::EvalAltResult>) -> Self {
        match unwrap_shlane(*err) {
            Ok(inner) => Self::Shlane(inner),
            Err(message) => Self::Script(message),
        }
    }
}

/// Dig a `ShlaneError` out of however deep Rhai wrapped it.
///
/// A failure inside a called function arrives as `ErrorInFunctionCall` around
/// the `ErrorSystem` a builtin raised.
fn unwrap_shlane(err: rhai::EvalAltResult) -> Result<crate::error::ShlaneError, String> {
    use rhai::EvalAltResult;
    match err {
        EvalAltResult::ErrorSystem(_, boxed) => match boxed.downcast::<crate::error::ShlaneError>()
        {
            Ok(inner) => Ok(*inner),
            Err(other) => Err(other.to_string()),
        },
        EvalAltResult::ErrorInFunctionCall(name, _, inner, _) => {
            unwrap_shlane(*inner).map_err(|message| if message.is_empty() { name } else { message })
        }
        other => Err(other.to_string()),
    }
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
