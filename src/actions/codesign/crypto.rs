//! Reading files encrypted by fastlane's `match`.
//!
//! `match` shells out to `openssl aes-256-cbc -k <password> -a -md <digest>`,
//! which produces OpenSSL's "Salted__" container: the literal `Salted__`, an
//! 8-byte salt, then AES-256-CBC ciphertext, all base64 encoded. The key and IV
//! come from `EVP_BytesToKey` with a single iteration.
//!
//! Doing this in Rust rather than shelling out to `openssl` is deliberate.
//! macOS ships LibreSSL under that name, and its differences from OpenSSL here
//! are exactly the kind of thing that has broken `match` for people before.

use crate::actions::google::decode_base64;
use aes::cipher::{block_padding::Pkcs7, BlockModeDecrypt, KeyIvInit};

const MAGIC: &[u8] = b"Salted__";
const SALT_LENGTH: usize = 8;
const KEY_LENGTH: usize = 32;
const IV_LENGTH: usize = 16;

type Decryptor = cbc::Decryptor<aes::Aes256>;

/// Which digest `EVP_BytesToKey` used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Digest {
    Sha256,
    /// What older `match` repositories were encrypted with.
    Md5,
}

impl Digest {
    fn hash(self, data: &[u8]) -> Vec<u8> {
        match self {
            Self::Sha256 => ring::digest::digest(&ring::digest::SHA256, data)
                .as_ref()
                .to_vec(),
            Self::Md5 => {
                use md5::Digest as _;
                let mut hasher = md5::Md5::new();
                hasher.update(data);
                hasher.finalize().to_vec()
            }
        }
    }
}

/// OpenSSL's `EVP_BytesToKey` with one iteration: the key schedule `match`
/// relies on.
fn derive(password: &[u8], salt: &[u8], digest: Digest) -> (Vec<u8>, Vec<u8>) {
    let mut material: Vec<u8> = Vec::new();
    let mut previous: Vec<u8> = Vec::new();

    while material.len() < KEY_LENGTH + IV_LENGTH {
        let mut input = previous.clone();
        input.extend_from_slice(password);
        input.extend_from_slice(salt);
        previous = digest.hash(&input);
        material.extend_from_slice(&previous);
    }

    (
        material[..KEY_LENGTH].to_vec(),
        material[KEY_LENGTH..KEY_LENGTH + IV_LENGTH].to_vec(),
    )
}

/// Decrypt one file, trying the current digest and then the older one.
///
/// A repository encrypted years ago still has to open, and nothing in the file
/// says which digest made it.
pub fn decrypt(contents: &[u8], password: &str) -> Result<Vec<u8>, String> {
    let raw = if contents.starts_with(MAGIC) {
        contents.to_vec()
    } else {
        // `-a`: base64 armoured, which is what match writes.
        let text = std::str::from_utf8(contents)
            .map_err(|_| "this is neither a salted OpenSSL file nor base64".to_string())?;
        decode_base64(text)?
    };

    if raw.len() < MAGIC.len() + SALT_LENGTH || !raw.starts_with(MAGIC) {
        return Err("this file was not encrypted by openssl with a salt".to_string());
    }

    let salt = &raw[MAGIC.len()..MAGIC.len() + SALT_LENGTH];
    let ciphertext = &raw[MAGIC.len() + SALT_LENGTH..];

    for digest in [Digest::Sha256, Digest::Md5] {
        if let Ok(plain) = attempt(ciphertext, salt, password, digest) {
            return Ok(plain);
        }
    }

    Err(
        "could not decrypt: the passphrase is wrong, or the file was encrypted some other way"
            .to_string(),
    )
}

fn attempt(
    ciphertext: &[u8],
    salt: &[u8],
    password: &str,
    digest: Digest,
) -> Result<Vec<u8>, String> {
    let (key, iv) = derive(password.as_bytes(), salt, digest);
    let cipher = Decryptor::new_from_slices(&key, &iv).map_err(|err| err.to_string())?;

    let mut buffer = ciphertext.to_vec();
    let plain = cipher
        .decrypt_padded::<Pkcs7>(&mut buffer)
        .map_err(|err| err.to_string())?;

    Ok(plain.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Produced by real OpenSSL 3:
    ///   openssl aes-256-cbc -k "the-passphrase" -in plain.txt -a -md sha256
    const SHA256_FIXTURE: &str = "U2FsdGVkX1/gQbLU4tcJi2OIraErSfzRQfAxuPBEdBeQiXRq/RkCKQFShnReJWdUXLA8F2mS8GuXx0ejqNRw/561PBqeMw6xAlogdIUsTi/WsGhwUgtvgxfvf+lYN+p3";

    /// The same, with `-md md5`: what older match repositories contain.
    const MD5_FIXTURE: &str = "U2FsdGVkX19Gy1GLVQohnu66MCFg545ulv5kb/FnkpZPnUDwc6cHNPLNR4utTMmxDIGo9VmYpEU8YfB0pcMJUts8Euj4GiRPuY6y+Hr0W6UJ+90lOtTgxkIxFfGZBpYL";

    const PASSWORD: &str = "the-passphrase";
    const PLAIN: &str = "shlane match fixture: the quick brown fox jumps over the lazy dog\n";

    #[test]
    fn reads_what_openssl_wrote_with_sha256() {
        let plain = decrypt(SHA256_FIXTURE.as_bytes(), PASSWORD).expect("should decrypt");
        assert_eq!(String::from_utf8(plain).expect("utf8"), PLAIN);
    }

    #[test]
    fn reads_an_older_repository_encrypted_with_md5() {
        let plain = decrypt(MD5_FIXTURE.as_bytes(), PASSWORD).expect("should decrypt");
        assert_eq!(String::from_utf8(plain).expect("utf8"), PLAIN);
    }

    #[test]
    fn accepts_the_armour_with_line_breaks() {
        let wrapped = format!("{}\n{}\n", &SHA256_FIXTURE[..40], &SHA256_FIXTURE[40..]);
        let plain = decrypt(wrapped.as_bytes(), PASSWORD).expect("should decrypt");
        assert_eq!(String::from_utf8(plain).expect("utf8"), PLAIN);
    }

    #[test]
    fn accepts_raw_binary_without_the_armour() {
        let raw = decode_base64(SHA256_FIXTURE).expect("valid base64");
        let plain = decrypt(&raw, PASSWORD).expect("should decrypt");
        assert_eq!(String::from_utf8(plain).expect("utf8"), PLAIN);
    }

    #[test]
    fn the_wrong_passphrase_is_reported_rather_than_returning_rubbish() {
        let error = decrypt(SHA256_FIXTURE.as_bytes(), "wrong").expect_err("should fail");
        assert!(error.contains("passphrase"), "{error}");
    }

    #[test]
    fn something_that_is_not_an_encrypted_file_is_reported() {
        assert!(decrypt(b"hello there", PASSWORD).is_err());
        assert!(decrypt(&[0xff, 0xfe, 0x00], PASSWORD).is_err());
    }

    #[test]
    fn derives_the_key_lengths_openssl_uses() {
        let (key, iv) = derive(b"pw", b"12345678", Digest::Sha256);
        assert_eq!(key.len(), KEY_LENGTH);
        assert_eq!(iv.len(), IV_LENGTH);

        let (key, iv) = derive(b"pw", b"12345678", Digest::Md5);
        assert_eq!(key.len(), KEY_LENGTH);
        assert_eq!(iv.len(), IV_LENGTH);
    }
}
