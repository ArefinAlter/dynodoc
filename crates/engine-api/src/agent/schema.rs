//! Documentation shapes of the agent routes for the OpenAPI document. The wire values
//! are produced by `engine-agent` (`Response::json_mirror`, protocol spec 12.3); these
//! structs describe them and are never constructed. A test compares real responses with
//! this description (`tests/agent_read_test.rs`, "openapi_describes_every_response") so
//! the two cannot drift silently.

use serde::Serialize;
use utoipa::ToSchema;

use super::routes::BlocksReadBody;

/// The keys every JSON mirror starts with.
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentEnvelope {
    /// `dynodoc.agent/1`.
    protocol: String,
    /// `files`, `outline`, `read`, `find`, `diff` or `status`.
    tool: String,
    /// Full document UUID (absent for `files`).
    file: Option<String>,
    /// Event sequence the response was computed at; `0` is the empty initial state.
    rev: Option<u64>,
    /// First 8 hex characters of the chain hash at `rev`.
    chain: Option<String>,
    /// Always `data`: every string below that came from a document is untrusted text.
    trust: String,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentFileRow {
    /// First 8 hex characters of the document UUID.
    file: String,
    /// Full document UUID.
    id: String,
    /// `document`, `spreadsheet` or `presentation`.
    kind: String,
    /// Head sequence.
    rev: u64,
    project: String,
    /// The caller's current role on the file.
    role: String,
    title: String,
}

/// `files`.
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentFiles {
    #[serde(flatten)]
    envelope: AgentEnvelope,
    n: usize,
    more: Option<String>,
    files: Vec<AgentFileRow>,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentBlockRow {
    #[serde(rename = "ref")]
    r: String,
    /// Block type (`P`, `H1`..`H6`, `UL<n>`, `OL<n>`, `TBL<r>x<c>`, `CODE`, ...).
    t: String,
    /// 6-hex content hash.
    h: String,
    /// Words.
    w: usize,
    /// Preview (escaped).
    p: String,
    /// Depth 0 only: `[blocks, words]` under the heading.
    under: Option<Vec<usize>>,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentSheetRow {
    #[serde(rename = "ref")]
    r: String,
    name: String,
    rows: usize,
    cols: usize,
    h: String,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentShapeRow {
    #[serde(rename = "ref")]
    r: String,
    kind: String,
    role: Option<String>,
    h: String,
    w: usize,
    p: String,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentSlideRow {
    pos: usize,
    #[serde(rename = "ref")]
    r: String,
    layout: String,
    title: String,
    body: usize,
    notes_words: usize,
    images: usize,
    image_bytes: usize,
    hidden: bool,
    shapes: Vec<AgentShapeRow>,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentColumnProfile {
    col: String,
    header: String,
    /// `num`, `text`, `date`, `bool`, `formula` or `mixed`.
    #[serde(rename = "type")]
    ty: String,
    n: usize,
    blank: usize,
    distinct: Option<usize>,
    distinct_over: bool,
    /// Up to 5 `[value, count]` pairs, only when few distinct values.
    #[schema(value_type = Vec<Object>)]
    top: Vec<serde_json::Value>,
    min: Option<f64>,
    max: Option<f64>,
    sum: Option<f64>,
    pattern: Option<String>,
    exceptions: Vec<u32>,
    exceptions_more: usize,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentProfile {
    name: String,
    rows: usize,
    cols: usize,
    header: String,
    /// `[frozen rows, frozen columns]`.
    freeze: Vec<u64>,
    merges: usize,
    columns: Vec<AgentColumnProfile>,
}

/// `outline` and the sheet profile.
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentOutline {
    #[serde(flatten)]
    envelope: AgentEnvelope,
    kind: String,
    depth: u8,
    /// Blocks, sheets or slides in the selection, before paging.
    count: usize,
    words: Option<usize>,
    /// Outline hash: pass it back as `if_none_match`.
    oh: Option<String>,
    unchanged: bool,
    more: Option<String>,
    blocks: Option<Vec<AgentBlockRow>>,
    sheets: Option<Vec<AgentSheetRow>>,
    profile: Option<AgentProfile>,
    slides: Option<Vec<AgentSlideRow>>,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentReadItem {
    #[serde(rename = "ref")]
    r: String,
    t: String,
    h: Option<String>,
    /// The caller's `have` hash matched: no text was sent.
    equal: bool,
    redacted: bool,
    features: String,
    /// Escaped text lines.
    lines: Vec<String>,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentShapeRead {
    #[serde(rename = "ref")]
    r: String,
    kind: String,
    role: Option<String>,
    h: Option<String>,
    equal: bool,
    redacted: bool,
    text: String,
    alt: Option<String>,
    omitted_bytes: usize,
    bullets: bool,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentSlideRead {
    pos: usize,
    #[serde(rename = "ref")]
    r: String,
    layout: String,
    hidden: bool,
    redacted: bool,
    shapes: Vec<AgentShapeRead>,
    /// `[hash, text]` of the notes, when there are any.
    notes: Option<Vec<String>>,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentRowRead {
    row: u32,
    #[serde(rename = "ref")]
    r: Option<String>,
    cells: Vec<String>,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentRangeRead {
    /// Ref of the sheet.
    sheet: String,
    /// Effective range, clamped to the used cells.
    range: String,
    h: String,
    /// `none`: formulas are not evaluated.
    calc: String,
    columns: Vec<String>,
    rows: Vec<AgentRowRead>,
}

/// `read` (blocks, cells, slides).
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentRead {
    #[serde(flatten)]
    envelope: AgentEnvelope,
    n: usize,
    more: Option<String>,
    items: Option<Vec<AgentReadItem>>,
    slides: Option<Vec<AgentSlideRead>>,
    range: Option<AgentRangeRead>,
    profile: Option<AgentProfile>,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentHit {
    /// `7C02`, `7C02.2`, `3B77.r2c1`, `R91W!C14` or `W2SK.A1B2`.
    at: String,
    snippet: String,
    /// Character offset of the match in the searched text.
    offset: usize,
}

/// `find`.
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentFind {
    #[serde(flatten)]
    envelope: AgentEnvelope,
    n: usize,
    total: usize,
    total_capped: bool,
    more: Option<String>,
    hits: Vec<AgentHit>,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentDiffEntry {
    /// `~` changed, `+` added, `-` removed, `>` moved.
    op: String,
    #[serde(rename = "ref")]
    r: String,
    t: String,
    old: Option<String>,
    new: Option<String>,
    row: Option<u32>,
    pos: Option<usize>,
    scope: Option<String>,
    cells: Option<usize>,
    /// `[deleted, inserted]` characters.
    chars: Option<Vec<usize>>,
    after: Option<String>,
    preview: Option<String>,
    /// Escaped detail lines (`detail=text`).
    detail: Vec<String>,
}

/// `diff`.
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentDiff {
    #[serde(flatten)]
    envelope: AgentEnvelope,
    from: u64,
    to: u64,
    blocks: usize,
    changed: usize,
    added: usize,
    removed: usize,
    moved: usize,
    more: Option<String>,
    entries: Vec<AgentDiffEntry>,
}

/// `access` (`status what=access`).
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentAccess {
    protocol: String,
    tool: String,
    what: String,
    project: String,
    /// The principal's current role in the project.
    role: String,
    /// Scopes of the grant that is active now.
    scopes: Vec<String>,
    /// The project's `write_mode`: `none` (agents only read), `draft` or `change_request`.
    write_mode: String,
    /// Whether a person must submit an agent's draft even in `change_request` mode.
    submit_requires_person: bool,
    /// The grant's budgets in this project: `calls_per_minute` and `read_bytes_per_day`,
    /// each `{limit, used}`; and, when writing is on, `proposals_per_day` and
    /// `open_proposals` (`{limit, used}`, counted for the agent identity) and
    /// `ops_per_proposal` (`{limit}`).
    #[schema(value_type = Object)]
    budget: Option<serde_json::Value>,
    /// What this credential can do now: `propose` and `submit` are true only when its grant
    /// has the scope and the project's policy lets the agent do it.
    #[schema(value_type = Object)]
    capabilities: serde_json::Value,
}

/// Body of `propose` (docs/AI-AGENT-PROTOCOL-SPEC.md 7.7). The file is the one in the path.
/// Unknown fields are refused. `ops` items are flat objects validated strictly against
/// the operation grammar (spec 8): `op` is one of `set_text`, `replace`, `replace_all`,
/// `insert`, `delete`, `move`, `set_type`, `set_cells`, `clear`, `set_shape`, `set_notes`;
/// the others (`fill`, `add_slide`, `delete_slide`, `macro`) are `unsupported_op`.
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentProposeBody {
    /// The file again (optional); if given it must be the file of the path.
    file: Option<String>,
    /// The revision the refs and hashes in `ops` came from (0 up to the head).
    rev: u64,
    /// A new UUID per call; reuse it only to retry. With the same canonical body it returns
    /// the stored receipt; with another body it is `idempotency_conflict`.
    request_id: String,
    /// An existing proposal of this agent to extend; omit to start one.
    proposal: Option<String>,
    /// A short title for reviewers (at most 200 characters; no control or hidden characters).
    name: Option<String>,
    /// One to 200 ops, each `{op, ref|sheet|slide..., text|find|with|values..., base, why, id}`.
    #[schema(value_type = Vec<Object>)]
    ops: Vec<serde_json::Value>,
    /// Compile and preview only: nothing is stored, no receipt is written.
    dry_run: Option<bool>,
    /// Refuse an op whose `base` hash differs from the team's current hash (`stale_base`)
    /// instead of recording a warning.
    strict: Option<bool>,
    /// `summary` (default) or `diff`.
    #[serde(rename = "return")]
    return_kind: Option<String>,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentPreviewChange {
    #[serde(rename = "ref")]
    r: String,
    /// `changed`, `created`, `deleted` or `moved`.
    kind: String,
    old: Option<String>,
    new: Option<String>,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentPreviewOp {
    index: usize,
    /// The agent's own label for the op.
    id: Option<String>,
    op: String,
    /// The agent's reason (untrusted text).
    why: Option<String>,
    engine_ops: usize,
    changes: Vec<AgentPreviewChange>,
    more_changes: usize,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentStale {
    /// The index of the op (absent in a replay).
    op: Option<usize>,
    /// The ref or range the op named.
    at: String,
    /// The team's hash now (absent in a replay, or when the target is gone).
    current: Option<String>,
}

/// `propose`: the preview of the call (or of a dry run), or the stored receipt of a call
/// already made (`replayed`; then `ops`, `entries`, `entries_total`, `would_apply` and
/// `truncated` are absent).
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentPropose {
    #[serde(flatten)]
    envelope: AgentEnvelope,
    /// The proposal's id (absent in a dry run of a new proposal).
    proposal: Option<String>,
    request_id: Option<String>,
    /// `draft`, `open`, `merged`, `declined` or `withdrawn` (absent in a dry run).
    state: Option<String>,
    /// The draft's revision after the call.
    revision: Option<i64>,
    /// The proposal's base revision, the team's head, and how far behind it is.
    base: i64,
    head: u64,
    behind: u64,
    /// Ops applied (0 in a dry run).
    applied: u64,
    would_apply: Option<u64>,
    /// Engine operations the ops became.
    expanded: u64,
    entries_total: Option<usize>,
    ops: Option<Vec<AgentPreviewOp>>,
    /// The ops whose `base` was out of date (at most five in a replay).
    stale: Vec<AgentStale>,
    stale_total: u64,
    entries: Option<Vec<AgentDiffEntry>>,
    truncated: Option<bool>,
    /// Whether this call opened the proposal (a replay only).
    opened: Option<bool>,
    dry_run: Option<bool>,
    replayed: bool,
    /// `stale`, `behind`, `truncated`.
    warn: Vec<String>,
}

/// Optional note on `submit` and `withdraw`.
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentNoteBody {
    /// At most 1000 characters; no control or hidden characters.
    note: Option<String>,
}

/// `submit` and `withdraw`.
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentSubmit {
    protocol: String,
    tool: String,
    /// `submit` or `withdraw`.
    action: String,
    proposal: String,
    file: String,
    /// `open`, `awaiting_person` (a person must submit it) or `withdrawn`.
    state: String,
    /// Whether this call changed anything.
    changed: bool,
    behind: u64,
    trust: String,
}

/// One proposal in `proposals` of the agent's own list.
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentProposalRow {
    /// The id the submit, withdraw and status routes take.
    proposal: String,
    /// Full document UUID.
    file: String,
    /// `draft`, `open`, `changes_requested`, `declined`, `merged` or `withdrawn`.
    state: String,
    /// The team revision the proposal started from, and the file's head now.
    base: i64,
    head: i64,
    behind: i64,
    revision: i64,
    /// Engine operations stored in the proposal.
    edits: i64,
    created_at: String,
    submitted_at: Option<String>,
}

/// The agent's own proposals in a project (the list route of the agent plane).
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentProposalList {
    protocol: String,
    tool: String,
    what: String,
    /// The filter that was applied: `active`, `all` or one state.
    state: String,
    n: usize,
    more: bool,
    write_mode: String,
    /// Proposals the person's agents opened since midnight UTC, and have open, against the
    /// project's limits.
    opened_today: i64,
    opened_limit: i64,
    open: i64,
    open_limit: i64,
    /// Absent from the DCT form, which lists them as `proposal` lines.
    proposals: Vec<AgentProposalRow>,
    trust: String,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentReviewNote {
    /// `approved`, `changes_requested` or `commented`.
    verdict: String,
    /// The reviewer's display name (untrusted text, at most 80 characters).
    by: String,
    /// The reviewer's note (untrusted text, at most 400 characters).
    note: String,
    /// The proposal changed after this review.
    stale: bool,
    /// The node the comment is on, when it is on one.
    node: Option<String>,
}

#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentConflict {
    #[serde(rename = "ref")]
    r: Option<String>,
    /// The engine field both sides changed.
    field: String,
    /// Six-hex hashes of the team's and the draft's value; never the values.
    team: String,
    draft: String,
}

/// `status` of a proposal.
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentProposalStatus {
    protocol: String,
    tool: String,
    what: String,
    proposal: String,
    file: String,
    /// `draft`, `open`, `changes_requested`, `declined`, `merged` or `withdrawn`.
    state: String,
    base: i64,
    head: u64,
    behind: u64,
    revision: i64,
    /// Engine operations in the proposal.
    edits: u64,
    approvals: i64,
    required: i64,
    satisfied: bool,
    /// Same-field overlaps a merge would meet (the first 20 are in `conflict_list`).
    conflicts: usize,
    reviews: Vec<AgentReviewNote>,
    conflict_list: Vec<AgentConflict>,
    merged_at: Option<String>,
    trust: String,
}

/// The discovery document.
#[derive(ToSchema, Serialize)]
#[allow(dead_code)]
pub struct AgentDiscovery {
    protocol: String,
    dct: String,
    #[schema(value_type = Object)]
    http: serde_json::Value,
    /// Null until an MCP server is deployed.
    #[schema(value_type = Object)]
    mcp: Option<serde_json::Value>,
    mcp_revisions: Vec<String>,
    protected_resource_metadata: Option<String>,
    /// The read scope (`content:read`), which every tool needs.
    scopes: Vec<String>,
    /// The write scopes (`content:propose`, `content:submit`), independent of each other, and
    /// each held beside the read scope. A project must also set `write_mode`.
    write_scopes: Vec<String>,
    #[schema(value_type = Object)]
    limits: serde_json::Value,
    profiles: Vec<String>,
    #[schema(value_type = Object)]
    capabilities: serde_json::Value,
}

#[derive(utoipa::OpenApi)]
#[openapi(
    paths(
        super::routes::files,
        super::routes::outline,
        super::routes::blocks,
        super::routes::blocks_read,
        super::routes::cells,
        super::routes::profile,
        super::routes::slides,
        super::routes::find,
        super::routes::diff,
        super::routes::access,
        super::routes::discovery,
        super::write::propose,
        super::proposal::submit,
        super::proposal::withdraw,
        super::proposal::status,
        super::proposal::list
    ),
    components(schemas(
        AgentEnvelope,
        AgentFiles,
        AgentFileRow,
        AgentOutline,
        AgentBlockRow,
        AgentSheetRow,
        AgentShapeRow,
        AgentSlideRow,
        AgentColumnProfile,
        AgentProfile,
        AgentRead,
        AgentReadItem,
        AgentShapeRead,
        AgentSlideRead,
        AgentRowRead,
        AgentRangeRead,
        AgentFind,
        AgentHit,
        AgentDiff,
        AgentDiffEntry,
        AgentAccess,
        AgentDiscovery,
        BlocksReadBody,
        AgentProposeBody,
        AgentPreviewChange,
        AgentPreviewOp,
        AgentStale,
        AgentPropose,
        AgentNoteBody,
        AgentSubmit,
        AgentReviewNote,
        AgentConflict,
        AgentProposalStatus,
        AgentProposalRow,
        AgentProposalList
    ))
)]
pub struct AgentApi;
