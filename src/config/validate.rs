//! Checks that can be made without running anything.

use super::model::{Config, Step, StepKind, SUPPORTED_VERSION};
use std::collections::BTreeSet;
use std::fmt::Write as _;

/// Validate a config, collecting every problem rather than stopping at the first.
pub fn check(config: &Config, registry: &crate::actions::Registry) -> Vec<String> {
    let mut problems = Vec::new();

    if let Some(version) = config.version {
        if version != SUPPORTED_VERSION {
            problems.push(format!(
                "version {version} is not supported by shlane {} (this build understands version {SUPPORTED_VERSION})",
                env!("CARGO_PKG_VERSION")
            ));
        }
    }

    if let Some(required) = &config.min_shlane {
        match compare_versions(required, env!("CARGO_PKG_VERSION")) {
            Ok(true) => problems.push(format!(
                "this config needs shlane {required} or newer; this is {}",
                env!("CARGO_PKG_VERSION")
            )),
            Ok(false) => {}
            Err(message) => problems.push(format!("min_shlane: {message}")),
        }
    }

    for (lane_name, lane) in &config.lanes {
        for (param_name, spec) in &lane.params {
            if let Some(default) = &spec.default {
                if let Err(message) = spec.param_type.check(default) {
                    problems.push(format!(
                        "lane '{lane_name}': default for parameter '{param_name}' {message}"
                    ));
                }
                if let Some(values) = &spec.values {
                    if !values.contains(default) {
                        problems.push(format!(
                            "lane '{lane_name}': default '{default}' for parameter '{param_name}' is not one of {}",
                            values.join(", ")
                        ));
                    }
                }
            }
            if spec.required && spec.default.is_some() {
                problems.push(format!(
                    "lane '{lane_name}': parameter '{param_name}' is required, so its default can never be used"
                ));
            }
        }

        let mut ids = BTreeSet::new();
        for step in lane_steps(lane) {
            if let Some(id) = &step.id {
                if !ids.insert(id.clone()) {
                    problems.push(format!("lane '{lane_name}': duplicate step id '{id}'"));
                }
            }
            check_step(config, registry, lane_name, step, &mut problems);
        }
    }

    for (label, steps) in [
        ("before_all", &config.before_all),
        ("after_all", &config.after_all),
        ("error", &config.error),
    ] {
        for step in steps {
            check_step(config, registry, label, step, &mut problems);
        }
    }

    problems.extend(find_cycles(config));
    problems
}

fn lane_steps(lane: &super::model::Lane) -> impl Iterator<Item = &Step> {
    lane.before
        .iter()
        .chain(lane.steps.iter())
        .chain(lane.after.iter())
}

fn check_step(
    config: &Config,
    registry: &crate::actions::Registry,
    owner: &str,
    step: &Step,
    problems: &mut Vec<String>,
) {
    match &step.kind {
        StepKind::Lane { name, .. } => {
            if !config.lanes.contains_key(name) {
                let mut message = format!("'{owner}' calls lane '{name}', which does not exist");
                let available = config.lane_names();
                if !available.is_empty() {
                    let _ = write!(message, " (lanes: {})", available.join(", "));
                }
                problems.push(message);
            }
        }
        StepKind::Action { name, with } => match registry.find(name) {
            Some(action) => {
                for problem in crate::actions::check_args(action, with) {
                    problems.push(format!("'{owner}': {problem}"));
                }
            }
            None => {
                let mut message = format!("'{owner}' uses action '{name}', which does not exist");
                let known = registry.names();
                if !known.is_empty() {
                    let _ = write!(message, " (try: {})", known.join(", "));
                }
                problems.push(message);
            }
        },
        StepKind::Run(_) | StepKind::Script(_) => {}
    }
}

/// Depth-first search for `lane:` steps that can reach themselves.
fn find_cycles(config: &Config) -> Vec<String> {
    let mut problems = Vec::new();
    let mut settled = BTreeSet::new();

    for start in config.lanes.keys() {
        let mut path = Vec::new();
        let mut on_path = BTreeSet::new();
        visit(
            config,
            start,
            &mut path,
            &mut on_path,
            &mut settled,
            &mut problems,
        );
    }

    problems.sort();
    problems.dedup();
    problems
}

fn visit(
    config: &Config,
    lane_name: &str,
    path: &mut Vec<String>,
    on_path: &mut BTreeSet<String>,
    settled: &mut BTreeSet<String>,
    problems: &mut Vec<String>,
) {
    if on_path.contains(lane_name) {
        let start = path
            .iter()
            .position(|name| name == lane_name)
            .unwrap_or_default();
        let mut cycle = path[start..].to_vec();
        cycle.push(lane_name.to_string());
        problems.push(format!(
            "lanes call each other in a loop: {}",
            cycle.join(" -> ")
        ));
        return;
    }
    if settled.contains(lane_name) {
        return;
    }

    let Some(lane) = config.lanes.get(lane_name) else {
        return;
    };

    path.push(lane_name.to_string());
    on_path.insert(lane_name.to_string());

    for step in lane_steps(lane) {
        if let StepKind::Lane { name, .. } = &step.kind {
            visit(config, name, path, on_path, settled, problems);
        }
    }

    on_path.remove(lane_name);
    path.pop();
    settled.insert(lane_name.to_string());
}

/// True when `required` is newer than `current`.
fn compare_versions(required: &str, current: &str) -> Result<bool, String> {
    let parse = |text: &str| -> Result<Vec<u64>, String> {
        text.trim()
            .split('.')
            .map(|part| {
                part.split(['-', '+'])
                    .next()
                    .unwrap_or(part)
                    .parse::<u64>()
                    .map_err(|_| format!("'{text}' is not a version like 1.2.3"))
            })
            .collect()
    };

    let required = parse(required)?;
    let current = parse(current)?;
    let width = required.len().max(current.len());

    for index in 0..width {
        let left = required.get(index).copied().unwrap_or(0);
        let right = current.get(index).copied().unwrap_or(0);
        if left != right {
            return Ok(left > right);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn config(yaml: &str) -> Config {
        crate::config::loader::parse(yaml, Path::new("shlane.yaml")).expect("should parse")
    }

    fn check(config: &Config) -> Vec<String> {
        super::check(config, &crate::actions::Registry::builtins())
    }

    #[test]
    fn a_plain_config_has_no_problems() {
        let problems = check(&config(
            "lanes:\n  build:\n    steps:\n      - run: \"true\"\n",
        ));
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn missing_lane_references_are_reported() {
        let problems = check(&config(
            "lanes:\n  build:\n    steps:\n      - lane: nope\n",
        ));
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("does not exist"), "{problems:?}");
    }

    #[test]
    fn direct_recursion_is_reported() {
        let problems = check(&config(
            "lanes:\n  build:\n    steps:\n      - lane: build\n",
        ));
        assert!(problems.iter().any(|p| p.contains("loop")), "{problems:?}");
    }

    #[test]
    fn indirect_recursion_is_reported() {
        let problems = check(&config(
            "lanes:\n  a:\n    steps:\n      - lane: b\n  b:\n    steps:\n      - lane: a\n",
        ));
        assert!(problems.iter().any(|p| p.contains("loop")), "{problems:?}");
    }

    #[test]
    fn a_diamond_is_not_a_cycle() {
        let problems = check(&config(
            "lanes:\n  a:\n    steps:\n      - lane: b\n      - lane: c\n  b:\n    steps:\n      - lane: d\n  c:\n    steps:\n      - lane: d\n  d: {}\n",
        ));
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn param_defaults_are_type_checked() {
        let problems = check(&config(
            "lanes:\n  a:\n    params:\n      count:\n        type: int\n        default: many\n",
        ));
        assert!(
            problems.iter().any(|p| p.contains("integer")),
            "{problems:?}"
        );
    }

    #[test]
    fn defaults_must_be_one_of_values() {
        let problems = check(&config(
            "lanes:\n  a:\n    params:\n      target:\n        values: [staging, production]\n        default: dev\n",
        ));
        assert!(
            problems.iter().any(|p| p.contains("not one of")),
            "{problems:?}"
        );
    }

    #[test]
    fn duplicate_step_ids_are_reported() {
        let problems = check(&config(
            "lanes:\n  a:\n    steps:\n      - run: \"true\"\n        id: x\n      - run: \"true\"\n        id: x\n",
        ));
        assert!(
            problems.iter().any(|p| p.contains("duplicate step id")),
            "{problems:?}"
        );
    }

    #[test]
    fn unsupported_versions_are_reported() {
        let problems = check(&config("version: 99\nlanes: {}\n"));
        assert!(
            problems.iter().any(|p| p.contains("not supported")),
            "{problems:?}"
        );
    }

    #[test]
    fn compares_versions_numerically() {
        assert_eq!(compare_versions("0.2.0", "0.1.0"), Ok(true));
        assert_eq!(compare_versions("0.1.0", "0.1.0"), Ok(false));
        assert_eq!(compare_versions("0.1", "0.1.0"), Ok(false));
        assert_eq!(compare_versions("1.0.0", "0.9.9"), Ok(true));
        assert_eq!(compare_versions("0.10.0", "0.9.0"), Ok(true));
        assert!(compare_versions("next", "0.1.0").is_err());
    }
}
