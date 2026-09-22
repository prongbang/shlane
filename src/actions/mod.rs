//! The action system (`docs/plan/06-actions-core.md`).
//!
//! An action is a named, self-describing step: it declares the arguments it
//! takes, so `shlane validate` can check a config without running it, and it
//! reports what it produced, so later steps can use it.

pub mod asc;
pub mod codesign;
pub mod context;
pub mod core;
pub mod google;

use crate::error::Result;
use context::ActionContext;
use std::collections::BTreeMap;

/// One argument of an action.
///
/// Owned rather than `&'static str` so a plugin can describe its own arguments
/// at load time (`docs/plan/09-plugins.md`).
#[derive(Debug, Clone)]
pub struct ArgSpec {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub default: Option<String>,
    /// Registered as a secret, so its value never reaches the output.
    pub sensitive: bool,
    /// This argument is itself a shell command, so a `${...}` substituted into
    /// it is escaped the way one in a `run:` step is. Without this, a value
    /// carrying a space or a `;` would be re-read by the shell the action
    /// hands it to.
    pub shell: bool,
}

impl ArgSpec {
    pub fn new(name: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            required: false,
            default: None,
            sensitive: false,
            shell: false,
        }
    }

    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub fn default(mut self, value: impl Into<String>) -> Self {
        self.default = Some(value.into());
        self
    }

    pub fn sensitive(mut self) -> Self {
        self.sensitive = true;
        self
    }

    /// Mark an argument that is run as a shell command.
    pub fn shell(mut self) -> Self {
        self.shell = true;
        self
    }
}

/// What a step passed to an action, after `${...}` substitution.
#[derive(Debug, Default, Clone)]
pub struct Args {
    values: BTreeMap<String, String>,
}

impl Args {
    pub fn new(values: BTreeMap<String, String>) -> Self {
        Self { values }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    pub fn get_or<'a>(&'a self, name: &str, fallback: &'a str) -> &'a str {
        self.get(name).unwrap_or(fallback)
    }

    /// True for `true`, `yes` and `1`.
    pub fn flag(&self, name: &str) -> bool {
        matches!(self.get(name), Some("true" | "yes" | "1"))
    }
}

/// What an action produced, stored under the step's `id`.
#[derive(Debug, Default)]
pub struct ActionOutput(pub BTreeMap<String, String>);

impl ActionOutput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with(mut self, key: &str, value: impl Into<String>) -> Self {
        self.0.insert(key.to_string(), value.into());
        self
    }
}

pub trait Action {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn schema(&self) -> Vec<ArgSpec>;
    /// Extra validation for relationships between action arguments.
    fn validate_args(&self, _provided: &BTreeMap<String, String>) -> Vec<String> {
        Vec::new()
    }
    /// Arguments a migration must fill so its generated config validates.
    fn migration_required_args(&self) -> Vec<ArgSpec> {
        self.schema()
            .into_iter()
            .filter(|spec| spec.required)
            .collect()
    }
    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput>;
}

/// Every built-in action, in listing order.
pub fn all() -> Vec<Box<dyn Action>> {
    core::all()
}

/// Check a step's arguments against an action's schema.
///
/// Returns every problem, so `shlane validate` can report them all at once.
pub fn check_args(action: &dyn Action, provided: &BTreeMap<String, String>) -> Vec<String> {
    let schema = action.schema();
    let mut problems = Vec::new();

    for spec in &schema {
        if spec.required && !provided.contains_key(&spec.name) {
            problems.push(format!(
                "action '{}' needs '{}' ({})",
                action.name(),
                spec.name,
                spec.description
            ));
        }
    }

    for name in provided.keys() {
        if !schema.iter().any(|spec| &spec.name == name) {
            let known: Vec<&str> = schema.iter().map(|spec| spec.name.as_str()).collect();
            problems.push(format!(
                "action '{}' has no argument '{name}' (it takes: {})",
                action.name(),
                if known.is_empty() {
                    "none".to_string()
                } else {
                    known.join(", ")
                }
            ));
        }
    }

    problems.extend(action.validate_args(provided));

    problems
}

/// Fill in defaults the step did not give.
pub fn with_defaults(action: &dyn Action, provided: &BTreeMap<String, String>) -> Args {
    let mut values = provided.clone();
    for spec in action.schema() {
        if let Some(default) = spec.default {
            values.entry(spec.name).or_insert(default);
        }
    }
    Args::new(values)
}

/// Every action available to a run: the built-ins, plus whatever the config's
/// `plugins:` brought in.
pub struct Registry {
    actions: Vec<Box<dyn Action>>,
}

impl Registry {
    pub fn builtins() -> Self {
        Self { actions: all() }
    }

    pub fn with_plugins(mut self, plugins: Vec<Box<dyn Action>>) -> Self {
        self.actions.extend(plugins);
        self
    }

    pub fn find(&self, name: &str) -> Option<&dyn Action> {
        self.actions
            .iter()
            .find(|action| action.name() == name)
            .map(AsRef::as_ref)
    }

    pub fn names(&self) -> Vec<&str> {
        self.actions.iter().map(|action| action.name()).collect()
    }

    pub fn iter(&self) -> impl Iterator<Item = &dyn Action> {
        self.actions.iter().map(AsRef::as_ref)
    }

    pub fn len(&self) -> usize {
        self.actions.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn every_action_has_a_unique_name_and_a_description() {
        let actions = all();
        let mut seen = std::collections::BTreeSet::new();
        for action in &actions {
            assert!(
                seen.insert(action.name()),
                "duplicate action name: {}",
                action.name()
            );
            assert!(
                !action.description().is_empty(),
                "{} has no description",
                action.name()
            );
            for spec in action.schema() {
                assert!(
                    !spec.description.is_empty(),
                    "{}.{} has no description",
                    action.name(),
                    spec.name
                );
                assert!(
                    !(spec.required && spec.default.is_some()),
                    "{}.{} is required and also has a default",
                    action.name(),
                    spec.name
                );
            }
        }
        assert!(actions.len() >= 10, "expected the P0 set to be registered");
    }

    #[test]
    fn missing_required_arguments_are_reported() {
        let registry = Registry::builtins();
        let action = registry
            .find("git_commit")
            .expect("git_commit should exist");
        let problems = check_args(action, &args(&[]));
        assert!(
            problems.iter().any(|p| p.contains("message")),
            "{problems:?}"
        );
    }

    #[test]
    fn unknown_arguments_are_reported() {
        let registry = Registry::builtins();
        let action = registry
            .find("git_commit")
            .expect("git_commit should exist");
        let problems = check_args(action, &args(&[("message", "x"), ("mesage", "y")]));
        assert!(
            problems.iter().any(|p| p.contains("mesage")),
            "{problems:?}"
        );
    }

    #[test]
    fn defaults_are_filled_in() {
        let registry = Registry::builtins();
        let action = registry.find("git_push").expect("git_push should exist");
        let filled = with_defaults(action, &args(&[]));
        assert_eq!(filled.get("remote"), Some("origin"));
    }

    #[test]
    fn flags_accept_the_usual_spellings() {
        let args = Args::new(args(&[("a", "true"), ("b", "1"), ("c", "no")]));
        assert!(args.flag("a"));
        assert!(args.flag("b"));
        assert!(!args.flag("c"));
        assert!(!args.flag("missing"));
    }

    struct ValidatedAction;

    impl Action for ValidatedAction {
        fn name(&self) -> &'static str {
            "validated"
        }

        fn description(&self) -> &'static str {
            "A test action with cross-field validation"
        }

        fn schema(&self) -> Vec<ArgSpec> {
            vec![ArgSpec::new("credential", "A credential")]
        }

        fn validate_args(&self, provided: &BTreeMap<String, String>) -> Vec<String> {
            if provided.contains_key("credential") {
                Vec::new()
            } else {
                vec!["needs credential".to_string()]
            }
        }

        fn run(&self, _ctx: &mut ActionContext<'_>, _args: &Args) -> Result<ActionOutput> {
            Ok(ActionOutput::new())
        }
    }

    #[test]
    fn actions_can_add_cross_field_validation_errors() {
        let problems = check_args(&ValidatedAction, &args(&[]));
        assert_eq!(problems, ["needs credential"]);
    }
}
