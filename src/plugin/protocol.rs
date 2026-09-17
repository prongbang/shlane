//! How shlane and a plugin talk (`docs/plan/09-plugins.md`).
//!
//! One JSON object in on stdin, one JSON object per line out on stdout. The
//! wire format is small on purpose: a plugin can be a shell script.

use serde::Deserialize;

/// Bumped only for a breaking change to this format.
pub const VERSION: u32 = 1;

/// Build the request sent to a plugin's stdin.
pub fn request(
    op: &str,
    action: &str,
    args: &std::collections::BTreeMap<String, String>,
    lane: &str,
    workdir: &str,
    dry_run: bool,
) -> String {
    use crate::actions::core::http::escape;

    let args = args
        .iter()
        .map(|(key, value)| format!("\"{}\":\"{}\"", escape(key), escape(value)))
        .collect::<Vec<_>>()
        .join(",");

    format!(
        "{{\"protocol\":{VERSION},\"op\":\"{}\",\"action\":\"{}\",\"args\":{{{args}}},\
         \"context\":{{\"lane\":\"{}\",\"workdir\":\"{}\",\"dry_run\":{dry_run}}}}}",
        escape(op),
        escape(action),
        escape(lane),
        escape(workdir)
    )
}

/// One line of a plugin's output.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    /// Something to show the user.
    Log {
        #[serde(default)]
        level: String,
        message: String,
    },
    /// A value to mask from here on.
    Secret { value: String },
    /// The outcome. A plugin that never sends one has failed.
    Result {
        #[serde(default)]
        ok: Option<bool>,
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        outputs: std::collections::BTreeMap<String, String>,
    },
    /// The answer to `describe`.
    Describe {
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        args: Vec<DescribedArg>,
    },
}

#[derive(Debug, Deserialize, Clone)]
pub struct DescribedArg {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
    #[serde(default)]
    pub sensitive: bool,
}

/// Parse a plugin's output, ignoring anything that is not an event.
///
/// A plugin that prints a stray line should not bring the lane down, so
/// unparsable lines are returned separately rather than treated as failures.
pub fn parse_events(output: &str) -> (Vec<Event>, Vec<String>) {
    let mut events = Vec::new();
    let mut ignored = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with('{') {
            ignored.push(line.to_string());
            continue;
        }
        // JSON is valid YAML, so this needs no extra parser.
        match serde_yaml::from_str::<Event>(line) {
            Ok(event) => events.push(event),
            Err(_) => ignored.push(line.to_string()),
        }
    }

    (events, ignored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn a_request_is_valid_json() {
        let args = BTreeMap::from([("text".to_string(), "hello \"world\"".to_string())]);
        let request = request("run", "notify_line", &args, "beta", "/repo", false);

        let parsed: serde_yaml::Value = serde_yaml::from_str(&request).expect("valid JSON");
        assert_eq!(parsed["protocol"].as_u64(), Some(1));
        assert_eq!(parsed["op"].as_str(), Some("run"));
        assert_eq!(parsed["action"].as_str(), Some("notify_line"));
        assert_eq!(parsed["args"]["text"].as_str(), Some("hello \"world\""));
        assert_eq!(parsed["context"]["lane"].as_str(), Some("beta"));
        assert_eq!(parsed["context"]["dry_run"].as_bool(), Some(false));
    }

    #[test]
    fn reads_the_events_a_plugin_sends() {
        let output = r#"
{"type":"log","level":"info","message":"sending"}
{"type":"secret","value":"abcd1234"}
{"type":"result","ok":true,"outputs":{"id":"123"}}
"#;
        let (events, ignored) = parse_events(output);
        assert_eq!(events.len(), 3, "{events:?}");
        assert!(ignored.is_empty(), "{ignored:?}");

        match &events[2] {
            Event::Result { ok, outputs, .. } => {
                assert_eq!(*ok, Some(true));
                assert_eq!(outputs.get("id").map(String::as_str), Some("123"));
            }
            other => panic!("expected a result, got {other:?}"),
        }
    }

    #[test]
    fn stray_output_is_kept_separately_rather_than_failing() {
        let (events, ignored) = parse_events("hello\n{\"type\":\"result\",\"ok\":true}\n");
        assert_eq!(events.len(), 1);
        assert_eq!(ignored, vec!["hello".to_string()]);
    }

    #[test]
    fn an_unknown_event_type_is_ignored() {
        let (events, ignored) = parse_events("{\"type\":\"whatever\"}\n");
        assert!(events.is_empty());
        assert_eq!(ignored.len(), 1);
    }

    #[test]
    fn reads_a_description() {
        let output = r#"{"type":"describe","description":"Send a LINE message","args":[{"name":"token","required":true,"sensitive":true,"description":"Channel token"}]}"#;
        let (events, _) = parse_events(output);
        match &events[0] {
            Event::Describe { description, args } => {
                assert_eq!(description.as_deref(), Some("Send a LINE message"));
                assert_eq!(args.len(), 1);
                assert!(args[0].required);
                assert!(args[0].sensitive);
            }
            other => panic!("expected a description, got {other:?}"),
        }
    }
}
