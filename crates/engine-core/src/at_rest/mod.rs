//! Customer-managed at-rest envelope encryption: the library core (Phase 0 of
//! docs/END-TO-END-ENCRYPTION.md section 7). Format, hierarchy and limits are in
//! docs/AT-REST-ENVELOPE.md.
//!
//! **What this is.** Service-decryptable protection of stored values: a value is
//! encrypted under a key derived from a per-project data key (DEK), and the DEK is held
//! only wrapped by a key provider the customer controls. The running service decrypts
//! in memory to do its work. This is not end-to-end encryption and gives no protection
//! against the service, its operators with process access, or a compromised
//! application. Wording rules: claims policy level 1, same document, section 3.
//!
//! **What exists.** The `DYNOENC` stored form ([`seal`], [`open`], [`Header`]), the key
//! hierarchy (KEK wraps DEK, DEK derives a key and nonce per value), the
//! [`KeyProvider`] trait with a local-file provider and a deterministic test provider,
//! and a [`DekCache`] with a TTL. **What does not.** Nothing in the storage or API code
//! uses this module yet: no store wrapper, no `data_key` tables, no migration, no
//! backfill, no Vault or cloud KMS provider.
//!
//! ```text
//!   KEK (provider) --wrap/unwrap, bound to installation + project + purpose--> DEK
//!   DEK --HKDF-SHA-256(salt per value, info = table/column/row)--> key + nonce
//!   key + nonce --AES-256-GCM, AAD = installation/table/column/row/header--> DYNOENC
//! ```
mod cache;
mod envelope;
mod provider;
mod secret;

pub use cache::{Clock, DekCache, ManualClock, SystemClock, DEFAULT_MAX_ENTRIES, DEFAULT_TTL};
pub use envelope::{
    has_magic, open, seal, seal_content_addressed, seal_with_salt, Context, Header, Salt,
    HEADER_LEN, MAGIC, MAX_NAME_BYTES, MAX_PLAINTEXT_BYTES, MAX_ROW_KEY_BYTES, MIN_ENVELOPE_LEN,
    SALT_LEN, TAG_LEN, VERSION,
};
pub use provider::{
    FileKeyProvider, KeyProvider, ProviderError, ProviderInfo, Purpose, TestKeyProvider,
    WrapContext, Wrapped, MAX_KEK_FILE_BYTES, MAX_KEK_VERSIONS, MAX_WRAPPED_BYTES,
    MIN_WRAPPED_BYTES,
};
pub use secret::{DataKey, Dek, Kek};

/// Everything the envelope and the key types can refuse, one variant per rule so callers
/// and tests can tell the rules apart. No message contains key bytes or plaintext.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("not an at-rest envelope: {len} bytes is shorter than the {min}-byte minimum")]
    Truncated { len: usize, min: usize },
    #[error("not an at-rest envelope: wrong magic")]
    BadMagic,
    #[error("unsupported at-rest envelope version {0}")]
    UnsupportedVersion(u8),
    #[error("at-rest envelope of {len} bytes is over the {max}-byte bound")]
    TooLarge { len: usize, max: usize },
    #[error("plaintext of {len} bytes is over the {max}-byte bound")]
    PlaintextTooLarge { len: usize, max: usize },
    #[error("the envelope names data key {found}, not {expected}")]
    DekMismatch { expected: u32, found: u32 },
    /// A wrong key, a wrong context and any change to the stored value look the same.
    #[error("cannot decrypt: wrong key, wrong context, or the value was changed")]
    Authentication,
    #[error("{0} must not be empty")]
    EmptyField(&'static str),
    #[error("{field} is {len} bytes, at most {max} are allowed")]
    FieldTooLong {
        field: &'static str,
        len: usize,
        max: usize,
    },
    #[error("a salt is {} bytes, got {len}", SALT_LEN)]
    BadSaltLength { len: usize },
    #[error("a key is 32 bytes, got {len}")]
    BadKeyLength { len: usize },
    #[error("system random generator failed")]
    Random,
    #[error("cipher setup failed")]
    Cipher,
}
