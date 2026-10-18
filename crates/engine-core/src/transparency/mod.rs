//! Transparency log core and client verifier (design
//! `docs/DESIGN-WITNESSES-AND-TRANSPARENCY.md`, phases W0, W1 core and W2 verifier
//! library).
//!
//! Pure and storage-agnostic: nothing here is used by an existing code path, and
//! nothing touches PostgreSQL, the network, a clock or a thread. Callers provide
//! bytes, keys, a [`tiles::TileStore`] and the current time. Not built: the outbox
//! and leader worker, HTTP endpoints, the witness protocol and service, journal v2,
//! CLI verbs and `sync` integration (design W1-server, W2-sync, W3 and later).
//!
//! | Module | Contents |
//! |---|---|
//! | [`merkle`] | RFC 6962 hashing, tree heads, inclusion and consistency proofs (generate, verify) |
//! | [`tiles`] | `tlog-tiles` layout (height 8), [`tiles::TileStore`], memory and directory stores, [`tiles::TileLog`] |
//! | [`log`] | [`log::Log`]: typed leaves, checkpoints, signing, inclusion bundles |
//! | [`note`] | signed notes, vkeys, checkpoints, log signatures and cosignatures |
//! | [`leaf`] | the six leaf types, canonical encoding, blinded tags |
//! | [`bundle`] | the inclusion bundle and its canonical JSON |
//! | [`policy`] | the trust policy and its validating loader |
//! | [`assurance`] | levels L0 to L3, known checkpoints, rollback and split-view detection |
//!
//! # Standards and what was checked
//!
//! No network and no published text were available when this was written. Everything
//! below is from the design's field-level description and from memory of the
//! standards. Each point is **unverified: check at build time** against the live
//! text before any interoperability claim:
//!
//! * C2SP `signed-note`: line format (em dash, space, name, space, base64 of key id
//!   and signature), key id = first four bytes of `SHA-256(name || 0x0A || type ||
//!   key)`, Ed25519 type `0x01`, `vkey` text form, forbidden characters.
//! * C2SP `tlog-checkpoint`: origin, decimal size, base64 root, no extension lines.
//! * C2SP `tlog-cosignature` `cosignature/v1`: type byte `0x04`, signed message
//!   `cosignature/v1\ntime <decimal>\n<checkpoint text>`, signature-line body of an
//!   8-byte big-endian time and a 64-byte signature.
//! * C2SP `tlog-tiles`: height 8, `tile/<L>/<N>[.p/<W>]`, `x`-prefixed three-digit
//!   groups for `N`, entry bundles of `u16`-length-prefixed entries.
//! * RFC 6962 section 2.1 (hashing, `PATH`, `SUBPROOF`) and RFC 9162 sections
//!   2.1.3.2 and 2.1.4.2 (iterative verification). The eight published RFC 6962
//!   test roots reproduce (`merkle` tests, Python generator), which checks the
//!   hashing and tree shape but not the proof algorithms against a published
//!   proof vector.
//!
//! `tlog-witness` (the `add-checkpoint` protocol) is not implemented here.
//!
//! The vectors in `tests/fixtures/transparency-v1.json` come from an independent
//! pure-Python implementation (`gen_transparency_vectors.py`) written from the same
//! recollection of the specifications, so they catch implementation bugs, not a
//! shared misreading of a standard.
pub mod assurance;
pub mod bundle;
mod encoding;
pub mod leaf;
pub mod log;
pub mod merkle;
pub mod note;
pub mod policy;
pub mod tiles;

pub use assurance::{
    assess, Assessment, ConsistencySource, Cosigner, Decision, KnownCheckpoints, Level, Limit,
    NoProofs, VerifiedCheckpoint,
};
pub use bundle::InclusionBundle;
pub use leaf::{Leaf, Tag, TagKey};
pub use log::Log;
pub use merkle::{Hash, HashSource, MemoryTree};
pub use note::{Checkpoint, KeyKind, Note, Origin, Signer, VerifierKey};
pub use policy::{OnFailure, Policy, PolicyError};
pub use tiles::{FsTileStore, MemoryTileStore, TileLog, TileStore};

/// Everything this module can refuse, one variant per rule so callers and tests can
/// tell the rules apart.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("invalid hex: {0}")]
    Hex(&'static str),
    #[error("invalid base64: {0}")]
    Base64(&'static str),

    // Tree and proofs.
    #[error("tree size {size} is outside the stored tree (at most {max})")]
    SizeOutOfRange { size: u64, max: u64 },
    #[error("leaf index {index} is outside a tree of {size} leaves")]
    IndexOutOfRange { index: u64, size: u64 },
    #[error("tree node (level {level}, index {index}) is not stored")]
    MissingNode { level: u8, index: u64 },
    #[error("the inclusion proof does not reproduce the root")]
    BadInclusionProof,
    #[error("the consistency proof does not connect the two roots")]
    BadConsistencyProof,

    // Storage.
    #[error("tile storage: {0}")]
    Storage(String),
    #[error("tile {0} is missing")]
    MissingTile(String),
    #[error("tile {0} is corrupt")]
    CorruptTile(String),
    #[error("an entry of {0} bytes does not fit an entry bundle")]
    EntryTooLarge(usize),

    // Keys, notes and checkpoints.
    #[error("malformed verifier key: {0}")]
    MalformedKey(&'static str),
    #[error("the key id in a verifier key does not match its name and key")]
    KeyIdMismatch,
    #[error("unsupported signature type {0:#04x}")]
    UnsupportedKeyType(u8),
    #[error("a key name is non-empty and has no whitespace, control characters or '+'")]
    InvalidKeyName,
    #[error("the signing key was rejected")]
    SigningKey,
    #[error("this key cannot produce that kind of signature")]
    WrongSignerKind,
    #[error("malformed note: {0}")]
    MalformedNote(&'static str),
    #[error("a second signature line from {0} with the same key id")]
    DuplicateSignature(String),
    #[error("malformed checkpoint: {0}")]
    MalformedCheckpoint(&'static str),
    #[error("the checkpoint has extension lines, which this format does not use")]
    CheckpointExtension,
    #[error("invalid origin: {0}")]
    InvalidOrigin(&'static str),
    #[error("checkpoint origin is {found}, expected {expected}")]
    WrongOrigin { expected: String, found: String },
    #[error("the checkpoint carries no signature of the log key")]
    MissingLogSignature,
    #[error("the signature of {0} does not verify")]
    BadSignature(String),

    // Leaves and bundles.
    #[error("invalid leaf: {0}")]
    InvalidLeaf(&'static str),
    #[error("invalid inclusion bundle: {0}")]
    InvalidBundle(&'static str),
    #[error("unsupported bundle version {0}")]
    BundleVersion(u64),
    #[error("the bundle is for tree size {bundle} but its checkpoint is for {checkpoint}")]
    BundleSizeMismatch { bundle: u64, checkpoint: u64 },
    #[error("the bundle's leaf is not the leaf the receipt requires")]
    LeafMismatch,

    // Policy and consistency evidence.
    #[error("invalid trust policy: {0}")]
    Policy(PolicyError),
    #[error("split view: two different roots at tree size {size}")]
    SplitView { size: u64 },
    #[error(
        "a checkpoint of size {new} is not consistent with the known checkpoint of size {known}"
    )]
    Inconsistent { known: u64, new: u64 },
}
