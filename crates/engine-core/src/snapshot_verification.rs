//! Legacy and shared-root snapshots are derived caches, not sources of truth.
//!
//! Ordinary reads require a receipt over the stored envelope and check its chain
//! position. Upgrade/backfill and explicit audits replay from genesis in one pass,
//! comparing every snapshot (including comments/suggestions/tombstones).
//! Receipts are internal attestations, not signatures against a database owner.

use std::io::{self, Write};

use engine_shared::{DocumentId, Event, Snapshot};
use futures::TryStreamExt;
use ring::digest::{Context, SHA256};
use serde::Deserialize;
use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use crate::{
    log::{self, ZERO_HASH},
    materializer::{DocumentState, Materializer},
    shared_checkpoint::{
        canonical::CanonicalValue,
        postgres::{self, chain_at, PgStore},
        typed, Address, Limits,
    },
    snapshot::{merkle_root, NamedStorage, SnapshotError},
};

fn invalid(snapshot: &Snapshot, detail: &str) -> SnapshotError {
    SnapshotError::Unverified {
        id: snapshot.id.0,
        seq: snapshot.through_seq,
        detail: detail.into(),
    }
}

fn receipt_version(snapshot: &Snapshot) -> i16 {
    if snapshot.state.get("$dynodoc_state").is_some() {
        2
    } else {
        1
    }
}

/// Strict reference recognition; a malformed/new version never falls back to JSON.
fn shared_root(snapshot: &Snapshot) -> Result<Option<Address>, SnapshotError> {
    let value = &snapshot.state;
    if value.get("$dynodoc_state").is_none() {
        return Ok(None);
    }
    if value["$dynodoc_state"] != 2 || value.as_object().is_none_or(|o| o.len() != 2) {
        return Err(invalid(snapshot, "invalid shared state reference"));
    }
    let root = value["root"]
        .as_str()
        .ok_or_else(|| invalid(snapshot, "missing shared state root"))?;
    Ok(Some(Address::try_from(root.to_owned())?))
}

async fn materialize(
    connection: &mut PgConnection,
    snapshot: &Snapshot,
) -> Result<DocumentState, SnapshotError> {
    if let Some(root) = shared_root(snapshot)? {
        Ok(typed::read_state(
            &mut PgStore::new(connection),
            snapshot.document_id,
            &root,
            Limits::default(),
            Default::default(),
        )
        .await?)
    } else {
        Ok(DocumentState::deserialize(&snapshot.state)?)
    }
}

/// Verify the stored envelope before decoding its graph. Never fingerprint the
/// hydrated response: v2 attests the immutable reference and v1 the inline state.
pub(crate) async fn read_verified_state(
    connection: &mut PgConnection,
    snapshot: &Snapshot,
) -> Result<DocumentState, SnapshotError> {
    check_receipt(connection, snapshot).await?;
    let state = materialize(connection, snapshot).await?;
    if snapshot.merkle_root != merkle_root(&state)? {
        return Err(invalid(
            snapshot,
            "node Merkle root differs from stored state",
        ));
    }
    Ok(state)
}

/// Hash borrowed canonical JSON directly into SHA-256. No second encoded state
/// buffer; object key-reference sorting and the resident snapshot still cost memory.
fn fingerprint(snapshot: &Snapshot) -> Result<Vec<u8>, serde_json::Error> {
    struct HashWriter(Context);
    impl Write for HashWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut writer = HashWriter(Context::new(&SHA256));
    // Versioned, ordered envelope. JSON objects in state are recursively sorted;
    // arrays retain their stored order, including the legacy removed-choice set.
    serde_json::to_writer(
        &mut writer,
        &(
            if receipt_version(snapshot) == 1 {
                "dynodoc.legacy-snapshot.v1"
            } else {
                "dynodoc.shared-snapshot.v2"
            },
            snapshot.id,
            snapshot.document_id,
            snapshot.through_seq,
            &snapshot.merkle_root,
            &snapshot.event_chain_hash,
            snapshot.created_at,
            CanonicalValue(&snapshot.state),
        ),
    )?;
    Ok(writer.0.finish().as_ref().to_vec())
}

pub(crate) async fn check_receipt(
    connection: &mut PgConnection,
    snapshot: &Snapshot,
) -> Result<(), SnapshotError> {
    let receipt: Option<(Uuid, i64, i16, Vec<u8>)> = sqlx::query_as(
        "select document_id,through_seq,version,fingerprint from snapshot_verification where snapshot_id=$1",
    ).bind(snapshot.id.0).fetch_optional(&mut *connection).await?;
    let Some((document, seq, version, hash)) = receipt else {
        return Err(invalid(
            snapshot,
            "no replay verification receipt; run verify-snapshots",
        ));
    };
    if document != snapshot.document_id.0
        || seq != snapshot.through_seq
        || version != receipt_version(snapshot)
        || hash != fingerprint(snapshot)?
    {
        return Err(invalid(
            snapshot,
            "stored envelope does not match its verification receipt",
        ));
    }
    if snapshot.through_seq < 0
        || snapshot.event_chain_hash
            != chain_at(connection, snapshot.document_id, snapshot.through_seq).await?
    {
        return Err(invalid(snapshot, "event chain position does not match"));
    }
    Ok(())
}

/// Caller must authorize the document. Never returns unchecked snapshot content.
pub async fn load(
    connection: &mut PgConnection,
    document: DocumentId,
    snapshot_id: Uuid,
) -> Result<Option<Snapshot>, SnapshotError> {
    let mut snapshot: Option<Snapshot> =
        sqlx::query_as("select * from snapshot where id=$1 and document_id=$2")
            .bind(snapshot_id)
            .bind(document.0)
            .fetch_optional(&mut *connection)
            .await?;
    if let Some(ref mut snapshot) = snapshot {
        let state = read_verified_state(connection, snapshot).await?;
        // Preserve legacy JSON exactly, including optional fields and set order.
        if receipt_version(snapshot) == 2 {
            snapshot.state = serde_json::to_value(state)?;
        }
    }
    Ok(snapshot)
}

async fn compare(
    connection: &mut PgConnection,
    snapshot: &Snapshot,
    state: &DocumentState,
    chain: &[u8],
) -> Result<(), SnapshotError> {
    // Serde normally ignores unknown struct fields. Reject them at this trust
    // boundary: otherwise backfill could attest bytes that replay never produced
    // and a JSON download/another consumer could still expose. Arbitrary document
    // content inside current_fields and suggestion detail remains supported.
    let known = |value: &serde_json::Value, fields: &[&str]| {
        value
            .as_object()
            .is_some_and(|object| object.keys().all(|key| fields.contains(&key.as_str())))
    };
    let value = &snapshot.state;
    let shape_ok = known(
        value,
        &["nodes", "comments", "suggestions", "removed_choices"],
    ) && value["nodes"].as_object().is_some_and(|nodes| {
        nodes.values().all(|node| {
            known(
                node,
                &[
                    "id",
                    "parent_id",
                    "type",
                    "pos",
                    "current_fields",
                    "var_name",
                    "deleted",
                ],
            )
        })
    }) && value.get("comments").is_none_or(|comments| {
        comments.as_array().is_some_and(|comments| {
            comments
                .iter()
                .all(|comment| known(comment, &["node_id", "body"]))
        })
    }) && value.get("suggestions").is_none_or(|suggestions| {
        suggestions.as_object().is_some_and(|suggestions| {
            suggestions.values().all(|suggestion| {
                known(
                    suggestion,
                    &["target_node_id", "detail", "accepted", "rejected_reason"],
                )
            })
        })
    });
    if shared_root(snapshot)?.is_none() && !shape_ok {
        return Err(invalid(
            snapshot,
            "state contains unsupported fields or structure",
        ));
    }
    let stored = materialize(connection, snapshot).await?;
    if &stored != state {
        return Err(invalid(
            snapshot,
            "state differs from canonical event replay",
        ));
    }
    if snapshot.merkle_root != merkle_root(state)? {
        return Err(invalid(snapshot, "node Merkle root differs from replay"));
    }
    if snapshot.event_chain_hash != chain {
        return Err(invalid(
            snapshot,
            "event chain position differs from replay",
        ));
    }
    Ok(())
}

async fn record(connection: &mut PgConnection, snapshot: &Snapshot) -> Result<(), SnapshotError> {
    sqlx::query(
        "insert into snapshot_verification(snapshot_id,document_id,through_seq,fingerprint,version)
        values($1,$2,$3,$4,$5) on conflict(snapshot_id) do nothing",
    )
    .bind(snapshot.id.0)
    .bind(snapshot.document_id.0)
    .bind(snapshot.through_seq)
    .bind(fingerprint(snapshot)?)
    .bind(receipt_version(snapshot))
    .execute(&mut *connection)
    .await?;
    // A conflicting receipt is never overwritten, even if an operator has changed
    // the row and wants to bless it again. Treat it as an integrity failure.
    check_receipt(connection, snapshot).await
}

/// Publish a snapshot from independently reconstructed canonical history. Caller
/// owns the transaction and document lock. No arbitrary caller-supplied state is
/// attested. Previously verified snapshot receipts bound the replay suffix;
/// periodic shared manifests aren't used as independent replay evidence here.
pub async fn take_in_connection(
    connection: &mut PgConnection,
    document: DocumentId,
    through_seq: i64,
) -> Result<Snapshot, SnapshotError> {
    take_with_storage_in_connection(connection, document, through_seq, NamedStorage::Legacy).await
}

/// Explicit writer selection for named versions, public pins and deployments.
/// The caller owns a transaction and document lock. Existing revisions retain
/// their original format. Returned snapshots always contain hydrated state.
pub async fn take_with_storage_in_connection(
    connection: &mut PgConnection,
    document: DocumentId,
    through_seq: i64,
    storage: NamedStorage,
) -> Result<Snapshot, SnapshotError> {
    if through_seq < 0 {
        return Err(SnapshotError::InvalidRevision(through_seq));
    }
    let existing: Option<Uuid> =
        sqlx::query_scalar("select id from snapshot where document_id=$1 and through_seq=$2")
            .bind(document.0)
            .bind(through_seq)
            .fetch_optional(&mut *connection)
            .await?;
    if let Some(id) = existing {
        return load(connection, document, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound.into());
    }
    // Use the same lock order as periodic builders, including legacy writers:
    // a mixed concurrent publication cannot leave an unreferenced typed graph.
    sqlx::query("select pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(postgres::publication_lock_key(document))
        .execute(&mut *connection)
        .await?;
    let existing: Option<Uuid> =
        sqlx::query_scalar("select id from snapshot where document_id=$1 and through_seq=$2")
            .bind(document.0)
            .bind(through_seq)
            .fetch_optional(&mut *connection)
            .await?;
    if let Some(id) = existing {
        return load(connection, document, id)
            .await?
            .ok_or(sqlx::Error::RowNotFound.into());
    }
    let state = crate::snapshot::read_legacy_state_at(connection, document, through_seq)
        .await?
        .state;
    let chain = chain_at(connection, document, through_seq).await?;
    let encoded = match storage {
        NamedStorage::Legacy => serde_json::to_value(&state)?,
        NamedStorage::SharedV2 => {
            let (root, _) = typed::write_state(
                &mut PgStore::new(connection),
                document,
                &state,
                Limits::default(),
                Default::default(),
            )
            .await?;
            serde_json::json!({"$dynodoc_state":2,"root":root})
        }
    };
    sqlx::query(
        "insert into snapshot(document_id,through_seq,state,merkle_root,event_chain_hash)
        values($1,$2,$3,$4,$5) on conflict(document_id,through_seq) do nothing",
    )
    .bind(document.0)
    .bind(through_seq)
    .bind(encoded)
    .bind(merkle_root(&state)?.as_slice())
    .bind(&chain)
    .execute(&mut *connection)
    .await?;
    let mut snapshot: Snapshot =
        sqlx::query_as("select * from snapshot where document_id=$1 and through_seq=$2")
            .bind(document.0)
            .bind(through_seq)
            .fetch_one(&mut *connection)
            .await?;
    compare(connection, &snapshot, &state, &chain).await?;
    record(connection, &snapshot).await?;
    if receipt_version(&snapshot) == 2 {
        snapshot.state = serde_json::to_value(state)?;
    }
    Ok(snapshot)
}

#[derive(Debug, serde::Serialize)]
pub struct VerificationReport {
    pub through_seq: i64,
    pub snapshots: usize,
    pub replayed_events: usize,
}

/// Audit the event chain AND every snapshot independently of receipts and
/// periodic shared checkpoints. Backfill writes receipts after replay comparison; one
/// transaction per document rolls them all back on failure. Event rows stream and
/// snapshots are keyset-read one at a time, never O(history) retained rows.
pub async fn verify_document(
    pool: &PgPool,
    document: DocumentId,
    backfill: bool,
) -> Result<VerificationReport, SnapshotError> {
    let mut tx = pool.begin().await?;
    sqlx::query("set transaction isolation level repeatable read")
        .execute(&mut *tx)
        .await?;
    // Prevent whole-document erasure while checking; ordinary appends can proceed.
    sqlx::query("select id from document where id=$1 for key share")
        .bind(document.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(log::EventError::DocumentNotFound(document.0))?;
    let through_seq: i64 =
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(document.0)
            .fetch_one(&mut *tx)
            .await?;
    let mut state = DocumentState::default();
    let mut seq = 0;
    let mut chain = ZERO_HASH.to_vec();
    let mut last_snapshot: Option<i64> = None;
    let mut count = 0;
    loop {
        let next: Option<Snapshot> = sqlx::query_as(
            "select * from snapshot where document_id=$1 and ($2::bigint is null or through_seq>$2) order by through_seq limit 1",
        ).bind(document.0).bind(last_snapshot).fetch_optional(&mut *tx).await?;
        let target = next.as_ref().map_or(through_seq, |s| s.through_seq);
        if target > through_seq || target < seq {
            return Err(match &next {
                Some(snapshot) => invalid(
                    snapshot,
                    "revision lies outside the canonical event history",
                ),
                None => SnapshotError::InvalidRevision(target),
            });
        }
        {
            let mut events = sqlx::query_as::<_, Event>(
                "select * from event where document_id=$1 and seq>$2 and seq<=$3 order by seq",
            )
            .bind(document.0)
            .bind(seq)
            .bind(target)
            .fetch(&mut *tx);
            while let Some(event) = events.try_next().await? {
                log::verify_event(&event, document, seq + 1, &chain)?;
                Materializer::fold_into(&mut state, std::slice::from_ref(&event))?;
                seq = event.seq;
                chain = event.chain_hash;
            }
        }
        if seq != target {
            return Err(SnapshotError::InvalidRevision(target));
        }
        let Some(snapshot) = next else {
            break;
        };
        compare(&mut tx, &snapshot, &state, &chain).await?;
        if backfill {
            record(&mut tx, &snapshot).await?;
        } else {
            // An old unverified snapshot may pass independent audit; a receipt is
            // only needed to serve it. If present it must also be consistent.
            let exists: bool = sqlx::query_scalar(
                "select exists(select 1 from snapshot_verification where snapshot_id=$1)",
            )
            .bind(snapshot.id.0)
            .fetch_one(&mut *tx)
            .await?;
            if exists {
                check_receipt(&mut tx, &snapshot).await?;
            }
        }
        last_snapshot = Some(snapshot.through_seq);
        count += 1;
    }
    tx.commit().await?;
    Ok(VerificationReport {
        through_seq,
        snapshots: count,
        replayed_events: seq as usize,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_receipt_v1_has_a_stable_envelope_vector() {
        let snapshot = Snapshot {
            id: engine_shared::SnapshotId(Uuid::nil()),
            document_id: DocumentId(Uuid::from_u128(1)),
            through_seq: 0,
            state: serde_json::json!({"nodes":{},"comments":[],"suggestions":{},"removed_choices":[]}),
            merkle_root: vec![0; 32],
            event_chain_hash: vec![0; 32],
            created_at: "2026-10-11T00:00:00Z".parse().unwrap(),
        };
        // Independently encoded with sorted JSON keys and SHA-256.
        let hex: String = fingerprint(&snapshot)
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            hex,
            "1547c22166c50bf50d510d1093a86be40648d86ff23bcbd6e3bb8814b05063ec"
        );
    }

    #[test]
    fn shared_receipt_v2_has_a_stable_envelope_vector() {
        let snapshot = Snapshot {
            id: engine_shared::SnapshotId(Uuid::nil()),
            document_id: DocumentId(Uuid::from_u128(1)),
            through_seq: 0,
            state: serde_json::json!({"$dynodoc_state":2,"root":"0123456789abcdef".repeat(4)}),
            merkle_root: vec![0; 32],
            event_chain_hash: vec![0; 32],
            created_at: "2026-10-11T00:00:00Z".parse().unwrap(),
        };
        // Independent Python JSON/SHA-256 envelope, not the engine encoder.
        let hex: String = fingerprint(&snapshot)
            .unwrap()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            hex,
            "3108937000f9ea2d5fef24341ac085f2886808ba24f9197613ea603e23ff3d76"
        );
    }
}
