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
///
/// A bare `${x}` looks in parameters then environment, as in v0.1.0. The
/// namespaced forms say exactly where to look: `${params.x}`, `${env.X}` and
/// `${shlane.lane}`.
pub struct Vars<'a> {
    pub params: &'a BTreeMap<String, String>,
    pub env: &'a BTreeMap<String, String>,
    pub meta: &'a BTreeMap<String, String>,
    /// `steps.<id>.<key>`, from steps that have already run.
    pub outputs: &'a BTreeMap<String, String>,
    /// During a dry run a step that was skipped has produced nothing, so a
    /// later reference to its output stands in for itself rather than ending
    /// the run. Outside a dry run, an unknown output is still an error.
    pub dry_run: bool,
}

impl Vars<'_> {
    /// What to show for a reference that cannot resolve during a dry run.
    fn placeholder(name: &str) -> String {
        format!("<{name}>")
    }

    /// The value to substitute, or nothing if the reference cannot resolve.
    fn resolve(&self, name: &str) -> Option<String> {
        if let Some(value) = self.get(name) {
            return Some(value.clone());
        }
        // Only step outputs: a missing parameter is still a mistake worth
        // catching before the lane runs for real.
        if self.dry_run && name.starts_with("steps.") {
            return Some(Self::placeholder(name));
        }
        None
    }

    fn get(&self, name: &str) -> Option<&String> {
        if let Some(rest) = name.strip_prefix("params.") {
            return self.params.get(rest);
        }
        if let Some(rest) = name.strip_prefix("env.") {
            return self.env.get(rest);
        }
        if let Some(rest) = name.strip_prefix("shlane.") {
            return self.meta.get(rest);
        }
        if let Some(rest) = name.strip_prefix("steps.") {
            return self.outputs.get(rest);
        }

        self.params.get(name).or_else(|| self.env.get(name))
    }
}

/// Where in the surrounding command a `${...}` reference appears.
///
/// Quoting has to respect this: wrapping a value in single quotes is right in
/// bare text, but inside `"..."` it would put literal quote characters into the
/// command's output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Quoting {
    Bare,
    Single,
    Double,
}

/// Quote a value so the shell treats it as literal text in `quoting` context.
fn quote_for(value: &str, quoting: Quoting) -> String {
    match quoting {
        Quoting::Bare => {
            if value.is_empty() {
                return "''".to_string();
            }
            if value.chars().all(|c| {
                c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '=' | '@')
            }) {
                return value.to_string();
            }
            format!("'{}'", value.replace('\'', r"'\''"))
        }
        // Close the string, add a quoted literal quote, reopen it.
        Quoting::Single => value.replace('\'', r"'\''"),
        // Inside double quotes the shell still acts on these four.
        Quoting::Double => {
            let mut out = String::with_capacity(value.len());
            for c in value.chars() {
                if matches!(c, '\\' | '"' | '$' | '`') {
                    out.push('\\');
                }
                out.push(c);
            }
            out
        }
    }
}

/// Substitute without any shell quoting.
///
/// For values that never reach a shell: `env:` entries, `workdir:` and the
/// `with:` map of a `lane:` step are handed to the process directly.
pub fn interpolate_plain(input: &str, vars: &Vars<'_>) -> Result<String> {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;

    while i < input.len() {
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
            let name = reference.strip_suffix(":raw").unwrap_or(reference);
            let value = vars
                .resolve(name)
                .ok_or_else(|| ShlaneError::UndefinedVariable {
                    name: name.to_string(),
                    source_text: input.to_string(),
                })?;
            out.push_str(&value);
            i += 2 + end + 1;
            continue;
        }
        let ch = input[i..].chars().next().unwrap_or_default();
        out.push(ch);
        i += ch.len_utf8();
    }

    Ok(out)
}

pub fn interpolate(input: &str, vars: &Vars<'_>) -> Result<String> {
    let mut out = String::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    let mut quoting = Quoting::Bare;

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
                .resolve(name)
                .ok_or_else(|| ShlaneError::UndefinedVariable {
                    name: name.to_string(),
                    source_text: input.to_string(),
                })?;

            if raw {
                out.push_str(&value);
            } else {
                out.push_str(&quote_for(&value, quoting));
            }
            i += 2 + end + 1;
            continue;
        }

        let ch = input[i..].chars().next().unwrap_or_default();
        match ch {
            // A backslash escapes the next character, except inside '...'.
            '\\' if quoting != Quoting::Single => {
                out.push(ch);
                i += ch.len_utf8();
                if let Some(next) = input[i..].chars().next() {
                    out.push(next);
                    i += next.len_utf8();
                }
                continue;
            }
            '\'' if quoting == Quoting::Bare => quoting = Quoting::Single,
            '\'' if quoting == Quoting::Single => quoting = Quoting::Bare,
            '"' if quoting == Quoting::Bare => quoting = Quoting::Double,
            '"' if quoting == Quoting::Double => quoting = Quoting::Bare,
            _ => {}
        }
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
        render_with(input, params, env, &[])
    }

    fn render_with(
        input: &str,
        params: &[(&str, &str)],
        env: &[(&str, &str)],
        meta: &[(&str, &str)],
    ) -> Result<String> {
        let params = map(params);
        let env = map(env);
        let meta = map(meta);
        let outputs = map(&[("build.stdout", "artifact.ipa")]);
        interpolate(
            input,
            &Vars {
                params: &params,
                env: &env,
                meta: &meta,
                outputs: &outputs,
                dry_run: false,
            },
        )
    }

    #[test]
    fn a_dry_run_stands_in_for_an_output_that_does_not_exist_yet() {
        let empty = BTreeMap::new();
        let vars = Vars {
            params: &empty,
            env: &empty,
            meta: &empty,
            outputs: &empty,
            dry_run: true,
        };

        let out = interpolate("echo ${steps.build.ipa}", &vars).expect("should render");
        assert_eq!(out, "echo '<steps.build.ipa>'");

        // A misspelled parameter is still a mistake, dry run or not.
        assert!(interpolate("echo ${typo}", &vars).is_err());
    }

    #[test]
    fn step_outputs_resolve() {
        let out = render("echo ${steps.build.stdout}", &[], &[]).expect("should render");
        assert_eq!(out, "echo artifact.ipa");
    }

    #[test]
    fn an_unknown_step_output_is_an_error() {
        let err = render("echo ${steps.nope.stdout}", &[], &[]).expect_err("should fail");
        assert!(
            matches!(err, ShlaneError::UndefinedVariable { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn namespaces_select_where_to_look() {
        let out =
            render("${params.x}/${env.x}", &[("x", "p")], &[("x", "e")]).expect("should render");
        assert_eq!(out, "p/e");
    }

    #[test]
    fn a_namespaced_reference_does_not_fall_back() {
        let err = render("${params.x}", &[], &[("x", "e")]).expect_err("should fail");
        assert!(
            matches!(err, ShlaneError::UndefinedVariable { .. }),
            "got {err:?}"
        );
    }

    #[test]
    fn shlane_metadata_is_available() {
        let out =
            render_with("${shlane.lane}", &[], &[], &[("lane", "beta")]).expect("should render");
        assert_eq!(out, "beta");
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
    fn inside_double_quotes_the_value_is_escaped_not_wrapped() {
        let out =
            render(r#"echo "to ${x}""#, &[("x", "staging; rm -rf /")], &[]).expect("should render");
        assert_eq!(out, r#"echo "to staging; rm -rf /""#);
    }

    #[test]
    fn inside_double_quotes_shell_metacharacters_are_escaped() {
        let out =
            render(r#"echo "${x}""#, &[("x", "$(whoami) `id` \"q\"")], &[]).expect("should render");
        assert_eq!(out, r#"echo "\$(whoami) \`id\` \"q\"""#);
    }

    #[test]
    fn inside_single_quotes_the_value_closes_and_reopens_the_string() {
        let out = render("echo 'to ${x}'", &[("x", "it's fine")], &[]).expect("should render");
        assert_eq!(out, r"echo 'to it'\''s fine'");
    }

    #[test]
    fn quote_state_resets_after_a_closed_string() {
        let out = render(r#"echo "a" ${x}"#, &[("x", "b c")], &[]).expect("should render");
        assert_eq!(out, r#"echo "a" 'b c'"#);
    }

    #[test]
    fn an_escaped_quote_does_not_open_a_string() {
        let out = render(r#"echo \" ${x}"#, &[("x", "b c")], &[]).expect("should render");
        assert_eq!(out, r#"echo \" 'b c'"#);
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
