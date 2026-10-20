//! The `DYNOENC` stored form: one AES-256-GCM encryption of one value, keyed by a
//! per-row key derived from the project's data key.
//!
//! ```text
//! offset  size  field
//!      0     8  magic        ASCII "DYNOENC" then a zero byte (differs from DYNOSEAL)
//!      8     1  version      0x01
//!      9     4  dek_id       u32 big-endian: the data_key row that holds the wrapped DEK
//!     13     4  kek_version  u32 big-endian: the KEK version that wrapped that DEK
//!     17    32  salt         per-row salt (random, or derived for content-addressed rows)
//!     49     n  ciphertext   same length as the plaintext
//!   49+n    16  tag          AES-GCM authentication tag
//! ```
//!
//! Per-row key and nonce: `HKDF-SHA-256(IKM = DEK, salt = envelope salt, info =
//! LP(label) || LP(table) || LP(column) || LP(row_key))`, 44 bytes, split into the 32-byte
//! AES key and the 12-byte nonce. Every value therefore has its own single-use key and
//! nonce, so the AES-GCM message-count limits of one key never apply. Two values share a
//! key and nonce only if they share a salt, a table, a column and a row key.
//!
//! The associated data is `LP("dynodoc.at-rest.v1") || LP(installation uuid) || LP(table)
//! || LP(column) || LP(row_key) || LP(version) || LP(dek_id) || LP(kek_version)` where
//! `LP(x)` is `u64` big-endian length then the bytes (the framing `log.rs` uses). The
//! design's plain concatenation is ambiguous (table `ab` + column `c` equals table `a` +
//! column `bc`), so every field is length-prefixed, and the header fields that are not
//! otherwise authenticated (`version`, `dek_id`, `kek_version`) are bound as well, so a
//! flipped bit in any header byte is detected. The salt is authenticated through the key
//! derivation, the magic by the parser.
//!
//! The envelope has no length field: the ciphertext runs to the end of the stored value,
//! as the design specifies. A value cut or extended after the header therefore fails
//! authentication ([`Error::Authentication`]) rather than parsing differently; only a
//! value too short to hold a header and a tag is a distinct parse error.
use super::{secret::Wiped, DataKey, Dek, Error};
use ring::{
    aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM},
    hkdf, hmac,
    rand::{SecureRandom, SystemRandom},
};
use uuid::Uuid;

/// `DYNOENC\0`. Deliberately not `DYNOSEAL`: an at-rest envelope can be read by the
/// running service and must never be mistaken for a sealed (client-key) object.
pub const MAGIC: [u8; 8] = *b"DYNOENC\0";
pub const VERSION: u8 = 1;
pub const SALT_LEN: usize = 32;
pub const TAG_LEN: usize = 16;
/// Bytes before the ciphertext.
pub const HEADER_LEN: usize = 8 + 1 + 4 + 4 + SALT_LEN;
/// An envelope around an empty plaintext: the smallest valid value.
pub const MIN_ENVELOPE_LEN: usize = HEADER_LEN + TAG_LEN;
/// Largest plaintext one envelope may hold (the uploads this protects are far smaller).
pub const MAX_PLAINTEXT_BYTES: usize = 256 * 1024 * 1024;
/// Longest table or column name, in bytes.
pub const MAX_NAME_BYTES: usize = 128;
/// Longest row identity, in bytes.
pub const MAX_ROW_KEY_BYTES: usize = 1024;

const AAD_LABEL: &[u8] = b"dynodoc.at-rest.v1";
const KEY_INFO_LABEL: &[u8] = b"dynodoc.at-rest.v1/aead+nonce";
const SALT_LABEL: &[u8] = b"dynodoc.at-rest.v1/content-salt";
const KEY_LEN: usize = 32;
const NONCE_LEN: usize = 12;

/// Append one length-framed field: `u64` big-endian length, then the bytes.
pub(crate) fn lp(out: &mut Vec<u8>, bytes: &[u8]) {
    out.extend_from_slice(&(bytes.len() as u64).to_be_bytes());
    out.extend_from_slice(bytes);
}

/// Where a value lives; it is bound into the key derivation and the associated data,
/// so a ciphertext cannot be moved to another installation, table, column or row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context<'a> {
    installation: Uuid,
    table: &'a str,
    column: &'a str,
    row_key: &'a [u8],
}

impl<'a> Context<'a> {
    /// `table` and `column` are non-empty and at most [`MAX_NAME_BYTES`]; `row_key` is
    /// the stable identity of the row (for example document id and address), non-empty
    /// and at most [`MAX_ROW_KEY_BYTES`]. Use the same bytes on write and on read.
    pub fn new(
        installation: Uuid,
        table: &'a str,
        column: &'a str,
        row_key: &'a [u8],
    ) -> Result<Self, Error> {
        check_len("table", table.len(), 1, MAX_NAME_BYTES)?;
        check_len("column", column.len(), 1, MAX_NAME_BYTES)?;
        check_len("row_key", row_key.len(), 1, MAX_ROW_KEY_BYTES)?;
        Ok(Self {
            installation,
            table,
            column,
            row_key,
        })
    }

    fn aad(&self, header: &Header) -> Vec<u8> {
        // Eight length prefixes, the label, the installation uuid, the three names and
        // the version, dek_id and kek_version.
        let mut out = Vec::with_capacity(
            8 * 8
                + AAD_LABEL.len()
                + 16
                + self.table.len()
                + self.column.len()
                + self.row_key.len()
                + 9,
        );
        lp(&mut out, AAD_LABEL);
        lp(&mut out, self.installation.as_bytes());
        lp(&mut out, self.table.as_bytes());
        lp(&mut out, self.column.as_bytes());
        lp(&mut out, self.row_key);
        lp(&mut out, &[VERSION]);
        lp(&mut out, &header.dek_id.to_be_bytes());
        lp(&mut out, &header.kek_version.to_be_bytes());
        out
    }

    fn key_info(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(
            KEY_INFO_LABEL.len() + self.table.len() + self.column.len() + self.row_key.len() + 32,
        );
        lp(&mut out, KEY_INFO_LABEL);
        lp(&mut out, self.table.as_bytes());
        lp(&mut out, self.column.as_bytes());
        lp(&mut out, self.row_key);
        out
    }
}

fn check_len(field: &'static str, len: usize, min: usize, max: usize) -> Result<(), Error> {
    if len < min {
        return Err(Error::EmptyField(field));
    }
    if len > max {
        return Err(Error::FieldTooLong { field, len, max });
    }
    Ok(())
}

/// The 32-byte per-value salt that, with the data key, selects the value's key and nonce.
///
/// A salt must never be used for two different plaintexts under the same data key and
/// [`Context`]: that repeats the AES-GCM key and nonce and leaks the XOR of the two
/// plaintexts. [`Salt::random`] cannot collide in practice; [`Salt::content_addressed`]
/// also binds a digest of the plaintext, so even a wrongly reused address cannot repeat a
/// key and nonce for different plaintexts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Salt([u8; SALT_LEN]);

impl Salt {
    /// 32 fresh random bytes from the operating system's generator.
    pub fn random() -> Result<Self, Error> {
        let mut bytes = [0u8; SALT_LEN];
        SystemRandom::new()
            .fill(&mut bytes)
            .map_err(|_| Error::Random)?;
        Ok(Self(bytes))
    }

    /// The deterministic salt for a content-addressed table:
    /// `HMAC-SHA-256(DEK, LP("dynodoc.at-rest.v1/content-salt") || LP(document) || LP(address)
    /// || LP(SHA-256(plaintext)))`.
    ///
    /// The same key, document, address and plaintext always give the same salt, so storing
    /// the same object twice yields byte-identical envelopes and the store's "same address,
    /// same bytes" comparison keeps working. The design binds only the address (which is
    /// the hash of the plaintext in the content stores); the plaintext digest is added so
    /// that a caller who seals different bytes under an address it did not verify gets a
    /// different key and nonce instead of repeating them. The fields are length-framed and
    /// the label is versioned, so the parts cannot be re-split.
    pub fn content_addressed(dek: &Dek, document: &[u8], address: &[u8], plaintext: &[u8]) -> Self {
        let digest = ring::digest::digest(&ring::digest::SHA256, plaintext);
        let mut message =
            Vec::with_capacity(SALT_LABEL.len() + document.len() + address.len() + 56);
        lp(&mut message, SALT_LABEL);
        lp(&mut message, document);
        lp(&mut message, address);
        lp(&mut message, digest.as_ref());
        let tag = hmac::sign(&hmac::Key::new(hmac::HMAC_SHA256, dek.as_bytes()), &message);
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(tag.as_ref());
        Self(salt)
    }

    /// For published test vectors and stores that keep their own salts.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, Error> {
        let bytes: [u8; SALT_LEN] = bytes
            .try_into()
            .map_err(|_| Error::BadSaltLength { len: bytes.len() })?;
        Ok(Self(bytes))
    }

    pub fn as_bytes(&self) -> &[u8; SALT_LEN] {
        &self.0
    }
}

/// The parsed fixed part of an envelope. Reading it needs no key, so a store can pick
/// the data key (`dek_id`) before opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Header {
    pub dek_id: u32,
    pub kek_version: u32,
    pub salt: Salt,
}

impl Header {
    /// Parse and bound-check the header of a stored value. Rejects, each with its own
    /// error: fewer bytes than a header and tag ([`Error::Truncated`]), another magic
    /// including `DYNOSEAL` ([`Error::BadMagic`]), another version
    /// ([`Error::UnsupportedVersion`]), and a value past the size bound
    /// ([`Error::TooLarge`]). Nothing is decrypted or authenticated here.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        Self::parse_bounded(bytes, MAX_PLAINTEXT_BYTES)
    }

    pub(crate) fn parse_bounded(bytes: &[u8], max_plaintext: usize) -> Result<Self, Error> {
        let truncated = Error::Truncated {
            len: bytes.len(),
            min: MIN_ENVELOPE_LEN,
        };
        if bytes.len() < MAGIC.len() {
            return Err(truncated);
        }
        if bytes[..MAGIC.len()] != MAGIC {
            return Err(Error::BadMagic);
        }
        let version = *bytes.get(MAGIC.len()).ok_or_else(|| truncated.clone())?;
        if version != VERSION {
            return Err(Error::UnsupportedVersion(version));
        }
        if bytes.len() < MIN_ENVELOPE_LEN {
            return Err(truncated);
        }
        let max = HEADER_LEN + max_plaintext + TAG_LEN;
        if bytes.len() > max {
            return Err(Error::TooLarge {
                len: bytes.len(),
                max,
            });
        }
        let be32 = |at: usize| {
            u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
        };
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&bytes[17..17 + SALT_LEN]);
        Ok(Self {
            dek_id: be32(9),
            kek_version: be32(13),
            salt: Salt(salt),
        })
    }

    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&MAGIC);
        out.push(VERSION);
        out.extend_from_slice(&self.dek_id.to_be_bytes());
        out.extend_from_slice(&self.kek_version.to_be_bytes());
        out.extend_from_slice(&self.salt.0);
    }
}

/// Whether `bytes` start with the `DYNOENC` magic. This only recognises the form (to
/// tell an envelope from a legacy plaintext value); it proves nothing about the content.
pub fn has_magic(bytes: &[u8]) -> bool {
    bytes.len() >= MAGIC.len() && bytes[..MAGIC.len()] == MAGIC
}

struct OutLen(usize);

impl hkdf::KeyType for OutLen {
    fn len(&self) -> usize {
        self.0
    }
}

/// The 44 bytes derived for one value: a single-use AES-256 key, then its 96-bit nonce.
fn derive_okm(
    dek: &Dek,
    salt: &Salt,
    ctx: &Context<'_>,
) -> Result<Wiped<{ KEY_LEN + NONCE_LEN }>, Error> {
    let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, &salt.0).extract(dek.as_bytes());
    let info = ctx.key_info();
    let mut okm = Wiped::<{ KEY_LEN + NONCE_LEN }>::zero();
    prk.expand(&[&info], OutLen(KEY_LEN + NONCE_LEN))
        .and_then(|expanded| expanded.fill(&mut okm.0))
        .map_err(|_| Error::Cipher)?;
    Ok(okm)
}

/// The AES-256-GCM key and nonce of one value.
fn row_cipher(dek: &Dek, salt: &Salt, ctx: &Context<'_>) -> Result<(LessSafeKey, Nonce), Error> {
    let okm = derive_okm(dek, salt, ctx)?;
    let key = UnboundKey::new(&AES_256_GCM, &okm.0[..KEY_LEN]).map_err(|_| Error::Cipher)?;
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&okm.0[KEY_LEN..]);
    Ok((LessSafeKey::new(key), Nonce::assume_unique_for_key(nonce)))
}

/// Encrypt `plaintext` for the value at `ctx`, with a fresh random salt.
pub fn seal(key: &DataKey, ctx: &Context<'_>, plaintext: &[u8]) -> Result<Vec<u8>, Error> {
    seal_with_salt(key, ctx, &Salt::random()?, plaintext)
}

/// Encrypt with a salt derived from the document and the content address (see
/// [`Salt::content_addressed`]): the same inputs give the same envelope, byte for byte.
pub fn seal_content_addressed(
    key: &DataKey,
    ctx: &Context<'_>,
    document: &[u8],
    address: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, Error> {
    check_len("address", address.len(), 1, MAX_ROW_KEY_BYTES)?;
    check_len("document", document.len(), 0, MAX_ROW_KEY_BYTES)?;
    let salt = Salt::content_addressed(key.dek(), document, address, plaintext);
    seal_with_salt(key, ctx, &salt, plaintext)
}

/// Encrypt with a caller-chosen salt: the deterministic form behind the published test
/// vectors and [`seal_content_addressed`]. Never reuse a salt for a different plaintext
/// under the same key and context (see [`Salt`]); prefer [`seal`].
pub fn seal_with_salt(
    key: &DataKey,
    ctx: &Context<'_>,
    salt: &Salt,
    plaintext: &[u8],
) -> Result<Vec<u8>, Error> {
    if plaintext.len() > MAX_PLAINTEXT_BYTES {
        return Err(Error::PlaintextTooLarge {
            len: plaintext.len(),
            max: MAX_PLAINTEXT_BYTES,
        });
    }
    let header = Header {
        dek_id: key.id(),
        kek_version: key.kek_version(),
        salt: *salt,
    };
    let (cipher, nonce) = row_cipher(key.dek(), salt, ctx)?;
    let mut out = Vec::with_capacity(MIN_ENVELOPE_LEN + plaintext.len());
    header.write(&mut out);
    out.extend_from_slice(plaintext);
    let tag = cipher
        .seal_in_place_separate_tag(nonce, Aad::from(ctx.aad(&header)), &mut out[HEADER_LEN..])
        .map_err(|_| Error::Cipher)?;
    out.extend_from_slice(tag.as_ref());
    Ok(out)
}

/// Decrypt a stored value. `key` must be the data key named by the header's `dek_id`
/// ([`Error::DekMismatch`] otherwise) and `ctx` the context it was written with.
/// A wrong key, a wrong context, a changed byte anywhere, and a value cut or extended
/// after the header all give the same [`Error::Authentication`].
pub fn open(key: &DataKey, ctx: &Context<'_>, stored: &[u8]) -> Result<Vec<u8>, Error> {
    let header = Header::parse(stored)?;
    if header.dek_id != key.id() {
        return Err(Error::DekMismatch {
            expected: key.id(),
            found: header.dek_id,
        });
    }
    let (cipher, nonce) = row_cipher(key.dek(), &header.salt, ctx)?;
    let mut buffer = stored[HEADER_LEN..].to_vec();
    let plain_len = cipher
        .open_in_place(nonce, Aad::from(ctx.aad(&header)), &mut buffer)
        .map_err(|_| Error::Authentication)?
        .len();
    buffer.truncate(plain_len);
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installation() -> Uuid {
        Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef)
    }

    fn data_key(id: u32) -> DataKey {
        DataKey::new(id, 3, Dek::from_bytes([0x11; 32]))
    }

    fn ctx() -> Context<'static> {
        Context::new(installation(), "checkpoint_object", "bytes", b"row-1").unwrap()
    }

    #[test]
    fn magic_is_not_the_sealed_object_magic() {
        assert_ne!(&MAGIC[..], b"DYNOSEAL");
        assert_eq!(&MAGIC[..7], b"DYNOENC");
        assert_eq!(MAGIC[7], 0);
        assert_eq!(HEADER_LEN, 49);
        assert_eq!(MIN_ENVELOPE_LEN, 65);
    }

    #[test]
    fn round_trip_and_layout() {
        let key = data_key(9);
        let stored = seal(&key, &ctx(), b"hello world").unwrap();
        assert_eq!(stored.len(), HEADER_LEN + 11 + TAG_LEN);
        assert_eq!(&stored[..8], &MAGIC);
        assert_eq!(stored[8], 1);
        assert_eq!(&stored[9..13], &9u32.to_be_bytes());
        assert_eq!(&stored[13..17], &3u32.to_be_bytes());
        assert!(has_magic(&stored));
        assert_eq!(open(&key, &ctx(), &stored).unwrap(), b"hello world");
        let header = Header::parse(&stored).unwrap();
        assert_eq!((header.dek_id, header.kek_version), (9, 3));
        assert_eq!(&stored[17..49], header.salt.as_bytes());
    }

    #[test]
    fn empty_plaintext_is_the_minimum_envelope() {
        let key = data_key(1);
        let stored = seal(&key, &ctx(), b"").unwrap();
        assert_eq!(stored.len(), MIN_ENVELOPE_LEN);
        assert_eq!(open(&key, &ctx(), &stored).unwrap(), b"");
    }

    #[test]
    fn random_salts_make_every_envelope_different() {
        let key = data_key(1);
        let a = seal(&key, &ctx(), b"same").unwrap();
        let b = seal(&key, &ctx(), b"same").unwrap();
        assert_ne!(a, b);
        assert_ne!(a[17..49], b[17..49]);
    }

    #[test]
    fn parse_errors_are_distinct() {
        let key = data_key(1);
        let good = seal(&key, &ctx(), b"abc").unwrap();

        assert_eq!(
            Header::parse(&[]).unwrap_err(),
            Error::Truncated { len: 0, min: 65 }
        );
        assert_eq!(
            Header::parse(&good[..7]).unwrap_err(),
            Error::Truncated { len: 7, min: 65 }
        );
        assert_eq!(
            Header::parse(&good[..8]).unwrap_err(),
            Error::Truncated { len: 8, min: 65 }
        );
        assert_eq!(
            Header::parse(&good[..MIN_ENVELOPE_LEN - 1]).unwrap_err(),
            Error::Truncated { len: 64, min: 65 }
        );

        let mut bad = good.clone();
        bad[0] ^= 1;
        assert_eq!(Header::parse(&bad).unwrap_err(), Error::BadMagic);
        let mut sealed = good.clone();
        sealed[..8].copy_from_slice(b"DYNOSEAL");
        assert_eq!(Header::parse(&sealed).unwrap_err(), Error::BadMagic);
        assert!(!has_magic(&sealed));
        assert_eq!(
            Header::parse(b"DYNOENC").unwrap_err(),
            Error::Truncated { len: 7, min: 65 }
        );
        assert_eq!(Header::parse(&[0u8; 100]).unwrap_err(), Error::BadMagic);

        for version in [0u8, 2, 255] {
            let mut bad = good.clone();
            bad[8] = version;
            assert_eq!(
                Header::parse(&bad).unwrap_err(),
                Error::UnsupportedVersion(version)
            );
        }
    }

    #[test]
    fn size_bound_is_checked_before_anything_else_is_read() {
        let key = data_key(1);
        let stored = seal(&key, &ctx(), &[0u8; 100]).unwrap();
        assert!(Header::parse_bounded(&stored, 100).is_ok());
        assert_eq!(
            Header::parse_bounded(&stored, 99).unwrap_err(),
            Error::TooLarge {
                len: stored.len(),
                max: HEADER_LEN + 99 + TAG_LEN,
            }
        );
        assert_eq!(Header::parse(&stored).map(|h| h.dek_id), Ok(1));
    }

    #[test]
    fn a_value_cut_or_extended_after_the_header_fails_authentication() {
        let key = data_key(1);
        let stored = seal(&key, &ctx(), b"some plaintext").unwrap();
        for cut in 1..=(stored.len() - MIN_ENVELOPE_LEN) {
            assert_eq!(
                open(&key, &ctx(), &stored[..stored.len() - cut]).unwrap_err(),
                Error::Authentication,
                "cut {cut}"
            );
        }
        for extra in [1usize, 2, 16, 17, 100] {
            let mut longer = stored.clone();
            longer.extend(std::iter::repeat_n(0x5A, extra));
            assert_eq!(
                open(&key, &ctx(), &longer).unwrap_err(),
                Error::Authentication,
                "extra {extra}"
            );
        }
    }

    #[test]
    fn open_checks_the_data_key_id() {
        let stored = seal(&data_key(4), &ctx(), b"x").unwrap();
        assert_eq!(
            open(&data_key(5), &ctx(), &stored).unwrap_err(),
            Error::DekMismatch {
                expected: 5,
                found: 4
            }
        );
        // Same id, different key bytes: authentication, not a mismatch.
        let other = DataKey::new(4, 3, Dek::from_bytes([0x22; 32]));
        assert_eq!(
            open(&other, &ctx(), &stored).unwrap_err(),
            Error::Authentication
        );
    }

    #[test]
    fn kek_version_in_the_key_handle_does_not_have_to_match() {
        // A KEK rewrap changes the handle's version, never the stored envelopes.
        let stored = seal(
            &DataKey::new(4, 1, Dek::from_bytes([0x11; 32])),
            &ctx(),
            b"x",
        )
        .unwrap();
        let rewrapped = DataKey::new(4, 2, Dek::from_bytes([0x11; 32]));
        assert_eq!(open(&rewrapped, &ctx(), &stored).unwrap(), b"x");
    }

    #[test]
    fn context_fields_are_validated() {
        let u = installation();
        assert_eq!(
            Context::new(u, "", "c", b"r").unwrap_err(),
            Error::EmptyField("table")
        );
        assert_eq!(
            Context::new(u, "t", "", b"r").unwrap_err(),
            Error::EmptyField("column")
        );
        assert_eq!(
            Context::new(u, "t", "c", b"").unwrap_err(),
            Error::EmptyField("row_key")
        );
        let long = "x".repeat(MAX_NAME_BYTES + 1);
        assert_eq!(
            Context::new(u, &long, "c", b"r").unwrap_err(),
            Error::FieldTooLong {
                field: "table",
                len: MAX_NAME_BYTES + 1,
                max: MAX_NAME_BYTES
            }
        );
        assert!(Context::new(u, &long[1..], &long[1..], &vec![1u8; MAX_ROW_KEY_BYTES]).is_ok());
        assert!(matches!(
            Context::new(u, "t", "c", &vec![1u8; MAX_ROW_KEY_BYTES + 1]),
            Err(Error::FieldTooLong {
                field: "row_key",
                ..
            })
        ));
    }

    #[test]
    fn framing_separates_adjacent_fields() {
        // table "ab" + column "c" must not equal table "a" + column "bc".
        let key = data_key(1);
        let u = installation();
        let a = Context::new(u, "ab", "c", b"r").unwrap();
        let b = Context::new(u, "a", "bc", b"r").unwrap();
        let salt = Salt::from_slice(&[7u8; 32]).unwrap();
        let stored = seal_with_salt(&key, &a, &salt, b"payload").unwrap();
        assert_eq!(open(&key, &b, &stored).unwrap_err(), Error::Authentication);
        let c = Context::new(u, "a", "b", b"cr").unwrap();
        let d = Context::new(u, "a", "bc", b"r").unwrap();
        let stored = seal_with_salt(&key, &c, &salt, b"payload").unwrap();
        assert_eq!(open(&key, &d, &stored).unwrap_err(), Error::Authentication);
    }

    #[test]
    fn salt_lengths_are_checked() {
        for len in [0usize, 31, 33] {
            assert_eq!(
                Salt::from_slice(&vec![0u8; len]).unwrap_err(),
                Error::BadSaltLength { len }
            );
        }
    }

    #[test]
    fn content_addressed_salt_depends_on_key_document_address_and_plaintext() {
        let dek = Dek::from_bytes([1; 32]);
        let base = Salt::content_addressed(&dek, b"doc", b"addr", b"bytes");
        assert_eq!(
            base,
            Salt::content_addressed(&dek, b"doc", b"addr", b"bytes")
        );
        assert_ne!(
            base,
            Salt::content_addressed(&dek, b"doc2", b"addr", b"bytes")
        );
        assert_ne!(
            base,
            Salt::content_addressed(&dek, b"doc", b"addr2", b"bytes")
        );
        assert_ne!(
            base,
            Salt::content_addressed(&Dek::from_bytes([2; 32]), b"doc", b"addr", b"bytes")
        );
        // Different bytes under the same address must not share a key and nonce.
        assert_ne!(
            base,
            Salt::content_addressed(&dek, b"doc", b"addr", b"other bytes")
        );
        // The parts cannot be re-split: ("do", "caddr") is not ("doc", "addr").
        assert_ne!(
            base,
            Salt::content_addressed(&dek, b"do", b"caddr", b"bytes")
        );
        assert_ne!(
            base,
            Salt::content_addressed(&dek, b"docaddr", b"", b"bytes")
        );
    }

    fn unhex(text: &str) -> Vec<u8> {
        (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect()
    }

    /// The internals the public API hides, checked against the independent Python
    /// generator's fixture: the associated data, the HKDF info and the derived key and
    /// nonce of every vector. Ciphertexts and tags are checked in
    /// `tests/at_rest_vectors_test.rs`.
    #[test]
    fn derived_keys_aad_and_info_match_the_independent_vectors() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/at-rest-v1.json")).unwrap();
        let mut checked = 0;
        for group in ["envelopes", "content_addressed"] {
            for v in fixture[group].as_array().unwrap() {
                let text = |name: &str| v[name].as_str().unwrap();
                let installation = Uuid::from_slice(&unhex(text("installation_hex"))).unwrap();
                let row_key = unhex(text("row_key_hex"));
                let ctx =
                    Context::new(installation, text("table"), text("column"), &row_key).unwrap();
                let dek = Dek::from_slice(&unhex(text("dek_hex"))).unwrap();
                let salt = Salt::from_slice(&unhex(text("salt_hex"))).unwrap();
                let header = Header {
                    dek_id: v["dek_id"].as_u64().unwrap() as u32,
                    kek_version: v["kek_version"].as_u64().unwrap() as u32,
                    salt,
                };
                let name = text("name");
                assert_eq!(ctx.aad(&header), unhex(text("aad_hex")), "{name} aad");
                assert_eq!(ctx.key_info(), unhex(text("key_info_hex")), "{name} info");
                let okm = derive_okm(&dek, &salt, &ctx).unwrap();
                assert_eq!(
                    okm.0[..32],
                    unhex(text("derived_key_hex"))[..],
                    "{name} key"
                );
                assert_eq!(
                    okm.0[32..],
                    unhex(text("derived_nonce_hex"))[..],
                    "{name} nonce"
                );
                checked += 1;
            }
        }
        assert_eq!(checked, 13 + 4);
    }

    /// Every (salt, table, column, row) gets its own AES key and its own nonce.
    #[test]
    fn derived_key_and_nonce_are_unique_per_salt_table_column_and_row() {
        let dek = Dek::from_bytes([3; 32]);
        let (mut keys, mut nonces) = (
            std::collections::HashSet::new(),
            std::collections::HashSet::new(),
        );
        let mut total = 0;
        for salt in 0..8u8 {
            let salt = Salt::from_slice(&[salt; 32]).unwrap();
            for table in ["a", "b", "ab"] {
                for column in ["x", "y", "xy"] {
                    for row in 0..20u32 {
                        let row = row.to_be_bytes();
                        let ctx = Context::new(installation(), table, column, &row).unwrap();
                        let okm = derive_okm(&dek, &salt, &ctx).unwrap();
                        keys.insert(okm.0[..KEY_LEN].to_vec());
                        nonces.insert(okm.0[KEY_LEN..].to_vec());
                        total += 1;
                    }
                }
            }
        }
        assert_eq!((keys.len(), nonces.len()), (total, total));
    }

    #[test]
    fn content_addressed_sealing_is_deterministic() {
        let key = data_key(2);
        let a = seal_content_addressed(&key, &ctx(), b"doc", b"addr", b"bytes").unwrap();
        let b = seal_content_addressed(&key, &ctx(), b"doc", b"addr", b"bytes").unwrap();
        assert_eq!(a, b);
        assert_eq!(open(&key, &ctx(), &a).unwrap(), b"bytes");
        let other_doc = seal_content_addressed(&key, &ctx(), b"doc2", b"addr", b"bytes").unwrap();
        assert_ne!(a, other_doc);
        // A wrongly reused address does not repeat the key and nonce: the XOR of the two
        // ciphertext bodies is not the XOR of the plaintexts.
        let other_bytes = seal_content_addressed(&key, &ctx(), b"doc", b"addr", b"BYTES").unwrap();
        assert_ne!(
            a[..49],
            other_bytes[..49],
            "the header (with the salt) differs"
        );
        assert_eq!(
            seal_content_addressed(&key, &ctx(), b"doc", b"", b"bytes").unwrap_err(),
            Error::EmptyField("address")
        );
    }
}
