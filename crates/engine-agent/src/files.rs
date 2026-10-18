//! `files` (protocol spec 7.2): a listing formatter over project metadata the caller
//! supplies. The caller has already applied authorisation (project grant, per-file role);
//! this module only filters, orders, pages and writes. Titles and project names are
//! untrusted text and are quoted and escaped.

use engine_shared::DocumentId;
use serde::Serialize;
use uuid::Uuid;

use crate::budget::{
    resolve_limit, resolve_max_bytes, start_position, Unit, DEFAULT_MAX_BYTES, FILES_DEFAULT_LIMIT,
    FILES_MAX_LIMIT, MAX_QUERY_CHARS,
};
use crate::dct::{quote_counted, token, Tool};
use crate::error::AgentError;
use crate::escape::Dropped;
use crate::model::FileKind;
use crate::paging::{paged, PagedSpec};
use crate::response::{Response, Warnings};

/// One file the caller may show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub id: DocumentId,
    pub kind: FileKind,
    /// Current head `seq`.
    pub rev: u64,
    pub project_id: Uuid,
    /// Project display name (untrusted text).
    pub project: String,
    /// The caller's effective product role on the file (`viewer`, `editor`, ...).
    pub role: String,
    pub title: String,
}

#[derive(Debug, Clone, Default)]
pub struct FilesOptions {
    pub project: Option<Uuid>,
    /// Title substring, case-insensitive (at most 200 characters).
    pub q: Option<String>,
    pub kind: Option<FileKind>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileRow {
    /// First 8 hex characters of the document UUID.
    pub file: String,
    pub id: String,
    pub kind: String,
    pub rev: u64,
    pub project: String,
    pub role: String,
    pub title: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FilesData {
    pub n: usize,
    pub more: Option<String>,
    pub files: Vec<FileRow>,
}

fn short(id: &DocumentId) -> String {
    id.0.simple().to_string()[..8].to_string()
}

/// The tool. Entries are ordered by title, then id, so a page is stable for a fixed list.
pub fn files(entries: &[FileEntry], opts: &FilesOptions) -> Result<Response<FilesData>, AgentError> {
    let limit = resolve_limit(opts.limit, FILES_DEFAULT_LIMIT, FILES_MAX_LIMIT)?;
    let max_bytes = resolve_max_bytes(opts.max_bytes, DEFAULT_MAX_BYTES)?;
    let (start, _) = start_position(opts.cursor.as_deref(), 0)?;
    let needle = match &opts.q {
        Some(q) if q.chars().count() > MAX_QUERY_CHARS => {
            return Err(AgentError::LimitExceeded {
                field: "q",
                limit: MAX_QUERY_CHARS as u64,
                value: q.chars().count() as u64,
            })
        }
        Some(q) => Some(q.to_lowercase()),
        None => None,
    };
    let mut list: Vec<&FileEntry> = entries
        .iter()
        .filter(|e| opts.project.is_none_or(|p| p == e.project_id))
        .filter(|e| opts.kind.is_none_or(|k| k == e.kind))
        .filter(|e| needle.as_ref().is_none_or(|n| e.title.to_lowercase().contains(n)))
        .collect();
    list.sort_by(|a, b| (&a.title, a.id.0).cmp(&(&b.title, b.id.0)));

    let mut units = Vec::with_capacity(list.len());
    let mut rows = Vec::with_capacity(list.len());
    for e in &list {
        let (title, mut dropped) = quote_counted(&e.title);
        let (project, d) = quote_or_token(&e.project);
        dropped.add(d);
        let (role, d) = quote_or_token(&e.role);
        dropped.add(d);
        units.push(Unit {
            text: format!(
                "{} {} rev={} project={project} role={role} {title}\n",
                short(&e.id),
                e.kind,
                e.rev
            ),
            dropped,
            ids: Vec::new(),
        });
        rows.push(FileRow {
            file: short(&e.id),
            id: e.id.0.to_string(),
            kind: e.kind.to_string(),
            rev: e.rev,
            project: crate::escape::sanitize(&e.project),
            role: crate::escape::sanitize(&e.role),
            title: crate::escape::sanitize(&e.title),
        });
    }
    let spec = PagedSpec {
        tool: Tool::Files,
        file: None,
        lead: Vec::new(),
        quoted: Vec::new(),
        header_rev: true,
        show_n: true,
        tail: Vec::new(),
        base_warn: Warnings::default(),
        fixed: String::new(),
        fixed_dropped: Dropped::default(),
        cursor_aux: None,
    };
    paged(spec, &units, start, limit, max_bytes, move |taken, cursor| FilesData {
        n: taken,
        more: cursor.map(|c| c.to_string()),
        files: rows.into_iter().skip(start).take(taken).collect(),
    })
}

/// A bare token when `text` allows it, otherwise quoted; counts what quoting drops.
fn quote_or_token(text: &str) -> (String, Dropped) {
    let t = token(text);
    if t.starts_with('"') {
        quote_counted(text)
    } else {
        (t, Dropped::default())
    }
}
