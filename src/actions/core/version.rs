//! Reading and bumping a project's version.
//!
//! One action that understands the usual files, rather than one per ecosystem
//! as fastlane has (`increment_version_number`, `increment_version_code`, ...).

use crate::actions::context::ActionContext;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Files that are checked, in order, when no file is given.
const CANDIDATES: [&str; 4] = ["Cargo.toml", "package.json", "pubspec.yaml", "VERSION"];

fn locate(ctx: &ActionContext<'_>, given: Option<&str>) -> Option<PathBuf> {
    if let Some(given) = given {
        let path = ctx.workdir().join(given);
        return path.is_file().then_some(path);
    }
    CANDIDATES
        .iter()
        .map(|name| ctx.workdir().join(name))
        .find(|path| path.is_file())
}

/// Pull the version out of a file, without a parser per format: every one of
/// these writes it as `version` followed by a quoted or bare value.
fn read_version(path: &Path, text: &str) -> Option<String> {
    let name = path.file_name()?.to_string_lossy().to_string();

    if name == "VERSION" {
        let value = text.trim();
        return (!value.is_empty()).then(|| value.to_string());
    }

    for line in text.lines() {
        let line = line.trim();
        // `?` would abandon the whole file on the first line that does not
        // match, so every step here has to keep the loop going instead.
        let rest = match name.as_str() {
            "package.json" => line
                .strip_prefix("\"version\"")
                .and_then(|rest| rest.trim_start().strip_prefix(':')),
            _ => line.strip_prefix("version").and_then(|rest| {
                let rest = rest.trim_start();
                rest.strip_prefix('=').or_else(|| rest.strip_prefix(':'))
            }),
        };
        let Some(rest) = rest else { continue };

        let value = rest
            .trim()
            .trim_end_matches(',')
            .trim()
            .trim_matches(['"', '\''])
            .trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    None
}

/// Increase one part of a semantic version.
fn bump(version: &str, part: &str) -> std::result::Result<String, String> {
    // Keep anything after the numbers (Flutter's `1.2.3+45`, a `-beta` suffix)
    // out of the arithmetic, then decide what to do with it.
    let (numbers, suffix) = match version.find(['-', '+']) {
        Some(at) => (&version[..at], &version[at..]),
        None => (version, ""),
    };

    let mut parts: Vec<u64> = Vec::new();
    for piece in numbers.split('.') {
        parts.push(
            piece
                .parse()
                .map_err(|_| format!("'{version}' is not a version like 1.2.3"))?,
        );
    }
    while parts.len() < 3 {
        parts.push(0);
    }

    match part {
        "major" => {
            parts[0] += 1;
            parts[1] = 0;
            parts[2] = 0;
        }
        "minor" => {
            parts[1] += 1;
            parts[2] = 0;
        }
        "patch" => parts[2] += 1,
        "build" => {
            // Flutter-style `1.2.3+45`: only the build number moves.
            let build: u64 = suffix
                .strip_prefix('+')
                .ok_or_else(|| format!("'{version}' has no build number to increase"))?
                .parse()
                .map_err(|_| format!("'{version}' has a build number that is not a number"))?;
            return Ok(format!(
                "{}.{}.{}+{}",
                parts[0],
                parts[1],
                parts[2],
                build + 1
            ));
        }
        other => {
            return Err(format!(
                "unknown part '{other}'; use major, minor, patch or build"
            ))
        }
    }

    // A pre-release suffix is not carried over: 1.2.3-beta bumped is 1.2.4.
    Ok(format!("{}.{}.{}", parts[0], parts[1], parts[2]))
}

pub struct ReadVersion;

impl Action for ReadVersion {
    fn name(&self) -> &'static str {
        "read_version"
    }

    fn description(&self) -> &'static str {
        "Read the project's version"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::new(
            "file",
            "File to read; by default Cargo.toml, package.json, pubspec.yaml or VERSION",
        )]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let Some(path) = locate(ctx, args.get("file")) else {
            return Err(ctx.error(
                self.name(),
                format!(
                    "no version file found in {} (looked for {})",
                    ctx.workdir().display(),
                    CANDIDATES.join(", ")
                ),
            ));
        };

        let text = fs::read_to_string(&path).map_err(|err| {
            ctx.error(
                self.name(),
                format!("cannot read {}: {err}", path.display()),
            )
        })?;

        let Some(version) = read_version(&path, &text) else {
            return Err(ctx.error(
                self.name(),
                format!("no version found in {}", path.display()),
            ));
        };

        ctx.ui
            .say(&format!("Version {version} ({})", path.display()));
        Ok(ActionOutput::new()
            .with("version", version)
            .with("file", path.display().to_string()))
    }
}

pub struct BumpVersion;

impl Action for BumpVersion {
    fn name(&self) -> &'static str {
        "bump_version"
    }

    fn description(&self) -> &'static str {
        "Raise the project's version and write it back"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("file", "File to update; found automatically by default"),
            ArgSpec::new("part", "major, minor, patch or build").default("patch"),
            ArgSpec::new("set", "Use this exact version instead of raising a part"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let Some(path) = locate(ctx, args.get("file")) else {
            return Err(ctx.error(
                self.name(),
                format!(
                    "no version file found in {} (looked for {})",
                    ctx.workdir().display(),
                    CANDIDATES.join(", ")
                ),
            ));
        };

        let text = fs::read_to_string(&path).map_err(|err| {
            ctx.error(
                self.name(),
                format!("cannot read {}: {err}", path.display()),
            )
        })?;

        let Some(current) = read_version(&path, &text) else {
            return Err(ctx.error(
                self.name(),
                format!("no version found in {}", path.display()),
            ));
        };

        let next = match args.get("set") {
            Some(exact) => exact.to_string(),
            None => bump(&current, args.get_or("part", "patch"))
                .map_err(|message| ctx.error(self.name(), message))?,
        };

        if ctx.dry_run {
            ctx.ui.say(&format!(
                "Would bump {current} to {next} in {}",
                path.display()
            ));
        } else {
            // Replace only the first occurrence, which is the declaration; a
            // dependency pinned to the same number must not move with it.
            let updated = text.replacen(&current, &next, 1);
            fs::write(&path, updated).map_err(|err| {
                ctx.error(
                    self.name(),
                    format!("cannot write {}: {err}", path.display()),
                )
            })?;
            ctx.ui
                .say(&format!("Bumped {current} to {next} in {}", path.display()));
        }

        Ok(ActionOutput::new()
            .with("version", next)
            .with("previous", current)
            .with("file", path.display().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_cargo_toml() {
        let text = "[package]\nname = \"demo\"\nversion = \"1.2.3\"\n";
        assert_eq!(
            read_version(Path::new("Cargo.toml"), text).as_deref(),
            Some("1.2.3")
        );
    }

    #[test]
    fn reads_package_json() {
        let text = "{\n  \"name\": \"demo\",\n  \"version\": \"0.4.1\"\n}\n";
        assert_eq!(
            read_version(Path::new("package.json"), text).as_deref(),
            Some("0.4.1")
        );
    }

    #[test]
    fn reads_pubspec() {
        let text = "name: demo\nversion: 1.0.0+7\n";
        assert_eq!(
            read_version(Path::new("pubspec.yaml"), text).as_deref(),
            Some("1.0.0+7")
        );
    }

    #[test]
    fn reads_a_plain_version_file() {
        assert_eq!(
            read_version(Path::new("VERSION"), "2.0.0\n").as_deref(),
            Some("2.0.0")
        );
    }

    #[test]
    fn bumps_each_part() {
        assert_eq!(bump("1.2.3", "patch"), Ok("1.2.4".to_string()));
        assert_eq!(bump("1.2.3", "minor"), Ok("1.3.0".to_string()));
        assert_eq!(bump("1.2.3", "major"), Ok("2.0.0".to_string()));
    }

    #[test]
    fn bumps_a_flutter_build_number() {
        assert_eq!(bump("1.0.0+7", "build"), Ok("1.0.0+8".to_string()));
        assert!(bump("1.0.0", "build").is_err());
    }

    #[test]
    fn fills_in_missing_parts() {
        assert_eq!(bump("1.2", "patch"), Ok("1.2.1".to_string()));
    }

    #[test]
    fn rejects_what_is_not_a_version() {
        assert!(bump("nightly", "patch").is_err());
        assert!(bump("1.2.3", "sideways").is_err());
    }
}
