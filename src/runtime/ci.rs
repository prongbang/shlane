//! Recognising the CI that is running shlane (`docs/plan/11-ci-integration.md`).

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    GitHub,
    GitLab,
    Bitrise,
    CircleCi,
    Jenkins,
    Buildkite,
    Travis,
    TeamCity,
    AzurePipelines,
    /// Something that sets `CI` without saying what it is.
    Unknown,
}

impl Provider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::GitLab => "gitlab",
            Self::Bitrise => "bitrise",
            Self::CircleCi => "circleci",
            Self::Jenkins => "jenkins",
            Self::Buildkite => "buildkite",
            Self::Travis => "travis",
            Self::TeamCity => "teamcity",
            Self::AzurePipelines => "azure",
            Self::Unknown => "unknown",
        }
    }
}

/// The variable each provider is known by, most specific first.
const SIGNATURES: [(&str, Provider); 9] = [
    ("GITHUB_ACTIONS", Provider::GitHub),
    ("GITLAB_CI", Provider::GitLab),
    ("BITRISE_IO", Provider::Bitrise),
    ("CIRCLECI", Provider::CircleCi),
    ("JENKINS_URL", Provider::Jenkins),
    ("BUILDKITE", Provider::Buildkite),
    ("TRAVIS", Provider::Travis),
    ("TEAMCITY_VERSION", Provider::TeamCity),
    ("TF_BUILD", Provider::AzurePipelines),
];

/// Which CI this is, if any.
pub fn detect(env: &BTreeMap<String, String>) -> Option<Provider> {
    for (name, provider) in SIGNATURES {
        if is_set(env, name) {
            return Some(provider);
        }
    }
    // `CI=false` is set deliberately by people who want to look local.
    is_set(env, "CI").then_some(Provider::Unknown)
}

fn is_set(env: &BTreeMap<String, String>, name: &str) -> bool {
    match env.get(name) {
        Some(value) => !value.is_empty() && value != "false" && value != "0",
        None => false,
    }
}

/// A line the CI turns into an annotation on the job, if it knows how.
///
/// GitHub shows these on the pull request itself, which is where someone
/// looking at a red build actually is.
pub fn annotation(provider: Provider, level: &str, message: &str) -> Option<String> {
    match provider {
        Provider::GitHub => Some(format!(
            "::{level} title=shlane::{}",
            escape_annotation(message)
        )),
        // Other providers have no equivalent that is worth the noise.
        _ => None,
    }
}

/// GitHub reads `%0A` as a newline inside an annotation, and a bare `%` would
/// be read as the start of one of these escapes.
fn escape_annotation(message: &str) -> String {
    message
        .replace('%', "%25")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn recognises_the_usual_providers() {
        assert_eq!(
            detect(&env(&[("CI", "true"), ("GITHUB_ACTIONS", "true")])),
            Some(Provider::GitHub)
        );
        assert_eq!(
            detect(&env(&[("GITLAB_CI", "true")])),
            Some(Provider::GitLab)
        );
        assert_eq!(
            detect(&env(&[("BITRISE_IO", "true")])),
            Some(Provider::Bitrise)
        );
        assert_eq!(
            detect(&env(&[("JENKINS_URL", "http://x")])),
            Some(Provider::Jenkins)
        );
    }

    #[test]
    fn a_bare_ci_variable_is_still_ci() {
        assert_eq!(detect(&env(&[("CI", "1")])), Some(Provider::Unknown));
    }

    #[test]
    fn nothing_set_is_not_ci() {
        assert_eq!(detect(&env(&[])), None);
        assert_eq!(detect(&env(&[("HOME", "/root")])), None);
    }

    #[test]
    fn ci_turned_off_deliberately_is_respected() {
        assert_eq!(detect(&env(&[("CI", "false")])), None);
        assert_eq!(detect(&env(&[("CI", "")])), None);
        assert_eq!(detect(&env(&[("CI", "0")])), None);
    }

    #[test]
    fn github_gets_an_annotation() {
        let line = annotation(Provider::GitHub, "error", "step 'build' failed").expect("a line");
        assert_eq!(line, "::error title=shlane::step 'build' failed");
    }

    #[test]
    fn an_annotation_keeps_its_newlines_and_percents() {
        let line =
            annotation(Provider::GitHub, "error", "first\nsecond 100% done").expect("a line");
        assert!(line.contains("%0A"), "{line}");
        assert!(line.contains("100%25"), "{line}");
        assert!(!line.contains('\n'), "{line}");
    }

    #[test]
    fn providers_without_annotations_get_none() {
        assert!(annotation(Provider::GitLab, "error", "x").is_none());
        assert!(annotation(Provider::Unknown, "error", "x").is_none());
    }

    #[test]
    fn the_more_specific_provider_wins_over_a_bare_ci() {
        assert_eq!(
            detect(&env(&[("CI", "true"), ("BUILDKITE", "true")])),
            Some(Provider::Buildkite)
        );
    }
}
