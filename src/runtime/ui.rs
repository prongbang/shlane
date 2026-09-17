//! Everything shlane prints itself.
//!
//! The plan called for `tracing`; a CLI needs a stable, documented event
//! stream more than it needs a subscriber stack, so the events are emitted
//! directly here. `--json` prints one object per line for other tools to read.

use super::secrets::SharedSecrets;
use std::io::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verbosity {
    /// Errors and the child's own output only.
    Quiet,
    #[default]
    Normal,
    /// Adds the resolved environment and step details.
    Verbose,
}

pub struct Ui {
    verbosity: Verbosity,
    json: bool,
    secrets: SharedSecrets,
}

impl Ui {
    pub fn new(verbosity: Verbosity, json: bool, secrets: SharedSecrets) -> Self {
        Self {
            verbosity,
            json,
            secrets,
        }
    }

    pub fn is_verbose(&self) -> bool {
        self.verbosity == Verbosity::Verbose
    }

    fn mask(&self, text: &str) -> String {
        self.secrets.borrow().mask(text)
    }

    /// Ordinary progress output, hidden by `--quiet`.
    pub fn say(&self, text: &str) {
        if self.json || self.verbosity == Verbosity::Quiet {
            return;
        }
        println!("{}", self.mask(text));
    }

    /// Detail only shown with `--verbose`.
    pub fn detail(&self, text: &str) {
        if self.json || self.verbosity != Verbosity::Verbose {
            return;
        }
        println!("{}", self.mask(text));
    }

    pub fn warn(&self, text: &str) {
        if self.json {
            self.event(&[("type", "warning"), ("message", text)]);
            return;
        }
        eprintln!("warning: {}", self.mask(text));
    }

    pub fn error(&self, text: &str) {
        if self.json {
            self.event(&[("type", "error"), ("message", text)]);
            return;
        }
        eprintln!("error: {}", self.mask(text));
    }

    /// One JSON object per line, for tools that read shlane's output.
    pub fn event(&self, fields: &[(&str, &str)]) {
        if !self.json {
            return;
        }
        let mut line = String::from("{");
        for (index, (key, value)) in fields.iter().enumerate() {
            if index > 0 {
                line.push(',');
            }
            line.push_str(&format!(
                "\"{}\":\"{}\"",
                escape(key),
                escape(&self.mask(value))
            ));
        }
        line.push('}');
        let mut handle = std::io::stdout().lock();
        let _ = writeln!(handle, "{line}");
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_json_strings() {
        assert_eq!(escape(r#"a "b" \ c"#), r#"a \"b\" \\ c"#);
        assert_eq!(escape("line\nbreak"), "line\\nbreak");
        assert_eq!(escape("\u{1}"), "\\u0001");
    }
}
