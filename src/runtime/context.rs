//! Per-lane state.
//!
//! Environment variables live here and are handed to child processes
//! explicitly. v0.1.0 called `std::env::set_var`, which mutates the whole
//! process (and is `unsafe` from Rust 2024 on); see
//! `docs/plan/10-secrets-and-env.md`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;

/// What the lane currently being executed can see.
///
/// Shared with the Rhai builtins so that `param()` and `env()` follow nested
/// `lane:` calls instead of being frozen at engine construction.
#[derive(Debug, Default, Clone)]
pub struct Frame {
    pub lane: String,
    pub params: BTreeMap<String, String>,
    pub env: BTreeMap<String, String>,
    pub workdir: PathBuf,
    pub dry_run: bool,
}

impl Frame {
    /// Values available as `${shlane.*}`.
    pub fn meta(&self) -> BTreeMap<String, String> {
        BTreeMap::from([
            ("lane".to_string(), self.lane.clone()),
            ("version".to_string(), env!("CARGO_PKG_VERSION").to_string()),
        ])
    }
}

pub type SharedFrame = Rc<RefCell<Frame>>;

/// What steps have produced so far, keyed by step id.
///
/// Outputs outlive the frame they were created in, so a later step can read
/// what an earlier one produced even across a `lane:` call.
#[derive(Debug, Default)]
pub struct Outputs {
    values: BTreeMap<String, BTreeMap<String, String>>,
}

impl Outputs {
    pub fn set(&mut self, id: &str, key: &str, value: impl Into<String>) {
        self.values
            .entry(id.to_string())
            .or_default()
            .insert(key.to_string(), value.into());
    }

    pub fn get(&self, id: &str, key: &str) -> Option<&String> {
        self.values.get(id)?.get(key)
    }

    /// Flattened as `<id>.<key>`, ready for `${steps.<id>.<key>}`.
    pub fn flatten(&self) -> BTreeMap<String, String> {
        let mut flat = BTreeMap::new();
        for (id, entries) in &self.values {
            for (key, value) in entries {
                flat.insert(format!("{id}.{key}"), value.clone());
            }
        }
        flat
    }
}

pub type SharedOutputs = Rc<RefCell<Outputs>>;

/// Parse `key=value` arguments from the command line.
///
/// A value may itself contain `=`; only the first one separates.
pub fn parse_params<I, S>(args: I) -> BTreeMap<String, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .filter_map(|arg| {
            arg.as_ref()
                .split_once('=')
                .map(|(k, v)| (k.to_string(), v.to_string()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_value_pairs() {
        let params = parse_params(["target=production", "notes=hello"]);
        assert_eq!(params.get("target").map(String::as_str), Some("production"));
        assert_eq!(params.get("notes").map(String::as_str), Some("hello"));
    }

    #[test]
    fn splits_on_the_first_equals_only() {
        let params = parse_params(["query=a=b"]);
        assert_eq!(params.get("query").map(String::as_str), Some("a=b"));
    }

    #[test]
    fn ignores_arguments_without_an_equals() {
        let params = parse_params(["standalone"]);
        assert!(params.is_empty());
    }

    #[test]
    fn meta_exposes_the_lane_and_version() {
        let frame = Frame {
            lane: "beta".to_string(),
            ..Frame::default()
        };
        let meta = frame.meta();
        assert_eq!(meta.get("lane").map(String::as_str), Some("beta"));
        assert!(meta.contains_key("version"));
    }

    #[test]
    fn outputs_are_looked_up_by_step_and_key() {
        let mut outputs = Outputs::default();
        outputs.set("build", "stdout", "ok");
        assert_eq!(
            outputs.get("build", "stdout").map(String::as_str),
            Some("ok")
        );
        assert_eq!(outputs.get("build", "missing"), None);
        assert_eq!(outputs.get("nope", "stdout"), None);
    }

    #[test]
    fn outputs_flatten_for_interpolation() {
        let mut outputs = Outputs::default();
        outputs.set("build", "stdout", "ok");
        outputs.set("build", "code", "0");
        let flat = outputs.flatten();
        assert_eq!(flat.get("build.stdout").map(String::as_str), Some("ok"));
        assert_eq!(flat.get("build.code").map(String::as_str), Some("0"));
    }
}

/// Something an action asked to have undone once the run is over.
///
/// `setup_ci` creates a keychain that must not outlive the job, and a step that
/// fails is exactly when it matters, so cleanups run whether the lane passed or
/// failed -- after the error hooks, which may still need what is being cleaned
/// up.
#[derive(Debug, Clone)]
pub struct Cleanup {
    /// Shown in the log, so an unexpected cleanup can be traced back.
    pub what: String,
    pub command: String,
}

pub type SharedCleanups = Rc<RefCell<Vec<Cleanup>>>;
