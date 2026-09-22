//! App Store Connect authentication.
//!
//! Apple wants an ES256 JWT signed with the `.p8` key from App Store Connect,
//! valid for at most 20 minutes. `ring` signs it; it is already in the tree.

use super::google::{base64url, decode_base64, pem_to_der};
use crate::actions::{ArgSpec, Args};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Apple rejects anything longer than 20 minutes.
const LIFETIME_SECONDS: u64 = 15 * 60;

const AUDIENCE: &str = "appstoreconnect-v1";

pub struct ApiKey {
    pub key_id: String,
    pub issuer_id: String,
    /// The `.p8` contents, in PEM.
    pub private_key: String,
}

/// The arguments that select one App Store Connect authentication shape.
pub fn credential_args() -> Vec<ArgSpec> {
    vec![
        ArgSpec::new("key_id", "App Store Connect key id"),
        ArgSpec::new("issuer_id", "App Store Connect issuer id"),
        ArgSpec::new("key", "The .p8 itself, base64 of it, or a path to it").sensitive(),
        ArgSpec::new(
            "api_key",
            "JSON or base64 JSON with keyId, issuerId and authKey",
        )
        .sensitive(),
    ]
}

/// The one credential placeholder a migration should emit for Apple actions.
pub fn migration_credential_arg() -> ArgSpec {
    ArgSpec::new(
        "api_key",
        "JSON or base64 JSON with keyId, issuerId and authKey",
    )
    .required()
    .sensitive()
}

/// Validate that callers supply either the legacy triplet or one key object.
pub fn credential_problems(provided: &BTreeMap<String, String>) -> Vec<String> {
    let has_object = provided
        .get("api_key")
        .is_some_and(|value| !value.trim().is_empty());
    let legacy = ["key_id", "issuer_id", "key"];
    let supplied_legacy: Vec<&str> = legacy
        .iter()
        .copied()
        .filter(|name| {
            provided
                .get(*name)
                .is_some_and(|value| !value.trim().is_empty())
        })
        .collect();

    if has_object && !supplied_legacy.is_empty() {
        return vec!["needs either api_key or key_id, issuer_id and key, not both".to_string()];
    }
    if has_object || supplied_legacy.len() == legacy.len() {
        return Vec::new();
    }
    if supplied_legacy.is_empty() {
        return vec!["needs api_key or key_id, issuer_id and key".to_string()];
    }

    let missing: Vec<&str> = legacy
        .iter()
        .copied()
        .filter(|name| {
            provided
                .get(*name)
                .is_none_or(|value| value.trim().is_empty())
        })
        .collect();
    vec![format!(
        "needs api_key or the remaining legacy arguments: {}",
        missing.join(", ")
    )]
}

/// Load the credential shape an App Store Connect action received.
pub fn load_credential(args: &Args, root: &Path) -> Result<ApiKey, String> {
    if let Some(value) = args.get("api_key") {
        return load_object(value, root);
    }
    ApiKey::load(
        args.get_or("key_id", ""),
        args.get_or("issuer_id", ""),
        args.get_or("key", ""),
        root,
    )
}

/// The identifiers an uploader needs before it reads the private key.
pub fn credential_identity(args: &Args) -> Result<(String, String), String> {
    if let Some(value) = args.get("api_key") {
        let object = parse_object(value)?;
        return Ok((object.key_id, object.issuer_id));
    }
    Ok((
        args.get_or("key_id", "").to_string(),
        args.get_or("issuer_id", "").to_string(),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialObject {
    #[serde(rename = "keyId")]
    key_id: String,
    #[serde(rename = "issuerId")]
    issuer_id: String,
    #[serde(rename = "authKey")]
    auth_key: String,
}

fn load_object(value: &str, root: &Path) -> Result<ApiKey, String> {
    let object = parse_object(value)?;
    ApiKey::load(&object.key_id, &object.issuer_id, &object.auth_key, root)
}

fn parse_object(value: &str) -> Result<CredentialObject, String> {
    let text = if value.trim_start().starts_with('{') {
        value.to_string()
    } else {
        let bytes =
            decode_base64(value).map_err(|_| "api_key must be JSON or base64 JSON".to_string())?;
        String::from_utf8(bytes)
            .map_err(|_| "api_key base64 must decode to UTF-8 JSON".to_string())?
    };
    let object: CredentialObject = serde_json::from_str(&text).map_err(|_| {
        "api_key must be a JSON object with keyId, issuerId and authKey".to_string()
    })?;

    if object.key_id.trim().is_empty()
        || object.issuer_id.trim().is_empty()
        || object.auth_key.trim().is_empty()
    {
        return Err("api_key fields keyId, issuerId and authKey must not be empty".to_string());
    }

    Ok(object)
}

impl ApiKey {
    /// Accept the key as PEM, as base64 of the PEM, or as a path to the file.
    pub fn load(
        key_id: &str,
        issuer_id: &str,
        key: &str,
        root: &std::path::Path,
    ) -> Result<Self, String> {
        let private_key = if key.contains("BEGIN") {
            key.to_string()
        } else {
            let path = root.join(key.trim());
            if path.is_file() {
                std::fs::read_to_string(&path)
                    .map_err(|err| format!("cannot read {}: {err}", path.display()))?
            } else {
                // CI usually carries the key base64 encoded in a variable.
                let bytes = decode_base64(key).map_err(|message| {
                    format!("the key is neither PEM, a file, nor base64: {message}")
                })?;
                String::from_utf8(bytes).map_err(|_| "the decoded key is not text".to_string())?
            }
        };

        if !private_key.contains("BEGIN") {
            return Err("the key does not look like a .p8 file".to_string());
        }

        Ok(Self {
            key_id: key_id.to_string(),
            issuer_id: issuer_id.to_string(),
            private_key,
        })
    }
}

/// The signed part of the token, separated out so it can be checked in tests.
pub fn token_claims(key: &ApiKey, now: u64) -> String {
    let header = format!(r#"{{"alg":"ES256","kid":"{}","typ":"JWT"}}"#, key.key_id);
    let claims = format!(
        r#"{{"iss":"{}","iat":{now},"exp":{},"aud":"{AUDIENCE}"}}"#,
        key.issuer_id,
        now + LIFETIME_SECONDS
    );
    format!(
        "{}.{}",
        base64url(header.as_bytes()),
        base64url(claims.as_bytes())
    )
}

/// Build a bearer token for the App Store Connect API.
pub fn token(key: &ApiKey) -> Result<String, String> {
    let der = pem_to_der(&key.private_key)?;
    let rng = ring::rand::SystemRandom::new();

    let pair = ring::signature::EcdsaKeyPair::from_pkcs8(
        &ring::signature::ECDSA_P256_SHA256_FIXED_SIGNING,
        &der,
        &rng,
    )
    .map_err(|err| format!("the App Store Connect key could not be read: {err}"))?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or_default();
    let signed = token_claims(key, now);

    let signature = pair
        .sign(&rng, signed.as_bytes())
        .map_err(|err| format!("could not sign the token: {err}"))?;

    Ok(format!("{signed}.{}", base64url(signature.as_ref())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn encode_base64(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let triple = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
            for position in 0..4 {
                if position <= chunk.len() {
                    out.push(ALPHABET[((triple >> (18 - position * 6)) & 0x3F) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    fn key() -> ApiKey {
        ApiKey {
            key_id: "ABC123".to_string(),
            issuer_id: "69a6de7e-0000-0000-0000-000000000000".to_string(),
            private_key: String::new(),
        }
    }

    fn decode(part: &str) -> String {
        let standard = part.replace('-', "+").replace('_', "/");
        String::from_utf8(decode_base64(&standard).expect("valid base64")).expect("utf8")
    }

    #[test]
    fn the_header_names_the_key() {
        let claims = token_claims(&key(), 1_000);
        let header = decode(claims.split('.').next().expect("a header"));
        assert!(header.contains(r#""alg":"ES256""#), "{header}");
        assert!(header.contains(r#""kid":"ABC123""#), "{header}");
    }

    #[test]
    fn the_payload_expires_within_apples_limit() {
        let claims = token_claims(&key(), 1_000);
        let payload = decode(claims.split('.').nth(1).expect("a payload"));
        assert!(
            payload.contains(r#""iss":"69a6de7e-0000-0000-0000-000000000000""#),
            "{payload}"
        );
        assert!(
            payload.contains(r#""aud":"appstoreconnect-v1""#),
            "{payload}"
        );
        assert!(payload.contains(r#""iat":1000"#), "{payload}");
        // 1000 + 900: comfortably inside the 20 minutes Apple allows.
        assert!(payload.contains(r#""exp":1900"#), "{payload}");
    }

    #[test]
    fn reads_a_key_given_as_pem() {
        let pem = "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----";
        let key = ApiKey::load("K", "I", pem, Path::new(".")).expect("valid");
        assert!(key.private_key.contains("BEGIN"));
    }

    #[test]
    fn reads_a_key_given_as_base64() {
        let pem = "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----";
        let encoded = {
            // Standard base64 of the PEM, the way CI usually carries it.
            const ALPHABET: &[u8] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let bytes = pem.as_bytes();
            let mut out = String::new();
            for chunk in bytes.chunks(3) {
                let b = [
                    chunk[0],
                    *chunk.get(1).unwrap_or(&0),
                    *chunk.get(2).unwrap_or(&0),
                ];
                let triple = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
                for position in 0..4 {
                    if position <= chunk.len() {
                        out.push(
                            ALPHABET[((triple >> (18 - position * 6)) & 0x3F) as usize] as char,
                        );
                    } else {
                        out.push('=');
                    }
                }
            }
            out
        };

        let key = ApiKey::load("K", "I", &encoded, Path::new(".")).expect("valid");
        assert!(key.private_key.contains("BEGIN"), "{}", key.private_key);
    }

    #[test]
    fn rejects_something_that_is_not_a_key() {
        assert!(ApiKey::load("K", "I", "not a key at all", Path::new(".")).is_err());
    }

    #[test]
    fn reads_an_apple_key_object_from_json() {
        let key = load_object(
            r#"{"keyId":"K","issuerId":"I","authKey":"-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----"}"#,
            Path::new("."),
        )
        .expect("valid object");
        assert_eq!(key.key_id, "K");
        assert_eq!(key.issuer_id, "I");
        assert!(key.private_key.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn reads_an_apple_key_object_from_base64() {
        let encoded = encode_base64(
            br#"{"keyId":"K","issuerId":"I","authKey":"-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----"}"#,
        );
        let key = load_object(&encoded, Path::new(".")).expect("valid object");
        assert_eq!(key.key_id, "K");
        assert_eq!(key.issuer_id, "I");
    }

    #[test]
    fn rejects_a_base64_encoded_yaml_apple_key_object() {
        let encoded = encode_base64(
            b"keyId: K\nissuerId: I\nauthKey: |\n  -----BEGIN PRIVATE KEY-----\n  abc\n  -----END PRIVATE KEY-----\n",
        );
        assert!(parse_object(&encoded).is_err());
    }

    #[test]
    fn apple_credential_errors_do_not_echo_the_input() {
        let secret = "not-base64-secret";
        let error = match load_object(secret, Path::new(".")) {
            Ok(_) => panic!("invalid input should fail"),
            Err(error) => error,
        };
        assert!(!error.contains(secret), "{error}");
    }
}
