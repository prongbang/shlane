//! Actions that talk to a network.

use crate::actions::context::ActionContext;
use crate::actions::{Action, ActionOutput, ArgSpec, Args};
use crate::error::Result;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(30);

/// Attempts, and how long to wait before each retry.
const RETRIES: u32 = 3;
const BACKOFF: Duration = Duration::from_millis(500);

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        // A 4xx or 5xx is a result to report, not a transport error.
        .http_status_as_error(false)
        .timeout_global(Some(TIMEOUT))
        .build()
        .into()
}

pub struct Response {
    pub status: u16,
    pub body: String,
}

/// What to send, if anything.
pub enum Payload<'a> {
    Empty,
    Text(&'a str),
    Bytes(&'a [u8]),
}

/// Send a request, retrying transport failures and 5xx.
///
/// CI networks fail often enough that one attempt is not enough
/// (`docs/plan/11-ci-integration.md`).
pub fn send(
    ctx: &ActionContext<'_>,
    method: &str,
    url: &str,
    headers: &[(String, String)],
    body: Payload<'_>,
) -> std::result::Result<Response, String> {
    let agent = agent();
    let mut last = String::new();

    for attempt in 1..=RETRIES {
        // ureq types requests by whether they carry a body, so the two shapes
        // cannot share one builder variable.
        let result = match method {
            "POST" | "PUT" | "PATCH" => {
                let mut request = match method {
                    "POST" => agent.post(url),
                    "PUT" => agent.put(url),
                    _ => agent.patch(url),
                };
                for (name, value) in headers {
                    request = request.header(name, value);
                }
                match body {
                    Payload::Empty => request.send(""),
                    Payload::Text(text) => request.send(text),
                    Payload::Bytes(bytes) => request.send(bytes),
                }
            }
            "GET" | "DELETE" | "HEAD" => {
                let mut request = match method {
                    "GET" => agent.get(url),
                    "DELETE" => agent.delete(url),
                    _ => agent.head(url),
                };
                for (name, value) in headers {
                    request = request.header(name, value);
                }
                request.call()
            }
            other => return Err(format!("unsupported HTTP method '{other}'")),
        };

        match result {
            Ok(mut response) => {
                let status = response.status().as_u16();
                let text = response
                    .body_mut()
                    .read_to_string()
                    .unwrap_or_else(|err| format!("<could not read body: {err}>"));

                if status < 500 || attempt == RETRIES {
                    return Ok(Response { status, body: text });
                }
                last = format!("HTTP {status}");
            }
            Err(err) => {
                last = err.to_string();
                if attempt == RETRIES {
                    return Err(last);
                }
            }
        }

        ctx.ui
            .say(&format!("Attempt {attempt} failed ({last}); retrying..."));
        std::thread::sleep(BACKOFF * attempt);
    }

    Err(last)
}

/// Fetch a URL as bytes.
///
/// Separate from [`send`] because that reads the body as a string, which is
/// right for an API and wrong for a `.zip`: a keystore read through
/// `read_to_string` comes out replaced with U+FFFD and only fails later, when
/// something tries to sign with it.
pub fn fetch_bytes(
    ctx: &ActionContext<'_>,
    url: &str,
    headers: &[(String, String)],
) -> std::result::Result<(u16, Vec<u8>), String> {
    let agent = agent();
    let mut last = String::new();

    for attempt in 1..=RETRIES {
        let mut request = agent.get(url);
        for (name, value) in headers {
            request = request.header(name, value);
        }

        match request.call() {
            Ok(mut response) => {
                let status = response.status().as_u16();
                let bytes = response
                    .body_mut()
                    .with_config()
                    .limit(u64::MAX)
                    .read_to_vec()
                    .map_err(|err| format!("could not read the body: {err}"))?;

                if status < 500 || attempt == RETRIES {
                    return Ok((status, bytes));
                }
                last = format!("HTTP {status}");
            }
            Err(err) => {
                last = err.to_string();
                if attempt == RETRIES {
                    return Err(last);
                }
            }
        }

        ctx.ui
            .say(&format!("Attempt {attempt} failed ({last}); retrying..."));
        std::thread::sleep(BACKOFF * attempt);
    }

    Err(last)
}

fn parse_headers(raw: &str) -> Vec<(String, String)> {
    raw.split('\n')
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .filter(|(name, _)| !name.is_empty())
        .collect()
}

pub struct HttpRequest;

impl Action for HttpRequest {
    fn name(&self) -> &'static str {
        "http_request"
    }

    fn description(&self) -> &'static str {
        "Send an HTTP request"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("url", "Where to send it").required(),
            ArgSpec::new("method", "GET, POST, PUT, ...").default("GET"),
            ArgSpec::new("body", "Request body"),
            ArgSpec::new("headers", "One `Name: value` per line"),
            ArgSpec::new(
                "expect_status",
                "Fail unless the response has this status; empty means any 2xx",
            ),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let url = args.get_or("url", "");
        let method = args.get_or("method", "GET").to_uppercase();
        let headers = parse_headers(args.get_or("headers", ""));

        if ctx.dry_run {
            ctx.ui.say(&format!("Would send {method} {url}"));
            return Ok(ActionOutput::new().with("status", "0").with("body", ""));
        }

        ctx.ui.say(&format!("{method} {url}"));
        let payload = match args.get("body") {
            Some(body) => Payload::Text(body),
            None => Payload::Empty,
        };
        let response = send(ctx, &method, url, &headers, payload).map_err(|message| {
            ctx.error(self.name(), format!("{method} {url} failed: {message}"))
        })?;

        let expected = args.get_or("expect_status", "");
        let ok = if expected.is_empty() {
            (200..300).contains(&response.status)
        } else {
            expected.parse::<u16>().ok() == Some(response.status)
        };

        if !ok {
            let body = response.body.trim();
            let detail = if body.is_empty() {
                String::new()
            } else {
                format!("\n  body: {}", body.chars().take(500).collect::<String>())
            };
            return Err(ctx.error(
                self.name(),
                format!("{method} {url} returned HTTP {}{detail}", response.status),
            ));
        }

        Ok(ActionOutput::new()
            .with("status", response.status.to_string())
            .with("body", response.body))
    }
}

pub struct NotifySlack;

impl Action for NotifySlack {
    fn name(&self) -> &'static str {
        "notify_slack"
    }

    fn description(&self) -> &'static str {
        "Post a message to a Slack incoming webhook"
    }

    fn schema(&self) -> Vec<ArgSpec> {
        vec![
            ArgSpec::new("webhook", "Incoming webhook URL")
                .required()
                .sensitive(),
            ArgSpec::new("text", "Message to post").required(),
            ArgSpec::new("channel", "Override the webhook's default channel"),
            ArgSpec::new("username", "Override the webhook's default name"),
        ]
    }

    fn run(&self, ctx: &mut ActionContext<'_>, args: &Args) -> Result<ActionOutput> {
        let webhook = args.get_or("webhook", "");
        // The URL is the credential; never let it reach the output.
        ctx.mark_secret(webhook);

        let mut fields = vec![("text", args.get_or("text", ""))];
        if let Some(channel) = args.get("channel") {
            fields.push(("channel", channel));
        }
        if let Some(username) = args.get("username") {
            fields.push(("username", username));
        }

        let payload = format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(key, value)| format!("\"{key}\":\"{}\"", escape(value)))
                .collect::<Vec<_>>()
                .join(",")
        );

        if ctx.dry_run {
            ctx.ui.say(&format!("Would post to Slack: {payload}"));
            return Ok(ActionOutput::new().with("status", "0"));
        }

        ctx.ui.say("Posting to Slack");
        let response = send(
            ctx,
            "POST",
            webhook,
            &[("Content-Type".to_string(), "application/json".to_string())],
            Payload::Text(&payload),
        )
        .map_err(|message| ctx.error(self.name(), format!("could not reach Slack: {message}")))?;

        if !(200..300).contains(&response.status) {
            return Err(ctx.error(
                self.name(),
                format!(
                    "Slack returned HTTP {}: {}",
                    response.status,
                    response.body.trim()
                ),
            ));
        }

        Ok(ActionOutput::new().with("status", response.status.to_string()))
    }
}

pub fn escape(text: &str) -> String {
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
    fn parses_headers() {
        let headers = parse_headers("Authorization: Bearer x\nContent-Type: application/json");
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].0, "Authorization");
        assert_eq!(headers[0].1, "Bearer x");
    }

    #[test]
    fn ignores_malformed_header_lines() {
        assert!(parse_headers("nonsense").is_empty());
        assert!(parse_headers("").is_empty());
    }

    #[test]
    fn escapes_json_payloads() {
        assert_eq!(escape("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(escape("a\nb"), "a\\nb");
    }
}
