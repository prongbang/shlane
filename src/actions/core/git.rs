//! Git actions.

use crate::actions::context::ActionContext;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::Result;

/// Quote a value for the shell, so a commit message or tag name containing
/// spaces or quotes cannot turn into extra arguments.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

pub struct GitStatusClean;

impl Action for GitStatusClean {
    fn name(&self) -> &'static str {
        "git_status_clean"
    }

    fn description(&self) -> &'static str {
        "Fail if the working tree has uncommitted changes"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::new("show", "Print the changed files when the tree is dirty").default("true")]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let status = ctx.capture("git status --porcelain")?;
        if status.trim().is_empty() {
            return Ok(ActionOutput::new().with("clean", "true"));
        }

        let detail = if args.flag("show") {
            format!("\n{status}")
        } else {
            String::new()
        };
        Err(ctx.error(
            self.name(),
            format!("the working tree has uncommitted changes{detail}"),
        ))
    }
}

pub struct GitBranch;

impl Action for GitBranch {
    fn name(&self) -> &'static str {
        "git_branch"
    }

    fn description(&self) -> &'static str {
        "Report the current branch"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        Vec::new()
    }

    fn run(&self, ctx: &mut ActionContext<'_>, _args: &Args) -> Result<ActionOutput> {
        let branch = ctx.capture("git rev-parse --abbrev-ref HEAD")?;
        let sha = ctx.capture("git rev-parse --short HEAD")?;
        ctx.ui.say(&format!("On branch {branch} ({sha})"));
        Ok(ActionOutput::new().with("name", branch).with("sha", sha))
    }
}

pub struct GitCommit;

impl Action for GitCommit {
    fn name(&self) -> &'static str {
        "git_commit"
    }

    fn description(&self) -> &'static str {
        "Commit changes"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("message", "Commit message").required(),
            ArgSpec::new("paths", "Paths to stage, space separated").default("."),
            ArgSpec::new(
                "allow_empty",
                "Commit even when nothing changed, instead of failing",
            )
            .default("false"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let message = args.get_or("message", "");
        let paths = args.get_or("paths", ".");

        if ctx.dry_run {
            ctx.ui.say(&format!(
                "Would stage {paths} and commit with message: {message}"
            ));
            return Ok(ActionOutput::new().with("sha", ""));
        }

        ctx.require(&format!("git add -- {paths}"))?;

        let staged = ctx.probe("git diff --cached --quiet")?;
        if staged.success && !args.flag("allow_empty") {
            // `git diff --cached --quiet` succeeds when there is nothing staged.
            return Err(ctx.error(
                self.name(),
                "nothing to commit; pass allow_empty: true if that is expected",
            ));
        }

        let empty = if args.flag("allow_empty") {
            " --allow-empty"
        } else {
            ""
        };
        ctx.require(&format!("git commit{empty} -m {}", quote(message)))?;

        let sha = ctx.capture("git rev-parse --short HEAD")?;
        Ok(ActionOutput::new().with("sha", sha))
    }
}

pub struct GitTag;

impl Action for GitTag {
    fn name(&self) -> &'static str {
        "git_tag"
    }

    fn description(&self) -> &'static str {
        "Create a tag"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("name", "Tag name").required(),
            ArgSpec::new("message", "Annotate the tag with this message"),
            ArgSpec::new("force", "Move the tag if it already exists").default("false"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let name = args.get_or("name", "");
        let force = if args.flag("force") { " -f" } else { "" };

        if ctx.dry_run {
            // The commit it would tag does not exist yet.
            ctx.ui.say(&format!("Would tag HEAD as {name}"));
            return Ok(ActionOutput::new().with("name", name));
        }

        let command = match args.get("message") {
            Some(message) => format!("git tag{force} -a {} -m {}", quote(name), quote(message)),
            None => format!("git tag{force} {}", quote(name)),
        };
        ctx.require(&command)?;

        Ok(ActionOutput::new().with("name", name))
    }
}

pub struct GitPush;

impl Action for GitPush {
    fn name(&self) -> &'static str {
        "git_push"
    }

    fn description(&self) -> &'static str {
        "Push commits, and optionally tags"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("remote", "Remote to push to").default("origin"),
            ArgSpec::new("branch", "Branch to push; defaults to the current one"),
            ArgSpec::new("tags", "Push tags as well").default("false"),
            ArgSpec::new("set_upstream", "Pass --set-upstream").default("false"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let remote = args.get_or("remote", "origin");
        let branch = match args.get("branch") {
            Some(branch) => branch.to_string(),
            None => ctx.capture("git rev-parse --abbrev-ref HEAD")?,
        };
        let upstream = if args.flag("set_upstream") {
            " --set-upstream"
        } else {
            ""
        };

        ctx.require(&format!(
            "git push{upstream} {} {}",
            quote(remote),
            quote(&branch)
        ))?;

        if args.flag("tags") {
            ctx.require(&format!("git push {} --tags", quote(remote)))?;
        }

        Ok(ActionOutput::new()
            .with("remote", remote)
            .with("branch", branch))
    }
}

pub struct LastGitTag;

impl Action for LastGitTag {
    fn name(&self) -> &'static str {
        "last_git_tag"
    }

    fn description(&self) -> &'static str {
        "Report the most recent tag, if there is one"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![ArgSpec::new(
            "pattern",
            "Only consider tags matching this glob",
        )]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let filter = match args.get("pattern") {
            Some(pattern) => format!(" --match {}", quote(pattern)),
            None => String::new(),
        };

        // A repository with no tags is not an error; it is a first release.
        let outcome = ctx.probe(&format!("git describe --tags --abbrev=0{filter}"))?;
        let tag = outcome.stdout.trim().to_string();

        if !outcome.success || tag.is_empty() {
            ctx.ui.say("No tag found yet");
            return Ok(ActionOutput::new().with("tag", "").with("found", "false"));
        }

        Ok(ActionOutput::new().with("tag", tag).with("found", "true"))
    }
}

pub struct ChangelogFromCommits;

impl Action for ChangelogFromCommits {
    fn name(&self) -> &'static str {
        "changelog_from_commits"
    }

    fn description(&self) -> &'static str {
        "Collect commit subjects since a tag or revision"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("from", "Start revision; defaults to the last tag"),
            ArgSpec::new("to", "End revision").default("HEAD"),
            ArgSpec::new("format", "git log --pretty format").default("- %s"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let to = args.get_or("to", "HEAD");
        let format = args.get_or("format", "- %s");

        let from = match args.get("from") {
            Some(from) => Some(from.to_string()),
            None => {
                let outcome = ctx.probe("git describe --tags --abbrev=0")?;
                let tag = outcome.stdout.trim().to_string();
                (outcome.success && !tag.is_empty()).then_some(tag)
            }
        };

        let range = match &from {
            Some(from) => format!("{}..{to}", from.as_str()),
            None => to.to_string(),
        };

        let log = ctx.capture(&format!(
            "git log --no-merges --pretty=format:{} {}",
            quote(format),
            quote(&range)
        ))?;

        let count = log.lines().filter(|line| !line.trim().is_empty()).count();
        ctx.ui.say(&format!(
            "{count} commit(s) since {}",
            from.as_deref().unwrap_or("the start")
        ));

        Ok(ActionOutput::new()
            .with("text", log)
            .with("count", count.to_string())
            .with("from", from.unwrap_or_default()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_survives_awkward_messages() {
        assert_eq!(quote("simple"), "'simple'");
        assert_eq!(quote("it's fine"), r"'it'\''s fine'");
        assert_eq!(quote("a; rm -rf /"), "'a; rm -rf /'");
    }
}
