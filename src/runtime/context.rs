//! Per-lane state.
//!
//! Environment variables live here and are handed to child processes
//! explicitly. v0.1.0 called `std::env::set_var`, which mutates the whole
//! process (and is `unsafe` from Rust 2024 on); see
//! `docs/plan/10-secrets-and-env.md`.

use super::interpolate::Vars;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Context {
    pub lane: String,
    pub params: BTreeMap<String, String>,
    pub env: BTreeMap<String, String>,
    pub workdir: PathBuf,
}

impl Context {
    pub fn new(
        lane: impl Into<String>,
        params: BTreeMap<String, String>,
        env: BTreeMap<String, String>,
        workdir: impl AsRef<Path>,
    ) -> Self {
        Self {
            lane: lane.into(),
            params,
            env,
            workdir: workdir.as_ref().to_path_buf(),
        }
    }

    pub fn vars(&self) -> Vars<'_> {
        Vars {
            params: &self.params,
            env: &self.env,
        }
    }
}

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
}
