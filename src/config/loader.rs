//! Locating and parsing the config file.

use super::model::Config;
use crate::error::{Result, ShlaneError};
use std::fs;
use std::path::Path;

pub const DEFAULT_FILENAME: &str = "shlane.yaml";

/// Load the config from `dir`.
///
/// Searching parent directories is planned for M1
/// (`docs/plan/03-config-schema.md`); today only `dir` is consulted.
pub fn load_from_dir(dir: &Path) -> Result<Config> {
    let path = dir.join(DEFAULT_FILENAME);
    if !path.is_file() {
        return Err(ShlaneError::ConfigNotFound { path });
    }
    load_file(&path)
}

pub fn load_file(path: &Path) -> Result<Config> {
    let text = fs::read_to_string(path).map_err(|source| ShlaneError::ConfigUnreadable {
        path: path.to_path_buf(),
        source,
    })?;
    parse(&text, path)
}

pub fn parse(text: &str, path: &Path) -> Result<Config> {
    match serde_yaml::from_str::<Config>(text) {
        Ok(config) => Ok(config),
        Err(err) => {
            let location = err.location().map(|loc| (loc.line(), loc.column()));
            Err(ShlaneError::ConfigInvalid {
                path: path.to_path_buf(),
                location,
                message: err.to_string(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_str(text: &str) -> Result<Config> {
        parse(text, Path::new("shlane.yaml"))
    }

    #[test]
    fn parses_a_minimal_config() {
        let config = parse_str("lanes:\n  build:\n    steps:\n      - run: \"true\"\n")
            .expect("minimal config should parse");
        assert_eq!(config.lane_names(), vec!["build".to_string()]);
    }

    #[test]
    fn lane_names_are_sorted() {
        let config =
            parse_str("lanes:\n  zeta: {}\n  alpha: {}\n  mid: {}\n").expect("config should parse");
        assert_eq!(config.lane_names(), ["alpha", "mid", "zeta"]);
    }

    #[test]
    fn reports_the_line_of_a_syntax_error() {
        let err = parse_str("lanes:\n  build:\n   - this is not a map\n")
            .expect_err("broken yaml should fail");
        match err {
            ShlaneError::ConfigInvalid { location, .. } => assert!(location.is_some()),
            other => panic!("expected ConfigInvalid, got {other:?}"),
        }
    }

    #[test]
    fn rejects_unknown_fields() {
        let err = parse_str("lanes:\n  build:\n    stepz:\n      - run: \"true\"\n")
            .expect_err("a misspelled key should be rejected");
        assert!(err.to_string().contains("stepz"), "got: {err}");
    }

    #[test]
    fn config_without_lanes_is_valid_but_empty() {
        let config = parse_str("env:\n  A: b\n").expect("config should parse");
        assert!(config.lane_names().is_empty());
    }
}
