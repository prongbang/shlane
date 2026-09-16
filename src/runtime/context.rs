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
#[derive(Debug, Default)]
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
}
