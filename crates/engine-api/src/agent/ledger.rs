//! The agent call ledger (migrations 0042 and 0044, protocol spec 9.6): one metadata row per
//! admitted agent call, read or write. Metadata only (credential, project, file, tool,
//! revision, a digest of the request, byte counts and the outcome code), never document
//! text, titles, `why` or request values. It is the only thing an agent read writes, and the
//! budgets of calls and bytes are counted from it. What a write call proposed (the proposal,
//! the counts of ops, the stale refs) is in its receipt, `agent_proposal_receipt`.

use ring::digest::{digest, SHA256};
use sqlx::PgPool;
use uuid::Uuid;

/// What is recorded about one call.
pub struct CallRow {
    pub principal: Uuid,
    pub key: Uuid,
    pub project: Uuid,
    pub document: Option<Uuid>,
    pub tool: &'static str,
    pub rev: Option<u64>,
    pub digest: [u8; 32],
    pub bytes_out: usize,
    pub outcome: &'static str,
    pub client: Option<String>,
    pub traceparent: Option<String>,
}

/// SHA-256 over `parts` joined by NUL: the request digest of the ledger. The caller
/// passes the method, the path and the query or body in a canonical form.
pub fn request_digest(parts: &[&[u8]]) -> [u8; 32] {
    let mut input = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            input.push(0);
        }
        input.extend_from_slice(part);
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(digest(&SHA256, &input).as_ref());
    out
}

/// Outcomes of a request the project policy stopped. A caller can repeat one at the
/// admission rate for as long as it likes (the file need not exist), and the runtime login
/// cannot delete a ledger row, so these are written at most once a minute per grant and
/// outcome ([`record`]).
const THROTTLED: [&str; 2] = ["policy_denied", "budget_exceeded"];

/// Append the row. A ledger failure is logged and never fails the read that was already
/// served: the content did not depend on it, and a read-only route has no state to roll
/// back. (A database outage fails the read itself before this point.)
///
/// A throttled outcome (`policy_denied`, `budget_exceeded`) is skipped when the same key,
/// project and outcome already has a row from the last minute (the index
/// `agent_call_key_project_at` serves the lookup), so a flood of refusals leaves one row
/// per minute, not one per request. Concurrent requests of one grant (at most two) can
/// both pass the test; the table stays bounded by that. Admitted calls are never
/// throttled.
pub async fn record(pool: &PgPool, row: CallRow) {
    let bytes = i32::try_from(row.bytes_out).unwrap_or(i32::MAX);
    let tokens = i32::try_from(row.bytes_out.div_ceil(4)).unwrap_or(i32::MAX);
    let throttled = THROTTLED.contains(&row.outcome);
    let result = sqlx::query("insert into agent_call(principal_id,key_id,project_id,document_id,tool,rev,request_digest,bytes_out,tok_est,outcome,client,traceparent) select $1::uuid,$2::uuid,$3::uuid,$4::uuid,$5::text,$6::bigint,$7::bytea,$8::int,$9::int,$10::text,$11::text,$12::text where not $13::bool or not exists(select 1 from agent_call where key_id=$2::uuid and project_id=$3::uuid and outcome=$10::text and at>now()-interval '1 minute')")
        .bind(row.principal)
        .bind(row.key)
        .bind(row.project)
        .bind(row.document)
        .bind(row.tool)
        .bind(row.rev.and_then(|r| i64::try_from(r).ok()))
        .bind(&row.digest[..])
        .bind(bytes)
        .bind(tokens)
        .bind(row.outcome)
        .bind(row.client)
        .bind(row.traceparent)
        .bind(throttled)
        .execute(pool)
        .await;
    if let Err(error) = result {
        tracing::warn!(%error, "agent call row was not recorded");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digest_binds_every_part_and_their_boundaries() {
        let a = request_digest(&[b"GET", b"/x", b"q=1"]);
        assert_eq!(a, request_digest(&[b"GET", b"/x", b"q=1"]));
        assert_ne!(a, request_digest(&[b"GET", b"/x", b"q=2"]));
        assert_ne!(a, request_digest(&[b"POST", b"/x", b"q=1"]));
        // "ab"+"c" and "a"+"bc" differ.
        assert_ne!(
            request_digest(&[b"ab", b"c"]),
            request_digest(&[b"a", b"bc"])
        );
    }
}
