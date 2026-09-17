//! Actions that are not tied to a platform (`docs/plan/06-actions-core.md`).

pub mod android;
pub mod firebase;
pub mod git;
pub mod http;
pub mod play;
pub mod shell;
pub mod version;

use super::Action;

pub fn all() -> Vec<Box<dyn Action>> {
    vec![
        Box::new(shell::Sh),
        Box::new(shell::EnsureEnvVars),
        Box::new(git::GitStatusClean),
        Box::new(git::GitBranch),
        Box::new(git::GitCommit),
        Box::new(git::GitTag),
        Box::new(git::GitPush),
        Box::new(git::LastGitTag),
        Box::new(git::ChangelogFromCommits),
        Box::new(version::ReadVersion),
        Box::new(version::BumpVersion),
        Box::new(android::Gradle),
        Box::new(android::BuildAndroid),
        Box::new(android::TestAndroid),
        Box::new(android::SignAndroid),
        Box::new(play::PlayStore),
        Box::new(firebase::FirebaseDistribution),
        Box::new(http::HttpRequest),
        Box::new(http::NotifySlack),
    ]
}
