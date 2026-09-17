//! Service-account authentication for Google APIs.
//!
//! Both Play and Firebase want an OAuth2 access token obtained by signing a
//! JWT with the service account's RSA key. The signing key is `ring`, which is
//! already in the tree via rustls.

use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

/// How long the assertion is valid. Google allows up to an hour.
const LIFETIME_SECONDS: u64 = 3600;

const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";

#[derive(Debug, Deserialize)]
pub struct ServiceAccount {
    pub client_email: String,
    pub private_key: String,
    #[serde(default)]
    pub token_uri: Option<String>,
}

impl ServiceAccount {
    /// Accept either the JSON itself or a path to it, since CI hands it over
    /// both ways.
    pub fn load(value: &str, root: &std::path::Path) -> Result<Self, String> {
        let text = if value.trim_start().starts_with('{') {
            value.to_string()
        } else {
            let path = root.join(value.trim());
            std::fs::read_to_string(&path)
                .map_err(|err| format!("cannot read {}: {err}", path.display()))?
        };

        // JSON is valid YAML, so this needs no extra parser.
        serde_yaml::from_str(&text)
            .map_err(|err| format!("this does not look like a service account key: {err}"))
    }

    pub fn token_uri(&self) -> &str {
        self.token_uri.as_deref().unwrap_or(TOKEN_URI)
    }
}

/// base64url without padding, as JWTs use.
pub fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();

    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let triple = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        let indices = [
            (triple >> 18) & 0x3F,
            (triple >> 12) & 0x3F,
            (triple >> 6) & 0x3F,
            triple & 0x3F,
        ];
        for (position, index) in indices.iter().enumerate() {
            if position <= chunk.len() {
                out.push(ALPHABET[*index as usize] as char);
            }
        }
    }

    out
}

/// Standard base64, ignoring whitespace and padding.
pub fn decode_base64(text: &str) -> Result<Vec<u8>, String> {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut buffer: u32 = 0;
    let mut bits = 0;
    let mut out = Vec::new();

    for byte in text.bytes() {
        if byte.is_ascii_whitespace() || byte == b'=' {
            continue;
        }
        let Some(value) = ALPHABET.iter().position(|candidate| *candidate == byte) else {
            return Err(format!("'{}' is not base64", byte as char));
        };
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }

    Ok(out)
}

/// Strip the PEM armour and decode the DER inside.
pub fn pem_to_der(pem: &str) -> Result<Vec<u8>, String> {
    let body: String = pem
        .lines()
        .filter(|line| !line.starts_with("-----"))
        .collect();
    if body.is_empty() {
        return Err("the private key is empty or not in PEM form".to_string());
    }
    decode_base64(&body)
}

/// The signed part of the assertion, so it can be checked without signing.
pub fn assertion_claims(account: &ServiceAccount, scope: &str, now: u64) -> String {
    let header = base64url(br#"{"alg":"RS256","typ":"JWT"}"#);
    let claims = format!(
        r#"{{"iss":"{}","scope":"{scope}","aud":"{}","iat":{now},"exp":{}}}"#,
        account.client_email,
        account.token_uri(),
        now + LIFETIME_SECONDS
    );
    format!("{header}.{}", base64url(claims.as_bytes()))
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default()
}

/// Build a signed `assertion` for the JWT bearer grant.
pub fn assertion(account: &ServiceAccount, scope: &str) -> Result<String, String> {
    let der = pem_to_der(&account.private_key)?;
    let key = ring::signature::RsaKeyPair::from_pkcs8(&der)
        .map_err(|err| format!("the service account's private key could not be read: {err}"))?;

    let signed = assertion_claims(account, scope, now_seconds());
    let mut signature = vec![0; key.public().modulus_len()];
    key.sign(
        &ring::signature::RSA_PKCS1_SHA256,
        &ring::rand::SystemRandom::new(),
        signed.as_bytes(),
        &mut signature,
    )
    .map_err(|err| format!("could not sign the request: {err}"))?;

    Ok(format!("{signed}.{}", base64url(&signature)))
}

/// Percent-encode a value for a form body.
pub fn form_encode(value: &str) -> String {
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

/// The body of the token request.
pub fn token_request_body(assertion: &str) -> String {
    format!(
        "grant_type={}&assertion={}",
        form_encode("urn:ietf:params:oauth:grant-type:jwt-bearer"),
        form_encode(assertion)
    )
}

#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> ServiceAccount {
        ServiceAccount {
            client_email: "bot@example.iam.gserviceaccount.com".to_string(),
            private_key: String::new(),
            token_uri: None,
        }
    }

    #[test]
    fn encodes_base64url_without_padding() {
        assert_eq!(base64url(b"hello"), "aGVsbG8");
        assert_eq!(base64url(b"hi"), "aGk");
        assert_eq!(base64url(b""), "");
        // base64url uses - and _ instead of + and /
        assert_eq!(base64url(&[251, 255]), "-_8");
    }

    #[test]
    fn round_trips_through_the_standard_alphabet() {
        assert_eq!(decode_base64("aGVsbG8=").expect("valid"), b"hello");
        assert!(decode_base64("!!!").is_err());
    }

    #[test]
    fn strips_pem_armour() {
        let pem = "-----BEGIN PRIVATE KEY-----\naGVsbG8=\n-----END PRIVATE KEY-----\n";
        assert_eq!(pem_to_der(pem).expect("valid"), b"hello");
        assert!(pem_to_der("").is_err());
    }

    #[test]
    fn claims_carry_the_account_scope_and_expiry() {
        let claims = assertion_claims(&account(), "https://example.com/scope", 1_000);
        let (header, payload) = claims.split_once('.').expect("two parts");
        assert!(!header.is_empty());

        let decoded = String::from_utf8(
            decode_base64(&payload.replace('-', "+").replace('_', "/")).expect("valid"),
        )
        .expect("utf8");

        assert!(
            decoded.contains("\"iss\":\"bot@example.iam.gserviceaccount.com\""),
            "{decoded}"
        );
        assert!(
            decoded.contains("\"scope\":\"https://example.com/scope\""),
            "{decoded}"
        );
        assert!(decoded.contains("\"iat\":1000"), "{decoded}");
        assert!(decoded.contains("\"exp\":4600"), "{decoded}");
        assert!(decoded.contains("oauth2.googleapis.com"), "{decoded}");
    }

    #[test]
    fn reads_a_service_account_from_json() {
        let json = r#"{"client_email":"a@b.com","private_key":"-----BEGIN PRIVATE KEY-----\nx\n-----END PRIVATE KEY-----\n","project_id":"demo"}"#;
        let account = ServiceAccount::load(json, std::path::Path::new(".")).expect("valid");
        assert_eq!(account.client_email, "a@b.com");
        assert!(account.private_key.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn rejects_json_that_is_not_a_service_account() {
        assert!(ServiceAccount::load("{\"hello\":1}", std::path::Path::new(".")).is_err());
    }

    #[test]
    fn form_encodes_the_grant_type() {
        let body = token_request_body("a.b.c");
        assert!(body.contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer"));
        assert!(body.ends_with("assertion=a.b.c"));
    }
}
