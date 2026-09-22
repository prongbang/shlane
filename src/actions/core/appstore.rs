//! `appstore`, in place of fastlane's `deliver`
//! (`docs/plan/07-actions-ios.md`).
//!
//! Pushes App Store metadata over the App Store Connect API, attaches a build,
//! and optionally sends the version for review. The binary itself still goes up
//! through `testflight`: uploading to Apple is `altool`'s job, and reimplementing
//! the transporter protocol would be a large amount of machinery to replace a
//! tool every macOS runner already has.

use crate::actions::asc::{
    credential_args, credential_problems, load_credential, migration_credential_arg, token,
};
use crate::actions::context::ActionContext;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::{Result, ShlaneError};
use serde_yaml::Value;
use std::collections::BTreeMap;
use std::path::Path;

const API: &str = "https://api.appstoreconnect.apple.com";

/// The fields of an `appStoreVersionLocalization`, and the file each one comes
/// from in a fastlane-shaped metadata directory.
const LOCALIZED: &[(&str, &str)] = &[
    ("description", "description.txt"),
    ("keywords", "keywords.txt"),
    ("whatsNew", "release_notes.txt"),
    ("promotionalText", "promotional_text.txt"),
    ("supportUrl", "support_url.txt"),
    ("marketingUrl", "marketing_url.txt"),
];

/// Fields that live on the app rather than the version, so a metadata directory
/// carrying them is not silently half-applied.
const APP_LEVEL: &[&str] = &["name.txt", "subtitle.txt", "privacy_url.txt"];

pub struct AppStore;

impl Action for AppStore {
    fn name(&self) -> &'static str {
        "appstore"
    }

    fn description(&self) -> &'static str {
        "Push App Store metadata, and optionally submit for review"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        let mut schema = vec![
            ArgSpec::new("bundle_id", "The app's bundle identifier").required(),
            ArgSpec::new("version", "Marketing version, e.g. 1.4.2").required(),
            ArgSpec::new("platform", "IOS, MAC_OS or TV_OS").default("IOS"),
            ArgSpec::new(
                "metadata_dir",
                "A fastlane-shaped metadata directory: <locale>/description.txt and so on",
            ),
            ArgSpec::new("locale", "Locale for the fields given directly").default("en-US"),
            ArgSpec::new("description", "App description for that locale"),
            ArgSpec::new("keywords", "Comma-separated keywords"),
            ArgSpec::new("whats_new", "Release notes for this version"),
            ArgSpec::new("promotional_text", "Promotional text"),
            ArgSpec::new("support_url", "Support URL"),
            ArgSpec::new("marketing_url", "Marketing URL"),
            ArgSpec::new("build", "Build number to attach to this version"),
            ArgSpec::new("release_type", "MANUAL, AFTER_APPROVAL or SCHEDULED"),
            ArgSpec::new(
                "submit_for_review",
                "Send the version for review once the metadata is in",
            )
            .default("false"),
        ];
        schema.extend(credential_args());
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
        let bundle_id = args.get_or("bundle_id", "");
        let version = args.get_or("version", "");
        let platform = args.get_or("platform", "IOS");

        let client = Client::new("appstore", ctx, args)?;

        // Reads happen for real under --dry-run, so it reports what would
        // actually change rather than what the config hoped was there.
        let app_id = client.app_id(ctx, bundle_id)?;
        ctx.ui.detail(&format!("app {bundle_id} is {app_id}"));

        let (version_id, state, existed) = client.version(ctx, &app_id, version, platform)?;
        ctx.ui.say(&format!(
            "Version {version} ({state}){}",
            if existed { "" } else { ", newly created" }
        ));

        let localizations = client.localizations(ctx, &version_id)?;
        let wanted = collect_metadata(ctx, args)?;

        let mut updated = 0;
        for (locale, fields) in &wanted {
            let Some(localization_id) = localizations.get(locale) else {
                // Creating one means claiming the app supports that language,
                // which is a store-listing decision, not a deploy-script one.
                ctx.ui.warn(&format!(
                    "this version has no '{locale}' localization; add the language in App Store Connect first"
                ));
                continue;
            };

            let body = patch_body("appStoreVersionLocalizations", localization_id, fields);
            if ctx.dry_run {
                ctx.ui.say(&format!(
                    "Would update {locale}: {}",
                    fields
                        .iter()
                        .map(|(name, _)| *name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            } else {
                client.request(
                    ctx,
                    "PATCH",
                    &format!("/v1/appStoreVersionLocalizations/{localization_id}"),
                    Some(&body),
                )?;
                ctx.ui.say(&format!("Updated {locale}"));
            }
            updated += 1;
        }

        if let Some(build) = args.get("build").filter(|value| !value.is_empty()) {
            let build_id = client.build_id(ctx, &app_id, build)?;
            let body = format!(
                r#"{{"data":{{"type":"builds","id":"{}"}}}}"#,
                escape(&build_id)
            );
            if ctx.dry_run {
                ctx.ui
                    .say(&format!("Would attach build {build} ({build_id})"));
            } else {
                client.request(
                    ctx,
                    "PATCH",
                    &format!("/v1/appStoreVersions/{version_id}/relationships/build"),
                    Some(&body),
                )?;
                ctx.ui.say(&format!("Attached build {build}"));
            }
        }

        if let Some(release_type) = args.get("release_type").filter(|value| !value.is_empty()) {
            let body = patch_body(
                "appStoreVersions",
                &version_id,
                &[("releaseType", release_type.to_string())],
            );
            if ctx.dry_run {
                ctx.ui
                    .say(&format!("Would set releaseType to {release_type}"));
            } else {
                client.request(
                    ctx,
                    "PATCH",
                    &format!("/v1/appStoreVersions/{version_id}"),
                    Some(&body),
                )?;
            }
        }

        let mut submitted = false;
        if args.flag("submit_for_review") {
            let body = format!(
                r#"{{"data":{{"type":"appStoreVersionSubmissions","relationships":{{"appStoreVersion":{{"data":{{"type":"appStoreVersions","id":"{}"}}}}}}}}}}"#,
                escape(&version_id)
            );
            if ctx.dry_run {
                ctx.ui
                    .say(&format!("Would submit version {version} for review"));
            } else {
                client.request(ctx, "POST", "/v1/appStoreVersionSubmissions", Some(&body))?;
                ctx.ui.say(&format!("Submitted {version} for review"));
                submitted = true;
            }
        }

        Ok(ActionOutput::new()
            .with("app_id", app_id)
            .with("version_id", version_id)
            .with("version", version)
            .with("state", state)
            .with("localizations", updated.to_string())
            .with("submitted", submitted.to_string()))
    }
}

/// A signed App Store Connect session.
struct Client {
    action: &'static str,
    bearer: String,
}

impl Client {
    fn new(action: &'static str, ctx: &ActionContext<'_>, args: &Args) -> Result<Self> {
        let key =
            load_credential(args, ctx.workdir()).map_err(|message| ctx.error(action, message))?;
        let bearer = token(&key).map_err(|message| ctx.error(action, message))?;
        Ok(Self { action, bearer })
    }

    fn request(
        &self,
        ctx: &ActionContext<'_>,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<Value> {
        use crate::actions::core::http::{send, Payload};

        let mut headers = vec![(
            "Authorization".to_string(),
            format!("Bearer {}", self.bearer),
        )];
        if body.is_some() {
            headers.push(("Content-Type".to_string(), "application/json".to_string()));
        }

        let response = send(
            ctx,
            method,
            &format!("{API}{path}"),
            &headers,
            match body {
                Some(body) => Payload::Text(body),
                None => Payload::Empty,
            },
        )
        .map_err(|message| ctx.error(self.action, message))?;

        if response.status >= 400 {
            return Err(ctx.error(
                self.action,
                format!(
                    "{method} {path} returned {}\n{}",
                    response.status,
                    first_error(&response.body)
                ),
            ));
        }

        parse(&response.body).map_err(|message| ctx.error(self.action, message))
    }

    fn app_id(&self, ctx: &ActionContext<'_>, bundle_id: &str) -> Result<String> {
        let body = self.request(
            ctx,
            "GET",
            &format!("/v1/apps?filter[bundleId]={}&limit=1", urlencode(bundle_id)),
            None,
        )?;
        id_at(&body, 0).ok_or_else(|| {
            ctx.error(
                self.action,
                format!("no app with bundle id '{bundle_id}' in this account"),
            )
        })
    }

    /// The `appStoreVersion` for this version string, created when it is not
    /// there yet.
    fn version(
        &self,
        ctx: &ActionContext<'_>,
        app_id: &str,
        version: &str,
        platform: &str,
    ) -> Result<(String, String, bool)> {
        let body = self.request(
            ctx,
            "GET",
            &format!(
                "/v1/apps/{app_id}/appStoreVersions?filter[versionString]={}&filter[platform]={}&limit=1",
                urlencode(version),
                urlencode(platform)
            ),
            None,
        )?;

        if let Some(id) = id_at(&body, 0) {
            let state = string_at(&body, 0, "appStoreState").unwrap_or_default();
            return Ok((id, state, true));
        }

        if ctx.dry_run {
            // Nothing is created during a dry run, so the rest of it reports
            // against a version that does not exist yet.
            ctx.ui
                .say(&format!("Would create version {version} for {platform}"));
            return Ok((String::new(), "WOULD_CREATE".to_string(), false));
        }

        let payload = format!(
            r#"{{"data":{{"type":"appStoreVersions","attributes":{{"platform":"{}","versionString":"{}"}},"relationships":{{"app":{{"data":{{"type":"apps","id":"{}"}}}}}}}}}}"#,
            escape(platform),
            escape(version),
            escape(app_id)
        );
        let created = self.request(ctx, "POST", "/v1/appStoreVersions", Some(&payload))?;
        let id = created["data"]["id"]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| ctx.error(self.action, "the created version came back without an id"))?;
        let state = created["data"]["attributes"]["appStoreState"]
            .as_str()
            .unwrap_or("PREPARE_FOR_SUBMISSION")
            .to_string();
        Ok((id, state, false))
    }

    /// Locale to localization id, for the localizations the version already has.
    fn localizations(
        &self,
        ctx: &ActionContext<'_>,
        version_id: &str,
    ) -> Result<std::collections::BTreeMap<String, String>> {
        if version_id.is_empty() {
            return Ok(std::collections::BTreeMap::new());
        }

        let body = self.request(
            ctx,
            "GET",
            &format!("/v1/appStoreVersions/{version_id}/appStoreVersionLocalizations?limit=200"),
            None,
        )?;

        let mut found = std::collections::BTreeMap::new();
        let mut index = 0;
        while let Some(id) = id_at(&body, index) {
            if let Some(locale) = string_at(&body, index, "locale") {
                found.insert(locale, id);
            }
            index += 1;
        }
        Ok(found)
    }

    fn build_id(&self, ctx: &ActionContext<'_>, app_id: &str, build: &str) -> Result<String> {
        let body = self.request(
            ctx,
            "GET",
            &format!(
                "/v1/builds?filter[app]={app_id}&filter[version]={}&limit=1",
                urlencode(build)
            ),
            None,
        )?;
        id_at(&body, 0).ok_or_else(|| {
            ctx.error(
                self.action,
                format!("no build {build} for this app; upload it with `testflight` first"),
            )
        })
    }
}

/// What to set, per locale, from a metadata directory and from the arguments.
///
/// The arguments win: someone who wrote `whats_new:` in the lane meant it,
/// whatever is in the directory.
fn collect_metadata(
    ctx: &ActionContext<'_>,
    args: &Args,
) -> Result<Vec<(String, LocalizedFields)>> {
    let mut by_locale: Metadata = Metadata::new();

    if let Some(dir) = args.get("metadata_dir").filter(|value| !value.is_empty()) {
        let root = ctx.workdir().join(dir);
        let (found, skipped) = metadata_from_dir(&root).map_err(|message| ShlaneError::Action {
            action: "appstore".to_string(),
            message,
        })?;
        by_locale = found;
        for name in skipped {
            ctx.ui.warn(&format!(
                "{name} is app-level metadata, which shlane does not set; change it in App Store Connect"
            ));
        }
    }

    let locale = args.get_or("locale", "en-US").to_string();
    let direct: Vec<(&'static str, Option<&str>)> = vec![
        ("description", args.get("description")),
        ("keywords", args.get("keywords")),
        ("whatsNew", args.get("whats_new")),
        ("promotionalText", args.get("promotional_text")),
        ("supportUrl", args.get("support_url")),
        ("marketingUrl", args.get("marketing_url")),
    ];

    for (field, value) in direct {
        let Some(value) = value.filter(|value| !value.is_empty()) else {
            continue;
        };
        let fields = by_locale.entry(locale.clone()).or_default();
        fields.retain(|(name, _)| *name != field);
        fields.push((field, value.to_string()));
    }

    Ok(by_locale.into_iter().collect())
}

/// The fields to set for one locale.
type LocalizedFields = Vec<(&'static str, String)>;

/// Read a fastlane-shaped metadata directory.
///
/// Returns what to set per locale, and anything found that belongs to the app
/// rather than to this version -- reported rather than skipped silently, so a
/// lane does not look like it applied metadata it did not.
type Metadata = std::collections::BTreeMap<String, LocalizedFields>;

fn metadata_from_dir(root: &Path) -> std::result::Result<(Metadata, Vec<String>), String> {
    let mut found: Metadata = std::collections::BTreeMap::new();
    let mut skipped = Vec::new();

    let entries =
        std::fs::read_dir(root).map_err(|err| format!("cannot read {}: {err}", root.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let locale = entry.file_name().to_string_lossy().into_owned();

        for name in APP_LEVEL {
            if path.join(name).is_file() {
                skipped.push(format!("{locale}/{name}"));
            }
        }

        for (field, file) in LOCALIZED {
            let file = path.join(file);
            if !file.is_file() {
                continue;
            }
            let text = std::fs::read_to_string(&file)
                .map_err(|err| format!("cannot read {}: {err}", file.display()))?;
            found
                .entry(locale.clone())
                .or_default()
                .push((field, text.trim_end().to_string()));
        }
    }

    skipped.sort();
    Ok((found, skipped))
}

/// A JSON:API PATCH body for one resource.
fn patch_body(kind: &str, id: &str, fields: &[(&str, String)]) -> String {
    let attributes: Vec<String> = fields
        .iter()
        .map(|(name, value)| format!(r#""{}":"{}""#, escape(name), escape(value)))
        .collect();
    format!(
        r#"{{"data":{{"type":"{}","id":"{}","attributes":{{{}}}}}}}"#,
        escape(kind),
        escape(id),
        attributes.join(",")
    )
}

/// Parse a JSON response.
///
/// Through `serde_yaml`, because JSON is a subset of YAML and the alternative
/// is another dependency for the same answer. Every string in a response is
/// quoted, so YAML's bare-word rules -- the ones that turn `no` into `false` --
/// never come into it.
fn parse(body: &str) -> std::result::Result<Value, String> {
    if body.trim().is_empty() {
        return Ok(Value::Null);
    }
    serde_yaml::from_str(body).map_err(|err| format!("could not read the response: {err}"))
}

fn id_at(body: &Value, index: usize) -> Option<String> {
    body["data"]
        .as_sequence()?
        .get(index)?
        .get("id")?
        .as_str()
        .map(str::to_string)
}

fn string_at(body: &Value, index: usize, attribute: &str) -> Option<String> {
    body["data"]
        .as_sequence()?
        .get(index)?
        .get("attributes")?
        .get(attribute)?
        .as_str()
        .map(str::to_string)
}

/// Apple's first error detail, which says more than the status code.
fn first_error(body: &str) -> String {
    let Ok(value) = parse(body) else {
        return body.to_string();
    };
    let Some(errors) = value["errors"].as_sequence() else {
        return body.to_string();
    };
    let Some(first) = errors.first() else {
        return body.to_string();
    };
    let title = first["title"].as_str().unwrap_or_default();
    let detail = first["detail"].as_str().unwrap_or_default();
    if title.is_empty() && detail.is_empty() {
        body.to_string()
    } else {
        format!("{title}: {detail}")
    }
}

fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn urlencode(value: &str) -> String {
    value
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (byte as char).to_string()
            }
            other => format!("%{other:02X}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_id_and_an_attribute_out_of_a_response() {
        let body = parse(
            r#"{"data":[{"type":"apps","id":"6001","attributes":{"bundleId":"com.example.app"}}]}"#,
        )
        .expect("should parse");
        assert_eq!(id_at(&body, 0), Some("6001".to_string()));
        assert_eq!(
            string_at(&body, 0, "bundleId"),
            Some("com.example.app".to_string())
        );
        assert_eq!(id_at(&body, 1), None);
    }

    #[test]
    fn an_empty_list_has_no_first_entry() {
        let body = parse(r#"{"data":[]}"#).expect("should parse");
        assert_eq!(id_at(&body, 0), None);
    }

    #[test]
    fn a_quoted_string_stays_a_string() {
        // YAML would read a bare `no` as false; every value here is quoted.
        let body = parse(r#"{"data":[{"id":"1","attributes":{"locale":"no","x":"yes"}}]}"#)
            .expect("should parse");
        assert_eq!(string_at(&body, 0, "locale"), Some("no".to_string()));
        assert_eq!(string_at(&body, 0, "x"), Some("yes".to_string()));
    }

    #[test]
    fn builds_a_patch_body_with_escaped_values() {
        let body = patch_body(
            "appStoreVersionLocalizations",
            "abc",
            &[("whatsNew", "Line one\nLine \"two\"".to_string())],
        );
        assert_eq!(
            body,
            r#"{"data":{"type":"appStoreVersionLocalizations","id":"abc","attributes":{"whatsNew":"Line one\nLine \"two\""}}}"#
        );
        // And what it produced is still JSON.
        parse(&body).expect("the body should be valid JSON");
    }

    #[test]
    fn reports_apples_error_rather_than_the_whole_envelope() {
        let body = r#"{"errors":[{"status":"409","title":"The provided entity is in conflict","detail":"The version is not editable"}]}"#;
        assert_eq!(
            first_error(body),
            "The provided entity is in conflict: The version is not editable"
        );
    }

    #[test]
    fn falls_back_to_the_body_when_there_is_no_error_detail() {
        assert_eq!(first_error("not json at all"), "not json at all");
    }

    #[test]
    fn reads_a_metadata_directory_the_way_fastlane_lays_it_out() {
        let root = std::env::temp_dir().join(format!("shlane-appstore-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("en-US")).expect("fixture");
        std::fs::create_dir_all(root.join("de-DE")).expect("fixture");
        std::fs::write(root.join("en-US/description.txt"), "An app.\n").expect("fixture");
        std::fs::write(root.join("en-US/release_notes.txt"), "Fixed things.\n").expect("fixture");
        std::fs::write(root.join("en-US/name.txt"), "My App\n").expect("fixture");
        std::fs::write(root.join("de-DE/description.txt"), "Eine App.\n").expect("fixture");

        let (found, skipped) = metadata_from_dir(&root).expect("should read");

        assert_eq!(
            found["en-US"],
            vec![
                ("description", "An app.".to_string()),
                ("whatsNew", "Fixed things.".to_string()),
            ]
        );
        assert_eq!(
            found["de-DE"],
            vec![("description", "Eine App.".to_string())]
        );
        // name.txt belongs to the app, not to this version.
        assert_eq!(skipped, vec!["en-US/name.txt".to_string()]);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reports_a_metadata_directory_that_is_not_there() {
        let error = metadata_from_dir(Path::new("/definitely/not/here")).expect_err("should fail");
        assert!(error.contains("cannot read"), "{error}");
    }

    #[test]
    fn encodes_a_filter_value() {
        assert_eq!(urlencode("com.example.app"), "com.example.app");
        assert_eq!(urlencode("1.4.2+build"), "1.4.2%2Bbuild");
    }
}
