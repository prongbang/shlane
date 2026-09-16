//! `${...}` substitution for shell commands.
//!
//! Two things differ from v0.1.0, both deliberate (see
//! `docs/plan/03-config-schema.md`):
//!
//! * values are shell-quoted, so a parameter can no longer inject commands;
//! * an unknown name is an error instead of being passed through to the shell.
//!
//! `${name:raw}` opts out of quoting, for the cases where a value really is
//! meant to expand into several shell words.

use crate::error::{Result, ShlaneError};
use std::collections::BTreeMap;

/// Everything a `${...}` reference may resolve against.
pub struct Vars<'a> {
    pub params: &'a BTreeMap<String, String>,
    pub env: &'a BTreeMap<String, String>,
}

impl Vars<'_> {
    fn get(&self, name: &str) -> Option<&String> {
        self.params.get(name).or_else(|| self.env.get(name))
    }
}

/// Quote a value so a POSIX shell treats it as a single literal word.
pub fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '=' | '@'))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', r"'\''"))
}

pub fn interpolate(input: &str, vars: &Vars<'_>) -> Result<String> {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < input.len() {
        // `$${` is an escape for a literal `${`.
        if bytes[i] == b'$' && input[i..].starts_with("$${") {
            out.push_str("${");
            i += 3;
            continue;
        }

        if bytes[i] == b'$' && input[i..].starts_with("${") {
            let rest = &input[i + 2..];
            let Some(end) = rest.find('}') else {
                return Err(ShlaneError::UnterminatedVariable {
                    source_text: input.to_string(),
                });
            };
            let reference = &rest[..end];
            let (name, raw) = match reference.strip_suffix(":raw") {
                Some(name) => (name, true),
                None => (reference, false),
            };

            let value = vars
                .get(name)
                .ok_or_else(|| ShlaneError::UndefinedVariable {
                    name: name.to_string(),
                    source_text: input.to_string(),
                })?;

            if raw {
                out.push_str(value);
            } else {
                out.push_str(&shell_quote(value));
            }
            i += 2 + end + 1;
            continue;
        }

        let ch = input[i..].chars().next().unwrap_or_default();
        out.push(ch);
        i += ch.len_utf8();
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn render(input: &str, params: &[(&str, &str)], env: &[(&str, &str)]) -> Result<String> {
        let params = map(params);
        let env = map(env);
        interpolate(
            input,
            &Vars {
                params: &params,
                env: &env,
            },
        )
    }

    #[test]
    fn substitutes_a_parameter() {
        let out = render("echo ${name}", &[("name", "world")], &[]).expect("should render");
        assert_eq!(out, "echo world");
    }

    #[test]
    fn falls_back_to_env() {
        let out =
            render("echo ${APP_ENV}", &[], &[("APP_ENV", "production")]).expect("should render");
        assert_eq!(out, "echo production");
    }

    #[test]
    fn parameters_take_precedence_over_env() {
        let out = render("echo ${x}", &[("x", "param")], &[("x", "env")]).expect("should render");
        assert_eq!(out, "echo param");
    }

    #[test]
    fn quotes_values_that_would_otherwise_be_parsed_by_the_shell() {
        let out = render("echo ${x}", &[("x", "a; rm -rf /")], &[]).expect("should render");
        assert_eq!(out, r"echo 'a; rm -rf /'");
    }

    #[test]
    fn escapes_embedded_single_quotes() {
        let out = render("echo ${x}", &[("x", "it's")], &[]).expect("should render");
        assert_eq!(out, r"echo 'it'\''s'");
    }

    #[test]
    fn empty_values_stay_one_argument() {
        let out = render("echo ${x}", &[("x", "")], &[]).expect("should render");
        assert_eq!(out, "echo ''");
    }

    #[test]
    fn raw_suffix_opts_out_of_quoting() {
        let out = render(
            "cargo build ${flags:raw}",
            &[("flags", "--release --locked")],
            &[],
        )
        .expect("should render");
        assert_eq!(out, "cargo build --release --locked");
    }

    #[test]
    fn unknown_names_are_an_error() {
        let err = render("echo ${nope}", &[], &[]).expect_err("should fail");
        assert!(
            matches!(err, ShlaneError::UndefinedVariable { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn unterminated_reference_is_an_error() {
        let err = render("echo ${nope", &[], &[]).expect_err("should fail");
        assert!(
            matches!(err, ShlaneError::UnterminatedVariable { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn double_dollar_escapes_the_reference() {
        let out = render("echo $${literal}", &[], &[]).expect("should render");
        assert_eq!(out, "echo ${literal}");
    }

    #[test]
    fn leaves_shell_variables_alone() {
        let out = render("echo $HOME and $(date)", &[], &[]).expect("should render");
        assert_eq!(out, "echo $HOME and $(date)");
    }

    #[test]
    fn handles_several_references_in_one_string() {
        let out = render("${a}-${b}", &[("a", "1"), ("b", "2")], &[]).expect("should render");
        assert_eq!(out, "1-2");
    }

    #[test]
    fn preserves_multibyte_text() {
        let out = render("echo ${x} เสร็จแล้ว", &[("x", "ทดสอบ")], &[]).expect("should render");
        assert_eq!(out, "echo 'ทดสอบ' เสร็จแล้ว");
    }
}
