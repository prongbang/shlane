//! `sh` and `ensure_env_vars`.

use crate::actions::context::ActionContext;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::Result;

/// The escape hatch: whatever an action does not cover yet.
pub struct Sh;

impl Action for Sh {
    fn name(&self) -> &'static str {
        "sh"
    }

    fn description(&self) -> &'static str {
        "Run a shell command"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::new("command", "The command to run").required()]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let command = args.get_or("command", "");
        let outcome = ctx.require(command)?;
        Ok(ActionOutput::new()
            .with("stdout", outcome.stdout.trim_end())
            .with("stderr", outcome.stderr.trim_end())
            .with("code", outcome.code.unwrap_or(-1).to_string()))
    }
}

/// Fail before the work starts, rather than three minutes into a build.
pub struct EnsureEnvVars;

impl Action for EnsureEnvVars {
    fn name(&self) -> &'static str {
        "ensure_env_vars"
    }

    fn description(&self) -> &'static str {
        "Check that environment variables are set before going further"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::new("names", "Comma-separated variable names").required()]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let names: Vec<&str> = args
            .get_or("names", "")
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .collect();

        let missing: Vec<&str> = names
            .iter()
            .copied()
            .filter(|name| ctx.env.get(*name).is_none_or(String::is_empty))
            .collect();

        if !missing.is_empty() {
            return Err(ctx.error(
                self.name(),
                format!(
                    "these environment variables are not set: {}",
                    missing.join(", ")
                ),
            ));
        }

        ctx.ui
            .detail(&format!("{} variable(s) present", names.len()));
        Ok(ActionOutput::new().with("checked", names.len().to_string()))
    }
}
