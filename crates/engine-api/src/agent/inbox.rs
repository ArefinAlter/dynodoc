//! The reviewer's side of AI-agent proposals (docs/AI-AGENT-CONNECTORS.md 6.2): one list, per
//! project, of what the project's AI agents have proposed and where each stands, for the
//! people who review it. Account sessions only (a device, agent or connector token is `401`),
//! mounted whether or not the agent gateway is on: proposals outlive the switch.
//!
//! `GET /projects/{id}/agent-proposals?state&offset&limit` lists drafts that an agent wrote
//! and a person has not yet submitted (**only to the person the agent acts for**, as
//! everywhere: nobody else sees an unsubmitted agent draft) and change requests that came from
//! an agent identity (to every member who can open the file, the authorisation of
//! `GET /projects/{id}/change-requests`, of which this is the agent-only view).
//!
//! Each row carries metadata only: the file, the agent and the client product it ran in, the
//! person it acts for, the state, the revision, the base and the team head, whether the base is
//! behind, and the size of the change (engine operations stored, operations the agent sent).
//! No document text, no diff and no reason text: the review screen of the request shows the
//! change to someone who may open the file, and the draft's name is the agent's own words, so
//! a client shows it as text (`name` is untrusted, like a person's).
//!
//! **Bounds.** At most 50 rows a page (25 by default), and the newest 2,000 visible agent
//! proposals are considered, so `offset` stops at 1,000 and a state filter looks inside those.
//! The state is computed by the SQL of [`crate::reviews::state_sql`], the same counting rule
//! the review screen and the agent's status use.

use axum::{
    extract::{rejection::QueryRejection, Path, Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::PgConnection;
use uuid::Uuid;

use super::{identity::owned_by, policy, write};
use crate::{
    auth::AuthContext,
    error::ApiError,
    projects::{self, TREE},
    reviews, AppState,
};

/// Rows a page may hold, and the default.
const MAX_LIMIT: i64 = 50;
const DEFAULT_LIMIT: i64 = 25;
/// Agent proposals considered at all (the newest), and so the widest `offset`.
const CANDIDATES: i64 = 2_000;
const MAX_OFFSET: i64 = 1_000;

/// The state filter values: one state, `active` (draft, open and changes_requested; the
/// default) or `all`.
pub(super) const STATES: [&str; 6] = [
    "draft",
    "open",
    "changes_requested",
    "merged",
    "declined",
    "withdrawn",
];

pub fn router() -> Router<AppState> {
    Router::new().route("/projects/{id}/agent-proposals", get(list))
}

/// A state filter from a query value: `active` when absent.
pub(super) fn state_filter(value: Option<&str>) -> Result<&'static str, ApiError> {
    match value {
        None | Some("active") => Ok("active"),
        Some("all") => Ok("all"),
        Some(one) => STATES
            .iter()
            .find(|s| **s == one)
            .copied()
            .ok_or_else(|| ApiError::BadRequest {
                reason: "state is one of active, all, draft, open, changes_requested, merged, declined, withdrawn".into(),
            }),
    }
}

/// SQL boolean on a column `state` of a subquery, for [`state_filter`] bound as `$n`.
pub(super) fn state_matches(column: &str, bound: &str) -> String {
    format!("({bound}='all' or {column}={bound} or ({bound}='active' and {column} in ('draft','open','changes_requested')))")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ListQuery {
    state: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
}

/// Page numbers from a query, refused when out of range.
pub(super) fn paging(
    offset: Option<i64>,
    limit: Option<i64>,
    default_limit: i64,
) -> Result<(i64, i64), ApiError> {
    let (offset, limit) = (offset.unwrap_or(0), limit.unwrap_or(default_limit));
    if !(0..=MAX_OFFSET).contains(&offset) || !(1..=MAX_LIMIT).contains(&limit) {
        return Err(ApiError::BadRequest {
            reason: format!(
                "offset is 0 to {MAX_OFFSET} and limit is 1 to {MAX_LIMIT} (the newest {CANDIDATES} proposals are listed)"
            ),
        });
    }
    Ok((offset, limit))
}

/// What the viewer's own agents have used of the project's proposal budgets, when the viewer
/// is the person any agent identity acts for: proposals opened today and still open against
/// the limits, and the operations one proposal may hold. `null` for everyone else.
async fn budgets(db: &mut PgConnection, project: Uuid, viewer: Uuid) -> Result<Value, ApiError> {
    let acts: bool =
        sqlx::query_scalar("select exists(select 1 from agent_identity where principal_id=$1)")
            .bind(viewer)
            .fetch_one(&mut *db)
            .await?;
    if !acts {
        return Ok(Value::Null);
    }
    let stored = policy::load(db, project, false).await?;
    let policy = stored.map(|s| s.policy).unwrap_or_default();
    let (today, open) = write::proposal_use(db, viewer, project).await?;
    Ok(json!({
        "write_mode": policy.write_mode,
        "proposals_today": {"used": today, "limit": policy.proposals_per_day},
        "open_proposals": {"used": open, "limit": policy.open_proposals},
        "ops_per_proposal": policy.ops_per_proposal,
    }))
}

/// The AI agents' proposals of a project: drafts waiting for the person each acts for, and
/// the change requests that came from agent identities, with where each stands.
#[utoipa::path(operation_id="agent_proposals_for_reviewers", get, path="/projects/{id}/agent-proposals", tag="agent policy", params(("id"=Uuid,Path,description="Project"),("state"=Option<String>,Query,description="active (draft, open and changes_requested; the default), all, or one of draft, open, changes_requested, merged, declined, withdrawn"),("offset"=Option<i64>,Query,description="0 to 1000"),("limit"=Option<i64>,Query,description="1 to 50, default 25")), security(("paseto"=[])), responses((status=200, description="items (newest activity first, open ones before closed), more, offset, state and the budgets the viewer's own agents have used (null when the viewer has none). Each item is metadata: id, name (the agent's words: untrusted text), state, revision, base_seq, head_seq, behind, stale_base, created_at, submitted_at, closed_at, document {id,title,kind}, agent {id,client_kind,display_name,principal{id,name},revoked}, mine, edits and ops {proposed,compiled}. Drafts only reach the person the agent acts for", body=Value),(status=400, description="Unknown state, or offset or limit out of range"),(status=401, description="Not an account session"),(status=403, description="Not a member of the project")))]
async fn list(
    State(state): State<AppState>,
    Path(project): Path<Uuid>,
    auth: AuthContext,
    parsed: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Value>, ApiError> {
    let Query(q) = parsed.map_err(|_| ApiError::BadRequest {
        reason: "state, offset and limit are the query parameters".into(),
    })?;
    let filter = state_filter(q.state.as_deref())?;
    let (offset, limit) = paging(q.offset, q.limit, DEFAULT_LIMIT)?;
    let viewer = auth.identity_id;
    let mut db = state.pool.acquire().await?;
    projects::require(&mut db, project, viewer).await?;

    let owned = owned_by("d", "$2");
    let states = reviews::state_sql("d");
    let matches = state_matches("c.state", "$4");
    let sql = format!(
        "{TREE} select jsonb_build_object('id',p.id,'name',p.name,'state',p.state,'revision',p.revision,'base_seq',p.base_seq,'head_seq',x.head,'behind',greatest(x.head-p.base_seq,0),'stale_base',(p.state in ('draft','open','changes_requested') and x.head>p.base_seq),'created_at',p.created_at,'submitted_at',p.submitted_at,'closed_at',p.closed_at,'document',jsonb_build_object('id',p.document_id,'title',p.title,'kind',p.kind),'agent',jsonb_build_object('id',p.agent_id,'client_kind',p.client_kind,'display_name',p.agent_name,'principal',jsonb_build_object('id',p.principal_id,'name',p.principal_name),'revoked',p.revoked),'mine',p.principal_id=$2,'edits',(select count(*) from draft_edit e where e.draft_id=p.id),'ops',(select jsonb_build_object('proposed',coalesce(sum(r.ops_in),0)::bigint,'compiled',coalesce(sum(r.ops_engine),0)::bigint) from agent_proposal_receipt r where r.draft_id=p.id and r.kind='propose')) \
         from (select c.* from (select d.id,d.name,d.revision,d.base_seq,d.created_at,d.submitted_at,coalesce(d.reviewed_at,d.merged_at) as closed_at,d.document_id,f.title,coalesce(f.settings->>'kind','questionnaire') as kind,ai.identity_id as agent_id,ai.client_kind,ag.display_name as agent_name,ai.principal_id,pr.display_name as principal_name,ai.revoked_at is not null as revoked,{states} as state \
               from workspace_draft d join document f on f.id=d.document_id join tree t on t.id=f.space_id join agent_identity ai on ai.identity_id=d.created_by join identity ag on ag.id=ai.identity_id join identity pr on pr.id=ai.principal_id \
               where f.deleted_at is null and document_member_role(f.id,$2) is not null and (d.submitted_at is not null or {owned}) \
               order by coalesce(d.submitted_at,d.created_at) desc,d.id limit {CANDIDATES}) c \
               where {matches} \
               order by (c.state in ('merged','declined','withdrawn')),coalesce(c.submitted_at,c.created_at) desc,c.id limit $5 offset $3) p \
         cross join lateral (select coalesce(max(seq),0) as head from event where document_id=p.document_id) x \
         order by (p.state in ('merged','declined','withdrawn')),coalesce(p.submitted_at,p.created_at) desc,p.id"
    );
    let mut items: Vec<Value> = sqlx::query_scalar(&sql)
        .bind(project)
        .bind(viewer)
        .bind(offset)
        .bind(filter)
        .bind(limit + 1)
        .fetch_all(&mut *db)
        .await?;
    let more = items.len() as i64 > limit;
    items.truncate(limit as usize);
    let budgets = budgets(&mut db, project, viewer).await?;
    Ok(Json(json!({
        "items": items,
        "more": more,
        "offset": offset,
        "state": filter,
        "budgets": budgets,
    })))
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(list))]
pub struct AgentInboxApi;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_state_filter_takes_the_six_states_and_two_groups() {
        assert_eq!(state_filter(None).unwrap(), "active");
        assert_eq!(state_filter(Some("active")).unwrap(), "active");
        assert_eq!(state_filter(Some("all")).unwrap(), "all");
        for s in STATES {
            assert_eq!(state_filter(Some(s)).unwrap(), s);
        }
        for bad in ["", "Open", "closed", "open;drop table x", "merged "] {
            assert!(state_filter(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_page_is_bounded() {
        assert_eq!(paging(None, None, 25).unwrap(), (0, 25));
        assert_eq!(paging(Some(1000), Some(50), 25).unwrap(), (1000, 50));
        for (offset, limit) in [
            (Some(-1), None),
            (Some(1001), None),
            (None, Some(0)),
            (None, Some(51)),
        ] {
            assert!(paging(offset, limit, 25).is_err(), "{offset:?} {limit:?}");
        }
    }
}
