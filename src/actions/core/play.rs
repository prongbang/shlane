//! Google Play publishing (`docs/plan/08-actions-android.md`).
//!
//! The Publishing API works in "edits": open one, upload into it, point a
//! track at what was uploaded, then commit. Nothing is visible until the
//! commit, so a failure part-way through leaves the store untouched.
//!
//! The request shapes here are unit-tested; the round trip against Google is
//! not, and needs a real service account (`docs/plan/13-testing-and-quality.md`).

use crate::actions::context::ActionContext;
use crate::actions::core::http::{send, Payload};
use crate::actions::google::{assertion, token_request_body, ServiceAccount, TokenResponse};
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::Result;
use serde::Deserialize;
use std::fs;

const SCOPE: &str = "https://www.googleapis.com/auth/androidpublisher";
const API: &str = "https://androidpublisher.googleapis.com/androidpublisher/v3/applications";
const UPLOAD: &str =
    "https://androidpublisher.googleapis.com/upload/androidpublisher/v3/applications";

#[derive(Debug, Deserialize)]
struct Edit {
    id: String,
}

#[derive(Debug, Deserialize)]
struct Uploaded {
    #[serde(rename = "versionCode")]
    version_code: i64,
}

fn edits_url(package: &str) -> String {
    format!("{API}/{package}/edits")
}

fn upload_url(package: &str, edit: &str, kind: &str) -> String {
    format!("{UPLOAD}/{package}/edits/{edit}/{kind}?uploadType=media")
}

fn track_url(package: &str, edit: &str, track: &str) -> String {
    format!("{API}/{package}/edits/{edit}/tracks/{track}")
}

fn commit_url(package: &str, edit: &str) -> String {
    format!("{API}/{package}/edits/{edit}:commit")
}

/// The body that points a track at what was just uploaded.
fn track_body(
    track: &str,
    version_code: i64,
    status: &str,
    rollout: Option<f64>,
    notes: Option<(&str, &str)>,
) -> String {
    use crate::actions::core::http::escape;

    let mut release = format!("\"versionCodes\":[\"{version_code}\"],\"status\":\"{status}\"");
    if let Some(fraction) = rollout {
        release.push_str(&format!(",\"userFraction\":{fraction}"));
    }
    if let Some((language, text)) = notes {
        release.push_str(&format!(
            ",\"releaseNotes\":[{{\"language\":\"{}\",\"text\":\"{}\"}}]",
            escape(language),
            escape(text)
        ));
    }

    format!(
        "{{\"track\":\"{}\",\"releases\":[{{{release}}}]}}",
        escape(track)
    )
}

pub struct PlayStore;

impl Action for PlayStore {
    fn name(&self) -> &'static str {
        "play_store"
    }

    fn description(&self) -> &'static str {
        "Upload a build to Google Play"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("package_name", "Application id, e.g. com.example.app").required(),
            ArgSpec::new(
                "service_account_json",
                "The key JSON, base64 JSON, or a path to it",
            )
            .required()
            .sensitive(),
            ArgSpec::new("aab", "App bundle to upload"),
            ArgSpec::new("apk", "APK to upload, if not using a bundle"),
            ArgSpec::new("track", "internal, alpha, beta or production").default("internal"),
            ArgSpec::new("status", "completed, draft, inProgress or halted").default("completed"),
            ArgSpec::new(
                "rollout",
                "Fraction of users for a staged rollout, e.g. 0.1",
            ),
            ArgSpec::new("release_notes", "What changed"),
            ArgSpec::new("language", "Language of the release notes").default("en-US"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let package = args.get_or("package_name", "");
        let track = args.get_or("track", "internal");
        let status = args.get_or("status", "completed");

        let (kind, file) = match (args.get("aab"), args.get("apk")) {
            (Some(path), _) => ("bundles", path),
            (None, Some(path)) => ("apks", path),
            (None, None) => return Err(ctx.error(self.name(), "give either aab or apk")),
        };
        let path = ctx.workdir().join(file);

        let rollout = match args.get("rollout") {
            Some(value) => Some(value.parse::<f64>().map_err(|_| {
                ctx.error(self.name(), format!("rollout '{value}' is not a number"))
            })?),
            None => None,
        };

        if ctx.dry_run {
            ctx.ui.say(&format!(
                "Would upload {} to {package} on the {track} track ({status})",
                path.display()
            ));
            return Ok(ActionOutput::new().with("track", track));
        }

        if !path.is_file() {
            return Err(ctx.error(self.name(), format!("{} does not exist", path.display())));
        }

        let account = ServiceAccount::load(args.get_or("service_account_json", ""), ctx.workdir())
            .map_err(|message| ctx.error(self.name(), message))?;
        let token = access_token(ctx, self.name(), &account)?;
        let auth = vec![
            ("Authorization".to_string(), format!("Bearer {token}")),
            ("Content-Type".to_string(), "application/json".to_string()),
        ];

        ctx.ui.say(&format!("Opening an edit for {package}"));
        let edit: Edit = call(
            ctx,
            self.name(),
            "POST",
            &edits_url(package),
            &auth,
            Payload::Empty,
        )?;

        let bytes = fs::read(&path).map_err(|err| {
            ctx.error(
                self.name(),
                format!("cannot read {}: {err}", path.display()),
            )
        })?;
        ctx.ui.say(&format!(
            "Uploading {} ({:.1} MB)",
            path.display(),
            bytes.len() as f64 / 1_048_576.0
        ));

        let upload_headers = vec![
            ("Authorization".to_string(), format!("Bearer {token}")),
            (
                "Content-Type".to_string(),
                "application/octet-stream".to_string(),
            ),
        ];
        let uploaded: Uploaded = call(
            ctx,
            self.name(),
            "POST",
            &upload_url(package, &edit.id, kind),
            &upload_headers,
            Payload::Bytes(&bytes),
        )?;

        ctx.ui.say(&format!(
            "Version code {} to the {track} track",
            uploaded.version_code
        ));
        let body = track_body(
            track,
            uploaded.version_code,
            status,
            rollout,
            args.get("release_notes")
                .map(|notes| (args.get_or("language", "en-US"), notes)),
        );
        let _: serde_yaml::Value = call(
            ctx,
            self.name(),
            "PUT",
            &track_url(package, &edit.id, track),
            &auth,
            Payload::Text(&body),
        )?;

        let _: serde_yaml::Value = call(
            ctx,
            self.name(),
            "POST",
            &commit_url(package, &edit.id),
            &auth,
            Payload::Empty,
        )?;

        ctx.ui.say("Committed");
        Ok(ActionOutput::new()
            .with("version_code", uploaded.version_code.to_string())
            .with("track", track)
            .with("edit_id", edit.id))
    }
}

/// Exchange the service account for an access token.
fn access_token(ctx: &ActionContext<'_>, action: &str, account: &ServiceAccount) -> Result<String> {
    let assertion = assertion(account, SCOPE).map_err(|message| ctx.error(action, message))?;

    let response = send(
        ctx,
        "POST",
        account.token_uri(),
        &[(
            "Content-Type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        )],
        Payload::Text(&token_request_body(&assertion)),
    )
    .map_err(|message| ctx.error(action, format!("could not reach Google: {message}")))?;

    if !(200..300).contains(&response.status) {
        return Err(ctx.error(
            action,
            format!(
                "Google refused the service account (HTTP {}): {}",
                response.status,
                response.body.trim()
            ),
        ));
    }

    let token: TokenResponse = serde_yaml::from_str(&response.body)
        .map_err(|err| ctx.error(action, format!("unexpected token response: {err}")))?;

    // The token is a bearer credential for the whole account.
    ctx.mark_secret(&token.access_token);
    Ok(token.access_token)
}

/// One API call, with its response parsed.
fn call<T: for<'de> Deserialize<'de>>(
    ctx: &ActionContext<'_>,
    action: &str,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Payload<'_>,
) -> Result<T> {
    let response = send(ctx, method, url, headers, body)
        .map_err(|message| ctx.error(action, format!("{method} {url} failed: {message}")))?;

    if !(200..300).contains(&response.status) {
        return Err(ctx.error(
            action,
            format!(
                "{method} {url} returned HTTP {}: {}",
                response.status,
                response.body.trim().chars().take(500).collect::<String>()
            ),
        ));
    }

    // An empty body is valid for a commit; `null` parses into what the caller
    // asked for when that is a Value.
    let text = if response.body.trim().is_empty() {
        "null".to_string()
    } else {
        response.body
    };

    serde_yaml::from_str(&text)
        .map_err(|err| ctx.error(action, format!("unexpected response from {url}: {err}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_the_api_urls() {
        assert_eq!(
            edits_url("com.example.app"),
            "https://androidpublisher.googleapis.com/androidpublisher/v3/applications/com.example.app/edits"
        );
        assert!(upload_url("com.example.app", "42", "bundles")
            .ends_with("/edits/42/bundles?uploadType=media"));
        assert!(
            track_url("com.example.app", "42", "internal").ends_with("/edits/42/tracks/internal")
        );
        assert!(commit_url("com.example.app", "42").ends_with("/edits/42:commit"));
    }

    #[test]
    fn a_plain_release_body() {
        let body = track_body("internal", 41, "completed", None, None);
        assert_eq!(
            body,
            r#"{"track":"internal","releases":[{"versionCodes":["41"],"status":"completed"}]}"#
        );
    }

    #[test]
    fn a_staged_rollout_with_notes() {
        let body = track_body(
            "production",
            7,
            "inProgress",
            Some(0.1),
            Some(("en-US", "Fixed \"the\" bug")),
        );
        assert!(body.contains(r#""userFraction":0.1"#), "{body}");
        assert!(body.contains(r#""status":"inProgress""#), "{body}");
        assert!(body.contains(r#""language":"en-US""#), "{body}");
        assert!(body.contains(r#"Fixed \"the\" bug"#), "{body}");
    }

    #[test]
    fn the_body_is_valid_json() {
        let body = track_body("beta", 3, "draft", Some(0.5), Some(("th", "ทดสอบ")));
        let parsed: serde_yaml::Value = serde_yaml::from_str(&body).expect("valid JSON");
        assert_eq!(parsed["track"].as_str(), Some("beta"));
        assert_eq!(
            parsed["releases"][0]["releaseNotes"][0]["text"].as_str(),
            Some("ทดสอบ")
        );
    }
}
