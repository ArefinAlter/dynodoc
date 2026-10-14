//! `dynodoc-key-v1`: a passphrase- or keystore-encrypted Ed25519 signing key file.
//! The wire format and the independent test vector are in docs/KEY-FILE-FORMAT.md.
//!
//! - KDF: PBKDF2-HMAC-SHA256 (`ring::pbkdf2`), at least 600,000 iterations, the
//!   parameters stored in the file and bounded when read.
//! - AEAD: ChaCha20-Poly1305 (`ring::aead`) over the PKCS#8 DER private key.
//! - Associated data binds the format id, the KDF parameters, the key-store
//!   binding and the public key, so none of them can be edited without detection.
//! - A wrong passphrase and a modified file produce the same generic error; the
//!   decryption check is the AEAD tag (constant time inside `ring`).
use crate::{
    keystore,
    passphrase::{wipe_slice, Secret},
};
use anyhow::{anyhow, ensure};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305},
    pbkdf2,
    rand::{SecureRandom, SystemRandom},
    signature::{Ed25519KeyPair, KeyPair},
};
use serde::{Deserialize, Serialize};
use std::num::NonZeroU32;

pub const FORMAT: &str = "dynodoc-key-v1";
pub const KDF_NAME: &str = "pbkdf2-hmac-sha256";
pub const AEAD_NAME: &str = "chacha20-poly1305";
/// Iterations written by this program (OWASP's PBKDF2-HMAC-SHA256 guidance).
pub const ITERATIONS: u32 = 600_000;
/// Bounds accepted when reading: no weaker than the default, and low enough that
/// a hostile file cannot keep the process busy for long.
pub const MIN_ITERATIONS: u32 = 600_000;
pub const MAX_ITERATIONS: u32 = 5_000_000;
pub const SALT_BYTES: usize = 16;
pub const MIN_SALT_BYTES: usize = 16;
pub const MAX_SALT_BYTES: usize = 64;
pub const NONCE_BYTES: usize = 12;
/// A whole container, before any parsing.
pub const MAX_FILE_BYTES: usize = 16 * 1024;
/// The PKCS#8 plaintext bound shared with plain key files.
pub const MAX_PLAINTEXT_BYTES: usize = 4096;
const TAG_BYTES: usize = 16;

/// What every wrong passphrase, every edit and every swapped field says.
const GENERIC: &str = "cannot decrypt the key file: wrong passphrase, or the file was changed";
const MALFORMED: &str = "not a valid dynodoc-key-v1 encrypted key file";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keystore {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Kdf {
    name: String,
    iterations: u32,
    salt: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cipher {
    name: String,
    nonce: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    format: String,
    public_key: String,
    kdf: Kdf,
    aead: Cipher,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    keystore: Option<Keystore>,
    ciphertext: String,
}

/// A parsed and bounds-checked container; nothing here is secret.
pub struct Container {
    public_key: String,
    iterations: u32,
    salt: Vec<u8>,
    salt_text: String,
    nonce: [u8; NONCE_BYTES],
    keystore: Option<Keystore>,
    ciphertext: Vec<u8>,
}

/// Plain DER starts with 0x30; a container is JSON and starts with `{`.
pub fn is_container(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .find(|b| !b.is_ascii_whitespace())
        .is_some_and(|b| *b == b'{')
}

fn decode(text: &str) -> Option<Vec<u8>> {
    let bytes = URL_SAFE_NO_PAD.decode(text).ok()?;
    // Reject non-canonical spellings so one file has one meaning.
    (URL_SAFE_NO_PAD.encode(&bytes) == text).then_some(bytes)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn public_key_hex(pair: &Ed25519KeyPair) -> String {
    hex(pair.public_key().as_ref())
}

impl Container {
    pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        ensure!(bytes.len() <= MAX_FILE_BYTES, MALFORMED);
        let wire: Wire = serde_json::from_slice(bytes).map_err(|_| anyhow!(MALFORMED))?;
        ensure!(wire.format == FORMAT, "{MALFORMED}: unsupported format");
        ensure!(
            wire.public_key.len() == 64
                && wire
                    .public_key
                    .bytes()
                    .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
            MALFORMED
        );
        ensure!(wire.kdf.name == KDF_NAME, "{MALFORMED}: unsupported KDF");
        ensure!(
            (MIN_ITERATIONS..=MAX_ITERATIONS).contains(&wire.kdf.iterations),
            "{MALFORMED}: iteration count outside {MIN_ITERATIONS}..={MAX_ITERATIONS}"
        );
        let salt = decode(&wire.kdf.salt).ok_or_else(|| anyhow!(MALFORMED))?;
        ensure!(
            (MIN_SALT_BYTES..=MAX_SALT_BYTES).contains(&salt.len()),
            MALFORMED
        );
        ensure!(
            wire.aead.name == AEAD_NAME,
            "{MALFORMED}: unsupported cipher"
        );
        let nonce: [u8; NONCE_BYTES] = decode(&wire.aead.nonce)
            .and_then(|n| n.try_into().ok())
            .ok_or_else(|| anyhow!(MALFORMED))?;
        let ciphertext = decode(&wire.ciphertext).ok_or_else(|| anyhow!(MALFORMED))?;
        ensure!(
            (TAG_BYTES + 1..=MAX_PLAINTEXT_BYTES + TAG_BYTES).contains(&ciphertext.len()),
            MALFORMED
        );
        if let Some(store) = &wire.keystore {
            let provider = keystore::Provider::parse(&store.provider)
                .ok_or_else(|| anyhow!("{MALFORMED}: unknown keystore provider"))?;
            match (&store.blob, provider.blob_in_file()) {
                (Some(blob), true) => {
                    let blob = decode(blob).ok_or_else(|| anyhow!(MALFORMED))?;
                    ensure!(
                        !blob.is_empty() && blob.len() <= keystore::MAX_BLOB_BYTES,
                        MALFORMED
                    );
                }
                (None, false) => {}
                _ => return Err(anyhow!(MALFORMED)),
            }
        }
        Ok(Self {
            public_key: wire.public_key,
            iterations: wire.kdf.iterations,
            salt,
            salt_text: wire.kdf.salt,
            nonce,
            keystore: wire.keystore,
            ciphertext,
        })
    }

    /// The Ed25519 public key (lowercase hex). It is unauthenticated until
    /// [`Container::open`] succeeds, which checks it against the decrypted key.
    pub fn public_key(&self) -> &str {
        &self.public_key
    }

    pub fn keystore(&self) -> Option<&Keystore> {
        self.keystore.as_ref()
    }

    /// The decoded DPAPI blob, when the container carries one.
    pub fn keystore_blob(&self) -> Option<Vec<u8>> {
        self.keystore
            .as_ref()
            .and_then(|k| k.blob.as_deref())
            .and_then(decode)
    }

    /// Bytes authenticated alongside the ciphertext. Field order and spelling are
    /// fixed; see docs/KEY-FILE-FORMAT.md.
    fn aad(&self) -> Vec<u8> {
        aad(
            &self.public_key,
            self.iterations,
            &self.salt_text,
            self.keystore.as_ref(),
        )
    }

    /// Decrypt with the wrapping secret. The error is the same generic message for
    /// a wrong secret and for any modification of the file.
    pub fn open(&self, secret: &[u8]) -> anyhow::Result<Secret> {
        let key = derive(secret, &self.salt, self.iterations)?;
        let mut plain = Secret::new(self.ciphertext.clone());
        let opened = key
            .open_in_place(
                Nonce::assume_unique_for_key(self.nonce),
                Aad::from(self.aad()),
                plain.as_mut_vec(),
            )
            .map(|opened| opened.len())
            .map_err(|_| anyhow!(GENERIC))?;
        plain.as_mut_vec().truncate(opened);
        // The tag already proved the content is what was sealed; this also proves
        // it is a usable key whose public half is the one the file declares.
        let pair = Ed25519KeyPair::from_pkcs8_maybe_unchecked(plain.as_bytes())
            .map_err(|_| anyhow!(GENERIC))?;
        ensure!(public_key_hex(&pair) == self.public_key, GENERIC);
        Ok(plain)
    }
}

fn aad(public_key: &str, iterations: u32, salt: &str, keystore: Option<&Keystore>) -> Vec<u8> {
    let binding = match keystore {
        None => "none".to_owned(),
        Some(Keystore {
            provider,
            blob: None,
        }) => provider.clone(),
        Some(Keystore {
            provider,
            blob: Some(blob),
        }) => format!("{provider}:{blob}"),
    };
    format!(
        "{FORMAT}\npublic_key={public_key}\nkdf={KDF_NAME}\niterations={iterations}\nsalt={salt}\naead={AEAD_NAME}\nkeystore={binding}"
    )
    .into_bytes()
}

fn derive(secret: &[u8], salt: &[u8], iterations: u32) -> anyhow::Result<LessSafeKey> {
    let iterations = NonZeroU32::new(iterations).ok_or_else(|| anyhow!(MALFORMED))?;
    let mut raw = [0u8; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        iterations,
        salt,
        secret,
        &mut raw,
    );
    let key = UnboundKey::new(&CHACHA20_POLY1305, &raw);
    wipe_slice(&mut raw);
    Ok(LessSafeKey::new(
        key.map_err(|_| anyhow!("cipher setup failed"))?,
    ))
}

/// Encrypt a PKCS#8 key under `secret` with fresh random salt and nonce.
pub fn seal(pkcs8: &[u8], secret: &[u8], keystore: Option<Keystore>) -> anyhow::Result<Vec<u8>> {
    let rng = SystemRandom::new();
    let mut salt = [0u8; SALT_BYTES];
    let mut nonce = [0u8; NONCE_BYTES];
    rng.fill(&mut salt)
        .and_then(|()| rng.fill(&mut nonce))
        .map_err(|_| anyhow!("system random generator failed"))?;
    seal_with(pkcs8, secret, keystore, ITERATIONS, &salt, nonce)
}

/// Deterministic form for the published test vector and parameter tests.
pub fn seal_with(
    pkcs8: &[u8],
    secret: &[u8],
    keystore: Option<Keystore>,
    iterations: u32,
    salt: &[u8],
    nonce: [u8; NONCE_BYTES],
) -> anyhow::Result<Vec<u8>> {
    ensure!(pkcs8.len() <= MAX_PLAINTEXT_BYTES, "signing key too large");
    ensure!(
        (MIN_SALT_BYTES..=MAX_SALT_BYTES).contains(&salt.len()),
        "salt length"
    );
    let pair = Ed25519KeyPair::from_pkcs8_maybe_unchecked(pkcs8)
        .map_err(|_| anyhow!("invalid Ed25519 PKCS#8 key"))?;
    let public_key = public_key_hex(&pair);
    let salt_text = URL_SAFE_NO_PAD.encode(salt);
    let aad = aad(&public_key, iterations, &salt_text, keystore.as_ref());
    let key = derive(secret, salt, iterations)?;
    let mut buffer = Secret::new(Vec::with_capacity(pkcs8.len() + TAG_BYTES));
    buffer.as_mut_vec().extend_from_slice(pkcs8);
    key.seal_in_place_append_tag(
        Nonce::assume_unique_for_key(nonce),
        Aad::from(aad),
        buffer.as_mut_vec(),
    )
    .map_err(|_| anyhow!("encryption failed"))?;
    let wire = Wire {
        format: FORMAT.into(),
        public_key,
        kdf: Kdf {
            name: KDF_NAME.into(),
            iterations,
            salt: salt_text,
        },
        aead: Cipher {
            name: AEAD_NAME.into(),
            nonce: URL_SAFE_NO_PAD.encode(nonce),
        },
        keystore,
        ciphertext: URL_SAFE_NO_PAD.encode(buffer.as_bytes()),
    };
    let mut out = serde_json::to_vec_pretty(&wire)?;
    out.push(b'\n');
    // What was written must be readable with the same secret before it is used.
    let reread = Container::parse(&out)?.open(secret)?;
    ensure!(
        reread.as_bytes() == pkcs8,
        "encrypted key self-check failed"
    );
    Ok(out)
}

/// The public key of an unencrypted PKCS#8 key, lowercase hex.
pub fn public_key_of(pkcs8: &[u8]) -> anyhow::Result<String> {
    let pair = Ed25519KeyPair::from_pkcs8_maybe_unchecked(pkcs8)
        .map_err(|_| anyhow!("invalid Ed25519 PKCS#8 key"))?;
    Ok(public_key_hex(&pair))
}

#[cfg(test)]
mod tests {
    use super::*;

    const VECTOR: &str = include_str!("../../engine-core/tests/fixtures/dynodoc-key-v1.json");

    fn pkcs8() -> Vec<u8> {
        Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .unwrap()
            .as_ref()
            .to_vec()
    }

    #[test]
    fn independent_vector_decrypts_and_reencrypts_identically() {
        let vector: serde_json::Value = serde_json::from_str(VECTOR).unwrap();
        let text = serde_json::to_vec(&vector["file"]).unwrap();
        let container = Container::parse(&text).unwrap();
        assert_eq!(container.public_key(), vector["public_key"]);
        let passphrase = vector["passphrase"].as_str().unwrap();
        let plain = container.open(passphrase.as_bytes()).unwrap();
        let expected = (0..vector["pkcs8_hex"].as_str().unwrap().len() / 2)
            .map(|i| {
                u8::from_str_radix(&vector["pkcs8_hex"].as_str().unwrap()[2 * i..2 * i + 2], 16)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(plain.as_bytes(), expected);
        // The Rust encryptor, given the vector's salt and nonce, is byte-compatible.
        let file = &vector["file"];
        let salt = URL_SAFE_NO_PAD
            .decode(file["kdf"]["salt"].as_str().unwrap())
            .unwrap();
        let nonce: [u8; 12] = URL_SAFE_NO_PAD
            .decode(file["aead"]["nonce"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let sealed = seal_with(
            &expected,
            passphrase.as_bytes(),
            None,
            file["kdf"]["iterations"].as_u64().unwrap() as u32,
            &salt,
            nonce,
        )
        .unwrap();
        let sealed: serde_json::Value = serde_json::from_slice(&sealed).unwrap();
        assert_eq!(&sealed, file);
        // The documented associated data and key match the vector's debug fields.
        let aad = String::from_utf8(container.aad()).unwrap();
        assert_eq!(aad, vector["aad"].as_str().unwrap());
    }

    #[test]
    fn round_trip_and_fresh_randomness() {
        let key = pkcs8();
        let a = seal(&key, b"a long enough passphrase", None).unwrap();
        let b = seal(&key, b"a long enough passphrase", None).unwrap();
        assert_ne!(a, b, "salt and nonce are fresh for every file");
        let opened = Container::parse(&a)
            .unwrap()
            .open(b"a long enough passphrase")
            .unwrap();
        assert_eq!(opened.as_bytes(), key);
        // Nothing of the private key or the passphrase is visible in the file.
        let text = String::from_utf8(a).unwrap();
        assert!(!text.contains("passphrase"));
        let seed_hex = hex(&key[16..48]);
        assert!(!text.contains(&seed_hex));
        assert!(text.contains(FORMAT));
    }

    #[test]
    fn wrong_passphrase_and_every_edit_give_the_same_generic_error() {
        let key = pkcs8();
        let file = seal(&key, b"right passphrase", None).unwrap();
        let container = Container::parse(&file).unwrap();
        let wrong = container.open(b"wrong passphrase").unwrap_err().to_string();
        assert_eq!(wrong, GENERIC);
        assert!(!wrong.contains("right"));
        let original: serde_json::Value = serde_json::from_slice(&file).unwrap();
        let mut tampered = Vec::new();
        // Ciphertext, public key, salt, iterations and nonce edits.
        for (path, value) in [
            (
                "/ciphertext",
                flip(original["ciphertext"].as_str().unwrap()),
            ),
            ("/kdf/salt", flip(original["kdf"]["salt"].as_str().unwrap())),
            (
                "/aead/nonce",
                flip(original["aead"]["nonce"].as_str().unwrap()),
            ),
        ] {
            let mut edited = original.clone();
            *edited.pointer_mut(path).unwrap() = serde_json::Value::String(value);
            tampered.push(edited);
        }
        let mut edited = original.clone();
        edited["kdf"]["iterations"] = serde_json::json!(MIN_ITERATIONS + 1);
        tampered.push(edited);
        // A different (valid) public key is authenticated data.
        let other = Ed25519KeyPair::from_pkcs8(&pkcs8()).unwrap();
        let mut edited = original.clone();
        edited["public_key"] = serde_json::Value::String(public_key_hex(&other));
        tampered.push(edited);
        // Moving the file under a keystore claim changes the associated data.
        let mut edited = original.clone();
        edited["keystore"] = serde_json::json!({"provider":"macos-keychain"});
        tampered.push(edited);
        for edited in tampered {
            let bytes = serde_json::to_vec(&edited).unwrap();
            match Container::parse(&bytes) {
                Ok(container) => assert_eq!(
                    container.open(b"right passphrase").unwrap_err().to_string(),
                    GENERIC,
                    "{edited}"
                ),
                Err(error) => assert!(error.to_string().contains("dynodoc-key-v1")),
            }
        }
        // The untouched file still opens.
        assert!(container.open(b"right passphrase").is_ok());
    }

    /// Change one character of a base64url string, keeping it canonical.
    fn flip(text: &str) -> String {
        let mut chars: Vec<char> = text.chars().collect();
        chars[2] = if chars[2] == 'A' { 'B' } else { 'A' };
        chars.into_iter().collect()
    }

    #[test]
    fn truncated_and_malformed_files_are_rejected_without_secrets() {
        let key = pkcs8();
        let file = seal(&key, b"right passphrase", None).unwrap();
        for cut in [0, 1, 20, file.len() / 2, file.len() - 2] {
            let error = Container::parse(&file[..cut]).err().unwrap().to_string();
            assert!(error.contains("dynodoc-key-v1"), "{error}");
        }
        assert!(Container::parse(b"[]").is_err());
        assert!(Container::parse(b"{}").is_err());
        let seed_hex = hex(&key[16..48]);
        let error = Container::parse(&file[..file.len() / 2])
            .err()
            .unwrap()
            .to_string();
        assert!(!error.contains(&seed_hex));
        let original: serde_json::Value = serde_json::from_slice(&file).unwrap();
        let mutate = |f: &dyn Fn(&mut serde_json::Value)| {
            let mut v = original.clone();
            f(&mut v);
            Container::parse(&serde_json::to_vec(&v).unwrap()).is_err()
        };
        assert!(mutate(&|v| v["format"] = "dynodoc-key-v2".into()));
        assert!(mutate(&|v| v["kdf"]["name"] = "scrypt".into()));
        assert!(mutate(&|v| v["aead"]["name"] = "aes-256-gcm".into()));
        assert!(mutate(&|v| v["extra"] = 1.into()));
        assert!(mutate(&|v| v["public_key"] = "ABCD".into()));
        assert!(mutate(&|v| v["aead"]["nonce"] = "AAAA".into()));
        assert!(mutate(&|v| v["ciphertext"] = "AAAA".into()));
        assert!(mutate(&|v| v["kdf"]["salt"] = "AA".into()));
        // A non-canonical base64 spelling of the same bytes is not accepted.
        let salt = original["kdf"]["salt"].as_str().unwrap().to_owned();
        assert!(mutate(&|v| v["kdf"]["salt"] = format!("{salt}=").into()));
    }

    #[test]
    fn iteration_salt_and_size_bounds_are_enforced_before_any_derivation() {
        let key = pkcs8();
        let file = seal(&key, b"right passphrase", None).unwrap();
        let original: serde_json::Value = serde_json::from_slice(&file).unwrap();
        let with = |iterations: u64| {
            let mut v = original.clone();
            v["kdf"]["iterations"] = iterations.into();
            Container::parse(&serde_json::to_vec(&v).unwrap())
        };
        assert!(with(u64::from(MIN_ITERATIONS) - 1).is_err());
        assert!(with(u64::from(MIN_ITERATIONS)).is_ok());
        assert!(with(u64::from(MAX_ITERATIONS)).is_ok());
        assert!(with(u64::from(MAX_ITERATIONS) + 1).is_err());
        assert!(with(u64::from(u32::MAX)).is_err());
        assert!(with(0).is_err());
        assert!(with(u64::MAX).is_err());
        // Oversized input is refused before JSON is parsed.
        assert!(Container::parse(&vec![b' '; MAX_FILE_BYTES + 1]).is_err());
        // Salt bounds.
        let mut v = original.clone();
        v["kdf"]["salt"] = URL_SAFE_NO_PAD.encode([1u8; MAX_SALT_BYTES + 1]).into();
        assert!(Container::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        v["kdf"]["salt"] = URL_SAFE_NO_PAD.encode([1u8; MAX_SALT_BYTES]).into();
        assert!(Container::parse(&serde_json::to_vec(&v).unwrap()).is_ok());
        // Ciphertext bound.
        v["ciphertext"] = URL_SAFE_NO_PAD
            .encode(vec![0u8; MAX_PLAINTEXT_BYTES + TAG_BYTES + 1])
            .into();
        assert!(Container::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        // A hostile iteration count is rejected fast, not computed.
        let started = std::time::Instant::now();
        assert!(with(u64::from(u32::MAX)).is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(1));
    }

    #[test]
    fn keystore_binding_is_authenticated_and_provider_rules_hold() {
        let key = pkcs8();
        let secret = keystore::random_secret().unwrap();
        let bound = Keystore {
            provider: "macos-keychain".into(),
            blob: None,
        };
        let file = seal(&key, secret.as_bytes(), Some(bound.clone())).unwrap();
        let container = Container::parse(&file).unwrap();
        assert_eq!(container.keystore(), Some(&bound));
        assert!(container.open(secret.as_bytes()).is_ok());
        // Removing the binding changes the associated data.
        let mut v: serde_json::Value = serde_json::from_slice(&file).unwrap();
        v.as_object_mut().unwrap().remove("keystore");
        let stripped = Container::parse(&serde_json::to_vec(&v).unwrap()).unwrap();
        assert_eq!(
            stripped.open(secret.as_bytes()).unwrap_err().to_string(),
            GENERIC
        );
        // DPAPI needs a blob; the others must not have one; unknown providers fail.
        for (provider, blob, ok) in [
            ("windows-dpapi", None, false),
            ("windows-dpapi", Some("AAAA"), true),
            ("macos-keychain", Some("AAAA"), false),
            ("linux-secret-service", None, true),
            ("something-else", None, false),
        ] {
            let mut v: serde_json::Value = serde_json::from_slice(&file).unwrap();
            v["keystore"] = serde_json::json!({"provider": provider});
            if let Some(blob) = blob {
                v["keystore"]["blob"] = blob.into();
            }
            assert_eq!(
                Container::parse(&serde_json::to_vec(&v).unwrap()).is_ok(),
                ok,
                "{provider} {blob:?}"
            );
        }
    }

    #[test]
    fn detection_separates_json_containers_from_der() {
        assert!(is_container(b"{\"format\":\"dynodoc-key-v1\"}"));
        assert!(is_container(b"  \n{"));
        assert!(!is_container(&pkcs8()));
        assert!(!is_container(b""));
    }
}
