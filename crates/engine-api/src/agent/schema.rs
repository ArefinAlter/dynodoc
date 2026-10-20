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
    /// `none`: no write exists in this version.
    write_mode: String,
    /// The grant's budgets in this project: `calls_per_minute` and `read_bytes_per_day`,
    /// each `{limit, used}`.
    #[schema(value_type = Object)]
    budget: Option<serde_json::Value>,
    #[schema(value_type = Object)]
    capabilities: serde_json::Value,
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
    scopes: Vec<String>,
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
        super::routes::discovery
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
        BlocksReadBody
    ))
)]
pub struct AgentApi;
