//! Snapshot engine (docs/15).

use engine_shared::{DocumentId, Event, EventPayload, Snapshot};
use futures::TryStreamExt;
use ring::digest::{Context, SHA256};
use sqlx::{PgConnection, PgPool};
use tracing::warn;

use crate::{
    log::{self, ZERO_HASH},
    materializer::{DocumentState, MaterializeError, MaterializedNode, Materializer},
    shared_checkpoint::{self, postgres as shared, Limits},
};

/// Default tail length before an automatic background snapshot is taken.
pub const DEFAULT_MAX_EVENTS_IN_TAIL: i64 = 500;

/// Why a snapshot was requested.
#[derive(Debug, Clone, Copy)]
pub enum SnapshotReason {
    Periodic,
    Deployed,
}

impl SnapshotReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Periodic => "periodic",
            Self::Deployed => "deployed",
        }
    }
}

/// Errors from snapshot reads/writes.
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("shared checkpoint failed: {0}")]
    Shared(Box<shared_checkpoint::Error>),
    #[error("revision {0} is outside the document history")]
    InvalidRevision(i64),
    #[error("event history gap: expected seq {expected}, found {actual}")]
    HistoryGap { expected: i64, actual: i64 },
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("event read failed: {0}")]
    Event(#[from] log::EventError),
    #[error("state materialization failed: {0}")]
    Materialize(#[from] MaterializeError),
    #[error("snapshot state serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
}

impl From<shared_checkpoint::Error> for SnapshotError {
    fn from(error: shared_checkpoint::Error) -> Self {
        Self::Shared(Box::new(error))
    }
}

/// Selected checkpoint representation; exposed for replay accounting, not the API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckpointFormat {
    Genesis,
    Legacy,
    SharedV1,
}

/// Automatic writes retain a reversible rollout boundary. Readers always support
/// both formats. Named/deployed snapshots continue to use the legacy contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeriodicStorage {
    Legacy,
    SharedV1,
}

/// A historical state and the work required to reconstruct it.
#[derive(Debug)]
pub struct HistoricalState {
    pub state: DocumentState,
    pub snapshot_seq: i64,
    pub checkpoint_format: CheckpointFormat,
    pub replayed_events: usize,
}

/// Read an exact revision from the nearest legacy/shared checkpoint and a streamed
/// event suffix. Ties prefer the verified shared graph. Corruption is an error,
/// never a reason to silently choose an older checkpoint. Full state stays resident.
/// Callers authorize document access and retain their existing transaction/locks.
pub async fn read_state_at(
    connection: &mut PgConnection,
    document_id: DocumentId,
    through_seq: i64,
) -> Result<HistoricalState, SnapshotError> {
    read_state_at_with_formats(connection, document_id, through_seq, true).await
}

/// Independent legacy/event reference for explicit shared checkpoint publication.
pub(crate) async fn read_legacy_state_at(
    connection: &mut PgConnection,
    document_id: DocumentId,
    through_seq: i64,
) -> Result<HistoricalState, SnapshotError> {
    read_state_at_with_formats(connection, document_id, through_seq, false).await
}

async fn read_state_at_with_formats(
    connection: &mut PgConnection,
    document_id: DocumentId,
    through_seq: i64,
    include_shared: bool,
) -> Result<HistoricalState, SnapshotError> {
    if through_seq < 0 {
        return Err(SnapshotError::InvalidRevision(through_seq));
    }
    // Metadata only: never hydrate an older full JSON snapshot just to compare it.
    let selected: Option<(i64, bool)> = sqlx::query_as(
        "select through_seq,shared from (
           (select through_seq,false as shared from snapshot
            where document_id=$1 and through_seq<=$2 order by through_seq desc limit 1)
           union all
           (select through_seq,true as shared from shared_checkpoint
            where document_id=$1 and through_seq<=$2 and $3 order by through_seq desc limit 1)
         ) candidates order by through_seq desc,shared desc limit 1",
    )
    .bind(document_id.0)
    .bind(through_seq)
    .bind(include_shared)
    .fetch_optional(&mut *connection)
    .await?;
    let (snapshot_seq, checkpoint_format, mut state) = match selected {
        Some((seq, true)) => (
            seq,
            CheckpointFormat::SharedV1,
            shared::load_in_connection(connection, document_id, seq, Limits::default()).await?,
        ),
        Some((seq, false)) => {
            let value = sqlx::query_scalar(
                "select state from snapshot where document_id=$1 and through_seq=$2",
            )
            .bind(document_id.0)
            .bind(seq)
            .fetch_one(&mut *connection)
            .await?;
            (
                seq,
                CheckpointFormat::Legacy,
                serde_json::from_value(value)?,
            )
        }
        None => (0, CheckpointFormat::Genesis, DocumentState::default()),
    };
    let mut tail = sqlx::query_as::<_, Event>(
        "select * from event where document_id=$1 and seq>$2 and seq<=$3 order by seq",
    )
    .bind(document_id.0)
    .bind(snapshot_seq)
    .bind(through_seq)
    .fetch(&mut *connection);
    let mut last_seq = snapshot_seq;
    let mut replayed_events = 0;
    while let Some(event) = tail.try_next().await? {
        if event.seq != last_seq + 1 {
            return Err(SnapshotError::HistoryGap {
                expected: last_seq + 1,
                actual: event.seq,
            });
        }
        Materializer::fold_into(&mut state, std::slice::from_ref(&event))?;
        last_seq = event.seq;
        replayed_events += 1;
    }
    if last_seq != through_seq {
        return Err(SnapshotError::InvalidRevision(through_seq));
    }
    Ok(HistoricalState {
        state,
        snapshot_seq,
        checkpoint_format,
        replayed_events,
    })
}

/// Pin the current immutable revision before selecting a checkpoint. Concurrent
/// appends are excluded from this read and delivered later by the existing SSE seq.
pub async fn read_current_state(
    connection: &mut PgConnection,
    document_id: DocumentId,
) -> Result<(DocumentState, i64), SnapshotError> {
    let seq = latest_event_seq(connection, document_id).await?;
    Ok((
        read_state_at(connection, document_id, seq).await?.state,
        seq,
    ))
}

async fn latest_event_seq(
    connection: &mut PgConnection,
    document_id: DocumentId,
) -> Result<i64, sqlx::Error> {
    sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
        .bind(document_id.0)
        .fetch_one(connection)
        .await
}

/// Advisory lock namespace for periodic builders. A collision only delays a
/// checkpoint; it cannot affect content authorization or canonical append locking.
async fn try_lock_periodic(
    connection: &mut PgConnection,
    document_id: DocumentId,
) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar("select pg_try_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("dynodoc.periodic-checkpoint:{}", document_id.0))
        .fetch_one(connection)
        .await
}

async fn checkpoint_due(
    connection: &mut PgConnection,
    document_id: DocumentId,
    max_events_in_tail: i64,
) -> Result<Option<i64>, sqlx::Error> {
    let (seq, last): (i64, i64) = sqlx::query_as(
        "select (select coalesce(max(seq),0) from event where document_id=$1),
         greatest((select coalesce(max(through_seq),0) from snapshot where document_id=$1),
         (select coalesce(max(through_seq),0) from shared_checkpoint where document_id=$1))",
    )
    .bind(document_id.0)
    .fetch_one(connection)
    .await?;
    Ok((seq > 0 && seq - last > max_events_in_tail).then_some(seq))
}

async fn insert_legacy(
    connection: &mut PgConnection,
    document_id: DocumentId,
    seq: i64,
    state: &DocumentState,
    chain: &[u8],
) -> Result<Snapshot, SnapshotError> {
    sqlx::query(
        "insert into snapshot(document_id,through_seq,state,merkle_root,event_chain_hash)
        values($1,$2,$3,$4,$5) on conflict(document_id,through_seq) do nothing",
    )
    .bind(document_id.0)
    .bind(seq)
    .bind(serde_json::to_value(state)?)
    .bind(merkle_root(state)?.as_slice())
    .bind(chain)
    .execute(&mut *connection)
    .await?;
    // A concurrent later snapshot must never replace this caller's exact revision.
    Ok(
        sqlx::query_as("select * from snapshot where document_id=$1 and through_seq=$2")
            .bind(document_id.0)
            .bind(seq)
            .fetch_one(connection)
            .await?,
    )
}

/// Persistent snapshot/materialization read path.
#[derive(Clone)]
pub struct SnapshotEngine {
    pool: PgPool,
    periodic_storage: PeriodicStorage,
}

impl SnapshotEngine {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            periodic_storage: PeriodicStorage::Legacy,
        }
    }

    /// Select the automatic writer for this instance. Default is legacy until
    /// deployment acceptance; it does not disable reading already-shared history.
    pub fn with_periodic_storage(mut self, storage: PeriodicStorage) -> Self {
        self.periodic_storage = storage;
        self
    }

    /// Explicit legacy snapshot contract used by deploys and the existing CLI.
    /// Uses the common mixed-format reader and pins an exact event sequence.
    pub async fn take(
        &self,
        document_id: DocumentId,
        _reason: SnapshotReason,
    ) -> Result<Snapshot, SnapshotError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("select id from document where id=$1 for key share")
            .bind(document_id.0)
            .fetch_one(&mut *tx)
            .await?;
        let (state, seq) = read_current_state(&mut tx, document_id).await?;
        let chain = shared::chain_at(&mut tx, document_id, seq).await?;
        let snapshot = insert_legacy(&mut tx, document_id, seq, &state, &chain).await?;
        tx.commit().await?;
        Ok(snapshot)
    }

    /// Best-effort periodic publication. One builder per document; other jobs skip
    /// redundant work. The shared writer rolls back all partial objects on capacity
    /// failure before saving a legacy checkpoint at the same exact revision.
    pub async fn ensure_recent(
        &self,
        document_id: DocumentId,
        max_events_in_tail: i64,
    ) -> Result<(), SnapshotError> {
        let mut tx = self.pool.begin().await?;
        // Most appends do not need a checkpoint. Do not let these cheap no-op
        // background jobs hold the builder lock and make a due request skip work.
        if checkpoint_due(&mut tx, document_id, max_events_in_tail)
            .await?
            .is_none()
        {
            return Ok(());
        }
        if !try_lock_periodic(&mut tx, document_id).await? {
            return Ok(());
        }
        // A prior builder may have committed between the first check and lock.
        let Some(seq) = checkpoint_due(&mut tx, document_id, max_events_in_tail).await? else {
            return Ok(());
        };
        let exists =
            sqlx::query("select id from document where id=$1 and deleted_at is null for key share")
                .bind(document_id.0)
                .fetch_optional(&mut *tx)
                .await?;
        if exists.is_none() {
            return Ok(());
        }
        let state = read_state_at(&mut tx, document_id, seq).await?.state;
        let chain = shared::chain_at(&mut tx, document_id, seq).await?;
        if self.periodic_storage == PeriodicStorage::SharedV1 {
            sqlx::query("savepoint shared_periodic")
                .execute(&mut *tx)
                .await?;
            match shared::publish_in_connection(
                &mut tx,
                document_id,
                seq,
                &state,
                chain.clone(),
                Limits::default(),
            )
            .await
            {
                Ok(_) => {
                    sqlx::query("release savepoint shared_periodic")
                        .execute(&mut *tx)
                        .await?;
                    tx.commit().await?;
                    return Ok(());
                }
                Err(shared_checkpoint::Error::Limit(reason)) => {
                    sqlx::query("rollback to savepoint shared_periodic")
                        .execute(&mut *tx)
                        .await?;
                    sqlx::query("release savepoint shared_periodic")
                        .execute(&mut *tx)
                        .await?;
                    warn!(document_id=%document_id.0, through_seq=seq, reason, "shared checkpoint capacity exceeded; saving legacy checkpoint");
                }
                Err(error) => return Err(error.into()),
            }
        }
        insert_legacy(&mut tx, document_id, seq, &state, &chain).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Read current state from the latest snapshot plus the current event tail.
    pub async fn read_current_state(
        &self,
        document_id: DocumentId,
    ) -> Result<DocumentState, SnapshotError> {
        Ok(self.read_current_state_with_seq(document_id).await?.0)
    }

    /// Like [`read_current_state`](Self::read_current_state), but also returns the event
    /// `seq` the state was folded through (0 for an empty log). The seq is pinned before
    /// selecting a checkpoint and bounds the suffix, so it matches the returned state —
    /// callers (the API read path) hand it to clients to resume an SSE stream without
    /// re-folding or missing events.
    pub async fn read_current_state_with_seq(
        &self,
        document_id: DocumentId,
    ) -> Result<(DocumentState, i64), SnapshotError> {
        let mut connection = self.pool.acquire().await?;
        read_current_state(&mut connection, document_id).await
    }
}

pub fn merkle_root(state: &DocumentState) -> Result<[u8; 32], SnapshotError> {
    if state.nodes.is_empty() {
        return Ok(ZERO_HASH);
    }

    let mut layer: Vec<[u8; 32]> = state
        .nodes
        .values()
        .map(hash_node)
        .collect::<Result<Vec<_>, _>>()?;

    while layer.len() > 1 {
        if layer.len() % 2 == 1 {
            layer.push(ZERO_HASH);
        }

        layer = layer
            .chunks(2)
            .map(|pair| {
                let mut ctx = Context::new(&SHA256);
                ctx.update(&[0x01]);
                ctx.update(&pair[0]);
                ctx.update(&pair[1]);
                finish(ctx)
            })
            .collect();
    }

    Ok(layer[0])
}

fn hash_node(node: &MaterializedNode) -> Result<[u8; 32], serde_json::Error> {
    let value = serde_json::to_value(node)?;
    let canonical = log::canonical_json(&value);
    let mut ctx = Context::new(&SHA256);
    ctx.update(&[0x00]);
    ctx.update(&canonical);
    Ok(finish(ctx))
}

fn finish(ctx: Context) -> [u8; 32] {
    let digest = ctx.finish();
    let mut out = [0u8; 32];
    out.copy_from_slice(digest.as_ref());
    out
}

/// Spawn a best-effort background `ensure_recent` task after a successful append.
pub fn spawn_ensure_recent(pool: PgPool, document_id: DocumentId, max_events_in_tail: i64) {
    tokio::spawn(async move {
        let storage =
            if std::env::var("DYNODOC_PERIODIC_CHECKPOINT_STORAGE").as_deref() == Ok("shared-v1") {
                PeriodicStorage::SharedV1
            } else {
                PeriodicStorage::Legacy
            };
        let engine = SnapshotEngine::new(pool).with_periodic_storage(storage);
        if let Err(err) = engine.ensure_recent(document_id, max_events_in_tail).await {
            warn!(
                document_id = %document_id.0,
                error = %err,
                "background ensure_recent failed"
            );
        }
    });
}

/// Whether a given event should force an immediate snapshot.
pub fn requires_immediate_snapshot(payload: &EventPayload) -> bool {
    matches!(payload, EventPayload::Deployed { .. })
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};

    use engine_shared::NodeId;
    use serde_json::json;

    use super::*;
    use crate::materializer::MaterializedNode;

    #[test]
    fn changing_one_node_byte_changes_the_merkle_root() {
        let mut state = DocumentState {
            nodes: BTreeMap::new(),
            comments: Vec::new(),
            suggestions: BTreeMap::new(),
            removed_choices: HashSet::new(),
        };
        state.nodes.insert(
            NodeId("01NODEA".into()),
            MaterializedNode {
                id: NodeId("01NODEA".into()),
                parent_id: None,
                node_type: "item".into(),
                pos: "a0".into(),
                current_fields: json!({ "label": "A" }),
                var_name: Some("a".into()),
                deleted: false,
            },
        );
        state.nodes.insert(
            NodeId("01NODEB".into()),
            MaterializedNode {
                id: NodeId("01NODEB".into()),
                parent_id: None,
                node_type: "item".into(),
                pos: "a1".into(),
                current_fields: json!({ "label": "B" }),
                var_name: Some("b".into()),
                deleted: false,
            },
        );

        let original = merkle_root(&state).unwrap();
        state
            .nodes
            .get_mut(&NodeId("01NODEB".into()))
            .unwrap()
            .current_fields = json!({ "label": "B!" });
        let changed = merkle_root(&state).unwrap();

        assert_ne!(original, changed);
    }
}
