//! Just enough Ruby to read a Fastfile.
//!
//! Not a parser: a set of shapes. Anything that does not match one is handed
//! back untouched so the caller can carry it across as a TODO.

/// `platform :ios do`
pub fn platform_block(line: &str) -> Option<String> {
    let rest = line.strip_prefix("platform ")?;
    let name = rest.trim().strip_prefix(':')?;
    let name = name.split_whitespace().next()?;
    line.ends_with(" do").then(|| name.to_string())
}

/// `desc "..."`
pub fn description(line: &str) -> Option<String> {
    let rest = line.strip_prefix("desc ")?.trim();
    Some(unquote(rest))
}

/// `lane :name do |options|` / `private_lane :name do`
///
/// Returns the name and whether it is private.
pub fn lane_start(line: &str) -> Option<(String, bool)> {
    let (rest, private) = match line.strip_prefix("private_lane ") {
        Some(rest) => (rest, true),
        None => (line.strip_prefix("lane ")?, false),
    };

    let rest = rest.trim().strip_prefix(':')?;
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();

    (!name.is_empty() && line.contains(" do")).then_some((name, private))
}

/// True for a line that opens a block shlane cannot represent.
pub fn opens_block(line: &str) -> bool {
    if line.starts_with("lane ")
        || line.starts_with("private_lane ")
        || line.starts_with("platform ")
    {
        return false;
    }
    line.ends_with(" do")
        || line.ends_with("do |")
        || line.contains(" do |")
        || line.starts_with("if ")
        || line.starts_with("unless ")
        || line.starts_with("case ")
        || line.starts_with("begin")
        || line.starts_with("def ")
        || line.starts_with("before_all")
        || line.starts_with("after_all")
        || line.starts_with("error do")
}

/// `sh "cmd"` / `sh("cmd")`
pub fn sh_command(line: &str) -> Option<String> {
    let rest = line
        .strip_prefix("sh(")
        .map(|rest| rest.trim_end().trim_end_matches(')'))
        .or_else(|| line.strip_prefix("sh "))?;
    Some(unquote(rest.trim()))
}

/// `action_name(key: value, other: "x")` or a bare `action_name`
pub fn action_call(line: &str) -> Option<(String, Vec<(String, String)>)> {
    let name: String = line
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_')
        .collect();
    if name.is_empty() || !name.starts_with(|c: char| c.is_ascii_lowercase()) {
        return None;
    }

    let rest = line[name.len()..].trim();
    if rest.is_empty() {
        return Some((name, Vec::new()));
    }

    // `gym(scheme: "X")` and `gym scheme: "X"` are both Ruby; only the first
    // has brackets to strip.
    let inside = match rest.strip_prefix('(') {
        Some(inner) => inner.strip_suffix(')')?,
        None => rest,
    };

    Some((name, arguments(inside)))
}

/// Split `key: value, other: "a, b"` without cutting inside quotes.
pub fn arguments(text: &str) -> Vec<(String, String)> {
    let mut args = Vec::new();
    for part in split_top_level(text) {
        let Some((key, value)) = part.split_once(':') else {
            continue;
        };
        let key = key.trim();
        if key.is_empty() || !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
            continue;
        }
        args.push((key.to_string(), unquote(value.trim())));
    }
    args
}

fn split_top_level(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut depth = 0usize;

    for ch in text.chars() {
        match ch {
            '"' | '\'' => {
                match quote {
                    Some(open) if open == ch => quote = None,
                    None => quote = Some(ch),
                    _ => {}
                }
                current.push(ch);
            }
            '(' | '[' | '{' if quote.is_none() => {
                depth += 1;
                current.push(ch);
            }
            ')' | ']' | '}' if quote.is_none() => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ',' if quote.is_none() && depth == 0 => {
                parts.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(ch),
        }
    }

    if !current.trim().is_empty() {
        parts.push(current.trim().to_string());
    }
    parts
}

/// Strip surrounding quotes, leaving everything else alone.
pub fn unquote(value: &str) -> String {
    let value = value.trim();
    for quote in ['"', '\''] {
        if value.len() >= 2 && value.starts_with(quote) && value.ends_with(quote) {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}

/// Turn Ruby's ways of reaching a value into shlane references.
///
/// `ENV["X"]` and `options[:x]` are how a Fastfile reads configuration, and
/// both have a direct equivalent.
pub fn interpolate(value: &str) -> String {
    let mut out = value.to_string();

    // `#{...}` first: it contains the same expressions, and converting those
    // first would leave this pass wrapping an already-converted `${x}` again.
    out = replace_all(&out, "#{", "}", |inner| {
        let inner = inner.trim();
        match inner
            .strip_prefix("options[:")
            .and_then(|rest| rest.strip_suffix(']'))
        {
            Some(name) => format!("${{{name}}}"),
            None => match inner
                .strip_prefix("ENV[\"")
                .and_then(|rest| rest.strip_suffix("\"]"))
            {
                Some(name) => format!("${{{name}}}"),
                None => format!("${{{inner}}}"),
            },
        }
    });

    out = replace_all(&out, "ENV[\"", "\"]", |name| format!("${{{name}}}"));
    out = replace_all(&out, "ENV['", "']", |name| format!("${{{name}}}"));
    out = replace_all(&out, "options[:", "]", |name| format!("${{{name}}}"));

    out
}

fn replace_all(text: &str, open: &str, close: &str, build: impl Fn(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;

    while let Some(start) = rest.find(open) {
        out.push_str(&rest[..start]);
        let after = &rest[start + open.len()..];
        let Some(end) = after.find(close) else {
            // Unbalanced: leave the rest as it is rather than losing it.
            out.push_str(&rest[start..]);
            return out;
        };
        out.push_str(&build(&after[..end]));
        rest = &after[end + close.len()..];
    }

    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_a_platform_block() {
        assert_eq!(platform_block("platform :ios do").as_deref(), Some("ios"));
        assert_eq!(
            platform_block("platform :android do").as_deref(),
            Some("android")
        );
        assert!(platform_block("platform :ios").is_none());
        assert!(platform_block("lane :beta do").is_none());
    }

    #[test]
    fn recognises_lanes() {
        assert_eq!(
            lane_start("lane :beta do"),
            Some(("beta".to_string(), false))
        );
        assert_eq!(
            lane_start("lane :beta do |options|"),
            Some(("beta".to_string(), false))
        );
        assert_eq!(
            lane_start("private_lane :setup do"),
            Some(("setup".to_string(), true))
        );
        assert!(lane_start("laneish :x do").is_none());
    }

    #[test]
    fn recognises_sh() {
        assert_eq!(
            sh_command("sh \"bundle install\"").as_deref(),
            Some("bundle install")
        );
        assert_eq!(
            sh_command("sh(\"pod install\")").as_deref(),
            Some("pod install")
        );
        assert!(sh_command("shell \"x\"").is_none());
    }

    #[test]
    fn reads_an_action_call() {
        let (name, args) = action_call("gym(scheme: \"MyApp\", clean: true)").expect("an action");
        assert_eq!(name, "gym");
        assert_eq!(args.len(), 2);
        assert_eq!(args[0], ("scheme".to_string(), "MyApp".to_string()));
        assert_eq!(args[1], ("clean".to_string(), "true".to_string()));
    }

    #[test]
    fn reads_an_action_without_brackets() {
        let (name, args) = action_call("slack message: \"hi\"").expect("an action");
        assert_eq!(name, "slack");
        assert_eq!(args[0].1, "hi");
    }

    #[test]
    fn reads_a_bare_action() {
        let (name, args) = action_call("ensure_git_status_clean").expect("an action");
        assert_eq!(name, "ensure_git_status_clean");
        assert!(args.is_empty());
    }

    #[test]
    fn a_comma_inside_quotes_does_not_split_arguments() {
        let args = arguments("message: \"one, two\", channel: \"#releases\"");
        assert_eq!(args.len(), 2, "{args:?}");
        assert_eq!(args[0].1, "one, two");
        assert_eq!(args[1].1, "#releases");
    }

    #[test]
    fn a_bracketed_value_stays_whole() {
        let args = arguments("devices: [\"iPhone 15\", \"iPad\"], clean: true");
        assert_eq!(args.len(), 2, "{args:?}");
        assert_eq!(args[0].1, "[\"iPhone 15\", \"iPad\"]");
    }

    #[test]
    fn translates_env_and_options() {
        assert_eq!(interpolate("ENV[\"SLACK_URL\"]"), "${SLACK_URL}");
        assert_eq!(interpolate("ENV['TOKEN']"), "${TOKEN}");
        assert_eq!(interpolate("options[:version]"), "${version}");
    }

    #[test]
    fn translates_string_interpolation() {
        assert_eq!(
            interpolate("Shipped #{options[:version]} now"),
            "Shipped ${version} now"
        );
        assert_eq!(interpolate("to #{ENV[\"STAGE\"]}"), "to ${STAGE}");
    }

    #[test]
    fn leaves_plain_text_alone() {
        assert_eq!(interpolate("just a message"), "just a message");
    }

    #[test]
    fn an_unbalanced_reference_is_not_lost() {
        assert_eq!(interpolate("ENV[\"OOPS"), "ENV[\"OOPS");
    }

    #[test]
    fn knows_which_lines_open_a_block() {
        assert!(opens_block("if ENV[\"CI\"]"));
        assert!(opens_block("[1,2].each do |n|"));
        assert!(opens_block("before_all do"));
        assert!(!opens_block("lane :beta do"));
        assert!(!opens_block("gym(scheme: \"X\")"));
    }
}
