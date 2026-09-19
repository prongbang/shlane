//! What each fastlane action becomes (`docs/plan/12-migration-from-fastlane.md`).

/// A fastlane action, the shlane action that replaces it, and the arguments
/// whose names differ.
pub struct Mapping {
    pub fastlane: &'static str,
    pub shlane: &'static str,
    /// `(fastlane name, shlane name)` for arguments that were renamed.
    pub renames: &'static [(&'static str, &'static str)],
    /// Arguments to add that fastlane expressed by using a different action.
    pub add: &'static [(&'static str, &'static str)],
}

const MAPPINGS: &[Mapping] = &[
    Mapping {
        fastlane: "deliver",
        shlane: "appstore",
        renames: &[
            ("app_identifier", "bundle_id"),
            ("metadata_path", "metadata_dir"),
        ],
        add: &[],
    },
    Mapping {
        fastlane: "upload_to_app_store",
        shlane: "appstore",
        renames: &[
            ("app_identifier", "bundle_id"),
            ("metadata_path", "metadata_dir"),
        ],
        add: &[],
    },
    Mapping {
        fastlane: "setup_ci",
        shlane: "setup_ci",
        renames: &[("keychain_name", "keychain_name"), ("timeout", "timeout")],
        add: &[],
    },
    Mapping {
        fastlane: "gym",
        shlane: "build_ios",
        renames: &[("export_method", "export_method"), ("team_id", "team_id")],
        add: &[],
    },
    Mapping {
        fastlane: "build_app",
        shlane: "build_ios",
        renames: &[],
        add: &[],
    },
    Mapping {
        fastlane: "build_ios_app",
        shlane: "build_ios",
        renames: &[],
        add: &[],
    },
    Mapping {
        fastlane: "scan",
        shlane: "test_ios",
        renames: &[("devices", "destination")],
        add: &[],
    },
    Mapping {
        fastlane: "run_tests",
        shlane: "test_ios",
        renames: &[("devices", "destination")],
        add: &[],
    },
    Mapping {
        fastlane: "pilot",
        shlane: "testflight",
        renames: &[("ipa", "ipa")],
        add: &[],
    },
    Mapping {
        fastlane: "upload_to_testflight",
        shlane: "testflight",
        renames: &[],
        add: &[],
    },
    Mapping {
        fastlane: "gradle",
        shlane: "gradle",
        renames: &[("project_dir", "project_dir"), ("flags", "flags")],
        add: &[],
    },
    Mapping {
        fastlane: "supply",
        shlane: "play_store",
        renames: &[
            ("package_name", "package_name"),
            ("aab", "aab"),
            ("apk", "apk"),
            ("track", "track"),
            ("json_key", "service_account_json"),
            ("json_key_data", "service_account_json"),
        ],
        add: &[],
    },
    Mapping {
        fastlane: "upload_to_play_store",
        shlane: "play_store",
        renames: &[
            ("json_key", "service_account_json"),
            ("json_key_data", "service_account_json"),
        ],
        add: &[],
    },
    Mapping {
        fastlane: "firebase_app_distribution",
        shlane: "firebase_distribution",
        renames: &[("app", "app_id"), ("release_notes", "release_notes")],
        add: &[],
    },
    Mapping {
        fastlane: "increment_build_number",
        shlane: "bump_version",
        renames: &[],
        add: &[("part", "build")],
    },
    Mapping {
        fastlane: "increment_version_number",
        shlane: "bump_version",
        renames: &[("bump_type", "part")],
        add: &[],
    },
    Mapping {
        fastlane: "get_version_number",
        shlane: "read_version",
        renames: &[],
        add: &[],
    },
    Mapping {
        fastlane: "ensure_git_status_clean",
        shlane: "git_status_clean",
        renames: &[],
        add: &[],
    },
    Mapping {
        fastlane: "git_commit",
        shlane: "git_commit",
        renames: &[("path", "paths"), ("message", "message")],
        add: &[],
    },
    Mapping {
        fastlane: "add_git_tag",
        shlane: "git_tag",
        renames: &[("tag", "name")],
        add: &[],
    },
    Mapping {
        fastlane: "push_to_git_remote",
        shlane: "git_push",
        renames: &[("remote", "remote"), ("local_branch", "branch")],
        add: &[],
    },
    Mapping {
        fastlane: "push_git_tags",
        shlane: "git_push",
        renames: &[],
        add: &[("tags", "true")],
    },
    Mapping {
        fastlane: "changelog_from_git_commits",
        shlane: "changelog_from_commits",
        renames: &[("between", "from"), ("pretty", "format")],
        add: &[],
    },
    Mapping {
        fastlane: "last_git_tag",
        shlane: "last_git_tag",
        renames: &[],
        add: &[],
    },
    Mapping {
        fastlane: "git_branch",
        shlane: "git_branch",
        renames: &[],
        add: &[],
    },
    Mapping {
        fastlane: "slack",
        shlane: "notify_slack",
        renames: &[("message", "text"), ("slack_url", "webhook")],
        add: &[],
    },
    Mapping {
        fastlane: "create_keychain",
        shlane: "keychain",
        renames: &[("name", "name"), ("password", "password")],
        add: &[("action", "create")],
    },
    Mapping {
        fastlane: "unlock_keychain",
        shlane: "keychain",
        renames: &[("path", "name"), ("password", "password")],
        add: &[("action", "unlock")],
    },
    Mapping {
        fastlane: "delete_keychain",
        shlane: "keychain",
        renames: &[("name", "name")],
        add: &[("action", "delete")],
    },
];

pub fn lookup(fastlane: &str) -> Option<&'static Mapping> {
    MAPPINGS.iter().find(|mapping| mapping.fastlane == fastlane)
}

/// Actions with no equivalent, and what to say about each.
const UNSUPPORTED: &[(&[&str], &str)] = &[
    (
        &["match", "sync_code_signing"],
        "codesign_sync reads an existing match repository; it never creates or revokes certificates, so move this by hand",
    ),
    (
        &["sigh", "get_provisioning_profile"],
        "provisioning_profile downloads an existing profile; it never creates one, so move this by hand",
    ),
    (
        &["cert", "get_certificates"],
        "certificate downloads an existing certificate; it never creates one, so move this by hand",
    ),
    (
        &["snapshot", "screengrab", "frameit", "precheck", "produce", "pem"],
        "no equivalent; keep using a `run:` step for this",
    ),
];

pub fn unsupported(name: &str) -> Option<&'static str> {
    UNSUPPORTED
        .iter()
        .find(|(names, _)| names.contains(&name))
        .map(|(_, reason)| *reason)
}

/// The generated part of `docs/migration.md`: every mapping and every action
/// with no equivalent, straight from the tables `shlane migrate` uses.
#[cfg(test)]
fn markdown_table() -> String {
    let mut mappings: Vec<&Mapping> = MAPPINGS.iter().collect();
    mappings.sort_by_key(|mapping| mapping.fastlane);

    let mut out = String::from("## Actions\n\n| fastlane | shlane | Arguments |\n|---|---|---|\n");
    for mapping in mappings {
        let mut notes: Vec<String> = mapping
            .renames
            .iter()
            .filter(|(from, to)| from != to)
            .map(|(from, to)| format!("`{from}` → `{to}`"))
            .collect();
        notes.extend(
            mapping
                .add
                .iter()
                .map(|(key, value)| format!("adds `{key}: {value}`")),
        );
        out.push_str(&format!(
            "| `{}` | `{}` | {} |\n",
            mapping.fastlane,
            mapping.shlane,
            notes.join(", ")
        ));
    }

    out.push_str("\n## Moved by hand\n\n| fastlane | Why |\n|---|---|\n");
    for (names, reason) in UNSUPPORTED {
        let names: Vec<String> = names.iter().map(|name| format!("`{name}`")).collect();
        out.push_str(&format!(
            "| {} | {} |\n",
            names.join(", "),
            reason.replace('|', "\\|")
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_the_common_actions() {
        assert_eq!(lookup("gym").map(|m| m.shlane), Some("build_ios"));
        assert_eq!(lookup("supply").map(|m| m.shlane), Some("play_store"));
        assert_eq!(lookup("slack").map(|m| m.shlane), Some("notify_slack"));
        assert!(lookup("nonexistent_action").is_none());
    }

    #[test]
    fn renames_arguments_that_changed_name() {
        let slack = lookup("slack").expect("slack is mapped");
        assert!(slack.renames.contains(&("message", "text")));
        assert!(slack.renames.contains(&("slack_url", "webhook")));
    }

    #[test]
    fn expresses_what_fastlane_said_with_a_separate_action() {
        let tags = lookup("push_git_tags").expect("mapped");
        assert_eq!(tags.shlane, "git_push");
        assert!(tags.add.contains(&("tags", "true")));
    }

    #[test]
    fn explains_what_has_no_equivalent() {
        assert!(unsupported("match").is_some());
        assert!(unsupported("snapshot").is_some());
        assert!(unsupported("gym").is_none());
        // deliver has an action now, so it is mapped rather than explained away.
        assert!(unsupported("deliver").is_none());
        assert_eq!(lookup("deliver").expect("mapped").shlane, "appstore");
    }

    /// `docs/migration.md` is generated from the tables above, so it cannot
    /// drift from what `shlane migrate` does. `UPDATE_DOCS=1 cargo test`
    /// rewrites it.
    #[test]
    fn the_migration_doc_matches_the_tables() {
        const BEGIN: &str =
            "<!-- BEGIN GENERATED: cargo test updates this, do not edit by hand -->\n";
        const END: &str = "<!-- END GENERATED -->";
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/migration.md");
        let doc = std::fs::read_to_string(&path).expect("docs/migration.md exists");
        let start = doc.find(BEGIN).expect("begin marker") + BEGIN.len();
        let end = doc.find(END).expect("end marker");
        let expected = format!("\n{}\n", markdown_table());

        if doc[start..end] == expected {
            return;
        }
        if std::env::var_os("UPDATE_DOCS").is_some() {
            let updated = format!("{}{expected}{}", &doc[..start], &doc[end..]);
            std::fs::write(&path, updated).expect("write docs/migration.md");
            return;
        }
        panic!("docs/migration.md is out of date; run `UPDATE_DOCS=1 cargo test` and commit it");
    }

    #[test]
    fn every_mapping_points_at_an_action_that_exists() {
        let registry = crate::actions::Registry::builtins();
        for mapping in MAPPINGS {
            assert!(
                registry.find(mapping.shlane).is_some(),
                "{} maps to '{}', which is not a real action",
                mapping.fastlane,
                mapping.shlane
            );
        }
    }
}
