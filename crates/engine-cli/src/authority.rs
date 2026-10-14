//! Receipt-authority epoch commands (docs/REF-AUTHORITY-ROTATION.md).
//!
//! `rotate-ref-authority` is offline: it reads an operator state file and private
//! keys, writes a new statement file and never contacts a database. The install,
//! show and revoke commands use the separate migration login.
use crate::signing::load as key;
use clap::Args;
use engine_core::repository::{
    attestation::SignedReceipt,
    authority_epoch::{
        Chain, Kind, Revocation, Served, SignedTransition, Transition, MAX_STATEMENT_BYTES,
    },
    server_authority::{self, State},
};
use ring::signature::KeyPair;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};
use uuid::Uuid;

#[derive(Args)]
pub struct Rotate {
    /// State saved by `show-ref-authority --out`. Statements are valid only for
    /// the seal in this file, so export it with receipt writers stopped.
    #[arg(long)]
    state: PathBuf,
    /// Private key of the active epoch. Not used with --recovery.
    #[arg(
        long,
        required_unless_present = "recovery",
        conflicts_with = "recovery"
    )]
    old_key: Option<PathBuf>,
    /// Private key of the new epoch (always required).
    #[arg(long)]
    new_key: PathBuf,
    /// Statement to write; created exclusively, never overwritten.
    #[arg(long)]
    out: PathBuf,
    /// The old key is lost or untrusted: sign only with the new key. Records a break.
    #[arg(long)]
    recovery: bool,
    /// Plain-language reason (required for --recovery; at most 200 bytes).
    #[arg(long)]
    reason: Option<String>,
}

/// An offline bundle: the authority's served epoch chain plus the receipts after a
/// pin. `receipt advance` and `receipt accept-break` verify everything locally;
/// the file itself carries no trust.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChainProof {
    pub version: u8,
    pub authority: Uuid,
    pub active_key: String,
    pub active_epoch: u32,
    pub revoked: Vec<Revocation>,
    pub rotations: Vec<SignedTransition>,
    pub receipts: Vec<SignedReceipt>,
}
impl ChainProof {
    pub fn served(&self) -> anyhow::Result<Served> {
        anyhow::ensure!(self.version == 1, "unsupported authority proof version");
        let mut revoked = std::collections::BTreeMap::new();
        for marker in &self.revoked {
            anyhow::ensure!(
                revoked.insert(marker.epoch, marker.clone()).is_none(),
                "duplicate revocation marker"
            );
        }
        let chain = Chain::new(self.authority, self.rotations.clone())?;
        Ok(Served::new(
            chain,
            self.active_key.clone(),
            self.active_epoch,
            revoked,
        )?)
    }
}

fn read(path: &Path, limit: usize) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(bytes.len() <= limit, "input exceeds byte limit");
    Ok(bytes)
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn create_new(path: &Path, bytes: &[u8]) -> anyhow::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => anyhow::anyhow!(
                "{} already exists; statements and state files are never overwritten",
                path.display()
            ),
            _ => e.into(),
        })?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// Offline dual signing (or recovery signing). Refuses to overwrite `out`.
pub fn rotate(args: Rotate) -> anyhow::Result<()> {
    let state: State = serde_json::from_slice(&read(&args.state, 64 * 1024)?)?;
    anyhow::ensure!(state.version == 1, "unsupported state file version");
    let new = key(&args.new_key)?;
    let new_public = hex(new.public_key().as_ref());
    anyhow::ensure!(
        !state.epochs.iter().any(|e| e.public_key == new_public) && new_public != state.public_key,
        "the new key is already used by an earlier epoch; every epoch needs a fresh key"
    );
    let kind = if args.recovery {
        Kind::Recovery
    } else {
        Kind::Rotation
    };
    match (kind, state.revocation.is_some()) {
        (Kind::Rotation, true) => anyhow::bail!(
            "epoch {} is revoked: a dual-signed rotation is not available; use --recovery",
            state.epoch
        ),
        (Kind::Recovery, false) => anyhow::bail!(
            "record the revocation first (revoke-ref-authority) so the break has a visible cause"
        ),
        _ => {}
    }
    let reason = args.reason.unwrap_or_default();
    let transition = Transition {
        version: 1,
        kind,
        authority: state.authority,
        from_epoch: state.epoch,
        to_epoch: state.epoch + 1,
        old_key: state.public_key.clone(),
        new_key: new_public,
        sealed: state.sealed.clone(),
        reason,
        declared_at_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_millis() as i64,
    };
    let signed = match kind {
        Kind::Rotation => {
            let old = key(args.old_key.as_ref().expect("clap requires --old-key"))?;
            anyhow::ensure!(
                hex(old.public_key().as_ref()) == state.public_key,
                "--old-key is not the active epoch's key"
            );
            SignedTransition::sign_rotation(transition, &old, &new)?
        }
        Kind::Recovery => SignedTransition::sign_recovery(transition, &new)?,
    };
    let address = signed.verify()?;
    let bytes = serde_json::to_vec_pretty(&signed)?;
    anyhow::ensure!(
        bytes.len() <= 2 * MAX_STATEMENT_BYTES,
        "statement byte limit"
    );
    create_new(&args.out, &bytes)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "kind": kind.as_str(),
            "from_epoch": signed.transition.from_epoch,
            "to_epoch": signed.transition.to_epoch,
            "statement_address": address,
            "sealed": signed.transition.sealed,
            "next": "install with the matching install-ref-authority-* command, then mount the new key and restart the API",
        }))?
    );
    Ok(())
}

fn statement(path: &Path) -> anyhow::Result<SignedTransition> {
    Ok(serde_json::from_slice(&read(
        path,
        2 * MAX_STATEMENT_BYTES,
    )?)?)
}

pub async fn install_rotation(pool: &PgPool, path: &Path) -> anyhow::Result<()> {
    let installed = server_authority::install_rotation(pool, &statement(path)?).await?;
    println!(
        "{}",
        if installed.already_installed {
            format!(
                "Epoch {} was already installed from this statement; nothing changed.",
                installed.epoch
            )
        } else {
            format!(
                "Installed authority epoch {} ({}). Mount the matching key and restart the API; clients that trust the previous key follow the signed statement.",
                installed.epoch, installed.statement_address
            )
        }
    );
    Ok(())
}

pub async fn install_recovery(
    pool: &PgPool,
    path: &Path,
    confirm_break: bool,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        confirm_break,
        "this records a break in the receipt chain: every client must re-pin out of band. Re-run with --confirm-break to proceed"
    );
    let installed = server_authority::install_recovery(pool, &statement(path)?).await?;
    println!(
        "Recorded authority break; epoch {} ({}) is active. Tell clients through an independent channel; none will accept it automatically.",
        installed.epoch, installed.statement_address
    );
    Ok(())
}

pub async fn show(pool: &PgPool, out: Option<&Path>) -> anyhow::Result<()> {
    let state = server_authority::state(pool).await?;
    let text = serde_json::to_string_pretty(&state)?;
    if let Some(out) = out {
        create_new(out, text.as_bytes())?;
    }
    println!("{text}");
    Ok(())
}

pub async fn revoke(
    pool: &PgPool,
    reason: &str,
    effective_at: Option<&str>,
    epoch: Option<u32>,
) -> anyhow::Result<()> {
    if let Some(time) = effective_at {
        // An explicit offset keeps the marker independent of the server time zone.
        let zone = time.ends_with('Z')
            || time
                .get(time.len().saturating_sub(6)..)
                .is_some_and(|tail| {
                    let b = tail.as_bytes();
                    b.len() == 6 && (b[0] == b'+' || b[0] == b'-') && b[3] == b':'
                });
        anyhow::ensure!(
            time.len() >= 20 && time.as_bytes()[10] == b'T' && zone,
            "--effective-at must be RFC 3339 with an explicit zone, for example 2026-10-14T09:30:00Z"
        );
    }
    let marker = server_authority::revoke(pool, reason, effective_at, epoch).await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "revoked_epoch": marker.epoch,
            "effective_at_ms": marker.effective_at_ms,
            "recorded_at_ms": marker.recorded_at_ms,
            "reason": marker.reason,
            "next": "receipt issuance for this epoch is stopped; install a recovery epoch with rotate-ref-authority --recovery and install-ref-authority-recovery",
        }))?
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use engine_core::repository::{
        authority_epoch::Seal,
        server_authority::{State, StateEpoch},
    };
    use ring::signature::Ed25519KeyPair;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("dynodoc-authority-{}-{name}", uuid::Uuid::new_v4()))
    }
    fn key_file(seed: u8) -> (PathBuf, String) {
        let mut der = vec![
            0x30, 0x2e, 0x02, 0x01, 0x00, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x04, 0x22,
            0x04, 0x20,
        ];
        der.extend([seed; 32]);
        let public = hex(Ed25519KeyPair::from_seed_unchecked(&[seed; 32])
            .unwrap()
            .public_key()
            .as_ref());
        let path = temp(&format!("key{seed}.der"));
        crate::private_file::write_private_test(&path, der).unwrap();
        (path, public)
    }
    fn state(public: &str, revoked: bool) -> State {
        State {
            version: 1,
            authority: uuid::Uuid::from_u128(5),
            epoch: 1,
            public_key: public.into(),
            sealed: Seal {
                receipts: 0,
                digest: "00".repeat(32),
            },
            revocation: revoked.then(|| engine_core::repository::server_authority::Revocation {
                epoch: 1,
                effective_at_ms: 1,
                recorded_at_ms: 2,
                reason: "test".into(),
            }),
            epochs: vec![StateEpoch {
                epoch: 1,
                kind: "genesis".into(),
                public_key: public.into(),
                installed_at_ms: 0,
                statement_address: None,
                revoked,
            }],
        }
    }
    #[test]
    fn offline_signing_binds_keys_state_and_refuses_overwrite() {
        let (old, old_public) = key_file(7);
        let (new, new_public) = key_file(8);
        let (other, _) = key_file(9);
        let state_path = temp("state.json");
        crate::private_file::write_private_test(
            &state_path,
            serde_json::to_vec(&state(&old_public, false)).unwrap(),
        )
        .unwrap();
        let out = temp("rotation.json");
        let args = |old_key: Option<&PathBuf>, new_key: &PathBuf, out: &PathBuf| Rotate {
            state: state_path.clone(),
            old_key: old_key.cloned(),
            new_key: new_key.clone(),
            out: out.clone(),
            recovery: false,
            reason: None,
        };
        // Wrong old key, reused key and a missing state are refused without output.
        assert!(rotate(args(Some(&other), &new, &out)).is_err());
        assert!(rotate(args(Some(&old), &old, &out)).is_err());
        assert!(!out.exists());
        rotate(args(Some(&old), &new, &out)).unwrap();
        let signed: SignedTransition =
            serde_json::from_slice(&std::fs::read(&out).unwrap()).unwrap();
        signed.verify().unwrap();
        assert_eq!(signed.transition.old_key, old_public);
        assert_eq!(signed.transition.new_key, new_public);
        assert_eq!(signed.transition.kind, Kind::Rotation);
        assert!(
            rotate(args(Some(&old), &new, &out)).is_err(),
            "an existing statement is never overwritten"
        );
        // Recovery is only possible after a marker and signs with the new key alone.
        let recovery_out = temp("recovery.json");
        let mut recovery = args(None, &new, &recovery_out);
        recovery.recovery = true;
        recovery.reason = Some("key lost".into());
        assert!(rotate(Rotate {
            ..recovery_clone(&recovery)
        })
        .is_err());
        std::fs::remove_file(&state_path).unwrap();
        crate::private_file::write_private_test(
            &state_path,
            serde_json::to_vec(&state(&old_public, true)).unwrap(),
        )
        .unwrap();
        assert!(rotate(args(Some(&old), &new, &temp("blocked.json"))).is_err());
        rotate(recovery).unwrap();
        let recovered: SignedTransition =
            serde_json::from_slice(&std::fs::read(&recovery_out).unwrap()).unwrap();
        recovered.verify().unwrap();
        assert!(recovered.old_signature.is_none());
        assert_eq!(recovered.transition.kind, Kind::Recovery);
        for path in [old, new, other, state_path, out, recovery_out] {
            let _ = std::fs::remove_file(path);
        }
    }
    fn recovery_clone(r: &Rotate) -> Rotate {
        Rotate {
            state: r.state.clone(),
            old_key: r.old_key.clone(),
            new_key: r.new_key.clone(),
            out: temp("unmarked.json"),
            recovery: r.recovery,
            reason: r.reason.clone(),
        }
    }
}
