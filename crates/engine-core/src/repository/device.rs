//! Account enrollment proof. This proves possession of a signing key; the server
//! must also authenticate the account and consume its own unexpired challenge.
use super::{hex, unhex, Error};
use crate::shared_checkpoint::canonical::CanonicalValue;
use ring::signature::{self, Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const DOMAIN: &[u8] = b"dynodoc.device-enrollment\0v1\0";
pub const MAX_PROOF_BYTES: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Challenge {
    pub version: u8,
    pub id: Uuid,
    /// Stable installation ID obtained through the authenticated service.
    pub authority: Uuid,
    pub identity: Uuid,
    pub public_key: String,
    /// A fresh cryptographically random 32-byte nonce, lowercase hexadecimal.
    pub nonce: String,
    pub expires_at_ms: i64,
}

pub fn validate_public_key(key: &str) -> Result<(), Error> {
    unhex(key, 32).map(|_| ())
}

impl Challenge {
    pub fn message(&self) -> Result<Vec<u8>, Error> {
        if self.version != 1
            || self.id.is_nil()
            || self.authority.is_nil()
            || self.identity.is_nil()
            || self.expires_at_ms <= 0
        {
            return Err(Error::Invalid("device enrollment challenge"));
        }
        validate_public_key(&self.public_key)?;
        unhex(&self.nonce, 32)?;
        let value = serde_json::to_value(self)?;
        let mut message = DOMAIN.to_vec();
        serde_json::to_writer(&mut message, &CanonicalValue(&value))?;
        if message.len() > MAX_PROOF_BYTES {
            return Err(Error::Limit("device proof bytes"));
        }
        Ok(message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proof {
    pub challenge: Challenge,
    pub signature: String,
}

impl Proof {
    /// Callers verify the server/account displayed by the authenticated session.
    /// Requiring both expected IDs prevents blindly signing a substituted scope.
    pub fn sign(
        challenge: Challenge,
        authority: Uuid,
        identity: Uuid,
        key: &Ed25519KeyPair,
    ) -> Result<Self, Error> {
        if challenge.authority != authority
            || challenge.identity != identity
            || challenge.public_key != hex(key.public_key().as_ref())
        {
            return Err(Error::Invalid("device enrollment scope or key"));
        }
        let signature = hex(key.sign(&challenge.message()?).as_ref());
        Ok(Self {
            challenge,
            signature,
        })
    }

    pub fn verify(&self) -> Result<(), Error> {
        signature::UnparsedPublicKey::new(
            &signature::ED25519,
            unhex(&self.challenge.public_key, 32)?,
        )
        .verify(&self.challenge.message()?, &unhex(&self.signature, 64)?)
        .map_err(|_| Error::Invalid("device enrollment signature"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_enrollment_vector_is_stable() {
        let value: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/device-enrollment-v1.json"
        ))
        .unwrap();
        let proof: Proof = serde_json::from_value(value["proof"].clone()).unwrap();
        proof.verify().unwrap();
        assert_eq!(
            hex(&proof.challenge.message().unwrap()),
            value["message_hex"].as_str().unwrap()
        );
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        assert_eq!(
            Proof::sign(
                proof.challenge.clone(),
                proof.challenge.authority,
                proof.challenge.identity,
                &key
            )
            .unwrap(),
            proof
        );
    }

    #[test]
    fn proof_binds_every_field_and_expected_scope() {
        let key = Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap();
        let challenge = Challenge {
            version: 1,
            id: Uuid::from_u128(1),
            authority: Uuid::from_u128(2),
            identity: Uuid::from_u128(3),
            public_key: hex(key.public_key().as_ref()),
            nonce: "09".repeat(32),
            expires_at_ms: 1_800_000_000_000,
        };
        let proof = Proof::sign(
            challenge.clone(),
            challenge.authority,
            challenge.identity,
            &key,
        )
        .unwrap();
        proof.verify().unwrap();
        let original = serde_json::to_value(&proof).unwrap();
        for (field, value) in [
            ("version", serde_json::json!(2)),
            ("id", serde_json::json!(Uuid::from_u128(4))),
            ("authority", serde_json::json!(Uuid::from_u128(4))),
            ("identity", serde_json::json!(Uuid::from_u128(4))),
            ("public_key", serde_json::json!("08".repeat(32))),
            ("nonce", serde_json::json!("08".repeat(32))),
            ("expires_at_ms", serde_json::json!(1_800_000_000_001_i64)),
        ] {
            let mut changed = original.clone();
            changed["challenge"][field] = value;
            assert!(
                serde_json::from_value::<Proof>(changed)
                    .unwrap()
                    .verify()
                    .is_err(),
                "{field}"
            );
        }
        assert!(Proof::sign(
            challenge.clone(),
            Uuid::from_u128(4),
            challenge.identity,
            &key
        )
        .is_err());
        assert!(Proof::sign(
            challenge.clone(),
            challenge.authority,
            Uuid::from_u128(4),
            &key
        )
        .is_err());
        assert!(Proof::sign(
            challenge.clone(),
            challenge.authority,
            challenge.identity,
            &Ed25519KeyPair::from_seed_unchecked(&[8; 32]).unwrap()
        )
        .is_err());
    }
}
