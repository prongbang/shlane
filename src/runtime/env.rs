//! Building a lane's environment.
//!
//! Precedence, lowest first (`docs/plan/10-secrets-and-env.md`):
//!
//! 1. `env:` in the config
//! 2. each file in `env_files:`, in the order listed
//! 3. the environment shlane itself was started with — so a value injected by
//!    CI always wins over one committed to a file
//! 4. `env:` on the lane
//! 5. `env:` on the step, and `set_env()` in a script

use super::secrets::Secrets;
use crate::config::dotenv;
use crate::config::model::Config;
use crate::error::Result;
use std::collections::BTreeMap;
use std::path::Path;

/// Name of the variable that `.env.${SHLANE_PROFILE}` style paths read.
pub const PROFILE_VAR: &str = "SHLANE_PROFILE";

pub fn build(
    config: &Config,
    root: &Path,
    profile: Option<&str>,
    secrets: &mut Secrets,
) -> Result<BTreeMap<String, String>> {
    let mut env: BTreeMap<String, String> = config.env.clone();

    let process: BTreeMap<String, String> = std::env::vars().collect();
    let profile = profile
        .map(str::to_string)
        .or_else(|| process.get(PROFILE_VAR).cloned());

    for pattern in &config.env_files {
        let Some(name) = resolve_path(pattern, &process, profile.as_deref()) else {
            // A path whose variables cannot be resolved names a file that
            // cannot exist; that is not an error.
            continue;
        };
        env.extend(dotenv::load(&root.join(name))?);
    }

    env.extend(process);

    if let Some(profile) = profile {
        env.insert(PROFILE_VAR.to_string(), profile);
    }

    for (name, value) in &env {
        secrets.add_env(name, value);
    }

    Ok(env)
}

/// Substitute `${VAR}` in an `env_files:` entry, or return `None` if something
/// in it is unknown.
fn resolve_path(
    pattern: &str,
    process: &BTreeMap<String, String>,
    profile: Option<&str>,
) -> Option<String> {
    let mut out = String::with_capacity(pattern.len());
    let mut rest = pattern;

    while let Some(start) = rest.find("${") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after.find('}')?;
        let name = &after[..end];

        let value = if name == PROFILE_VAR {
            profile.map(str::to_string)
        } else {
            process.get(name).cloned()
        };
        out.push_str(&value?);
        rest = &after[end + 1..];
    }

    out.push_str(rest);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn substitutes_known_variables() {
        let process = map(&[("HOME", "/root")]);
        assert_eq!(
            resolve_path("${HOME}/.env", &process, None).as_deref(),
            Some("/root/.env")
        );
    }

    #[test]
    fn reads_the_profile() {
        let process = BTreeMap::new();
        assert_eq!(
            resolve_path(".env.${SHLANE_PROFILE}", &process, Some("ci")).as_deref(),
            Some(".env.ci")
        );
    }

    #[test]
    fn gives_up_on_unknown_variables() {
        let process = BTreeMap::new();
        assert_eq!(resolve_path(".env.${NOPE}", &process, None), None);
        assert_eq!(resolve_path(".env.${SHLANE_PROFILE}", &process, None), None);
    }

    #[test]
    fn leaves_plain_paths_alone() {
        let process = BTreeMap::new();
        assert_eq!(
            resolve_path(".env", &process, None).as_deref(),
            Some(".env")
        );
    }
}
