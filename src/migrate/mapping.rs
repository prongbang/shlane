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
pub fn unsupported(name: &str) -> Option<&'static str> {
    match name {
        "match" | "sync_code_signing" => Some(
            "shlane has no synced certificate store yet; build_ios uses Xcode's -allowProvisioningUpdates with an App Store Connect key",
        ),
        "sigh" | "get_provisioning_profile" | "cert" | "get_certificates" => {
            Some("signing is handled by Xcode via -allowProvisioningUpdates; there is no direct equivalent")
        }
        "snapshot" | "screengrab" | "frameit" | "precheck" | "produce" | "pem" => {
            Some("no equivalent; keep using a `run:` step for this")
        }
        _ => None,
    }
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
