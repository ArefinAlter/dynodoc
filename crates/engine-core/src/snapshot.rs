//! Snapshot engine (docs/15).

use engine_shared::{DocumentId, EventPayload, Snapshot};
use ring::digest::{Context, SHA256};
use sqlx::PgPool;
use tracing::warn;

use crate::{
    log::{self, ZERO_HASH},
    materializer::{DocumentState, MaterializeError, MaterializedNode, Materializer},
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
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("event read failed: {0}")]
    Event(#[from] log::EventError),
    #[error("state materialization failed: {0}")]
    Materialize(#[from] MaterializeError),
    #[error("snapshot state serialization failed: {0}")]
    Serialize(#[from] serde_json::Error),
}

/// Persistent snapshot/materialization read path.
#[derive(Clone)]
pub struct SnapshotEngine {
    pool: PgPool,
}

impl SnapshotEngine {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Take a fresh snapshot at the latest event seq for the document.
    pub async fn take(
        &self,
        document_id: DocumentId,
        reason: SnapshotReason,
    ) -> Result<Snapshot, SnapshotError> {
        let prior_snapshot = latest_snapshot(&self.pool, document_id).await?;
        let base_seq = prior_snapshot
            .as_ref()
            .map(|snapshot| snapshot.through_seq)
            .unwrap_or(0);
        let tail = log::read_since_snapshot(&self.pool, document_id, base_seq).await?;

        if tail.is_empty() {
            if let Some(snapshot) = prior_snapshot {
                return Ok(snapshot);
            }
        }

        let state = match prior_snapshot.as_ref() {
            Some(snapshot) => Materializer::from_snapshot(snapshot, &tail)?,
            None => Materializer::fold(&tail)?,
        };

        let through_seq = tail
            .last()
            .map(|event| event.seq)
            .or_else(|| prior_snapshot.as_ref().map(|snapshot| snapshot.through_seq))
            .unwrap_or(0);
        let event_chain_hash = tail
            .last()
            .map(|event| event.chain_hash.clone())
            .or_else(|| {
                prior_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.event_chain_hash.clone())
            })
            .unwrap_or_else(|| ZERO_HASH.to_vec());
        let merkle_root = merkle_root(&state)?;
        let state_json = serde_json::to_value(&state)?;

        match sqlx::query_as::<_, Snapshot>(
            "insert into snapshot (document_id, through_seq, state, merkle_root, event_chain_hash)
             values ($1, $2, $3, $4, $5)
             returning *",
        )
        .bind(document_id.0)
        .bind(through_seq)
        .bind(state_json)
        .bind(&merkle_root[..])
        .bind(&event_chain_hash[..])
        .fetch_one(&self.pool)
        .await
        {
            Ok(snapshot) => Ok(snapshot),
            Err(err) if is_duplicate_snapshot(&err) => {
                warn!(
                    document_id = %document_id.0,
                    through_seq,
                    reason = reason.as_str(),
                    "snapshot already exists for this chain position; reusing latest"
                );
                Ok(latest_snapshot(&self.pool, document_id)
                    .await?
                    .expect("duplicate snapshot implies one exists"))
            }
            Err(err) => Err(SnapshotError::Db(err)),
        }
    }

    /// Ensure the current document has a recent snapshot, folding in the background
    /// when the event tail grows beyond the threshold.
    pub async fn ensure_recent(
        &self,
        document_id: DocumentId,
        max_events_in_tail: i64,
    ) -> Result<(), SnapshotError> {
        let latest_snapshot_seq = latest_snapshot(&self.pool, document_id)
            .await?
            .map(|snapshot| snapshot.through_seq)
            .unwrap_or(0);
        let latest_event_seq: Option<i64> =
            sqlx::query_scalar("select max(seq) from event where document_id = $1")
                .bind(document_id.0)
                .fetch_one(&self.pool)
                .await?;

        if let Some(latest_event_seq) = latest_event_seq {
            if latest_event_seq - latest_snapshot_seq > max_events_in_tail {
                let _ = self.take(document_id, SnapshotReason::Periodic).await?;
            }
        }

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
    /// `seq` the state was folded through (0 for an empty log). The seq comes from the
    /// same tail read as the fold, so it is exactly the position of the returned state —
    /// callers (the API read path) hand it to clients to resume an SSE stream without
    /// re-folding or missing events.
    pub async fn read_current_state_with_seq(
        &self,
        document_id: DocumentId,
    ) -> Result<(DocumentState, i64), SnapshotError> {
        if let Some(snapshot) = latest_snapshot(&self.pool, document_id).await? {
            let tail =
                log::read_since_snapshot(&self.pool, document_id, snapshot.through_seq).await?;
            let seq = tail.last().map_or(snapshot.through_seq, |event| event.seq);
            Ok((Materializer::from_snapshot(&snapshot, &tail)?, seq))
        } else {
            let events = log::read_since_snapshot(&self.pool, document_id, 0).await?;
            let seq = events.last().map_or(0, |event| event.seq);
            Ok((Materializer::fold(&events)?, seq))
        }
    }
}

pub(crate) async fn latest_snapshot(
    pool: &PgPool,
    document_id: DocumentId,
) -> Result<Option<Snapshot>, sqlx::Error> {
    sqlx::query_as(
        "select * from snapshot where document_id = $1 order by through_seq desc limit 1",
    )
    .bind(document_id.0)
    .fetch_optional(pool)
    .await
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

fn is_duplicate_snapshot(err: &sqlx::Error) -> bool {
    err.as_database_error()
        .and_then(|db| db.constraint())
        .is_some_and(|name| name == "snapshot_document_seq_unique")
}

/// Spawn a best-effort background `ensure_recent` task after a successful append.
pub fn spawn_ensure_recent(pool: PgPool, document_id: DocumentId, max_events_in_tail: i64) {
    tokio::spawn(async move {
        let engine = SnapshotEngine::new(pool);
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
