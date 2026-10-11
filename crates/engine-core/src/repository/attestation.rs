//! Signed ref receipts and client trust state. A signature proves key control;
//! callers establish the authority key through an independent trusted channel.
//! These primitives do not issue account grants or operate an external witness.
use super::{hex, unhex, Error, Head};
use crate::shared_checkpoint::{canonical::CanonicalValue, Address};
use ring::{
    digest,
    signature::{self, Ed25519KeyPair, KeyPair},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const RECEIPT_DOMAIN: &[u8] = b"dynodoc.ref-receipt\0v1\0";
const SIGN_DOMAIN: &[u8] = b"dynodoc.ref-receipt-signature\0v1\0";
const ROTATION_DOMAIN: &[u8] = b"dynodoc.ref-authority-rotation\0v1\0";
const ROTATION_SIGN_DOMAIN: &[u8] = b"dynodoc.ref-authority-rotation-signature\0v1\0";
pub const MAX_PROOF_RECEIPTS: usize = 256;
pub const MAX_RECEIPT_BYTES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scope {
    /// Stable authority/server identity, explicitly pinned by the client.
    pub authority: Uuid,
    pub project: Uuid,
    pub branch: String,
}
impl Scope {
    fn validate(&self) -> Result<(), Error> {
        if self.authority.is_nil() || self.project.is_nil() || !super::valid_branch(&self.branch) {
            return Err(Error::Invalid("receipt scope"));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub version: u8,
    pub scope: Scope,
    pub head: Head,
    pub previous: Option<Address>,
    /// Declared time, not a trusted timestamp or proof of signing time.
    pub issued_at_ms: i64,
    pub issuer_key: String,
}
fn hash<T: Serialize>(domain: &[u8], value: &T) -> Result<Address, Error> {
    let value = serde_json::to_value(value)?;
    let bytes = serde_json::to_vec(&CanonicalValue(&value))?;
    if bytes.len() > MAX_RECEIPT_BYTES {
        return Err(Error::Limit("receipt bytes"));
    }
    let mut hash = digest::Context::new(&digest::SHA256);
    hash.update(domain);
    hash.update(&bytes);
    Ok(Address::try_from(hex(hash.finish().as_ref()))?)
}
fn message(domain: &[u8], address: &Address) -> Result<Vec<u8>, Error> {
    let mut message = domain.to_vec();
    message.extend(unhex(address.as_str(), 32)?);
    Ok(message)
}
fn verify(key: &str, signature: &str, message: &[u8]) -> Result<(), Error> {
    signature::UnparsedPublicKey::new(&signature::ED25519, unhex(key, 32)?)
        .verify(message, &unhex(signature, 64)?)
        .map_err(|_| Error::Invalid("receipt signature"))
}
impl Receipt {
    pub fn address(&self) -> Result<Address, Error> {
        self.scope.validate()?;
        if self.version != 1
            || self.head.generation < 1
            || self.issued_at_ms < 0
            || (self.head.generation == 1) != self.previous.is_none()
        {
            return Err(Error::Invalid("receipt envelope"));
        }
        unhex(&self.issuer_key, 32)?;
        hash(RECEIPT_DOMAIN, self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedReceipt {
    pub receipt: Receipt,
    pub signature: String,
}
impl SignedReceipt {
    /// The authority must first verify and atomically record its accepted ref.
    /// Signing arbitrary supplied fields alone is not server acceptance.
    pub fn sign(mut receipt: Receipt, key: &Ed25519KeyPair) -> Result<Self, Error> {
        receipt.issuer_key = hex(key.public_key().as_ref());
        let signature = hex(key
            .sign(&message(SIGN_DOMAIN, &receipt.address()?)?)
            .as_ref());
        Ok(Self { receipt, signature })
    }
    pub fn verify(&self) -> Result<Address, Error> {
        let address = self.receipt.address()?;
        verify(
            &self.receipt.issuer_key,
            &self.signature,
            &message(SIGN_DOMAIN, &address)?,
        )?;
        Ok(address)
    }
}

/// Serializable local trust state. Keep it outside the remote database's trust
/// boundary. Deleting/rolling back the client's own trusted storage loses a pin.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pin {
    trusted_key: String,
    current: SignedReceipt,
    revoked: bool,
}
impl Pin {
    /// Explicit bootstrap: trusts this checkpoint, not unseen earlier history.
    /// Never obtain `trusted_key` solely from the untrusted receipt itself.
    pub fn trust_checkpoint(
        scope: &Scope,
        trusted_key: &str,
        receipt: SignedReceipt,
    ) -> Result<Self, Error> {
        if &receipt.receipt.scope != scope || receipt.receipt.issuer_key != trusted_key {
            return Err(Error::Invalid("untrusted receipt scope/key"));
        }
        receipt.verify()?;
        Ok(Self {
            trusted_key: trusted_key.into(),
            current: receipt,
            revoked: false,
        })
    }
    pub fn validate(&self) -> Result<(), Error> {
        if self.current.receipt.issuer_key != self.trusted_key {
            return Err(Error::Invalid("stored receipt authority"));
        }
        self.current.verify()?;
        Ok(())
    }
    pub fn scope(&self) -> &Scope {
        &self.current.receipt.scope
    }
    pub fn receipt(&self) -> &SignedReceipt {
        &self.current
    }
    pub fn revoked(&self) -> bool {
        self.revoked
    }
    pub fn revoke(&mut self) {
        self.revoked = true;
    }

    fn next(&self, receipt: &SignedReceipt, key: &str) -> Result<Self, Error> {
        let prior = self.current.verify()?;
        receipt.verify()?;
        if receipt.receipt.scope != *self.scope()
            || receipt.receipt.issuer_key != key
            || receipt.receipt.previous.as_ref() != Some(&prior)
            || Some(receipt.receipt.head.generation)
                != self.current.receipt.head.generation.checked_add(1)
        {
            return Err(Error::Invalid("receipt rollback, fork or missing proof"));
        }
        Ok(Self {
            trusted_key: key.into(),
            current: receipt.clone(),
            revoked: false,
        })
    }

    /// Contiguous bounded proof; a retry may include an already accepted prefix
    /// only when it connects exactly to the current pin.
    /// Skipped/reordered generations, a fork, or an unapproved key all fail.
    pub fn advance(&self, proof: &[SignedReceipt]) -> Result<Self, Error> {
        self.validate()?;
        if self.revoked {
            return Err(Error::Invalid("receipt authority revoked"));
        }
        if proof.len() > MAX_PROOF_RECEIPTS {
            return Err(Error::Limit("receipt proof"));
        }
        for (index, receipt) in proof.iter().enumerate() {
            receipt.verify()?;
            if receipt.receipt.scope != *self.scope()
                || receipt.receipt.issuer_key != self.trusted_key
            {
                return Err(Error::Invalid("untrusted receipt scope/key"));
            }
            if index > 0 && receipt != &proof[index - 1] {
                let previous = &proof[index - 1];
                if receipt.receipt.previous.as_ref() != Some(&previous.verify()?)
                    || Some(receipt.receipt.head.generation)
                        != previous.receipt.head.generation.checked_add(1)
                {
                    return Err(Error::Invalid("noncontiguous receipt proof"));
                }
            }
        }
        let start = if proof
            .first()
            .is_some_and(|r| r.receipt.head.generation <= self.current.receipt.head.generation)
        {
            proof
                .iter()
                .position(|r| r == &self.current)
                .ok_or(Error::Invalid("receipt rollback or fork"))?
        } else {
            0
        };
        let mut pin = self.clone();
        for receipt in &proof[start..] {
            if receipt == &pin.current {
                continue;
            }
            pin = pin.next(receipt, &pin.trusted_key)?;
        }
        Ok(pin)
    }

    /// Require both old and new key signatures at this exact pinned receipt.
    /// A revoked authority cannot authorize its own replacement; recovery then
    /// requires an independently trusted, explicit new bootstrap.
    pub fn rotate(&self, transition: &SignedRotation, next: &SignedReceipt) -> Result<Self, Error> {
        self.validate()?;
        if self.revoked {
            return Err(Error::Invalid("receipt authority revoked"));
        }
        let rotation = &transition.rotation;
        let bytes = message(ROTATION_SIGN_DOMAIN, &rotation.address()?)?;
        verify(&rotation.old_key, &transition.old_signature, &bytes)?;
        verify(&rotation.new_key, &transition.new_signature, &bytes)?;
        if rotation.scope != *self.scope()
            || rotation.old_key != self.trusted_key
            || rotation.previous != self.current.verify()?
            || Some(rotation.next_generation) != self.current.receipt.head.generation.checked_add(1)
        {
            return Err(Error::Invalid("authority rotation scope/position"));
        }
        self.next(next, &rotation.new_key)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rotation {
    pub version: u8,
    pub scope: Scope,
    pub previous: Address,
    pub next_generation: i64,
    pub old_key: String,
    pub new_key: String,
}
impl Rotation {
    pub fn address(&self) -> Result<Address, Error> {
        self.scope.validate()?;
        unhex(&self.old_key, 32)?;
        unhex(&self.new_key, 32)?;
        if self.version != 1 || self.next_generation < 2 || self.old_key == self.new_key {
            return Err(Error::Invalid("authority rotation"));
        }
        hash(ROTATION_DOMAIN, self)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedRotation {
    pub rotation: Rotation,
    pub old_signature: String,
    pub new_signature: String,
}
impl SignedRotation {
    pub fn sign(pin: &Pin, old: &Ed25519KeyPair, new: &Ed25519KeyPair) -> Result<Self, Error> {
        pin.validate()?;
        if pin.revoked || pin.trusted_key != hex(old.public_key().as_ref()) {
            return Err(Error::Invalid("rotation authority"));
        }
        let rotation = Rotation {
            version: 1,
            scope: pin.scope().clone(),
            previous: pin.current.verify()?,
            next_generation: pin
                .current
                .receipt
                .head
                .generation
                .checked_add(1)
                .ok_or(Error::Limit("ref generation"))?,
            old_key: pin.trusted_key.clone(),
            new_key: hex(new.public_key().as_ref()),
        };
        let bytes = message(ROTATION_SIGN_DOMAIN, &rotation.address()?)?;
        Ok(Self {
            rotation,
            old_signature: hex(old.sign(&bytes).as_ref()),
            new_signature: hex(new.sign(&bytes).as_ref()),
        })
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    pub(crate) fn key(seed: u8) -> Ed25519KeyPair {
        Ed25519KeyPair::from_seed_unchecked(&[seed; 32]).unwrap()
    }
    pub(crate) fn scope(project: Uuid) -> Scope {
        Scope {
            authority: Uuid::from_u128(1),
            project,
            branch: "main".into(),
        }
    }
    pub(crate) fn signed(
        scope: Scope,
        generation: i64,
        previous: Option<Address>,
        seed: u8,
    ) -> SignedReceipt {
        SignedReceipt::sign(
            Receipt {
                version: 1,
                scope,
                head: Head {
                    commit: Address::try_from(format!("{generation:064x}")).unwrap(),
                    generation,
                },
                previous,
                issued_at_ms: 1_700_000_000_000,
                issuer_key: String::new(),
            },
            &key(seed),
        )
        .unwrap()
    }
    fn first() -> (SignedReceipt, Pin) {
        let first = signed(scope(Uuid::from_u128(2)), 1, None, 7);
        let pin = Pin::trust_checkpoint(
            &first.receipt.scope,
            &hex(key(7).public_key().as_ref()),
            first.clone(),
        )
        .unwrap();
        (first, pin)
    }
    #[test]
    fn receipt_hash_has_an_independent_known_answer() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/ref-receipt-v1.json")).unwrap();
        let receipt: Receipt = serde_json::from_value(fixture["receipt"].clone()).unwrap();
        assert_eq!(
            receipt.address().unwrap().as_str(),
            fixture["address"].as_str().unwrap()
        );
        assert_eq!(
            serde_json::to_string(&CanonicalValue(&serde_json::to_value(receipt).unwrap()))
                .unwrap(),
            fixture["canonical"].as_str().unwrap()
        );
    }
    #[test]
    fn receipt_and_rotation_signatures_match_independent_openssl_vectors() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/ref-receipt-signatures-v1.json"
        ))
        .unwrap();
        let first: SignedReceipt =
            serde_json::from_value(fixture["first"]["signed"].clone()).unwrap();
        assert_eq!(
            first.verify().unwrap().as_str(),
            fixture["first"]["address"].as_str().unwrap()
        );
        assert_eq!(
            SignedReceipt::sign(first.receipt.clone(), &key(7)).unwrap(),
            first
        );
        let pin = Pin::trust_checkpoint(
            &first.receipt.scope,
            &hex(key(7).public_key().as_ref()),
            first.clone(),
        )
        .unwrap();
        let transition: SignedRotation =
            serde_json::from_value(fixture["rotation"]["signed"].clone()).unwrap();
        assert_eq!(
            transition.rotation.address().unwrap().as_str(),
            fixture["rotation"]["address"].as_str().unwrap()
        );
        assert_eq!(
            SignedRotation::sign(&pin, &key(7), &key(8)).unwrap(),
            transition
        );
        let next: SignedReceipt =
            serde_json::from_value(fixture["next"]["signed"].clone()).unwrap();
        assert_eq!(
            next.verify().unwrap().as_str(),
            fixture["next"]["address"].as_str().unwrap()
        );
        assert_eq!(
            SignedReceipt::sign(next.receipt.clone(), &key(8)).unwrap(),
            next
        );
        assert_eq!(pin.rotate(&transition, &next).unwrap().receipt(), &next);
    }
    #[test]
    fn signature_binds_scope_position_commit_time_and_key() {
        let (first, _) = first();
        let original = serde_json::to_value(&first).unwrap();
        let mut unknown = original.clone();
        unknown["receipt"]["head"]["unsigned_extra"] = serde_json::json!("unbound data");
        assert!(serde_json::from_value::<SignedReceipt>(unknown).is_err());
        for (path, replacement) in [
            (
                "/receipt/scope/authority",
                serde_json::json!(Uuid::from_u128(3)),
            ),
            (
                "/receipt/scope/project",
                serde_json::json!(Uuid::from_u128(4)),
            ),
            ("/receipt/scope/branch", serde_json::json!("proposal")),
            ("/receipt/head/generation", serde_json::json!(2)),
            ("/receipt/head/commit", serde_json::json!("aa".repeat(32))),
            ("/receipt/issued_at_ms", serde_json::json!(1)),
            (
                "/receipt/issuer_key",
                serde_json::json!(hex(key(8).public_key().as_ref())),
            ),
            ("/signature", serde_json::json!("00".repeat(64))),
        ] {
            let mut changed = original.clone();
            *changed.pointer_mut(path).unwrap() = replacement;
            let changed: SignedReceipt = serde_json::from_value(changed).unwrap();
            assert!(changed.verify().is_err(), "{path}");
        }
        assert!(Pin::trust_checkpoint(
            &first.receipt.scope.clone(),
            &hex(key(8).public_key().as_ref()),
            first
        )
        .is_err());
    }
    #[test]
    fn pin_rejects_rollback_forks_gaps_reordering_and_scope_substitution() {
        let (a, pin) = first();
        let b = signed(pin.scope().clone(), 2, Some(a.verify().unwrap()), 7);
        let mut restored = signed(pin.scope().clone(), 3, Some(b.verify().unwrap()), 7).receipt;
        restored.head.commit = a.receipt.head.commit.clone();
        // A newly authorized ref update may point at retained old content. It is
        // distinct from rolling the receipt generation back and hiding updates.
        let c = SignedReceipt::sign(restored, &key(7)).unwrap();
        assert!(pin.advance(std::slice::from_ref(&c)).is_err());
        assert!(pin.advance(&[c.clone(), b.clone()]).is_err());
        let latest = pin.advance(&[b.clone(), c.clone()]).unwrap();
        assert_eq!(
            latest.advance(&[a.clone(), b.clone(), c.clone()]).unwrap(),
            latest,
            "lost-ACK retry is idempotent"
        );
        assert!(latest.advance(std::slice::from_ref(&b)).is_err());
        let mut fork = c.receipt.clone();
        fork.head.commit = Address::try_from("ff".repeat(32)).unwrap();
        let fork = SignedReceipt::sign(fork, &key(7)).unwrap();
        assert!(
            latest.advance(&[fork]).is_err(),
            "even a valid authority signature cannot overwrite a pin"
        );
        let mut other_scope = pin.scope().clone();
        other_scope.branch = "other".into();
        let elsewhere = signed(other_scope, 2, Some(a.verify().unwrap()), 7);
        assert!(pin.advance(&[elsewhere]).is_err());
        let foreign = signed(pin.scope().clone(), 2, Some(a.verify().unwrap()), 8);
        assert!(pin.advance(&[foreign]).is_err());
        assert!(pin.advance(&vec![a; MAX_PROOF_RECEIPTS + 1]).is_err());
    }
    #[test]
    fn rotation_requires_both_keys_and_exact_pin_and_revocation_survives_serialization() {
        let (a, pin) = first();
        let transition = SignedRotation::sign(&pin, &key(7), &key(8)).unwrap();
        let next = signed(pin.scope().clone(), 2, Some(a.verify().unwrap()), 8);
        assert!(pin.advance(std::slice::from_ref(&next)).is_err());
        let rotated = pin.rotate(&transition, &next).unwrap();
        assert!(
            rotated.rotate(&transition, &next).is_err(),
            "rotation replay must fail"
        );
        for old in [true, false] {
            let mut altered = transition.clone();
            if old {
                altered.old_signature = "00".repeat(64);
            } else {
                altered.new_signature = "00".repeat(64);
            }
            assert!(pin.rotate(&altered, &next).is_err());
        }
        let old_key = signed(pin.scope().clone(), 3, Some(next.verify().unwrap()), 7);
        assert!(rotated.advance(&[old_key]).is_err());
        let newest = signed(pin.scope().clone(), 3, Some(next.verify().unwrap()), 8);
        let mut latest = rotated.advance(std::slice::from_ref(&newest)).unwrap();
        latest.revoke();
        let saved: Pin = serde_json::from_slice(&serde_json::to_vec(&latest).unwrap()).unwrap();
        assert!(saved.revoked());
        assert!(saved.advance(&[]).is_err());
        assert!(SignedRotation::sign(&saved, &key(8), &key(9)).is_err());
    }
}
