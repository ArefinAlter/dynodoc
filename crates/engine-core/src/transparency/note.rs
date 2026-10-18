//! Signed notes, checkpoints and cosignatures (C2SP `signed-note`,
//! `tlog-checkpoint`, `tlog-cosignature`).
//!
//! **Revisions.** Implemented from the field-level description in the design
//! (section 4.2) and from memory of the three C2SP specifications as published in
//! 2025-2026; no published text was available while writing (unverified: check at
//! build time). Specifically unchecked: the signature-type byte `0x04` and the
//! `cosignature/v1` message layout (`cosignature/v1\ntime <decimal>\n` followed by
//! the checkpoint text) for witness keys; the exact set of forbidden control
//! characters (this module rejects U+0000-U+001F other than newline, and
//! U+007F-U+009F); and the signature-line body for cosignatures (8-byte big-endian
//! time, then the 64-byte signature). The log's own signature (type `0x01`, signed
//! over the note text, key id = first four bytes of
//! `SHA-256(name || 0x0A || 0x01 || public key)`) is the format of the Go
//! `golang.org/x/mod/sumdb/note` package.
//!
//! **Strictness.** Unlike a lenient note reader this parser rejects: a second
//! signature line with the same name and key id (no "drop repeated" leniency),
//! non-canonical base64, control characters anywhere, and checkpoint extension
//! lines (the design says they are not used). Signature lines of unknown keys are
//! kept and ignored, as the signed-note format requires.
use std::collections::HashSet;

use ring::digest::{digest, SHA256};
use ring::signature::{self, Ed25519KeyPair, KeyPair};

use super::encoding::{b64, hex, unb64, unhex_array};
use super::merkle::Hash;
use super::Error;

const DASH: &str = "\u{2014} ";
const LOG_TYPE: u8 = 0x01;
const COSIGNATURE_TYPE: u8 = 0x04;

/// What a key signs, which fixes its signature type byte and message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyKind {
    /// Ed25519 over the note text (type byte `0x01`): the log's checkpoint signature.
    Log,
    /// Timestamped Ed25519 `cosignature/v1` (type byte `0x04`): a witness.
    Cosigner,
}

impl KeyKind {
    fn type_byte(self) -> u8 {
        match self {
            Self::Log => LOG_TYPE,
            Self::Cosigner => COSIGNATURE_TYPE,
        }
    }
}

/// A key name: non-empty, no whitespace, no control characters, no `+`.
pub fn valid_key_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('+')
        && !name.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn key_id(name: &str, kind: KeyKind, public: &[u8; 32]) -> [u8; 4] {
    let mut input = Vec::with_capacity(name.len() + 34);
    input.extend_from_slice(name.as_bytes());
    input.push(b'\n');
    input.push(kind.type_byte());
    input.extend_from_slice(public);
    let sum = digest(&SHA256, &input);
    sum.as_ref()[..4].try_into().expect("four bytes")
}

/// A log identity: `<host>/dynodoc/v1/transparency`, a schema-less URL (design 4.2).
/// Printable ASCII only, without `+`, quotes or backslash, so it is a legal key
/// name and needs no escaping inside canonical JSON.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Origin(String);

const ORIGIN_SUFFIX: &str = "/dynodoc/v1/transparency";

impl Origin {
    pub fn new(origin: &str) -> Result<Self, Error> {
        let invalid = |why| Err(Error::InvalidOrigin(why));
        if !origin.bytes().all(|b| (0x21..=0x7e).contains(&b)) || origin.contains(['+', '"', '\\'])
        {
            return invalid("only printable ASCII without '+', quote or backslash");
        }
        if origin.contains("://") {
            return invalid("must be a schema-less URL");
        }
        match origin.strip_suffix(ORIGIN_SUFFIX) {
            Some(host) if !host.is_empty() && !host.contains('/') => Ok(Self(origin.to_string())),
            _ => invalid("must be <host>/dynodoc/v1/transparency"),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A public verification key in the signed-note `vkey` text form
/// `<name>+<8 hex key id>+<base64(type byte || 32-byte key)>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifierKey {
    name: String,
    kind: KeyKind,
    public: [u8; 32],
}

impl VerifierKey {
    pub fn new(name: &str, kind: KeyKind, public: [u8; 32]) -> Result<Self, Error> {
        if !valid_key_name(name) {
            return Err(Error::InvalidKeyName);
        }
        Ok(Self {
            name: name.to_string(),
            kind,
            public,
        })
    }

    /// Parse a `vkey`; the stated key id must be the one derived from name and key.
    pub fn parse(vkey: &str) -> Result<Self, Error> {
        // Base64 may contain '+', names may not: split into three at the first two.
        let mut parts = vkey.splitn(3, '+');
        let (Some(name), Some(id), Some(key)) = (parts.next(), parts.next(), parts.next()) else {
            return Err(Error::MalformedKey("expected name+id+key"));
        };
        let raw = unb64(key)?;
        let (kind, public) = match raw.split_first() {
            Some((&LOG_TYPE, public)) => (KeyKind::Log, public),
            Some((&COSIGNATURE_TYPE, public)) => (KeyKind::Cosigner, public),
            Some((other, _)) => return Err(Error::UnsupportedKeyType(*other)),
            None => return Err(Error::MalformedKey("empty key")),
        };
        let public: [u8; 32] = public
            .try_into()
            .map_err(|_| Error::MalformedKey("an Ed25519 public key is 32 bytes"))?;
        let parsed = Self::new(name, kind, public)?;
        if unhex_array::<4>(id).map_err(|_| Error::MalformedKey("key id is 8 hex digits"))?
            != parsed.key_id()
        {
            return Err(Error::KeyIdMismatch);
        }
        Ok(parsed)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> KeyKind {
        self.kind
    }

    pub fn public(&self) -> &[u8; 32] {
        &self.public
    }

    pub fn key_id(&self) -> [u8; 4] {
        key_id(&self.name, self.kind, &self.public)
    }
}

impl std::fmt::Display for VerifierKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut raw = vec![self.kind.type_byte()];
        raw.extend_from_slice(&self.public);
        write!(f, "{}+{}+{}", self.name, hex(&self.key_id()), b64(&raw))
    }
}

/// An Ed25519 signing key bound to a name and a role. The caller owns custody; the
/// log key and every witness key must be different keys (design 3.3).
pub struct Signer {
    name: String,
    kind: KeyKind,
    pair: Ed25519KeyPair,
}

impl Signer {
    pub fn from_seed(name: &str, kind: KeyKind, seed: &[u8; 32]) -> Result<Self, Error> {
        let pair = Ed25519KeyPair::from_seed_unchecked(seed).map_err(|_| Error::SigningKey)?;
        Self::with_pair(name, kind, pair)
    }

    /// From an unencrypted PKCS#8 v1/v2 Ed25519 document, as ring generates and the
    /// operator key files hold.
    pub fn from_pkcs8(name: &str, kind: KeyKind, pkcs8: &[u8]) -> Result<Self, Error> {
        let pair =
            Ed25519KeyPair::from_pkcs8_maybe_unchecked(pkcs8).map_err(|_| Error::SigningKey)?;
        Self::with_pair(name, kind, pair)
    }

    fn with_pair(name: &str, kind: KeyKind, pair: Ed25519KeyPair) -> Result<Self, Error> {
        if !valid_key_name(name) {
            return Err(Error::InvalidKeyName);
        }
        Ok(Self {
            name: name.to_string(),
            kind,
            pair,
        })
    }

    pub fn verifier_key(&self) -> VerifierKey {
        let public = self
            .pair
            .public_key()
            .as_ref()
            .try_into()
            .expect("32 bytes");
        VerifierKey {
            name: self.name.clone(),
            kind: self.kind,
            public,
        }
    }

    /// The log's signature over a checkpoint. The checkpoint's origin must be this
    /// key's name, so a log key cannot be used to sign for another log.
    pub fn sign_checkpoint(&self, text: &str) -> Result<NoteSignature, Error> {
        if self.kind != KeyKind::Log {
            return Err(Error::WrongSignerKind);
        }
        let checkpoint = Checkpoint::parse(text)?;
        if checkpoint.origin != self.name {
            return Err(Error::WrongOrigin {
                expected: self.name.clone(),
                found: checkpoint.origin,
            });
        }
        Ok(self.line(self.pair.sign(text.as_bytes()).as_ref().to_vec()))
    }

    /// A witness cosignature at `time` (Unix seconds). The caller must already have
    /// verified the checkpoint's consistency with what the witness last signed; this
    /// only checks that the text is a well-formed checkpoint.
    pub fn cosign_checkpoint(&self, text: &str, time: u64) -> Result<NoteSignature, Error> {
        if self.kind != KeyKind::Cosigner {
            return Err(Error::WrongSignerKind);
        }
        Checkpoint::parse(text)?;
        let mut body = time.to_be_bytes().to_vec();
        body.extend_from_slice(self.pair.sign(&cosignature_message(text, time)).as_ref());
        Ok(self.line(body))
    }

    fn line(&self, body: Vec<u8>) -> NoteSignature {
        NoteSignature {
            name: self.name.clone(),
            key_id: self.verifier_key().key_id(),
            body,
        }
    }
}

fn cosignature_message(text: &str, time: u64) -> Vec<u8> {
    format!("cosignature/v1\ntime {time}\n{text}").into_bytes()
}

/// One signature line: the key name, the four-byte key id and the signature bytes
/// (64 for a log signature; time then signature, 72, for a cosignature).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NoteSignature {
    name: String,
    key_id: [u8; 4],
    body: Vec<u8>,
}

impl NoteSignature {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn key_id(&self) -> [u8; 4] {
        self.key_id
    }

    fn encode(&self) -> String {
        let mut raw = self.key_id.to_vec();
        raw.extend_from_slice(&self.body);
        format!("{DASH}{} {}\n", self.name, b64(&raw))
    }

    fn parse(line: &str) -> Result<Self, Error> {
        let rest = line.strip_prefix(DASH).ok_or(Error::MalformedNote(
            "signature line must start with an em dash and a space",
        ))?;
        let (name, encoded) = rest.split_once(' ').ok_or(Error::MalformedNote(
            "signature line has no signature field",
        ))?;
        if !valid_key_name(name) {
            return Err(Error::MalformedNote("invalid key name in a signature line"));
        }
        let raw = unb64(encoded)?;
        if raw.len() < 5 {
            return Err(Error::MalformedNote(
                "signature field shorter than a key id and a signature",
            ));
        }
        Ok(Self {
            name: name.to_string(),
            key_id: raw[..4].try_into().expect("four bytes"),
            body: raw[4..].to_vec(),
        })
    }
}

/// What a verified signature line carried besides validity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verified {
    /// The witness's clock for a cosignature; `None` for a log signature.
    pub time: Option<u64>,
}

/// A text with signatures: `text`, a blank line, then one line per signature.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    text: String,
    signatures: Vec<NoteSignature>,
}

fn check_characters(text: &str) -> Result<(), Error> {
    if text.chars().any(|c| c != '\n' && c.is_control()) {
        return Err(Error::MalformedNote("control character"));
    }
    Ok(())
}

impl Note {
    /// A note with no signatures yet; `text` must end with a newline.
    pub fn new(text: &str) -> Result<Self, Error> {
        check_characters(text)?;
        if !text.ends_with('\n') {
            return Err(Error::MalformedNote("text must end with a newline"));
        }
        Ok(Self {
            text: text.to_string(),
            signatures: Vec::new(),
        })
    }

    /// Add a signature line; a second line for the same name and key id is refused.
    pub fn with_signature(mut self, signature: NoteSignature) -> Result<Self, Error> {
        if self
            .signatures
            .iter()
            .any(|s| s.name == signature.name && s.key_id == signature.key_id)
        {
            return Err(Error::DuplicateSignature(signature.name));
        }
        self.signatures.push(signature);
        Ok(self)
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn signatures(&self) -> &[NoteSignature] {
        &self.signatures
    }

    /// The wire form: text, blank line, signature lines. Parsing this returns an
    /// equal note.
    pub fn encode(&self) -> String {
        let mut out = format!("{}\n", self.text);
        for signature in &self.signatures {
            out.push_str(&signature.encode());
        }
        out
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        let message =
            std::str::from_utf8(bytes).map_err(|_| Error::MalformedNote("not valid UTF-8"))?;
        check_characters(message)?;
        let split = message
            .rfind("\n\n")
            .ok_or(Error::MalformedNote("no blank line before the signatures"))?;
        let (text, block) = (&message[..=split], &message[split + 2..]);
        let lines = block.strip_suffix('\n').ok_or(Error::MalformedNote(
            "the signature block must end with a newline",
        ))?;
        if lines.is_empty() {
            return Err(Error::MalformedNote("no signature lines"));
        }
        let mut note = Self::new(text)?;
        let mut seen = HashSet::new();
        for line in lines.split('\n') {
            let signature = NoteSignature::parse(line)?;
            if !seen.insert((signature.name.clone(), signature.key_id)) {
                return Err(Error::DuplicateSignature(signature.name));
            }
            note.signatures.push(signature);
        }
        Ok(note)
    }

    /// Check the signature line of `key`, if the note has one. `Ok(None)` means no
    /// line names this key (name and key id); a line that does but fails is an error.
    pub fn verify(&self, key: &VerifierKey) -> Result<Option<Verified>, Error> {
        let id = key.key_id();
        let Some(line) = self
            .signatures
            .iter()
            .find(|s| s.name == key.name && s.key_id == id)
        else {
            return Ok(None);
        };
        let bad = || Error::BadSignature(key.name.clone());
        let (time, signature, message) = match key.kind {
            KeyKind::Log => (None, &line.body[..], self.text.clone().into_bytes()),
            KeyKind::Cosigner => {
                let (time, signature) = line.body.split_first_chunk::<8>().ok_or_else(bad)?;
                let time = u64::from_be_bytes(*time);
                (Some(time), signature, cosignature_message(&self.text, time))
            }
        };
        signature::UnparsedPublicKey::new(&signature::ED25519, key.public)
            .verify(&message, signature)
            .map_err(|_| bad())?;
        Ok(Some(Verified { time }))
    }
}

/// The body of a signed checkpoint note (tlog-checkpoint): origin, tree size in
/// decimal, base64 root hash, each on its own line. No extension lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Checkpoint {
    pub origin: String,
    pub size: u64,
    pub root: Hash,
}

impl Checkpoint {
    pub fn text(&self) -> String {
        format!("{}\n{}\n{}\n", self.origin, self.size, b64(&self.root))
    }

    pub fn parse(text: &str) -> Result<Self, Error> {
        let body = text
            .strip_suffix('\n')
            .ok_or(Error::MalformedCheckpoint("must end with a newline"))?;
        let mut lines = body.split('\n');
        let (Some(origin), Some(size), Some(root)) = (lines.next(), lines.next(), lines.next())
        else {
            return Err(Error::MalformedCheckpoint(
                "expected origin, size and root lines",
            ));
        };
        if lines.next().is_some() {
            return Err(Error::CheckpointExtension);
        }
        if origin.is_empty() {
            return Err(Error::MalformedCheckpoint("empty origin"));
        }
        if size.is_empty()
            || !size.bytes().all(|b| b.is_ascii_digit())
            || (size.len() > 1 && size.starts_with('0'))
        {
            return Err(Error::MalformedCheckpoint("size is not a plain decimal"));
        }
        let size = size
            .parse()
            .map_err(|_| Error::MalformedCheckpoint("size does not fit 64 bits"))?;
        let root = unb64(root)?
            .try_into()
            .map_err(|_| Error::MalformedCheckpoint("root is not 32 bytes"))?;
        Ok(Self {
            origin: origin.to_string(),
            size,
            root,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORIGIN: &str = "log.dynodoc.example/dynodoc/v1/transparency";

    fn signer(name: &str, kind: KeyKind, seed: u8) -> Signer {
        Signer::from_seed(name, kind, &[seed; 32]).unwrap()
    }

    fn checkpoint(size: u64) -> Checkpoint {
        Checkpoint {
            origin: ORIGIN.into(),
            size,
            root: [7; 32],
        }
    }

    fn signed(size: u64) -> Note {
        let text = checkpoint(size).text();
        let log = signer(ORIGIN, KeyKind::Log, 1);
        let w1 = signer("witness.one.example/w", KeyKind::Cosigner, 2);
        Note::new(&text)
            .unwrap()
            .with_signature(log.sign_checkpoint(&text).unwrap())
            .unwrap()
            .with_signature(w1.cosign_checkpoint(&text, 1_700_000_000).unwrap())
            .unwrap()
    }

    #[test]
    fn checkpoint_text_round_trips() {
        let cp = checkpoint(20852163);
        let text = cp.text();
        assert!(text.starts_with(&format!("{ORIGIN}\n20852163\n")));
        assert_eq!(Checkpoint::parse(&text).unwrap(), cp);
        assert_eq!(
            Checkpoint::parse(&format!("{ORIGIN}\n0\n{}\n", b64(&[0; 32])))
                .unwrap()
                .size,
            0
        );
    }

    #[test]
    fn malformed_checkpoints_are_rejected() {
        let root = b64(&[7; 32]);
        for (bad, why) in [
            (format!("{ORIGIN}\n5\n{root}"), "no final newline"),
            (format!("{ORIGIN}\n5\n"), "missing root"),
            (format!("\n5\n{root}\n"), "empty origin"),
            (format!("{ORIGIN}\n05\n{root}\n"), "leading zero"),
            (format!("{ORIGIN}\n-5\n{root}\n"), "sign"),
            (format!("{ORIGIN}\n 5\n{root}\n"), "space"),
            (format!("{ORIGIN}\n\n{root}\n"), "empty size"),
            (
                format!("{ORIGIN}\n18446744073709551616\n{root}\n"),
                "overflow",
            ),
            (format!("{ORIGIN}\n5\n{}\n", b64(&[7; 31])), "short root"),
            (
                format!("{ORIGIN}\n5\n{}\n", root.trim_end_matches('=')),
                "unpadded",
            ),
        ] {
            assert!(
                matches!(
                    Checkpoint::parse(&bad),
                    Err(Error::MalformedCheckpoint(_) | Error::Base64(_))
                ),
                "{why}"
            );
        }
        assert_eq!(
            Checkpoint::parse(&format!("{ORIGIN}\n5\n{root}\nextension\n")),
            Err(Error::CheckpointExtension)
        );
    }

    #[test]
    fn key_ids_and_vkeys() {
        let log = signer(ORIGIN, KeyKind::Log, 1).verifier_key();
        let vkey = log.to_string();
        assert!(vkey.starts_with(&format!("{ORIGIN}+")));
        assert_eq!(VerifierKey::parse(&vkey).unwrap(), log);
        // The same name and key as a cosigner has a different type byte, so a different id.
        let as_witness = VerifierKey::new(ORIGIN, KeyKind::Cosigner, *log.public()).unwrap();
        assert_ne!(as_witness.key_id(), log.key_id());
        // Hand-derived: first four bytes of SHA-256(name || 0x0A || 0x01 || key).
        let mut input = ORIGIN.as_bytes().to_vec();
        input.extend_from_slice(&[0x0a, 0x01]);
        input.extend_from_slice(log.public());
        assert_eq!(log.key_id(), digest(&SHA256, &input).as_ref()[..4]);
    }

    #[test]
    fn malformed_vkeys_are_rejected() {
        let log = signer(ORIGIN, KeyKind::Log, 1).verifier_key();
        let name = ORIGIN;
        let id = hex(&log.key_id());
        let key_of = |type_byte: u8| {
            let mut raw = vec![type_byte];
            raw.extend_from_slice(log.public());
            b64(&raw)
        };
        let key = key_of(0x01);
        let other_id = hex(&[log.key_id()[0] ^ 1, 0, 0, 0]);
        assert!(matches!(
            VerifierKey::parse(name),
            Err(Error::MalformedKey(_))
        ));
        assert!(matches!(
            VerifierKey::parse(&format!("{name}+{key}")),
            Err(Error::MalformedKey(_) | Error::Base64(_))
        ));
        assert_eq!(
            VerifierKey::parse(&format!("{name}+{other_id}+{key}")),
            Err(Error::KeyIdMismatch)
        );
        assert!(matches!(
            VerifierKey::parse(&format!("{name}+{}+{key}", id.to_uppercase())),
            Err(Error::MalformedKey(_) | Error::KeyIdMismatch)
        ));
        assert_eq!(
            VerifierKey::parse(&format!("{name}+{id}+{}", key_of(0x02))),
            Err(Error::UnsupportedKeyType(0x02))
        );
        assert!(matches!(
            VerifierKey::parse(&format!("{name}+{id}+{}", b64(&[1, 2, 3]))),
            Err(Error::MalformedKey(_))
        ));
        assert!(matches!(
            VerifierKey::parse(&format!("two words+{id}+{key}")),
            Err(Error::InvalidKeyName)
        ));
        assert!(valid_key_name("a.b/c"));
        assert!(!valid_key_name(""));
        assert!(!valid_key_name("a+b"));
        assert!(!valid_key_name("a b"));
    }

    #[test]
    fn origins_are_schemaless_dynodoc_urls() {
        assert!(Origin::new(ORIGIN).is_ok());
        for bad in [
            "",
            "https://log.example/dynodoc/v1/transparency",
            "log.example/dynodoc/v1",
            "/dynodoc/v1/transparency",
            "a/b/dynodoc/v1/transparency",
            "log.example/dynodoc/v1/transparency/",
            "log example/dynodoc/v1/transparency",
            "log+x.example/dynodoc/v1/transparency",
            "l\u{f6}g.example/dynodoc/v1/transparency",
            "log\".example/dynodoc/v1/transparency",
        ] {
            assert!(Origin::new(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn signed_note_round_trips_and_verifies() {
        let note = signed(10);
        let bytes = note.encode();
        assert!(bytes.contains("\n\n\u{2014} "));
        let parsed = Note::parse(bytes.as_bytes()).unwrap();
        assert_eq!(parsed, note);
        assert_eq!(parsed.encode(), bytes);
        let log = signer(ORIGIN, KeyKind::Log, 1).verifier_key();
        let w1 = signer("witness.one.example/w", KeyKind::Cosigner, 2).verifier_key();
        assert_eq!(parsed.verify(&log).unwrap(), Some(Verified { time: None }));
        assert_eq!(
            parsed.verify(&w1).unwrap(),
            Some(Verified {
                time: Some(1_700_000_000)
            })
        );
        let w2 = signer("witness.two.example/w", KeyKind::Cosigner, 3).verifier_key();
        assert_eq!(parsed.verify(&w2).unwrap(), None);
    }

    #[test]
    fn wrong_key_and_bad_signature_are_distinct_outcomes() {
        let note = signed(10);
        // Same name, different key: a different key id, so the line is not this key's.
        let impostor = signer(ORIGIN, KeyKind::Log, 9).verifier_key();
        assert_eq!(note.verify(&impostor), Ok(None));
        // Right key, damaged signature: an error.
        let log = signer(ORIGIN, KeyKind::Log, 1).verifier_key();
        let mut damaged = note.clone();
        damaged.signatures[0].body[0] ^= 1;
        assert_eq!(
            damaged.verify(&log),
            Err(Error::BadSignature(ORIGIN.into()))
        );
        // A cosignature does not verify against a changed time, text or key kind.
        let w1 = signer("witness.one.example/w", KeyKind::Cosigner, 2).verifier_key();
        let mut late = note.clone();
        late.signatures[1].body[7] ^= 1;
        assert!(matches!(late.verify(&w1), Err(Error::BadSignature(_))));
        let mut other_text = note.clone();
        other_text.text = checkpoint(11).text();
        assert!(matches!(
            other_text.verify(&log),
            Err(Error::BadSignature(_))
        ));
        assert!(matches!(
            other_text.verify(&w1),
            Err(Error::BadSignature(_))
        ));
        // Wrong body length for the key's kind.
        let mut short = note;
        short.signatures[1].body.pop();
        assert!(matches!(short.verify(&w1), Err(Error::BadSignature(_))));
    }

    #[test]
    fn malformed_notes_are_rejected() {
        let good = signed(10).encode();
        let lines: Vec<&str> = good.lines().collect();
        let first_sig = lines[4];
        let cases: Vec<(String, &str)> = vec![
            (String::new(), "empty"),
            (checkpoint(1).text(), "no signatures at all"),
            (
                format!("{}\n", checkpoint(1).text()),
                "blank line but no signature",
            ),
            (good.trim_end().to_string(), "no final newline"),
            (good.replacen("\n\n", "\n", 1), "no blank line"),
            (good.replace('\u{2014}', "-"), "hyphen instead of em dash"),
            (
                good.replacen("\u{2014} ", "\u{2014}", 1),
                "no space after the dash",
            ),
            (
                format!("{}\n{first_sig}\n\n", checkpoint(1).text()),
                "trailing blank line",
            ),
            (
                good.replace("log.dynodoc.example/dynodoc/v1/transparency ", "two words "),
                "name",
            ),
            (
                good.replace(first_sig, "\u{2014} onlyname"),
                "no signature field",
            ),
            (
                good.replace(first_sig, "\u{2014} name AAAA"),
                "signature field too short",
            ),
            (
                good.replace(first_sig, "\u{2014} name not*base64"),
                "bad base64",
            ),
            (good.replacen('\n', "\n\u{7}", 1), "control character"),
        ];
        for (bad, why) in cases {
            assert!(
                matches!(
                    Note::parse(bad.as_bytes()),
                    Err(Error::MalformedNote(_) | Error::Base64(_))
                ),
                "{why}: {:?}",
                Note::parse(bad.as_bytes())
            );
        }
        assert!(matches!(
            Note::parse(&[0xff, 0xfe]),
            Err(Error::MalformedNote("not valid UTF-8"))
        ));
        assert!(matches!(
            Note::new("no newline"),
            Err(Error::MalformedNote(_))
        ));
    }

    #[test]
    fn duplicate_signature_lines_are_rejected() {
        let good = signed(10).encode();
        let lines: Vec<&str> = good.lines().collect();
        let doubled = format!("{good}{}\n", lines[4]);
        assert_eq!(
            Note::parse(doubled.as_bytes()),
            Err(Error::DuplicateSignature(ORIGIN.into()))
        );
        let note = signed(10);
        let again = signer(ORIGIN, KeyKind::Log, 1)
            .sign_checkpoint(&checkpoint(10).text())
            .unwrap();
        assert_eq!(
            note.with_signature(again),
            Err(Error::DuplicateSignature(ORIGIN.into()))
        );
    }

    #[test]
    fn unknown_signature_lines_are_kept_and_ignored() {
        let note = signed(10);
        let stranger = signer("stranger.example/w", KeyKind::Cosigner, 5)
            .cosign_checkpoint(note.text(), 5)
            .unwrap();
        let bytes = note.with_signature(stranger).unwrap().encode();
        let parsed = Note::parse(bytes.as_bytes()).unwrap();
        assert_eq!(parsed.signatures().len(), 3);
        let log = signer(ORIGIN, KeyKind::Log, 1).verifier_key();
        assert!(parsed.verify(&log).unwrap().is_some());
    }

    #[test]
    fn signers_refuse_the_wrong_role_and_origin() {
        let text = checkpoint(1).text();
        let witness = signer("witness.one.example/w", KeyKind::Cosigner, 2);
        let log = signer(ORIGIN, KeyKind::Log, 1);
        assert_eq!(witness.sign_checkpoint(&text), Err(Error::WrongSignerKind));
        assert_eq!(log.cosign_checkpoint(&text, 1), Err(Error::WrongSignerKind));
        let elsewhere = signer("other.example/dynodoc/v1/transparency", KeyKind::Log, 1);
        assert!(matches!(
            elsewhere.sign_checkpoint(&text),
            Err(Error::WrongOrigin { .. })
        ));
        assert!(matches!(
            log.sign_checkpoint("not a checkpoint\n"),
            Err(Error::MalformedCheckpoint(_))
        ));
        assert!(matches!(
            Signer::from_seed("bad name", KeyKind::Log, &[1; 32]),
            Err(Error::InvalidKeyName)
        ));
    }

    #[test]
    fn pkcs8_keys_sign_like_seed_keys() {
        let rng = ring::rand::SystemRandom::new();
        let pkcs8 = Ed25519KeyPair::generate_pkcs8(&rng).unwrap();
        let log = Signer::from_pkcs8(ORIGIN, KeyKind::Log, pkcs8.as_ref()).unwrap();
        let text = checkpoint(3).text();
        let note = Note::new(&text)
            .unwrap()
            .with_signature(log.sign_checkpoint(&text).unwrap())
            .unwrap();
        assert!(note.verify(&log.verifier_key()).unwrap().is_some());
        assert!(Signer::from_pkcs8(ORIGIN, KeyKind::Log, b"junk").is_err());
    }
}
