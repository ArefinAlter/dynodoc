//! History horizon record v1 (docs/HORIZON-RECORD-V1.md, design
//! docs/DESIGN-COMMIT-PRUNING.md phases P0 and P1).
//!
//! A horizon record is metadata beside the commit graph. It names no path,
//! message, author key or document name (H10), changes no commit byte, address
//! or signature, and nothing in this module deletes, writes or signs anything.
//! This file holds the record types, canonical serialization, address, signature
//! domain, the Merkle roots over the skeleton, the frontier and the released
//! states, and the checks for invariants H1 to H10. [`policy`] is the retention
//! policy schema and loader; [`plan`] is the read-only planner.
//!
//! Not built: the schema-4 tables, the applier, the archive, signing flows, sync
//! and the shared service (design phases P2 and later). `Scope` and the signature
//! checks exist so a record can be verified; nothing here produces a signature.

use super::{hex, unhex};
use crate::shared_checkpoint::{canonical::CanonicalValue, Address};
use ring::{digest, signature};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub mod plan;
pub mod policy;
#[cfg(test)]
mod tests;

pub const RECORD_KIND: &str = "dynodoc.history-horizon";
pub const RECORD_VERSION: u8 = 1;
/// Canonical record bytes, as in the design (section 5.1).
pub const MAX_RECORD_BYTES: usize = 64 * 1024;
/// Signatures carried by one envelope; the largest `signatures.required` is the same.
pub const MAX_SIGNATURES: usize = 8;
pub const MAX_ENVELOPE_BYTES: usize = MAX_RECORD_BYTES + 4096;
/// Holds one record can list: 128 holds of at most about 330 bytes fit the limit.
pub const MAX_HOLDS: usize = 128;
pub const MAX_HOLD_REASON_BYTES: usize = 200;
/// Largest integer every JSON implementation represents exactly (2^53 - 1).
pub const MAX_JSON_INT: u64 = (1 << 53) - 1;

/// `SHA-256(ADDRESS_DOMAIN || canonical record bytes)` is the record address.
const ADDRESS_DOMAIN: &[u8] = b"dynodoc.history-horizon\0v1\0";
/// Ed25519 signs `SIGNATURE_DOMAIN || the 32 raw address bytes`. The domain differs
/// from every commit, receipt and rotation domain by design (docs section 5).
pub const SIGNATURE_DOMAIN: &[u8] = b"dynodoc.history-horizon-signature\0v1\0";
const HEADS_DOMAIN: &[u8] = b"dynodoc.horizon-heads\0v1\0";
const SKELETON_LEAF_DOMAIN: &[u8] = b"dynodoc.horizon-skeleton-leaf\0v1\0";
const FRONTIER_LEAF_DOMAIN: &[u8] = b"dynodoc.horizon-frontier-leaf\0v1\0";
const STATE_LEAF_DOMAIN: &[u8] = b"dynodoc.horizon-state-leaf\0v1\0";
/// Plan-only: the retained commit set (not a record field).
const RETAINED_LEAF_DOMAIN: &[u8] = b"dynodoc.horizon-retained-leaf\0v1\0";

/// Closed vocabulary of `evidence` entries; a record lists them sorted and unique.
pub const EVIDENCE: &[&str] = &[
    "holds",
    "merge_bases",
    "merge_inputs:bodies",
    "merge_inputs:none",
    "merge_inputs:states",
    "named_versions",
    "open_review_candidates",
    "recovery_sources",
    "reviewed_candidates:body",
    "reviewed_candidates:body+states",
    "reviewed_candidates:none",
    "topology:drop",
];

/// The ten invariants of the design (section 5.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Invariant {
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    H7,
    H8,
    H9,
    H10,
}

impl std::fmt::Display for Invariant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "H{}", *self as u8 + 1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("invalid horizon data: {0}")]
    Format(&'static str),
    #[error("horizon limit: {0}")]
    Limit(&'static str),
    #[error("horizon invariant {0} violated: {1}")]
    Violated(Invariant, String),
    #[error("horizon planning: {0}")]
    Plan(&'static str),
    #[error("horizon planning needs commit {0:?}, which is neither stored nor in a skeleton")]
    Missing(Address),
    #[error("repository: {0}")]
    Repository(String),
}

impl From<super::Error> for Error {
    fn from(value: super::Error) -> Self {
        Self::Repository(value.to_string())
    }
}
impl From<crate::shared_checkpoint::Error> for Error {
    fn from(value: crate::shared_checkpoint::Error) -> Self {
        Self::Repository(value.to_string())
    }
}
impl From<serde_json::Error> for Error {
    fn from(value: serde_json::Error) -> Self {
        // An unknown field is a path, message, key or name the record must not carry.
        if value.to_string().starts_with("unknown field") {
            Self::Violated(Invariant::H10, value.to_string())
        } else {
            Self::Format("horizon encoding")
        }
    }
}

fn violated(invariant: Invariant, detail: impl Into<String>) -> Error {
    Error::Violated(invariant, detail.into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    /// Releases object bytes of old file states; every commit body stays.
    Content,
    /// Also releases commit bodies and keeps a skeleton (or, if dropped, nothing).
    History,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRef {
    pub id: String,
    /// Policy hash, see [`policy::Policy::hash`].
    pub hash: Address,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Counts {
    pub commits_released: u64,
    pub commit_bytes_released: u64,
    pub states_released: u64,
    pub objects_released: u64,
    pub object_bytes_released: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Roots {
    pub skeleton: Address,
    pub frontier: Address,
    pub released_states: Address,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Archive {
    pub sha256: Address,
    pub bytes: u64,
    pub commits: u64,
    pub objects: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hold {
    pub commit: Address,
    /// Free text shown to project members: at most 200 bytes, no control characters.
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HorizonRecord {
    pub version: u8,
    pub kind: String,
    pub project: uuid::Uuid,
    /// Contiguous per project from 1 (H1).
    pub sequence: u64,
    /// Address of sequence `n - 1`; `null` exactly when `sequence == 1` (H1).
    pub previous: Option<Address>,
    pub created_at_ms: u64,
    pub tier: Tier,
    pub policy: PolicyRef,
    /// See [`heads_digest`].
    pub heads_digest: Address,
    pub counts: Counts,
    pub roots: Roots,
    /// Present exactly when an archive was written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<Archive>,
    /// Holds in force, sorted by `(commit, reason)`.
    pub holds: Vec<Hold>,
    pub evidence: Vec<String>,
}

fn sha256(parts: &[&[u8]]) -> [u8; 32] {
    let mut context = digest::Context::new(&digest::SHA256);
    for part in parts {
        context.update(part);
    }
    context
        .finish()
        .as_ref()
        .try_into()
        .expect("SHA-256 output")
}

pub(crate) fn raw(address: &Address) -> [u8; 32] {
    unhex(address.as_str(), 32)
        .ok()
        .and_then(|bytes| bytes.try_into().ok())
        .expect("an Address is 64 lowercase hexadecimal digits")
}

pub(crate) fn address_of(raw: &[u8; 32]) -> Address {
    Address::try_from(hex(raw)).expect("64 lowercase hexadecimal digits")
}

/// Compact canonical JSON: sorted keys, no whitespace, UTF-8 (as `Commit::bytes`).
pub(crate) fn canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, Error> {
    let value = serde_json::to_value(value)?;
    Ok(serde_json::to_vec(&CanonicalValue(&value))?)
}

fn within_json_int(value: u64) -> bool {
    value <= MAX_JSON_INT
}

fn text_ok(text: &str, limit: usize) -> bool {
    !text.is_empty() && text.len() <= limit && !text.chars().any(char::is_control)
}

/// Lowercase letters, digits, `.`, `_`, `-`; starts with a letter or digit; 1-64 bytes.
pub(crate) fn valid_policy_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"._-".contains(b))
}

impl HorizonRecord {
    /// Every rule that needs only this record. Cross-record and cross-data rules are
    /// [`validate_chain`], [`check_state`] and [`check_counters`].
    pub fn validate(&self) -> Result<(), Error> {
        if self.version != RECORD_VERSION {
            return Err(Error::Format("horizon version"));
        }
        if self.kind != RECORD_KIND {
            return Err(Error::Format("horizon kind"));
        }
        if self.project.is_nil() {
            return Err(Error::Format("horizon project"));
        }
        if self.sequence == 0 || !within_json_int(self.sequence) {
            return Err(violated(Invariant::H1, "sequence is not in 1..=2^53-1"));
        }
        if self.previous.is_some() != (self.sequence != 1) {
            return Err(violated(
                Invariant::H1,
                "previous is null exactly when sequence is 1",
            ));
        }
        let counts = &self.counts;
        let numbers = [
            self.created_at_ms,
            counts.commits_released,
            counts.commit_bytes_released,
            counts.states_released,
            counts.objects_released,
            counts.object_bytes_released,
        ];
        if !numbers.iter().all(|n| within_json_int(*n)) {
            return Err(Error::Format("horizon number exceeds 2^53-1"));
        }
        if !valid_policy_id(&self.policy.id) {
            return Err(violated(
                Invariant::H10,
                "policy id is not a plain identifier",
            ));
        }
        if (counts.commits_released == 0 && counts.commit_bytes_released != 0)
            || (counts.objects_released == 0 && counts.object_bytes_released != 0)
            || (counts.states_released == 0 && counts.objects_released != 0)
        {
            return Err(Error::Format("horizon counts are inconsistent"));
        }
        if self.tier == Tier::Content && counts.commits_released != 0 {
            return Err(Error::Format("a content horizon releases no commit"));
        }
        if let Some(archive) = &self.archive {
            if !within_json_int(archive.bytes)
                || archive.commits != counts.commits_released
                || archive.objects != counts.objects_released
            {
                return Err(Error::Format("archive does not match the released counts"));
            }
        }
        if self.holds.len() > MAX_HOLDS {
            return Err(Error::Limit("horizon holds"));
        }
        for hold in &self.holds {
            if !text_ok(&hold.reason, MAX_HOLD_REASON_BYTES) {
                return Err(violated(
                    Invariant::H10,
                    "hold reason is empty, over 200 bytes or has control characters",
                ));
            }
        }
        if !self.holds.windows(2).all(|pair| pair[0] < pair[1]) {
            return Err(Error::Format("holds are not sorted and unique"));
        }
        if !self.evidence.iter().all(|e| EVIDENCE.contains(&e.as_str())) {
            return Err(violated(
                Invariant::H10,
                "evidence outside the closed vocabulary",
            ));
        }
        if !self.evidence.windows(2).all(|pair| pair[0] < pair[1]) {
            return Err(Error::Format("evidence is not sorted and unique"));
        }
        Ok(())
    }

    /// Canonical bytes: `validate`d, at most [`MAX_RECORD_BYTES`].
    pub fn bytes(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        let bytes = canonical_bytes(self)?;
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(Error::Limit("horizon record bytes"));
        }
        Ok(bytes)
    }

    pub fn address(&self) -> Result<Address, Error> {
        Ok(address_of(&sha256(&[ADDRESS_DOMAIN, &self.bytes()?])))
    }

    /// Strict parse: closed fields, valid record, and exactly the canonical bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_RECORD_BYTES {
            return Err(Error::Limit("horizon record bytes"));
        }
        let record: Self = serde_json::from_slice(bytes)?;
        if record.bytes()? != bytes {
            return Err(Error::Format("horizon record is not in canonical form"));
        }
        Ok(record)
    }
}

/// What a signature's key stands for. The vocabulary is closed; which roles are
/// legal depends on the [`Scope`] the record is verified in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Owner,
    Authority,
    Device,
}

/// Where a horizon is verified. `Local` accepts owner and device keys, `Shared`
/// owner and authority keys (design section 5.1, H2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Local,
    Shared,
}

impl Scope {
    fn allows(self, role: Role) -> bool {
        match self {
            Self::Local => matches!(role, Role::Owner | Role::Device),
            Self::Shared => matches!(role, Role::Owner | Role::Authority),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Signature {
    pub role: Role,
    /// Ed25519 public key, 64 lowercase hexadecimal digits.
    pub key: String,
    /// Ed25519 signature, 128 lowercase hexadecimal digits.
    pub signature: String,
}

/// The exact bytes an Ed25519 key signs for a record with this address.
pub fn signing_input(address: &Address) -> Vec<u8> {
    let mut message = SIGNATURE_DOMAIN.to_vec();
    message.extend(raw(address));
    message
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope {
    pub horizon: HorizonRecord,
    /// Sorted by `key`, ascending and unique.
    pub signatures: Vec<Signature>,
}

impl Envelope {
    pub fn bytes(&self) -> Result<Vec<u8>, Error> {
        self.horizon.validate()?;
        let bytes = canonical_bytes(self)?;
        if bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(Error::Limit("horizon envelope bytes"));
        }
        Ok(bytes)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_ENVELOPE_BYTES {
            return Err(Error::Limit("horizon envelope bytes"));
        }
        let envelope: Self = serde_json::from_slice(bytes)?;
        if envelope.bytes()? != bytes {
            return Err(Error::Format("horizon envelope is not in canonical form"));
        }
        Ok(envelope)
    }

    /// H2: roles legal in `scope`, keys sorted and distinct, every signature verifies
    /// over the record address, and at least `required` distinct keys signed.
    /// Returns the record address.
    pub fn verify(&self, scope: Scope, required: u32) -> Result<Address, Error> {
        let address = self.horizon.address()?;
        if required == 0 {
            return Err(violated(
                Invariant::H2,
                "at least one signature is required",
            ));
        }
        if self.signatures.len() > MAX_SIGNATURES {
            return Err(Error::Limit("horizon signatures"));
        }
        let message = signing_input(&address);
        let mut previous: Option<&str> = None;
        for entry in &self.signatures {
            if !scope.allows(entry.role) {
                return Err(violated(Invariant::H2, "signature role is not legal here"));
            }
            if previous.is_some_and(|key| key >= entry.key.as_str()) {
                return Err(violated(
                    Invariant::H2,
                    "signatures are not sorted by distinct key",
                ));
            }
            previous = Some(&entry.key);
            let key = unhex(&entry.key, 32).map_err(|_| violated(Invariant::H2, "key encoding"))?;
            let sig = unhex(&entry.signature, 64)
                .map_err(|_| violated(Invariant::H2, "signature encoding"))?;
            signature::UnparsedPublicKey::new(&signature::ED25519, key)
                .verify(&message, &sig)
                .map_err(|_| violated(Invariant::H2, "signature does not verify"))?;
        }
        if (self.signatures.len() as u64) < u64::from(required) {
            return Err(violated(
                Invariant::H2,
                format!(
                    "{} distinct signatures where {required} are required",
                    self.signatures.len()
                ),
            ));
        }
        Ok(address)
    }
}

/// H1 and H2 over a project's chain, oldest first. `required` returns the number of
/// signatures the policy named by a record demands, or `None` if that policy is not
/// known, which is a violation: a horizon is not trusted under an unseen policy.
pub fn validate_chain(
    chain: &[Envelope],
    scope: Scope,
    required: impl Fn(&PolicyRef) -> Option<u32>,
) -> Result<(), Error> {
    let mut previous: Option<Address> = None;
    for (index, envelope) in chain.iter().enumerate() {
        let record = &envelope.horizon;
        if record.sequence != index as u64 + 1 {
            return Err(violated(
                Invariant::H1,
                format!("sequence {} at position {}", record.sequence, index + 1),
            ));
        }
        if record.previous != previous {
            return Err(violated(
                Invariant::H1,
                "previous does not name the preceding record",
            ));
        }
        if index > 0 && record.project != chain[0].horizon.project {
            return Err(violated(Invariant::H1, "records of different projects"));
        }
        let needed = required(&record.policy)
            .ok_or_else(|| violated(Invariant::H2, "the record's policy is not known"))?;
        previous = Some(envelope.verify(scope, needed)?);
    }
    Ok(())
}

// ---- Merkle roots (RFC 6962 section 2.1) -------------------------------------

fn leaf_hash(data: &[&[u8]]) -> [u8; 32] {
    let mut parts: Vec<&[u8]> = vec![&[0x00]];
    parts.extend_from_slice(data);
    sha256(&parts)
}

/// RFC 6962 `MTH`: the empty tree hashes the empty string.
fn merkle_root(hashes: &[[u8; 32]]) -> [u8; 32] {
    match hashes.len() {
        0 => sha256(&[]),
        1 => hashes[0],
        n => {
            // Largest power of two strictly below n.
            let split = 1usize << (usize::BITS - 1 - (n - 1).leading_zeros());
            sha256(&[
                &[0x01],
                &merkle_root(&hashes[..split]),
                &merkle_root(&hashes[split..]),
            ])
        }
    }
}

/// A commit released into a skeleton: address, ordered parents and height.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkeletonEntry {
    pub address: Address,
    /// At most two, in commit order (the first parent is significant).
    pub parents: Vec<Address>,
    pub height: u64,
}

/// An edge from a retained commit to a parent that this horizon released.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrontierEdge {
    pub child: Address,
    pub parent: Address,
    pub parent_height: u64,
}

/// One typed document state: stable document identity and state root.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateKey {
    pub document: uuid::Uuid,
    pub state_root: Address,
}

fn root_of<K: Ord, T>(
    items: &[T],
    key: impl Fn(&T) -> K,
    leaf: impl Fn(&T) -> Result<[u8; 32], Error>,
) -> Result<Address, Error> {
    let mut order: Vec<&T> = items.iter().collect();
    order.sort_by_key(|item| key(item));
    if order.windows(2).any(|pair| key(pair[0]) == key(pair[1])) {
        return Err(Error::Format("duplicate leaf key"));
    }
    let hashes = order.into_iter().map(leaf).collect::<Result<Vec<_>, _>>()?;
    Ok(address_of(&merkle_root(&hashes)))
}

/// Leaf data: domain, address, parent count (1 byte), parents in order, height
/// (u64 big-endian). Leaves sort by address. Input order does not matter.
pub fn skeleton_root(entries: &[SkeletonEntry]) -> Result<Address, Error> {
    root_of(
        entries,
        |e| e.address.clone(),
        |e| {
            if e.parents.len() > 2 || !within_json_int(e.height) {
                return Err(Error::Format("skeleton entry"));
            }
            let parents: Vec<u8> = e.parents.iter().flat_map(raw).collect();
            Ok(leaf_hash(&[
                SKELETON_LEAF_DOMAIN,
                &raw(&e.address),
                &[e.parents.len() as u8],
                &parents,
                &e.height.to_be_bytes(),
            ]))
        },
    )
}

/// Leaf data: domain, child, parent, parent height (u64 big-endian); sorted by
/// `(child, parent)`.
pub fn frontier_root(edges: &[FrontierEdge]) -> Result<Address, Error> {
    root_of(
        edges,
        |e| (e.child.clone(), e.parent.clone()),
        |e| {
            if !within_json_int(e.parent_height) {
                return Err(Error::Format("frontier edge"));
            }
            Ok(leaf_hash(&[
                FRONTIER_LEAF_DOMAIN,
                &raw(&e.child),
                &raw(&e.parent),
                &e.parent_height.to_be_bytes(),
            ]))
        },
    )
}

/// Leaf data: domain, document UUID (16 raw bytes), state root; sorted by that pair.
pub fn released_states_root(states: &[StateKey]) -> Result<Address, Error> {
    root_of(
        states,
        |s| (s.document, s.state_root.clone()),
        |s| {
            Ok(leaf_hash(&[
                STATE_LEAF_DOMAIN,
                s.document.as_bytes(),
                &raw(&s.state_root),
            ]))
        },
    )
}

/// Plan-only root over the retained commit addresses (domain, address; sorted).
pub fn retained_root(commits: &[Address]) -> Result<Address, Error> {
    root_of(
        commits,
        |a| a.clone(),
        |a| Ok(leaf_hash(&[RETAINED_LEAF_DOMAIN, &raw(a)])),
    )
}

/// One branch head as the world a plan was computed against.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HeadRef {
    pub branch: String,
    pub commit: Address,
    pub generation: i64,
}

/// `SHA-256("dynodoc.horizon-heads\0v1\0" || lines)` where each line is
/// `branch NUL commit-hex NUL generation-decimal LF`, ordered by branch bytes.
pub fn heads_digest(heads: &[HeadRef]) -> Result<Address, Error> {
    let mut ordered: Vec<&HeadRef> = heads.iter().collect();
    ordered.sort_by(|a, b| a.branch.cmp(&b.branch));
    if ordered
        .windows(2)
        .any(|pair| pair[0].branch == pair[1].branch)
    {
        return Err(Error::Format("duplicate branch"));
    }
    let mut lines = Vec::new();
    for head in ordered {
        if !super::valid_branch(&head.branch) || head.generation <= 0 {
            return Err(Error::Format("branch head"));
        }
        lines.extend(head.branch.as_bytes());
        lines.push(0);
        lines.extend(head.commit.as_str().as_bytes());
        lines.push(0);
        lines.extend(head.generation.to_string().as_bytes());
        lines.push(b'\n');
    }
    Ok(address_of(&sha256(&[HEADS_DOMAIN, &lines])))
}

// ---- Invariants over data (H3 to H9) ----------------------------------------

/// A commit whose body is still stored, as the checks need it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Retained {
    pub address: Address,
    pub parents: Vec<Address>,
    pub height: u64,
    /// States of its file entries, for H6 and H8.
    pub states: Vec<StateKey>,
}

/// One horizon with the rows it introduced.
#[derive(Clone, Copy, Debug)]
pub struct HorizonRows<'a> {
    pub record: &'a HorizonRecord,
    pub skeleton: &'a [SkeletonEntry],
    pub frontier: &'a [FrontierEdge],
    pub released_states: &'a [StateKey],
}

/// H3 to H8 for the current repository: `horizons` oldest first, `heads` the
/// branches now, `present` the states whose objects are all stored.
pub fn check_state(
    retained: &[Retained],
    horizons: &[HorizonRows<'_>],
    heads: &[HeadRef],
    present: &BTreeSet<StateKey>,
) -> Result<(), Error> {
    let mut bodies: BTreeMap<&Address, &Retained> = BTreeMap::new();
    for commit in retained {
        if bodies.insert(&commit.address, commit).is_some() {
            return Err(violated(
                Invariant::H7,
                format!("{} listed twice", commit.address.as_str()),
            ));
        }
    }
    // H7: a skeleton address appears once across horizons and never as a body.
    let mut skeleton: BTreeMap<&Address, &SkeletonEntry> = BTreeMap::new();
    for rows in horizons {
        for entry in rows.skeleton {
            if bodies.contains_key(&entry.address)
                || skeleton.insert(&entry.address, entry).is_some()
            {
                return Err(violated(
                    Invariant::H7,
                    format!(
                        "{} is both retained and released, or released twice",
                        entry.address.as_str()
                    ),
                ));
            }
        }
    }
    // H5: the rows recompute the committed roots (and as many rows as were counted).
    for rows in horizons {
        let record = rows.record;
        let dropped = record.evidence.iter().any(|e| e == "topology:drop");
        let counted = if dropped {
            0
        } else {
            record.counts.commits_released
        };
        let matches = skeleton_root(rows.skeleton)? == record.roots.skeleton
            && frontier_root(rows.frontier)? == record.roots.frontier
            && released_states_root(rows.released_states)? == record.roots.released_states
            && rows.skeleton.len() as u64 == counted
            && rows.released_states.len() as u64 == record.counts.states_released;
        if !matches {
            return Err(violated(
                Invariant::H5,
                format!(
                    "rows of sequence {} do not reproduce its roots",
                    record.sequence
                ),
            ));
        }
    }
    // H3: no silent holes below a retained commit.
    for commit in retained {
        for parent in &commit.parents {
            if !bodies.contains_key(parent) && !skeleton.contains_key(parent) {
                return Err(violated(
                    Invariant::H3,
                    format!(
                        "parent {} of retained {} is neither retained nor released",
                        parent.as_str(),
                        commit.address.as_str()
                    ),
                ));
            }
        }
    }
    // H4: skeleton closure and heights.
    let height = |address: &Address| {
        bodies
            .get(address)
            .map(|c| c.height)
            .or_else(|| skeleton.get(address).map(|e| e.height))
    };
    for entry in skeleton.values() {
        let mut expected = 0;
        for parent in &entry.parents {
            let parent_height = height(parent).ok_or_else(|| {
                violated(
                    Invariant::H4,
                    format!(
                        "skeleton parent {} of {} is unknown",
                        parent.as_str(),
                        entry.address.as_str()
                    ),
                )
            })?;
            expected = expected.max(parent_height.saturating_add(1));
        }
        if entry.parents.len() > 2 || entry.height != expected {
            return Err(violated(
                Invariant::H4,
                format!(
                    "height of {} is not one above its parents",
                    entry.address.as_str()
                ),
            ));
        }
    }
    for rows in horizons {
        for edge in rows.frontier {
            let consistent = bodies
                .get(&edge.child)
                .is_some_and(|c| c.parents.contains(&edge.parent))
                && skeleton
                    .get(&edge.parent)
                    .is_some_and(|e| e.height == edge.parent_height);
            if !consistent {
                return Err(violated(
                    Invariant::H4,
                    format!(
                        "frontier edge {} -> {} matches no rows",
                        edge.child.as_str(),
                        edge.parent.as_str()
                    ),
                ));
            }
        }
    }
    // H6: no current head released, in body or in states.
    let released: BTreeSet<&StateKey> = horizons.iter().flat_map(|h| h.released_states).collect();
    for head in heads {
        let commit = bodies.get(&head.commit).ok_or_else(|| {
            violated(
                Invariant::H6,
                format!(
                    "head {} of {} is not retained",
                    head.commit.as_str(),
                    head.branch
                ),
            )
        })?;
        if commit.states.iter().any(|s| released.contains(s)) {
            return Err(violated(
                Invariant::H6,
                format!("a state of the head of {} is released", head.branch),
            ));
        }
    }
    // H8: a released state is not present; any other state of a retained commit is.
    for state in &released {
        if present.contains(*state) {
            return Err(violated(
                Invariant::H8,
                "a released state still has its objects",
            ));
        }
    }
    for commit in retained {
        for state in &commit.states {
            if !present.contains(state) && !released.contains(state) {
                return Err(violated(
                    Invariant::H8,
                    format!(
                        "state of retained {} is missing and not listed as released",
                        commit.address.as_str()
                    ),
                ));
            }
        }
    }
    Ok(())
}

/// Capacity counters of a local repository (commits, objects, logical bytes).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub commits: u64,
    pub objects: u64,
    pub bytes: u64,
}

/// H9: after an apply the recorded counters equal a fresh recount of the rows.
pub fn check_counters(recorded: Usage, recount: Usage) -> Result<(), Error> {
    if recorded != recount {
        return Err(violated(
            Invariant::H9,
            format!("recorded {recorded:?} but a recount finds {recount:?}"),
        ));
    }
    Ok(())
}
