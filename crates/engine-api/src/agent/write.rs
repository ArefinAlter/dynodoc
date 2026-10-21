//! The write half of the agent plane (docs/AI-AGENT-CONNECTORS.md sections 6 and 11.3,
//! docs/AI-AGENT-PROTOCOL-SPEC.md 7.7 to 7.9, 9 and 12.1 routes 10 to 13): `propose`,
//! `submit`, `withdraw` and the status of a proposal.
//!
//! | Route | Scope | |
//! |---|---|---|
//! | `POST /projects/{id}/agent/files/{file}/propose` | `content:propose` | stage ops in a draft owned by the agent identity (`propose`) |
//! | `POST /projects/{id}/agent/proposals/{proposal}/submit` | `content:submit` | make the draft a change request, if the policy lets an agent ([`proposal`]) |
//! | `POST /projects/{id}/agent/proposals/{proposal}/withdraw` | `content:submit` | close the agent's own proposal ([`proposal`]) |
//! | `GET /projects/{id}/agent/proposals/{proposal}` | `content:read` | its state, reviews and conflicts ([`proposal`]) |
//!
//! # The gate
//!
//! **Nothing here appends to the team's event log.** A proposal is a `workspace_draft`
//! created by an agent identity ([`super::identity`]) with `draft_edit` rows, a receipt, an
//! audit row and a call-ledger row: exactly the tables a person's draft uses, plus the
//! receipt. The team version changes only when a person merges the change request, through
//! `reviews::merge`, under the file's review rules. The code of this module never calls
//! `append_to_team`, `log::append_in_tx` or `apply::materialize_node_change`, and the tests
//! compare the row counts and chain head of the event, snapshot and checkpoint tables before
//! and after every call.
//!
//! # Authority, in the order it is checked
//!
//! 1. [`crate::auth::agent::AgentAuth`] with the route's scope: a device session whose key is
//!    unrevoked, enrolled for an AI agent and granted that scope for this project, whose account
//!    is active in the session's generation and whose session post-dates the project's last
//!    kill switch. Re-read on every request.
//! 2. The person's project role, the admission permit and the project policy
//!    (`Ctx::admit`): enabled, role at or above the floor, room in the grant's call and byte
//!    budgets. Then, for `propose`, `write_mode` is not `none`.
//! 3. The file: in this project, a kind the policy admits, not hidden by its file policy (`404`
//!    for every reason), then a file policy of `read` refuses writes (`policy_denied`).
//! 4. The agent identity of the key's product ([`super::identity::resolve`]).
//! 5. Inside the write transaction, with the document and project locked and the account, policy
//!    and file-policy rows `FOR SHARE` (`verify_write`): everything above once more, so that a
//!    kill switch, a revoked grant, a disabled or erased account, a changed policy or a demoted
//!    person either finishes after this write commits or is seen by it. The file is looked up
//!    again too (it may have been trashed, moved to another project or lost its kind since).
//!    The person's file role is `effective_document_role` as `create_draft` reads it: a viewer
//!    cannot stage drafts.
//!
//! # What a call may hold
//!
//! A call that stores holds the document write lock and one pool connection, so it is bounded:
//! statements at most 10 s, the lock wait 3 s, and the compiler `COMPILE_DEADLINE` (2 s,
//! then `deadline_exceeded`). A `dry_run` stores nothing and so holds neither: it reads the
//! state the read routes read and compiles, with no lock and no transaction. `submit` and
//! `withdraw` take the lock but do not materialise the file.
//!
//! Which proposal a path or body names is never trusted: a proposal is found by `created_by =
//! the agent identity` and the document's project, and every other case (a person's draft,
//! another agent's, another project's, an invented id, a file the person lost) is the same
//! `404`.
//!
//! # Decisions and deviations from the specification
//!
//! 1. **Route names.** `POST .../files/{file}/propose` rather than the spec's
//!    `.../proposals`: a Phase 0 test pins `POST .../files/{file}/proposals` as absent. A
//!    proposal id that is not a UUID is `404` before the credential is looked at
//!    (`ProposalId`), which keeps the other Phase 0 assertion about `.../proposals/x/...`.
//! 2. **The proposal id is the draft's id, chosen by the server**, not the first
//!    `request_id`. A client-chosen primary key in a table shared with every draft would let
//!    two clients that reuse a request id collide, and tell each other that a draft exists.
//!    `request_id` is the idempotency key (agent identity, request id), kept in the receipt.
//! 3. **Replays return the receipt, not the diff.** The same `request_id` with the same canonical
//!    digest answers with the stored counts and stale refs and `replayed`, and the proposal's
//!    state now; it does not re-send the preview (the receipt holds no document text).
//!    A dry run never reads or writes a receipt. The digest covers `{file, rev, proposal, ops,
//!    strict}` exactly as received; `name` and `return` are left out so a retry may change them.
//! 4. **`Idempotency-Key` is not required** on `submit` and `withdraw`: both are idempotent by
//!    state (a second `submit` of an open proposal answers `open` and writes nothing).
//! 5. **`submit` is allowed only when `write_mode` is `change_request` and
//!    `submit_requires_person` is false**; otherwise it changes nothing and answers
//!    `state=awaiting_person` (and the ledger outcome says so). With `write_mode` `none` it is
//!    `policy_denied`. `withdraw` is allowed whenever the agent is otherwise admitted: it only
//!    reduces what is pending.
//! 6. **Conflicts are signalled, not refused, at submit.** `status` lists them (same-field
//!    overlaps of the three-way merge, by ref, field and short hashes); the merge by a person
//!    refuses to proceed until each is resolved. The spec's `conflict` error at submit is not
//!    raised.
//! 7. **Budgets.** `proposals_per_day` (proposals opened per UTC day) and `open_proposals` are
//!    counted per person and project (over all of the person's agent identities, so that more
//!    client products do not multiply them), under an advisory lock so concurrent proposals
//!    cannot overrun them; `ops_per_proposal` bounds the engine ops of one proposal in all. A
//!    refusal stores nothing.
//! 8. **Review link.** The API does not know the web app's address; responses give the
//!    document and proposal ids instead of a URL.
//! 9. **Withheld blocks.** No DLP exists, so the compiler is given no redaction set.

use std::{collections::BTreeSet, sync::Arc, time::Duration};

use axum::{
    extract::{
        rejection::JsonRejection, DefaultBodyLimit, FromRequestParts, Path, RawPathParams, State,
    },
    http::{request::Parts, HeaderMap, StatusCode, Uri},
    routing::{get, post},
    Json, Router,
};
use engine_agent::{
    compile::{compile, CompileInput, DerivedIds},
    dct::{HeaderBuilder, Tool},
    diff::Detail,
    error::{AgentError, CompileError},
    grammar::{parse_propose, Op, ProposeArgs, ReturnKind, MAX_BODY_BYTES},
    hash::Hash,
    model::{FileKind, FileMeta},
    preview::{preview, PreviewOptions},
};
use engine_core::{governance::Role, materializer::DocumentState};
use engine_shared::{DocumentId, IdentityId};
use serde_json::{json, Map, Value};
use sqlx::{pool::PoolConnection, PgConnection, Postgres, Transaction};
use uuid::Uuid;

use super::{
    error::AgentFail,
    http::{self, Format},
    identity::{self, AgentIdentity},
    ledger,
    policy::{self, Policy},
    proposal,
    routes::{Call, Ctx, Out, Served},
    state,
};
use crate::{
    auth::agent::{verify_current, AgentAuth, Claimed, ContentPropose},
    error::ApiError,
    ops::apply,
    product::audit,
    projects, reviews,
    workloads::ReadPermit,
    AppState,
};

/// The submit and withdraw bodies hold one short note.
const NOTE_BODY_LIMIT: usize = 16 * 1024;
/// The most time the compiler may take on one call. A write holds the document lock and a
/// pool connection meanwhile, so a call that would take longer is `deadline_exceeded`
/// (`504`, the protocol's nearest code: a write is retried with the same `request_id`).
const COMPILE_DEADLINE: Duration = Duration::from_secs(2);

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/projects/{id}/agent/files/{file}/propose",
            post(propose).layer(DefaultBodyLimit::max(MAX_BODY_BYTES)),
        )
        .route(
            "/projects/{id}/agent/proposals/{proposal}/submit",
            post(proposal::submit).layer(DefaultBodyLimit::max(NOTE_BODY_LIMIT)),
        )
        .route(
            "/projects/{id}/agent/proposals/{proposal}/withdraw",
            post(proposal::withdraw).layer(DefaultBodyLimit::max(NOTE_BODY_LIMIT)),
        )
        .route(
            "/projects/{id}/agent/proposals/{proposal}",
            get(proposal::status),
        )
        .route("/projects/{id}/agent/proposals", get(proposal::list))
}

// --- the path ------------------------------------------------------------------------

/// The `{proposal}` segment of a proposal route, which must be a UUID. Anything else cannot
/// name a proposal and is `404` at once, before the credential is read: there is nothing to
/// find, and the answer is the one an absent route gives.
pub(super) struct ProposalId(pub Uuid);

impl<S: Send + Sync> FromRequestParts<S> for ProposalId {
    type Rejection = AgentFail;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, AgentFail> {
        let params = RawPathParams::from_request_parts(parts, state)
            .await
            .map_err(|_| not_found())?;
        params
            .iter()
            .find(|(name, _)| *name == "proposal")
            .and_then(|(_, value)| Uuid::parse_str(value).ok())
            .map(Self)
            .ok_or_else(not_found)
    }
}

/// The one answer for a proposal, file or reference the caller may not see.
pub(super) fn not_found() -> AgentFail {
    AgentError::NotFound { refs: Vec::new() }.into()
}

/// A policy refusal that is also recorded in the ledger as `policy_denied` (at most one row
/// a minute per grant, like every refusal of the policy).
pub(super) fn denied(call: &mut Call, rule: &'static str) -> AgentFail {
    call.refused = Some("policy_denied");
    AgentError::PolicyDenied { policy: rule }.into()
}

// --- canonical digest and bodies -------------------------------------------------------

/// The canonical request digest of a `propose` call (spec 9.3): SHA-256 over the canonical
/// JSON (sorted keys, compact) of `{file, rev, proposal, ops, strict}`. `name`, `return` and
/// `dry_run` are not part of it, so a retry may change how the answer looks.
pub fn request_digest(
    file: Uuid,
    rev: u64,
    proposal: Option<Uuid>,
    ops: &Value,
    strict: bool,
) -> [u8; 32] {
    let hash = Hash::of(&json!({
        "file": file,
        "rev": rev,
        "proposal": proposal,
        "ops": ops,
        "strict": strict,
    }));
    let mut out = [0u8; 32];
    // A fingerprint is 64 lowercase hex characters.
    for (byte, pair) in out.iter_mut().zip(hash.as_hex().as_bytes().chunks(2)) {
        *byte = u8::from_str_radix(std::str::from_utf8(pair).unwrap_or("00"), 16).unwrap_or(0);
    }
    out
}

/// The JSON body of a write call, or the protocol error for a body that is too large or not
/// JSON (an unknown shape is the grammar's `bad_request`, naming the field).
pub(super) fn json_body(parsed: Result<Json<Value>, JsonRejection>) -> Result<Value, AgentFail> {
    parsed.map(|Json(body)| body).map_err(|rejection| {
        if rejection.status() == StatusCode::PAYLOAD_TOO_LARGE {
            AgentError::LimitExceeded {
                field: "body",
                limit: MAX_BODY_BYTES as u64,
                value: MAX_BODY_BYTES as u64 + 1,
            }
            .into()
        } else {
            AgentError::BadRequest { field: "body" }.into()
        }
    })
}

/// The arguments of a `propose` call. The route names the file, so the body may leave `file`
/// out; if it says one it must be this one.
fn parse_call(body: &Value, document: Uuid) -> Result<ProposeArgs, CompileError> {
    let mut map = body
        .as_object()
        .cloned()
        .ok_or(AgentError::BadRequest { field: "body" })?;
    match map.get("file") {
        None => {
            map.insert("file".into(), json!(document));
        }
        Some(named) if named.as_str().and_then(|s| Uuid::parse_str(s).ok()) == Some(document) => {}
        Some(_) => return Err(AgentError::BadRequest { field: "file" }.into()),
    }
    parse_propose(&Value::Object(map))
}

// --- answers ---------------------------------------------------------------------------

/// A short response for the write tools other than the preview: the fields as JSON, or as one
/// DCT header (scalars only) and the given body lines. Nothing from a document is in the
/// header; the lines are the caller's to escape.
pub(super) fn answer(
    format: Format,
    tool: Tool,
    fields: Vec<(&'static str, Value)>,
    lines: &[String],
) -> String {
    match format {
        Format::Json => {
            let mut map = Map::new();
            map.insert("protocol".into(), "dynodoc.agent/1".into());
            map.insert("tool".into(), tool.as_str().into());
            for (key, value) in fields {
                map.insert(key.into(), value);
            }
            map.insert("trust".into(), "data".into());
            Value::Object(map).to_string()
        }
        Format::Dct => {
            let mut header = HeaderBuilder::new(tool);
            for (key, value) in &fields {
                header = match value {
                    Value::String(s) if s.is_empty() => header,
                    Value::String(s) => header.kv(key, s),
                    Value::Number(n) => header.kv(key, n),
                    Value::Bool(b) => header.kv(key, u8::from(*b)),
                    _ => header,
                };
            }
            header.kv("trust", "data").assemble(&lines.concat())
        }
    }
}

/// `value` is the first 8 hex characters of a UUID, as the DCT headers name files and
/// proposals.
pub(super) fn short(id: Uuid) -> String {
    id.simple().to_string()[..8].to_owned()
}

/// Add `extras` to the top level of a JSON mirror (existing keys are kept).
fn add_fields(mirror: &str, extras: Value) -> String {
    let Ok(Value::Object(mut map)) = serde_json::from_str::<Value>(mirror) else {
        return mirror.to_owned();
    };
    if let Value::Object(extras) = extras {
        for (key, value) in extras {
            map.entry(key).or_insert(value);
        }
    }
    Value::Object(map).to_string()
}

/// Run CPU-bound work off the async workers.
pub(super) async fn blocking<T, E, F>(work: F) -> Result<T, AgentFail>
where
    F: FnOnce() -> Result<T, E> + Send + 'static,
    T: Send + 'static,
    E: Into<AgentFail> + Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => result.map_err(Into::into),
        Err(error) => {
            tracing::error!(%error, "agent write task failed");
            Err(AgentError::Internal.into())
        }
    }
}

/// The document write lock and the person's role in the file, as `apply::begin_write_locked`
/// takes them, except that a file that has gone while the call waited for the lock (trashed,
/// or moved to a project the person is not in, so that they have no role in it) is the same
/// `404` as every other way a file can be unavailable, not a `403`.
pub(super) async fn begin_locked<'a>(
    ctx: &Ctx<'a>,
    document: Uuid,
) -> Result<(Transaction<'a, Postgres>, Role), AgentFail> {
    match apply::begin_write_locked(ctx.state, DocumentId(document), IdentityId(ctx.identity)).await
    {
        Ok(begun) => Ok(begun),
        Err(ApiError::Forbidden) => {
            let present: bool = sqlx::query_scalar("select exists(select 1 from document d where d.id=$1 and d.deleted_at is null and document_project(d.id)=$2)")
                .bind(document)
                .bind(ctx.project)
                .fetch_one(&ctx.state.pool)
                .await
                .map_err(ApiError::from)?;
            Err(if present {
                ApiError::Forbidden.into()
            } else {
                not_found()
            })
        }
        Err(other) => Err(other.into()),
    }
}

/// [`blocking`] with a deadline. The work is not cancelled (a blocking task cannot be), but
/// the caller stops waiting for it, so the document lock and the connection it holds are
/// released; anything the work captured, such as the admission permit, is released only when
/// it really ends.
pub(super) async fn within<T, E, F>(deadline: Duration, work: F) -> Result<T, AgentFail>
where
    F: FnOnce() -> Result<T, E> + Send + 'static,
    T: Send + 'static,
    E: Into<AgentFail> + Send + 'static,
{
    match tokio::time::timeout(deadline, blocking(work)).await {
        Ok(done) => done,
        Err(_) => Err(AgentError::DeadlineExceeded.into()),
    }
}

// --- the transaction's second look ------------------------------------------------------

/// The facts `admit` read, read again inside a write transaction that holds the document and
/// project locks: the session and key, grant and account (`FOR SHARE` on the account row), the
/// policy (`FOR SHARE`), the person's project role and the file's policy. Returns the policy as
/// locked. Anything that changed since `admit` is refused with the same answer `admit` would
/// have given, and a change that has not committed yet waits for this transaction.
pub(super) async fn verify_write(
    tx: &mut Transaction<'_, Postgres>,
    ctx: &Ctx<'_>,
    document: Uuid,
    writing: bool,
) -> Result<Policy, AgentFail> {
    verify_current(
        tx,
        &Claimed {
            identity: ctx.identity,
            key: ctx.key,
            project: ctx.project,
            session: ctx.session,
            generation: ctx.generation,
            scope: ctx.scope,
        },
        true,
    )
    .await?;
    let stored = policy::load_shared(tx, ctx.project)
        .await
        .map_err(ApiError::from)?;
    let Some(policy) = stored.map(|s| s.policy).filter(|p| p.enabled) else {
        return Err(ApiError::Forbidden.into());
    };
    let role = projects::require(tx, ctx.project, ctx.identity).await?;
    if !policy::role_allowed(&role, &policy) {
        return Err(ApiError::Forbidden.into());
    }
    if writing && !policy.can_propose() {
        return Err(AgentError::PolicyDenied {
            policy: "write_mode",
        }
        .into());
    }
    // The file is looked up before the lock is taken; a move to another project, a trashing
    // or a change of the policy's kinds in between is seen here (the lock holds the row).
    let reachable: bool = sqlx::query_scalar("select exists(select 1 from document d where d.id=$1 and d.deleted_at is null and document_project(d.id)=$2 and coalesce(d.settings->>'kind','questionnaire')=any($3::text[]))")
        .bind(document)
        .bind(ctx.project)
        .bind(&policy.kinds)
        .fetch_one(&mut **tx)
        .await
        .map_err(ApiError::from)?;
    if !reachable {
        return Err(not_found());
    }
    let access: Option<String> =
        sqlx::query_scalar("select access from file_agent_policy where document_id=$1 for share")
            .bind(document)
            .fetch_optional(&mut **tx)
            .await
            .map_err(ApiError::from)?;
    match access.as_deref() {
        Some("none") => return Err(not_found()),
        Some("read") if writing => {
            return Err(AgentError::PolicyDenied { policy: "file" }.into());
        }
        _ => {}
    }
    Ok(policy)
}

/// The file policy of `document` as the caller's pre-check reads it: `read` lets agents read
/// the file and nothing more.
async fn require_writable_file(
    db: &mut PgConnection,
    document: Uuid,
    call: &mut Call,
) -> Result<(), AgentFail> {
    let access: Option<String> =
        sqlx::query_scalar("select access from file_agent_policy where document_id=$1")
            .bind(document)
            .fetch_optional(&mut *db)
            .await
            .map_err(ApiError::from)?;
    if access.as_deref() == Some("read") {
        return Err(denied(call, "file"));
    }
    Ok(())
}

// --- proposal budgets ------------------------------------------------------------------

/// What one person's agents have used in a project: proposals opened since midnight UTC and
/// proposals still open (not merged, declined or withdrawn), over all of the person's agent
/// identities, so that more client products do not multiply the limit.
pub(super) async fn proposal_use(
    db: &mut PgConnection,
    principal: Uuid,
    project: Uuid,
) -> Result<(i64, i64), sqlx::Error> {
    sqlx::query_as("select (select count(*) from agent_proposal_receipt where principal_id=$1 and project_id=$2 and opened and at>=date_trunc('day',now() at time zone 'utc') at time zone 'utc'),(select count(*) from agent_proposal_receipt r join workspace_draft d on d.id=r.draft_id where r.principal_id=$1 and r.project_id=$2 and r.opened and d.merged_at is null and d.review_outcome is null)")
        .bind(principal)
        .bind(project)
        .fetch_one(db)
        .await
}

/// The refusal for a spent proposal budget, with the time until it has room again.
async fn budget_refusal(
    db: &mut PgConnection,
    policy: &Policy,
    (today, open): (i64, i64),
) -> Result<Option<AgentError>, sqlx::Error> {
    if today >= i64::from(policy.proposals_per_day) {
        let wait: f64 = sqlx::query_scalar(policy::DAY_RETRY_SQL)
            .fetch_one(db)
            .await?;
        return Ok(Some(AgentError::BudgetExceeded {
            retry_after: wait.ceil().clamp(1.0, 86_400.0) as u64,
        }));
    }
    if open >= i64::from(policy.open_proposals) {
        // Nothing ages out: a person has to merge, decline or withdraw one.
        return Ok(Some(AgentError::BudgetExceeded { retry_after: 3_600 }));
    }
    Ok(None)
}

// --- propose ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct Receipt {
    draft_id: Uuid,
    request_digest: Vec<u8>,
    rev: i64,
    ops_in: i32,
    ops_engine: i32,
    stale: i32,
    stale_refs: Vec<String>,
    revision: i64,
    opened: bool,
    document_id: Uuid,
    base_seq: i64,
}

/// The proposal's state word now, from its row: `draft`, `open`, or a closed outcome.
pub(super) fn state_word(submitted: bool, merged: bool, outcome: Option<&str>) -> &'static str {
    match (merged, outcome) {
        (true, _) | (_, Some("merged")) => "merged",
        (_, Some("declined")) => "declined",
        (_, Some("withdrawn")) => "withdrawn",
        _ if submitted => "open",
        _ => "draft",
    }
}

/// Stage edits as a reviewable proposal (spec 7.7). The ops are compiled by the pure compiler
/// of `engine-agent` against the state they apply to and stored, exactly as compiled, as
/// `draft_edit` rows of a draft the agent identity owns. Nothing reaches the team version.
#[utoipa::path(operation_id="agent_propose", post, path="/projects/{id}/agent/files/{file}/propose", tag="agent", params(("id"=Uuid,Path,description="Project"),("file"=String,Path,description="File UUID, or its first 8 hex characters when unique among the caller's files")), request_body=super::schema::AgentProposeBody, security(("device_session"=["content:propose"])), responses((status=200, description="The proposal after the call (or, with dry_run, what the call would do: nothing is stored and no receipt is written). The same request_id with the same canonical digest returns the stored receipt with replayed=true. Never changes the team version", content((super::schema::AgentPropose="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=400, description="Invalid body or op (details.agent_code, details.field, details.op names the op), including text with control or hidden characters"),(status=401, description="Not a project device session"),(status=403, description="Scope, grant, key class, membership, role or policy (details.policy: write_mode or file) refuses"),(status=404, description="File or proposal not visible (one answer for every reason)"),(status=409, description="idempotency_conflict, stale_base (strict), match_count, no_match, protected or proposal_closed"),(status=413, description="Body over 1 MiB"),(status=429, description="Call, byte or proposal budget (Retry-After)")))]
async fn propose(
    State(state): State<AppState>,
    auth: AgentAuth<ContentPropose>,
    Path((project, file)): Path<(Uuid, String)>,
    headers: HeaderMap,
    uri: Uri,
    parsed: Result<Json<Value>, JsonRejection>,
) -> Out {
    let ctx = Ctx::new(
        &state,
        &auth,
        project,
        &headers,
        Tool::Propose,
        ledger::request_digest(&[b"POST", uri.path().as_bytes()]),
    )?;
    let mut call = Call::default();
    let result = propose_call(&ctx, &mut call, &file, parsed).await;
    ctx.finish(call, result).await
}

async fn propose_call(
    ctx: &Ctx<'_>,
    call: &mut Call,
    file: &str,
    parsed: Result<Json<Value>, JsonRejection>,
) -> Result<Served, AgentFail> {
    let mut admitted = ctx.admit(call).await?;
    if !admitted.allowed.policy.can_propose() {
        return Err(denied(call, "write_mode"));
    }
    let (document, kind) = ctx
        .find_file(&mut admitted.db, call, file, &admitted.allowed.policy.kinds)
        .await?;
    require_writable_file(&mut admitted.db, document, call).await?;
    let agent = identity::resolve(&mut admitted.db, ctx.identity, ctx.key)
        .await
        .map_err(ApiError::from)?
        .ok_or(ApiError::Forbidden)?;
    let body = json_body(parsed)?;
    let args = parse_call(&body, document)?;
    let digest = request_digest(document, args.rev, args.proposal, &body["ops"], args.strict);
    call.digest = Some(digest);
    // The permit is held until the compiler is done, even if the caller gives up.
    let permit = admitted._permit;
    let body = if args.dry_run {
        // A dry run stores nothing, so it takes no document lock and opens no write
        // transaction: it reads what the read routes read.
        dry_run_proposal(
            ctx,
            call,
            &agent,
            document,
            kind,
            args,
            admitted.db,
            &admitted.allowed.policy,
            permit,
        )
        .await?
    } else {
        // The connection goes back to the pool.
        drop(admitted.db);
        write_proposal(ctx, call, &agent, document, kind, args, digest, permit).await?
    };
    Ok(Served {
        bytes: body.len(),
        response: http::reply(ctx.format, body, None),
    })
}

/// The draft a call extends, as its row says now.
struct Existing {
    id: Uuid,
    revision: i64,
    base_seq: i64,
    submitted: bool,
    /// The engine operations it holds already.
    ops: i64,
}

/// The proposal `id` of the agent identity `agent` in `document`, open. Only the agent
/// identity's own draft, in this file: a person's draft, another agent's, another file's and
/// an invented id are all the same `404`. `lock` takes the row `FOR UPDATE` (a write).
async fn find_existing(
    db: &mut PgConnection,
    id: Uuid,
    document: Uuid,
    agent: Uuid,
    lock: bool,
) -> Result<Existing, AgentFail> {
    let sql = format!("select revision,base_seq,submitted_at is not null,merged_at is not null,review_outcome from workspace_draft where id=$1 and document_id=$2 and created_by=$3{}", if lock { " for update" } else { "" });
    let row: Option<(i64, i64, bool, bool, Option<String>)> = sqlx::query_as(&sql)
        .bind(id)
        .bind(document)
        .bind(agent)
        .fetch_optional(&mut *db)
        .await
        .map_err(ApiError::from)?;
    let Some((revision, base_seq, submitted, merged, outcome)) = row else {
        return Err(not_found());
    };
    if merged || outcome.is_some() {
        return Err(AgentError::ProposalClosed.into());
    }
    let ops: i64 = sqlx::query_scalar("select count(*) from draft_edit where draft_id=$1")
        .bind(id)
        .fetch_one(&mut *db)
        .await
        .map_err(ApiError::from)?;
    Ok(Existing {
        id,
        revision,
        base_seq,
        submitted,
        ops,
    })
}

/// What one call compiles from.
struct Stage<'a> {
    agent: &'a AgentIdentity,
    document: Uuid,
    kind: FileKind,
    point: &'a state::Point,
    /// The id timestamp of the revision the agent read.
    stamp: u64,
    /// What the ops apply to: the state the agent read for a new proposal, the proposal's own
    /// current state for an extension (spec 9.1).
    before: Arc<DocumentState>,
    /// The team's head, for the `base` comparison.
    head: Arc<DocumentState>,
    policy: &'a Policy,
    existing: Option<&'a Existing>,
    proposal_id: Uuid,
    revision_after: i64,
}

/// A call compiled and previewed, not yet stored. The permit comes back so that the caller
/// holds it as long as it did.
struct Staged {
    compiled: engine_agent::compile::Compiled,
    shown: engine_agent::response::Response<engine_agent::preview::PreviewData>,
    base_seq: i64,
    extras: Value,
    _permit: ReadPermit,
}

/// Compile and preview a call, with at most `COMPILE_DEADLINE` spent on it, and apply the
/// per-proposal operation cap.
async fn stage(
    s: Stage<'_>,
    args: &ProposeArgs,
    ops: Vec<Op>,
    permit: ReadPermit,
) -> Result<Staged, AgentFail> {
    let meta = FileMeta {
        id: DocumentId(s.document),
        kind: s.kind,
        rev: s.point.rev,
        chain: s.point.chain_hex(),
    };
    let state_now = s
        .existing
        .map_or("draft", |e| if e.submitted { "open" } else { "draft" });
    let options = PreviewOptions {
        detail: if args.ret == ReturnKind::Diff {
            Detail::Text
        } else {
            Detail::Summary
        },
        max_bytes: None,
        dry_run: args.dry_run,
        proposal: (!args.dry_run || s.existing.is_some()).then_some(s.proposal_id),
        state: (!args.dry_run).then_some(state_now),
        revision: (!args.dry_run).then_some(s.revision_after as u64),
    };
    let seed = format!("{}:{}", s.agent.id, args.request_id);
    let (strict, stamp) = (args.strict, s.stamp);
    let (compiled, shown, permit) = {
        let (before, head, meta) = (s.before.clone(), s.head.clone(), meta.clone());
        // The permit goes with the work: a call that times out frees the document, not the
        // slot, until the compiler is really done.
        within(COMPILE_DEADLINE, move || {
            let mut input = CompileInput::new(&meta, &before, Role::Author);
            input.head = Some(&*head);
            input.strict = strict;
            input.redacted = BTreeSet::new();
            let mut ids = DerivedIds::new(stamp, &seed);
            let compiled = compile(&input, &ops, &mut ids)?;
            let shown = preview(&meta, &before, &compiled, &options).map_err(CompileError::from)?;
            Ok::<_, CompileError>((compiled, shown, permit))
        })
        .await?
    };
    if compiled.ops.is_empty() {
        return Err(AgentError::NoChange.into());
    }
    let existing_ops = s.existing.map_or(0, |e| e.ops);
    let total = existing_ops + compiled.ops.len() as i64;
    if total > i64::from(s.policy.ops_per_proposal) {
        return Err(AgentError::LimitExceeded {
            field: "engine_ops",
            limit: s.policy.ops_per_proposal as u64,
            value: total as u64,
        }
        .into());
    }

    let base_seq = s.existing.map_or(s.point.rev as i64, |e| e.base_seq);
    let behind = s.point.head.saturating_sub(base_seq as u64);
    let mut warn: Vec<&str> = Vec::new();
    if !compiled.stale.is_empty() {
        warn.push("stale");
    }
    if behind > 0 {
        warn.push("behind");
    }
    if shown.data.truncated {
        warn.push("truncated");
    }
    let extras = json!({
        "proposal": (!args.dry_run || s.existing.is_some()).then_some(s.proposal_id),
        "request_id": args.request_id,
        "state": (!args.dry_run).then_some(state_now),
        "revision": (!args.dry_run).then_some(s.revision_after),
        "base": base_seq,
        "head": s.point.head,
        "behind": behind,
        "stale_total": compiled.stale.len(),
        "dry_run": args.dry_run,
        "replayed": false,
        "warn": warn,
    });
    Ok(Staged {
        compiled,
        shown,
        base_seq,
        extras,
        _permit: permit,
    })
}

/// The answer of a `propose` call in the negotiated form.
fn render(
    ctx: &Ctx<'_>,
    shown: &engine_agent::response::Response<engine_agent::preview::PreviewData>,
    extras: &Value,
) -> String {
    match ctx.format {
        Format::Dct => shown.text.clone(),
        Format::Json => add_fields(&shown.json_mirror(Tool::Propose), extras.clone()),
    }
}

/// A `dry_run` call: what the ops would do, from the state the read routes read. Nothing is
/// locked, stored or counted, and no write transaction is open, so a flood of dry runs
/// cannot hold the document lock or a transaction against the writers. The admission checks
/// are those of the call (`admit`), and the viewer rule of the write.
#[allow(clippy::too_many_arguments)]
async fn dry_run_proposal(
    ctx: &Ctx<'_>,
    call: &mut Call,
    agent: &AgentIdentity,
    document: Uuid,
    kind: FileKind,
    mut args: ProposeArgs,
    mut db: PoolConnection<Postgres>,
    policy: &Policy,
    permit: ReadPermit,
) -> Result<String, AgentFail> {
    let role: Option<String> = sqlx::query_scalar("select effective_document_role($1,$2)")
        .bind(document)
        .bind(ctx.identity)
        .fetch_one(&mut *db)
        .await
        .map_err(ApiError::from)?;
    match role.and_then(|r| r.parse::<Role>().ok()) {
        None | Some(Role::Auditor) => return Err(ApiError::Forbidden.into()),
        Some(_) => {}
    }
    let point = state::point(&mut db, document, Some(args.rev), None).await?;
    call.rev = Some(point.rev);
    let existing = match args.proposal {
        None => None,
        Some(id) => Some(find_existing(&mut db, id, document, agent.id, false).await?),
    };
    let head_point = if point.rev == point.head {
        point.clone()
    } else {
        state::point(&mut db, document, None, None).await?
    };
    let cache = &ctx.state.agent_cache;
    let head = state::load(cache, &mut db, &head_point).await?;
    let (before, proposal_id, revision_after) = match &existing {
        None if point.rev == point.head => (head.clone(), Uuid::new_v4(), 1),
        None => (
            state::load(cache, &mut db, &point).await?,
            Uuid::new_v4(),
            1,
        ),
        Some(e) => {
            let row: Value =
                sqlx::query_scalar("select to_jsonb(d) from workspace_draft d where d.id=$1")
                    .bind(e.id)
                    .fetch_one(&mut *db)
                    .await
                    .map_err(ApiError::from)?;
            let drafted = crate::workspace::replay_draft(&mut db, document, row).await?;
            (Arc::new(drafted.local), e.id, e.revision + 1)
        }
    };
    let stamp = id_timestamp(&mut db, document, point.rev)
        .await
        .map_err(ApiError::from)?;
    // The connection goes back to the pool before the compiler runs.
    drop(db);
    let ops = std::mem::take(&mut args.ops);
    let staged = stage(
        Stage {
            agent,
            document,
            kind,
            point: &point,
            stamp,
            before,
            head,
            policy,
            existing: existing.as_ref(),
            proposal_id,
            revision_after,
        },
        &args,
        ops,
        permit,
    )
    .await?;
    Ok(render(ctx, &staged.shown, &staged.extras))
}

/// The transaction of one `propose` call that stores (a dry run is [`dry_run_proposal`]).
#[allow(clippy::too_many_arguments)]
async fn write_proposal(
    ctx: &Ctx<'_>,
    call: &mut Call,
    agent: &AgentIdentity,
    document: Uuid,
    kind: FileKind,
    mut args: ProposeArgs,
    digest: [u8; 32],
    permit: ReadPermit,
) -> Result<String, AgentFail> {
    let (mut tx, role) = begin_locked(ctx, document).await?;
    let (head, _) = engine_core::snapshot::read_current_state(&mut tx, DocumentId(document))
        .await
        .map_err(ApiError::from)?;
    sqlx::query("set local statement_timeout='10s'")
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    sqlx::query("set local lock_timeout='3s'")
        .execute(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    // A viewer cannot stage drafts (`create_draft` refuses auditors).
    if role == Role::Auditor {
        return Err(ApiError::Forbidden.into());
    }
    let policy = verify_write(&mut tx, ctx, document, true).await?;
    let point = state::point(&mut tx, document, Some(args.rev), None).await?;
    call.rev = Some(point.rev);

    // The same request again: a replay of its receipt, or a conflict.
    let found: Option<Receipt> = sqlx::query_as("select draft_id,request_digest,rev,ops_in,ops_engine,stale,stale_refs,revision,opened,document_id,base_seq from agent_proposal_receipt where agent_identity_id=$1 and request_id=$2 and kind='propose'")
        .bind(agent.id)
        .bind(args.request_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    if let Some(receipt) = found {
        if receipt.request_digest != digest || receipt.document_id != document {
            return Err(AgentError::IdempotencyConflict.into());
        }
        return replay(&mut tx, ctx, &receipt, head_seq(&point)).await;
    }

    // The draft this call extends, or the budgets for a new one.
    let existing = match args.proposal {
        None => None,
        Some(id) => Some(find_existing(&mut tx, id, document, agent.id, true).await?),
    };
    if existing.is_none() {
        // One budget check at a time per person and project, so that two concurrent
        // proposals cannot both take the last place.
        sqlx::query("select pg_advisory_xact_lock(hashtextextended($1::text,384))")
            .bind(format!("agent_proposals:{}:{}", ctx.identity, ctx.project))
            .execute(&mut *tx)
            .await
            .map_err(ApiError::from)?;
        let used = proposal_use(&mut tx, ctx.identity, ctx.project)
            .await
            .map_err(ApiError::from)?;
        if let Some(refusal) = budget_refusal(&mut tx, &policy, used)
            .await
            .map_err(ApiError::from)?
        {
            call.refused = Some("budget_exceeded");
            return Err(refusal.into());
        }
    }

    // What the ops apply to: the state the agent read for a new proposal, the proposal's own
    // current state for an extension (spec 9.1).
    let head = Arc::new(head);
    let (before, proposal_id, revision_after) = match &existing {
        None => {
            let base = if point.rev == point.head {
                head.clone()
            } else {
                Arc::new(reviews::state_through(&mut tx, document, point.rev as i64).await?)
            };
            (base, Uuid::new_v4(), 1)
        }
        Some(e) => {
            let ctxd =
                crate::workspace::load_draft(&mut tx, document, e.id, Some(agent.id)).await?;
            (Arc::new(ctxd.local), e.id, e.revision + 1)
        }
    };
    let stamp = id_timestamp(&mut tx, document, point.rev)
        .await
        .map_err(ApiError::from)?;
    let ops = std::mem::take(&mut args.ops);
    let Staged {
        compiled,
        shown,
        base_seq,
        extras,
        _permit,
    } = stage(
        Stage {
            agent,
            document,
            kind,
            point: &point,
            stamp,
            before: before.clone(),
            head,
            policy: &policy,
            existing: existing.as_ref(),
            proposal_id,
            revision_after,
        },
        &args,
        ops,
        permit,
    )
    .await?;

    // Store. A new proposal is a new draft owned by the agent identity, with the base state
    // the agent read; every proposal gets the compiled ops, in order, as draft edits by it.
    if existing.is_none() {
        let base_json = crate::draft_storage::encode(
            &mut tx,
            DocumentId(document),
            &before,
            ctx.state.shared_draft_bases,
        )
        .await?;
        let name = args.name.clone().unwrap_or_else(|| {
            format!("Proposal from {}", agent.display_name)
                .chars()
                .take(200)
                .collect()
        });
        let source = json!({
            "kind": "agent",
            "client_kind": agent.client_kind,
            "agent": agent.id,
            "principal": ctx.identity,
        });
        sqlx::query("insert into workspace_draft(id,document_id,name,base_seq,base_state,created_by,source,revision,content_changed_at) values($1,$2,$3,$4,$5,$6,$7,0,now())")
            .bind(proposal_id)
            .bind(document)
            .bind(name)
            .bind(point.rev as i64)
            .bind(base_json)
            .bind(agent.id)
            .bind(source)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::from)?;
    }
    for chunk in compiled.ops.chunks(500) {
        sqlx::query("insert into draft_edit(draft_id,payload,actor_id) select $1,e.value,$3 from jsonb_array_elements($2::jsonb) with ordinality as e(value,ord) order by e.ord")
            .bind(proposal_id)
            .bind(json!(chunk))
            .bind(agent.id)
            .execute(&mut *tx)
            .await
            .map_err(ApiError::from)?;
    }
    // New content makes earlier approvals of this draft's change request stale.
    reviews::audit_lapsed_reviews(&mut tx, document, proposal_id, agent.id, "edited").await?;
    let revision: i64 = sqlx::query_scalar("update workspace_draft set revision=revision+1,content_revision=revision+1,content_changed_at=now() where id=$1 returning revision")
        .bind(proposal_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(ApiError::from)?;
    debug_assert_eq!(revision, revision_after);

    // The receipt: counts, the stale refs and the agent's own reasons, no document text.
    let whys: Vec<Value> = compiled
        .results
        .iter()
        .map(|r| {
            let mut entry = json!({"i":r.index,"op":r.op,"n":r.engine_ops});
            if let Some(id) = &r.id {
                entry["id"] = json!(id);
            }
            if let Some(why) = &r.why {
                entry["why"] = json!(why);
            }
            entry
        })
        .collect();
    let stale_refs: Vec<String> = compiled
        .stale
        .iter()
        .take(5)
        .map(|s| s.at.clone())
        .collect();
    let stored = sqlx::query("insert into agent_proposal_receipt(draft_id,document_id,project_id,agent_identity_id,principal_id,key_id,kind,request_id,opened,rev,base_seq,head_seq,ops_in,ops_engine,stale,stale_refs,revision,request_digest,outcome,why) values($1,$2,$3,$4,$5,$6,'propose',$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,'ok',$18)")
        .bind(proposal_id)
        .bind(document)
        .bind(ctx.project)
        .bind(agent.id)
        .bind(ctx.identity)
        .bind(ctx.key)
        .bind(args.request_id)
        .bind(existing.is_none())
        .bind(point.rev as i64)
        .bind(base_seq)
        .bind(point.head as i64)
        .bind(compiled.results.len() as i32)
        .bind(compiled.ops.len() as i32)
        .bind(compiled.stale.len() as i32)
        .bind(&stale_refs)
        .bind(revision)
        .bind(&digest[..])
        .bind(Value::Array(whys))
        .execute(&mut *tx)
        .await;
    match stored {
        Ok(_) => {}
        // A concurrent call with the same request id on another file got there first.
        Err(sqlx::Error::Database(e)) if e.code().as_deref() == Some("23505") => {
            return Err(AgentError::IdempotencyConflict.into());
        }
        Err(other) => return Err(ApiError::from(other).into()),
    }
    audit(
        &mut tx,
        agent.id,
        if existing.is_none() {
            "agent.proposal.created"
        } else {
            "agent.proposal.extended"
        },
        proposal_id,
        json!({
            "file": document,
            "principal": ctx.identity,
            "client_kind": agent.client_kind,
            "base_seq": base_seq,
            "rev": point.rev,
            "ops_in": compiled.results.len(),
            "ops_engine": compiled.ops.len(),
            "stale": compiled.stale.len(),
            "request_digest": hex::encode(digest),
        }),
    )
    .await?;
    tx.commit().await.map_err(ApiError::from)?;
    Ok(render(ctx, &shown, &extras))
}

fn head_seq(point: &state::Point) -> u64 {
    point.head
}

/// The stored answer of a call that was already made.
async fn replay(
    tx: &mut Transaction<'_, Postgres>,
    ctx: &Ctx<'_>,
    receipt: &Receipt,
    head: u64,
) -> Result<String, AgentFail> {
    let row: (bool, bool, Option<String>) = sqlx::query_as(
        "select submitted_at is not null,merged_at is not null,review_outcome from workspace_draft where id=$1",
    )
    .bind(receipt.draft_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(ApiError::from)?;
    let state = state_word(row.0, row.1, row.2.as_deref());
    let behind = head.saturating_sub(receipt.base_seq as u64);
    let mut warn: Vec<&str> = Vec::new();
    if receipt.stale > 0 {
        warn.push("stale");
    }
    if behind > 0 {
        warn.push("behind");
    }
    let dct = ctx.format == Format::Dct;
    // The same keys as the first answer; the receipt keeps a count and at most five refs.
    let stale: Vec<Value> = receipt
        .stale_refs
        .iter()
        .map(|at| json!({"at": at}))
        .collect();
    let fields = vec![
        (
            "file",
            if dct {
                json!(short(receipt.document_id))
            } else {
                json!(receipt.document_id)
            },
        ),
        // The whole id: the proposal routes take nothing shorter.
        ("proposal", json!(receipt.draft_id)),
        ("rev", json!(receipt.rev)),
        ("applied", json!(receipt.ops_in)),
        ("expanded", json!(receipt.ops_engine)),
        (
            "stale",
            if dct {
                json!(receipt.stale)
            } else {
                json!(stale)
            },
        ),
        (
            if dct { "stale_refs" } else { "stale_total" },
            if dct {
                json!(receipt.stale_refs.join(","))
            } else {
                json!(receipt.stale)
            },
        ),
        ("state", json!(state)),
        ("revision", json!(receipt.revision)),
        ("opened", json!(receipt.opened)),
        ("base", json!(receipt.base_seq)),
        ("head", json!(head)),
        ("behind", json!(behind)),
        ("warn", json!(warn)),
        ("replayed", json!(true)),
    ];
    Ok(answer(ctx.format, Tool::Propose, fields, &[]))
}

/// The millisecond clock of the ids of new blocks: the time of the revision the call read
/// (the document's creation for revision 0), so a dry run, the real call and a retry name new
/// blocks alike. Not the current time.
async fn id_timestamp(db: &mut PgConnection, document: Uuid, rev: u64) -> Result<u64, sqlx::Error> {
    let ms: i64 = sqlx::query_scalar("select (extract(epoch from coalesce((select created_at from event where document_id=$1 and seq=$2),(select created_at from document where id=$1)))*1000)::bigint")
        .bind(document)
        .bind(rev as i64)
        .fetch_one(db)
        .await?;
    Ok(u64::try_from(ms).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_digest_binds_what_the_spec_says_and_nothing_else() {
        let file = Uuid::from_u128(1);
        let ops = json!([{"op":"set_text","ref":"7C02","text":"x"}]);
        let base = request_digest(file, 4, None, &ops, false);
        assert_eq!(base, request_digest(file, 4, None, &ops, false));
        // Key order of the ops does not matter (canonical JSON).
        let reordered = json!([{"text":"x","ref":"7C02","op":"set_text"}]);
        assert_eq!(base, request_digest(file, 4, None, &reordered, false));
        for changed in [
            request_digest(Uuid::from_u128(2), 4, None, &ops, false),
            request_digest(file, 5, None, &ops, false),
            request_digest(file, 4, Some(Uuid::from_u128(9)), &ops, false),
            request_digest(
                file,
                4,
                None,
                &json!([{"op":"set_text","ref":"7C02","text":"y"}]),
                false,
            ),
            request_digest(file, 4, None, &ops, true),
        ] {
            assert_ne!(base, changed);
        }
        assert_ne!(base, [0u8; 32]);
    }

    #[test]
    fn the_file_of_the_route_is_the_file_of_the_call() {
        let doc = Uuid::from_u128(7);
        let body =
            json!({"rev":3,"request_id":Uuid::from_u128(8),"ops":[{"op":"delete","ref":"7C02"}]});
        let args = parse_call(&body, doc).unwrap();
        assert_eq!(args.file, doc);
        let mut named = body.clone();
        named["file"] = json!(doc);
        assert!(parse_call(&named, doc).is_ok());
        named["file"] = json!(Uuid::from_u128(9));
        let error = parse_call(&named, doc).unwrap_err();
        assert_eq!(error.error, AgentError::BadRequest { field: "file" });
        // Unknown fields and a body that is not an object are the grammar's refusals.
        assert!(parse_call(&json!([1]), doc).is_err());
        let mut extra = body;
        extra["surprise"] = json!(1);
        assert!(parse_call(&extra, doc).is_err());
    }

    #[tokio::test]
    async fn a_slow_compile_is_deadline_exceeded_and_holds_what_it_captured_until_it_ends() {
        use std::sync::atomic::{AtomicBool, Ordering};
        // Stands for the admission permit: freed when the work really ends, not when the
        // caller stops waiting.
        struct Held(Arc<AtomicBool>);
        impl Drop for Held {
            fn drop(&mut self) {
                self.0.store(true, Ordering::SeqCst);
            }
        }
        let freed = Arc::new(AtomicBool::new(false));
        let held = Held(freed.clone());
        let slow = within(Duration::from_millis(50), move || {
            let _held = held;
            std::thread::sleep(Duration::from_millis(400));
            Ok::<_, AgentError>(1)
        })
        .await;
        assert_eq!(slow.unwrap_err().agent_code(), "deadline_exceeded");
        assert!(
            !freed.load(Ordering::SeqCst),
            "the permit was freed while the work still ran"
        );
        tokio::time::sleep(Duration::from_millis(700)).await;
        assert!(freed.load(Ordering::SeqCst));
        // Work inside the deadline is untouched, and its own errors keep their code.
        let quick = within(Duration::from_secs(2), || Ok::<_, AgentError>(7)).await;
        assert_eq!(quick.unwrap(), 7);
        let refused = within(Duration::from_secs(2), || {
            Err::<u8, _>(AgentError::NoChange)
        })
        .await;
        assert_eq!(refused.unwrap_err().agent_code(), "no_change");
    }

    #[test]
    fn states_are_the_words_of_the_lifecycle() {
        assert_eq!(state_word(false, false, None), "draft");
        assert_eq!(state_word(true, false, None), "open");
        assert_eq!(state_word(true, true, None), "merged");
        assert_eq!(state_word(true, false, Some("merged")), "merged");
        assert_eq!(state_word(true, false, Some("declined")), "declined");
        assert_eq!(state_word(false, false, Some("withdrawn")), "withdrawn");
    }

    #[test]
    fn answers_are_one_header_in_dct_and_an_object_in_json() {
        let fields = vec![
            ("proposal", json!("4d0e9b1c")),
            ("state", json!("open")),
            ("behind", json!(3)),
            ("replayed", json!(true)),
            ("note", Value::Null),
        ];
        let dct = answer(Format::Dct, Tool::Submit, fields.clone(), &[]);
        assert!(
            dct.starts_with(
                "dd/1 submit proposal=4d0e9b1c state=open behind=3 replayed=1 trust=data"
            ),
            "{dct}"
        );
        let json: Value =
            serde_json::from_str(&answer(Format::Json, Tool::Submit, fields, &[])).unwrap();
        assert_eq!(json["tool"], "submit");
        assert_eq!(json["trust"], "data");
        assert_eq!(json["behind"], 3);
        assert!(json["note"].is_null());
        assert_eq!(
            short(Uuid::from_u128(0xabcdef00_0000_0000_0000_000000000001)),
            "abcdef00"
        );
    }
}
