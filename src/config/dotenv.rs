//! Reading `.env` files.
//!
//! Deliberately small: `KEY=value`, optional `export`, `#` comments, and the
//! two quoting styles people actually use.

use crate::error::{Result, ShlaneError};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// Parse a `.env` file. A file that does not exist yields nothing, so an
/// optional `.env` needs no ceremony.
pub fn load(path: &Path) -> Result<BTreeMap<String, String>> {
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let text = fs::read_to_string(path).map_err(|source| ShlaneError::ConfigUnreadable {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(parse(&text))
}

pub fn parse(text: &str) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line).trim_start();

        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() {
            continue;
        }
        values.insert(key.to_string(), parse_value(value.trim()));
    }

    values
}

fn parse_value(value: &str) -> String {
    if let Some(inner) = value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')) {
        // Single quotes are literal, as in a shell.
        return inner.to_string();
    }
    if let Some(inner) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        return inner
            .replace("\\n", "\n")
            .replace("\\t", "\t")
            .replace("\\\"", "\"");
    }
    // Unquoted: an inline comment ends the value.
    match value.split_once(" #") {
        Some((before, _)) => before.trim_end().to_string(),
        None => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_plain_pairs() {
        let values = parse("A=1\nB=two\n");
        assert_eq!(values.get("A").map(String::as_str), Some("1"));
        assert_eq!(values.get("B").map(String::as_str), Some("two"));
    }

    #[test]
    fn skips_comments_and_blanks() {
        let values = parse("# a comment\n\nA=1\n");
        assert_eq!(values.len(), 1);
    }

    #[test]
    fn accepts_export() {
        let values = parse("export TOKEN=abc\n");
        assert_eq!(values.get("TOKEN").map(String::as_str), Some("abc"));
    }

    #[test]
    fn keeps_quoted_values_whole() {
        let values = parse("A=\"one two\"\nB='three # four'\n");
        assert_eq!(values.get("A").map(String::as_str), Some("one two"));
        assert_eq!(values.get("B").map(String::as_str), Some("three # four"));
    }

    #[test]
    fn strips_inline_comments_from_unquoted_values() {
        let values = parse("A=one # a note\n");
        assert_eq!(values.get("A").map(String::as_str), Some("one"));
    }

    #[test]
    fn keeps_a_hash_that_is_part_of_the_value() {
        let values = parse("CHANNEL=#releases\n");
        assert_eq!(values.get("CHANNEL").map(String::as_str), Some("#releases"));
    }

    #[test]
    fn expands_escapes_only_inside_double_quotes() {
        let values = parse("A=\"line\\nbreak\"\nB='line\\nbreak'\n");
        assert_eq!(values.get("A").map(String::as_str), Some("line\nbreak"));
        assert_eq!(values.get("B").map(String::as_str), Some("line\\nbreak"));
    }

    #[test]
    fn ignores_lines_without_an_equals() {
        let values = parse("nonsense\nA=1\n");
        assert_eq!(values.len(), 1);
    }
}
