//! Offline receipt tools. Journal state stays separate from project data. No
//! command connects to a server, registers an account key or publishes a branch.
use crate::signing::load as key;
use clap::Subcommand;
use engine_core::repository::{
    attestation::{Pin, Receipt, Scope, SignedReceipt, SignedRotation},
    receipt_journal::ReceiptJournal,
};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Subcommand)]
pub enum Action {
    /// Sign an operator-supplied manifest; does not assert server acceptance.
    Sign { manifest: PathBuf, key: PathBuf },
    /// Verify the signature and explicit scope/key without changing a pin.
    Verify {
        receipt: PathBuf,
        trusted_key: String,
    },
    /// Explicit first trust. Obtain the authority key independently of the receipt.
    Trust {
        receipt: PathBuf,
        trusted_key: String,
    },
    /// Atomically accept a contiguous JSON array of signed receipts (max 256).
    Advance { proof: PathBuf },
    /// Read the saved pin and local revocation state.
    Show,
    /// Permanently revoke this local pin; receipt retries cannot revive it.
    Revoke,
    /// Sign a key transition at the current pin using both old and new keys.
    SignRotation { old_key: PathBuf, new_key: PathBuf },
    /// Accept a dual-signed transition and the next receipt in one transaction.
    Rotate {
        transition: PathBuf,
        receipt: PathBuf,
    },
}
fn read(path: &Path, limit: usize) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= limit,
        "receipt input exceeds command byte limit"
    );
    Ok(bytes)
}
fn json<T: serde::de::DeserializeOwned>(path: &Path, limit: usize) -> anyhow::Result<T> {
    Ok(serde_json::from_slice(&read(path, limit)?)?)
}
pub async fn run(path: PathBuf, scope: Scope, action: Action) -> anyhow::Result<()> {
    let output = match action {
        Action::Sign {
            manifest,
            key: path,
        } => {
            let receipt: Receipt = json(&manifest, 8192)?;
            anyhow::ensure!(
                receipt.scope == scope,
                "manifest does not match the explicit scope"
            );
            serde_json::to_value(SignedReceipt::sign(receipt, &key(&path)?)?)?
        }
        Action::Verify {
            receipt,
            trusted_key,
        } => {
            let pin = Pin::trust_checkpoint(&scope, &trusted_key, json(&receipt, 8192)?)?;
            serde_json::json!({"receipt":pin.receipt().verify()?,"scope":scope,"signature_valid":true})
        }
        action => {
            let journal = ReceiptJournal::open(&path).await?;
            let result: anyhow::Result<serde_json::Value> = async {
                Ok(match action {
                    Action::Trust {
                        receipt,
                        trusted_key,
                    } => serde_json::to_value(
                        journal
                            .trust_checkpoint(&scope, &trusted_key, json(&receipt, 8192)?)
                            .await?,
                    )?,
                    Action::Advance { proof } => {
                        let proof: Vec<SignedReceipt> = json(&proof, 2 * 1024 * 1024)?;
                        serde_json::to_value(journal.advance(&scope, &proof).await?)?
                    }
                    Action::Show => serde_json::to_value(journal.pin(&scope).await?)?,
                    Action::Revoke => serde_json::to_value(journal.revoke(&scope).await?)?,
                    Action::SignRotation { old_key, new_key } => {
                        let pin = journal
                            .pin(&scope)
                            .await?
                            .ok_or_else(|| anyhow::anyhow!("no trusted pin for this scope"))?;
                        serde_json::to_value(SignedRotation::sign(
                            &pin,
                            &key(&old_key)?,
                            &key(&new_key)?,
                        )?)?
                    }
                    Action::Rotate {
                        transition,
                        receipt,
                    } => serde_json::to_value(
                        journal
                            .rotate(&scope, &json(&transition, 8192)?, &json(&receipt, 8192)?)
                            .await?,
                    )?,
                    Action::Sign { .. } | Action::Verify { .. } => unreachable!(),
                })
            }
            .await;
            journal.close().await;
            result?
        }
    };
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}
