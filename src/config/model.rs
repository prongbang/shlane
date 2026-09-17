//! Shape of `shlane.yaml` (schema v1).
//!
//! See `docs/plan/03-config-schema.md`. The v0.1.0 shorthands still parse:
//! a hook entry may be a bare command string, and `- run: cmd` needs none of
//! the fields added here.

use serde::{de, Deserialize, Deserializer};
use serde_yaml::Value;
use std::collections::BTreeMap;
use std::time::Duration;

/// The only schema version this build understands.
pub const SUPPORTED_VERSION: u64 = 1;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Schema version. Absent means "whatever this build supports".
    pub version: Option<u64>,
    /// Minimum shlane version this config needs.
    pub min_shlane: Option<String>,
    /// Environment variables handed to every command in every lane.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// `.env` files to read, lowest priority first.
    #[serde(default)]
    pub env_files: Vec<String>,
    /// Values to hide wherever they appear in the output.
    #[serde(default)]
    pub secrets: Vec<String>,
    /// Plugins to load (`docs/plan/09-plugins.md`).
    #[serde(default)]
    pub plugins: Vec<PluginRef>,
    /// Rhai source evaluated once before a lane runs.
    pub script: Option<String>,
    /// Steps run before any lane's own steps.
    #[serde(default)]
    pub before_all: Vec<Step>,
    /// Steps run after a lane succeeds.
    #[serde(default)]
    pub after_all: Vec<Step>,
    /// Steps run when a lane fails.
    #[serde(default)]
    pub error: Vec<Step>,
    /// Lanes, keyed by name. A `BTreeMap` keeps listings deterministic.
    #[serde(default)]
    pub lanes: BTreeMap<String, Lane>,
}

impl Config {
    pub fn lane_names(&self) -> Vec<String> {
        self.lanes.keys().cloned().collect()
    }

    /// Lanes that may be started from the command line.
    pub fn public_lane_names(&self) -> Vec<String> {
        self.lanes
            .iter()
            .filter(|(_, lane)| !lane.private)
            .map(|(name, _)| name.clone())
            .collect()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginRef {
    pub name: String,
    /// Directory holding the plugin's manifest.
    pub path: Option<String>,
    /// Reserved for `github:owner/repo@tag`, which is not implemented yet.
    pub source: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lane {
    /// One line shown by `shlane list`.
    pub description: Option<String>,
    /// Free-form grouping, e.g. `ios` or `android`.
    pub platform: Option<String>,
    /// Private lanes can only be reached from another lane.
    #[serde(default)]
    pub private: bool,
    /// Declared parameters, validated before the lane runs.
    #[serde(default)]
    pub params: BTreeMap<String, ParamSpec>,
    /// Environment variables for this lane only.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    #[serde(default)]
    pub before: Vec<Step>,
    #[serde(default)]
    pub steps: Vec<Step>,
    #[serde(default)]
    pub after: Vec<Step>,
    /// Rhai source run after the lane's steps.
    pub script: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ParamType {
    #[default]
    String,
    Int,
    Bool,
}

impl ParamType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::String => "string",
            Self::Int => "int",
            Self::Bool => "bool",
        }
    }

    /// Check a command-line value against this type.
    pub fn check(self, value: &str) -> Result<(), String> {
        match self {
            Self::String => Ok(()),
            Self::Int => value
                .parse::<i64>()
                .map(|_| ())
                .map_err(|_| format!("expected an integer, got '{value}'")),
            Self::Bool => match value {
                "true" | "false" => Ok(()),
                other => Err(format!("expected true or false, got '{other}'")),
            },
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParamSpec {
    #[serde(rename = "type", default)]
    pub param_type: ParamType,
    #[serde(default)]
    pub required: bool,
    /// Used when the parameter is not passed. Numbers and booleans are
    /// accepted and stringified.
    #[serde(default, deserialize_with = "optional_scalar")]
    pub default: Option<String>,
    /// Restrict the parameter to this set of values.
    pub values: Option<Vec<String>>,
    pub description: Option<String>,
}

#[derive(Debug)]
pub struct Step {
    pub name: Option<String>,
    pub id: Option<String>,
    pub kind: StepKind,
    /// A Rhai expression; the step runs only when it evaluates to true.
    pub condition: Option<String>,
    pub env: BTreeMap<String, String>,
    pub workdir: Option<String>,
    pub timeout: Option<Duration>,
    /// Extra attempts after the first one.
    pub retry: u32,
    pub continue_on_error: bool,
}

#[derive(Debug)]
pub enum StepKind {
    Run(String),
    Script(String),
    Lane {
        name: String,
        with: BTreeMap<String, String>,
    },
    Action {
        name: String,
        with: BTreeMap<String, String>,
    },
}

impl Step {
    /// What to call this step in logs and in the summary.
    pub fn label(&self) -> String {
        if let Some(name) = &self.name {
            return name.clone();
        }
        if let Some(id) = &self.id {
            return id.clone();
        }
        match &self.kind {
            StepKind::Run(command) => truncate(command, 48),
            StepKind::Script(_) => "script".to_string(),
            StepKind::Lane { name, .. } => format!("lane {name}"),
            StepKind::Action { name, .. } => name.clone(),
        }
    }
}

fn truncate(text: &str, max: usize) -> String {
    let text = text.trim();
    let single_line = text.lines().next().unwrap_or(text);
    if single_line.chars().count() <= max {
        return single_line.to_string();
    }
    let kept: String = single_line.chars().take(max - 1).collect();
    format!("{kept}…")
}

// ---------------------------------------------------------------------------
// Deserialization
// ---------------------------------------------------------------------------

/// A hook entry may be a bare string, as in v0.1.0, or a full step map.
impl<'de> Deserialize<'de> for Step {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        if let Value::String(command) = value {
            return Ok(Step {
                name: None,
                id: None,
                kind: StepKind::Run(command),
                condition: None,
                env: BTreeMap::new(),
                workdir: None,
                timeout: None,
                retry: 0,
                continue_on_error: false,
            });
        }

        let raw = RawStep::deserialize(value).map_err(de::Error::custom)?;
        raw.into_step().map_err(de::Error::custom)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawStep {
    name: Option<String>,
    id: Option<String>,
    run: Option<String>,
    script: Option<String>,
    lane: Option<String>,
    action: Option<String>,
    #[serde(default)]
    with: BTreeMap<String, Value>,
    #[serde(rename = "if")]
    condition: Option<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    workdir: Option<String>,
    timeout: Option<Value>,
    #[serde(default)]
    retry: u32,
    #[serde(default)]
    continue_on_error: bool,
}

impl RawStep {
    fn into_step(self) -> Result<Step, String> {
        let mut kinds: Vec<&str> = Vec::new();
        if self.run.is_some() {
            kinds.push("run");
        }
        if self.script.is_some() {
            kinds.push("script");
        }
        if self.lane.is_some() {
            kinds.push("lane");
        }
        if self.action.is_some() {
            kinds.push("action");
        }

        let kind = match kinds.as_slice() {
            ["run"] if !self.with.is_empty() => {
                return Err("`with` is only valid on `lane` steps".to_string())
            }
            ["script"] if !self.with.is_empty() => {
                return Err("`with` is only valid on `lane` steps".to_string())
            }
            ["run"] => StepKind::Run(self.run.unwrap_or_default()),
            ["script"] => StepKind::Script(self.script.unwrap_or_default()),
            ["lane"] => StepKind::Lane {
                name: self.lane.unwrap_or_default(),
                with: stringify_map(self.with)?,
            },
            ["action"] => StepKind::Action {
                name: self.action.unwrap_or_default(),
                with: stringify_map(self.with)?,
            },
            [] => return Err("a step needs one of `run`, `script`, `lane` or `action`".to_string()),
            several => {
                return Err(format!(
                    "a step may only have one of `run`, `script`, `lane` or `action`, found: {}",
                    several.join(", ")
                ))
            }
        };

        let timeout = match self.timeout {
            Some(value) => Some(parse_duration(&value)?),
            None => None,
        };

        Ok(Step {
            name: self.name,
            id: self.id,
            kind,
            condition: self.condition,
            env: self.env,
            workdir: self.workdir,
            timeout,
            retry: self.retry,
            continue_on_error: self.continue_on_error,
        })
    }
}

fn stringify_map(map: BTreeMap<String, Value>) -> Result<BTreeMap<String, String>, String> {
    map.into_iter()
        .map(|(key, value)| {
            scalar_to_string(&value)
                .map(|value| (key.clone(), value))
                .ok_or_else(|| format!("`with.{key}` must be a string, number or boolean"))
        })
        .collect()
}

fn scalar_to_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        _ => None,
    }
}

fn optional_scalar<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    if value.is_null() {
        return Ok(None);
    }
    scalar_to_string(&value)
        .map(Some)
        .ok_or_else(|| de::Error::custom("default must be a string, number or boolean"))
}

/// `30`, `"30s"`, `"20m"`, `"1h"`.
fn parse_duration(value: &Value) -> Result<Duration, String> {
    let text = match value {
        Value::Number(number) => number.to_string(),
        Value::String(text) => text.clone(),
        _ => return Err("timeout must be a number or a string like '20m'".to_string()),
    };
    let text = text.trim();

    let (digits, multiplier) = match text.chars().last() {
        Some('s') => (&text[..text.len() - 1], 1),
        Some('m') => (&text[..text.len() - 1], 60),
        Some('h') => (&text[..text.len() - 1], 3600),
        _ => (text, 1),
    };

    let amount: u64 = digits
        .trim()
        .parse()
        .map_err(|_| format!("cannot read '{text}' as a duration; try 30s, 20m or 1h"))?;
    if amount == 0 {
        return Err("timeout must be greater than zero".to_string());
    }
    Ok(Duration::from_secs(amount * multiplier))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(yaml: &str) -> Result<Step, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    #[test]
    fn a_bare_string_is_a_run_step() {
        let step = step("echo hi").expect("should parse");
        assert!(matches!(step.kind, StepKind::Run(ref c) if c == "echo hi"));
    }

    #[test]
    fn a_run_map_parses() {
        let step = step("run: echo hi").expect("should parse");
        assert!(matches!(step.kind, StepKind::Run(_)));
        assert_eq!(step.retry, 0);
    }

    #[test]
    fn a_step_needs_exactly_one_kind() {
        let err = step("name: nothing").expect_err("should fail");
        assert!(err.to_string().contains("needs one of"), "got: {err}");

        let err = step("run: a\nscript: b").expect_err("should fail");
        assert!(err.to_string().contains("only have one of"), "got: {err}");
    }

    #[test]
    fn durations_accept_suffixes() {
        assert_eq!(
            step("run: a\ntimeout: 90").expect("parse").timeout,
            Some(Duration::from_secs(90))
        );
        assert_eq!(
            step("run: a\ntimeout: 2m").expect("parse").timeout,
            Some(Duration::from_secs(120))
        );
        assert_eq!(
            step("run: a\ntimeout: 1h").expect("parse").timeout,
            Some(Duration::from_secs(3600))
        );
        assert!(step("run: a\ntimeout: soon").is_err());
        assert!(step("run: a\ntimeout: 0s").is_err());
    }

    #[test]
    fn unknown_step_fields_are_rejected() {
        let err = step("run: a\nretires: 2").expect_err("should fail");
        assert!(err.to_string().contains("retires"), "got: {err}");
    }

    #[test]
    fn lane_steps_stringify_their_arguments() {
        let step = step("lane: notify\nwith:\n  channel: releases\n  count: 3\n").expect("parse");
        match step.kind {
            StepKind::Lane { name, with } => {
                assert_eq!(name, "notify");
                assert_eq!(with.get("channel").map(String::as_str), Some("releases"));
                assert_eq!(with.get("count").map(String::as_str), Some("3"));
            }
            other => panic!("expected a lane step, got {other:?}"),
        }
    }

    #[test]
    fn labels_fall_back_from_name_to_id_to_content() {
        let named = step("run: a\nname: build the app").expect("parse");
        assert_eq!(named.label(), "build the app");
        let with_id = step("run: a\nid: build").expect("parse");
        assert_eq!(with_id.label(), "build");
        let bare = step("run: cargo test").expect("parse");
        assert_eq!(bare.label(), "cargo test");
    }

    #[test]
    fn param_types_check_values() {
        assert!(ParamType::Int.check("12").is_ok());
        assert!(ParamType::Int.check("x").is_err());
        assert!(ParamType::Bool.check("true").is_ok());
        assert!(ParamType::Bool.check("yes").is_err());
        assert!(ParamType::String.check("anything").is_ok());
    }
}
