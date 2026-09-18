//! Files and artifacts (`docs/plan/06-actions-core.md`).

use crate::actions::context::ActionContext;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::Result;
use std::path::{Path, PathBuf};

/// Match a path against a pattern with `*` (within one component) and `**`
/// (across components).
///
/// A small matcher rather than a glob crate: the patterns people write for
/// artifacts are `build/**/*.apk`, and a dependency that pulls in a regex
/// engine to answer that is a poor trade.
pub fn matches(pattern: &str, path: &str) -> bool {
    let pattern: Vec<&str> = pattern.split('/').collect();
    let path: Vec<&str> = path.split('/').collect();
    match_segments(&pattern, &path)
}

fn match_segments(pattern: &[&str], path: &[&str]) -> bool {
    match pattern.first() {
        None => path.is_empty(),
        Some(&"**") => {
            // `**` takes any number of components, including none.
            (0..=path.len()).any(|taken| match_segments(&pattern[1..], &path[taken..]))
        }
        Some(segment) => match path.first() {
            Some(component) if match_one(segment, component) => {
                match_segments(&pattern[1..], &path[1..])
            }
            _ => false,
        },
    }
}

/// One path component against one pattern component, where `*` matches any run
/// of characters.
fn match_one(pattern: &str, component: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == component;
    }

    let mut rest = component;
    if let Some(first) = parts.first() {
        match rest.strip_prefix(first) {
            Some(tail) => rest = tail,
            None => return false,
        }
    }
    if let Some(last) = parts.last() {
        if !rest.ends_with(last) || rest.len() < last.len() {
            return false;
        }
        rest = &rest[..rest.len() - last.len()];
    }
    for middle in &parts[1..parts.len().saturating_sub(1)] {
        match rest.find(middle) {
            Some(at) => rest = &rest[at + middle.len()..],
            None => return false,
        }
    }
    true
}

/// Every file under `root` whose path relative to it matches `pattern`.
fn find(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    walk(root, root, &mut |relative, absolute| {
        if matches(pattern, relative) {
            found.push(absolute.to_path_buf());
        }
    });
    found.sort();
    found
}

fn walk(root: &Path, at: &Path, visit: &mut impl FnMut(&str, &Path)) {
    let Ok(entries) = std::fs::read_dir(at) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(root, &path, visit);
            continue;
        }
        if let Ok(relative) = path.strip_prefix(root) {
            visit(&relative.to_string_lossy().replace('\\', "/"), &path);
        }
    }
}

/// Bundle files up for a CI to keep.
pub struct Zip;

impl Action for Zip {
    fn name(&self) -> &'static str {
        "zip"
    }

    fn description(&self) -> &'static str {
        "Create a zip archive"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("path", "File or directory to archive").required(),
            ArgSpec::new("output", "Archive to write; defaults to <path>.zip"),
            ArgSpec::new("exclude", "Comma-separated patterns to leave out"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let path = args.get_or("path", "");
        let output = match args.get("output") {
            Some(output) if !output.is_empty() => output.to_string(),
            _ => format!("{path}.zip"),
        };
        let excludes: Vec<&str> = args
            .get("exclude")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty())
            .collect();

        if has_tool(ctx, "zip")? {
            let mut command = format!("zip -r -q {} {}", quote(&output), quote(path));
            for pattern in &excludes {
                command.push_str(&format!(" -x {}", quote(pattern)));
            }
            ctx.require(&command)?;
            return Ok(ActionOutput::new().with("archive", output));
        }

        // Windows has no `zip`, but it has PowerShell, and Compress-Archive
        // stores the directory as the root entry exactly as `zip -r` does.
        if !excludes.is_empty() {
            // Never silently: a file the lane asked to keep out of an archive
            // could be a keystore or a .env, and an archive gets uploaded.
            return Err(ctx.error(
                self.name(),
                "'zip' is not installed, and the PowerShell fallback cannot exclude anything; install zip, or drop the exclude argument",
            ));
        }
        let binary = require_powershell(ctx, self.name())?;
        ctx.require(&powershell(
            binary,
            &format!(
                "Compress-Archive -Path {} -DestinationPath {} -Force",
                ps_quote(path),
                ps_quote(&output)
            ),
        ))?;

        Ok(ActionOutput::new().with("archive", output))
    }
}

pub struct Unzip;

impl Action for Unzip {
    fn name(&self) -> &'static str {
        "unzip"
    }

    fn description(&self) -> &'static str {
        "Extract a zip archive"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("archive", "Archive to extract").required(),
            ArgSpec::new("into", "Directory to extract into").default("."),
            ArgSpec::new("overwrite", "Replace files that are already there").default("true"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let archive = args.get_or("archive", "");
        let into = args.get_or("into", ".");
        let overwrite = args.flag("overwrite");

        if has_tool(ctx, "unzip")? {
            let flag = if overwrite { "-o" } else { "-n" };
            ctx.require(&format!(
                "unzip -q {flag} {} -d {}",
                quote(archive),
                quote(into)
            ))?;
            return Ok(ActionOutput::new().with("into", into));
        }

        let binary = require_powershell(ctx, self.name())?;
        // Expand-Archive without -Force refuses to replace, which is what
        // overwrite: false asks for.
        let force = if overwrite { " -Force" } else { "" };
        ctx.require(&powershell(
            binary,
            &format!(
                "Expand-Archive -Path {} -DestinationPath {}{force}",
                ps_quote(archive),
                ps_quote(into)
            ),
        ))?;

        Ok(ActionOutput::new().with("into", into))
    }
}

/// Gather the things worth keeping into one directory, so a CI can upload it.
pub struct CopyArtifacts;

impl Action for CopyArtifacts {
    fn name(&self) -> &'static str {
        "copy_artifacts"
    }

    fn description(&self) -> &'static str {
        "Copy build outputs into one directory"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new(
                "paths",
                "Comma-separated paths or patterns, e.g. build/**/*.apk",
            )
            .required(),
            ArgSpec::new("into", "Directory to copy them into").required(),
            ArgSpec::new(
                "flatten",
                "Drop the directories and keep only the file names",
            )
            .default("true"),
            ArgSpec::new(
                "fail_on_missing",
                "Fail when a pattern matches nothing at all",
            )
            .default("false"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let into = ctx.workdir().join(args.get_or("into", "."));
        let flatten = args.flag("flatten");
        let patterns: Vec<&str> = args
            .get_or("paths", "")
            .split(',')
            .map(str::trim)
            .filter(|pattern| !pattern.is_empty())
            .collect();

        let mut copied = Vec::new();
        let mut empty = Vec::new();

        for pattern in &patterns {
            // A path with no wildcard in it is taken literally, so a file whose
            // name contains a bracket is not silently skipped.
            let found = if pattern.contains('*') {
                find(ctx.workdir(), pattern)
            } else {
                let direct = ctx.workdir().join(pattern);
                if direct.is_file() {
                    vec![direct]
                } else {
                    Vec::new()
                }
            };

            if found.is_empty() {
                empty.push(*pattern);
                continue;
            }
            copied.extend(found);
        }

        if !empty.is_empty() && args.flag("fail_on_missing") {
            return Err(ctx.error(
                self.name(),
                format!("these patterns matched nothing: {}", empty.join(", ")),
            ));
        }
        for pattern in &empty {
            ctx.ui.warn(&format!("'{pattern}' matched nothing"));
        }

        if ctx.dry_run {
            ctx.ui.say(&format!(
                "Would copy {} file(s) into {}",
                copied.len(),
                into.display()
            ));
            return Ok(ActionOutput::new().with("count", copied.len().to_string()));
        }

        std::fs::create_dir_all(&into).map_err(|err| {
            ctx.error(
                self.name(),
                format!("cannot create {}: {err}", into.display()),
            )
        })?;

        for source in &copied {
            let relative = source.strip_prefix(ctx.workdir()).unwrap_or(source);
            let target = if flatten {
                into.join(source.file_name().unwrap_or_default())
            } else {
                into.join(relative)
            };
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent).map_err(|err| {
                    ctx.error(
                        self.name(),
                        format!("cannot create {}: {err}", parent.display()),
                    )
                })?;
            }
            std::fs::copy(source, &target).map_err(|err| {
                ctx.error(
                    self.name(),
                    format!(
                        "cannot copy {} to {}: {err}",
                        source.display(),
                        target.display()
                    ),
                )
            })?;
            ctx.ui
                .detail(&format!("{} -> {}", relative.display(), target.display()));
        }

        ctx.ui.say(&format!(
            "Copied {} file(s) into {}",
            copied.len(),
            into.display()
        ));
        Ok(ActionOutput::new()
            .with("count", copied.len().to_string())
            .with("into", into.to_string_lossy()))
    }
}

/// Fetch something over HTTP: an SDK, a keystore, a translation bundle.
pub struct Download;

impl Action for Download {
    fn name(&self) -> &'static str {
        "download"
    }

    fn description(&self) -> &'static str {
        "Download a file over HTTP"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("url", "What to fetch").required(),
            ArgSpec::new("output", "Where to write it").required(),
            ArgSpec::new("headers", "Extra headers, one per line as Name: value").sensitive(),
            ArgSpec::new(
                "sha256",
                "Expected checksum; the download is rejected when it differs",
            ),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        use crate::actions::core::http::fetch_bytes;

        let url = args.get_or("url", "");
        let output = ctx.workdir().join(args.get_or("output", ""));

        if ctx.dry_run {
            ctx.ui
                .say(&format!("Would download {url} to {}", output.display()));
            return Ok(ActionOutput::new().with("path", output.to_string_lossy()));
        }

        let headers = parse_headers(args.get_or("headers", ""));
        let (status, bytes) =
            fetch_bytes(ctx, url, &headers).map_err(|message| ctx.error(self.name(), message))?;

        if status >= 400 {
            return Err(ctx.error(self.name(), format!("{url} returned {status}")));
        }

        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                ctx.error(
                    self.name(),
                    format!("cannot create {}: {err}", parent.display()),
                )
            })?;
        }
        std::fs::write(&output, &bytes).map_err(|err| {
            ctx.error(
                self.name(),
                format!("cannot write {}: {err}", output.display()),
            )
        })?;

        let digest = sha256_hex(&bytes);
        if let Some(expected) = args.get("sha256").filter(|value| !value.is_empty()) {
            if !expected.eq_ignore_ascii_case(&digest) {
                // Left on disk would be a file something later step trusts.
                let _ = std::fs::remove_file(&output);
                return Err(ctx.error(
                    self.name(),
                    format!("checksum mismatch\n  expected sha256:{expected}\n  found    sha256:{digest}"),
                ));
            }
        }

        ctx.ui.say(&format!(
            "Downloaded {} bytes to {}",
            bytes.len(),
            output.display()
        ));
        Ok(ActionOutput::new()
            .with("path", output.to_string_lossy())
            .with("sha256", digest)
            .with("status", status.to_string()))
    }
}

/// In place of fastlane's `erb`: substitute `${...}` in a file.
pub struct TemplateRender;

impl Action for TemplateRender {
    fn name(&self) -> &'static str {
        "template_render"
    }

    fn description(&self) -> &'static str {
        "Render a template, substituting ${params.x}, ${env.X} and ${steps.id.key}"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("template", "File to read").required(),
            ArgSpec::new("output", "File to write; printed instead when absent"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        use crate::runtime::interpolate::{interpolate_plain, Vars};

        let template = ctx.workdir().join(args.get_or("template", ""));
        let source = std::fs::read_to_string(&template).map_err(|err| {
            ctx.error(
                self.name(),
                format!("cannot read {}: {err}", template.display()),
            )
        })?;

        // The same names a step's `with:` can use, so a template does not have
        // a second vocabulary to learn.
        let (params, env, meta) = {
            let frame = ctx.frame.borrow();
            (frame.params.clone(), frame.env.clone(), frame.meta())
        };
        let outputs = ctx.outputs.borrow().flatten();
        let rendered = interpolate_plain(
            &source,
            &Vars {
                params: &params,
                env: &env,
                meta: &meta,
                outputs: &outputs,
                dry_run: ctx.dry_run,
            },
        )?;

        match args.get("output").filter(|value| !value.is_empty()) {
            Some(output) => {
                let path = ctx.workdir().join(output);
                if ctx.dry_run {
                    ctx.ui.say(&format!("Would write {}", path.display()));
                } else {
                    if let Some(parent) = path.parent() {
                        std::fs::create_dir_all(parent).map_err(|err| {
                            ctx.error(
                                self.name(),
                                format!("cannot create {}: {err}", parent.display()),
                            )
                        })?;
                    }
                    std::fs::write(&path, &rendered).map_err(|err| {
                        ctx.error(
                            self.name(),
                            format!("cannot write {}: {err}", path.display()),
                        )
                    })?;
                    ctx.ui.say(&format!("Wrote {}", path.display()));
                }
                Ok(ActionOutput::new().with("path", path.to_string_lossy()))
            }
            None => {
                ctx.ui.say(&rendered);
                Ok(ActionOutput::new().with("rendered", rendered))
            }
        }
    }
}

fn parse_headers(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    ring::digest::digest(&ring::digest::SHA256, bytes)
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// Whether a binary is on PATH.
///
/// `probe`, not `sh`: under `--dry-run` the answer to "is it installed" has to
/// be the real one, or the dry run reports a problem that does not exist.
fn has_tool(ctx: &ActionContext<'_>, tool: &str) -> Result<bool> {
    Ok(ctx.probe(&format!("command -v {tool}"))?.success)
}

/// Which PowerShell to use, preferring the cross-platform one.
fn require_powershell(ctx: &ActionContext<'_>, action: &str) -> Result<&'static str> {
    for candidate in ["pwsh", "powershell"] {
        if has_tool(ctx, candidate)? {
            return Ok(candidate);
        }
    }
    Err(ctx.error(
        action,
        "neither 'zip'/'unzip' nor PowerShell is available; install one of them",
    ))
}

/// Run a PowerShell command from the POSIX shell steps already use.
///
/// The binary is chosen before this is built rather than with a `||` chain,
/// which would run the command a second time when the first attempt failed for
/// its own reasons.
fn powershell(binary: &str, script: &str) -> String {
    format!(
        "{binary} -NoProfile -NonInteractive -Command {}",
        quote(script)
    )
}

/// Quote a path for PowerShell, where a single-quoted string escapes a quote by
/// doubling it.
fn ps_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_a_literal_path() {
        assert!(matches("build/app.apk", "build/app.apk"));
        assert!(!matches("build/app.apk", "build/other.apk"));
    }

    #[test]
    fn star_stays_inside_one_component() {
        assert!(matches("build/*.apk", "build/app.apk"));
        assert!(!matches("build/*.apk", "build/outputs/app.apk"));
    }

    #[test]
    fn double_star_crosses_components_including_none() {
        assert!(matches("build/**/*.apk", "build/outputs/apk/app.apk"));
        assert!(matches("build/**/*.apk", "build/app.apk"));
        assert!(!matches("build/**/*.apk", "other/app.apk"));
    }

    #[test]
    fn a_star_in_the_middle_of_a_name() {
        assert!(matches("app-*-release.apk", "app-prod-release.apk"));
        assert!(!matches("app-*-release.apk", "app-prod-debug.apk"));
    }

    #[test]
    fn several_stars_in_one_component() {
        assert!(matches("*-*-release.*", "app-prod-release.apk"));
        assert!(!matches("*-*-release.*", "app-release.apk"));
    }

    #[test]
    fn reads_headers_one_per_line() {
        let headers = parse_headers("Authorization: Bearer x\nAccept: application/json\n");
        assert_eq!(
            headers,
            vec![
                ("Authorization".to_string(), "Bearer x".to_string()),
                ("Accept".to_string(), "application/json".to_string()),
            ]
        );
    }

    #[test]
    fn quotes_a_path_the_way_powershell_does() {
        assert_eq!(ps_quote("build/app.zip"), "'build/app.zip'");
        // PowerShell escapes a quote inside a single-quoted string by doubling it.
        assert_eq!(ps_quote("it's here"), "'it''s here'");
    }

    #[test]
    fn builds_a_powershell_command_the_posix_shell_can_carry() {
        let command = powershell(
            "pwsh",
            "Compress-Archive -Path 'payload' -DestinationPath 'out.zip' -Force",
        );
        assert_eq!(
            command,
            "pwsh -NoProfile -NonInteractive -Command 'Compress-Archive -Path '\\''payload'\\'' -DestinationPath '\\''out.zip'\\'' -Force'"
        );
    }

    #[test]
    fn hashes_the_way_sha256sum_does() {
        // echo -n abc | sha256sum
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
