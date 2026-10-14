//! Receipt-authority epochs: signed succession statements between receipt keys.
//! A `rotation` needs both the old and the new key. A `recovery` is signed only by
//! the new key and is a *recorded break*: no client may follow it implicitly.
//! Receipts keep their own version-1 encoding; they name their epoch through the
//! signing key, and a key belongs to exactly one epoch.
use super::{
    attestation::{hash, message, verify},
    hex, unhex, Error,
};
use crate::shared_checkpoint::Address;
use ring::digest;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

const EPOCH_DOMAIN: &[u8] = b"dynodoc.ref-authority-epoch\0v1\0";
const EPOCH_SIGN_DOMAIN: &[u8] = b"dynodoc.ref-authority-epoch-signature\0v1\0";
const SEAL_DOMAIN: &[u8] = b"dynodoc.ref-authority-seal\0v1\0";
/// Hard cap on authority epochs (also a database check).
pub const MAX_EPOCH: u32 = 1024;
/// Statements per served page and per offline proof.
pub const MAX_CHAIN_PAGE: usize = 64;
pub const MAX_REASON_BYTES: usize = 200;
pub const MAX_STATEMENT_BYTES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Rotation,
    Recovery,
}
impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Rotation => "rotation",
            Kind::Recovery => "recovery",
        }
    }
}

/// Global marker: how many receipts existed when the previous epoch ended, and a
/// digest over their addresses. It is cumulative across epochs and names no scope.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seal {
    pub receipts: u64,
    pub digest: String,
}
/// Streaming seal. Feed receipt addresses ordered by project UUID bytes, branch
/// UTF-8 bytes and generation.
pub struct SealBuilder {
    context: digest::Context,
    receipts: u64,
}
impl Default for SealBuilder {
    fn default() -> Self {
        Self::new()
    }
}
impl SealBuilder {
    pub fn new() -> Self {
        let mut context = digest::Context::new(&digest::SHA256);
        context.update(SEAL_DOMAIN);
        Self {
            context,
            receipts: 0,
        }
    }
    pub fn push(&mut self, receipt_address: &str) -> Result<(), Error> {
        self.context.update(&unhex(receipt_address, 32)?);
        self.receipts = self
            .receipts
            .checked_add(1)
            .ok_or(Error::Limit("seal count"))?;
        Ok(())
    }
    pub fn finish(self) -> Seal {
        Seal {
            receipts: self.receipts,
            digest: hex(self.context.finish().as_ref()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Transition {
    pub version: u8,
    pub kind: Kind,
    /// Installation UUID; the same value every receipt scope carries.
    pub authority: Uuid,
    pub from_epoch: u32,
    pub to_epoch: u32,
    pub old_key: String,
    pub new_key: String,
    pub sealed: Seal,
    pub reason: String,
    /// Operator clock; not trusted time.
    pub declared_at_ms: i64,
}
impl Transition {
    fn validate(&self) -> Result<(), Error> {
        if self.version != 1
            || self.authority.is_nil()
            || self.from_epoch < 1
            || self.to_epoch != self.from_epoch.saturating_add(1)
            || self.to_epoch > MAX_EPOCH
            || self.old_key == self.new_key
            || self.declared_at_ms < 0
            || self.reason.len() > MAX_REASON_BYTES
            || self.reason.chars().any(char::is_control)
            || (self.kind == Kind::Recovery && self.reason.trim().is_empty())
        {
            return Err(Error::Invalid("authority epoch statement"));
        }
        unhex(&self.old_key, 32)?;
        unhex(&self.new_key, 32)?;
        unhex(&self.sealed.digest, 32)?;
        Ok(())
    }
    pub fn address(&self) -> Result<Address, Error> {
        self.validate()?;
        hash(EPOCH_DOMAIN, self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedTransition {
    pub transition: Transition,
    /// Required for a rotation, forbidden for a recovery.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_signature: Option<String>,
    pub new_signature: String,
}
impl SignedTransition {
    /// Both keys sign. `transition` must name exactly these two public keys.
    pub fn sign_rotation(
        transition: Transition,
        old: &ring::signature::Ed25519KeyPair,
        new: &ring::signature::Ed25519KeyPair,
    ) -> Result<Self, Error> {
        use ring::signature::KeyPair;
        if transition.kind != Kind::Rotation
            || transition.old_key != hex(old.public_key().as_ref())
            || transition.new_key != hex(new.public_key().as_ref())
        {
            return Err(Error::Invalid("authority rotation keys"));
        }
        let bytes = message(EPOCH_SIGN_DOMAIN, &transition.address()?)?;
        Ok(Self {
            old_signature: Some(hex(old.sign(&bytes).as_ref())),
            new_signature: hex(new.sign(&bytes).as_ref()),
            transition,
        })
    }
    /// Only the new key signs: the old key is lost or must not be trusted.
    pub fn sign_recovery(
        transition: Transition,
        new: &ring::signature::Ed25519KeyPair,
    ) -> Result<Self, Error> {
        use ring::signature::KeyPair;
        if transition.kind != Kind::Recovery || transition.new_key != hex(new.public_key().as_ref())
        {
            return Err(Error::Invalid("authority recovery key"));
        }
        let bytes = message(EPOCH_SIGN_DOMAIN, &transition.address()?)?;
        Ok(Self {
            old_signature: None,
            new_signature: hex(new.sign(&bytes).as_ref()),
            transition,
        })
    }
    /// Structure and every signature this kind requires. Says nothing about whether
    /// `old_key` is a key the caller trusts.
    pub fn verify(&self) -> Result<Address, Error> {
        let address = self.transition.address()?;
        let bytes = message(EPOCH_SIGN_DOMAIN, &address)?;
        verify(&self.transition.new_key, &self.new_signature, &bytes)?;
        match (self.transition.kind, &self.old_signature) {
            (Kind::Rotation, Some(signature)) => {
                verify(&self.transition.old_key, signature, &bytes)?
            }
            (Kind::Recovery, None) => {}
            _ => return Err(Error::Invalid("authority statement signatures")),
        }
        if serde_json::to_vec(self)?.len() > MAX_STATEMENT_BYTES {
            return Err(Error::Limit("authority statement bytes"));
        }
        Ok(address)
    }
}

/// Why a key cannot be reached from another through served statements.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathError {
    /// A key is not part of the served chain.
    UnknownKey,
    /// The target key belongs to an earlier epoch.
    Backwards,
    /// The only way forward is a recorded break; it needs an explicit re-pin.
    Break { to_epoch: u32, reason: String },
}
impl std::fmt::Display for PathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PathError::UnknownKey => f.write_str("key is not part of the authority chain"),
            PathError::Backwards => f.write_str("authority chain does not lead to that key"),
            PathError::Break { to_epoch, reason } => write!(
                f,
                "authority chain break at epoch {to_epoch} ({reason}); explicit independent re-pin required"
            ),
        }
    }
}
impl std::error::Error for PathError {}

/// A verified, contiguous run of statements. Construction checks every signature,
/// the authority UUID, epoch numbers, linkage and that no key is reused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chain {
    authority: Uuid,
    statements: Vec<SignedTransition>,
}
impl Chain {
    pub fn new(authority: Uuid, statements: Vec<SignedTransition>) -> Result<Self, Error> {
        if statements.len() >= MAX_EPOCH as usize {
            return Err(Error::Limit("authority chain"));
        }
        let mut keys = BTreeSet::new();
        for (index, statement) in statements.iter().enumerate() {
            statement.verify()?;
            let t = &statement.transition;
            if t.authority != authority {
                return Err(Error::Invalid("authority chain identity"));
            }
            if index == 0 {
                keys.insert(t.old_key.clone());
            } else {
                let previous = &statements[index - 1].transition;
                if t.from_epoch != previous.to_epoch || t.old_key != previous.new_key {
                    return Err(Error::Invalid("authority chain linkage"));
                }
            }
            if !keys.insert(t.new_key.clone()) {
                return Err(Error::Invalid("authority key reused"));
            }
        }
        Ok(Self {
            authority,
            statements,
        })
    }
    pub fn authority(&self) -> Uuid {
        self.authority
    }
    pub fn statements(&self) -> &[SignedTransition] {
        &self.statements
    }
    /// Epoch number of a key named by this chain.
    pub fn epoch_of(&self, key: &str) -> Option<u32> {
        self.statements.iter().find_map(|s| {
            if s.transition.old_key == key {
                Some(s.transition.from_epoch)
            } else if s.transition.new_key == key {
                Some(s.transition.to_epoch)
            } else {
                None
            }
        })
    }
    /// Statements that lead from `from_key` to `to_key`. Any recovery statement on
    /// the way is a break and is reported, never skipped.
    pub fn path(&self, from_key: &str, to_key: &str) -> Result<&[SignedTransition], PathError> {
        let from = self.epoch_of(from_key).ok_or(PathError::UnknownKey)?;
        let to = self.epoch_of(to_key).ok_or(PathError::UnknownKey)?;
        if to < from {
            return Err(PathError::Backwards);
        }
        let start = self
            .statements
            .iter()
            .position(|s| s.transition.from_epoch == from)
            .unwrap_or(self.statements.len());
        let end = self
            .statements
            .iter()
            .position(|s| s.transition.from_epoch == to)
            .unwrap_or(self.statements.len());
        let path = &self.statements[start..end.max(start)];
        if let Some(broken) = path.iter().find(|s| s.transition.kind == Kind::Recovery) {
            return Err(PathError::Break {
                to_epoch: broken.transition.to_epoch,
                reason: broken.transition.reason.clone(),
            });
        }
        Ok(path)
    }
}

/// Unsigned operator note that an epoch's key must not be trusted. Never edited.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Revocation {
    pub epoch: u32,
    /// Best-known time the key stopped being safe (operator clock).
    pub effective_at_ms: i64,
    pub recorded_at_ms: i64,
    pub reason: String,
}

/// Why a client must not follow the served chain. Always fail closed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The key is not part of the served chain.
    UnknownKey,
    /// The pinned key is newer than the key being checked.
    Backwards,
    /// A recorded break lies between the keys.
    Break { to_epoch: u32, reason: String },
    /// An epoch from the trusted key onward carries a revocation marker.
    /// `own` is true when it is the trusted key's own epoch.
    Revoked { marker: Revocation, own: bool },
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::UnknownKey => f.write_str("authority key is not part of the server's epoch chain"),
            Refusal::Backwards => f.write_str("authority chain does not lead to that key"),
            Refusal::Break { to_epoch, reason } => write!(
                f,
                "authority chain break at epoch {to_epoch} ({reason}); verify the new key out of band and re-pin explicitly (receipt accept-break)"
            ),
            Refusal::Revoked { marker, own } => write!(
                f,
                "authority epoch {} was revoked ({}); {}",
                marker.epoch,
                marker.reason,
                if *own {
                    "this pin accepts nothing further until an explicit re-pin (receipt accept-break) or a new journal from an independently obtained key"
                } else {
                    "a later epoch cannot be trusted; re-pin from an independently obtained key"
                }
            ),
        }
    }
}
impl std::error::Error for Refusal {}

/// What a server claims about its authority: the verified statements, the active
/// key and the revocation markers. Nothing here is trusted until [`Served::vet`]
/// connects it to a key the client already trusts.
#[derive(Clone, Debug)]
pub struct Served {
    pub chain: Chain,
    pub active_key: String,
    pub active_epoch: u32,
    pub revoked: BTreeMap<u32, Revocation>,
}
impl Served {
    /// Consistency of what was served; fails when statements and the active key
    /// or epoch disagree.
    pub fn new(
        chain: Chain,
        active_key: String,
        active_epoch: u32,
        revoked: BTreeMap<u32, Revocation>,
    ) -> Result<Self, Error> {
        unhex(&active_key, 32)?;
        match chain.statements().last() {
            Some(last)
                if last.transition.new_key != active_key
                    || last.transition.to_epoch != active_epoch =>
            {
                return Err(Error::Invalid("served active epoch differs from its chain"));
            }
            None if active_epoch != 1 => {
                return Err(Error::Invalid("served epoch lacks its statements"));
            }
            _ => {}
        }
        if revoked
            .iter()
            .any(|(epoch, m)| *epoch != m.epoch || *epoch > active_epoch)
        {
            return Err(Error::Invalid("served revocation marker"));
        }
        Ok(Self {
            chain,
            active_key,
            active_epoch,
            revoked,
        })
    }
    pub fn epoch_of(&self, key: &str) -> Option<u32> {
        if key == self.active_key {
            Some(self.active_epoch)
        } else {
            self.chain.epoch_of(key)
        }
    }
    /// A key the server's chain names. Says nothing about trust.
    pub fn knows(&self, key: &str) -> bool {
        self.epoch_of(key).is_some()
    }
    /// May a client that trusts `from` accept receipts signed by `to`? Requires
    /// dual-signed statements the whole way, no recorded break, and no revocation
    /// marker on any epoch from `from` up to the active epoch.
    pub fn vet(&self, from: &str, to: &str) -> Result<(), Refusal> {
        let from_epoch = self.epoch_of(from).ok_or(Refusal::UnknownKey)?;
        let to_epoch = self.epoch_of(to).ok_or(Refusal::UnknownKey)?;
        if to_epoch < from_epoch {
            return Err(Refusal::Backwards);
        }
        if let Some((epoch, marker)) = self.revoked.range(from_epoch..).next() {
            return Err(Refusal::Revoked {
                marker: marker.clone(),
                own: *epoch == from_epoch,
            });
        }
        if from_epoch < to_epoch {
            self.chain.path(from, to).map_err(|e| match e {
                PathError::UnknownKey => Refusal::UnknownKey,
                PathError::Backwards => Refusal::Backwards,
                PathError::Break { to_epoch, reason } => Refusal::Break { to_epoch, reason },
            })?;
        }
        Ok(())
    }
    /// Two keys a client may hold at once (an old configuration and a newer
    /// pin) must not be separated by a recorded break in either direction.
    /// Revocation markers are judged separately by [`Served::vet`].
    pub fn related(&self, a: &str, b: &str) -> Result<(), Refusal> {
        let (a_epoch, b_epoch) = (
            self.epoch_of(a).ok_or(Refusal::UnknownKey)?,
            self.epoch_of(b).ok_or(Refusal::UnknownKey)?,
        );
        let (low, high) = if a_epoch <= b_epoch { (a, b) } else { (b, a) };
        if a_epoch != b_epoch {
            self.chain.path(low, high).map_err(|e| match e {
                PathError::UnknownKey => Refusal::UnknownKey,
                PathError::Backwards => Refusal::Backwards,
                PathError::Break { to_epoch, reason } => Refusal::Break { to_epoch, reason },
            })?;
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::repository::attestation::tests::key;
    use ring::signature::KeyPair;

    fn public(seed: u8) -> String {
        hex(key(seed).public_key().as_ref())
    }
    pub(crate) fn rotation(from: u32, old: u8, new: u8) -> SignedTransition {
        SignedTransition::sign_rotation(
            Transition {
                version: 1,
                kind: Kind::Rotation,
                authority: Uuid::from_u128(1),
                from_epoch: from,
                to_epoch: from + 1,
                old_key: public(old),
                new_key: public(new),
                sealed: SealBuilder::new().finish(),
                reason: String::new(),
                declared_at_ms: 1_700_000_000_000,
            },
            &key(old),
            &key(new),
        )
        .unwrap()
    }
    pub(crate) fn recovery(from: u32, old: u8, new: u8) -> SignedTransition {
        SignedTransition::sign_recovery(
            Transition {
                version: 1,
                kind: Kind::Recovery,
                authority: Uuid::from_u128(1),
                from_epoch: from,
                to_epoch: from + 1,
                old_key: public(old),
                new_key: public(new),
                sealed: SealBuilder::new().finish(),
                reason: "private key lost".into(),
                declared_at_ms: 1_700_000_000_000,
            },
            &key(new),
        )
        .unwrap()
    }
    #[test]
    fn statements_match_independent_openssl_vectors() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/ref-authority-epoch-v1.json"
        ))
        .unwrap();
        for name in ["rotation_1_2", "rotation_2_3", "recovery_3_4"] {
            let case = &fixture[name];
            let signed: SignedTransition = serde_json::from_value(case["signed"].clone()).unwrap();
            assert_eq!(
                signed.verify().unwrap().as_str(),
                case["address"].as_str().unwrap(),
                "{name}"
            );
            let value = serde_json::to_value(&signed.transition).unwrap();
            assert_eq!(
                serde_json::to_string(&crate::shared_checkpoint::canonical::CanonicalValue(&value))
                    .unwrap(),
                case["canonical"].as_str().unwrap(),
                "{name}"
            );
        }
        // The implementation signs exactly the independently generated bytes.
        let first: SignedTransition =
            serde_json::from_value(fixture["rotation_1_2"]["signed"].clone()).unwrap();
        assert_eq!(
            SignedTransition::sign_rotation(first.transition.clone(), &key(7), &key(8)).unwrap(),
            first
        );
        let last: SignedTransition =
            serde_json::from_value(fixture["recovery_3_4"]["signed"].clone()).unwrap();
        assert_eq!(
            SignedTransition::sign_recovery(last.transition.clone(), &key(10)).unwrap(),
            last
        );
        let mut seal = SealBuilder::new();
        for address in fixture["seal"]["addresses"].as_array().unwrap() {
            seal.push(address.as_str().unwrap()).unwrap();
        }
        let seal = seal.finish();
        assert_eq!(seal.receipts, fixture["seal"]["receipts"].as_u64().unwrap());
        assert_eq!(seal.digest, fixture["seal"]["digest"].as_str().unwrap());
        assert_eq!(
            SealBuilder::new().finish().digest,
            fixture["seal"]["empty_digest"].as_str().unwrap()
        );
    }
    #[test]
    fn every_signed_field_and_signature_is_bound() {
        let good = rotation(1, 7, 8);
        good.verify().unwrap();
        let original = serde_json::to_value(&good).unwrap();
        for (path, replacement) in [
            (
                "/transition/authority",
                serde_json::json!(Uuid::from_u128(9)),
            ),
            ("/transition/from_epoch", serde_json::json!(2)),
            ("/transition/to_epoch", serde_json::json!(3)),
            ("/transition/old_key", serde_json::json!(public(9))),
            ("/transition/new_key", serde_json::json!(public(9))),
            ("/transition/sealed/receipts", serde_json::json!(1)),
            (
                "/transition/sealed/digest",
                serde_json::json!("00".repeat(32)),
            ),
            ("/transition/reason", serde_json::json!("changed")),
            ("/transition/declared_at_ms", serde_json::json!(1)),
            ("/transition/kind", serde_json::json!("recovery")),
            ("/old_signature", serde_json::json!("00".repeat(64))),
            ("/new_signature", serde_json::json!("00".repeat(64))),
        ] {
            let mut changed = original.clone();
            *changed.pointer_mut(path).unwrap() = replacement;
            let changed: SignedTransition = serde_json::from_value(changed).unwrap();
            assert!(changed.verify().is_err(), "{path}");
        }
        let mut unknown = original.clone();
        unknown["transition"]["extra"] = serde_json::json!(1);
        assert!(serde_json::from_value::<SignedTransition>(unknown).is_err());
        // Wrong signing keys cannot produce a statement.
        assert!(
            SignedTransition::sign_rotation(good.transition.clone(), &key(9), &key(8)).is_err()
        );
        assert!(
            SignedTransition::sign_rotation(good.transition.clone(), &key(7), &key(9)).is_err()
        );
        // A recovery carries no old signature and a rotation cannot omit it.
        let mut rotation_without_old = good.clone();
        rotation_without_old.old_signature = None;
        assert!(rotation_without_old.verify().is_err());
        let mut recovery_with_old = recovery(1, 7, 8);
        recovery_with_old.old_signature = good.old_signature.clone();
        assert!(recovery_with_old.verify().is_err());
        // A recovery needs a reason; both kinds refuse an overlong or control reason.
        let mut silent = recovery(1, 7, 8).transition;
        silent.reason.clear();
        assert!(SignedTransition::sign_recovery(silent, &key(8)).is_err());
        let mut loud = rotation(1, 7, 8).transition;
        loud.reason = "x".repeat(MAX_REASON_BYTES + 1);
        assert!(SignedTransition::sign_rotation(loud, &key(7), &key(8)).is_err());
        let mut ctrl = rotation(1, 7, 8).transition;
        ctrl.reason = "line\nbreak".into();
        assert!(SignedTransition::sign_rotation(ctrl, &key(7), &key(8)).is_err());
    }
    #[test]
    fn chain_checks_linkage_identity_and_reports_breaks() {
        let authority = Uuid::from_u128(1);
        let chain = Chain::new(
            authority,
            vec![rotation(1, 7, 8), rotation(2, 8, 9), recovery(3, 9, 10)],
        )
        .unwrap();
        assert_eq!(chain.epoch_of(&public(7)), Some(1));
        assert_eq!(chain.epoch_of(&public(10)), Some(4));
        assert_eq!(chain.epoch_of(&public(11)), None);
        assert_eq!(chain.path(&public(7), &public(7)).unwrap().len(), 0);
        assert_eq!(chain.path(&public(7), &public(9)).unwrap().len(), 2);
        assert_eq!(chain.path(&public(8), &public(9)).unwrap().len(), 1);
        assert_eq!(
            chain.path(&public(9), &public(7)),
            Err(PathError::Backwards)
        );
        assert_eq!(
            chain.path(&public(7), &public(11)),
            Err(PathError::UnknownKey)
        );
        assert!(matches!(
            chain.path(&public(7), &public(10)),
            Err(PathError::Break { to_epoch: 4, .. })
        ));
        assert!(matches!(
            chain.path(&public(9), &public(10)),
            Err(PathError::Break { to_epoch: 4, .. })
        ));
        assert!(Chain::new(Uuid::from_u128(2), vec![rotation(1, 7, 8)]).is_err());
        // Gap, reorder, mismatched key and key reuse.
        assert!(Chain::new(authority, vec![rotation(1, 7, 8), rotation(3, 9, 10)]).is_err());
        assert!(Chain::new(authority, vec![rotation(2, 8, 9), rotation(1, 7, 8)]).is_err());
        assert!(Chain::new(authority, vec![rotation(1, 7, 8), rotation(2, 11, 9)]).is_err());
        assert!(Chain::new(authority, vec![rotation(1, 7, 8), rotation(2, 8, 7)]).is_err());
    }
    #[test]
    fn pins_follow_dual_signed_chains_and_refuse_breaks_until_explicitly_repinned() {
        use crate::repository::attestation::{
            tests::{scope, signed},
            Pin,
        };
        let scope = scope(Uuid::from_u128(2));
        let authority = scope.authority;
        let a = signed(scope.clone(), 1, None, 7);
        let pin = Pin::trust_checkpoint(&scope, &public(7), a.clone()).unwrap();
        let chain = Chain::new(
            authority,
            vec![rotation(1, 7, 8), rotation(2, 8, 9), recovery(3, 9, 10)],
        )
        .unwrap();
        let b = signed(scope.clone(), 2, Some(a.verify().unwrap()), 7);
        let c = signed(scope.clone(), 3, Some(b.verify().unwrap()), 8);
        let d = signed(scope.clone(), 4, Some(c.verify().unwrap()), 9);
        let proof = [b.clone(), c.clone(), d.clone()];
        let (pinned, used) = pin.advance_chain(&chain, &proof).unwrap();
        assert_eq!(used.len(), 2);
        assert_eq!(pinned.trusted_key(), public(9));
        assert_eq!(pinned.receipt(), &d);
        // A lost acknowledgement retry, even with the older prefix, is a no-op.
        let (again, none) = pinned
            .advance_chain(&chain, &[a.clone(), b.clone(), c.clone(), d.clone()])
            .unwrap();
        assert_eq!((&again, none.len()), (&pinned, 0));
        // Skipped generation, wrong order, old-key receipt after rotation and an
        // unrelated key are refused.
        assert!(pin.advance_chain(&chain, &[c.clone(), d.clone()]).is_err());
        assert!(pin.advance_chain(&chain, &[d.clone(), c.clone()]).is_err());
        let stale = signed(scope.clone(), 5, Some(d.verify().unwrap()), 7);
        assert!(pinned.advance_chain(&chain, &[stale]).is_err());
        let stranger = signed(scope.clone(), 5, Some(d.verify().unwrap()), 12);
        assert!(pinned.advance_chain(&chain, &[stranger]).is_err());
        // The recorded break is never crossed implicitly.
        let e = signed(scope.clone(), 5, Some(d.verify().unwrap()), 10);
        assert!(pinned
            .advance_chain(&chain, std::slice::from_ref(&e))
            .is_err());
        let statement = recovery(3, 9, 10);
        assert!(pinned
            .rotate_authority(std::slice::from_ref(&statement), &e)
            .is_err());
        // Explicit re-pin: independently supplied key, matching statement, extending receipt.
        assert!(pinned.accept_break(&statement, &public(11), &e).is_err());
        assert!(pinned
            .accept_break(&rotation(3, 9, 10), &public(10), &e)
            .is_err());
        assert!(pin.accept_break(&statement, &public(10), &e).is_err());
        let detached = signed(scope.clone(), 7, Some(d.verify().unwrap()), 10);
        assert!(pinned
            .accept_break(&statement, &public(10), &detached)
            .is_err());
        let repinned = pinned.accept_break(&statement, &public(10), &e).unwrap();
        assert_eq!(repinned.trusted_key(), public(10));
        let old_key = signed(scope.clone(), 6, Some(e.verify().unwrap()), 9);
        assert!(repinned.advance(&[old_key]).is_err());
        // A revoked pin accepts nothing implicitly. The explicit re-pin still works,
        // because it needs the independently known key and an extending receipt.
        let mut revoked = pin.clone();
        revoked.revoke();
        assert!(revoked.advance_chain(&chain, &[]).is_err());
        assert!(revoked.accept_break(&statement, &public(10), &e).is_err());
        let mut revoked_at_break = pinned.clone();
        revoked_at_break.revoke();
        assert!(revoked_at_break.advance(std::slice::from_ref(&e)).is_err());
        let healed = revoked_at_break
            .accept_break(&statement, &public(10), &e)
            .unwrap();
        assert!(!healed.revoked());
        assert_eq!(healed.trusted_key(), public(10));
        let foreign = Chain::new(Uuid::from_u128(77), vec![]).unwrap();
        assert!(pin.advance_chain(&foreign, &[]).is_err());
    }
}
