//! A plugin written in Rhai (`docs/plan/09-plugins.md`, option B).
//!
//! The manifest points at a `.rhai` file, and each action is a function in it
//! with the same name. The function is handed a map of arguments and returns a
//! map of outputs.
//!
//! This is for glue: it needs no compiler and no separate process, and it gets
//! the same builtins a lane's own script has — `run`, `capture`, `env`,
//! `set_env`, `ui_message`, `secret`. The exception is `action()`, which would
//! mean handing a plugin the registry that holds it.

use super::ManifestAction;
use crate::actions::context::ActionContext;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::{Result, ShlaneError};
use crate::script::builtins::Runtime;
use rhai::{Dynamic, Map, Scope};
use std::path::{Path, PathBuf};

pub struct RhaiAction {
    declared: ManifestAction,
    plugin: String,
    script: PathBuf,
}

impl RhaiAction {
    pub fn new(declared: ManifestAction, plugin: String, script: PathBuf) -> Self {
        Self {
            declared,
            plugin,
            script,
        }
    }
}

/// Everything a plugin script can reach, minus `action()`.
pub fn engine_for(ctx: &ActionContext<'_>) -> rhai::Engine {
    crate::script::engine::build(&Runtime {
        frame: ctx.frame.clone(),
        outputs: ctx.outputs.clone(),
        secrets: ctx.secrets.clone(),
        ui: ctx.ui.clone(),
        cleanups: ctx.cleanups.clone(),
        registry: ctx.registry.clone(),
        // A plugin runs as one step; there is no lane for it to return to.
        lane_caller: None,
        depth: ctx.depth.clone(),
    })
}

/// Compile a plugin's script, for running it or for checking it.
pub fn compile(engine: &rhai::Engine, script: &Path) -> std::result::Result<rhai::AST, String> {
    let source = std::fs::read_to_string(script)
        .map_err(|err| format!("cannot read {}: {err}", script.display()))?;
    engine
        .compile(&source)
        .map_err(|err| format!("{}: {err}", script.display()))
}

/// The functions a compiled script defines.
pub fn function_names(ast: &rhai::AST) -> Vec<String> {
    ast.iter_functions()
        .map(|function| function.name.to_string())
        .collect()
}

impl Action for RhaiAction {
    fn name(&self) -> &str {
        &self.declared.name
    }

    fn description(&self) -> &str {
        self.declared
            .description
            .as_deref()
            .unwrap_or("(from a Rhai plugin)")
    }

    fn schema(&self) -> Vec<ArgSpec> {
        super::action::schema_from(&self.declared)
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let engine = engine_for(ctx);
        let ast = compile(&engine, &self.script).map_err(|message| ShlaneError::Action {
            action: self.name().to_string(),
            message: format!("plugin '{}': {message}", self.plugin),
        })?;

        // Only the declared arguments are passed in: a plugin should not be
        // able to read something the config did not mean for it.
        let mut arguments = Map::new();
        for spec in self.schema() {
            if let Some(value) = args.get(&spec.name) {
                arguments.insert(spec.name.clone().into(), Dynamic::from(value.to_string()));
            }
        }
        arguments.insert("dry_run".into(), Dynamic::from(ctx.dry_run));

        let mut scope = Scope::new();
        let returned: Dynamic = engine
            .call_fn(&mut scope, &ast, self.name(), (arguments,))
            .map_err(|err| ShlaneError::Action {
                action: self.name().to_string(),
                message: format!("plugin '{}': {err}", self.plugin),
            })?;

        outputs_from(returned).map_err(|message| ShlaneError::Action {
            action: self.name().to_string(),
            message: format!("plugin '{}': {message}", self.plugin),
        })
    }
}

/// A function may return a map of outputs, or nothing at all.
fn outputs_from(returned: Dynamic) -> std::result::Result<ActionOutput, String> {
    if returned.is_unit() {
        return Ok(ActionOutput::new());
    }

    let map = returned
        .try_cast::<Map>()
        .ok_or_else(|| "an action has to return a map of outputs, or nothing".to_string())?;

    let mut output = ActionOutput::new();
    for (key, value) in map {
        output = output.with(&key, value.to_string());
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_returned_is_no_outputs() {
        let output = outputs_from(Dynamic::UNIT).expect("valid");
        assert!(output.0.is_empty());
    }

    #[test]
    fn a_map_becomes_outputs() {
        let mut map = Map::new();
        map.insert("id".into(), Dynamic::from("msg-1".to_string()));
        map.insert("count".into(), Dynamic::from(3_i64));

        let output = outputs_from(Dynamic::from(map)).expect("valid");
        assert_eq!(output.0.get("id").map(String::as_str), Some("msg-1"));
        assert_eq!(output.0.get("count").map(String::as_str), Some("3"));
    }

    #[test]
    fn returning_something_else_is_reported() {
        let error =
            outputs_from(Dynamic::from("just a string".to_string())).expect_err("should fail");
        assert!(error.contains("map of outputs"), "{error}");
    }

    #[test]
    fn finds_the_functions_a_script_defines() {
        let engine = rhai::Engine::new();
        let ast = engine
            .compile("fn notify(args) { #{ ok: true } }\nfn helper() {}")
            .expect("compiles");
        let mut names = function_names(&ast);
        names.sort();
        assert_eq!(names, vec!["helper".to_string(), "notify".to_string()]);
    }
}
