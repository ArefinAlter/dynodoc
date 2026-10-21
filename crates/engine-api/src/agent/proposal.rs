//! `submit`, `withdraw` and the status of a proposal (docs/AI-AGENT-PROTOCOL-SPEC.md 7.8, 7.9;
//! routes 11 to 13). The decisions and the order of checks are those of [`super::write`].
//!
//! * **Whose.** A proposal is found by `created_by = the agent identity`, in this project, in
//!   a file the person can still open and the policy still shows. Anything else (a person's
//!   draft, another agent's, another project's, an invented id, a file since hidden or lost)
//!   is one `404`. Possession of an id is not authority.
//! * **submit** makes the draft a change request through `reviews::mark_submitted`, the path a
//!   person's submit takes, with the agent identity as the actor (audit row, notification of
//!   the reviewers). It does so only when `write_mode` is `change_request` and the project
//!   does not require a person to submit; otherwise it changes nothing and says
//!   `awaiting_person`. A closed proposal is `proposal_closed`; an open one answers `open`
//!   and writes nothing.
//! * **withdraw** closes the agent's own proposal (`review_outcome = withdrawn`); a person can
//!   still reopen it by submitting it again. It needs the grant and a role that may draft, not
//!   `write_mode`: it only reduces what is pending.
//! * **status** reports the state (`draft`, `open`, `changes_requested`, `declined`, `merged`,
//!   `withdrawn`), the base and head revisions and how far behind the proposal is, the
//!   approvals, what the reviewers said (bounded, with control and hidden characters dropped:
//!   data, not instructions) and the same-field conflicts a merge would meet, by ref, field and
//!   short hashes, never their content.
//!
//! None of the three appends to the event log.

use std::collections::BTreeMap;

use axum::{
    body::Bytes,
    extract::{rejection::QueryRejection, Path, Query, State},
    http::{HeaderMap, Uri},
};
use engine_agent::{
    dct::{quote, Tool},
    error::AgentError,
    escape::{escape, sanitize},
    hash::Hash,
    refs::RefIndex,
};
use engine_core::governance::Role;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{PgConnection, Postgres, Transaction};
use uuid::Uuid;

use super::{
    error::AgentFail,
    http,
    identity::{self, AgentIdentity},
    ledger,
    policy::{self, Policy},
    routes::{Call, Ctx, Out, Served},
    state,
    write::{answer, begin_locked, denied, not_found, short, state_word, verify_write, ProposalId},
};
use crate::{
    access,
    auth::agent::{AgentAuth, ContentRead, ContentSubmit},
    error::ApiError,
    product::audit,
    reviews, AppState,
};

/// Characters of the note on a submit or withdraw.
const MAX_NOTE_CHARS: usize = 1000;
/// Characters of a reviewer's note shown to the agent, and of a reviewer's name.
const SHOWN_NOTE_CHARS: usize = 400;
const SHOWN_NAME_CHARS: usize = 80;
/// Reviews and conflicts listed in a status.
const SHOWN_ITEMS: usize = 20;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoteBody {
    note: Option<String>,
}

/// The optional note of a submit or withdraw: at most 1000 characters, no control or hidden
/// characters (the same rule as the text of `propose`), stored with the request for the
/// reviewers. No body at all is no note.
fn note_of(body: &[u8]) -> Result<String, AgentFail> {
    if body.iter().all(u8::is_ascii_whitespace) {
        return Ok(String::new());
    }
    let parsed: NoteBody =
        serde_json::from_slice(body).map_err(|_| AgentError::BadRequest { field: "body" })?;
    let note = parsed.note.unwrap_or_default().replace("\r\n", "\n");
    let count = note.chars().count();
    if count > MAX_NOTE_CHARS {
        return Err(AgentError::LimitExceeded {
            field: "note",
            limit: MAX_NOTE_CHARS as u64,
            value: count as u64,
        }
        .into());
    }
    let (_, dropped) = escape(&note);
    if dropped.ctrl > 0 || dropped.hidden > 0 {
        return Err(AgentError::BadRequest { field: "note" }.into());
    }
    Ok(note.trim().to_owned())
}

/// `text` as data for the agent: control and hidden characters dropped, cut to `max`
/// characters (with an ellipsis).
fn bounded(text: &str, max: usize) -> String {
    let clean = sanitize(text);
    if clean.chars().count() > max {
        let mut cut: String = clean.chars().take(max).collect();
        cut.push('\u{2026}');
        cut
    } else {
        clean
    }
}

/// One proposal of the agent identity, as its row says.
struct Found {
    id: Uuid,
    document: Uuid,
    base_seq: i64,
    revision: i64,
    submitted: bool,
    merged: bool,
    outcome: Option<String>,
}

impl Found {
    fn closed(&self) -> bool {
        self.merged || self.outcome.is_some()
    }
    fn state(&self) -> &'static str {
        state_word(self.submitted, self.merged, self.outcome.as_deref())
    }
}

/// The agent identity's proposal `proposal`, if it has one, in this project, in a file the
/// person can open today and the policy shows to agents.
async fn find_proposal(
    db: &mut PgConnection,
    ctx: &Ctx<'_>,
    agent: Uuid,
    proposal: Uuid,
    kinds: &[String],
) -> Result<Found, AgentFail> {
    let row: Option<(Uuid, i64, i64, bool, bool, Option<String>)> = sqlx::query_as(&format!("select w.document_id,w.base_seq,w.revision,w.submitted_at is not null,w.merged_at is not null,w.review_outcome from workspace_draft w join document d on d.id=w.document_id where w.id=$1 and w.created_by=$2 and document_project(w.document_id)=$3 and d.deleted_at is null and coalesce(d.settings->>'kind','questionnaire')=any($5::text[]) and document_member_role(d.id,$4) is not null and {}", policy::FILE_VISIBLE))
        .bind(proposal)
        .bind(agent)
        .bind(ctx.project)
        .bind(ctx.identity)
        .bind(kinds)
        .fetch_optional(db)
        .await
        .map_err(ApiError::from)?;
    let Some((document, base_seq, revision, submitted, merged, outcome)) = row else {
        return Err(not_found());
    };
    Ok(Found {
        id: proposal,
        document,
        base_seq,
        revision,
        submitted,
        merged,
        outcome,
    })
}

/// A proposal found, with the permit held for the rest of the request.
struct Opened {
    _permit: crate::workloads::ReadPermit,
    policy: Policy,
    agent: AgentIdentity,
    found: Found,
}

async fn open(ctx: &Ctx<'_>, call: &mut Call, proposal: Uuid) -> Result<Opened, AgentFail> {
    let mut admitted = ctx.admit(call).await?;
    // No agent identity means no proposal of its own, which is the same `404`.
    let agent = identity::resolve(&mut admitted.db, ctx.identity, ctx.key)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(not_found)?;
    let found = find_proposal(
        &mut admitted.db,
        ctx,
        agent.id,
        proposal,
        &admitted.allowed.policy.kinds,
    )
    .await?;
    call.document = Some(found.document);
    Ok(Opened {
        _permit: admitted._permit,
        policy: admitted.allowed.policy,
        agent,
        found,
    })
}

/// The receipt of a submit or withdraw that changed state.
#[allow(clippy::too_many_arguments)]
async fn receipt(
    tx: &mut Transaction<'_, Postgres>,
    ctx: &Ctx<'_>,
    agent: &AgentIdentity,
    found: &Found,
    kind: &str,
    head: u64,
    revision: i64,
    outcome: &str,
) -> Result<(), AgentFail> {
    sqlx::query("insert into agent_proposal_receipt(draft_id,document_id,project_id,agent_identity_id,principal_id,key_id,kind,rev,base_seq,head_seq,revision,request_digest,outcome) values($1,$2,$3,$4,$5,$6,$7,$8,$8,$9,$10,$11,$12)")
        .bind(found.id)
        .bind(found.document)
        .bind(ctx.project)
        .bind(agent.id)
        .bind(ctx.identity)
        .bind(ctx.key)
        .bind(kind)
        .bind(found.base_seq)
        .bind(head as i64)
        .bind(revision)
        .bind(&ctx.digest[..])
        .bind(outcome)
        .execute(&mut **tx)
        .await
        .map_err(ApiError::from)?;
    Ok(())
}

/// The answer of `submit` and `withdraw`.
fn outcome_body(
    ctx: &Ctx<'_>,
    tool_action: &'static str,
    found: &Found,
    state: &str,
    behind: u64,
    changed: bool,
) -> String {
    let dct = ctx.format == http::Format::Dct;
    answer(
        ctx.format,
        Tool::Submit,
        vec![
            ("action", json!(tool_action)),
            // The routes take the whole id, so a DCT-only client must be shown it.
            ("proposal", json!(found.id)),
            (
                "file",
                if dct {
                    json!(short(found.document))
                } else {
                    json!(found.document)
                },
            ),
            ("state", json!(state)),
            ("changed", json!(changed)),
            ("behind", json!(behind)),
        ],
        &[],
    )
}

// --- submit ----------------------------------------------------------------------------

/// Send the agent's proposal to the file's reviewers, when the project lets an agent
/// (spec 7.8). You cannot approve, decline or merge.
#[utoipa::path(operation_id="agent_submit", post, path="/projects/{id}/agent/proposals/{proposal}/submit", tag="agent", params(("id"=Uuid,Path,description="Project"),("proposal"=Uuid,Path,description="Proposal id from propose")), request_body(content=super::schema::AgentNoteBody, description="Optional note for the reviewers, at most 1000 characters; the body may be empty"), security(("device_session"=["content:submit"])), responses((status=200, description="state is open (submitted now, or already open: nothing is written twice), or awaiting_person when the project requires a person to submit (nothing changed: a person submits it from the app). Notifies the reviewers when it submits. changed says whether this call did", content((super::schema::AgentSubmit="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=400, description="Invalid body or note"),(status=401, description="Not a project device session"),(status=403, description="Scope, grant, key class, membership, role or policy (write_mode) refuses"),(status=404, description="No such proposal of this agent (one answer for every reason; a malformed id is 404 before authentication)"),(status=409, description="proposal_closed: merged, declined or withdrawn"),(status=429, description="Budget")))]
pub(super) async fn submit(
    State(state): State<AppState>,
    ProposalId(proposal): ProposalId,
    auth: AgentAuth<ContentSubmit>,
    Path((project, _proposal)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Out {
    let digest = ledger::request_digest(&[b"POST", uri.path().as_bytes(), &body]);
    let ctx = Ctx::new(&state, &auth, project, &headers, Tool::Submit, digest)?;
    let mut call = Call::default();
    let result = submit_call(&ctx, &mut call, proposal, &body).await;
    ctx.finish(call, result).await
}

async fn submit_call(
    ctx: &Ctx<'_>,
    call: &mut Call,
    proposal: Uuid,
    body: &[u8],
) -> Result<Served, AgentFail> {
    let opened = open(ctx, call, proposal).await?;
    let note = note_of(body)?;
    let Opened {
        policy,
        agent,
        found,
        ..
    } = &opened;
    if found.closed() {
        return Err(AgentError::ProposalClosed.into());
    }
    if !policy.can_propose() {
        return Err(denied(call, "write_mode"));
    }
    let served = |text: String| Served {
        bytes: text.len(),
        response: http::reply(ctx.format, text, None),
    };
    let head = head_of(ctx, found.document).await?;
    let behind = head.saturating_sub(found.base_seq as u64);
    if found.submitted {
        // Already a change request: the state, and nothing written twice.
        return Ok(served(outcome_body(
            ctx, "submit", found, "open", behind, false,
        )));
    }
    if !policy.agent_may_submit() {
        call.outcome = Some("awaiting_person");
        return Ok(served(outcome_body(
            ctx,
            "submit",
            found,
            "awaiting_person",
            behind,
            false,
        )));
    }

    // The lock, not the state: nothing here reads the file's content.
    let (mut tx, role) = begin_locked(ctx, found.document).await?;
    sqlx::query("set local statement_timeout='10s'")
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    sqlx::query("set local lock_timeout='3s'")
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    if role == Role::Auditor {
        return Err(ApiError::Forbidden.into());
    }
    let locked = verify_write(&mut tx, ctx, found.document, true).await?;
    // The proposal, locked, as it is now.
    let row: Option<(i64, bool, bool, Option<String>)> = sqlx::query_as("select revision,submitted_at is not null,merged_at is not null,review_outcome from workspace_draft where id=$1 and document_id=$2 and created_by=$3 for update")
        .bind(found.id)
        .bind(found.document)
        .bind(agent.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    let Some((revision, submitted, merged, outcome)) = row else {
        return Err(not_found());
    };
    if merged || outcome.is_some() {
        return Err(AgentError::ProposalClosed.into());
    }
    if submitted {
        return Ok(served(outcome_body(
            ctx, "submit", found, "open", behind, false,
        )));
    }
    if !locked.agent_may_submit() {
        call.outcome = Some("awaiting_person");
        return Ok(served(outcome_body(
            ctx,
            "submit",
            found,
            "awaiting_person",
            behind,
            false,
        )));
    }
    // The existing review path, as the agent identity: stamps the draft, writes the
    // `document.change_request_submitted` audit row and notifies the reviewers.
    if !reviews::mark_submitted(&mut tx, found.document, found.id, agent.id, &note, false).await? {
        return Err(AgentError::ProposalClosed.into());
    }
    receipt(&mut tx, ctx, agent, found, "submit", head, revision, "ok").await?;
    audit(
        &mut tx,
        agent.id,
        "agent.proposal.submitted",
        found.id,
        json!({"file":found.document,"principal":ctx.identity,"by":"agent","client_kind":agent.client_kind}),
    )
    .await?;
    tx.commit().await.map_err(ApiError::from)?;
    Ok(served(outcome_body(
        ctx, "submit", found, "open", behind, true,
    )))
}

/// The team version's head revision of a file.
async fn head_of(ctx: &Ctx<'_>, document: Uuid) -> Result<u64, AgentFail> {
    let head: i64 =
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(document)
            .fetch_one(&ctx.state.pool)
            .await
            .map_err(ApiError::from)?;
    Ok(u64::try_from(head).unwrap_or(0))
}

// --- withdraw --------------------------------------------------------------------------

/// Close the agent's own proposal (spec 7.8): a draft or an open change request. A person can
/// still reopen it. It needs no `write_mode`, only the grant and a role that may draft.
#[utoipa::path(operation_id="agent_withdraw", post, path="/projects/{id}/agent/proposals/{proposal}/withdraw", tag="agent", params(("id"=Uuid,Path,description="Project"),("proposal"=Uuid,Path,description="Proposal id from propose")), request_body(content=super::schema::AgentNoteBody, description="Optional note, at most 1000 characters; the body may be empty"), security(("device_session"=["content:submit"])), responses((status=200, description="state is withdrawn. Withdrawing a withdrawn proposal answers the same with changed=false", content((super::schema::AgentSubmit="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=400, description="Invalid body or note"),(status=401, description="Not a project device session"),(status=403, description="Scope, grant, key class, membership or role refuses"),(status=404, description="No such proposal of this agent (one answer for every reason)"),(status=409, description="proposal_closed: merged or declined"),(status=429, description="Budget")))]
pub(super) async fn withdraw(
    State(state): State<AppState>,
    ProposalId(proposal): ProposalId,
    auth: AgentAuth<ContentSubmit>,
    Path((project, _proposal)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    uri: Uri,
    body: Bytes,
) -> Out {
    let digest = ledger::request_digest(&[b"POST", uri.path().as_bytes(), &body]);
    let ctx = Ctx::new(&state, &auth, project, &headers, Tool::Submit, digest)?;
    let mut call = Call::default();
    let result = withdraw_call(&ctx, &mut call, proposal, &body).await;
    ctx.finish(call, result).await
}

async fn withdraw_call(
    ctx: &Ctx<'_>,
    call: &mut Call,
    proposal: Uuid,
    body: &[u8],
) -> Result<Served, AgentFail> {
    let opened = open(ctx, call, proposal).await?;
    let note = note_of(body)?;
    let Opened { agent, found, .. } = &opened;
    let served = |text: String| Served {
        bytes: text.len(),
        response: http::reply(ctx.format, text, None),
    };
    let head = head_of(ctx, found.document).await?;
    let behind = head.saturating_sub(found.base_seq as u64);
    if found.outcome.as_deref() == Some("withdrawn") {
        return Ok(served(outcome_body(
            ctx,
            "withdraw",
            found,
            "withdrawn",
            behind,
            false,
        )));
    }
    if found.closed() {
        return Err(AgentError::ProposalClosed.into());
    }
    // The lock, not the state: nothing here reads the file's content.
    let (mut tx, role) = begin_locked(ctx, found.document).await?;
    sqlx::query("set local statement_timeout='10s'")
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    sqlx::query("set local lock_timeout='3s'")
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    if role == Role::Auditor {
        return Err(ApiError::Forbidden.into());
    }
    verify_write(&mut tx, ctx, found.document, false).await?;
    let row: Option<(i64, bool, bool, Option<String>)> = sqlx::query_as("select revision,submitted_at is not null,merged_at is not null,review_outcome from workspace_draft where id=$1 and document_id=$2 and created_by=$3 for update")
        .bind(found.id)
        .bind(found.document)
        .bind(agent.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    let Some((revision, submitted, merged, outcome)) = row else {
        return Err(not_found());
    };
    if outcome.as_deref() == Some("withdrawn") {
        return Ok(served(outcome_body(
            ctx,
            "withdraw",
            found,
            "withdrawn",
            behind,
            false,
        )));
    }
    if merged || outcome.is_some() {
        return Err(AgentError::ProposalClosed.into());
    }
    sqlx::query("update workspace_draft set review_outcome='withdrawn',reviewed_by=$3,reviewed_at=now(),review_note=$4 where id=$1 and document_id=$2 and created_by=$3 and merged_at is null and review_outcome is null")
        .bind(found.id)
        .bind(found.document)
        .bind(agent.id)
        .bind(&note)
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    // The existing audit vocabulary: a request withdrawn, or a draft that never became one.
    audit(
        &mut tx,
        agent.id,
        if submitted {
            "document.change_request_withdrawn"
        } else {
            "document.draft_withdrawn"
        },
        found.document,
        json!({"draft_id":found.id,"contributor":agent.id,"contributor_kind":"agent","agent":{"client_kind":agent.client_kind,"principal":ctx.identity}}),
    )
    .await?;
    receipt(&mut tx, ctx, agent, found, "withdraw", head, revision, "ok").await?;
    audit(
        &mut tx,
        agent.id,
        "agent.proposal.withdrawn",
        found.id,
        json!({"file":found.document,"principal":ctx.identity,"by":"agent","was_submitted":submitted}),
    )
    .await?;
    tx.commit().await.map_err(ApiError::from)?;
    Ok(served(outcome_body(
        ctx,
        "withdraw",
        found,
        "withdrawn",
        behind,
        true,
    )))
}

// --- status ----------------------------------------------------------------------------

/// The state of one proposal of the agent (spec 7.9): where it is in review, what the
/// reviewers said, how far behind the team version it is and where a merge would conflict.
#[utoipa::path(operation_id="agent_proposal_status", get, path="/projects/{id}/agent/proposals/{proposal}", tag="agent", params(("id"=Uuid,Path,description="Project"),("proposal"=Uuid,Path,description="Proposal id from propose")), security(("device_session"=["content:read"])), responses((status=200, description="state is draft, open, changes_requested, declined, merged or withdrawn; base and head revisions and behind; the approvals counted and required; the latest reviews (verdict, reviewer name, bounded note: all untrusted text, trust=data); and, while the proposal is not closed, the same-field conflicts a merge would meet by ref, field and short hashes (never content)", content((super::schema::AgentProposalStatus="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=401, description="Not a project device session"),(status=403, description="Scope, grant, key class, membership or policy refuses"),(status=404, description="No such proposal of this agent (one answer for every reason)"),(status=429, description="Budget")))]
pub(super) async fn status(
    State(state): State<AppState>,
    ProposalId(proposal): ProposalId,
    auth: AgentAuth<ContentRead>,
    Path((project, _proposal)): Path<(Uuid, Uuid)>,
    headers: HeaderMap,
    uri: Uri,
) -> Out {
    let digest = ledger::request_digest(&[
        b"GET",
        uri.path().as_bytes(),
        uri.query().unwrap_or("").as_bytes(),
    ]);
    let ctx = Ctx::new(&state, &auth, project, &headers, Tool::Status, digest)?;
    let mut call = Call::default();
    let result = status_call(&ctx, &mut call, proposal).await;
    ctx.finish(call, result).await
}

async fn status_call(ctx: &Ctx<'_>, call: &mut Call, proposal: Uuid) -> Result<Served, AgentFail> {
    let opened = open(ctx, call, proposal).await?;
    let Opened { found, .. } = &opened;
    let mut db = ctx.state.pool.acquire().await.map_err(ApiError::from)?;
    let point = state::point(&mut db, found.document, None, None).await?;
    call.rev = Some(point.rev);
    let behind = point.head.saturating_sub(found.base_seq as u64);

    // The request as the review screen reads it, and the reviews counted under the file's
    // rules. The principal is the contributor (I5), so an agent's request never counts them.
    let request: Value = sqlx::query_scalar(&format!("select {}||jsonb_build_object('mine',true) from workspace_draft d join identity a on a.id=d.created_by left join identity r on r.id=d.reviewed_by where d.id=$1", reviews::REQUEST_JSON))
        .bind(found.id)
        .fetch_one(&mut *db)
        .await
        .map_err(ApiError::from)?;
    let member = access::governance_role(&mut db, found.document, ctx.identity).await?;
    let rules = access::policy(&mut db, found.document).await?;
    let review =
        reviews::review_state(&mut db, found.document, found.id, &request, member, &rules).await?;
    let blocked = review["changes_requested_by"]
        .as_array()
        .is_some_and(|l| !l.is_empty());
    let state_name = match found.state() {
        "open" if blocked => "changes_requested",
        other => other,
    };

    // Conflicts, while there is something to merge.
    let mut conflicts: Vec<Value> = Vec::new();
    let mut conflict_total = 0usize;
    if !found.closed() {
        let team = state::load(&ctx.state.agent_cache, &mut db, &point).await?;
        let row: Value =
            sqlx::query_scalar("select to_jsonb(d) from workspace_draft d where d.id=$1")
                .bind(found.id)
                .fetch_one(&mut *db)
                .await
                .map_err(ApiError::from)?;
        let drafted = crate::workspace::replay_draft(&mut db, found.document, row).await?;
        let (total, listed) = super::write::blocking(move || {
            let plan = engine_core::workspace_merge::merge(
                &drafted.base,
                &team,
                &drafted.local,
                &BTreeMap::new(),
            );
            let idx = RefIndex::from_states(&[&team, &drafted.local]);
            let listed: Vec<Value> = plan
                .conflicts
                .iter()
                .take(SHOWN_ITEMS)
                .map(|c| {
                    json!({
                        "ref": idx.ref_of(&c.node_id).map(|r| r.as_str().to_owned()),
                        "field": bounded(&c.field, 40),
                        "team": Hash::of(&c.team).h6(),
                        "draft": Hash::of(&c.draft).h6(),
                    })
                })
                .collect();
            Ok::<_, AgentError>((plan.conflicts.len(), listed))
        })
        .await?;
        conflict_total = total;
        conflicts = listed;
    }
    drop(db);

    // What the reviewers said: untrusted text, so cleaned and bounded.
    let latest: Vec<Value> = review["reviews"]
        .as_array()
        .map(|all| {
            all.iter()
                .rev()
                .take(SHOWN_ITEMS)
                .map(|r| {
                    let node = r["node_id"]
                        .as_str()
                        .filter(|n| n.len() == 26 && n.bytes().all(|b| b.is_ascii_alphanumeric()))
                        .map(str::to_owned);
                    json!({
                        "verdict": r["verdict"],
                        "by": bounded(r["reviewer"]["name"].as_str().unwrap_or(""), SHOWN_NAME_CHARS),
                        "note": bounded(r["note"].as_str().unwrap_or(""), SHOWN_NOTE_CHARS),
                        "stale": r["stale"],
                        "node": node,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let approvals = review["approvals"].as_i64().unwrap_or(0);
    let required = review["required_approvals"].as_i64().unwrap_or(0);

    let dct = ctx.format == http::Format::Dct;
    let mut fields = vec![
        ("what", json!("proposal")),
        ("proposal", json!(found.id)),
        (
            "file",
            if dct {
                json!(short(found.document))
            } else {
                json!(found.document)
            },
        ),
        ("state", json!(state_name)),
        ("base", json!(found.base_seq)),
        ("head", json!(point.head)),
        ("behind", json!(behind)),
        ("revision", json!(found.revision)),
        ("edits", request["edits"].clone()),
        ("approvals", json!(approvals)),
        ("required", json!(required)),
        ("satisfied", review["satisfied"].clone()),
        ("conflicts", json!(conflict_total)),
    ];
    if !dct {
        fields.push(("reviews", json!(latest)));
        fields.push(("conflict_list", json!(conflicts)));
        fields.push(("merged_at", request["merged_at"].clone()));
    }
    // DCT body: one line per review and per conflict; text only ever inside quotes.
    let mut lines: Vec<String> = Vec::new();
    for r in &latest {
        lines.push(format!(
            "review {} by={} note={}\n",
            r["verdict"].as_str().unwrap_or("comment"),
            quote(r["by"].as_str().unwrap_or("")),
            quote(r["note"].as_str().unwrap_or(""))
        ));
    }
    for c in &conflicts {
        lines.push(format!(
            "conflict @{} field={} team=#{} draft=#{}\n",
            c["ref"].as_str().unwrap_or("?"),
            c["field"].as_str().unwrap_or("?"),
            c["team"].as_str().unwrap_or("?"),
            c["draft"].as_str().unwrap_or("?")
        ));
    }
    let body = answer(ctx.format, Tool::Status, fields, &lines);
    Ok(Served {
        bytes: body.len(),
        response: http::reply(ctx.format, body, None),
    })
}

// --- list ------------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ListQuery {
    state: Option<String>,
    offset: Option<i64>,
    limit: Option<i64>,
}

/// The agent's own proposals in this project (the list the `status` tool lacked): each with
/// its state, base, head, revision and size, newest first, open ones before closed. Only the
/// proposals of the agent identity of this key's product, in files the person can open and the
/// policy shows; a key that has no agent identity yet has none.
#[utoipa::path(operation_id="agent_proposals", get, path="/projects/{id}/agent/proposals", tag="agent", params(("id"=Uuid,Path,description="Project"),("state"=Option<String>,Query,description="active (draft, open and changes_requested; the default), all, or one of draft, open, changes_requested, merged, declined, withdrawn"),("offset"=Option<i64>,Query,description="0 to 1000"),("limit"=Option<i64>,Query,description="1 to 50, default 20")), security(("device_session"=["content:read"])), responses((status=200, description="This agent's proposals in the project, newest activity first: proposal (the id the submit, withdraw and status routes take), file, state, base, head, behind, revision, edits (engine operations stored), created_at and submitted_at; more says whether another page exists; the budget fields say how many proposals the person's agents have opened today and have open against the limits. Ids, numbers and states only", content((super::schema::AgentProposalList="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=400, description="Unknown state, or offset or limit out of range"),(status=401, description="Not a project device session"),(status=403, description="Scope, grant, key class, membership or policy refuses"),(status=429, description="Budget")))]
pub(super) async fn list(
    State(state): State<AppState>,
    auth: AgentAuth<ContentRead>,
    Path(project): Path<Uuid>,
    headers: HeaderMap,
    uri: Uri,
    parsed: Result<Query<ListQuery>, QueryRejection>,
) -> Out {
    let digest = ledger::request_digest(&[
        b"GET",
        uri.path().as_bytes(),
        uri.query().unwrap_or("").as_bytes(),
    ]);
    let ctx = Ctx::new(&state, &auth, project, &headers, Tool::Status, digest)?;
    let mut call = Call::default();
    let result = list_call(&ctx, &mut call, parsed).await;
    ctx.finish(call, result).await
}

async fn list_call(
    ctx: &Ctx<'_>,
    call: &mut Call,
    parsed: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Served, AgentFail> {
    let Query(q) = parsed.map_err(|_| AgentError::BadRequest { field: "query" })?;
    let filter = super::inbox::state_filter(q.state.as_deref())
        .map_err(|_| AgentError::BadRequest { field: "state" })?;
    let (offset, limit) = super::inbox::paging(q.offset, q.limit, 20)
        .map_err(|_| AgentError::BadRequest { field: "limit" })?;
    let mut admitted = ctx.admit(call).await?;
    let policy = admitted.allowed.policy.clone();
    let agent = identity::resolve(&mut admitted.db, ctx.identity, ctx.key)
        .await
        .map_err(ApiError::from)?;
    let mut rows: Vec<Value> = Vec::new();
    if let Some(agent) = &agent {
        let matches = super::inbox::state_matches("k.state", "$7");
        let states = reviews::state_sql("d");
        let sql = format!(
            "select jsonb_build_object('proposal',c.id,'file',c.document_id,'state',c.state,'base',c.base_seq,'head',x.head,'behind',greatest(x.head-c.base_seq,0),'revision',c.revision,'edits',(select count(*) from draft_edit e where e.draft_id=c.id),'created_at',c.created_at,'submitted_at',c.submitted_at) \
             from (select k.* from (select d.id,d.revision,d.base_seq,d.created_at,d.submitted_at,d.document_id,{states} as state \
                   from workspace_draft d join document f on f.id=d.document_id \
                   where d.created_by=$1 and document_project(d.document_id)=$2 and f.deleted_at is null and coalesce(f.settings->>'kind','questionnaire')=any($6::text[]) and document_member_role(f.id,$3) is not null and not exists(select 1 from file_agent_policy fp where fp.document_id=f.id and fp.access='none') \
                   order by coalesce(d.submitted_at,d.created_at) desc,d.id limit 2000) k \
                   where {matches} \
                   order by (k.state in ('merged','declined','withdrawn')),coalesce(k.submitted_at,k.created_at) desc,k.id limit $5 offset $4) c \
             cross join lateral (select coalesce(max(seq),0) as head from event where document_id=c.document_id) x \
             order by (c.state in ('merged','declined','withdrawn')),coalesce(c.submitted_at,c.created_at) desc,c.id"
        );
        rows = sqlx::query_scalar(&sql)
            .bind(agent.id)
            .bind(ctx.project)
            .bind(ctx.identity)
            .bind(offset)
            .bind(limit + 1)
            .bind(&policy.kinds)
            .bind(filter)
            .fetch_all(&mut *admitted.db)
            .await
            .map_err(ApiError::from)?;
    }
    let more = rows.len() as i64 > limit;
    rows.truncate(limit as usize);
    let (today, open) = super::write::proposal_use(&mut admitted.db, ctx.identity, ctx.project)
        .await
        .map_err(ApiError::from)?;
    drop(admitted.db);

    let dct = ctx.format == http::Format::Dct;
    let mut fields = vec![
        ("what", json!("proposals")),
        ("state", json!(filter)),
        ("n", json!(rows.len())),
        ("more", json!(more)),
        ("write_mode", json!(policy.write_mode)),
        ("opened_today", json!(today)),
        ("opened_limit", json!(policy.proposals_per_day)),
        ("open", json!(open)),
        ("open_limit", json!(policy.open_proposals)),
    ];
    let mut lines: Vec<String> = Vec::new();
    if dct {
        // Ids, numbers and state words only: nothing from a document or a person.
        for r in &rows {
            let file = r["file"]
                .as_str()
                .and_then(|f| Uuid::parse_str(f).ok())
                .map(short)
                .unwrap_or_default();
            lines.push(format!(
                "proposal {} file={} state={} base={} behind={} revision={} edits={}\n",
                r["proposal"].as_str().unwrap_or("?"),
                file,
                r["state"].as_str().unwrap_or("?"),
                r["base"],
                r["behind"],
                r["revision"],
                r["edits"]
            ));
        }
    } else {
        fields.push(("proposals", json!(rows)));
    }
    let body = answer(ctx.format, Tool::Status, fields, &lines);
    Ok(Served {
        bytes: body.len(),
        response: http::reply(ctx.format, body, None),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_note_is_short_plain_data() {
        assert_eq!(note_of(b"").unwrap(), "");
        assert_eq!(note_of(b"  \n").unwrap(), "");
        assert_eq!(
            note_of(br#"{"note":"  Ready for review.  "}"#).unwrap(),
            "Ready for review."
        );
        assert_eq!(note_of(br#"{}"#).unwrap(), "");
        // The escapes are JSON's own: control, bidirectional and zero-width characters.
        let holding = |c: char| format!("{{\"note\":\"x{c}y\"}}").into_bytes();
        let bad: Vec<Vec<u8>> = vec![
            br#"{"note":"a\u0000b"}"#.to_vec(),
            holding('\u{202E}'),
            holding('\u{200B}'),
            holding('\u{7}'),
            br#"{"note":1}"#.to_vec(),
            br#"{"note":"ok","extra":1}"#.to_vec(),
            b"not json".to_vec(),
        ];
        for body in &bad {
            assert!(note_of(body).is_err(), "{}", String::from_utf8_lossy(body));
        }
        // A zero-width joiner, which Bengali and Persian need, is allowed.
        assert!(note_of(&holding('\u{200D}')).is_ok());
        let long = format!(r#"{{"note":"{}"}}"#, "n".repeat(1001));
        assert!(matches!(
            note_of(long.as_bytes()),
            Err(AgentFail::Agent(AgentError::LimitExceeded {
                field: "note",
                ..
            }))
        ));
        let exact = format!(r#"{{"note":"{}"}}"#, "n".repeat(1000));
        assert_eq!(note_of(exact.as_bytes()).unwrap().chars().count(), 1000);
    }

    #[test]
    fn reviewer_text_is_cleaned_and_cut() {
        assert_eq!(bounded("Keep 'policy'.", 400), "Keep 'policy'.");
        assert_eq!(bounded("a\u{202E}b\u{0}c\u{200B}", 10), "abc");
        let cut = bounded(&"x".repeat(500), 400);
        assert_eq!(cut.chars().count(), 401);
        assert!(cut.ends_with('\u{2026}'));
    }
}
