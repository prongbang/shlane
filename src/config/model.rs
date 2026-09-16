//! Shape of `shlane.yaml`.
//!
//! The schema here is deliberately unchanged from v0.1.0; the v1 schema is
//! planned in `docs/plan/03-config-schema.md`.

use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Environment variables handed to every command in every lane.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Rhai source evaluated once before a lane runs.
    pub script: Option<String>,
    /// Lanes, keyed by name. A `BTreeMap` keeps listings deterministic.
    #[serde(default)]
    pub lanes: BTreeMap<String, Lane>,
}

impl Config {
    pub fn lane_names(&self) -> Vec<String> {
        self.lanes.keys().cloned().collect()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lane {
    pub before: Option<Vec<String>>,
    pub steps: Option<Vec<Step>>,
    pub after: Option<Vec<String>>,
    pub script: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
    pub run: String,
}
