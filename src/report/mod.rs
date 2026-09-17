//! Writing a run's results somewhere a CI can read them
//! (`docs/plan/11-ci-integration.md`).

pub mod xcresult;

use crate::error::{Result, ShlaneError};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Skipped,
    Failed,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

/// One step, as the report sees it.
#[derive(Debug, Clone)]
pub struct StepReport {
    pub lane: String,
    pub step: String,
    pub status: Status,
    pub duration: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Junit,
    Json,
    Markdown,
}

#[derive(Debug, Clone)]
pub struct Target {
    pub format: Format,
    pub path: PathBuf,
}

/// Parse `junit:./reports/shlane.xml`.
pub fn parse(spec: &str) -> std::result::Result<Target, String> {
    let (format, path) = spec
        .split_once(':')
        .ok_or_else(|| format!("'{spec}' should look like junit:<path>"))?;

    let format = match format.trim().to_ascii_lowercase().as_str() {
        "junit" | "xml" => Format::Junit,
        "json" => Format::Json,
        "md" | "markdown" => Format::Markdown,
        other => {
            return Err(format!(
                "unknown report format '{other}'; use junit, json or md"
            ))
        }
    };

    let path = path.trim();
    if path.is_empty() {
        return Err(format!("'{spec}' has no path"));
    }

    Ok(Target {
        format,
        path: PathBuf::from(path),
    })
}

pub fn write(target: &Target, lane: &str, steps: &[StepReport], failed: bool) -> Result<()> {
    let body = match target.format {
        Format::Junit => junit(lane, steps),
        Format::Json => json(lane, steps, failed),
        Format::Markdown => markdown(lane, steps),
    };

    if let Some(parent) = target.path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|source| ShlaneError::ConfigUnreadable {
                path: parent.to_path_buf(),
                source,
            })?;
        }
    }

    append_or_write(&target.path, &body, target.format)
}

/// GitHub's step summary file is appended to, not replaced.
fn append_or_write(path: &Path, body: &str, format: Format) -> Result<()> {
    use std::io::Write as _;

    let result = if format == Format::Markdown && path.exists() {
        fs::OpenOptions::new()
            .append(true)
            .open(path)
            .and_then(|mut file| file.write_all(body.as_bytes()))
    } else {
        fs::write(path, body)
    };

    result.map_err(|source| ShlaneError::ConfigUnreadable {
        path: path.to_path_buf(),
        source,
    })
}

fn junit(lane: &str, steps: &[StepReport]) -> String {
    let failures = steps
        .iter()
        .filter(|step| step.status == Status::Failed)
        .count();
    let total: f64 = steps.iter().map(|step| step.duration.as_secs_f64()).sum();

    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let _ = writeln!(
        out,
        "<testsuites name=\"shlane\" tests=\"{}\" failures=\"{failures}\" time=\"{total:.3}\">",
        steps.len()
    );
    let _ = writeln!(
        out,
        "  <testsuite name=\"{}\" tests=\"{}\" failures=\"{failures}\" time=\"{total:.3}\">",
        escape_xml(lane),
        steps.len()
    );

    for step in steps {
        let _ = write!(
            out,
            "    <testcase classname=\"{}\" name=\"{}\" time=\"{:.3}\"",
            escape_xml(&step.lane),
            escape_xml(&step.step),
            step.duration.as_secs_f64()
        );
        match step.status {
            Status::Ok => {
                let _ = writeln!(out, " />");
            }
            Status::Skipped => {
                let _ = writeln!(out, ">\n      <skipped />\n    </testcase>");
            }
            Status::Failed => {
                let _ = writeln!(
                    out,
                    ">\n      <failure message=\"step failed\" />\n    </testcase>"
                );
            }
        }
    }

    out.push_str("  </testsuite>\n</testsuites>\n");
    out
}

fn json(lane: &str, steps: &[StepReport], failed: bool) -> String {
    let mut out = String::from("{\n");
    let _ = writeln!(out, "  \"lane\": \"{}\",", escape_json(lane));
    let _ = writeln!(
        out,
        "  \"result\": \"{}\",",
        if failed { "failed" } else { "ok" }
    );
    out.push_str("  \"steps\": [\n");

    for (index, step) in steps.iter().enumerate() {
        let comma = if index + 1 == steps.len() { "" } else { "," };
        let _ = writeln!(
            out,
            "    {{\"lane\": \"{}\", \"step\": \"{}\", \"status\": \"{}\", \"seconds\": {:.3}}}{comma}",
            escape_json(&step.lane),
            escape_json(&step.step),
            step.status.as_str(),
            step.duration.as_secs_f64()
        );
    }

    out.push_str("  ]\n}\n");
    out
}

fn markdown(lane: &str, steps: &[StepReport]) -> String {
    let mut out = format!("\n### shlane: {lane}\n\n| step | result | time |\n|---|---|---|\n");
    for step in steps {
        let mark = match step.status {
            Status::Ok => "✅",
            Status::Skipped => "⏭️",
            Status::Failed => "❌",
        };
        let _ = writeln!(
            out,
            "| {} | {mark} {} | {:.1}s |",
            escape_markdown(&step.step),
            step.status.as_str(),
            step.duration.as_secs_f64()
        );
    }
    out
}

pub(crate) fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn escape_json(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out
}

fn escape_markdown(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn steps() -> Vec<StepReport> {
        vec![
            StepReport {
                lane: "beta".to_string(),
                step: "build & sign".to_string(),
                status: Status::Ok,
                duration: Duration::from_millis(1500),
            },
            StepReport {
                lane: "beta".to_string(),
                step: "upload".to_string(),
                status: Status::Failed,
                duration: Duration::from_millis(500),
            },
            StepReport {
                lane: "beta".to_string(),
                step: "announce".to_string(),
                status: Status::Skipped,
                duration: Duration::ZERO,
            },
        ]
    }

    #[test]
    fn parses_report_specifications() {
        let target = parse("junit:./reports/shlane.xml").expect("valid");
        assert_eq!(target.format, Format::Junit);
        assert_eq!(target.path, PathBuf::from("./reports/shlane.xml"));

        assert_eq!(parse("json:out.json").expect("valid").format, Format::Json);
        assert_eq!(parse("md:out.md").expect("valid").format, Format::Markdown);

        assert!(parse("junit").is_err());
        assert!(parse("junit:").is_err());
        assert!(parse("toml:out.toml").is_err());
    }

    #[test]
    fn junit_counts_and_escapes() {
        let xml = junit("beta", &steps());
        assert!(xml.contains("tests=\"3\""), "{xml}");
        assert!(xml.contains("failures=\"1\""), "{xml}");
        assert!(xml.contains("build &amp; sign"), "{xml}");
        assert!(xml.contains("<skipped />"), "{xml}");
        assert!(xml.contains("<failure"), "{xml}");
    }

    #[test]
    fn json_is_well_formed_enough_to_parse() {
        let text = json("beta", &steps(), true);
        let parsed: serde_yaml::Value = serde_yaml::from_str(&text).expect("JSON is valid YAML");
        assert_eq!(parsed["result"].as_str(), Some("failed"));
        assert_eq!(parsed["steps"].as_sequence().map(Vec::len), Some(3));
    }

    #[test]
    fn markdown_escapes_pipes() {
        let step = StepReport {
            lane: "a".to_string(),
            step: "run a | b".to_string(),
            status: Status::Ok,
            duration: Duration::from_secs(1),
        };
        let text = markdown("a", &[step]);
        assert!(text.contains(r"run a \| b"), "{text}");
    }
}
