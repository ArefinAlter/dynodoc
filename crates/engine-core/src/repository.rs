//! Portable project history, separate from centralized per-file event sequences.
//! Local signatures prove control of a key, not a verified Dynodoc account or
//! permission. Servers must authorize object exchange, actors and ref publication.

use crate::shared_checkpoint::{Address, Error as ObjectError};
use engine_shared::DocumentId;
use ring::{
    digest,
    signature::{self, Ed25519KeyPair, KeyPair},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub mod attestation;
pub mod compare;
pub mod device;
pub mod local;
pub mod receipt_journal;
pub mod recovery;
pub mod server_authority;

pub const MAX_FILES: usize = 4096;
pub const MAX_COMMIT_BYTES: usize = 1024 * 1024;
pub const MAX_ANCESTORS: usize = 100_000;
const COMMIT_DOMAIN: &[u8] = b"dynodoc.project-commit\0v1\0";
const SIGNATURE_DOMAIN: &[u8] = b"dynodoc.project-commit-signature\0v1\0";
const RECOVERY_COMMIT_DOMAIN: &[u8] = b"dynodoc.project-commit\0v2\0";
const RECOVERY_SIGNATURE_DOMAIN: &[u8] = b"dynodoc.project-commit-signature\0v2\0";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid repository data: {0}")]
    Invalid(&'static str),
    #[error("repository limit: {0}")]
    Limit(&'static str),
    #[error("branch changed; fetch and compare before retrying")]
    StaleRef,
    #[error("missing commit {0:?}")]
    Missing(Address),
    #[error("repository database: {0}")]
    Db(#[from] sqlx::Error),
    #[error("repository encoding: {0}")]
    Json(#[from] serde_json::Error),
    #[error("repository content: {0}")]
    Object(#[from] ObjectError),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FileEntry {
    pub document: DocumentId,
    /// V2 typed state root, not a legacy snapshot or original native file blob.
    pub state: Address,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub version: u8,
    pub project: uuid::Uuid,
    /// Ordered parents preserve the first-parent/mainline distinction. At most two.
    pub parents: Vec<Address>,
    pub files: BTreeMap<String, FileEntry>,
    /// Ed25519 public key, lowercase hexadecimal. Identity binding is external.
    pub author_key: String,
    pub authored_at_ms: i64,
    pub message: String,
    /// V2 only. Recovery references are signed provenance, not merge parents.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<recovery::Source>,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(value: &str, size: usize) -> Result<Vec<u8>, Error> {
    if value.len() != size * 2
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(Error::Invalid("hex encoding"));
    }
    Ok(value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
            digit(pair[0]) * 16 + digit(pair[1])
        })
        .collect())
}

pub fn valid_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 1024
        && !path.contains(['\\', ':'])
        && !path.chars().any(char::is_control)
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

pub fn valid_branch(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && name
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-_/ .".contains(&b))
        && !name.contains(' ')
        && !name.ends_with('.')
        && !name.contains("..")
}

impl Commit {
    /// Required commit objects for import/transfer. Only `parents` are ancestry;
    /// recovery sources/mainlines are additional provenance dependencies.
    pub fn dependencies(&self) -> Vec<Address> {
        let mut result = self.parents.clone();
        if let Some(source) = &self.recovery {
            for address in std::iter::once(&source.commit).chain(source.mainline.iter()) {
                if !result.contains(address) {
                    result.push(address.clone());
                }
            }
        }
        result
    }

    pub fn validate(&self) -> Result<(), Error> {
        if !matches!((self.version, &self.recovery), (1, None) | (2, Some(_)))
            || (self.recovery.is_some() && self.parents.len() != 1)
            || self.project.is_nil()
            || self.parents.len() > 2
            || (self.parents.len() == 2 && self.parents[0] == self.parents[1])
            || self.authored_at_ms < 0
            || self.message.len() > 4096
        {
            return Err(Error::Invalid("commit metadata"));
        }
        unhex(&self.author_key, 32)?;
        if self.files.len() > MAX_FILES {
            return Err(Error::Limit("project files"));
        }
        let mut documents = BTreeSet::new();
        for (path, file) in &self.files {
            if !valid_path(path) || file.document.0.is_nil() || !documents.insert(file.document.0) {
                return Err(Error::Invalid("file path or duplicate document identity"));
            }
            // A logical file cannot simultaneously act as a directory.
            if path
                .match_indices('/')
                .any(|(index, _)| self.files.contains_key(&path[..index]))
            {
                return Err(Error::Invalid("file/directory collision"));
            }
        }
        Ok(())
    }

    pub fn bytes(&self) -> Result<Vec<u8>, Error> {
        self.validate()?;
        let value = serde_json::to_value(self)?;
        let bytes =
            serde_json::to_vec(&crate::shared_checkpoint::canonical::CanonicalValue(&value))?;
        if bytes.len() > MAX_COMMIT_BYTES {
            return Err(Error::Limit("commit bytes"));
        }
        Ok(bytes)
    }
    pub fn address(&self) -> Result<Address, Error> {
        let mut hash = digest::Context::new(&digest::SHA256);
        hash.update(if self.version == 1 {
            COMMIT_DOMAIN
        } else {
            RECOVERY_COMMIT_DOMAIN
        });
        hash.update(&self.bytes()?);
        Ok(Address::try_from(hex(hash.finish().as_ref()))?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SignedCommit {
    pub commit: Commit,
    pub signature: String,
}

impl SignedCommit {
    pub fn sign(mut commit: Commit, key: &Ed25519KeyPair) -> Result<Self, Error> {
        commit.author_key = hex(key.public_key().as_ref());
        let address = commit.address()?;
        let mut message = signature_domain(commit.version).to_vec();
        message.extend(unhex(address.as_str(), 32)?);
        Ok(Self {
            commit,
            signature: hex(key.sign(&message).as_ref()),
        })
    }
    pub fn verify(&self) -> Result<Address, Error> {
        let address = self.commit.address()?;
        let mut message = signature_domain(self.commit.version).to_vec();
        message.extend(unhex(address.as_str(), 32)?);
        signature::UnparsedPublicKey::new(&signature::ED25519, unhex(&self.commit.author_key, 32)?)
            .verify(&message, &unhex(&self.signature, 64)?)
            .map_err(|_| Error::Invalid("commit signature"))?;
        Ok(address)
    }
}

fn signature_domain(version: u8) -> &'static [u8] {
    if version == 1 {
        SIGNATURE_DOMAIN
    } else {
        RECOVERY_SIGNATURE_DOMAIN
    }
}

/// Generation prevents an ABA ref update (A -> B -> A) from satisfying an old CAS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Head {
    pub commit: Address,
    pub generation: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn signed(
        project: uuid::Uuid,
        parents: Vec<Address>,
        files: BTreeMap<String, FileEntry>,
        message: &str,
    ) -> SignedCommit {
        SignedCommit::sign(
            Commit {
                version: 1,
                project,
                parents,
                files,
                author_key: String::new(),
                authored_at_ms: 1_700_000_000_000,
                message: message.into(),
                recovery: None,
            },
            &Ed25519KeyPair::from_seed_unchecked(&[7; 32]).unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn signature_binds_repository_parents_files_actor_time_and_message() {
        let project = uuid::Uuid::from_u128(1);
        let original = signed(project, vec![], BTreeMap::new(), "Initial");
        let root = original.verify().unwrap();
        let mut changes = Vec::new();
        let mut changed = original.clone();
        changed.commit.project = uuid::Uuid::from_u128(2);
        changes.push(changed);
        let mut changed = original.clone();
        changed.commit.parents.push(root);
        changes.push(changed);
        let mut changed = original.clone();
        changed.commit.authored_at_ms += 1;
        changes.push(changed);
        let mut changed = original.clone();
        changed.commit.message.push('!');
        changes.push(changed);
        let mut changed = original.clone();
        changed.commit.author_key = "00".repeat(32);
        changes.push(changed);
        for changed in changes {
            assert!(changed.verify().is_err());
        }
        let mut wrong = original;
        wrong.signature = "00".repeat(64);
        assert!(wrong.verify().is_err());
    }
    #[test]
    fn paths_and_branches_are_bounded_relative_identifiers() {
        for bad in ["", "../a", "/a", "a//b", "a/./b", "a\\b", "C:a", "a\nb"] {
            assert!(!valid_path(bad));
        }
        assert!(valid_path("বাংলা/Document.docx"));
        for bad in ["", "/main", "../main", "a//b", "a b", "x..y", "main."] {
            assert!(!valid_branch(bad));
        }
        assert!(valid_branch("review/round-2"));
    }

    #[test]
    fn independent_commit_hash_vector_and_file_identity_constraints() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/project-commit-v1.json")).unwrap();
        let commit: Commit = serde_json::from_value(fixture["commit"].clone()).unwrap();
        assert_eq!(
            commit.bytes().unwrap(),
            fixture["canonical"].as_str().unwrap().as_bytes()
        );
        assert_eq!(
            commit.address().unwrap().as_str(),
            fixture["address"].as_str().unwrap()
        );
        let mut changed = commit;
        let entry = FileEntry {
            document: DocumentId(uuid::Uuid::from_u128(1)),
            state: Address::try_from("ab".repeat(32)).unwrap(),
        };
        changed.files.insert("a".into(), entry.clone());
        changed.files.insert(
            "a-else".into(),
            FileEntry {
                document: DocumentId(uuid::Uuid::from_u128(2)),
                ..entry.clone()
            },
        );
        changed.files.insert(
            "a/file".into(),
            FileEntry {
                document: DocumentId(uuid::Uuid::from_u128(3)),
                ..entry.clone()
            },
        );
        assert!(changed.validate().is_err());
        changed.files.remove("a/file");
        changed.files.insert("duplicate".into(), entry);
        assert!(changed.validate().is_err());
    }
}
