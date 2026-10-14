//! Offline receipt tools. Journal state stays separate from project data. No
//! command connects to a server, registers an account key or publishes a branch.
use crate::signing::load as key;
use clap::Subcommand;
use engine_core::repository::{
    attestation::{Pin, Receipt, Scope, SignedReceipt, SignedRotation},
    authority_epoch::{Kind, Refusal},
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
    /// Atomically accept a contiguous JSON array of signed receipts (max 256), or an
    /// authority proof file from `sync ... authority-proof` that also carries the
    /// epoch statements. Dual-signed succession is followed; a recorded break or a
    /// revocation marker is refused.
    Advance { proof: PathBuf },
    /// Explicit re-pin across a recorded break. NEW_KEY must come from outside the
    /// server. The first new receipt must extend this pin; the break is kept visible.
    AcceptBreak {
        proof: PathBuf,
        new_key: String,
        #[arg(long)]
        confirm_break: bool,
    },
    /// List the authority statements (rotations and recorded breaks) this journal accepted.
    Authority,
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
use crate::authority::ChainProof;

/// Follow the bundle's dual-signed statements from the pinned key. Everything is
/// verified locally; a marker on the pinned key's epoch revokes this pin for good.
async fn advance_bundle(
    journal: &ReceiptJournal,
    scope: &Scope,
    bundle: &ChainProof,
) -> anyhow::Result<serde_json::Value> {
    anyhow::ensure!(
        bundle.authority == scope.authority,
        "proof names a different authority"
    );
    let served = bundle.served()?;
    let pin = journal
        .pin(scope)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no trusted pin for this scope"))?;
    anyhow::ensure!(!pin.revoked(), "receipt authority revoked");
    match served.vet(pin.trusted_key(), &served.active_key) {
        Ok(()) => {}
        Err(Refusal::Revoked { marker, own }) => {
            if own {
                journal.revoke(scope).await?;
            }
            anyhow::bail!("{}", Refusal::Revoked { marker, own });
        }
        Err(refusal) => anyhow::bail!("{refusal}"),
    }
    Ok(serde_json::to_value(
        journal
            .advance_chain(scope, &served.chain, &bundle.receipts)
            .await?,
    )?)
}

/// Explicit re-pin: the break statement must follow the pinned key, the new key
/// is supplied by the caller, and the next receipt must extend the pin.
async fn accept_break(
    journal: &ReceiptJournal,
    scope: &Scope,
    bundle: &ChainProof,
    new_key: &str,
) -> anyhow::Result<serde_json::Value> {
    anyhow::ensure!(
        bundle.authority == scope.authority,
        "proof names a different authority"
    );
    let served = bundle.served()?;
    let pin = journal
        .pin(scope)
        .await?
        .ok_or_else(|| anyhow::anyhow!("no trusted pin for this scope"))?;
    let statement = served
        .chain
        .statements()
        .iter()
        .find(|s| s.transition.kind == Kind::Recovery && s.transition.old_key == pin.trusted_key())
        .ok_or_else(|| anyhow::anyhow!("no recorded break follows the pinned key"))?;
    anyhow::ensure!(
        statement.transition.new_key == new_key,
        "the recorded break names a different key than NEW_KEY; do not accept it"
    );
    // Later epochs must be unrevoked and unbroken from the new key onward.
    served
        .vet(new_key, &served.active_key)
        .map_err(|refusal| anyhow::anyhow!("{refusal}"))?;
    let generation = pin.receipt().receipt.head.generation + 1;
    let first = bundle
        .receipts
        .iter()
        .find(|r| r.receipt.head.generation == generation && r.receipt.issuer_key == new_key)
        .ok_or_else(|| {
            anyhow::anyhow!("the proof has no receipt by NEW_KEY at generation {generation}")
        })?;
    let mut result = journal
        .accept_break(scope, statement, new_key, first)
        .await?;
    let rest: Vec<_> = bundle
        .receipts
        .iter()
        .filter(|r| r.receipt.head.generation > generation)
        .cloned()
        .collect();
    if !rest.is_empty() {
        result = journal.advance_chain(scope, &served.chain, &rest).await?;
    }
    Ok(serde_json::to_value(result)?)
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
                        let bytes = read(&proof, 4 * 1024 * 1024)?;
                        // A bare array is the original receipt-only proof.
                        match serde_json::from_slice::<Vec<SignedReceipt>>(&bytes) {
                            Ok(receipts) => {
                                serde_json::to_value(journal.advance(&scope, &receipts).await?)?
                            }
                            Err(_) => {
                                let bundle: ChainProof = serde_json::from_slice(&bytes)?;
                                advance_bundle(&journal, &scope, &bundle).await?
                            }
                        }
                    }
                    Action::AcceptBreak {
                        proof,
                        new_key,
                        confirm_break,
                    } => {
                        anyhow::ensure!(
                            confirm_break,
                            "this trusts a new authority key after a recorded break; re-run with --confirm-break once NEW_KEY was obtained independently of the server"
                        );
                        let bundle: ChainProof = json(&proof, 4 * 1024 * 1024)?;
                        accept_break(&journal, &scope, &bundle, &new_key).await?
                    }
                    Action::Authority => {
                        serde_json::json!({"statements": journal.statements().await?})
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
