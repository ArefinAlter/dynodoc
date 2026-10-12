//! Offline signing of an enrollment challenge. Private keys never leave this process.
use clap::Subcommand;
use engine_core::repository::device::{Challenge, Proof, MAX_PROOF_BYTES};
use ring::signature::KeyPair;
use std::{io::Read, path::PathBuf};
use uuid::Uuid;

#[derive(Subcommand)]
pub enum Action {
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

pub fn run(action: Action) -> anyhow::Result<()> {
    match action {
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
