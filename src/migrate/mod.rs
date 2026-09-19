//! Converting a Fastfile (`docs/plan/12-migration-from-fastlane.md`).
//!
//! Best-effort by construction: a Fastfile is Ruby, and Ruby can do anything.
//! What this understands is the shape almost every Fastfile actually has —
//! platforms, lanes, `desc`, action calls, `sh` — and everything it does not
//! understand is carried across as a comment with a note, rather than dropped.

pub mod mapping;
pub mod ruby;

use std::fmt::Write as _;

#[derive(Debug, Default)]
pub struct Migration {
    pub yaml: String,
    /// Things a person has to look at.
    pub notes: Vec<String>,
    pub lanes: usize,
    pub actions: usize,
    pub manual: usize,
}

struct Lane {
    name: String,
    description: Option<String>,
    platform: Option<String>,
    private: bool,
    steps: Vec<Step>,
}

enum Step {
    Run(String),
    Action {
        name: String,
        args: Vec<(String, String)>,
    },
    Manual(String),
}

pub fn convert(fastfile: &str) -> Migration {
    let mut migration = Migration::default();
    let mut lanes: Vec<Lane> = Vec::new();

    let mut platform: Option<String> = None;
    let mut description: Option<String> = None;
    let mut current: Option<Lane> = None;
    // Depth inside the lane, so a nested `do ... end` does not close it early.
    let mut depth = 0usize;

    for raw in fastfile.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some(name) = ruby::platform_block(line) {
            platform = Some(name);
            continue;
        }

        if let Some(text) = ruby::description(line) {
            description = Some(text);
            continue;
        }

        if let Some((name, private)) = ruby::lane_start(line) {
            if let Some(lane) = current.take() {
                // A lane that never closed: keep what was collected.
                migration
                    .notes
                    .push(format!("lane '{}' had no matching `end`", lane.name));
                lanes.push(lane);
            }
            current = Some(Lane {
                name,
                description: description.take(),
                platform: platform.clone(),
                private,
                steps: Vec::new(),
            });
            depth = 0;
            continue;
        }

        let Some(lane) = current.as_mut() else {
            // Outside any lane: before_all, after_all, error and plain Ruby.
            if ruby::opens_block(line) {
                migration.notes.push(format!(
                    "`{line}` is outside a lane; global hooks become before_all/after_all/error in shlane.yaml"
                ));
            }
            continue;
        };

        if line == "end" {
            match depth {
                0 => lanes.extend(current.take()),
                _ => depth -= 1,
            }
            continue;
        }

        if ruby::opens_block(line) {
            depth += 1;
            lane.steps.push(Step::Manual(line.to_string()));
            migration.manual += 1;
            // The block's contents become ordinary steps, which for a
            // conditional means they now always run. Saying so is the whole
            // point: a silently unconditional deploy is worse than no
            // conversion at all.
            migration.notes.push(format!(
                "lane '{}': `{line}` was not converted, and the steps inside it now run unconditionally -- add an `if:` to each, or a separate lane",
                lane.name
            ));
            continue;
        }

        lane.steps.push(statement(line, &mut migration));
    }

    if let Some(lane) = current.take() {
        migration
            .notes
            .push(format!("lane '{}' had no matching `end`", lane.name));
        lanes.push(lane);
    }

    // fastlane scopes a lane by platform (`fastlane ios test`); shlane has one
    // namespace, so a name used on more than one platform takes the platform.
    let mut uses: std::collections::BTreeMap<String, usize> = Default::default();
    for lane in &lanes {
        *uses.entry(lane.name.clone()).or_default() += 1;
    }
    for lane in &mut lanes {
        if let (Some(platform), Some(&count)) = (&lane.platform, uses.get(&lane.name)) {
            if count > 1 {
                let renamed = format!("{platform}_{}", lane.name);
                migration.notes.push(format!(
                    "`fastlane {platform} {}` is `shlane run {renamed}`: shlane lane names are not scoped by platform",
                    lane.name
                ));
                lane.name = renamed;
            }
        }
    }

    migration.lanes = lanes.len();
    migration.yaml = render(&lanes);
    migration
}

fn statement(line: &str, migration: &mut Migration) -> Step {
    if let Some(command) = ruby::sh_command(line) {
        return Step::Run(ruby::interpolate(&command));
    }

    let Some((name, args)) = ruby::action_call(line) else {
        migration.manual += 1;
        return Step::Manual(line.to_string());
    };

    if let Some(reason) = mapping::unsupported(&name) {
        migration.manual += 1;
        migration.notes.push(format!("`{name}`: {reason}"));
        return Step::Manual(line.to_string());
    }

    let Some(mapping) = mapping::lookup(&name) else {
        migration.manual += 1;
        migration.notes.push(format!(
            "`{name}` has no equivalent action; left as a comment"
        ));
        return Step::Manual(line.to_string());
    };

    let mut converted: Vec<(String, String)> = Vec::new();
    for (key, value) in args {
        let renamed = mapping
            .renames
            .iter()
            .find(|(from, _)| *from == key)
            .map(|(_, to)| (*to).to_string())
            .unwrap_or(key);
        let value = ruby::interpolate(&value);
        let value = if renamed == "destination" {
            simulator_destination(&value)
        } else {
            value
        };
        converted.push((renamed, value));
    }
    for (key, value) in mapping.add {
        converted.push(((*key).to_string(), (*value).to_string()));
    }

    // fastlane reads a lot from the Appfile and the environment; shlane asks
    // for it in the step. Anything required that the Fastfile did not say is
    // filled with a placeholder, so the result still validates and every gap is
    // visible in one place rather than appearing one failed run at a time.
    let registry = crate::actions::Registry::builtins();
    if let Some(action) = registry.find(mapping.shlane) {
        for spec in action.schema() {
            if !spec.required || converted.iter().any(|(key, _)| key == &spec.name) {
                continue;
            }
            migration.notes.push(format!(
                "`{name}` -> `{}`: fill in `{}` ({}); fastlane took it from the Appfile or the environment",
                mapping.shlane, spec.name, spec.description
            ));
            converted.push((spec.name.clone(), format!("TODO-{}", spec.name)));
        }
    }

    migration.actions += 1;
    Step::Action {
        name: mapping.shlane.to_string(),
        args: converted,
    }
}

/// scan's `devices: ["iPhone 16"]` names a simulator; xcodebuild wants a
/// destination. Only the first device carries over.
fn simulator_destination(value: &str) -> String {
    if value.contains("platform=") {
        return value.to_string();
    }
    let first = value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .next()
        .unwrap_or_default()
        .trim()
        .trim_matches(|c| c == '"' || c == '\'');
    format!("platform=iOS Simulator,name={first}")
}

fn render(lanes: &[Lane]) -> String {
    let mut out = String::from(
        "# Converted from a Fastfile by `shlane migrate`.\n\
         # Check every step before relying on it; lines marked TODO were not understood.\n\
         version: 1\n\nlanes:\n",
    );

    if lanes.is_empty() {
        out.push_str("  # no lanes were found\n");
        return out;
    }

    for lane in lanes {
        let _ = writeln!(out, "  {}:", lane.name);
        if let Some(description) = &lane.description {
            let _ = writeln!(out, "    description: {}", yaml_string(description));
        }
        if let Some(platform) = &lane.platform {
            let _ = writeln!(out, "    platform: {platform}");
        }
        if lane.private {
            let _ = writeln!(out, "    private: true");
        }

        if lane.steps.is_empty() {
            let _ = writeln!(out, "    steps: []");
            out.push('\n');
            continue;
        }

        let _ = writeln!(out, "    steps:");
        for step in &lane.steps {
            match step {
                Step::Run(command) => {
                    let _ = writeln!(out, "      - run: {}", yaml_string(command));
                }
                Step::Action { name, args } => {
                    let _ = writeln!(out, "      - action: {name}");
                    if !args.is_empty() {
                        let _ = writeln!(out, "        with:");
                        for (key, value) in args {
                            let _ = writeln!(out, "          {key}: {}", yaml_string(value));
                        }
                    }
                }
                Step::Manual(line) => {
                    let _ = writeln!(out, "      # TODO: migrate by hand: {line}");
                }
            }
        }
        out.push('\n');
    }

    out
}

/// Quote a value so YAML reads it back unchanged.
fn yaml_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FASTFILE: &str = r#"
default_platform(:ios)

platform :ios do
  desc "Push a new beta build to TestFlight"
  lane :beta do |options|
    ensure_git_status_clean
    increment_build_number
    gym(scheme: "MyApp", export_method: "app-store")
    pilot(skip_waiting_for_build_processing: true)
    slack(message: "Shipped #{options[:version]}", slack_url: ENV["SLACK_URL"])
  end

  private_lane :setup do
    sh "bundle install"
  end

  lane :release do
    match(type: "appstore")
    if ENV["CI"]
      sh "echo on ci"
    end
  end
end
"#;

    fn migrate() -> Migration {
        convert(FASTFILE)
    }

    #[test]
    fn finds_every_lane() {
        let migration = migrate();
        assert_eq!(migration.lanes, 3, "{}", migration.yaml);
        assert!(migration.yaml.contains("  beta:"), "{}", migration.yaml);
        assert!(migration.yaml.contains("  setup:"), "{}", migration.yaml);
        assert!(migration.yaml.contains("  release:"), "{}", migration.yaml);
    }

    #[test]
    fn a_lane_name_on_two_platforms_takes_the_platform() {
        let migration = convert(
            "platform :ios do\n  lane :test do\n    sh \"echo ios\"\n  end\nend\nplatform :android do\n  lane :test do\n    sh \"echo android\"\n  end\n  lane :deploy do\n    sh \"echo deploy\"\n  end\nend\n",
        );
        let yaml = &migration.yaml;
        assert!(yaml.contains("  ios_test:"), "{yaml}");
        assert!(yaml.contains("  android_test:"), "{yaml}");
        // A name used once keeps its name.
        assert!(yaml.contains("  deploy:"), "{yaml}");
        assert!(migration
            .notes
            .iter()
            .any(|note| note.contains("shlane run ios_test")));
    }

    #[test]
    fn scan_devices_become_a_simulator_destination() {
        let migration = convert(
            "lane :test do\n  scan(scheme: \"App\", devices: [\"iPhone 16\", \"iPad Air\"])\nend\n",
        );
        assert!(
            migration
                .yaml
                .contains("destination: \"platform=iOS Simulator,name=iPhone 16\""),
            "{}",
            migration.yaml
        );
        assert_eq!(
            simulator_destination("platform=iOS Simulator,name=iPhone 16"),
            "platform=iOS Simulator,name=iPhone 16"
        );
    }

    #[test]
    fn carries_over_description_platform_and_privacy() {
        let yaml = migrate().yaml;
        assert!(
            yaml.contains("description: \"Push a new beta build to TestFlight\""),
            "{yaml}"
        );
        assert!(yaml.contains("platform: ios"), "{yaml}");
        assert!(yaml.contains("private: true"), "{yaml}");
    }

    #[test]
    fn converts_actions_and_their_arguments() {
        let yaml = migrate().yaml;
        assert!(yaml.contains("- action: git_status_clean"), "{yaml}");
        assert!(yaml.contains("- action: build_ios"), "{yaml}");
        assert!(yaml.contains("scheme: \"MyApp\""), "{yaml}");
        assert!(yaml.contains("- action: testflight"), "{yaml}");
        // increment_build_number carries the part fastlane implied
        assert!(yaml.contains("- action: bump_version"), "{yaml}");
        assert!(yaml.contains("part: \"build\""), "{yaml}");
    }

    #[test]
    fn renames_arguments_that_changed() {
        let yaml = migrate().yaml;
        assert!(yaml.contains("- action: notify_slack"), "{yaml}");
        assert!(yaml.contains("text: "), "{yaml}");
        assert!(yaml.contains("webhook: \"${SLACK_URL}\""), "{yaml}");
    }

    #[test]
    fn translates_ruby_interpolation_into_shlane_references() {
        let yaml = migrate().yaml;
        assert!(yaml.contains("${version}"), "{yaml}");
    }

    #[test]
    fn sh_becomes_a_run_step() {
        let yaml = migrate().yaml;
        assert!(yaml.contains("- run: \"bundle install\""), "{yaml}");
    }

    #[test]
    fn required_arguments_fastlane_left_implicit_become_visible_placeholders() {
        let migration = convert("lane :beta do\n  pilot\nend\n");
        assert!(
            migration.yaml.contains("ipa: \"TODO-ipa\""),
            "{}",
            migration.yaml
        );
        assert!(
            migration
                .notes
                .iter()
                .any(|note| note.contains("fill in `ipa`")),
            "{:?}",
            migration.notes
        );

        // Still a config shlane accepts, so `validate` reports real problems
        // rather than refusing to read the file at all.
        let config =
            crate::config::loader::parse(&migration.yaml, std::path::Path::new("shlane.yaml"))
                .expect("parses");
        let problems =
            crate::config::validate::check(&config, &crate::actions::Registry::builtins());
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn what_it_cannot_do_is_reported_rather_than_dropped() {
        let migration = migrate();
        assert!(
            migration.notes.iter().any(|note| note.contains("match")),
            "{:?}",
            migration.notes
        );
        assert!(
            migration.yaml.contains("# TODO: migrate by hand: match"),
            "{}",
            migration.yaml
        );
        assert!(
            migration
                .yaml
                .contains("# TODO: migrate by hand: if ENV[\"CI\"]"),
            "{}",
            migration.yaml
        );
        assert!(migration.manual >= 2, "{}", migration.manual);
    }

    #[test]
    fn the_result_is_a_config_shlane_can_read() {
        let migration = migrate();
        let config =
            crate::config::loader::parse(&migration.yaml, std::path::Path::new("shlane.yaml"))
                .expect("the generated config should parse");
        assert_eq!(config.lanes.len(), 3);
    }

    #[test]
    fn a_nested_block_does_not_close_the_lane_early() {
        let migration = convert(
            "lane :a do\n  [1,2].each do |n|\n    sh \"echo\"\n  end\n  sh \"after\"\nend\nlane :b do\n  sh \"b\"\nend\n",
        );
        assert_eq!(migration.lanes, 2, "{}", migration.yaml);
        assert!(
            migration.yaml.contains("- run: \"after\""),
            "{}",
            migration.yaml
        );
    }

    #[test]
    fn an_empty_fastfile_produces_a_valid_config() {
        let migration = convert("");
        assert_eq!(migration.lanes, 0);
        assert!(
            migration.yaml.contains("no lanes were found"),
            "{}",
            migration.yaml
        );
    }
}
