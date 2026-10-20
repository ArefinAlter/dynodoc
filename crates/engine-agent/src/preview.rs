//! Dry run and preview (protocol spec 7.7, 8.7): what a compiled `propose` call would
//! change, as a DCT response of bounded size.
//!
//! The body is the `diff` of the state the call started from and the state after the
//! compiled ops (`diff::diff`, so a preview reads exactly like a `diff` of the resulting
//! draft would), cut at whole entries to fit `max_bytes`. The header carries the counts of
//! the call:
//!
//! ```text
//! dd/1 propose file=7034d450 proposal=4d0e9b1c rev=412 applied=3 expanded=3 stale=0 state=draft revision=2 trust=data ~tok=60
//! ~7C02 P #b7a210>a41c07 -6 +9
//! +B200 P #3e0001 after A19D "New paragraph preview…"
//! ```
//!
//! `applied` is the number of agent ops, `expanded` the number of engine ops they became.
//! A dry run (spec 8.7) says `applied=0 would_apply=<n>` instead. The per-op results (new
//! refs, hashes before and after) and the stale targets travel in the typed data, not in
//! the text; the header names at most five stale refs (`stale_refs=`).
//!
//! A preview never exceeds `max_bytes` (500 to 64,000, default 8,000, like `diff`): when the
//! entries do not all fit the response ends with the ones that do and `warn=truncated`; it
//! never holds half an entry.

use std::collections::BTreeMap;

use engine_core::materializer::DocumentState;
use serde::Serialize;
use uuid::Uuid;

use crate::budget::{resolve_max_bytes, DIFF_DEFAULT_MAX_BYTES, MAX_MAX_BYTES};
use crate::compile::{ChangeKind, Compiled};
use crate::dct::{HeaderBuilder, Tool};
use crate::diff::{diff, Detail, DiffEntry, DiffOptions};
use crate::error::AgentError;
use crate::escape::Dropped;
use crate::model::FileMeta;
use crate::refs::{Ref, RefIndex};
use crate::response::{Meta, Response, Warnings};

/// How to render a preview.
#[derive(Debug, Clone, Default)]
pub struct PreviewOptions {
    /// `summary` (default) or `text`, the `return=diff` form with word-level changes.
    pub detail: Detail,
    /// 500 to 64,000, default 8,000.
    pub max_bytes: Option<usize>,
    /// A dry run: nothing is stored, `applied=0 would_apply=<n>`.
    pub dry_run: bool,
    /// The proposal the call belongs to (its first 8 hex characters go in the header).
    pub proposal: Option<Uuid>,
    /// `draft`, `submitted`, ... (the server's word; absent in a dry run).
    pub state: Option<&'static str>,
    /// The proposal's `revision` after the call.
    pub revision: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewChange {
    #[serde(rename = "ref")]
    pub r: String,
    /// `changed`, `created`, `deleted` or `moved`.
    pub kind: &'static str,
    /// Hashes in full hex; the text shows six characters.
    pub old: Option<String>,
    pub new: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewOp {
    pub index: usize,
    pub id: Option<String>,
    pub op: &'static str,
    pub engine_ops: usize,
    pub changes: Vec<PreviewChange>,
    pub more_changes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewStale {
    pub op: usize,
    pub at: String,
    pub current: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PreviewData {
    pub applied: usize,
    pub would_apply: usize,
    pub expanded: usize,
    /// Blocks, rows or slides that differ after the call (all of them, shown or not).
    pub entries_total: usize,
    pub ops: Vec<PreviewOp>,
    pub stale: Vec<PreviewStale>,
    /// The diff entries that fit, in the order of the text.
    pub entries: Vec<DiffEntry>,
    /// Some entries were left out.
    pub truncated: bool,
}

/// Split the body of a `diff` response into entries: an entry starts with a line whose first
/// character is `~`, `+`, `-` or `>`; document text is escaped so that none of its lines
/// does.
fn units(body: &str) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let mut start = 0usize;
    for (i, line) in body.split_inclusive('\n').scan(0usize, |at, l| {
        let i = *at;
        *at += l.len();
        Some((i, l))
    }) {
        if i > start && line.starts_with(['~', '+', '-', '>']) {
            out.push(&body[start..i]);
            start = i;
        }
    }
    if start < body.len() {
        out.push(&body[start..]);
    }
    out
}

/// Render the preview of `compiled`, a successful [`crate::compile::compile`] of a call that
/// started from `before`. `file` describes the revision the call was made against.
pub fn preview(
    file: &FileMeta,
    before: &DocumentState,
    compiled: &Compiled,
    opts: &PreviewOptions,
) -> Result<Response<PreviewData>, AgentError> {
    let max_bytes = resolve_max_bytes(opts.max_bytes, DIFF_DEFAULT_MAX_BYTES)?;
    let full = diff(
        file,
        file.rev,
        before,
        &compiled.after,
        &DiffOptions {
            detail: opts.detail,
            max_bytes: Some(MAX_MAX_BYTES),
            cursor: None,
        },
    )?;
    let body = full.text.split_once('\n').map_or("", |(_, b)| b);
    let units = units(body);
    let diff_cut = full.data.more.is_some();
    let dropped = Dropped {
        ctrl: usize::from(full.meta.warn.iter().any(|w| w == "ctrl")),
        hidden: usize::from(full.meta.warn.iter().any(|w| w == "hidden")),
    };

    let ops = compiled.results.len();
    let stale_refs: Vec<&str> = compiled
        .stale
        .iter()
        .map(|s| s.at.as_str())
        .take(5)
        .collect();
    let header = |kept: usize| -> (HeaderBuilder, Warnings) {
        let truncated = diff_cut || kept < units.len();
        let warnings = Warnings {
            stale: !compiled.stale.is_empty(),
            truncated,
            dropped,
            ..Warnings::default()
        };
        let mut h = HeaderBuilder::new(Tool::Propose).kv("file", file.short_id());
        if let Some(p) = opts.proposal {
            h = h.kv("proposal", &p.simple().to_string()[..8]);
        }
        h = h.kv("rev", file.rev);
        h = if opts.dry_run {
            h.kv("applied", 0).kv("would_apply", ops)
        } else {
            h.kv("applied", ops)
        };
        h = h
            .kv("expanded", compiled.ops.len())
            .kv("stale", compiled.stale.len());
        if !stale_refs.is_empty() {
            h = h.kv("stale_refs", stale_refs.join(","));
        }
        h = h
            .kv_opt("state", opts.state)
            .kv_opt("revision", opts.revision);
        (warnings.apply(h).kv("trust", "data"), warnings)
    };
    // The longest prefix of entries whose whole response fits.
    let (mut kept, mut size) = (0usize, 0usize);
    for u in &units {
        if header(kept + 1).0.assembled_len(size + u.len()) > max_bytes {
            break;
        }
        kept += 1;
        size += u.len();
    }
    let (h, warnings) = header(kept);
    let text = h.assemble(&units[..kept].concat());
    debug_assert!(text.len() <= max_bytes);

    // Refs shown in the text, for `_meta.ids`.
    let idx = RefIndex::from_states(&[before, &compiled.after]);
    let shown: Vec<DiffEntry> = full.data.entries.iter().take(kept).cloned().collect();
    let mut ids = BTreeMap::new();
    for e in &shown {
        for r in [Some(&e.r), e.scope.as_ref()].into_iter().flatten() {
            if let Some(id) = Ref::parse(r).ok().and_then(|r| idx.resolve(&r).ok()) {
                ids.insert(r.clone(), id.0);
            }
        }
    }
    let meta = Meta::new(file, text.len(), max_bytes, None, ids, &warnings);

    let kind_name = |k: ChangeKind| match k {
        ChangeKind::Changed => "changed",
        ChangeKind::Created => "created",
        ChangeKind::Deleted => "deleted",
        ChangeKind::Moved => "moved",
    };
    let data = PreviewData {
        applied: if opts.dry_run { 0 } else { ops },
        would_apply: ops,
        expanded: compiled.ops.len(),
        entries_total: full.data.blocks,
        ops: compiled
            .results
            .iter()
            .map(|r| PreviewOp {
                index: r.index,
                id: r.id.clone(),
                op: r.op,
                engine_ops: r.engine_ops,
                changes: r
                    .changes
                    .iter()
                    .map(|c| PreviewChange {
                        r: c.at.to_string(),
                        kind: kind_name(c.kind),
                        old: c.old.as_ref().map(|h| h.as_hex().to_string()),
                        new: c.new.as_ref().map(|h| h.as_hex().to_string()),
                    })
                    .collect(),
                more_changes: r.more_changes,
            })
            .collect(),
        stale: compiled
            .stale
            .iter()
            .map(|s| PreviewStale {
                op: s.op,
                at: s.at.clone(),
                current: s.current.as_ref().map(|h| h.as_hex().to_string()),
            })
            .collect(),
        entries: shown,
        truncated: warnings.truncated,
    };
    Ok(Response { text, meta, data })
}
