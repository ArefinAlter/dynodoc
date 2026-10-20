//! Project and file agent policy, read side (docs/AI-AGENT-CONNECTORS.md sections 5.1 to
//! 5.3, docs/AI-AGENT-PROTOCOL-SPEC.md section 14.3). The routes that write it are in
//! [`super::manage`]; migration 0043 has the tables and the fields left out.
//!
//! **Default deny.** A project with no `project_agent_policy` row, or a row with
//! `enabled = false`, serves no agent request at all. The row is read on every request
//! (nothing here is cached), so a change or the kill switch applies to the very next call.
//!
//! **Effective permission** (spec 14.3, here for reads): the grant and the key class
//! ([`crate::auth::agent::AgentAuth`]), the person's project role and file role, then this
//! module: the project must be enabled ([`check`]), the role must be at least `min_role`,
//! the file kind must be one the policy admits and the file must not be `none`
//! ([`FILE_VISIBLE`]), and the per-grant budgets must have room. Each layer can only
//! narrow what the one before allowed.
//!
//! **Uniform answers.** A disabled project and a role below the floor are the same
//! `403 forbidden` as a missing grant or a non-member, and a file the policy hides is the
//! same `404` as a file that does not exist, so a refusal never says which layer refused.
//! The ledger row (internal) records `policy_denied` so an owner can see attempts.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgConnection;
use uuid::Uuid;

use super::error::AgentFail;
use crate::error::ApiError;
use engine_agent::error::AgentError;

/// File kinds an agent may ever read, in the order they are stored and shown.
pub const KINDS: [&str; 3] = ["document", "spreadsheet", "presentation"];
/// Product roles (the vocabulary of `document_member_role`), lowest first.
pub const ROLES: [&str; 6] = [
    "viewer",
    "contributor",
    "reviewer",
    "editor",
    "manager",
    "owner",
];
/// Inclusive bounds the migration also enforces. 600 is the project admission limit of
/// [`crate::workloads`]; a budget above it could never be reached.
pub const CALLS_PER_MINUTE: (i64, i64) = (1, 600);
pub const READ_BYTES_PER_DAY: (i64, i64) = (10_000, 1_000_000_000);

/// The enforceable fields of spec 5.2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Policy {
    pub enabled: bool,
    /// Lowest project role (product vocabulary) whose agents may read.
    pub min_role: String,
    pub kinds: Vec<String>,
    pub calls_per_minute: i32,
    pub read_bytes_per_day: i64,
}

impl Default for Policy {
    /// What a project that never set a policy has: agents disabled, the spec defaults.
    fn default() -> Self {
        Self {
            enabled: false,
            min_role: ROLES[0].to_owned(),
            kinds: KINDS.iter().map(|k| (*k).to_owned()).collect(),
            calls_per_minute: 60,
            read_bytes_per_day: 20_000_000,
        }
    }
}

/// A saved policy row.
#[derive(Debug, Clone)]
pub struct Stored {
    pub policy: Policy,
    pub revision: i64,
    pub updated_at: DateTime<Utc>,
}

/// The project's saved policy, or `None` when it never set one (which means disabled).
/// `lock` takes the row `FOR UPDATE`, for the writers.
pub async fn load(
    db: &mut PgConnection,
    project: Uuid,
    lock: bool,
) -> Result<Option<Stored>, sqlx::Error> {
    let row: Option<(bool, String, Vec<String>, i32, i64, i64, DateTime<Utc>)> =
        sqlx::query_as(&format!(
            "select enabled,min_role,kinds,calls_per_minute,read_bytes_per_day,revision,updated_at from project_agent_policy where project_id=$1{}",
            if lock { " for update" } else { "" }
        ))
        .bind(project)
        .fetch_optional(db)
        .await?;
    Ok(row.map(
        |(enabled, min_role, kinds, calls_per_minute, read_bytes_per_day, revision, updated_at)| {
            Stored {
                policy: Policy {
                    enabled,
                    min_role,
                    kinds,
                    calls_per_minute,
                    read_bytes_per_day,
                },
                revision,
                updated_at,
            }
        },
    ))
}

/// The product name of a stored project role (`document_member_role`'s vocabulary).
pub fn product_role(stored: &str) -> &str {
    match stored {
        "approver" => "reviewer",
        "reviewer" => "contributor",
        other => other,
    }
}

/// Position of a product role in [`ROLES`] (1 for the lowest), 0 for a name not in it.
fn rank(product: &str) -> usize {
    ROLES
        .iter()
        .position(|r| *r == product)
        .map_or(0, |at| at + 1)
}

/// SQL predicate on a `document d` row: not hidden from agents by its file policy. The
/// kind is checked beside it, against the policy's `kinds` (`$n::text[]`).
pub const FILE_VISIBLE: &str =
    "not exists(select 1 from file_agent_policy fp where fp.document_id=d.id and fp.access='none')";

/// Why a request was stopped after the credential and membership checks passed.
#[derive(Debug)]
pub enum Refusal {
    /// Agents are not enabled for the project, or the person's role is below the floor.
    Policy,
    /// A per-grant budget is used up; seconds until it has room again.
    Budget(u64),
}

impl Refusal {
    /// The ledger outcome (internal; the caller sees [`Refusal::into_fail`]).
    pub fn outcome(&self) -> &'static str {
        match self {
            Refusal::Policy => "policy_denied",
            Refusal::Budget(_) => "budget_exceeded",
        }
    }
    /// Policy is the uniform `403`; a budget is `429` with `Retry-After`, which only a
    /// caller that is already allowed in ever reaches.
    pub fn into_fail(self) -> AgentFail {
        match self {
            Refusal::Policy => ApiError::Forbidden.into(),
            Refusal::Budget(retry_after) => AgentError::BudgetExceeded { retry_after }.into(),
        }
    }
}

/// Either a refusal or a database failure.
pub enum Gate {
    Refused(Refusal),
    Db(sqlx::Error),
}
impl From<sqlx::Error> for Gate {
    fn from(error: sqlx::Error) -> Self {
        Gate::Db(error)
    }
}

/// What an admitted request may do, and what the grant has used so far.
#[derive(Debug, Clone)]
pub struct Allowed {
    pub policy: Policy,
    /// Admitted calls by this grant in the last minute (refusals are not counted).
    pub calls_used: i64,
    /// Response bytes served to this grant since midnight UTC.
    pub bytes_used: i64,
}

/// The project policy for one request of the grant `key`, whose person has the stored
/// project role `stored_role`. Reads the policy and the ledger now; called after the
/// membership check and the admission permit.
///
/// The budgets are counted from `agent_call`, so they hold across API processes. They are
/// not a transaction: concurrent requests of one grant (at most two are admitted at once)
/// can each see room for the last call, so a budget may be overrun by one response.
pub async fn check(
    db: &mut PgConnection,
    project: Uuid,
    key: Uuid,
    stored_role: &str,
) -> Result<Allowed, Gate> {
    let policy = match load(&mut *db, project, false).await? {
        Some(stored) if stored.policy.enabled => stored.policy,
        _ => return Err(Gate::Refused(Refusal::Policy)),
    };
    if rank(product_role(stored_role)) < rank(&policy.min_role) {
        return Err(Gate::Refused(Refusal::Policy));
    }
    // Calls in the last minute and bytes since midnight UTC of this grant. Refusals are
    // not counted, or a client that retried hard could never recover.
    let (calls, retry_calls, bytes, retry_day): (i64, Option<f64>, i64, f64) = sqlx::query_as(
        "select count(*) filter (where at>now()-interval '1 minute' and outcome not in ('budget_exceeded','policy_denied')),
                extract(epoch from (min(at) filter (where at>now()-interval '1 minute' and outcome not in ('budget_exceeded','policy_denied')) + interval '1 minute' - now()))::float8,
                coalesce(sum(bytes_out) filter (where at>=date_trunc('day',now() at time zone 'utc') at time zone 'utc'),0)::bigint,
                extract(epoch from (date_trunc('day',now() at time zone 'utc') at time zone 'utc' + interval '1 day' - now()))::float8
         from agent_call
         where key_id=$1 and project_id=$2 and at>=date_trunc('day',now() at time zone 'utc') at time zone 'utc' - interval '1 minute'",
    )
    .bind(key)
    .bind(project)
    .fetch_one(db)
    .await?;
    if calls >= i64::from(policy.calls_per_minute) {
        let wait = retry_calls.unwrap_or(60.0).ceil().clamp(1.0, 60.0);
        return Err(Gate::Refused(Refusal::Budget(wait as u64)));
    }
    if bytes >= policy.read_bytes_per_day {
        let wait = retry_day.ceil().clamp(1.0, 86_400.0);
        return Err(Gate::Refused(Refusal::Budget(wait as u64)));
    }
    Ok(Allowed {
        policy,
        calls_used: calls,
        bytes_used: bytes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_are_ordered_and_map_from_the_stored_names() {
        assert_eq!(product_role("approver"), "reviewer");
        assert_eq!(product_role("reviewer"), "contributor");
        assert_eq!(product_role("viewer"), "viewer");
        let stored = [
            "viewer", "reviewer", "approver", "editor", "manager", "owner",
        ];
        let ranks: Vec<usize> = stored.iter().map(|r| rank(product_role(r))).collect();
        assert_eq!(ranks, [1, 2, 3, 4, 5, 6]);
        assert_eq!(rank("auditor"), 0);
    }

    #[test]
    fn the_default_policy_is_disabled_with_the_spec_budgets() {
        let policy = Policy::default();
        assert!(!policy.enabled);
        assert_eq!(policy.kinds, KINDS);
        assert_eq!(
            (policy.calls_per_minute, policy.read_bytes_per_day),
            (60, 20_000_000)
        );
        assert!((CALLS_PER_MINUTE.0..=CALLS_PER_MINUTE.1).contains(&60));
    }

    #[test]
    fn a_refusal_keeps_the_uniform_answer() {
        assert_eq!(Refusal::Policy.outcome(), "policy_denied");
        assert!(matches!(
            Refusal::Policy.into_fail(),
            AgentFail::Api(ApiError::Forbidden)
        ));
        assert!(matches!(
            Refusal::Budget(7).into_fail(),
            AgentFail::Agent(AgentError::BudgetExceeded { retry_after: 7 })
        ));
    }
}
