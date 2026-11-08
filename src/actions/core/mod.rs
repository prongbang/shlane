//! Actions that are not tied to a platform (`docs/plan/06-actions-core.md`).

pub mod android;
pub mod appstore;
pub mod ci;
pub mod files;
pub mod firebase;
pub mod git;
pub mod http;
pub mod ios;
pub mod play;
pub mod shell;
pub mod version;

use super::Action;

const MAX_KEYCHAIN_TIMEOUT_SECONDS: u64 = 86_400;

pub fn timeout_seconds(value: &str) -> Option<String> {
    let seconds = value.parse::<u64>().ok()?;
    (1..=MAX_KEYCHAIN_TIMEOUT_SECONDS)
        .contains(&seconds)
        .then(|| seconds.to_string())
}

pub fn all() -> Vec<Box<dyn Action>> {
    vec![
        Box::new(shell::Sh),
        Box::new(shell::EnsureEnvVars),
        Box::new(shell::WhichTool),
        Box::new(git::GitStatusClean),
        Box::new(git::GitBranch),
        Box::new(git::GitCommit),
        Box::new(git::GitTag),
        Box::new(git::GitPush),
        Box::new(git::GitPull),
        Box::new(git::LastGitTag),
        Box::new(git::ChangelogFromCommits),
        Box::new(version::ReadVersion),
        Box::new(version::BumpVersion),
        Box::new(android::Gradle),
        Box::new(android::BuildAndroid),
        Box::new(android::TestAndroid),
        Box::new(android::SignAndroid),
        Box::new(crate::actions::codesign::sync::CodesignSync),
        Box::new(crate::actions::codesign::fetch::ProvisioningProfile),
        Box::new(crate::actions::codesign::fetch::Certificate),
        Box::new(crate::actions::codesign::fetch::XcodeSettings),
        Box::new(ios::BuildIos),
        Box::new(ios::TestIos),
        Box::new(ios::Keychain),
        Box::new(ios::TestFlight),
        Box::new(ios::AscApiKey),
        Box::new(ios::AscRequest),
        Box::new(appstore::AppStore),
        Box::new(ci::SetupCi),
        Box::new(play::PlayStore),
        Box::new(firebase::FirebaseDistribution),
        Box::new(http::HttpRequest),
        Box::new(files::Zip),
        Box::new(files::Unzip),
        Box::new(files::CopyArtifacts),
        Box::new(files::CleanBuildArtifacts),
        Box::new(files::Download),
        Box::new(files::TemplateRender),
        Box::new(http::NotifySlack),
        Box::new(http::NotifyDiscord),
        Box::new(http::NotifyTeams),
    ]
}

#[cfg(test)]
mod tests {
    use super::timeout_seconds;

    #[test]
    fn keychain_timeout_accepts_only_bounded_positive_integers() {
        assert_eq!(timeout_seconds("3600"), Some("3600".to_string()));
        assert_eq!(timeout_seconds("86400"), Some("86400".to_string()));
        assert_eq!(timeout_seconds("0"), None);
        assert_eq!(timeout_seconds("86401"), None);
        assert_eq!(timeout_seconds("10; echo injected"), None);
    }
}
