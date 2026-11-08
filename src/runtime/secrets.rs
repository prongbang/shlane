//! Keeping secret values out of the output.
//!
//! Everything shlane prints goes through [`Secrets::mask`]: the command it is
//! about to run, the child's stdout and stderr, error messages, the summary and
//! the JSON event stream. See `docs/plan/10-secrets-and-env.md`.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

/// What a masked value is replaced with.
const MASK: &str = "***";

/// Values shorter than this are not worth masking: they match far too much
/// ordinary text, and a two-character secret is not a secret.
const MIN_LENGTH: usize = 4;

/// Environment variables whose names say they hold a secret.
const SENSITIVE_SUFFIXES: [&str; 6] = [
    "_TOKEN",
    "_SECRET",
    "_PASSWORD",
    "_KEY",
    "_CREDENTIALS",
    "_API_KEY",
];

const SENSITIVE_NAMES: [&str; 4] = ["TOKEN", "SECRET", "PASSWORD", "CREDENTIALS"];

#[derive(Debug, Default, Clone)]
pub struct Secrets {
    values: BTreeSet<String>,
}

impl Secrets {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a value to be hidden wherever it appears.
    pub fn add(&mut self, value: &str) {
        let value = value.trim();
        if value.len() < MIN_LENGTH {
            return;
        }
        self.values.insert(value.to_string());

        // Tools often re-encode a secret before logging it, so hide the
        // encoded forms too.
        let encoded = url_encode(value);
        if encoded != value && encoded.len() >= MIN_LENGTH {
            self.values.insert(encoded);
        }
    }

    /// Register an environment variable if its name looks sensitive.
    pub fn add_env(&mut self, name: &str, value: &str) {
        if is_sensitive_name(name) {
            self.add(value);
        }
    }

    pub fn mask(&self, text: &str) -> String {
        if self.values.is_empty() {
            return text.to_string();
        }
        let mut masked = text.to_string();
        // Longest first, so a secret that contains another is replaced whole.
        let mut values: Vec<&String> = self.values.iter().collect();
        values.sort_by_key(|value| std::cmp::Reverse(value.len()));
        for value in values {
            if masked.contains(value.as_str()) {
                masked = masked.replace(value.as_str(), MASK);
            }
        }
        masked
    }
}

pub type SharedSecrets = Rc<RefCell<Secrets>>;

pub fn is_sensitive_name(name: &str) -> bool {
    let upper = name.to_ascii_uppercase();
    SENSITIVE_NAMES.contains(&upper.as_str())
        || SENSITIVE_SUFFIXES
            .iter()
            .any(|suffix| upper.ends_with(suffix))
}

fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_a_registered_value() {
        let mut secrets = Secrets::new();
        secrets.add("hunter2000");
        assert_eq!(secrets.mask("token=hunter2000 ok"), "token=*** ok");
    }

    #[test]
    fn masks_every_occurrence() {
        let mut secrets = Secrets::new();
        secrets.add("s3cret!");
        assert_eq!(secrets.mask("s3cret! and s3cret!"), "*** and ***");
    }

    #[test]
    fn masks_url_encoded_forms() {
        let mut secrets = Secrets::new();
        secrets.add("a b/c+d");
        let masked = secrets.mask("https://x/?k=a%20b%2Fc%2Bd");
        assert!(!masked.contains("a%20b"), "got: {masked}");
    }

    #[test]
    fn prefers_the_longest_match() {
        let mut secrets = Secrets::new();
        secrets.add("abcd");
        secrets.add("abcdefgh");
        assert_eq!(secrets.mask("abcdefgh"), "***");
    }

    #[test]
    fn ignores_values_too_short_to_be_secret() {
        let mut secrets = Secrets::new();
        secrets.add("ab");
        assert_eq!(secrets.mask("ab cd"), "ab cd");
    }

    #[test]
    fn recognises_sensitive_names() {
        assert!(is_sensitive_name("GITHUB_TOKEN"));
        assert!(is_sensitive_name("keystore_password"));
        assert!(is_sensitive_name("ASC_API_KEY"));
        assert!(is_sensitive_name("PASSWORD"));
        assert!(!is_sensitive_name("APP_ENV"));
        assert!(!is_sensitive_name("KEYBOARD"));
    }

    #[test]
    fn only_registers_env_values_with_sensitive_names() {
        let mut secrets = Secrets::new();
        secrets.add_env("APP_ENV", "production");
        secrets.add_env("API_TOKEN", "abcd1234");
        assert_eq!(secrets.mask("production abcd1234"), "production ***");
    }
}
