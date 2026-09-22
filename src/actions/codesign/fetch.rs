//! `provisioning_profile`, `certificate` and `xcode_settings`
//! (`docs/plan/07-actions-ios.md`).
//!
//! In place of fastlane's `sigh` and `cert`, and read-only for the same reason
//! `codesign_sync` is: issuing or revoking a certificate is how a team loses
//! the ability to ship, and a tool that can do it by accident will.

use crate::actions::asc::{
    credential_args, credential_problems, load_credential, migration_credential_arg, token,
};
use crate::actions::context::ActionContext;
use crate::actions::google::decode_base64;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::Result;
use std::collections::BTreeMap;
use std::path::PathBuf;

const API: &str = "https://api.appstoreconnect.apple.com";

/// The arguments every App Store Connect action needs.
fn key_args() -> Vec<ArgSpec> {
    credential_args()
}

fn get(ctx: &ActionContext<'_>, action: &str, path: &str, args: &Args) -> Result<String> {
    use crate::actions::core::http::{send, Payload};

    let key = load_credential(args, ctx.workdir()).map_err(|message| ctx.error(action, message))?;
    let bearer = token(&key).map_err(|message| ctx.error(action, message))?;

    let url = format!("{API}{path}");
    let response = send(
        ctx,
        "GET",
        &url,
        &[("Authorization".to_string(), format!("Bearer {bearer}"))],
        Payload::Empty,
    )
    .map_err(|message| ctx.error(action, message))?;

    if response.status >= 400 {
        return Err(ctx.error(
            action,
            format!("{path} returned {}\n{}", response.status, response.body),
        ));
    }
    Ok(response.body)
}

/// The first `"field": "value"` in a JSON document.
///
/// The responses wanted here have one interesting string each, and a JSON
/// parser in the dependency tree to find it would have to be worth more than
/// that.
fn field(json: &str, name: &str) -> Option<String> {
    let needle = format!("\"{name}\"");
    let start = json.find(&needle)? + needle.len();
    let rest = &json[start..];
    let rest = rest.trim_start().strip_prefix(':')?.trim_start();
    let rest = rest.strip_prefix('"')?;

    let mut value = String::new();
    let mut characters = rest.chars();
    while let Some(character) = characters.next() {
        match character {
            '\\' => value.push(characters.next()?),
            '"' => return Some(value),
            other => value.push(other),
        }
    }
    None
}

/// Count how many objects a list response returned.
fn count_entries(json: &str) -> usize {
    json.matches("\"type\"").count()
}

/// Download a provisioning profile Apple already holds, in place of `sigh`.
pub struct ProvisioningProfile;

impl Action for ProvisioningProfile {
    fn name(&self) -> &'static str {
        "provisioning_profile"
    }

    fn description(&self) -> &'static str {
        "Download a provisioning profile from App Store Connect (read-only)"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        let mut schema = vec![
            ArgSpec::new(
                "name",
                "Profile name, as it appears in the developer portal",
            )
            .required(),
            ArgSpec::new("output", "Where to write the .mobileprovision"),
            ArgSpec::new(
                "install",
                "Also copy it where Xcode looks (~/Library/MobileDevice/Provisioning Profiles)",
            )
            .default("false"),
        ];
        schema.extend(key_args());
        schema
    }

    fn validate_args(&self, provided: &BTreeMap<String, String>) -> Vec<String> {
        credential_problems(provided)
    }

    fn migration_required_args(&self) -> Vec<ArgSpec> {
        let mut required: Vec<ArgSpec> = self
            .schema()
            .into_iter()
            .filter(|spec| spec.required)
            .collect();
        required.push(migration_credential_arg());
        required
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let name = args.get_or("name", "");

        if ctx.dry_run {
            ctx.ui
                .say(&format!("Would download the profile named '{name}'"));
            return Ok(ActionOutput::new().with("name", name));
        }

        // Reading is not a change, so this runs under --dry-run too -- except
        // that writing the file is, which is why the dry run stops above.
        let body = get(
            ctx,
            self.name(),
            &format!("/v1/profiles?filter[name]={}&limit=1", urlencode(name)),
            args,
        )?;

        if count_entries(&body) == 0 {
            return Err(ctx.error(
                self.name(),
                format!("no profile named '{name}'; shlane does not create one -- issuing profiles is left to Xcode or the portal"),
            ));
        }

        let content = field(&body, "profileContent").ok_or_else(|| {
            ctx.error(
                self.name(),
                "the profile came back without its contents".to_string(),
            )
        })?;
        let bytes = decode_base64(&content).map_err(|message| {
            ctx.error(self.name(), format!("profile is not base64: {message}"))
        })?;

        let parsed = crate::actions::codesign::profile::read(&bytes)
            .map_err(|message| ctx.error(self.name(), message))?;

        let output = match args.get("output").filter(|value| !value.is_empty()) {
            Some(output) => ctx.workdir().join(output),
            None => ctx
                .workdir()
                .join(format!("{}.mobileprovision", parsed.uuid)),
        };
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                ctx.error(
                    self.name(),
                    format!("cannot create {}: {err}", parent.display()),
                )
            })?;
        }
        std::fs::write(&output, &bytes).map_err(|err| {
            ctx.error(
                self.name(),
                format!("cannot write {}: {err}", output.display()),
            )
        })?;
        ctx.ui.say(&format!("Wrote {}", output.display()));

        let mut installed = String::new();
        if args.flag("install") {
            let target = installed_path(&parsed.uuid);
            match target {
                Some(target) => {
                    if let Some(parent) = target.parent() {
                        std::fs::create_dir_all(parent).map_err(|err| {
                            ctx.error(
                                self.name(),
                                format!("cannot create {}: {err}", parent.display()),
                            )
                        })?;
                    }
                    std::fs::write(&target, &bytes).map_err(|err| {
                        ctx.error(
                            self.name(),
                            format!("cannot write {}: {err}", target.display()),
                        )
                    })?;
                    ctx.ui.say(&format!("Installed {}", target.display()));
                    installed = target.to_string_lossy().into_owned();
                }
                None => ctx
                    .ui
                    .warn("no home directory, so the profile was not installed"),
            }
        }

        Ok(ActionOutput::new()
            .with("path", output.to_string_lossy())
            .with("uuid", parsed.uuid)
            .with("name", parsed.name)
            .with("team_id", parsed.team_id)
            .with("installed", installed))
    }
}

fn installed_path(uuid: &str) -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join("Library/MobileDevice/Provisioning Profiles")
            .join(format!("{uuid}.mobileprovision")),
    )
}

/// List or download signing certificates, in place of `cert`.
pub struct Certificate;

impl Action for Certificate {
    fn name(&self) -> &'static str {
        "certificate"
    }

    fn description(&self) -> &'static str {
        "Download a signing certificate from App Store Connect (read-only)"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        let mut schema = vec![
            ArgSpec::new(
                "type",
                "Certificate type, e.g. DISTRIBUTION, DEVELOPMENT, IOS_DISTRIBUTION",
            )
            .default("DISTRIBUTION"),
            ArgSpec::new("output", "Where to write the .cer"),
        ];
        schema.extend(key_args());
        schema
    }

    fn validate_args(&self, provided: &BTreeMap<String, String>) -> Vec<String> {
        credential_problems(provided)
    }

    fn migration_required_args(&self) -> Vec<ArgSpec> {
        let mut required: Vec<ArgSpec> = self
            .schema()
            .into_iter()
            .filter(|spec| spec.required)
            .collect();
        required.push(migration_credential_arg());
        required
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let kind = args.get_or("type", "DISTRIBUTION");

        if ctx.dry_run {
            ctx.ui.say(&format!("Would look up a {kind} certificate"));
            return Ok(ActionOutput::new().with("type", kind));
        }

        let body = get(
            ctx,
            self.name(),
            &format!(
                "/v1/certificates?filter[certificateType]={}&limit=1",
                urlencode(kind)
            ),
            args,
        )?;

        if count_entries(&body) == 0 {
            return Err(ctx.error(
                self.name(),
                format!("no {kind} certificate in this account; shlane does not issue one -- a certificate issued by mistake has to be revoked, and revoking the wrong one stops the team shipping"),
            ));
        }

        let content = field(&body, "certificateContent").ok_or_else(|| {
            ctx.error(
                self.name(),
                "the certificate came back without its contents".to_string(),
            )
        })?;
        let bytes = decode_base64(&content).map_err(|message| {
            ctx.error(self.name(), format!("certificate is not base64: {message}"))
        })?;

        let name = field(&body, "name").unwrap_or_else(|| kind.to_string());
        let expires = field(&body, "expirationDate").unwrap_or_default();
        let serial = field(&body, "serialNumber").unwrap_or_default();

        let mut path = String::new();
        if let Some(output) = args.get("output").filter(|value| !value.is_empty()) {
            let target = ctx.workdir().join(output);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|err| {
                    ctx.error(
                        self.name(),
                        format!("cannot create {}: {err}", parent.display()),
                    )
                })?;
            }
            std::fs::write(&target, &bytes).map_err(|err| {
                ctx.error(
                    self.name(),
                    format!("cannot write {}: {err}", target.display()),
                )
            })?;
            ctx.ui.say(&format!("Wrote {}", target.display()));
            path = target.to_string_lossy().into_owned();
        }

        // Said plainly, because someone reaching for `cert` expects a keychain
        // it can sign with at the end of it.
        ctx.ui.say(&format!(
            "{name} expires {expires}. This is the public certificate only: Apple does not hand back the private key, so signing still needs the .p12 (see codesign_sync) or -allowProvisioningUpdates."
        ));

        Ok(ActionOutput::new()
            .with("path", path)
            .with("name", name)
            .with("serial", serial)
            .with("expires", expires))
    }
}

/// Set the signing settings in an Xcode project, in place of
/// `update_project_team` and `update_code_signing_settings`.
pub struct XcodeSettings;

impl Action for XcodeSettings {
    fn name(&self) -> &'static str {
        "xcode_settings"
    }

    fn description(&self) -> &'static str {
        "Change build settings in an Xcode project file"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new(
                "project",
                "The .xcodeproj, or the project.pbxproj inside it",
            )
            .required(),
            ArgSpec::new("team_id", "DEVELOPMENT_TEAM"),
            ArgSpec::new("code_sign_style", "CODE_SIGN_STYLE: Manual or Automatic"),
            ArgSpec::new("code_sign_identity", "CODE_SIGN_IDENTITY"),
            ArgSpec::new("profile_specifier", "PROVISIONING_PROFILE_SPECIFIER"),
            ArgSpec::new("bundle_id", "PRODUCT_BUNDLE_IDENTIFIER"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let given = ctx.workdir().join(args.get_or("project", ""));
        let path = if given.extension().is_some_and(|ext| ext == "xcodeproj") {
            given.join("project.pbxproj")
        } else {
            given
        };

        let settings: Vec<(&str, &str)> = [
            ("DEVELOPMENT_TEAM", args.get("team_id")),
            ("CODE_SIGN_STYLE", args.get("code_sign_style")),
            ("CODE_SIGN_IDENTITY", args.get("code_sign_identity")),
            (
                "PROVISIONING_PROFILE_SPECIFIER",
                args.get("profile_specifier"),
            ),
            ("PRODUCT_BUNDLE_IDENTIFIER", args.get("bundle_id")),
        ]
        .into_iter()
        .filter_map(|(key, value)| value.filter(|v| !v.is_empty()).map(|v| (key, v)))
        .collect();

        if settings.is_empty() {
            return Err(ctx.error(
                self.name(),
                "nothing to change; pass at least one of team_id, code_sign_style, code_sign_identity, profile_specifier or bundle_id",
            ));
        }

        let source = std::fs::read_to_string(&path).map_err(|err| {
            ctx.error(
                self.name(),
                format!("cannot read {}: {err}", path.display()),
            )
        })?;

        let mut updated = source.clone();
        let mut changed = 0;
        for (key, value) in &settings {
            let (next, count) = set_setting(&updated, key, value);
            updated = next;
            changed += count;
            if count == 0 {
                // Silence here means a setting the lane thinks it set: the
                // build then signs with whatever was there before.
                ctx.ui.warn(&format!(
                    "{key} does not appear in this project, so it was not set"
                ));
            } else {
                ctx.ui
                    .detail(&format!("{key} = {value} ({count} place(s))"));
            }
        }

        if changed == 0 {
            return Err(ctx.error(
                self.name(),
                format!(
                    "none of those settings appear in {}; add them in Xcode once so there is something to change",
                    path.display()
                ),
            ));
        }

        if ctx.dry_run {
            ctx.ui.say(&format!(
                "Would change {changed} setting(s) in {}",
                path.display()
            ));
            return Ok(ActionOutput::new().with("changed", changed.to_string()));
        }

        std::fs::write(&path, updated).map_err(|err| {
            ctx.error(
                self.name(),
                format!("cannot write {}: {err}", path.display()),
            )
        })?;
        ctx.ui.say(&format!(
            "Changed {changed} setting(s) in {}",
            path.display()
        ));

        Ok(ActionOutput::new()
            .with("path", path.to_string_lossy())
            .with("changed", changed.to_string()))
    }
}

/// Replace the value of every `KEY = value;` in a pbxproj.
///
/// Only settings the project already declares are touched. Inserting one means
/// guessing which build configurations it belongs to, and guessing wrong in a
/// project file is not something a diff makes obvious.
fn set_setting(source: &str, key: &str, value: &str) -> (String, usize) {
    let mut out = String::with_capacity(source.len());
    let mut changed = 0;

    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let is_setting = trimmed
            .strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='));

        if !is_setting {
            out.push_str(line);
            continue;
        }

        let indent = &line[..line.len() - trimmed.len()];
        let ending = if line.ends_with("\r\n") {
            "\r\n"
        } else if line.ends_with('\n') {
            "\n"
        } else {
            ""
        };
        out.push_str(&format!("{indent}{key} = {};{ending}", quote_pbx(value)));
        changed += 1;
    }

    (out, changed)
}

/// pbxproj quotes a value when it is not a bare word.
fn quote_pbx(value: &str) -> String {
    let bare = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '$');
    if bare {
        value.to_string()
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            b' ' => "%20".to_string(),
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_string_field_out_of_json() {
        let json = r#"{"data":[{"type":"profiles","attributes":{"name":"Acme Dist","profileContent":"YWJj"}}]}"#;
        assert_eq!(field(json, "name"), Some("Acme Dist".to_string()));
        assert_eq!(field(json, "profileContent"), Some("YWJj".to_string()));
        assert_eq!(field(json, "missing"), None);
    }

    #[test]
    fn keeps_escaped_characters_in_a_field() {
        let json = r#"{"name":"Acme \"Team\" Ltd"}"#;
        assert_eq!(field(json, "name"), Some(r#"Acme "Team" Ltd"#.to_string()));
    }

    #[test]
    fn counts_an_empty_list_as_nothing_found() {
        assert_eq!(count_entries(r#"{"data":[]}"#), 0);
        assert_eq!(count_entries(r#"{"data":[{"type":"certificates"}]}"#), 1);
    }

    #[test]
    fn replaces_a_setting_everywhere_it_appears() {
        let pbx = "\t\t\t\tDEVELOPMENT_TEAM = OLD123;\n\t\t\t\tOTHER = 1;\n\t\t\t\tDEVELOPMENT_TEAM = OLD123;\n";
        let (out, changed) = set_setting(pbx, "DEVELOPMENT_TEAM", "NEW456");
        assert_eq!(changed, 2);
        assert_eq!(out.matches("DEVELOPMENT_TEAM = NEW456;").count(), 2);
        assert!(out.contains("OTHER = 1;"), "other settings are left alone");
        assert!(out.contains("\t\t\t\tDEVELOPMENT_TEAM"), "indentation kept");
    }

    #[test]
    fn does_not_touch_a_setting_whose_name_merely_starts_the_same() {
        let pbx = "\t\tCODE_SIGN_STYLE = Automatic;\n\t\tCODE_SIGN_STYLE_EXTRA = 1;\n";
        let (out, changed) = set_setting(pbx, "CODE_SIGN_STYLE", "Manual");
        assert_eq!(changed, 1);
        assert!(out.contains("CODE_SIGN_STYLE_EXTRA = 1;"), "{out}");
    }

    #[test]
    fn reports_nothing_changed_when_the_setting_is_absent() {
        let (_, changed) = set_setting("\t\tOTHER = 1;\n", "DEVELOPMENT_TEAM", "X");
        assert_eq!(changed, 0);
    }

    #[test]
    fn quotes_only_what_pbxproj_needs_quoted() {
        assert_eq!(quote_pbx("ABCD1234"), "ABCD1234");
        assert_eq!(quote_pbx("com.example.app"), "com.example.app");
        assert_eq!(quote_pbx("iPhone Distribution"), "\"iPhone Distribution\"");
        assert_eq!(quote_pbx(""), "\"\"");
    }

    #[test]
    fn encodes_a_profile_name_for_a_query_string() {
        assert_eq!(urlencode("Acme Dist"), "Acme%20Dist");
        assert_eq!(urlencode("a/b&c"), "a%2Fb%26c");
    }
}
