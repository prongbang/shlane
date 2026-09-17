//! The action system (`docs/plan/06-actions-core.md`).
//!
//! An action is a named, self-describing step: it declares the arguments it
//! takes, so `shlane validate` can check a config without running it, and it
//! reports what it produced, so later steps can use it.

pub mod context;
pub mod core;

use crate::error::Result;
use context::ActionContext;
use std::collections::BTreeMap;

/// One argument of an action.
pub struct ArgSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub required: bool,
    pub default: Option<&'static str>,
    /// Registered as a secret, so its value never reaches the output.
    pub sensitive: bool,
}

impl ArgSpec {
    pub const fn new(name: &'static str, description: &'static str) -> Self {
        Self {
            name,
            description,
            required: false,
            default: None,
            sensitive: false,
        }
    }

    pub const fn required(mut self) -> Self {
        self.required = true;
        self
    }

    pub const fn default(mut self, value: &'static str) -> Self {
        self.default = Some(value);
        self
    }

    pub const fn sensitive(mut self) -> Self {
        self.sensitive = true;
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
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn schema(&self) -> Vec<ArgSpec>;
    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput>;
}

/// Every built-in action, in listing order.
pub fn all() -> Vec<Box<dyn Action>> {
    core::all()
}

pub fn find(name: &str) -> Option<Box<dyn Action>> {
    all().into_iter().find(|action| action.name() == name)
}

pub fn names() -> Vec<&'static str> {
    all().iter().map(|action| action.name()).collect()
}

/// Check a step's arguments against an action's schema.
///
/// Returns every problem, so `shlane validate` can report them all at once.
pub fn check_args(action: &dyn Action, provided: &BTreeMap<String, String>) -> Vec<String> {
    let schema = action.schema();
    let mut problems = Vec::new();

    for spec in &schema {
        if spec.required && !provided.contains_key(spec.name) {
            problems.push(format!(
                "action '{}' needs '{}' ({})",
                action.name(),
                spec.name,
                spec.description
            ));
        }
    }

    for name in provided.keys() {
        if !schema.iter().any(|spec| spec.name == name) {
            let known: Vec<&str> = schema.iter().map(|spec| spec.name).collect();
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

    problems
}

/// Fill in defaults the step did not give.
pub fn with_defaults(action: &dyn Action, provided: &BTreeMap<String, String>) -> Args {
    let mut values = provided.clone();
    for spec in action.schema() {
        if let Some(default) = spec.default {
            values
                .entry(spec.name.to_string())
                .or_insert_with(|| default.to_string());
        }
    }
    Args::new(values)
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
        let action = find("git_commit").expect("git_commit should exist");
        let problems = check_args(action.as_ref(), &args(&[]));
        assert!(
            problems.iter().any(|p| p.contains("message")),
            "{problems:?}"
        );
    }

    #[test]
    fn unknown_arguments_are_reported() {
        let action = find("git_commit").expect("git_commit should exist");
        let problems = check_args(action.as_ref(), &args(&[("message", "x"), ("mesage", "y")]));
        assert!(
            problems.iter().any(|p| p.contains("mesage")),
            "{problems:?}"
        );
    }

    #[test]
    fn defaults_are_filled_in() {
        let action = find("git_push").expect("git_push should exist");
        let filled = with_defaults(action.as_ref(), &args(&[]));
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
}
