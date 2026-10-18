//! Assurance levels L0 to L3 (design 4.6) computed from an inclusion bundle, a
//! trust policy and the checkpoints this client already verified.
//!
//! | Level | Reached when |
//! |---|---|
//! | L0 | The caller's own check of the receipt against its pinned authority passed (not done here) and nothing below holds. Also the level of any rejected bundle. |
//! | L1 | The bundle's path folds to the root of a checkpoint carrying a valid signature of the policy's log key. |
//! | L2 | L1 and enough witness cosignatures (the policy's threshold, one per operator when so configured) are within `max_age_seconds` of `now`. |
//! | L3 | L2 and the checkpoint is consistent with the latest checkpoint the client already holds (a consistency proof verifies, in either direction, or the roots at equal size match). |
//!
//! A client with no prior checkpoint is at most L2. A contradiction with a prior
//! checkpoint (two roots at one size, or a proof that does not verify) rejects the
//! bundle outright: such a bundle is evidence of a fork or rewrite, not a mere
//! shortfall. A missing proof is only a shortfall: the level stays L2 and the
//! assessment says why.
//!
//! Verifying the receipt itself (signature, chain, pin; level L0) is the job of
//! `repository::attestation`; pass the leaf computed from that receipt as
//! `expected` to bind the two. This module never touches the network: proofs come
//! from a [`ConsistencySource`] the caller supplies.
use std::collections::{BTreeMap, HashSet};

use super::bundle::InclusionBundle;
use super::leaf::Leaf;
use super::merkle::{consistency_proof, verify_consistency, Hash, HashSource};
use super::note::{Checkpoint, Note, Origin};
use super::policy::{OnFailure, Policy};
use super::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    L0,
    L1,
    L2,
    L3,
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::L0 => "L0",
            Self::L1 => "L1",
            Self::L2 => "L2",
            Self::L3 => "L3",
        })
    }
}

/// A policy witness whose cosignature verified.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cosigner {
    pub name: String,
    pub operator: String,
    /// The witness's clock, Unix seconds.
    pub time: u64,
}

/// A checkpoint whose log signature (and every policy witness signature present)
/// verified for the policy's origin. Constructed only by
/// [`Policy::verify_checkpoint`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifiedCheckpoint {
    checkpoint: Checkpoint,
    cosigners: Vec<Cosigner>,
}

impl VerifiedCheckpoint {
    pub fn checkpoint(&self) -> &Checkpoint {
        &self.checkpoint
    }

    pub fn cosigners(&self) -> &[Cosigner] {
        &self.cosigners
    }
}

impl Policy {
    /// Strictly verify a checkpoint note: well-formed note and checkpoint, the
    /// policy's origin, the log's signature, and every cosignature line that names
    /// a policy witness. A signature line of a policy key that does not verify is an
    /// error, not an ignored line; lines of other keys are ignored.
    pub fn verify_checkpoint(&self, note: &Note) -> Result<VerifiedCheckpoint, Error> {
        let checkpoint = Checkpoint::parse(note.text())?;
        if checkpoint.origin != self.origin().as_str() {
            return Err(Error::WrongOrigin {
                expected: self.origin().as_str().to_string(),
                found: checkpoint.origin,
            });
        }
        if note.verify(self.log_key())?.is_none() {
            return Err(Error::MissingLogSignature);
        }
        let mut cosigners = Vec::new();
        for witness in self.witnesses() {
            if let Some(verified) = note.verify(&witness.key)? {
                cosigners.push(Cosigner {
                    name: witness.name.clone(),
                    operator: witness.operator.clone(),
                    time: verified.time.expect("a cosignature carries a time"),
                });
            }
        }
        Ok(VerifiedCheckpoint {
            checkpoint,
            cosigners,
        })
    }

    fn quorum(&self, cosigners: &[Cosigner], now: u64) -> usize {
        let fresh = cosigners
            .iter()
            .filter(|c| c.time.abs_diff(now) <= self.max_age_seconds());
        if self.distinct_operators() {
            fresh
                .map(|c| c.operator.as_str())
                .collect::<HashSet<_>>()
                .len()
        } else {
            fresh.count()
        }
    }
}

/// Checkpoints of one log this client already trusts: at most one root per size.
/// Record only checkpoints that reached L2 (see [`Assessment::trusted_checkpoint`]);
/// recording one only the log vouched for would let the log poison the set.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KnownCheckpoints {
    origin: String,
    by_size: BTreeMap<u64, Hash>,
}

impl KnownCheckpoints {
    pub fn new(origin: &Origin) -> Self {
        Self {
            origin: origin.as_str().to_string(),
            by_size: BTreeMap::new(),
        }
    }

    /// Remember a verified checkpoint. A different root at an already known size
    /// is a split view and is refused (and not stored).
    pub fn record(&mut self, verified: &VerifiedCheckpoint) -> Result<(), Error> {
        let checkpoint = verified.checkpoint();
        if checkpoint.origin != self.origin {
            return Err(Error::WrongOrigin {
                expected: self.origin.clone(),
                found: checkpoint.origin.clone(),
            });
        }
        match self.by_size.get(&checkpoint.size) {
            Some(root) if *root != checkpoint.root => Err(Error::SplitView {
                size: checkpoint.size,
            }),
            _ => {
                self.by_size.insert(checkpoint.size, checkpoint.root);
                Ok(())
            }
        }
    }

    /// The largest known checkpoint: `(size, root)`.
    pub fn latest(&self) -> Option<(u64, Hash)> {
        self.by_size.iter().next_back().map(|(s, r)| (*s, *r))
    }

    pub fn len(&self) -> usize {
        self.by_size.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_size.is_empty()
    }
}

/// Where consistency proofs come from (the log, a witness, a local archive). It
/// may decline; declining only caps the level.
pub trait ConsistencySource {
    fn consistency_proof(&self, first: u64, second: u64) -> Option<Vec<Hash>>;
}

impl<T: HashSource> ConsistencySource for T {
    fn consistency_proof(&self, first: u64, second: u64) -> Option<Vec<Hash>> {
        consistency_proof(self, first, second).ok()
    }
}

/// A source that has no proofs: every check beyond equal sizes stays unavailable.
pub struct NoProofs;

impl ConsistencySource for NoProofs {
    fn consistency_proof(&self, _first: u64, _second: u64) -> Option<Vec<Hash>> {
        None
    }
}

/// Why the level is not higher than it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Limit {
    /// First contact: nothing to be consistent with (bootstrap, at most L2).
    NoPriorCheckpoint,
    /// Fewer fresh cosignatures than the policy requires.
    QuorumNotMet { counted: usize, needed: usize },
    /// A prior checkpoint exists but no consistency proof was supplied.
    ConsistencyUnavailable { known_size: u64 },
}

/// The outcome of [`assess`]. `level` is always meaningful: a rejected bundle is L0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assessment {
    pub level: Level,
    /// Why the bundle was refused, if it was.
    pub rejection: Option<Error>,
    /// Why the level is not higher (empty at L3 and after a rejection).
    pub limits: Vec<Limit>,
    /// The checkpoint the bundle folded to, once it reached L1.
    pub checkpoint: Option<VerifiedCheckpoint>,
}

/// What a consumer does with an assessment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Accept,
    Warn,
    Reject,
}

impl Assessment {
    /// Accept when the level is at least `required` and nothing was rejected; else
    /// follow the policy's `on_failure` (warn or reject).
    pub fn decision(&self, required: Level, on_failure: OnFailure) -> Decision {
        if self.rejection.is_none() && self.level >= required {
            Decision::Accept
        } else if on_failure == OnFailure::Warn {
            Decision::Warn
        } else {
            Decision::Reject
        }
    }

    /// The checkpoint to add to [`KnownCheckpoints`]: only one that reached L2.
    pub fn trusted_checkpoint(&self) -> Option<&VerifiedCheckpoint> {
        if self.level >= Level::L2 {
            self.checkpoint.as_ref()
        } else {
            None
        }
    }
}

/// Compute the assurance of `bundle` under `policy`.
///
/// `expected` is the leaf the caller derived from the receipt it holds, if it can
/// (a different leaf in the bundle is rejected). `known` is what the client already
/// trusts, `proofs` supplies consistency proofs, `now` is Unix seconds.
pub fn assess(
    policy: &Policy,
    bundle: &InclusionBundle,
    expected: Option<&Leaf>,
    known: &KnownCheckpoints,
    proofs: &dyn ConsistencySource,
    now: u64,
) -> Assessment {
    match assess_inner(policy, bundle, expected, known, proofs, now) {
        Ok(assessment) => assessment,
        Err(rejection) => Assessment {
            level: Level::L0,
            rejection: Some(rejection),
            limits: Vec::new(),
            checkpoint: None,
        },
    }
}

fn assess_inner(
    policy: &Policy,
    bundle: &InclusionBundle,
    expected: Option<&Leaf>,
    known: &KnownCheckpoints,
    proofs: &dyn ConsistencySource,
    now: u64,
) -> Result<Assessment, Error> {
    let note = Note::parse(bundle.checkpoint.as_bytes())?;
    let verified = policy.verify_checkpoint(&note)?;
    let checkpoint = verified.checkpoint();
    if bundle.size != checkpoint.size {
        return Err(Error::BundleSizeMismatch {
            bundle: bundle.size,
            checkpoint: checkpoint.size,
        });
    }
    if expected.is_some_and(|leaf| *leaf != bundle.leaf) {
        return Err(Error::LeafMismatch);
    }
    bundle.verify_path(&checkpoint.root)?;
    let prior = check_known(policy, known, checkpoint, proofs)?;
    let counted = policy.quorum(verified.cosigners(), now);
    let mut level = Level::L1;
    let mut limits = Vec::new();
    if counted >= policy.threshold() {
        level = Level::L2;
    } else {
        limits.push(Limit::QuorumNotMet {
            counted,
            needed: policy.threshold(),
        });
    }
    match prior {
        Prior::None => limits.push(Limit::NoPriorCheckpoint),
        Prior::Unproven(known_size) => limits.push(Limit::ConsistencyUnavailable { known_size }),
        Prior::Consistent if level == Level::L2 => level = Level::L3,
        Prior::Consistent => {}
    }
    Ok(Assessment {
        level,
        rejection: None,
        limits,
        checkpoint: Some(verified),
    })
}

enum Prior {
    None,
    Unproven(u64),
    Consistent,
}

/// Compare a checkpoint with the latest one already known: equal sizes must have
/// equal roots; otherwise the smaller must be a verified prefix of the larger.
fn check_known(
    policy: &Policy,
    known: &KnownCheckpoints,
    checkpoint: &Checkpoint,
    proofs: &dyn ConsistencySource,
) -> Result<Prior, Error> {
    if known.origin != policy.origin().as_str() {
        return Err(Error::WrongOrigin {
            expected: policy.origin().as_str().to_string(),
            found: known.origin.clone(),
        });
    }
    if known
        .by_size
        .get(&checkpoint.size)
        .is_some_and(|root| *root != checkpoint.root)
    {
        return Err(Error::SplitView {
            size: checkpoint.size,
        });
    }
    let Some((known_size, known_root)) = known.latest() else {
        return Ok(Prior::None);
    };
    if known_size == checkpoint.size {
        return Ok(Prior::Consistent);
    }
    // The older checkpoint is the first argument whichever one the log just showed.
    let (first, second, first_root, second_root) = if known_size < checkpoint.size {
        (known_size, checkpoint.size, known_root, checkpoint.root)
    } else {
        (checkpoint.size, known_size, checkpoint.root, known_root)
    };
    match proofs.consistency_proof(first, second) {
        None => Ok(Prior::Unproven(known_size)),
        Some(proof) => verify_consistency(first, second, &first_root, &second_root, &proof)
            .map(|()| Prior::Consistent)
            .map_err(|_| Error::Inconsistent {
                known: known_size,
                new: checkpoint.size,
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_are_ordered_and_named() {
        assert!(Level::L0 < Level::L1 && Level::L1 < Level::L2 && Level::L2 < Level::L3);
        assert_eq!(Level::L3.to_string(), "L3");
    }

    #[test]
    fn decisions_follow_level_rejection_and_policy() {
        let at = |level, rejection| Assessment {
            level,
            rejection,
            limits: Vec::new(),
            checkpoint: None,
        };
        let ok = at(Level::L2, None);
        assert_eq!(ok.decision(Level::L2, OnFailure::Reject), Decision::Accept);
        assert_eq!(ok.decision(Level::L1, OnFailure::Reject), Decision::Accept);
        assert_eq!(ok.decision(Level::L3, OnFailure::Reject), Decision::Reject);
        assert_eq!(ok.decision(Level::L3, OnFailure::Warn), Decision::Warn);
        let rejected = at(Level::L0, Some(Error::LeafMismatch));
        assert_eq!(
            rejected.decision(Level::L0, OnFailure::Warn),
            Decision::Warn
        );
        assert_eq!(
            rejected.decision(Level::L0, OnFailure::Reject),
            Decision::Reject
        );
        assert!(ok.trusted_checkpoint().is_none());
    }
}
