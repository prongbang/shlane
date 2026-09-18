//! Adding and removing a `plugins:` entry in an existing `shlane.yaml`.
//!
//! Edited as text rather than parsed and re-serialised: a round trip through
//! serde would throw away every comment and reformat a file the user wrote by
//! hand, which is a bad trade for adding three lines.

/// Add an entry to the config's `plugins:` list.
///
/// Inserted at the front of the list, because the order of a YAML sequence has
/// no meaning here and finding the front is unambiguous; a config with no
/// `plugins:` key gets one at the end.
pub fn insert(text: &str, name: &str, source: &str) -> Result<String, String> {
    let entry = format!("  - name: {name}\n    source: {source}\n");

    match find_key(text, "plugins:") {
        Some(index) => {
            let rest = &text[index..];
            let line_end = rest
                .find('\n')
                .map_or(text.len(), |offset| index + offset + 1);
            let line = &text[index..line_end];
            let after_colon = line
                .split_once(':')
                .map(|(_, rest)| rest.trim())
                .unwrap_or_default();

            // `plugins: []` or a flow sequence: inserting a block item under it
            // would produce a file that no longer parses.
            if !after_colon.is_empty() {
                return Err(format!(
                    "the config writes `plugins:` on one line; add this by hand:\n\n  - name: {name}\n    source: {source}"
                ));
            }

            Ok(format!("{}{entry}{}", &text[..line_end], &text[line_end..]))
        }
        None => {
            let separator = if text.ends_with('\n') || text.is_empty() {
                ""
            } else {
                "\n"
            };
            Ok(format!("{text}{separator}\nplugins:\n{entry}"))
        }
    }
}

/// Remove the entry naming `name` from the config's `plugins:` list.
pub fn remove(text: &str, name: &str) -> Result<String, String> {
    let lines: Vec<&str> = text.lines().collect();
    let target = format!("- name: {name}");

    let start = lines
        .iter()
        .position(|line| line.trim() == target)
        .ok_or_else(|| format!("'{name}' is not declared in this config"))?;

    // The entry runs until the next list item, or until something at or left of
    // the item's own indentation -- the next top-level key.
    let indent = lines[start].len() - lines[start].trim_start().len();
    let mut end = start + 1;
    while end < lines.len() {
        let line = lines[end];
        if line.trim().is_empty() {
            end += 1;
            continue;
        }
        let this_indent = line.len() - line.trim_start().len();
        if this_indent <= indent {
            break;
        }
        end += 1;
    }

    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    kept.extend_from_slice(&lines[..start]);
    kept.extend_from_slice(&lines[end..]);

    // A `plugins:` key with nothing under it parses as null, which is fine, but
    // leaving it is untidy; drop it when it has become empty.
    let result = drop_empty_plugins(&kept);
    Ok(if text.ends_with('\n') {
        format!("{result}\n")
    } else {
        result
    })
}

fn drop_empty_plugins(lines: &[&str]) -> String {
    let Some(index) = lines.iter().position(|line| line.trim_end() == "plugins:") else {
        return lines.join("\n");
    };

    let has_items = lines[index + 1..]
        .iter()
        .find(|line| !line.trim().is_empty())
        .is_some_and(|line| line.starts_with(' ') || line.starts_with('\t'));

    if has_items {
        return lines.join("\n");
    }

    let mut kept: Vec<&str> = lines[..index].to_vec();
    kept.extend_from_slice(&lines[index + 1..]);
    // Collapse the blank line the removed key may have left behind.
    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }
    kept.join("\n")
}

/// Offset of a top-level key, ignoring one inside a nested block or a comment.
fn find_key(text: &str, key: &str) -> Option<usize> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.starts_with(key) {
            return Some(offset);
        }
        offset += line.len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_a_plugins_block_when_there_is_none() {
        let out = insert(
            "lanes:\n  hello:\n    steps:\n      - run: echo hi\n",
            "demo",
            "github:me/demo@v1",
        )
        .expect("should insert");
        assert!(out.contains("plugins:\n  - name: demo\n    source: github:me/demo@v1\n"));
        assert!(out.starts_with("lanes:"), "the existing config is kept");
    }

    #[test]
    fn adds_to_an_existing_list_without_touching_the_rest() {
        let text = "# keep me\nplugins:\n  - name: first\n    path: ./first\nlanes: {}\n";
        let out = insert(text, "second", "github:me/second@v2").expect("should insert");
        assert!(out.contains("# keep me"), "comments survive");
        assert!(out.contains("- name: first"), "the existing entry survives");
        assert!(out.contains("- name: second"));
    }

    #[test]
    fn refuses_a_flow_sequence_rather_than_producing_a_broken_file() {
        let error =
            insert("plugins: []\n", "demo", "github:me/demo@v1").expect_err("should refuse");
        assert!(error.contains("by hand"), "{error}");
    }

    #[test]
    fn removes_an_entry_and_its_fields() {
        let text = "plugins:\n  - name: first\n    path: ./first\n  - name: second\n    source: github:me/second@v2\nlanes: {}\n";
        let out = remove(text, "first").expect("should remove");
        assert!(!out.contains("./first"));
        assert!(out.contains("- name: second"));
        assert!(out.contains("lanes: {}"));
    }

    #[test]
    fn removes_the_plugins_key_when_the_last_entry_goes() {
        let text = "plugins:\n  - name: only\n    path: ./only\nlanes: {}\n";
        let out = remove(text, "only").expect("should remove");
        assert!(!out.contains("plugins:"), "{out}");
        assert!(out.contains("lanes: {}"));
    }

    #[test]
    fn reports_a_name_that_is_not_there() {
        let error =
            remove("plugins:\n  - name: one\n    path: ./one\n", "two").expect_err("should fail");
        assert!(error.contains("not declared"), "{error}");
    }

    #[test]
    fn round_trips_through_the_yaml_parser() {
        let text = "lanes:\n  hello:\n    steps:\n      - run: echo hi\n";
        let added = insert(text, "demo", "github:me/demo@v1").expect("should insert");
        let config: crate::config::model::Config =
            serde_yaml::from_str(&added).expect("the result should still parse");
        assert_eq!(config.plugins.len(), 1);
        assert_eq!(config.plugins[0].name, "demo");

        let removed = remove(&added, "demo").expect("should remove");
        let config: crate::config::model::Config =
            serde_yaml::from_str(&removed).expect("the result should still parse");
        assert!(config.plugins.is_empty());
    }
}
