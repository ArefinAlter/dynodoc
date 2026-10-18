//! The agent data plane, read half (docs/AI-AGENT-PROTOCOL-SPEC.md section 12.1,
//! routes 1 to 9, 14 and 15). Every handler is a thin shell around `engine-agent`: it
//! authorises, reads the state at the wanted revision, calls the pure function and
//! writes the body in the negotiated format. No handler appends to any event table,
//! changes membership, or writes anything but one `agent_call` metadata row.
//!
//! Order of every request, none of it cached beyond the token's own lifetime:
//!
//! 1. [`AgentAuth`]: a device token with `content:read` for this project, whose key is
//!    unrevoked, enrolled as an agent key and holds an active grant.
//! 2. The caller's current project role (`403` for a non-member, the same answer for an
//!    unknown project) and the process-local admission budget.
//! 3. For a file route, the file in this project with a role for the caller and a
//!    supported kind (`404` otherwise, one answer for every reason).
//! 4. The head, the revision asked for and its stored chain hash; `If-None-Match` is
//!    answered here, before any state is read.
//! 5. The verified state (cache or checkpoint plus tail), then the tool, off the async
//!    workers.

use std::{collections::BTreeMap, sync::Arc};

use axum::{
    extract::{
        rejection::{JsonRejection, QueryRejection},
        DefaultBodyLimit, Path, Query, State,
    },
    http::{HeaderMap, Uri},
    response::Response,
    routing::{get, post},
    Json, Router,
};
use engine_agent::{
    budget::Cursor,
    dct::{HeaderBuilder, Tool},
    diff::{Detail, DiffOptions},
    error::AgentError,
    files::{FileEntry, FilesOptions},
    find::{FindOptions, Scope},
    model::{FileKind, FileMeta},
    outline::OutlineOptions,
    read::ReadOptions,
    response::Response as ToolResponse,
    text::Fmt,
};
use engine_core::materializer::DocumentState;
use engine_shared::DocumentId;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{pool::PoolConnection, PgConnection, Postgres};
use uuid::Uuid;

use super::{
    error::AgentFail,
    http::{self, Format},
    ledger::{self, CallRow},
    schema,
    state::{self, Point},
};
use crate::{
    auth::agent::{AgentAuth, ContentRead},
    error::ApiError,
    projects,
    workloads::ReadPermit,
    AppState,
};

type Out = Result<Response, AgentFail>;

/// Request bodies of the one POST route are tiny (at most 100 refs and their hashes).
const BODY_LIMIT: usize = 256 * 1024;
/// Most files a single project listing will consider; a larger project is refused
/// rather than silently truncated.
const MAX_LISTED_FILES: i64 = 10_000;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects/{id}/agent/files", get(files))
        .route("/projects/{id}/agent/files/{file}/outline", get(outline))
        .route("/projects/{id}/agent/files/{file}/blocks", get(blocks))
        .route(
            "/projects/{id}/agent/files/{file}/blocks:read",
            post(blocks_read).layer(DefaultBodyLimit::max(BODY_LIMIT)),
        )
        .route("/projects/{id}/agent/files/{file}/cells", get(cells))
        .route(
            "/projects/{id}/agent/files/{file}/sheets/{sheet}/profile",
            get(profile),
        )
        .route("/projects/{id}/agent/files/{file}/slides", get(slides))
        .route("/projects/{id}/agent/files/{file}/find", get(find))
        .route("/projects/{id}/agent/files/{file}/diff", get(diff))
        .route("/projects/{id}/agent/access", get(access))
        .route("/.well-known/dynodoc-agent", get(discovery))
}

// --- request plumbing ---------------------------------------------------------------

/// What the request has done so far, for the ledger.
#[derive(Default)]
struct Call {
    /// Past the membership check and the admission budget: worth a ledger row.
    admitted: bool,
    document: Option<Uuid>,
    rev: Option<u64>,
}

/// A finished response and the size of its body.
struct Served {
    response: Response,
    bytes: usize,
}

/// A connection with the caller's project role checked and an admission permit held.
struct Admitted {
    db: PoolConnection<Postgres>,
    /// The stored project role (`owner`, `manager`, `editor`, `approver`, `reviewer`, `viewer`).
    role: String,
    _permit: ReadPermit,
}

struct Ctx<'a> {
    state: &'a AppState,
    auth: &'a AgentAuth<ContentRead>,
    project: Uuid,
    headers: &'a HeaderMap,
    tool: Tool,
    digest: [u8; 32],
    format: Format,
}

impl<'a> Ctx<'a> {
    fn new(
        state: &'a AppState,
        auth: &'a AgentAuth<ContentRead>,
        project: Uuid,
        headers: &'a HeaderMap,
        tool: Tool,
        digest: [u8; 32],
    ) -> Result<Self, AgentFail> {
        Ok(Self {
            state,
            auth,
            project,
            headers,
            tool,
            digest,
            format: http::negotiate(http::accept(headers))?,
        })
    }

    async fn admit(&self, call: &mut Call) -> Result<Admitted, AgentFail> {
        let identity = self.auth.identity_id;
        let mut db = self.state.pool.acquire().await?;
        let role = projects::require(&mut db, self.project, identity).await?;
        // Called only after the membership check, as `Workloads::enter` requires.
        let permit = self.state.workloads.enter(identity, Some(self.project))?;
        call.admitted = true;
        Ok(Admitted {
            db,
            role,
            _permit: permit,
        })
    }

    /// The file named by a path segment: a full UUID, or the first 8 hex characters when
    /// they are unique among the caller's files in this project (spec 6.2). One answer
    /// for every way a file can be unavailable: absent, trashed, another project, a kind
    /// agents do not read, or no role for the caller.
    async fn find_file(
        &self,
        db: &mut PgConnection,
        call: &mut Call,
        segment: &str,
    ) -> Result<(Uuid, FileKind), AgentFail> {
        const VISIBLE: &str = "d.deleted_at is null and coalesce(d.settings->>'kind','questionnaire') in ('document','spreadsheet','presentation') and document_member_role(d.id,$2) is not null";
        let identity = self.auth.identity_id;
        let rows: Vec<(Uuid, String)> = if let Ok(id) = Uuid::parse_str(segment) {
            sqlx::query_as(&format!("select d.id,d.settings->>'kind' from document d where d.id=$1 and document_project(d.id)=$3 and {VISIBLE}"))
                .bind(id)
                .bind(identity)
                .bind(self.project)
                .fetch_all(&mut *db)
                .await?
        } else if segment.len() == 8 && segment.bytes().all(|b| b.is_ascii_hexdigit()) {
            sqlx::query_as(&format!("with recursive tree as (select id from access_space where id=$1 union select s.id from access_space s join tree t on s.parent_id=t.id) select d.id,d.settings->>'kind' from document d join tree t on t.id=d.space_id where replace(d.id::text,'-','') like $3::text||'%' and {VISIBLE} limit 2"))
                .bind(self.project)
                .bind(identity)
                .bind(segment.to_ascii_lowercase())
                .fetch_all(&mut *db)
                .await?
        } else {
            Vec::new()
        };
        let [(id, kind)] = rows.as_slice() else {
            // None, or an ambiguous prefix: indistinguishable from absence.
            return Err(AgentError::NotFound { refs: Vec::new() }.into());
        };
        let kind =
            FileKind::parse(kind).ok_or(AgentError::NotFound { refs: Vec::new() })?;
        call.document = Some(*id);
        Ok((*id, kind))
    }

    /// The common path of the single-file tools. `conditional` answers `If-None-Match`
    /// with `304` before any state is read.
    #[allow(clippy::too_many_arguments)]
    async fn file_tool<F>(
        &self,
        call: &mut Call,
        file: &str,
        rev: Option<u64>,
        cursor: Option<&str>,
        conditional: bool,
        run: F,
    ) -> Result<Served, AgentFail>
    where
        F: FnOnce(FileMeta, Arc<DocumentState>, Format) -> Result<String, AgentError>
            + Send
            + 'static,
    {
        let mut admitted = self.admit(call).await?;
        let (document, kind) = self.find_file(&mut admitted.db, call, file).await?;
        let point = state::point(&mut admitted.db, document, rev, cursor).await?;
        call.rev = Some(point.rev);
        let token = point.token();
        if conditional && http::if_none_match(self.headers, &token) {
            return Ok(Served {
                response: http::not_modified(&token),
                bytes: 0,
            });
        }
        let loaded = state::load(&self.state.agent_cache, &mut admitted.db, &point).await?;
        drop(admitted.db);
        let meta = file_meta(&point, kind);
        let format = self.format;
        let body = compute(move || run(meta, loaded, format)).await?;
        let bytes = body.len();
        Ok(Served {
            response: http::reply(format, body, Some(&token)),
            bytes,
        })
    }

    /// Record the call and turn the result into the response.
    async fn finish(&self, call: Call, result: Result<Served, AgentFail>) -> Out {
        if call.admitted {
            let (outcome, bytes_out) = match &result {
                Ok(served) => ("ok", served.bytes),
                Err(error) => (error.agent_code(), 0),
            };
            ledger::record(
                &self.state.pool,
                CallRow {
                    principal: self.auth.identity_id,
                    key: self.auth.binding().key_id,
                    project: self.project,
                    document: call.document,
                    tool: self.tool.as_str(),
                    rev: call.rev,
                    digest: self.digest,
                    bytes_out,
                    outcome,
                    client: http::client_claim(self.headers),
                    traceparent: http::traceparent(self.headers),
                },
            )
            .await;
        }
        result.map(|served| served.response)
    }
}

fn file_meta(point: &Point, kind: FileKind) -> FileMeta {
    FileMeta {
        id: DocumentId(point.document),
        kind,
        rev: point.rev,
        chain: point.chain_hex(),
    }
}

/// Run CPU-bound tool code off the async workers.
async fn compute<T, F>(work: F) -> Result<T, AgentFail>
where
    F: FnOnce() -> Result<T, AgentError> + Send + 'static,
    T: Send + 'static,
{
    match tokio::task::spawn_blocking(work).await {
        Ok(result) => Ok(result?),
        Err(error) => {
            tracing::error!(%error, "agent tool task failed");
            Err(AgentError::Internal.into())
        }
    }
}

fn render<T: Serialize>(response: ToolResponse<T>, tool: Tool, format: Format) -> String {
    match format {
        Format::Dct => response.text,
        Format::Json => response.json_mirror(tool),
    }
}

fn query<T>(parsed: Result<Query<T>, QueryRejection>) -> Result<T, AgentFail> {
    parsed
        .map(|Query(value)| value)
        .map_err(|_| AgentError::BadRequest { field: "query" }.into())
}

/// `a,b,c` as a list; empty items are dropped.
fn list(value: Option<&str>) -> Vec<String> {
    value
        .map(|v| {
            v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn fmt_of(value: Option<&str>) -> Result<Fmt, AgentError> {
    match value {
        None => Ok(Fmt::Plain),
        Some(raw) => Fmt::parse(raw).ok_or(AgentError::BadRequest { field: "fmt" }),
    }
}

fn get_digest(uri: &Uri) -> [u8; 32] {
    ledger::request_digest(&[
        b"GET",
        uri.path().as_bytes(),
        uri.query().unwrap_or("").as_bytes(),
    ])
}

// --- 1. files -----------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FilesQuery {
    q: Option<String>,
    kind: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
    max_bytes: Option<usize>,
}

/// The product role vocabulary of `document_member_role`, from a stored project role.
fn product_role(stored: &str) -> &str {
    match stored {
        "approver" => "reviewer",
        "reviewer" => "contributor",
        other => other,
    }
}

#[utoipa::path(operation_id="agent_files", get, path="/projects/{id}/agent/files", tag="agent", params(("id"=Uuid,Path,description="Project"),("q"=Option<String>,Query,description="Title substring, at most 200 characters"),("kind"=Option<String>,Query,description="document, spreadsheet or presentation"),("limit"=Option<usize>,Query,description="Files per page (1 to 50, default 20)"),("cursor"=Option<String>,Query,description="Continuation from a previous page"),("max_bytes"=Option<usize>,Query,description="Response budget in bytes (500 to 64000, default 16000)")), security(("device_session"=["content:read"])), responses((status=200, description="Files of the project the caller may open, ordered by title. Titles are untrusted text", content((schema::AgentFiles="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=401, description="Not a project device session"),(status=403, description="Scope, grant, key class or project membership missing (uniform)"),(status=429, description="Admission budget")))]
async fn files(
    State(state): State<AppState>,
    auth: AgentAuth<ContentRead>,
    Path(project): Path<Uuid>,
    headers: HeaderMap,
    uri: Uri,
    parsed: Result<Query<FilesQuery>, QueryRejection>,
) -> Out {
    let ctx = Ctx::new(&state, &auth, project, &headers, Tool::Files, get_digest(&uri))?;
    let mut call = Call::default();
    let result: Result<Served, AgentFail> = async {
        let q = query(parsed)?;
        let kind = q
            .kind
            .as_deref()
            .map(|raw| FileKind::parse(raw).ok_or(AgentError::BadRequest { field: "kind" }))
            .transpose()?;
        let opts = FilesOptions {
            project: None,
            q: q.q,
            kind,
            limit: q.limit,
            cursor: q.cursor,
            max_bytes: q.max_bytes,
        };
        let mut admitted = ctx.admit(&mut call).await?;
        let rows: Vec<(Uuid, String, String, Option<String>, i64)> = sqlx::query_as("with recursive tree as (select id from access_space where id=$1 union select s.id from access_space s join tree t on s.parent_id=t.id) select d.id,d.title,d.settings->>'kind',document_member_role(d.id,$2),(select coalesce(max(seq),0) from event where document_id=d.id) from document d join tree t on t.id=d.space_id where d.deleted_at is null and coalesce(d.settings->>'kind','questionnaire') in ('document','spreadsheet','presentation') and document_member_role(d.id,$2) is not null order by d.title,d.id limit $3")
            .bind(project)
            .bind(auth.identity_id)
            .bind(MAX_LISTED_FILES + 1)
            .fetch_all(&mut *admitted.db)
            .await?;
        if rows.len() as i64 > MAX_LISTED_FILES {
            return Err(AgentError::LimitExceeded {
                field: "files",
                limit: MAX_LISTED_FILES as u64,
                value: rows.len() as u64,
            }
            .into());
        }
        let name: String = sqlx::query_scalar("select name from access_space where id=$1")
            .bind(project)
            .fetch_one(&mut *admitted.db)
            .await?;
        drop(admitted.db);
        let entries: Vec<FileEntry> = rows
            .into_iter()
            .filter_map(|(id, title, kind, role, rev)| {
                Some(FileEntry {
                    id: DocumentId(id),
                    kind: FileKind::parse(&kind)?,
                    rev: u64::try_from(rev).ok()?,
                    project_id: project,
                    project: name.clone(),
                    role: role?,
                    title,
                })
            })
            .collect();
        let format = ctx.format;
        let body = compute(move || {
            engine_agent::files::files(&entries, &opts).map(|r| render(r, Tool::Files, format))
        })
        .await?;
        let bytes = body.len();
        Ok(Served {
            response: http::reply(format, body, None),
            bytes,
        })
    }
    .await;
    ctx.finish(call, result).await
}

// --- 2. outline ---------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct OutlineQuery {
    depth: Option<u8>,
    preview: Option<usize>,
    under: Option<String>,
    if_none_match: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
    max_bytes: Option<usize>,
}

impl OutlineQuery {
    fn options(self) -> OutlineOptions {
        OutlineOptions {
            depth: self.depth,
            preview: self.preview,
            under: self.under,
            if_none_match: self.if_none_match,
            limit: self.limit,
            cursor: self.cursor,
            max_bytes: self.max_bytes,
        }
    }
}

#[utoipa::path(operation_id="agent_outline", get, path="/projects/{id}/agent/files/{file}/outline", tag="agent", params(("id"=Uuid,Path,description="Project"),("file"=String,Path,description="File UUID, or its first 8 hex characters when unique among the caller's files"),("depth"=Option<u8>,Query,description="0: headings, sheets or slide titles; 1 (default): every block with a preview"),("preview"=Option<usize>,Query,description="Preview characters per block (0 to 120, default 48)"),("under"=Option<String>,Query,description="Heading ref (documents) or sheet name or ref (spreadsheets)"),("if_none_match"=Option<String>,Query,description="An oh value from an earlier outline; an unchanged file answers with one line"),("limit"=Option<usize>,Query,description="Lines per page (1 to 1000, default 200)"),("cursor"=Option<String>,Query,description="Continuation; pins its revision"),("max_bytes"=Option<usize>,Query,description="Response budget in bytes (500 to 64000, default 16000)")), security(("device_session"=["content:read"])), responses((status=200, description="Block refs, types, hashes and previews (documents); sheets or one sheet's column profile (spreadsheets); slides (presentations). Sets ETag \"<rev>:<chain8>\"", content((schema::AgentOutline="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=304, description="If-None-Match names the revision served"),(status=400, description="Invalid parameter (details.agent_code, details.field)"),(status=401, description="Not a project device session"),(status=403, description="Scope, grant, key class or project membership missing (uniform)"),(status=404, description="File not visible (one answer for every reason)"),(status=429, description="Admission budget")))]
async fn outline(
    State(state): State<AppState>,
    auth: AgentAuth<ContentRead>,
    Path((project, file)): Path<(Uuid, String)>,
    headers: HeaderMap,
    uri: Uri,
    parsed: Result<Query<OutlineQuery>, QueryRejection>,
) -> Out {
    let ctx = Ctx::new(&state, &auth, project, &headers, Tool::Outline, get_digest(&uri))?;
    let mut call = Call::default();
    let result: Result<Served, AgentFail> = async {
        let q = query(parsed)?;
        let cursor = q.cursor.clone();
        let opts = q.options();
        ctx.file_tool(&mut call, &file, None, cursor.as_deref(), true, move |meta, st, fmt| {
            engine_agent::outline::outline(&meta, &st, &opts).map(|r| render(r, Tool::Outline, fmt))
        })
        .await
    }
    .await;
    ctx.finish(call, result).await
}

// --- 6. sheet profile ---------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProfileQuery {
    if_none_match: Option<String>,
    limit: Option<usize>,
    cursor: Option<String>,
    max_bytes: Option<usize>,
}

#[utoipa::path(operation_id="agent_sheet_profile", get, path="/projects/{id}/agent/files/{file}/sheets/{sheet}/profile", tag="agent", params(("id"=Uuid,Path,description="Project"),("file"=String,Path,description="Spreadsheet file UUID or unique 8-character prefix"),("sheet"=String,Path,description="Sheet name or ref"),("if_none_match"=Option<String>,Query,description="A sheet hash (h) from an earlier profile"),("limit"=Option<usize>,Query,description="Column lines per page"),("cursor"=Option<String>,Query,description="Continuation; pins its revision"),("max_bytes"=Option<usize>,Query,description="Response budget in bytes")), security(("device_session"=["content:read"])), responses((status=200, description="Column statistics over the stored strings of one sheet (formulas are not evaluated: calc=none). Sets ETag", content((schema::AgentOutline="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=304, description="If-None-Match names the revision served"),(status=400, description="Invalid parameter, or the file is not a spreadsheet"),(status=403, description="Scope, grant, key class or project membership missing (uniform)"),(status=404, description="File not visible")))]
async fn profile(
    State(state): State<AppState>,
    auth: AgentAuth<ContentRead>,
    Path((project, file, sheet)): Path<(Uuid, String, String)>,
    headers: HeaderMap,
    uri: Uri,
    parsed: Result<Query<ProfileQuery>, QueryRejection>,
) -> Out {
    let ctx = Ctx::new(&state, &auth, project, &headers, Tool::Outline, get_digest(&uri))?;
    let mut call = Call::default();
    let result: Result<Served, AgentFail> = async {
        let q = query(parsed)?;
        let cursor = q.cursor.clone();
        let opts = OutlineOptions {
            depth: Some(1),
            preview: None,
            under: Some(sheet),
            if_none_match: q.if_none_match,
            limit: q.limit,
            cursor: q.cursor,
            max_bytes: q.max_bytes,
        };
        ctx.file_tool(&mut call, &file, None, cursor.as_deref(), true, move |meta, st, fmt| {
            if meta.kind != FileKind::Spreadsheet {
                return Err(AgentError::BadRequest { field: "sheet" });
            }
            engine_agent::outline::outline(&meta, &st, &opts).map(|r| render(r, Tool::Outline, fmt))
        })
        .await
    }
    .await;
    ctx.finish(call, result).await
}

// --- 3. blocks, 4. blocks:read --------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BlocksQuery {
    /// Comma-separated refs, sub-addresses or full node ids (at most 100).
    refs: Option<String>,
    section: Option<String>,
    fmt: Option<String>,
    rev: Option<u64>,
    max_bytes: Option<usize>,
    cursor: Option<String>,
}

#[utoipa::path(operation_id="agent_blocks", get, path="/projects/{id}/agent/files/{file}/blocks", tag="agent", params(("id"=Uuid,Path,description="Project"),("file"=String,Path,description="File UUID or unique 8-character prefix"),("refs"=Option<String>,Query,description="Comma-separated refs (at most 100)"),("section"=Option<String>,Query,description="Heading ref: the heading and its blocks"),("fmt"=Option<String>,Query,description="plain (default) or md"),("rev"=Option<u64>,Query,description="Read an earlier revision"),("max_bytes"=Option<usize>,Query,description="Response budget in bytes (500 to 64000, default 16000)"),("cursor"=Option<String>,Query,description="Continuation; pins its revision")), security(("device_session"=["content:read"])), responses((status=200, description="The text of the selected blocks, escaped and marked trust=data. Exactly one of refs or section. Sets ETag", content((schema::AgentRead="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=304, description="If-None-Match names the revision served"),(status=400, description="Invalid parameter or selector"),(status=403, description="Scope, grant, key class or project membership missing (uniform)"),(status=404, description="File not visible")))]
async fn blocks(
    State(state): State<AppState>,
    auth: AgentAuth<ContentRead>,
    Path((project, file)): Path<(Uuid, String)>,
    headers: HeaderMap,
    uri: Uri,
    parsed: Result<Query<BlocksQuery>, QueryRejection>,
) -> Out {
    let ctx = Ctx::new(&state, &auth, project, &headers, Tool::Read, get_digest(&uri))?;
    let mut call = Call::default();
    let result: Result<Served, AgentFail> = async {
        let q = query(parsed)?;
        let opts = ReadOptions {
            refs: list(q.refs.as_deref()),
            section: q.section,
            fmt: fmt_of(q.fmt.as_deref())?,
            max_bytes: q.max_bytes,
            cursor: q.cursor.clone(),
            ..ReadOptions::default()
        };
        read_tool(&ctx, &mut call, &file, q.rev, q.cursor.as_deref(), true, opts).await
    }
    .await;
    ctx.finish(call, result).await
}

async fn read_tool(
    ctx: &Ctx<'_>,
    call: &mut Call,
    file: &str,
    rev: Option<u64>,
    cursor: Option<&str>,
    conditional: bool,
    opts: ReadOptions,
) -> Result<Served, AgentFail> {
    ctx.file_tool(call, file, rev, cursor, conditional, move |meta, st, fmt| {
        engine_agent::read::read(&meta, &st, &opts).map(|r| render(r, Tool::Read, fmt))
    })
    .await
}

/// Body of `POST .../blocks:read`: the selection and the hashes the caller already holds.
#[derive(Serialize, Deserialize, utoipa::ToSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct BlocksReadBody {
    /// Block refs, sub-addresses or full node ids (at most 100); answered in this order.
    #[serde(default)]
    refs: Vec<String>,
    /// Heading ref, instead of `refs`.
    #[serde(default)]
    section: Option<String>,
    /// `ref -> hash` of content the caller holds; unchanged blocks come back as one line.
    #[serde(default)]
    have: BTreeMap<String, String>,
    /// `plain` (default) or `md`.
    #[serde(default)]
    fmt: Option<String>,
    #[serde(default)]
    rev: Option<u64>,
    #[serde(default)]
    max_bytes: Option<usize>,
    #[serde(default)]
    cursor: Option<String>,
}

#[utoipa::path(operation_id="agent_blocks_read", post, path="/projects/{id}/agent/files/{file}/blocks:read", tag="agent", params(("id"=Uuid,Path,description="Project"),("file"=String,Path,description="File UUID or unique 8-character prefix")), request_body=BlocksReadBody, security(("device_session"=["content:read"])), responses((status=200, description="Like GET blocks, with the hash cache: a block whose hash equals the caller's have entry is returned as one @ref = #hash line. A read; nothing is stored but a call-metadata row. Sets ETag; never 304", content((schema::AgentRead="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=400, description="Invalid body or selector"),(status=403, description="Scope, grant, key class or project membership missing (uniform)"),(status=404, description="File not visible"),(status=413, description="Body larger than 256 KiB")))]
async fn blocks_read(
    State(state): State<AppState>,
    auth: AgentAuth<ContentRead>,
    Path((project, file)): Path<(Uuid, String)>,
    headers: HeaderMap,
    uri: Uri,
    parsed: Result<Json<BlocksReadBody>, JsonRejection>,
) -> Out {
    // Parse first: the digest covers the typed body, in the struct's fixed field order.
    let body = parsed.map(|Json(b)| b);
    let canonical = body
        .as_ref()
        .ok()
        .and_then(|b| serde_json::to_vec(b).ok())
        .unwrap_or_default();
    let digest = ledger::request_digest(&[b"POST", uri.path().as_bytes(), &canonical]);
    let ctx = Ctx::new(&state, &auth, project, &headers, Tool::Read, digest)?;
    let mut call = Call::default();
    let result: Result<Served, AgentFail> = async {
        let b = body.map_err(|rejection| {
            if rejection.status() == axum::http::StatusCode::PAYLOAD_TOO_LARGE {
                let sent = headers
                    .get(axum::http::header::CONTENT_LENGTH)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok());
                AgentError::LimitExceeded {
                    field: "body",
                    limit: BODY_LIMIT as u64,
                    value: sent.unwrap_or(BODY_LIMIT as u64 + 1),
                }
            } else {
                AgentError::BadRequest { field: "body" }
            }
        })?;
        let opts = ReadOptions {
            refs: b.refs,
            section: b.section,
            have: b.have,
            fmt: fmt_of(b.fmt.as_deref())?,
            max_bytes: b.max_bytes,
            cursor: b.cursor.clone(),
            ..ReadOptions::default()
        };
        read_tool(&ctx, &mut call, &file, b.rev, b.cursor.as_deref(), false, opts).await
    }
    .await;
    ctx.finish(call, result).await
}

// --- 5. cells, 7. slides ------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CellsQuery {
    /// A1 range with a sheet name or ref, such as `Budget!A1:F40`.
    range: String,
    /// `refs` adds a stable row ref column.
    rows: Option<String>,
    fmt: Option<String>,
    rev: Option<u64>,
    max_bytes: Option<usize>,
    cursor: Option<String>,
}

#[utoipa::path(operation_id="agent_cells", get, path="/projects/{id}/agent/files/{file}/cells", tag="agent", params(("id"=Uuid,Path,description="Project"),("file"=String,Path,description="Spreadsheet file UUID or unique 8-character prefix"),("range"=String,Query,description="A1 range with a sheet name or ref, e.g. Budget!A1:F40 (at most 2000 cells)"),("rows"=Option<String>,Query,description="refs: add a row-ref column"),("fmt"=Option<String>,Query,description="plain (default) or md"),("rev"=Option<u64>,Query,description="Read an earlier revision"),("max_bytes"=Option<usize>,Query,description="Response budget in bytes"),("cursor"=Option<String>,Query,description="Continuation; pins its revision")), security(("device_session"=["content:read"])), responses((status=200, description="The stored cell strings of the range (a leading = is a formula; nothing is evaluated: calc=none). Sets ETag", content((schema::AgentRead="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=304, description="If-None-Match names the revision served"),(status=400, description="Invalid range or parameter"),(status=403, description="Scope, grant, key class or project membership missing (uniform)"),(status=404, description="File not visible")))]
async fn cells(
    State(state): State<AppState>,
    auth: AgentAuth<ContentRead>,
    Path((project, file)): Path<(Uuid, String)>,
    headers: HeaderMap,
    uri: Uri,
    parsed: Result<Query<CellsQuery>, QueryRejection>,
) -> Out {
    let ctx = Ctx::new(&state, &auth, project, &headers, Tool::Read, get_digest(&uri))?;
    let mut call = Call::default();
    let result: Result<Served, AgentFail> = async {
        let q = query(parsed)?;
        let row_refs = match q.rows.as_deref() {
            None => false,
            Some("refs") => true,
            Some(_) => return Err(AgentError::BadRequest { field: "rows" }.into()),
        };
        let opts = ReadOptions {
            range: Some(q.range),
            row_refs,
            fmt: fmt_of(q.fmt.as_deref())?,
            max_bytes: q.max_bytes,
            cursor: q.cursor.clone(),
            ..ReadOptions::default()
        };
        read_tool(&ctx, &mut call, &file, q.rev, q.cursor.as_deref(), true, opts).await
    }
    .await;
    ctx.finish(call, result).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SlidesQuery {
    /// Comma-separated slide positions (`s2`) or refs (at most 50).
    ids: String,
    fmt: Option<String>,
    rev: Option<u64>,
    max_bytes: Option<usize>,
    cursor: Option<String>,
}

#[utoipa::path(operation_id="agent_slides", get, path="/projects/{id}/agent/files/{file}/slides", tag="agent", params(("id"=Uuid,Path,description="Project"),("file"=String,Path,description="Presentation file UUID or unique 8-character prefix"),("ids"=String,Query,description="Comma-separated slide positions (s2) or refs (at most 50)"),("fmt"=Option<String>,Query,description="plain (default) or md"),("rev"=Option<u64>,Query,description="Read an earlier revision"),("max_bytes"=Option<usize>,Query,description="Response budget in bytes"),("cursor"=Option<String>,Query,description="Continuation; pins its revision")), security(("device_session"=["content:read"])), responses((status=200, description="Text of the selected slides; images are never returned (kind, alt text and size only). Sets ETag", content((schema::AgentRead="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=304, description="If-None-Match names the revision served"),(status=400, description="Invalid slide selector or parameter"),(status=403, description="Scope, grant, key class or project membership missing (uniform)"),(status=404, description="File not visible")))]
async fn slides(
    State(state): State<AppState>,
    auth: AgentAuth<ContentRead>,
    Path((project, file)): Path<(Uuid, String)>,
    headers: HeaderMap,
    uri: Uri,
    parsed: Result<Query<SlidesQuery>, QueryRejection>,
) -> Out {
    let ctx = Ctx::new(&state, &auth, project, &headers, Tool::Read, get_digest(&uri))?;
    let mut call = Call::default();
    let result: Result<Served, AgentFail> = async {
        let q = query(parsed)?;
        let opts = ReadOptions {
            slides: list(Some(q.ids.as_str())),
            fmt: fmt_of(q.fmt.as_deref())?,
            max_bytes: q.max_bytes,
            cursor: q.cursor.clone(),
            ..ReadOptions::default()
        };
        read_tool(&ctx, &mut call, &file, q.rev, q.cursor.as_deref(), true, opts).await
    }
    .await;
    ctx.finish(call, result).await
}

// --- 8. find ------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FindQuery {
    q: String,
    #[serde(default)]
    regex: bool,
    /// Case-sensitive.
    #[serde(default)]
    case: bool,
    #[serde(default)]
    word: bool,
    /// `text` (default), `formulas` or `notes`.
    #[serde(rename = "in")]
    scope: Option<String>,
    context: Option<usize>,
    limit: Option<usize>,
    cursor: Option<String>,
    max_bytes: Option<usize>,
}

#[utoipa::path(operation_id="agent_find", get, path="/projects/{id}/agent/files/{file}/find", tag="agent", params(("id"=Uuid,Path,description="Project"),("file"=String,Path,description="File UUID or unique 8-character prefix"),("q"=String,Query,description="Literal text or, with regex, a pattern (1 to 200 characters)"),("regex"=Option<bool>,Query,description="Linear-time regular expression"),("case"=Option<bool>,Query,description="Case-sensitive"),("word"=Option<bool>,Query,description="Whole words"),("in"=Option<String>,Query,description="text (default), formulas or notes"),("context"=Option<usize>,Query,description="Snippet context characters (0 to 120, default 30)"),("limit"=Option<usize>,Query,description="Hits per page (1 to 100, default 20)"),("cursor"=Option<String>,Query,description="Continuation; pins its revision"),("max_bytes"=Option<usize>,Query,description="Response budget in bytes")), security(("device_session"=["content:read"])), responses((status=200, description="Refs or cells with short snippets, in document order. Sets ETag", content((schema::AgentFind="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=304, description="If-None-Match names the revision served"),(status=400, description="Invalid pattern or parameter"),(status=403, description="Scope, grant, key class or project membership missing (uniform)"),(status=404, description="File not visible")))]
async fn find(
    State(state): State<AppState>,
    auth: AgentAuth<ContentRead>,
    Path((project, file)): Path<(Uuid, String)>,
    headers: HeaderMap,
    uri: Uri,
    parsed: Result<Query<FindQuery>, QueryRejection>,
) -> Out {
    let ctx = Ctx::new(&state, &auth, project, &headers, Tool::Find, get_digest(&uri))?;
    let mut call = Call::default();
    let result: Result<Served, AgentFail> = async {
        let q = query(parsed)?;
        let scope = match q.scope.as_deref() {
            None => Scope::Text,
            Some(raw) => Scope::parse(raw).ok_or(AgentError::BadRequest { field: "in" })?,
        };
        let cursor = q.cursor.clone();
        let opts = FindOptions {
            q: q.q,
            regex: q.regex,
            case: q.case,
            word: q.word,
            scope,
            context: q.context,
            limit: q.limit,
            cursor: q.cursor,
            max_bytes: q.max_bytes,
        };
        ctx.file_tool(&mut call, &file, None, cursor.as_deref(), true, move |meta, st, fmt| {
            engine_agent::find::find(&meta, &st, &opts).map(|r| render(r, Tool::Find, fmt))
        })
        .await
    }
    .await;
    ctx.finish(call, result).await
}

// --- 9. diff ------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DiffQuery {
    /// The earlier revision. Required unless a cursor carries it.
    from: Option<u64>,
    /// The later revision (default: the head).
    to: Option<u64>,
    /// Agent proposals do not exist yet; any value is a `404`.
    proposal: Option<String>,
    /// `summary` (default) or `text`.
    detail: Option<String>,
    max_bytes: Option<usize>,
    cursor: Option<String>,
}

#[utoipa::path(operation_id="agent_diff", get, path="/projects/{id}/agent/files/{file}/diff", tag="agent", params(("id"=Uuid,Path,description="Project"),("file"=String,Path,description="File UUID or unique 8-character prefix"),("from"=Option<u64>,Query,description="Earlier revision (required unless a cursor carries it)"),("to"=Option<u64>,Query,description="Later revision (default: head)"),("proposal"=Option<String>,Query,description="Reserved for agent proposals; always 404 in this version"),("detail"=Option<String>,Query,description="summary (default) or text"),("max_bytes"=Option<usize>,Query,description="Response budget in bytes (default 8000)"),("cursor"=Option<String>,Query,description="Continuation; pins both revisions")), security(("device_session"=["content:read"])), responses((status=200, description="Per-block changes between two revisions of the file. Sets ETag of the later revision", content((schema::AgentDiff="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=304, description="If-None-Match names the later revision"),(status=400, description="Invalid revisions or parameter"),(status=403, description="Scope, grant, key class or project membership missing (uniform)"),(status=404, description="File or proposal not visible")))]
async fn diff(
    State(state): State<AppState>,
    auth: AgentAuth<ContentRead>,
    Path((project, file)): Path<(Uuid, String)>,
    headers: HeaderMap,
    uri: Uri,
    parsed: Result<Query<DiffQuery>, QueryRejection>,
) -> Out {
    let ctx = Ctx::new(&state, &auth, project, &headers, Tool::Diff, get_digest(&uri))?;
    let mut call = Call::default();
    let result: Result<Served, AgentFail> = async {
        let q = query(parsed)?;
        let detail = match q.detail.as_deref() {
            None => Detail::Summary,
            Some(raw) => Detail::parse(raw).ok_or(AgentError::BadRequest { field: "detail" })?,
        };
        let pinned = q
            .cursor
            .as_deref()
            .map(|raw| {
                raw.parse::<Cursor>()
                    .map_err(|_| AgentError::BadRequest { field: "cursor" })
            })
            .transpose()?;
        let from = match (q.from, pinned.and_then(|c| c.aux)) {
            (Some(a), Some(b)) if a != b => {
                return Err(AgentError::BadRequest { field: "cursor" }.into())
            }
            (Some(a), _) | (None, Some(a)) => a,
            (None, None) => return Err(AgentError::BadRequest { field: "from" }.into()),
        };
        let opts = DiffOptions {
            detail,
            max_bytes: q.max_bytes,
            cursor: q.cursor.clone(),
        };
        let wants_proposal = q.proposal.is_some();
        diff_tool(&ctx, &mut call, &file, from, q.to, q.cursor.as_deref(), wants_proposal, opts)
            .await
    }
    .await;
    ctx.finish(call, result).await
}

#[allow(clippy::too_many_arguments)]
async fn diff_tool(
    ctx: &Ctx<'_>,
    call: &mut Call,
    file: &str,
    from: u64,
    to: Option<u64>,
    cursor: Option<&str>,
    wants_proposal: bool,
    opts: DiffOptions,
) -> Result<Served, AgentFail> {
    let mut admitted = ctx.admit(call).await?;
    let (document, kind) = ctx.find_file(&mut admitted.db, call, file).await?;
    if wants_proposal {
        // The file exists for this caller; there is simply no proposal of any agent.
        return Err(AgentError::NotFound { refs: Vec::new() }.into());
    }
    let later = state::point(&mut admitted.db, document, to, cursor).await?;
    call.rev = Some(later.rev);
    if from > later.rev {
        return Err(AgentError::BadRequest { field: "from" }.into());
    }
    let token = later.token();
    if http::if_none_match(ctx.headers, &token) {
        return Ok(Served {
            response: http::not_modified(&token),
            bytes: 0,
        });
    }
    let earlier = state::point(&mut admitted.db, document, Some(from), None).await?;
    let from_state = state::load(&ctx.state.agent_cache, &mut admitted.db, &earlier).await?;
    let to_state = state::load(&ctx.state.agent_cache, &mut admitted.db, &later).await?;
    drop(admitted.db);
    let meta = file_meta(&later, kind);
    let format = ctx.format;
    let body = compute(move || {
        engine_agent::diff::diff(&meta, from, &from_state, &to_state, &opts)
            .map(|r| render(r, Tool::Diff, format))
    })
    .await?;
    let bytes = body.len();
    Ok(Served {
        response: http::reply(format, body, Some(&token)),
        bytes,
    })
}

// --- 14. access ---------------------------------------------------------------------

/// What this credential may do in the project (spec 7.9, `what=access`), re-read now.
#[derive(Serialize)]
pub(super) struct AccessView {
    protocol: &'static str,
    tool: &'static str,
    what: &'static str,
    project: Uuid,
    /// The principal's current product role in the project.
    role: String,
    /// The scopes of the grant that is active right now.
    scopes: Vec<String>,
    /// No write exists in this version: `none`.
    write_mode: &'static str,
    /// Always null in this version: budgets are not built.
    budget: Option<Value>,
    capabilities: Value,
}

#[utoipa::path(operation_id="agent_access", get, path="/projects/{id}/agent/access", tag="agent", params(("id"=Uuid,Path,description="Project")), security(("device_session"=["content:read"])), responses((status=200, description="The credential's active grant, the principal's current role, and what this server can do (capabilities are advertised, not assumed)", content((schema::AgentAccess="application/json"),(String="text/vnd.dynodoc.dct; v=1"))),(status=401, description="Not a project device session"),(status=403, description="Scope, grant, key class or project membership missing (uniform)"),(status=429, description="Admission budget")))]
async fn access(
    State(state): State<AppState>,
    auth: AgentAuth<ContentRead>,
    Path(project): Path<Uuid>,
    headers: HeaderMap,
    uri: Uri,
) -> Out {
    let ctx = Ctx::new(&state, &auth, project, &headers, Tool::Status, get_digest(&uri))?;
    let mut call = Call::default();
    let result: Result<Served, AgentFail> = async {
        let mut admitted = ctx.admit(&mut call).await?;
        let scopes: Option<Vec<String>> = sqlx::query_scalar("select scopes from device_key_project_grant where key_id=$1 and project_id=$2 and revoked_at is null")
            .bind(auth.binding().key_id)
            .bind(project)
            .fetch_optional(&mut *admitted.db)
            .await?;
        let view = AccessView {
            protocol: "dynodoc.agent/1",
            tool: "status",
            what: "access",
            project,
            role: product_role(&admitted.role).to_owned(),
            scopes: scopes.unwrap_or_default(),
            write_mode: "none",
            budget: None,
            capabilities: capabilities(),
        };
        let body = match ctx.format {
            Format::Json => serde_json::to_string(&view)
                .map_err(|_| ApiError::Internal("access view".into()))?,
            Format::Dct => HeaderBuilder::new(Tool::Status)
                .kv("what", "access")
                .kv("project", project)
                .kv("role", &view.role)
                .kv("scopes", view.scopes.join(","))
                .kv("write_mode", view.write_mode)
                .kv("calc", "none")
                .assemble(""),
        };
        let bytes = body.len();
        Ok(Served {
            response: http::reply(ctx.format, body, None),
            bytes,
        })
    }
    .await;
    ctx.finish(call, result).await
}

/// What this server version can and cannot do, for the discovery document and `access`.
fn capabilities() -> Value {
    json!({
        "read": true,
        "propose": false,
        "submit": false,
        "calc": "none",
        "fill": false,
        "sort": false,
        "add_slide": false,
        "attribution": null,
        "dlp": false
    })
}

// --- 15. discovery ------------------------------------------------------------------

#[utoipa::path(operation_id="agent_discovery", get, path="/.well-known/dynodoc-agent", tag="agent", responses((status=200, description="Protocol ids, formats, limits and capabilities of this server. No authentication; carries no project or user data", body=schema::AgentDiscovery, content_type="application/json")))]
async fn discovery(State(state): State<AppState>) -> Response {
    use engine_agent::budget::*;
    let body = json!({
        "protocol": "dynodoc.agent/1",
        "dct": "dd/1",
        "http": {
            "base": state.auth.public_base_url,
            "routes": "/projects/{id}/agent",
            "formats": ["application/json", "text/vnd.dynodoc.dct; v=1"],
            "etag": "\"<rev>:<chain8>\""
        },
        "mcp": null,
        "mcp_revisions": [],
        "protected_resource_metadata": null,
        "scopes": ["content:read"],
        "limits": {
            "read_max_bytes": MAX_MAX_BYTES,
            "read_default_bytes": DEFAULT_MAX_BYTES,
            "diff_default_bytes": DIFF_DEFAULT_MAX_BYTES,
            "outline_default_limit": OUTLINE_DEFAULT_LIMIT,
            "outline_max_limit": OUTLINE_MAX_LIMIT,
            "find_max_limit": FIND_MAX_LIMIT,
            "files_max_limit": FILES_MAX_LIMIT,
            "refs_per_read": MAX_REFS,
            "slides_per_read": MAX_SLIDES,
            "ops_per_call": null
        },
        "profiles": [],
        "capabilities": capabilities()
    });
    let mut response = Response::new(axum::body::Body::from(body.to_string()));
    let headers = response.headers_mut();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json"),
    );
    headers.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("public, max-age=300"),
    );
    response
}
