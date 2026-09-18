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

/// Fail at the start of a lane rather than three steps in, when the binary a
/// later step needs turns out not to be installed.
pub struct WhichTool;

impl Action for WhichTool {
    fn name(&self) -> &'static str {
        "which_tool"
    }

    fn description(&self) -> &'static str {
        "Check a binary is installed, and optionally new enough"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("name", "The binary to look for").required(),
            ArgSpec::new(
                "min_version",
                "Fail when the version found is older than this, e.g. 8.0",
            ),
            ArgSpec::new("version_arg", "How to ask it for its version").default("--version"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let name = args.get_or("name", "");

        // `probe`, not `sh`: under --dry-run the answer to "is it installed"
        // has to be the real one, or the dry run reports a problem that does
        // not exist and hides one that does.
        let found = ctx.probe(&format!("command -v {name}"))?;
        if !found.success {
            return Err(ctx.error(
                self.name(),
                format!("'{name}' is not installed, or not on PATH"),
            ));
        }
        let path = found.stdout.trim().to_string();

        let Some(minimum) = args.get("min_version").filter(|value| !value.is_empty()) else {
            ctx.ui.detail(&format!("{name} is at {path}"));
            return Ok(ActionOutput::new().with("path", path));
        };

        let reported = ctx.probe(&format!(
            "{name} {}",
            args.get_or("version_arg", "--version")
        ))?;
        let text = format!("{}{}", reported.stdout, reported.stderr);
        let Some(version) = first_version(&text) else {
            return Err(ctx.error(
                self.name(),
                format!("could not read a version out of `{name} --version`"),
            ));
        };

        if compare_versions(&version, minimum) == std::cmp::Ordering::Less {
            return Err(ctx.error(
                self.name(),
                format!("{name} is {version}, but this lane needs {minimum} or newer"),
            ));
        }

        ctx.ui.detail(&format!("{name} {version} at {path}"));
        Ok(ActionOutput::new()
            .with("path", path)
            .with("version", version))
    }
}

/// The first dotted number in a `--version` banner, which is where every tool
/// puts it even though no two agree on the rest of the line.
fn first_version(text: &str) -> Option<String> {
    let mut current = String::new();
    for character in text.chars() {
        if character.is_ascii_digit() || (character == '.' && !current.is_empty()) {
            current.push(character);
            continue;
        }
        if current.contains('.') {
            return Some(current.trim_end_matches('.').to_string());
        }
        current.clear();
    }
    if current.contains('.') {
        return Some(current.trim_end_matches('.').to_string());
    }
    None
}

/// Compare two dotted versions numerically, so 10.0 is newer than 9.0.
fn compare_versions(left: &str, right: &str) -> std::cmp::Ordering {
    let parse = |value: &str| -> Vec<u64> {
        value
            .split('.')
            .map(|part| part.parse::<u64>().unwrap_or_default())
            .collect()
    };
    let (left, right) = (parse(left), parse(right));
    for index in 0..left.len().max(right.len()) {
        let ordering = left
            .get(index)
            .copied()
            .unwrap_or_default()
            .cmp(&right.get(index).copied().unwrap_or_default());
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }
    std::cmp::Ordering::Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_version_in_a_banner() {
        assert_eq!(first_version("Gradle 8.5"), Some("8.5".to_string()));
        assert_eq!(
            first_version("git version 2.43.0"),
            Some("2.43.0".to_string())
        );
        assert_eq!(
            first_version("xcodebuild -version\nXcode 16.1\nBuild version 16B40"),
            Some("16.1".to_string())
        );
    }

    #[test]
    fn reports_no_version_when_there_is_none() {
        assert_eq!(first_version("a tool with no version"), None);
        // A bare integer is not a version: "Java 21" would otherwise pass a
        // 21.0 check by accident.
        assert_eq!(first_version("tool 21"), None);
    }

    #[test]
    fn compares_numerically_not_alphabetically() {
        use std::cmp::Ordering;
        assert_eq!(compare_versions("10.0", "9.0"), Ordering::Greater);
        assert_eq!(compare_versions("8.5", "8.5.0"), Ordering::Equal);
        assert_eq!(compare_versions("8.4.9", "8.5"), Ordering::Less);
    }
}
