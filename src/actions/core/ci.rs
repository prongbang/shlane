//! `setup_ci` (`docs/plan/11-ci-integration.md`).

use crate::actions::context::ActionContext;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::Result;
use crate::runtime::ci;
use std::collections::BTreeMap;

/// Prepare a CI machine for signing, and undo it afterwards.
///
/// The keychain is the part people get wrong: a build machine that keeps the
/// one the last job created ends up with a keychain nobody can unlock, so the
/// deletion is registered before the keychain is created rather than appended
/// to the lane and skipped by the first failure.
pub struct SetupCi;

impl Action for SetupCi {
    fn name(&self) -> &'static str {
        "setup_ci"
    }

    fn description(&self) -> &'static str {
        "Prepare a CI machine for signing, and clean up afterwards"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("keychain_name", "Keychain to create").default("shlane_tmp.keychain-db"),
            ArgSpec::new(
                "keychain_password",
                "Password for it; a random one is used when this is not given",
            )
            .sensitive(),
            ArgSpec::new("timeout", "Lock the keychain again after this many seconds")
                .default("3600"),
            ArgSpec::new(
                "force",
                "Set up even when this does not look like a CI machine",
            )
            .default("false"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let provider = ci::detect(ctx.env);

        if provider.is_none() && !args.flag("force") {
            // On a developer's machine this would take over the default
            // keychain and lock them out of their own certificates.
            // Said out loud rather than logged as a detail: a setup step that
            // quietly does nothing is how a signing failure three steps later
            // becomes a mystery.
            ctx.ui
                .say("Not running on CI, so there is nothing to set up (force: true overrides)");
            return Ok(ActionOutput::new()
                .with("ci", "false")
                .with("keychain", String::new()));
        }

        let provider_name = provider.map_or("unknown", ci::Provider::as_str);
        ctx.ui.say(&format!("CI detected: {provider_name}"));

        if !cfg!(target_os = "macos") {
            // Everything below is `security`, which only exists on macOS. Say
            // so rather than failing with "command not found".
            ctx.ui.say("Not macOS, so there is no keychain to set up");
            return Ok(ActionOutput::new()
                .with("ci", "true")
                .with("provider", provider_name)
                .with("keychain", String::new()));
        }

        let name = args.get_or("keychain_name", "shlane_tmp.keychain-db");
        let password = match args.get("keychain_password") {
            Some(password) if !password.is_empty() => password.to_string(),
            _ => {
                let generated = random_password();
                if generated.is_empty() {
                    return Err(ctx.error(
                        self.name(),
                        "could not generate a keychain password; pass keychain_password",
                    ));
                }
                ctx.mark_secret(&generated);
                generated
            }
        };
        let env = BTreeMap::from([("SHLANE_KEYCHAIN_PASS".to_string(), password)]);
        let quoted = shell_quote(name);

        // Registered first: if creating it half succeeds, the leftover still
        // gets removed.
        ctx.on_finish(
            format!("delete the keychain {name}"),
            format!("security delete-keychain {quoted}"),
        );

        let _ = ctx.sh(&format!("security delete-keychain {quoted}"));
        ctx.require_with_env(
            &format!("security create-keychain -p \"$SHLANE_KEYCHAIN_PASS\" {quoted}"),
            &env,
        )?;
        ctx.require(&format!(
            "security set-keychain-settings -lut {} {quoted}",
            args.get_or("timeout", "3600")
        ))?;
        ctx.require_with_env(
            &format!("security unlock-keychain -p \"$SHLANE_KEYCHAIN_PASS\" {quoted}"),
            &env,
        )?;
        ctx.require(&format!(
            "security list-keychains -d user -s {quoted} login.keychain"
        ))?;

        Ok(ActionOutput::new()
            .with("ci", "true")
            .with("provider", provider_name)
            .with("keychain", name))
    }
}

/// A password for a keychain that lives for one job and is never written down.
///
/// From `ring`'s system CSPRNG rather than the clock: a keychain password
/// guessable from the job's start time is not a password.
fn random_password() -> String {
    use ring::rand::SecureRandom;
    let mut bytes = [0u8; 16];
    if ring::rand::SystemRandom::new().fill(&mut bytes).is_err() {
        // The OS refusing to produce randomness is not something to paper over
        // with a weaker password.
        return String::new();
    }
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_a_hex_password_that_differs_each_time() {
        let first = random_password();
        assert_eq!(first.len(), 32);
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(first, random_password());
    }

    #[test]
    fn quotes_a_keychain_name_with_a_quote_in_it() {
        assert_eq!(shell_quote("it's.keychain"), "'it'\\''s.keychain'");
    }
}
