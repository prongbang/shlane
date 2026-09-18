//! `shlane env`.

use crate::config::dotenv;
use crate::config::loader::Discovered;
use crate::error::Result;
use crate::runtime::{ci, env, secrets::Secrets};
use std::collections::BTreeSet;

pub fn show(found: &Discovered, profile: Option<&str>, all: bool) -> Result<()> {
    let mut secrets = Secrets::new();
    let resolved = env::build(&found.config, &found.root, profile, &mut secrets)?;

    match ci::detect(&resolved) {
        Some(provider) => println!("running on {} CI\n", provider.as_str()),
        None => println!("not running on CI\n"),
    }

    // What the config contributes, which is what someone running this wants to
    // check. The rest is the process environment they already have.
    let mut from_config: BTreeSet<String> = found.config.env.keys().cloned().collect();
    for pattern in &found.config.env_files {
        // Unresolvable patterns name files that cannot exist; skipping them
        // here matches what a run does.
        if pattern.contains("${") {
            continue;
        }
        from_config.extend(dotenv::load(&found.root.join(pattern))?.into_keys());
    }

    let shown: Vec<(&String, &String)> = resolved
        .iter()
        .filter(|(name, _)| all || from_config.contains(*name))
        .collect();

    if all {
        println!("{} variable(s), as a lane would see them:", shown.len());
    } else {
        println!(
            "{} variable(s) from this config ({} inherited from the environment, --all to see them):",
            shown.len(),
            resolved.len() - shown.len()
        );
    }

    for (name, value) in shown {
        // Everything here goes through the same masking as a lane's output;
        // `shlane env` would otherwise be the easiest way to leak a token.
        println!("  {name}={}", secrets.mask(value));
    }

    Ok(())
}
