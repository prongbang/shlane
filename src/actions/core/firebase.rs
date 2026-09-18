//! Firebase App Distribution.
//!
//! This one wraps the `firebase` CLI rather than calling the REST API. The
//! upload endpoint returns a long-running operation that has to be polled, and
//! an action that cannot be tested against the real service is worth less than
//! one that delegates to the tool Google maintains. `docs/plan/08-actions-android.md`
//! keeps the REST version as future work.

use crate::actions::context::ActionContext;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::Result;
use std::collections::BTreeMap;

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

/// Build the CLI invocation.
fn command(binary: &str, file: &str, app_id: &str, args: &Args) -> String {
    let mut parts = vec![
        binary.to_string(),
        "appdistribution:distribute".to_string(),
        quote(file),
        "--app".to_string(),
        quote(app_id),
    ];

    if let Some(groups) = args.get("groups") {
        parts.push("--groups".to_string());
        parts.push(quote(groups));
    }
    if let Some(testers) = args.get("testers") {
        parts.push("--testers".to_string());
        parts.push(quote(testers));
    }
    if let Some(notes) = args.get("release_notes") {
        parts.push("--release-notes".to_string());
        parts.push(quote(notes));
    }

    parts.join(" ")
}

pub struct FirebaseDistribution;

impl Action for FirebaseDistribution {
    fn name(&self) -> &'static str {
        "firebase_distribution"
    }

    fn description(&self) -> &'static str {
        "Distribute a build through Firebase App Distribution (needs the firebase CLI)"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("file", "APK, AAB or IPA to distribute").required(),
            ArgSpec::new("app_id", "Firebase app id, e.g. 1:123:android:abc").required(),
            ArgSpec::new("service_account_json", "Path to the key the CLI should use").sensitive(),
            ArgSpec::new("groups", "Comma-separated tester groups"),
            ArgSpec::new("testers", "Comma-separated tester emails"),
            ArgSpec::new("release_notes", "What changed"),
            ArgSpec::new("binary", "The firebase CLI to call").default("firebase"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let file = args.get_or("file", "");
        let path = ctx.workdir().join(file);
        let command = command(
            args.get_or("binary", "firebase"),
            &path.display().to_string(),
            args.get_or("app_id", ""),
            args,
        );

        // The CLI reads its credentials from the environment, so the path never
        // reaches the command line.
        let mut env = BTreeMap::new();
        if let Some(key) = args.get("service_account_json") {
            env.insert(
                "GOOGLE_APPLICATION_CREDENTIALS".to_string(),
                ctx.workdir().join(key).display().to_string(),
            );
        }

        if ctx.dry_run {
            ctx.ui.say(&format!("Would run: {command}"));
            return Ok(ActionOutput::new().with("file", path.display().to_string()));
        }

        if !path.is_file() {
            return Err(ctx.error(self.name(), format!("{} does not exist", path.display())));
        }

        ctx.require_with_env(&command, &env)?;
        Ok(ActionOutput::new().with("file", path.display().to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn args(pairs: &[(&str, &str)]) -> Args {
        Args::new(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect::<BTreeMap<String, String>>(),
        )
    }

    #[test]
    fn builds_the_minimal_invocation() {
        let command = command("firebase", "app.apk", "1:2:android:3", &args(&[]));
        assert_eq!(
            command,
            "firebase appdistribution:distribute 'app.apk' --app '1:2:android:3'"
        );
    }

    #[test]
    fn adds_groups_testers_and_notes() {
        let command = command(
            "firebase",
            "app.aab",
            "1:2:android:3",
            &args(&[
                ("groups", "qa,beta"),
                ("testers", "a@example.com"),
                ("release_notes", "it's fixed"),
            ]),
        );
        assert!(command.contains("--groups 'qa,beta'"), "{command}");
        assert!(command.contains("--testers 'a@example.com'"), "{command}");
        assert!(
            command.contains(r"--release-notes 'it'\''s fixed'"),
            "{command}"
        );
    }
}
