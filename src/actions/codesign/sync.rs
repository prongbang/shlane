//! `codesign_sync`: read an existing `match` repository.
//!
//! Read-only on purpose. `match` can also create and revoke certificates, and
//! doing that wrong takes a team's ability to ship with it; the CI case —
//! fetch, decrypt, install — is the one worth having, and the one that cannot
//! break anything on Apple's side (`docs/plan/07-actions-ios.md`).

use super::{crypto, profile};
use crate::actions::context::ActionContext;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::Result;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Where `match` puts things, by the type of signing being asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    pub certs: &'static str,
    pub profiles: &'static str,
    pub prefix: &'static str,
}

pub fn layout(kind: &str) -> Option<Layout> {
    match kind {
        "appstore" => Some(Layout {
            certs: "certs/distribution",
            profiles: "profiles/appstore",
            prefix: "AppStore_",
        }),
        "adhoc" => Some(Layout {
            certs: "certs/distribution",
            profiles: "profiles/adhoc",
            prefix: "AdHoc_",
        }),
        "development" => Some(Layout {
            certs: "certs/development",
            profiles: "profiles/development",
            prefix: "Development_",
        }),
        "enterprise" => Some(Layout {
            certs: "certs/enterprise",
            profiles: "profiles/enterprise",
            prefix: "InHouse_",
        }),
        _ => None,
    }
}

/// The profile file `match` would have written for this app.
pub fn profile_path(root: &Path, layout: Layout, app_identifier: &str) -> PathBuf {
    root.join(layout.profiles)
        .join(format!("{}{app_identifier}.mobileprovision", layout.prefix))
}

/// The first `.p12` in the certificate directory.
///
/// `match` names them after the certificate id, which the config does not know.
fn find_p12(root: &Path, layout: Layout) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(root.join(layout.certs))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "p12"))
        .collect();
    found.sort();
    found.pop()
}

/// A decrypted file, removed when the step ends.
struct Decrypted {
    path: PathBuf,
}

impl Decrypted {
    fn write(path: &Path, contents: &[u8]) -> std::io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, contents)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for Decrypted {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub struct CodesignSync;

impl Action for CodesignSync {
    fn name(&self) -> &'static str {
        "codesign_sync"
    }

    fn description(&self) -> &'static str {
        "Fetch certificates and profiles from a fastlane match repository (read-only)"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("git_url", "The match repository").required(),
            ArgSpec::new("branch", "Branch to read").default("master"),
            ArgSpec::new("type", "appstore, adhoc, development or enterprise").default("appstore"),
            ArgSpec::new("app_identifier", "Bundle identifier").required(),
            ArgSpec::new(
                "passphrase",
                "The MATCH_PASSWORD the repository was encrypted with",
            )
            .required()
            .sensitive(),
            ArgSpec::new("storage", "Where to keep the clone").default(".shlane/codesign"),
            ArgSpec::new("output_dir", "Where to write the decrypted files")
                .default(".shlane/codesign-out"),
            ArgSpec::new(
                "install",
                "Import into a keychain and install the profile (macOS only)",
            )
            .default("true"),
            ArgSpec::new("keychain", "Keychain to import the certificate into")
                .default("shlane.keychain-db"),
            ArgSpec::new("keychain_password", "Password for that keychain").sensitive(),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let kind = args.get_or("type", "appstore");
        let Some(layout) = layout(kind) else {
            return Err(ctx.error(
                self.name(),
                format!("unknown type '{kind}'; use appstore, adhoc, development or enterprise"),
            ));
        };

        let app_identifier = args.get_or("app_identifier", "");
        let passphrase = args.get_or("passphrase", "");
        let storage = ctx
            .workdir()
            .join(args.get_or("storage", ".shlane/codesign"));
        let git_url = args.get_or("git_url", "");
        let branch = args.get_or("branch", "master");

        if ctx.dry_run {
            ctx.ui.say(&format!(
                "Would fetch {git_url} ({branch}) and install the {kind} profile for {app_identifier}"
            ));
            return Ok(ActionOutput::new().with("type", kind));
        }

        fetch(ctx, self.name(), &storage, git_url, branch)?;

        // Profile
        let encrypted_profile = profile_path(&storage, layout, app_identifier);
        if !encrypted_profile.is_file() {
            return Err(ctx.error(
                self.name(),
                format!(
                    "no {kind} profile for {app_identifier} in the repository (looked for {})",
                    encrypted_profile.display()
                ),
            ));
        }
        let profile_bytes = decrypt_file(ctx, self.name(), &encrypted_profile, passphrase)?;
        let details =
            profile::read(&profile_bytes).map_err(|message| ctx.error(self.name(), message))?;

        // Certificate
        let Some(encrypted_p12) = find_p12(&storage, layout) else {
            return Err(ctx.error(
                self.name(),
                format!("no .p12 in {}/{}", storage.display(), layout.certs),
            ));
        };
        let p12_bytes = decrypt_file(ctx, self.name(), &encrypted_p12, passphrase)?;

        let output_dir = ctx
            .workdir()
            .join(args.get_or("output_dir", ".shlane/codesign-out"));
        let profile_out = output_dir.join(format!("{}.mobileprovision", details.uuid));
        let p12_out = output_dir.join("certificate.p12");

        let profile_file = Decrypted::write(&profile_out, &profile_bytes)
            .map_err(|err| ctx.error(self.name(), format!("cannot write the profile: {err}")))?;
        let p12_file = Decrypted::write(&p12_out, &p12_bytes).map_err(|err| {
            ctx.error(self.name(), format!("cannot write the certificate: {err}"))
        })?;

        ctx.ui.say(&format!(
            "Profile '{}' ({}), team {}",
            details.name, details.uuid, details.team_id
        ));

        if !args.flag("install") {
            // The caller wants the files, not the side effects: keep them.
            std::mem::forget(profile_file);
            std::mem::forget(p12_file);
            return Ok(ActionOutput::new()
                .with("uuid", details.uuid)
                .with("name", details.name)
                .with("team_id", details.team_id)
                .with("profile", profile_out.display().to_string())
                .with("certificate", p12_out.display().to_string())
                .with("installed", "false"));
        }

        if !cfg!(target_os = "macos") {
            return Err(ctx.error(
                self.name(),
                "installing needs macOS; pass install: false to just fetch and decrypt",
            ));
        }

        install(
            ctx,
            self.name(),
            args,
            passphrase,
            &p12_out,
            &profile_out,
            &details,
        )?;

        Ok(ActionOutput::new()
            .with("uuid", details.uuid)
            .with("name", details.name)
            .with("team_id", details.team_id)
            .with("installed", "true"))
    }
}

/// Clone the repository, or bring an existing clone up to date.
fn fetch(
    ctx: &ActionContext<'_>,
    action: &str,
    storage: &Path,
    git_url: &str,
    branch: &str,
) -> Result<()> {
    if storage.join(".git").is_dir() {
        ctx.ui.say(&format!("Updating {}", storage.display()));
        let previous = storage.to_path_buf();
        let outcome = ctx.sh(&format!(
            "git -C {} fetch --depth 1 origin {} && git -C {} reset --hard FETCH_HEAD",
            quote(&previous.display().to_string()),
            quote(branch),
            quote(&previous.display().to_string())
        ))?;
        if !outcome.success {
            return Err(ctx.error(action, "could not update the certificate repository"));
        }
        return Ok(());
    }

    if let Some(parent) = storage.parent() {
        let _ = fs::create_dir_all(parent);
    }

    ctx.ui.say(&format!("Cloning {git_url} ({branch})"));
    let outcome = ctx.sh(&format!(
        "git clone --depth 1 --branch {} {} {}",
        quote(branch),
        quote(git_url),
        quote(&storage.display().to_string())
    ))?;
    if !outcome.success {
        return Err(ctx.error(
            action,
            format!("could not clone {git_url}; check the URL, the branch and the credentials"),
        ));
    }
    Ok(())
}

fn decrypt_file(
    ctx: &ActionContext<'_>,
    action: &str,
    path: &Path,
    passphrase: &str,
) -> Result<Vec<u8>> {
    let contents = fs::read(path)
        .map_err(|err| ctx.error(action, format!("cannot read {}: {err}", path.display())))?;
    crypto::decrypt(&contents, passphrase)
        .map_err(|message| ctx.error(action, format!("{}: {message}", path.display())))
}

/// Import into a keychain and put the profile where Xcode looks for it.
fn install(
    ctx: &ActionContext<'_>,
    action: &str,
    args: &Args,
    passphrase: &str,
    p12: &Path,
    profile_file: &Path,
    details: &profile::Profile,
) -> Result<()> {
    let keychain = args.get_or("keychain", "shlane.keychain-db");

    // match sets the p12's own password to the repository passphrase.
    let env = BTreeMap::from([("SHLANE_P12_PASS".to_string(), passphrase.to_string())]);
    ctx.require_with_env(
        &format!(
            "security import {} -k {} -P \"$SHLANE_P12_PASS\" -T /usr/bin/codesign -T /usr/bin/security -A",
            quote(&p12.display().to_string()),
            quote(keychain)
        ),
        &env,
    )?;

    // Without this, codesign prompts for permission and hangs a CI job.
    if let Some(keychain_password) = args.get("keychain_password") {
        let env = BTreeMap::from([(
            "SHLANE_KEYCHAIN_PASS".to_string(),
            keychain_password.to_string(),
        )]);
        ctx.require_with_env(
            &format!(
                "security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k \"$SHLANE_KEYCHAIN_PASS\" {}",
                quote(keychain)
            ),
            &env,
        )?;
    } else {
        ctx.ui
            .warn("no keychain_password given: codesign may prompt, which hangs a CI job");
    }

    let destination = format!(
        "$HOME/Library/MobileDevice/Provisioning Profiles/{}.mobileprovision",
        details.uuid
    );
    ctx.require(&format!(
        "mkdir -p \"$HOME/Library/MobileDevice/Provisioning Profiles\" && cp {} \"{destination}\"",
        quote(&profile_file.display().to_string())
    ))
    .map_err(|err| ctx.error(action, format!("could not install the profile: {err}")))?;

    ctx.ui.say("Certificate and profile installed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_where_match_keeps_each_kind() {
        let appstore = layout("appstore").expect("known");
        assert_eq!(appstore.certs, "certs/distribution");
        assert_eq!(appstore.profiles, "profiles/appstore");
        assert_eq!(appstore.prefix, "AppStore_");

        assert_eq!(layout("development").expect("known").prefix, "Development_");
        assert_eq!(layout("adhoc").expect("known").prefix, "AdHoc_");
        assert_eq!(layout("enterprise").expect("known").prefix, "InHouse_");
        assert!(layout("sideloaded").is_none());
    }

    #[test]
    fn builds_the_path_match_would_have_written() {
        let path = profile_path(
            Path::new("/repo"),
            layout("appstore").expect("known"),
            "com.example.app",
        );
        assert_eq!(
            path,
            Path::new("/repo/profiles/appstore/AppStore_com.example.app.mobileprovision")
        );
    }

    #[test]
    fn finds_the_certificate_without_being_told_its_id() {
        let root = std::env::temp_dir().join(format!("shlane-match-{}", std::process::id()));
        let certs = root.join("certs/distribution");
        fs::create_dir_all(&certs).expect("creatable");
        fs::write(certs.join("ABC123.cer"), "x").expect("writable");
        fs::write(certs.join("ABC123.p12"), "x").expect("writable");

        let found = find_p12(&root, layout("appstore").expect("known")).expect("a p12");
        assert!(found.ends_with("ABC123.p12"), "{found:?}");

        let _ = fs::remove_dir_all(&root);
    }
}
