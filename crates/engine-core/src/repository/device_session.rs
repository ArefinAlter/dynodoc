//! Proof of possession for a short-lived, project-scoped service token.
//!
//! The challenge is issued statelessly by the service (it carries a MAC the
//! service alone can compute), so asking for one writes nothing. The device signs
//! the challenge; the service verifies the MAC, the signature, the key binding and
//! current project access, and consumes the challenge id exactly once. This proves
//! possession of an enrolled key; it is neither a project grant nor a refresh
//! credential. The domain label differs from device enrollment, so neither proof
//! can be replayed as the other.
use super::{hex, unhex, Error};
use crate::shared_checkpoint::canonical::CanonicalValue;
use ring::signature::{self, Ed25519KeyPair, KeyPair};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const DOMAIN: &[u8] = b"dynodoc.device-session\0v1\0";
pub const MAX_SESSION_BYTES: usize = 2048;
/// Sorted. Scopes are independent; none implies another.
pub const SCOPES: [&str; 4] = [
    "history:read",
    "history:review",
    "history:write",
    "project:read",
];
/// A challenge may be answered for two minutes.
pub const CHALLENGE_TTL_MS: i64 = 120_000;
/// The only token lifetime the device accepts and the service issues.
pub const TOKEN_TTL_SECONDS: i64 = 3600;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionChallenge {
    pub version: u8,
    pub id: Uuid,
    /// Installation ID of the service that issued the challenge.
    pub authority: Uuid,
    pub public_key: String,
    pub project: Uuid,
    /// Sorted, unique, a non-empty subset of [`SCOPES`].
    pub scopes: Vec<String>,
    /// A fresh cryptographically random 32-byte nonce, lowercase hexadecimal.
    pub nonce: String,
    pub issued_at_ms: i64,
    pub expires_at_ms: i64,
    pub token_ttl_seconds: i64,
}

/// Sort/deduplicate requested scopes and refuse unknown or empty sets.
pub fn normalize_scopes<S: AsRef<str>>(requested: &[S]) -> Result<Vec<String>, Error> {
    let mut scopes: Vec<String> = requested.iter().map(|s| s.as_ref().to_owned()).collect();
    scopes.sort();
    scopes.dedup();
    if scopes.is_empty() || scopes.iter().any(|s| !SCOPES.contains(&s.as_str())) {
        return Err(Error::Invalid("device session scopes"));
    }
    Ok(scopes)
}

impl SessionChallenge {
    /// Structural checks only; expiry and authority belong to the verifier.
    pub fn validate(&self) -> Result<(), Error> {
        if self.version != 1
            || self.id.is_nil()
            || self.authority.is_nil()
            || self.project.is_nil()
            || self.issued_at_ms <= 0
            || self.expires_at_ms <= self.issued_at_ms
            || self.expires_at_ms - self.issued_at_ms > CHALLENGE_TTL_MS
            || self.token_ttl_seconds != TOKEN_TTL_SECONDS
            || normalize_scopes(&self.scopes)? != self.scopes
        {
            return Err(Error::Invalid("device session challenge"));
        }
        unhex(&self.public_key, 32)?;
        unhex(&self.nonce, 32)?;
        Ok(())
    }

    /// Canonical JSON (sorted keys, compact UTF-8) of every field.
    pub fn canonical(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        let value = serde_json::to_value(self)?;
        let bytes = serde_json::to_vec(&CanonicalValue(&value))?;
        if bytes.len() > MAX_SESSION_BYTES {
            return Err(Error::Limit("device session bytes"));
        }
        Ok(bytes)
    }

    /// The bytes the device signs.
    pub fn message(&self) -> Result<Vec<u8>, Error> {
        let mut message = DOMAIN.to_vec();
        message.extend(self.canonical()?);
        Ok(message)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionProof {
    pub challenge: SessionChallenge,
    /// Service MAC over the challenge, echoed unchanged. It is not signed by the
    /// device: it lets a stateless service recognize its own challenge.
    pub mac: String,
    pub signature: String,
}

impl SessionProof {
    /// The caller passes the installation, project and scopes it intended to use,
    /// so it never signs a substituted challenge.
    pub fn sign(
        challenge: SessionChallenge,
        mac: String,
        authority: Uuid,
        project: Uuid,
        scopes: &[String],
        key: &Ed25519KeyPair,
    ) -> Result<Self, Error> {
        if challenge.authority != authority
            || challenge.project != project
            || challenge.scopes != normalize_scopes(scopes)?
            || challenge.public_key != hex(key.public_key().as_ref())
        {
            return Err(Error::Invalid("device session scope or key"));
        }
        unhex(&mac, 32)?;
        let signature = hex(key.sign(&challenge.message()?).as_ref());
        Ok(Self {
            challenge,
            mac,
            signature,
        })
    }

    /// Checks the device signature only. The service must also verify the MAC,
    /// freshness, key binding and project access.
    pub fn verify_signature(&self) -> Result<(), Error> {
        unhex(&self.mac, 32)?;
        signature::UnparsedPublicKey::new(
            &signature::ED25519,
            unhex(&self.challenge.public_key, 32)?,
        )
        .verify(&self.challenge.message()?, &unhex(&self.signature, 64)?)
        .map_err(|_| Error::Invalid("device session signature"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer(seed: u8) -> Ed25519KeyPair {
        Ed25519KeyPair::from_seed_unchecked(&[seed; 32]).unwrap()
    }
    fn challenge(key: &Ed25519KeyPair) -> SessionChallenge {
        SessionChallenge {
            version: 1,
            id: Uuid::from_u128(1),
            authority: Uuid::from_u128(2),
            public_key: hex(key.public_key().as_ref()),
            project: Uuid::from_u128(3),
            scopes: vec!["history:read".into(), "history:write".into()],
            nonce: "09".repeat(32),
            issued_at_ms: 1_800_000_000_000,
            expires_at_ms: 1_800_000_120_000,
            token_ttl_seconds: TOKEN_TTL_SECONDS,
        }
    }

    #[test]
    fn independent_vector_is_stable() {
        // Produced by a separate Node.js crypto implementation from a public
        // synthetic seed; do not regenerate it to conceal a protocol mismatch.
        let value: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/device-session-v1.json"))
                .unwrap();
        let proof: SessionProof = serde_json::from_value(value["proof"].clone()).unwrap();
        proof.verify_signature().unwrap();
        assert_eq!(
            String::from_utf8(proof.challenge.canonical().unwrap()).unwrap(),
            value["canonical"].as_str().unwrap()
        );
        assert_eq!(
            hex(&proof.challenge.message().unwrap()),
            value["message_hex"].as_str().unwrap()
        );
        let key = signer(7);
        assert_eq!(
            SessionProof::sign(
                proof.challenge.clone(),
                proof.mac.clone(),
                proof.challenge.authority,
                proof.challenge.project,
                &proof.challenge.scopes,
                &key
            )
            .unwrap(),
            proof
        );
    }

    #[test]
    fn signature_binds_every_field_and_the_expected_scope() {
        let key = signer(7);
        let original = challenge(&key);
        let proof = SessionProof::sign(
            original.clone(),
            "ab".repeat(32),
            original.authority,
            original.project,
            &original.scopes,
            &key,
        )
        .unwrap();
        proof.verify_signature().unwrap();
        let encoded = serde_json::to_value(&proof).unwrap();
        for (field, value) in [
            ("id", serde_json::json!(Uuid::from_u128(9))),
            ("authority", serde_json::json!(Uuid::from_u128(9))),
            ("public_key", serde_json::json!("08".repeat(32))),
            ("project", serde_json::json!(Uuid::from_u128(9))),
            ("scopes", serde_json::json!(["history:read"])),
            ("nonce", serde_json::json!("08".repeat(32))),
            ("issued_at_ms", serde_json::json!(1_800_000_000_001_i64)),
            ("expires_at_ms", serde_json::json!(1_800_000_119_999_i64)),
        ] {
            let mut changed = encoded.clone();
            changed["challenge"][field] = value;
            assert!(
                serde_json::from_value::<SessionProof>(changed)
                    .unwrap()
                    .verify_signature()
                    .is_err(),
                "{field}"
            );
        }
        let sign = |authority, project, scopes: &[String], signer: &Ed25519KeyPair| {
            SessionProof::sign(
                original.clone(),
                "ab".repeat(32),
                authority,
                project,
                scopes,
                signer,
            )
        };
        assert!(sign(Uuid::from_u128(9), original.project, &original.scopes, &key).is_err());
        assert!(sign(
            original.authority,
            Uuid::from_u128(9),
            &original.scopes,
            &key
        )
        .is_err());
        assert!(sign(
            original.authority,
            original.project,
            &["history:read".to_owned()],
            &key
        )
        .is_err());
        assert!(sign(
            original.authority,
            original.project,
            &original.scopes,
            &signer(8)
        )
        .is_err());
    }

    #[test]
    fn enrollment_and_session_proofs_are_not_interchangeable() {
        // The session message always begins with its own domain label, so no
        // enrollment message (`dynodoc.device-enrollment`) can equal it.
        let key = signer(7);
        let message = challenge(&key).message().unwrap();
        assert!(message.starts_with(b"dynodoc.device-session\0v1\0"));
        assert!(!message.starts_with(b"dynodoc.device-enrollment\0v1\0"));
    }

    #[test]
    fn scopes_must_be_known_sorted_and_nonempty_and_ttl_fixed() {
        assert_eq!(
            normalize_scopes(&["project:read", "history:read", "history:read"]).unwrap(),
            vec!["history:read", "project:read"]
        );
        assert_eq!(
            normalize_scopes(&["history:write", "history:review"]).unwrap(),
            vec!["history:review", "history:write"]
        );
        assert!(normalize_scopes::<&str>(&[]).is_err());
        assert!(normalize_scopes(&["history:read", "admin"]).is_err());
        let key = signer(7);
        let mut value = challenge(&key);
        value.validate().unwrap();
        value.scopes = vec!["history:write".into(), "history:read".into()];
        assert!(value.validate().is_err());
        value.scopes = vec!["history:read".into()];
        value.token_ttl_seconds += 1;
        assert!(value.validate().is_err());
        value.token_ttl_seconds = TOKEN_TTL_SECONDS;
        value.expires_at_ms = value.issued_at_ms + CHALLENGE_TTL_MS + 1;
        assert!(value.validate().is_err());
        value.expires_at_ms = value.issued_at_ms + 1;
        value.nonce = "A".repeat(64);
        assert!(value.validate().is_err());
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let key = signer(7);
        let mut value = serde_json::to_value(challenge(&key)).unwrap();
        value["extra"] = serde_json::json!(true);
        assert!(serde_json::from_value::<SessionChallenge>(value).is_err());
    }
}
