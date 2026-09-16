//! Locating and parsing the config file.

use super::model::Config;
use crate::error::{Result, ShlaneError};
use std::fs;
use std::path::{Path, PathBuf};

pub const DEFAULT_FILENAME: &str = "shlane.yaml";

/// Candidate file names, in the order they are tried.
const FILENAMES: [&str; 2] = ["shlane.yaml", "shlane.yml"];

/// A config file and the directory its relative paths resolve against.
pub struct Discovered {
    pub config: Config,
    pub path: PathBuf,
    pub root: PathBuf,
}

/// Find a config by walking up from `dir`, so shlane can be run from any
/// subdirectory of a project and behave the same.
///
/// `$SHLANE_CONFIG` overrides the search.
pub fn discover(dir: &Path) -> Result<Discovered> {
    if let Some(from_env) = std::env::var_os("SHLANE_CONFIG") {
        let path = PathBuf::from(from_env);
        return open(&path);
    }

    let mut current = Some(dir);
    while let Some(directory) = current {
        for filename in FILENAMES {
            let candidate = directory.join(filename);
            if candidate.is_file() {
                return open(&candidate);
            }
        }
        current = directory.parent();
    }

    Err(ShlaneError::ConfigNotFound {
        path: dir.join(DEFAULT_FILENAME),
    })
}

/// Load one specific config file.
pub fn open(path: &Path) -> Result<Discovered> {
    if !path.is_file() {
        return Err(ShlaneError::ConfigNotFound {
            path: path.to_path_buf(),
        });
    }
    let config = load_file(path)?;
    let root = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    Ok(Discovered {
        config,
        path: path.to_path_buf(),
        root,
    })
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
