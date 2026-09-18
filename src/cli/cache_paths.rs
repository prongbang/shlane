//! `shlane cache-paths` (`docs/plan/11-ci-integration.md`).
//!
//! shlane does not cache anything itself -- a tool that manages a CI's cache
//! is a tool that gets it wrong on somebody else's CI -- but it knows what the
//! config is going to download, so it can say what is worth keeping.

use crate::config::model::{Config, StepKind};
use std::collections::BTreeSet;

/// A path worth caching, and what puts something there.
pub struct Suggestion {
    pub path: &'static str,
    pub why: &'static str,
}

/// What the actions and commands in this config are going to download.
///
/// Derived from the config rather than printed as a fixed list: suggesting a
/// Gradle cache to a project that has no Gradle in it teaches people to ignore
/// the output.
pub fn suggest(config: &Config) -> Vec<Suggestion> {
    let mut actions = BTreeSet::new();
    let mut commands = String::new();

    let mut collect = |steps: &[crate::config::model::Step]| {
        for step in steps {
            match &step.kind {
                StepKind::Action { name, .. } => {
                    actions.insert(name.clone());
                }
                StepKind::Run(command) => {
                    commands.push_str(command);
                    commands.push('\n');
                }
                StepKind::Script(source) => {
                    commands.push_str(source);
                    commands.push('\n');
                }
                StepKind::Lane { .. } => {}
            }
        }
    };

    collect(&config.before_all);
    collect(&config.after_all);
    collect(&config.error);
    for lane in config.lanes.values() {
        collect(&lane.before);
        collect(&lane.steps);
        collect(&lane.after);
    }

    let uses = |names: &[&str]| {
        names.iter().any(|name| {
            actions.contains(*name)
                || commands.contains(&format!("{name} "))
                || commands.contains(&format!("{name}\n"))
        })
    };

    let mut out = Vec::new();

    if uses(&["gradle", "build_android", "test_android", "sign_android"])
        || commands.contains("gradlew")
    {
        out.push(Suggestion {
            path: "~/.gradle/caches",
            why: "Gradle's dependency cache",
        });
        out.push(Suggestion {
            path: "~/.gradle/wrapper",
            why: "the Gradle distribution the wrapper downloads",
        });
    }

    if uses(&["build_ios", "test_ios"]) || commands.contains("xcodebuild") {
        out.push(Suggestion {
            path: "~/Library/Developer/Xcode/DerivedData",
            why: "Xcode's build products and module cache",
        });
        out.push(Suggestion {
            path: "~/Library/Caches/org.swift.swiftpm",
            why: "Swift packages Xcode resolved",
        });
    }

    if commands.contains("swift build") || commands.contains("swift test") {
        out.push(Suggestion {
            path: ".build",
            why: "SwiftPM's build directory",
        });
    }

    if commands.contains("npm ") || commands.contains("npx ") || commands.contains("yarn ") {
        out.push(Suggestion {
            path: "~/.npm",
            why: "the npm cache",
        });
    }

    if commands.contains("pod ") {
        out.push(Suggestion {
            path: "Pods",
            why: "the pods CocoaPods installed",
        });
    }

    if !config.plugins.is_empty() {
        out.push(Suggestion {
            path: ".shlane/plugins",
            why: "the plugins this config declares",
        });
    }

    out
}

pub fn print(config: &Config, json: bool) {
    let suggestions = suggest(config);

    if json {
        let paths: Vec<String> = suggestions
            .iter()
            .map(|s| format!("\"{}\"", s.path))
            .collect();
        println!("[{}]", paths.join(","));
        return;
    }

    if suggestions.is_empty() {
        println!("Nothing in this config downloads anything worth caching.");
        return;
    }

    let width = suggestions
        .iter()
        .map(|s| s.path.len())
        .max()
        .unwrap_or_default();
    for suggestion in &suggestions {
        println!("{:<width$}  {}", suggestion.path, suggestion.why);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> Config {
        serde_yaml::from_str(yaml).expect("fixture should parse")
    }

    #[test]
    fn suggests_nothing_for_a_config_that_downloads_nothing() {
        let config = parse("lanes:\n  hello:\n    steps:\n      - run: echo hi\n");
        assert!(suggest(&config).is_empty());
    }

    #[test]
    fn finds_gradle_through_an_action_and_through_a_command() {
        let by_action = parse(
            "lanes:\n  build:\n    steps:\n      - action: build_android\n        with:\n          format: apk\n",
        );
        assert!(suggest(&by_action)
            .iter()
            .any(|s| s.path == "~/.gradle/caches"));

        let by_command = parse("lanes:\n  build:\n    steps:\n      - run: ./gradlew assemble\n");
        assert!(suggest(&by_command)
            .iter()
            .any(|s| s.path == "~/.gradle/caches"));
    }

    #[test]
    fn suggests_the_plugin_directory_only_when_plugins_are_declared() {
        let without = parse("lanes:\n  hello:\n    steps:\n      - run: echo hi\n");
        assert!(!suggest(&without)
            .iter()
            .any(|s| s.path == ".shlane/plugins"));

        let with = parse(
            "plugins:\n  - name: demo\n    path: ./plugins/demo\nlanes:\n  hello:\n    steps:\n      - run: echo hi\n",
        );
        assert!(suggest(&with).iter().any(|s| s.path == ".shlane/plugins"));
    }

    #[test]
    fn looks_inside_hooks_and_lanes_alike() {
        let config = parse(
            "before_all:\n  - run: xcodebuild -version\nlanes:\n  hello:\n    steps:\n      - run: echo hi\n",
        );
        assert!(suggest(&config)
            .iter()
            .any(|s| s.path.contains("DerivedData")));
    }
}
