//! Reproduces `fixtures/at-rest-v1.json` byte for byte. The fixture is written by
//! `fixtures/gen_at_rest_vectors.py`, which uses the PyPI `cryptography` package (OpenSSL
//! AES-GCM and HKDF) plus `hmac`/`hashlib` and shares no code with the Rust module under
//! test; never regenerate it to make one of these checks pass.
//!
//! The derived keys, nonces, associated data and HKDF info are private, so they are
//! checked by a unit test in `src/at_rest/envelope.rs` against the same file. Here:
//! every ciphertext, tag, envelope, deterministic salt and wrapped data key.
use engine_core::at_rest::{
    has_magic, open, seal_content_addressed, seal_with_salt, Context, DataKey, Dek, Error,
    FileKeyProvider, Header, KeyProvider, Purpose, Salt, TestKeyProvider, WrapContext, Wrapped,
    HEADER_LEN, MAGIC, TAG_LEN, VERSION,
};
use ring::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM},
    digest::{digest, SHA256},
    hkdf,
};
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/at-rest-v1.json")).unwrap()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// The application builds with an older minimum Rust version than the standalone engine, whose
// Clippy asks for `is_multiple_of`; one source serves both, so the lint is allowed here.
#[allow(clippy::manual_is_multiple_of)]
fn unhex(text: &str) -> Vec<u8> {
    assert!(text.len() % 2 == 0, "odd hex {text}");
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    hex(digest(&SHA256, bytes).as_ref())
}

fn s<'a>(v: &'a Value, name: &str) -> &'a str {
    v[name].as_str().unwrap_or_else(|| panic!("{name} missing"))
}

fn n(v: &Value, name: &str) -> u64 {
    v[name].as_u64().unwrap_or_else(|| panic!("{name} missing"))
}

/// `byte i = (i * 7 + 3) mod 256`, the generator's pattern for long plaintexts.
fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| ((i * 7 + 3) & 0xFF) as u8).collect()
}

fn plaintext_of(v: &Value) -> Vec<u8> {
    match v.get("plaintext_hex") {
        Some(text) => unhex(text.as_str().unwrap()),
        None => {
            let p = &v["plaintext_pattern"];
            assert_eq!((n(p, "mul"), n(p, "add")), (7, 3));
            pattern(n(p, "len") as usize)
        }
    }
}

fn installation_of(v: &Value) -> Uuid {
    Uuid::from_slice(&unhex(s(v, "installation_hex"))).unwrap()
}

fn data_key_of(v: &Value) -> DataKey {
    DataKey::new(
        n(v, "dek_id") as u32,
        n(v, "kek_version") as u32,
        Dek::from_slice(&unhex(s(v, "dek_hex"))).unwrap(),
    )
}

/// Check one envelope vector against the Rust output for `envelope`.
fn assert_envelope_matches(v: &Value, envelope: &[u8]) {
    let name = s(v, "name");
    assert_eq!(envelope.len() as u64, n(v, "envelope_len"), "{name} length");
    assert_eq!(
        sha256_hex(envelope),
        s(v, "envelope_sha256"),
        "{name} envelope"
    );
    if let Some(full) = v.get("envelope_hex") {
        assert_eq!(
            hex(envelope),
            full.as_str().unwrap(),
            "{name} envelope bytes"
        );
    }
    let (ciphertext, tag) = envelope[HEADER_LEN..].split_at(envelope.len() - HEADER_LEN - TAG_LEN);
    assert_eq!(hex(tag), s(v, "tag_hex"), "{name} tag");
    assert_eq!(
        sha256_hex(ciphertext),
        s(v, "ciphertext_sha256"),
        "{name} ciphertext"
    );
    if let Some(full) = v.get("ciphertext_hex") {
        assert_eq!(
            hex(ciphertext),
            full.as_str().unwrap(),
            "{name} ciphertext bytes"
        );
    }
    assert_eq!(&envelope[..8], &MAGIC);
    assert_eq!(envelope[8], VERSION);
    assert_eq!(&envelope[9..13], &(n(v, "dek_id") as u32).to_be_bytes());
    assert_eq!(
        &envelope[13..17],
        &(n(v, "kek_version") as u32).to_be_bytes()
    );
    assert_eq!(
        hex(&envelope[17..49]),
        s(v, "salt_hex"),
        "{name} salt in header"
    );
}

#[test]
fn constants_match_the_fixture() {
    let c = &fixture()["constants"];
    assert_eq!(hex(&MAGIC), s(c, "magic_hex"));
    assert_eq!(u64::from(VERSION), n(c, "version"));
    assert_eq!(HEADER_LEN as u64, n(c, "header_len"));
    assert_eq!(TAG_LEN as u64, n(c, "tag_len"));
    assert_ne!(&MAGIC[..], b"DYNOSEAL");
}

/// The two primitives behind the construction, against published known answers that the
/// generator also asserts, so a platform `ring` that disagrees is caught here.
#[test]
fn primitives_reproduce_published_known_answers() {
    struct Len(usize);
    impl hkdf::KeyType for Len {
        fn len(&self) -> usize {
            self.0
        }
    }
    let p = &fixture()["primitives"];
    let ikm = [0x0Bu8; 22];
    let salt: Vec<u8> = (0x00..0x0D).collect();
    let info: Vec<u8> = (0xF0..0xFA).collect();
    let mut okm = [0u8; 42];
    hkdf::Salt::new(hkdf::HKDF_SHA256, &salt)
        .extract(&ikm)
        .expand(&[&info], Len(42))
        .unwrap()
        .fill(&mut okm)
        .unwrap();
    assert_eq!(hex(&okm), s(p, "hkdf_rfc5869_a1_okm_hex"));

    let key = LessSafeKey::new(UnboundKey::new(&AES_256_GCM, &[0u8; 32]).unwrap());
    let mut block = [0u8; 16];
    let tag = key
        .seal_in_place_separate_tag(
            Nonce::assume_unique_for_key([0u8; 12]),
            Aad::empty(),
            &mut block,
        )
        .unwrap();
    assert_eq!(hex(&block), s(p, "gcm_case14_ciphertext_hex"));
    assert_eq!(hex(tag.as_ref()), s(p, "gcm_case14_tag_hex"));
}

#[test]
fn every_envelope_vector_is_reproduced_and_decrypts() {
    let fixture = fixture();
    let vectors = fixture["envelopes"].as_array().unwrap();
    assert_eq!(vectors.len(), 13);
    for v in vectors {
        let name = s(v, "name");
        let key = data_key_of(v);
        let row_key = unhex(s(v, "row_key_hex"));
        let ctx =
            Context::new(installation_of(v), s(v, "table"), s(v, "column"), &row_key).unwrap();
        let salt = Salt::from_slice(&unhex(s(v, "salt_hex"))).unwrap();
        let plaintext = plaintext_of(v);

        let envelope = seal_with_salt(&key, &ctx, &salt, &plaintext).unwrap();
        assert_envelope_matches(v, &envelope);
        assert_eq!(
            open(&key, &ctx, &envelope).unwrap(),
            plaintext,
            "{name} decrypt"
        );

        // Also decrypt the generator's own bytes when it wrote them out.
        if let Some(full) = v.get("envelope_hex") {
            let theirs = unhex(full.as_str().unwrap());
            assert_eq!(
                open(&key, &ctx, &theirs).unwrap(),
                plaintext,
                "{name} decrypt (python)"
            );
            let header = Header::parse(&theirs).unwrap();
            assert_eq!(u64::from(header.dek_id), n(v, "dek_id"));
            assert_eq!(u64::from(header.kek_version), n(v, "kek_version"));
            assert_eq!(hex(header.salt.as_bytes()), s(v, "salt_hex"));
        }
    }
}

#[test]
fn the_installation_alone_changes_the_envelope() {
    let fixture = fixture();
    let by_name = |name: &str| {
        fixture["envelopes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| s(v, "name") == name)
            .unwrap()
            .clone()
    };
    let (a, b) = (by_name("empty"), by_name("other_installation"));
    assert_eq!(s(&a, "salt_hex"), s(&b, "salt_hex"));
    assert_eq!(s(&a, "row_key_hex"), s(&b, "row_key_hex"));
    assert_ne!(s(&a, "envelope_hex"), s(&b, "envelope_hex"));
    assert_ne!(s(&a, "tag_hex"), s(&b, "tag_hex"));
}

#[test]
fn every_content_addressed_vector_is_reproduced() {
    let fixture = fixture();
    let vectors = fixture["content_addressed"].as_array().unwrap();
    assert_eq!(vectors.len(), 4);
    for v in vectors {
        let name = s(v, "name");
        let key = data_key_of(v);
        let document = unhex(s(v, "document_hex"));
        let address = unhex(s(v, "address_hex"));
        let row_key = unhex(s(v, "row_key_hex"));
        assert_eq!([document.clone(), address.clone()].concat(), row_key);
        let ctx =
            Context::new(installation_of(v), s(v, "table"), s(v, "column"), &row_key).unwrap();
        let plaintext = plaintext_of(v);

        let salt = Salt::content_addressed(key.dek(), &document, &address);
        assert_eq!(hex(salt.as_bytes()), s(v, "salt_hex"), "{name} salt");
        let envelope = seal_content_addressed(&key, &ctx, &document, &address, &plaintext).unwrap();
        assert_envelope_matches(v, &envelope);
        // Storing the same object again gives the same bytes.
        assert_eq!(
            envelope,
            seal_content_addressed(&key, &ctx, &document, &address, &plaintext).unwrap()
        );
        assert_eq!(open(&key, &ctx, &envelope).unwrap(), plaintext, "{name}");
    }
    // Same address in two documents: different salt, different bytes.
    assert_ne!(s(&vectors[0], "salt_hex"), s(&vectors[1], "salt_hex"));
    assert_eq!(s(&vectors[0], "address_hex"), s(&vectors[1], "address_hex"));
    assert_ne!(
        s(&vectors[0], "envelope_sha256"),
        s(&vectors[1], "envelope_sha256")
    );
}

fn wrap_context_of(v: &Value) -> WrapContext {
    let project = v["project_hex"]
        .as_str()
        .map(|text| Uuid::from_slice(&unhex(text)).unwrap());
    let purpose = match s(v, "purpose") {
        "data" => Purpose::Data,
        "backup" => Purpose::Backup,
        other => panic!("purpose {other}"),
    };
    WrapContext::new(installation_of(v), project, purpose)
}

fn test_provider(fixture: &Value) -> TestKeyProvider {
    let keks = fixture["wrapped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| {
            let kek: [u8; 32] = unhex(s(v, "kek_hex")).try_into().unwrap();
            (n(v, "kek_version") as u32, kek)
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    TestKeyProvider::new(keks)
}

/// A scratch directory under the system temp directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("dynodoc-at-rest-vec-{}-{tag}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[cfg(unix)]
fn owner_only(path: &std::path::Path) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

#[cfg(not(unix))]
fn owner_only(_path: &std::path::Path) {}

#[test]
fn every_wrapped_dek_vector_is_reproduced_and_unwraps() {
    let fixture = fixture();
    let provider = test_provider(&fixture);

    // The KEK files of the fixture, as the file provider reads them.
    let dir = TempDir::new("wrapped");
    for f in fixture["kek_files"].as_array().unwrap() {
        let path = dir.0.join(format!("{}.txt", s(f, "name")));
        std::fs::write(&path, s(f, "text")).unwrap();
        owner_only(&path);
        let file = FileKeyProvider::from_files([(n(f, "version") as u32, path)]).unwrap();
        // One version loaded: a wrap under it unwraps with the in-memory provider's key.
        let ctx = WrapContext::new(Uuid::from_u128(5), None, Purpose::Data);
        let dek = Dek::from_bytes([9; 32]);
        let wrapped = file.wrap(&dek, &ctx).unwrap();
        assert_eq!(wrapped.kek_version(), n(f, "version") as u32);
        assert_eq!(
            file.unwrap(&wrapped, &ctx).unwrap().as_bytes(),
            dek.as_bytes()
        );
        assert_eq!(wrapped.as_bytes().len() as u64, 61);
    }
    let both = FileKeyProvider::from_files(
        ["lf", "crlf"]
            .iter()
            .zip([1u32, 2])
            .map(|(name, version)| (version, dir.0.join(format!("{name}.txt")))),
    )
    .unwrap();
    assert_eq!(both.kek_versions(), vec![1, 2]);

    let wrapped_vectors = fixture["wrapped"].as_array().unwrap();
    assert_eq!(wrapped_vectors.len(), 4);
    for v in wrapped_vectors {
        let name = s(v, "name");
        let ctx = wrap_context_of(v);
        let dek = Dek::from_slice(&unhex(s(v, "dek_hex"))).unwrap();
        let nonce: [u8; 12] = unhex(s(v, "nonce_hex")).try_into().unwrap();
        let version = n(v, "kek_version") as u32;

        assert_eq!(hex(&ctx.to_bytes()), s(v, "context_hex"), "{name} context");
        let wrapped = provider.wrap_at(version, &dek, &ctx, nonce).unwrap();
        assert_eq!(wrapped.kek_version(), version);
        assert_eq!(
            hex(wrapped.as_bytes()),
            s(v, "wrapped_hex"),
            "{name} wrapped"
        );

        // The generator's bytes unwrap under both providers.
        let theirs = Wrapped::new(version, unhex(s(v, "wrapped_hex"))).unwrap();
        for unwrapped in [provider.unwrap(&theirs, &ctx), both.unwrap(&theirs, &ctx)] {
            assert_eq!(
                unwrapped.unwrap().as_bytes(),
                dek.as_bytes(),
                "{name} unwrap"
            );
        }
    }
}

#[test]
fn every_reject_vector_gets_its_own_error() {
    let fixture = fixture();
    let rejects = fixture["rejects"].as_array().unwrap();
    assert_eq!(rejects.len(), 11);
    for v in rejects {
        let name = s(v, "name");
        let input = unhex(s(v, "input_hex"));
        let error = Header::parse(&input).unwrap_err();
        match s(v, "error") {
            "truncated" => assert!(matches!(error, Error::Truncated { .. }), "{name}: {error}"),
            "bad_magic" => assert_eq!(error, Error::BadMagic, "{name}"),
            "unsupported_version" => {
                assert_eq!(
                    error,
                    Error::UnsupportedVersion(n(v, "version") as u8),
                    "{name}"
                )
            }
            other => panic!("unknown expectation {other}"),
        }
        assert_eq!(
            has_magic(&input),
            !matches!(error, Error::BadMagic | Error::Truncated { len: 0..=7, .. }),
            "{name}"
        );
        // Opening never gets past the parser for these.
        let key = DataKey::new(1, 1, Dek::from_bytes([0x40; 32]));
        let ctx = Context::new(Uuid::from_u128(1), "t", "c", b"r").unwrap();
        assert_eq!(open(&key, &ctx, &input).unwrap_err(), error, "{name}");
    }
}
