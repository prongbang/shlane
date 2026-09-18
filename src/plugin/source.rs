//! Where a plugin comes from (`docs/plan/09-plugins.md`).
//!
//! `github:owner/repo@v1.2.3`, a git URL, or a local path. The reference is
//! kept separate because a plugin without one floats: the tag it was installed
//! from can be moved later, which is the whole reason the lockfile exists.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub url: String,
    /// Tag, branch or commit. `None` means the repository's default branch.
    pub reference: Option<String>,
}

impl Source {
    /// True when nothing pins what will be fetched.
    pub fn is_floating(&self) -> bool {
        self.reference.is_none()
    }
}

pub fn parse(spec: &str) -> Result<Source, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("the source is empty".to_string());
    }

    if let Some(rest) = spec.strip_prefix("github:") {
        let (path, reference) = split_reference(rest);
        let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
        if segments.len() != 2 {
            return Err(format!(
                "'{spec}' should look like github:owner/repo@v1.0.0"
            ));
        }
        return Ok(Source {
            url: format!("https://github.com/{}/{}.git", segments[0], segments[1]),
            reference,
        });
    }

    if let Some(rest) = spec.strip_prefix("git:") {
        let (url, reference) = split_reference(rest);
        return Ok(Source {
            url: url.to_string(),
            reference,
        });
    }

    if spec.starts_with("path:") || spec.starts_with('.') || spec.starts_with('/') {
        return Err(format!(
            "'{spec}' is a local plugin: use `path:` in the config rather than fetching it"
        ));
    }

    // A plain git URL, including scp-style `git@host:owner/repo.git`.
    if spec.contains("://") || spec.contains('@') && spec.contains(':') {
        let (url, reference) = split_reference_for_url(spec);
        return Ok(Source {
            url: url.to_string(),
            reference,
        });
    }

    Err(format!(
        "'{spec}' is not a source; use github:owner/repo@tag, a git URL, or `path:` for a local plugin"
    ))
}

fn split_reference(spec: &str) -> (&str, Option<String>) {
    match spec.rsplit_once('@') {
        Some((path, reference)) if !reference.is_empty() => (path, Some(reference.to_string())),
        _ => (spec, None),
    }
}

/// Like [`split_reference`], but an `@` that belongs to `git@host` is not a ref.
fn split_reference_for_url(spec: &str) -> (&str, Option<String>) {
    let Some((url, reference)) = spec.rsplit_once('@') else {
        return (spec, None);
    };
    // `git@github.com:owner/repo.git` has no reference; the @ is the user.
    if reference.contains('/') || reference.contains(':') || reference.is_empty() {
        return (spec, None);
    }
    (url, Some(reference.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_github_shorthand() {
        let source = parse("github:someone/shlane-line-notify@v0.1.0").expect("valid");
        assert_eq!(
            source.url,
            "https://github.com/someone/shlane-line-notify.git"
        );
        assert_eq!(source.reference.as_deref(), Some("v0.1.0"));
        assert!(!source.is_floating());
    }

    #[test]
    fn a_github_shorthand_without_a_tag_floats() {
        let source = parse("github:someone/repo").expect("valid");
        assert_eq!(source.reference, None);
        assert!(source.is_floating());
    }

    #[test]
    fn rejects_a_malformed_shorthand() {
        assert!(parse("github:justtheowner").is_err());
        assert!(parse("github:a/b/c").is_err());
    }

    #[test]
    fn reads_a_git_url() {
        let source = parse("git:https://example.com/plugins/notify.git@main").expect("valid");
        assert_eq!(source.url, "https://example.com/plugins/notify.git");
        assert_eq!(source.reference.as_deref(), Some("main"));
    }

    #[test]
    fn reads_a_plain_https_url() {
        let source = parse("https://example.com/notify.git").expect("valid");
        assert_eq!(source.url, "https://example.com/notify.git");
        assert_eq!(source.reference, None);
    }

    #[test]
    fn the_at_in_an_ssh_url_is_not_a_reference() {
        let source = parse("git@github.com:someone/repo.git").expect("valid");
        assert_eq!(source.url, "git@github.com:someone/repo.git");
        assert_eq!(source.reference, None);
    }

    #[test]
    fn an_ssh_url_can_still_carry_a_tag() {
        let source = parse("git@github.com:someone/repo.git@v2").expect("valid");
        assert_eq!(source.url, "git@github.com:someone/repo.git");
        assert_eq!(source.reference.as_deref(), Some("v2"));
    }

    #[test]
    fn a_local_path_is_sent_back_to_the_path_field() {
        let error = parse("./tools/notify").expect_err("should fail");
        assert!(error.contains("path:"), "{error}");
        assert!(parse("path:./tools/notify").is_err());
    }

    #[test]
    fn nonsense_is_reported() {
        assert!(parse("").is_err());
        assert!(parse("just-a-word").is_err());
    }
}
