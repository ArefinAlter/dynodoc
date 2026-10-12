//! Offline signing of an enrollment challenge. Private keys never leave this process.
use clap::Subcommand;
use engine_core::repository::device::{Challenge, Proof, MAX_PROOF_BYTES};
use ring::{
    rand::SystemRandom,
    signature::{Ed25519KeyPair, KeyPair},
};
use std::{io::Read, path::PathBuf};
use uuid::Uuid;

#[derive(Subcommand)]
pub enum Action {
    /// Create a new unencrypted DER PKCS#8 Ed25519 signing key and print its public
    /// key. Never overwrites a file. The file is the long-lived credential for
    /// `sync --device-key`; keep it in a private directory and enroll it in your profile.
    Keygen {
        #[arg(long)]
        out: PathBuf,
    },
    /// Display the public key to enroll through your authenticated profile.
    PublicKey { key: PathBuf },
    /// Prove possession for the installation/account shown in your profile.
    Prove {
        challenge: PathBuf,
        #[arg(long)]
        authority: Uuid,
        #[arg(long)]
        identity: Uuid,
        #[arg(long)]
        key: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
}

/// Generate a key with the system CSPRNG, store it exclusively (mode 0600 on Unix)
/// and return its lowercase hexadecimal public key. The private key is never printed.
fn keygen(out: &std::path::Path) -> anyhow::Result<String> {
    let pkcs8 = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
        .map_err(|_| anyhow::anyhow!("system random generator failed"))?;
    // The written bytes must load through the same parser every signing command uses.
    let key = Ed25519KeyPair::from_pkcs8(pkcs8.as_ref())
        .map_err(|_| anyhow::anyhow!("generated key failed validation"))?;
    crate::private_file::write_new(out, pkcs8.as_ref())?;
    Ok(key
        .public_key()
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

pub fn run(action: Action) -> anyhow::Result<()> {
    match action {
        Action::Keygen { out } => println!("{}", keygen(&out)?),
        Action::PublicKey { key } => {
            let key = crate::signing::load(&key)?;
            let public_key: String = key
                .public_key()
                .as_ref()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            println!("{public_key}");
        }
        Action::Prove {
            challenge,
            authority,
            identity,
            key,
            out,
        } => {
            let mut bytes = Vec::new();
            std::fs::File::open(challenge)?
                .take(MAX_PROOF_BYTES as u64 + 1)
                .read_to_end(&mut bytes)?;
            anyhow::ensure!(bytes.len() <= MAX_PROOF_BYTES, "challenge too large");
            let challenge: Challenge = serde_json::from_slice(&bytes)?;
            anyhow::ensure!(
                challenge.expires_at_ms as u128
                    > std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_millis(),
                "challenge expired; request a fresh challenge"
            );
            let proof = Proof::sign(challenge, authority, identity, &crate::signing::load(&key)?)?;
            // Never overwrite a key or an unrelated existing file by mistake.
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(out)?;
            serde_json::to_writer_pretty(file, &proof)?;
            println!("Enrollment proof saved. Submit it through the same signed-in profile.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keygen_creates_a_loadable_private_key_once() {
        let dir = std::env::temp_dir().join(format!("dynodoc-keygen-{}", Uuid::new_v4()));
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("signing-key.der");
        let public = keygen(&path).unwrap();
        assert_eq!(public.len(), 64);
        let loaded = crate::signing::load(&path).unwrap();
        assert_eq!(
            public,
            loaded
                .public_key()
                .as_ref()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        // A second run neither overwrites the key nor prints another one.
        let before = std::fs::read(&path).unwrap();
        assert!(keygen(&path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert_ne!(keygen(&dir.join("another.der")).unwrap(), public);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
