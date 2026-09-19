//! The documentation site (`docs/site/`).
//!
//! Every page there is a stub that pulls in Markdown which already lives in the
//! repository, so there is only ever one copy of the text. The cost of that is a
//! silent failure mode: mdBook renders an `{{#include}}` whose anchor has gone as
//! an empty page and still exits 0. These tests are what turns that into a red
//! build — a renamed README section, a moved file or a page nobody linked all
//! fail here rather than on the published site.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn src_dir() -> PathBuf {
    repo_root().join("docs/site/src")
}

/// A Windows checkout may have turned every `\n` into `\r\n`.
fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .replace("\r\n", "\n")
}

/// The pages `SUMMARY.md` links, in the order it lists them.
fn summary_pages() -> Vec<String> {
    let summary = read(&src_dir().join("SUMMARY.md"));
    let mut pages = Vec::new();
    for line in summary.lines() {
        let Some(open) = line.find("](") else {
            continue;
        };
        let rest = &line[open + 2..];
        let Some(close) = rest.find(')') else {
            continue;
        };
        pages.push(rest[..close].to_string());
    }
    pages
}

/// Every `{{#include <path>}}` or `{{#include <path>:<anchor>}}` in a page.
fn includes(page: &str) -> Vec<(String, Option<String>)> {
    let mut found = Vec::new();
    let mut rest = page;
    while let Some(start) = rest.find("{{#include ") {
        rest = &rest[start + "{{#include ".len()..];
        let end = rest.find("}}").expect("an include is closed with }}");
        let target = rest[..end].trim().to_string();
        rest = &rest[end..];
        match target.split_once(':') {
            Some((path, anchor)) => found.push((path.to_string(), Some(anchor.to_string()))),
            None => found.push((target, None)),
        }
    }
    found
}

/// The anchors a file defines, mapped to the text between the markers.
fn anchors(text: &str) -> BTreeMap<String, String> {
    const OPEN: &str = "<!-- ANCHOR: ";
    let mut found = BTreeMap::new();
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        rest = &rest[start + OPEN.len()..];
        let end = rest
            .find(" -->")
            .expect("an ANCHOR marker is closed with -->");
        let name = rest[..end].to_string();
        let body = &rest[end + " -->".len()..];
        let close = format!("<!-- ANCHOR_END: {name} -->");
        let stop = body
            .find(&close)
            .unwrap_or_else(|| panic!("anchor {name:?} is opened but never closed"));
        found.insert(name, body[..stop].to_string());
        rest = body;
    }
    found
}

/// Headings outside fenced code blocks: `(level, text, line number)`.
fn headings(text: &str) -> Vec<(usize, String, usize)> {
    let mut found = Vec::new();
    let mut fenced = false;
    for (i, line) in text.lines().enumerate() {
        if line.starts_with("```") {
            fenced = !fenced;
        } else if !fenced {
            let level = line.chars().take_while(|c| *c == '#').count();
            if (1..=3).contains(&level) && line[level..].starts_with(' ') {
                found.push((level, line[level + 1..].trim().to_string(), i));
            }
        }
    }
    found
}

#[test]
fn every_page_summary_links_exists() {
    for page in summary_pages() {
        let path = src_dir().join(&page);
        assert!(
            path.is_file(),
            "SUMMARY.md links {page}, which is not in docs/site/src"
        );
    }
}

#[test]
fn every_page_is_reachable_from_summary() {
    let listed: BTreeSet<String> = summary_pages().into_iter().collect();
    for entry in fs::read_dir(src_dir()).expect("docs/site/src exists") {
        let name = entry.expect("read docs/site/src").file_name();
        let name = name.to_string_lossy().to_string();
        if name == "SUMMARY.md" || !name.ends_with(".md") {
            continue;
        }
        assert!(
            listed.contains(&name),
            "docs/site/src/{name} is not linked from SUMMARY.md, so nobody can reach it"
        );
    }
}

/// The one mdBook will not complain about: a target that moved, or an anchor
/// that was renamed, leaves a page with nothing but its title on it.
#[test]
fn every_include_resolves_to_something() {
    for page in summary_pages() {
        let page_path = src_dir().join(&page);
        let text = read(&page_path);
        let found = includes(&text);
        assert!(
            !found.is_empty(),
            "docs/site/src/{page} includes nothing — it would render as a bare title"
        );
        for (target, anchor) in found {
            let target_path = src_dir().join(&target);
            assert!(
                target_path.is_file(),
                "docs/site/src/{page} includes {target}, which does not exist"
            );
            let Some(anchor) = anchor else { continue };
            let defined = anchors(&read(&target_path));
            let body = defined.get(&anchor).unwrap_or_else(|| {
                panic!(
                    "docs/site/src/{page} includes the anchor {anchor:?} from {target}, \
                     which does not define it. The anchors it does define: {:?}",
                    defined.keys().collect::<Vec<_>>()
                )
            });
            assert!(
                !body.trim().is_empty(),
                "the anchor {anchor:?} in {target} is empty, so docs/site/src/{page} \
                 would render as a bare title"
            );
        }
    }
}

/// A new README section has to be given a home on the site, or it is only ever
/// read by people who scroll the README.
#[test]
fn every_readme_section_is_on_the_site() {
    // `Contents` is the README's own table of contents, which the site's sidebar
    // replaces; `License` is a repository fact rather than documentation.
    const NOT_ON_THE_SITE: [&str; 2] = ["Contents", "License"];

    let readme = read(&repo_root().join("README.md"));
    let lines: Vec<&str> = readme.lines().collect();
    let defined = anchors(&readme);

    let used: BTreeSet<String> = summary_pages()
        .iter()
        .flat_map(|page| includes(&read(&src_dir().join(page))))
        .filter(|(target, _)| target.ends_with("README.md"))
        .filter_map(|(_, anchor)| anchor)
        .collect();

    for (level, title, line) in headings(&readme) {
        if level != 2 || NOT_ON_THE_SITE.contains(&title.as_str()) {
            continue;
        }
        let next = lines
            .get(line + 1..)
            .unwrap_or_default()
            .iter()
            .find(|l| !l.trim().is_empty())
            .copied()
            .unwrap_or_default();
        let opened = next
            .strip_prefix("<!-- ANCHOR: ")
            .and_then(|rest| rest.strip_suffix(" -->"))
            .map(str::to_string);
        let Some(name) = opened else {
            panic!(
                "README.md line {}: the section {title:?} is not wrapped in an ANCHOR, so it \
                 is missing from the documentation site. Add `<!-- ANCHOR: <name> -->` under \
                 the heading and `<!-- ANCHOR_END: <name> -->` above the next one, then a \
                 page in docs/site/src that includes it.",
                line + 1
            )
        };
        assert!(
            defined.contains_key(&name),
            "README.md line {}: the section {title:?} opens the anchor {name:?}, which is \
             never closed",
            line + 1
        );
        assert!(
            used.contains(&name),
            "README.md line {}: the section {title:?} defines the anchor {name:?}, but no \
             page in docs/site/src includes it",
            line + 1
        );
    }
}

/// The mirror of the check above: an anchor left behind after the section it
/// wrapped was rewritten.
#[test]
fn every_readme_anchor_is_used_by_a_page() {
    let used: BTreeSet<String> = summary_pages()
        .iter()
        .flat_map(|page| includes(&read(&src_dir().join(page))))
        .filter(|(target, _)| target.ends_with("README.md"))
        .filter_map(|(_, anchor)| anchor)
        .collect();

    for name in anchors(&read(&repo_root().join("README.md"))).keys() {
        assert!(
            used.contains(name),
            "README.md defines the anchor {name:?}, which no page on the site includes. \
             Either give it a page, or take the markers out."
        );
    }
}
