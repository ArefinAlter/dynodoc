//! Reading one file at one revision for the agent routes: which revision, which chain
//! hash, and the verified state (from the cache or the checkpoint-plus-tail reader).

use std::sync::Arc;

use engine_agent::{
    budget::Cursor,
    error::AgentError,
    rev::{Chain8, RevToken},
};
use engine_core::{materializer::DocumentState, snapshot};
use engine_shared::DocumentId;
use sqlx::PgConnection;
use uuid::Uuid;

use super::{
    cache::{estimate_bytes, StateCache},
    error::AgentFail,
};
use crate::error::ApiError;

/// A revision of one document and the chain hash stored at it.
#[derive(Debug, Clone)]
pub struct Point {
    pub document: Uuid,
    pub head: u64,
    pub rev: u64,
    /// `chain_hash` of the event at `rev` (32 zero bytes at `rev = 0`).
    pub chain: [u8; 32],
}

impl Point {
    /// The ETag value of this materialisation.
    pub fn token(&self) -> RevToken {
        RevToken::new(
            self.rev,
            Chain8::from_bytes(&self.chain).unwrap_or_else(|_| Chain8::zero()),
        )
    }

    pub fn chain_hex(&self) -> String {
        hex::encode(self.chain)
    }
}

/// The revision a request asks for. A cursor pins its own revision; `rev` and a cursor
/// must agree when both are given; with neither the head is read. A revision past the
/// head is a client error, not a missing resource.
pub fn pick_rev(rev: Option<u64>, cursor: Option<&str>, head: u64) -> Result<u64, AgentError> {
    let pinned = cursor
        .map(|raw| {
            raw.parse::<Cursor>()
                .map(|c| c.rev)
                .map_err(|_| AgentError::BadRequest { field: "cursor" })
        })
        .transpose()?;
    let wanted = match (rev, pinned) {
        (Some(r), Some(c)) if r != c => return Err(AgentError::BadRequest { field: "cursor" }),
        (Some(r), _) => r,
        (None, Some(c)) => c,
        (None, None) => head,
    };
    if wanted > head {
        return Err(AgentError::BadRequest {
            field: if rev.is_some() { "rev" } else { "cursor" },
        });
    }
    Ok(wanted)
}

/// Head sequence and the stored chain hash at the wanted revision.
pub async fn point(
    db: &mut PgConnection,
    document: Uuid,
    rev: Option<u64>,
    cursor: Option<&str>,
) -> Result<Point, AgentFail> {
    let head: i64 =
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(document)
            .fetch_one(&mut *db)
            .await?;
    let head = u64::try_from(head).unwrap_or(0);
    let wanted = pick_rev(rev, cursor, head)?;
    let chain = if wanted == 0 {
        [0u8; 32]
    } else {
        // Revisions 1..=head exist; a missing row is a hole in the history.
        let stored: Option<Vec<u8>> =
            sqlx::query_scalar("select chain_hash from event where document_id=$1 and seq=$2")
                .bind(document)
                .bind(wanted as i64)
                .fetch_optional(&mut *db)
                .await?;
        stored
            .and_then(|bytes| <[u8; 32]>::try_from(bytes).ok())
            .ok_or(ApiError::ReplayFailed {
                document_id: document,
                seq: Some(wanted as i64),
            })?
    };
    Ok(Point {
        document,
        head,
        rev: wanted,
        chain,
    })
}

/// The state at `point`: the cache when it holds exactly this `(document, rev, chain)`,
/// otherwise the checkpoint-plus-tail reader, which verifies the chain of every event it
/// replays (a document that does not replay is `replay_failed`, as on every other read
/// route). The caller has already authorised the request.
pub async fn load(
    cache: &StateCache,
    db: &mut PgConnection,
    point: &Point,
) -> Result<Arc<DocumentState>, AgentFail> {
    let key = (point.document, point.rev, point.chain);
    if let Some(hit) = cache.get(&key) {
        return Ok(hit);
    }
    let read = snapshot::read_state_at(db, DocumentId(point.document), point.rev as i64).await?;
    let state = Arc::new(read.state);
    cache.insert(key, state.clone(), estimate_bytes(&state));
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_selection() {
        assert_eq!(pick_rev(None, None, 9).unwrap(), 9);
        assert_eq!(pick_rev(Some(3), None, 9).unwrap(), 3);
        assert_eq!(pick_rev(Some(0), None, 0).unwrap(), 0);
        // A cursor pins its own revision, even when the file has moved on.
        assert_eq!(pick_rev(None, Some("c_4.2"), 9).unwrap(), 4);
        assert_eq!(pick_rev(Some(4), Some("c_4.2"), 9).unwrap(), 4);
        assert!(matches!(
            pick_rev(Some(5), Some("c_4.2"), 9),
            Err(AgentError::BadRequest { field: "cursor" })
        ));
        assert!(matches!(
            pick_rev(Some(10), None, 9),
            Err(AgentError::BadRequest { field: "rev" })
        ));
        assert!(matches!(
            pick_rev(None, Some("c_a.0"), 9),
            Err(AgentError::BadRequest { field: "cursor" })
        ));
        assert!(matches!(
            pick_rev(None, Some("nonsense"), 9),
            Err(AgentError::BadRequest { field: "cursor" })
        ));
    }
}
