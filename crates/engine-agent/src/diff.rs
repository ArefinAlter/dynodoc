//! `diff` (protocol spec 6.9, 7.6): what changed between two revisions of a file.
//!
//! The candidates come from `workspace_merge::diff(from, to)`, the semantic operations the
//! reviewer UI uses; each is mapped to the node it touches, and the node is then compared
//! in both states by hash, so edits that change nothing the protocol shows (a `label`
//! rewrite with equal content, geometry) produce no line.
//!
//! Documents follow spec 6.9. Spreadsheets (rows) and presentations (slides and shapes)
//! are not specified there; this crate writes them with the same line grammar, as an
//! extension listed in the crate documentation.
//!
//! ```text
//! ~REF TYPE #old>new [r<row>] [s<pos>] [in SCOPE] [cells=N] [-D +I] ["preview"]   changed
//! +REF TYPE #hash   [...] [after PREV|first] ["preview"]                            added
//! -REF TYPE #hash   [...] ["preview"]                                               removed
//! >REF TYPE #hash   [...] [after PREV|first]                                        moved
//! ```
//!
//! With `detail=text` each entry is followed by its detail lines: a word-level inline diff
//! `[-deleted-]{+inserted+}` for changed text, the block text for added blocks, and
//! `A1<TAB>old<TAB>new` for changed cells.

use std::collections::{BTreeMap, BTreeSet};

use engine_core::materializer::DocumentState;
use engine_core::workspace_merge;
use engine_shared::NodeId;
use serde::Serialize;
use similar::{ChangeTag, TextDiff};

use crate::a1::CellRef;
use crate::budget::{resolve_max_bytes, Cursor, Unit, DIFF_DEFAULT_MAX_BYTES};
use crate::dct::Tool;
use crate::doc::{doc_blocks, DocBlock};
use crate::error::AgentError;
use crate::escape::{escape_with, guard_line_start, Ctx, Dropped, EXTRA_DIFF, EXTRA_QUOTED};
use crate::hash::{notes_hash, Hash};
use crate::model::{FileKind, FileMeta, Tree};
use crate::paging::{paged, PagedSpec};
use crate::refs::{Ref, RefIndex};
use crate::response::{Response, Warnings};
use crate::sheet::SheetView;
use crate::slides::SlideView;
use crate::text::{body_lines, Inline};

const PREVIEW_CHARS: usize = 48;
/// Words of unchanged text kept around a change in an inline diff.
const CONTEXT_WORDS: usize = 4;
/// Longer texts are shown as one replacement instead of a word diff.
const WORD_DIFF_LIMIT: usize = 100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Detail {
    #[default]
    Summary,
    Text,
}

impl Detail {
    pub fn parse(s: &str) -> Option<Detail> {
        match s {
            "summary" => Some(Detail::Summary),
            "text" => Some(Detail::Text),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct DiffOptions {
    pub detail: Detail,
    /// Default 8,000 for `diff` (spec 7.6).
    pub max_bytes: Option<usize>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffEntry {
    /// `~` changed, `+` added, `-` removed, `>` moved.
    pub op: String,
    #[serde(rename = "ref")]
    pub r: String,
    pub t: String,
    pub old: Option<String>,
    pub new: Option<String>,
    /// Sheet row number in the `to` revision.
    pub row: Option<u32>,
    /// Slide position in the `to` revision.
    pub pos: Option<usize>,
    /// Sheet or slide ref the entry lives in.
    pub scope: Option<String>,
    pub cells: Option<usize>,
    /// Characters deleted and inserted.
    pub chars: Option<(usize, usize)>,
    /// Ref of the block before it in the `to` revision, or `first`.
    pub after: Option<String>,
    pub preview: Option<String>,
    /// Sanitised detail lines (`detail=text`).
    pub detail: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffData {
    pub from: u64,
    pub to: u64,
    pub blocks: usize,
    pub changed: usize,
    pub added: usize,
    pub removed: usize,
    pub moved: usize,
    pub more: Option<String>,
    pub entries: Vec<DiffEntry>,
}

struct Pending {
    entry: DiffEntry,
    unit: Unit,
}

impl DiffEntry {
    fn new(op: &str, r: &Ref, t: &str) -> Self {
        Self {
            op: op.into(),
            r: r.to_string(),
            t: t.into(),
            old: None,
            new: None,
            row: None,
            pos: None,
            scope: None,
            cells: None,
            chars: None,
            after: None,
            preview: None,
            detail: Vec::new(),
        }
    }
}

/// A word-level inline diff: marked text (escaped, ready for a line) and the characters
/// deleted and inserted.
struct Words {
    marked: String,
    del: usize,
    ins: usize,
    dropped: Dropped,
}

enum Piece {
    Equal(String),
    Change { del: String, ins: String },
}

fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// Byte offset just after the `n`-th word of `s` (0 when `n` is 0).
fn after_words(s: &str, n: usize) -> usize {
    let mut seen = 0;
    let mut in_word = false;
    for (i, c) in s.char_indices() {
        if c.is_whitespace() {
            if in_word {
                seen += 1;
                in_word = false;
                if seen == n {
                    return i;
                }
            }
        } else {
            in_word = true;
        }
    }
    s.len()
}

/// Byte offset where the last `n` words of `s` start.
fn last_words_start(s: &str, n: usize) -> usize {
    let total = word_count(s);
    if n >= total {
        return 0;
    }
    let skip = total - n;
    let mut seen = 0;
    let mut in_word = false;
    for (i, c) in s.char_indices() {
        if c.is_whitespace() {
            in_word = false;
        } else if !in_word {
            in_word = true;
            if seen == skip {
                return i;
            }
            seen += 1;
        }
    }
    s.len()
}

fn word_diff(old: &str, new: &str) -> Words {
    let mut pieces: Vec<Piece> = Vec::new();
    if old.len() + new.len() > WORD_DIFF_LIMIT {
        pieces.push(Piece::Change {
            del: old.into(),
            ins: new.into(),
        });
    } else {
        let diff = TextDiff::from_words(old, new);
        let mut equal = String::new();
        let (mut del, mut ins) = (String::new(), String::new());
        for ch in diff.iter_all_changes() {
            match ch.tag() {
                ChangeTag::Equal => {
                    if !del.is_empty() || !ins.is_empty() {
                        pieces.push(Piece::Change {
                            del: std::mem::take(&mut del),
                            ins: std::mem::take(&mut ins),
                        });
                    }
                    equal.push_str(ch.value());
                }
                tag => {
                    if !equal.is_empty() {
                        pieces.push(Piece::Equal(std::mem::take(&mut equal)));
                    }
                    if tag == ChangeTag::Delete {
                        del.push_str(ch.value());
                    } else {
                        ins.push_str(ch.value());
                    }
                }
            }
        }
        if !equal.is_empty() {
            pieces.push(Piece::Equal(equal));
        }
        if !del.is_empty() || !ins.is_empty() {
            pieces.push(Piece::Change { del, ins });
        }
    }
    let mut marked = String::new();
    let (mut ndel, mut nins) = (0, 0);
    let mut dropped = Dropped::default();
    let mut esc = |s: &str, out: &mut String| {
        let (e, d) = escape_with(s, EXTRA_DIFF);
        out.push_str(&e);
        dropped.add(d);
    };
    let last = pieces.len().saturating_sub(1);
    for (i, p) in pieces.iter().enumerate() {
        match p {
            Piece::Equal(t) => {
                let n = word_count(t);
                let first = i == 0;
                let end = i == last;
                if first && n > CONTEXT_WORDS {
                    marked.push('…');
                    esc(&t[last_words_start(t, CONTEXT_WORDS)..], &mut marked);
                } else if end && n > CONTEXT_WORDS {
                    esc(&t[..after_words(t, CONTEXT_WORDS)], &mut marked);
                    marked.push('…');
                } else if !first && !end && n > 2 * CONTEXT_WORDS {
                    esc(&t[..after_words(t, CONTEXT_WORDS)], &mut marked);
                    marked.push_str(" … ");
                    esc(&t[last_words_start(t, CONTEXT_WORDS)..], &mut marked);
                } else {
                    esc(t, &mut marked);
                }
            }
            Piece::Change { del, ins } => {
                if !del.is_empty() {
                    marked.push_str("[-");
                    esc(del, &mut marked);
                    marked.push_str("-]");
                    ndel += del.chars().count();
                }
                if !ins.is_empty() {
                    marked.push_str("{+");
                    esc(ins, &mut marked);
                    marked.push_str("+}");
                    nins += ins.chars().count();
                }
            }
        }
    }
    Words {
        marked,
        del: ndel,
        ins: nins,
        dropped,
    }
}

/// Quoted, escaped preview of `inline` for a summary line.
fn quoted_preview(inline: &Inline) -> (String, Dropped) {
    let (p, d) = inline.preview_with(PREVIEW_CHARS, EXTRA_QUOTED);
    (format!("\"{p}\""), d)
}

fn summary(e: &DiffEntry, quoted: Option<&str>) -> String {
    let hashes = match (&e.old, &e.new) {
        (Some(o), Some(n)) => format!(" #{o}>{n}"),
        (Some(h), None) | (None, Some(h)) => format!(" #{h}"),
        (None, None) => String::new(),
    };
    let mut s = format!("{}{} {}{hashes}", e.op, e.r, e.t);
    if let Some(r) = e.row {
        s.push_str(&format!(" r{r}"));
    }
    if let Some(p) = e.pos {
        s.push_str(&format!(" s{p}"));
    }
    if let Some(sc) = &e.scope {
        s.push_str(&format!(" in {sc}"));
    }
    if let Some(c) = e.cells {
        s.push_str(&format!(" cells={c}"));
    }
    if let Some((d, i)) = e.chars {
        s.push_str(&format!(" -{d} +{i}"));
    }
    if let Some(a) = &e.after {
        s.push_str(&format!(" after {a}"));
    }
    if let Some(q) = quoted {
        s.push(' ');
        s.push_str(q);
    }
    s
}

/// Build an entry's unit: the summary line plus `detail` lines, each already escaped.
fn finish(
    entry: DiffEntry,
    quoted: Option<(String, Dropped)>,
    detail_lines: Vec<String>,
    mut dropped: Dropped,
    ids: Vec<(Ref, NodeId)>,
) -> Pending {
    let mut text = summary(&entry, quoted.as_ref().map(|(q, _)| q.as_str()));
    text.push('\n');
    if let Some((_, d)) = &quoted {
        dropped.add(*d);
    }
    for l in &detail_lines {
        text.push_str(l);
        text.push('\n');
    }
    Pending {
        entry,
        unit: Unit { text, dropped, ids },
    }
}

fn tag_of(label: &str) -> String {
    label.to_string()
}

fn diff_documents(
    idx: &RefIndex,
    from: &DocumentState,
    to: &DocumentState,
    touched: &BTreeSet<NodeId>,
    detail: Detail,
) -> Vec<Pending> {
    let (ft, tt) = (Tree::new(from), Tree::new(to));
    let fb = doc_blocks(&ft, idx, false);
    let tb = doc_blocks(&tt, idx, false);
    let fmap: BTreeMap<&NodeId, &DocBlock> = fb.iter().map(|b| (&b.node.id, b)).collect();
    let tmap: BTreeMap<&NodeId, usize> = tb.iter().enumerate().map(|(i, b)| (&b.node.id, i)).collect();
    let mut out = Vec::new();
    for (i, b) in tb.iter().enumerate() {
        if !touched.contains(&b.node.id) {
            continue;
        }
        let after = Some(i.checked_sub(1).map_or("first".to_string(), |p| tb[p].r.to_string()));
        let ids = vec![(b.r.clone(), b.node.id.clone())];
        match fmap.get(&b.node.id) {
            None => {
                let mut e = DiffEntry::new("+", &b.r, &tag_of(&b.text.label));
                e.new = Some(b.hash.h6().into());
                e.after = after;
                let inline = b.text.preview_inline();
                e.preview = Some(inline.preview_raw(PREVIEW_CHARS));
                let mut dropped = Dropped::default();
                let mut lines = Vec::new();
                if detail == Detail::Text {
                    let (ls, d) = body_lines(&b.text, false, Ctx::Diff);
                    dropped.add(d);
                    e.detail = ls.iter().map(|l| crate::escape::sanitize(l)).collect();
                    lines = ls.into_iter().map(|l| guard_line_start(l, Ctx::Diff)).collect();
                }
                let q = quoted_preview(&inline);
                out.push(finish(e, Some(q), lines, dropped, ids));
            }
            Some(old) => {
                let moved = old.node.pos != b.node.pos || old.node.parent_id != b.node.parent_id;
                if old.hash != b.hash {
                    let (a, c) = (old.text.plain(), b.text.plain());
                    let w = word_diff(&a, &c);
                    let mut e = DiffEntry::new("~", &b.r, &tag_of(&b.text.label));
                    e.old = Some(old.hash.h6().into());
                    e.new = Some(b.hash.h6().into());
                    e.chars = Some((w.del, w.ins));
                    let mut lines = Vec::new();
                    if detail == Detail::Text {
                        lines.push(guard_line_start(w.marked.clone(), Ctx::Diff));
                        e.detail = vec![format!("{a}\u{2192}{c}")];
                        e.detail = vec![plain_marked(&w)];
                    }
                    out.push(finish(e, None, lines, w.dropped, ids));
                } else if moved {
                    let mut e = DiffEntry::new(">", &b.r, &tag_of(&b.text.label));
                    e.new = Some(b.hash.h6().into());
                    e.after = after;
                    out.push(finish(e, None, Vec::new(), Dropped::default(), ids));
                }
            }
        }
    }
    for b in fb.iter().filter(|b| touched.contains(&b.node.id) && !tmap.contains_key(&b.node.id)) {
        let mut e = DiffEntry::new("-", &b.r, &tag_of(&b.text.label));
        e.old = Some(b.hash.h6().into());
        let inline = b.text.preview_inline();
        e.preview = Some(inline.preview_raw(PREVIEW_CHARS));
        let q = quoted_preview(&inline);
        out.push(finish(
            e,
            Some(q),
            Vec::new(),
            Dropped::default(),
            vec![(b.r.clone(), b.node.id.clone())],
        ));
    }
    out
}

/// The inline-diff text for the typed data: the line escapes undone, markers kept.
fn plain_marked(w: &Words) -> String {
    crate::escape::unescape(&w.marked).unwrap_or_default()
}

fn diff_sheets(
    idx: &RefIndex,
    from: &DocumentState,
    to: &DocumentState,
    touched: &BTreeSet<NodeId>,
    detail: Detail,
) -> Vec<Pending> {
    let (ft, tt) = (Tree::new(from), Tree::new(to));
    let fviews: Vec<SheetView> = ft.sheets().iter().map(|s| SheetView::new(&ft, s)).collect();
    let tviews: Vec<SheetView> = tt.sheets().iter().map(|s| SheetView::new(&tt, s)).collect();
    // row id -> (sheet index, row index)
    let locate = |views: &[SheetView]| -> BTreeMap<NodeId, (usize, usize)> {
        let mut m = BTreeMap::new();
        for (s, v) in views.iter().enumerate() {
            for (r, row) in v.rows.iter().enumerate() {
                m.insert(row.id.clone(), (s, r));
            }
        }
        m
    };
    let (frows, trows) = (locate(&fviews), locate(&tviews));
    let mut out = Vec::new();
    for tv in &tviews {
        let Some(sref) = idx.ref_of(&tv.node.id) else { continue };
        if touched.contains(&tv.node.id) && !fviews.iter().any(|f| f.node.id == tv.node.id) {
            let mut e = DiffEntry::new("+", &sref, "SHEET");
            e.new = Some(tv.hash().h6().into());
            e.cells = Some(tv.rows.len());
            let q = quoted_preview(&Inline::text(tv.name.clone()));
            out.push(finish(e, Some(q), Vec::new(), Dropped::default(), vec![(sref.clone(), tv.node.id.clone())]));
        }
        for (r, row) in tv.rows.iter().enumerate() {
            if !touched.contains(&row.id) {
                continue;
            }
            let Some(rref) = idx.ref_of(&row.id) else { continue };
            let ids = vec![(rref.clone(), row.id.clone())];
            let prior = frows.get(&row.id).map(|&(s, rr)| (&fviews[s], rr));
            match prior {
                None => {
                    let mut e = DiffEntry::new("+", &rref, "ROW");
                    e.new = Some(crate::hash::row_hash(&tv.row_cells(r)).h6().into());
                    e.row = Some(r as u32 + 1);
                    e.scope = Some(sref.to_string());
                    out.push(finish(e, None, Vec::new(), Dropped::default(), ids));
                }
                Some((fv, fr)) => {
                    let width = fv.cols.max(tv.cols);
                    let cells = |v: &SheetView, i: usize| -> Vec<String> { (0..width).map(|c| v.cell(i, c)).collect() };
                    let (old, new) = (cells(fv, fr), cells(tv, r));
                    if old != new {
                        let changed: Vec<usize> = (0..width).filter(|&c| old[c] != new[c]).collect();
                        let mut e = DiffEntry::new("~", &rref, "ROW");
                        e.old = Some(crate::hash::row_hash(&old).h6().into());
                        e.new = Some(crate::hash::row_hash(&new).h6().into());
                        e.row = Some(r as u32 + 1);
                        e.scope = Some(sref.to_string());
                        e.cells = Some(changed.len());
                        let mut dropped = Dropped::default();
                        let mut lines = Vec::new();
                        if detail == Detail::Text {
                            for c in changed {
                                let at = CellRef { col: c as u16, row: r as u32 + 1 };
                                let (o, d1) = crate::escape::escape(&old[c]);
                                let (n, d2) = crate::escape::escape(&new[c]);
                                dropped.add(d1);
                                dropped.add(d2);
                                lines.push(format!("{at}\t{o}\t{n}"));
                                e.detail.push(format!(
                                    "{at}\t{}\t{}",
                                    crate::escape::sanitize(&old[c]),
                                    crate::escape::sanitize(&new[c])
                                ));
                            }
                        }
                        out.push(finish(e, None, lines, dropped, ids));
                    } else if fv.rows[fr].pos != row.pos {
                        let mut e = DiffEntry::new(">", &rref, "ROW");
                        e.new = Some(crate::hash::row_hash(&new).h6().into());
                        e.row = Some(r as u32 + 1);
                        e.scope = Some(sref.to_string());
                        out.push(finish(e, None, Vec::new(), Dropped::default(), ids));
                    }
                }
            }
        }
    }
    for fv in &fviews {
        let Some(sref) = idx.ref_of(&fv.node.id) else { continue };
        if touched.contains(&fv.node.id) && !tviews.iter().any(|t| t.node.id == fv.node.id) {
            let mut e = DiffEntry::new("-", &sref, "SHEET");
            e.old = Some(fv.hash().h6().into());
            let q = quoted_preview(&Inline::text(fv.name.clone()));
            out.push(finish(e, Some(q), Vec::new(), Dropped::default(), vec![(sref.clone(), fv.node.id.clone())]));
        }
        for (r, row) in fv.rows.iter().enumerate() {
            if !touched.contains(&row.id) || trows.contains_key(&row.id) {
                continue;
            }
            let Some(rref) = idx.ref_of(&row.id) else { continue };
            let mut e = DiffEntry::new("-", &rref, "ROW");
            e.old = Some(crate::hash::row_hash(&fv.row_cells(r)).h6().into());
            e.scope = Some(sref.to_string());
            out.push(finish(e, None, Vec::new(), Dropped::default(), vec![(rref.clone(), row.id.clone())]));
        }
    }
    out
}

fn slide_own_hash(v: &SlideView) -> Hash {
    Hash::of(&serde_json::json!({
        "label": crate::model::field_string(&v.node.current_fields["label"]),
        "layout": v.layout,
        "hidden": v.hidden,
        "notes": notes_hash(&v.notes).as_hex(),
    }))
}

fn diff_slides(
    idx: &RefIndex,
    from: &DocumentState,
    to: &DocumentState,
    touched: &BTreeSet<NodeId>,
    detail: Detail,
) -> Vec<Pending> {
    let (ft, tt) = (Tree::new(from), Tree::new(to));
    let fviews: Vec<SlideView> = ft.slides().iter().map(|s| SlideView::new(&ft, s)).collect();
    let tviews: Vec<SlideView> = tt.slides().iter().map(|s| SlideView::new(&tt, s)).collect();
    let find = |views: &'_ [SlideView<'_>], id: &NodeId| views.iter().position(|v| &v.node.id == id);
    let mut out = Vec::new();
    for (p, tv) in tviews.iter().enumerate() {
        let Some(sref) = idx.ref_of(&tv.node.id) else { continue };
        let sids = vec![(sref.clone(), tv.node.id.clone())];
        if touched.contains(&tv.node.id) {
            match find(&fviews, &tv.node.id) {
                None => {
                    let mut e = DiffEntry::new("+", &sref, "SLIDE");
                    e.new = Some(slide_own_hash(tv).h6().into());
                    e.pos = Some(p + 1);
                    out.push(finish(e, None, Vec::new(), Dropped::default(), sids.clone()));
                }
                Some(fp) => {
                    let fv = &fviews[fp];
                    let (oh, nh) = (slide_own_hash(fv), slide_own_hash(tv));
                    if oh != nh {
                        let mut e = DiffEntry::new("~", &sref, "SLIDE");
                        e.old = Some(oh.h6().into());
                        e.new = Some(nh.h6().into());
                        e.pos = Some(p + 1);
                        let mut lines = Vec::new();
                        let mut dropped = Dropped::default();
                        if fv.notes != tv.notes {
                            let w = word_diff(&fv.notes, &tv.notes);
                            e.chars = Some((w.del, w.ins));
                            if detail == Detail::Text {
                                lines.push(format!("notes\t{}", w.marked));
                                e.detail = vec![format!("notes\t{}", plain_marked(&w))];
                                dropped.add(w.dropped);
                            }
                        }
                        out.push(finish(e, None, lines, dropped, sids.clone()));
                    } else if fv.node.pos != tv.node.pos {
                        let mut e = DiffEntry::new(">", &sref, "SLIDE");
                        e.new = Some(nh.h6().into());
                        e.pos = Some(p + 1);
                        out.push(finish(e, None, Vec::new(), Dropped::default(), sids.clone()));
                    }
                }
            }
        }
        for s in &tv.shapes {
            if !touched.contains(&s.node.id) {
                continue;
            }
            let Some(shr) = idx.ref_of(&s.node.id) else { continue };
            let ids = vec![(shr.clone(), s.node.id.clone()), (sref.clone(), tv.node.id.clone())];
            let tag = s.role.unwrap_or(s.kind);
            let prior = fviews
                .iter()
                .find_map(|fv| fv.shapes.iter().find(|x| x.node.id == s.node.id).map(|x| (fv, x)));
            match prior {
                None => {
                    let mut e = DiffEntry::new("+", &shr, tag);
                    e.new = Some(s.hash().h6().into());
                    e.scope = Some(sref.to_string());
                    let q = (s.kind == "text").then(|| quoted_preview(&Inline::text(s.text.clone())));
                    out.push(finish(e, q, Vec::new(), Dropped::default(), ids));
                }
                Some((fv, old)) => {
                    if old.hash() != s.hash() {
                        let mut e = DiffEntry::new("~", &shr, tag);
                        e.old = Some(old.hash().h6().into());
                        e.new = Some(s.hash().h6().into());
                        e.scope = Some(sref.to_string());
                        let w = word_diff(&old.text, &s.text);
                        e.chars = Some((w.del, w.ins));
                        let mut lines = Vec::new();
                        if detail == Detail::Text {
                            lines.push(guard_line_start(w.marked.clone(), Ctx::Diff));
                            e.detail = vec![plain_marked(&w)];
                        }
                        out.push(finish(e, None, lines, w.dropped, ids));
                    } else if old.node.pos != s.node.pos || fv.node.id != tv.node.id {
                        let mut e = DiffEntry::new(">", &shr, tag);
                        e.new = Some(s.hash().h6().into());
                        e.scope = Some(sref.to_string());
                        out.push(finish(e, None, Vec::new(), Dropped::default(), ids));
                    }
                }
            }
        }
    }
    for fv in &fviews {
        let Some(sref) = idx.ref_of(&fv.node.id) else { continue };
        if touched.contains(&fv.node.id) && find(&tviews, &fv.node.id).is_none() {
            let mut e = DiffEntry::new("-", &sref, "SLIDE");
            e.old = Some(slide_own_hash(fv).h6().into());
            out.push(finish(e, None, Vec::new(), Dropped::default(), vec![(sref.clone(), fv.node.id.clone())]));
        }
        for s in &fv.shapes {
            let gone = !tviews.iter().any(|tv| tv.shapes.iter().any(|x| x.node.id == s.node.id));
            if !touched.contains(&s.node.id) || !gone {
                continue;
            }
            let Some(shr) = idx.ref_of(&s.node.id) else { continue };
            let mut e = DiffEntry::new("-", &shr, s.role.unwrap_or(s.kind));
            e.old = Some(s.hash().h6().into());
            e.scope = Some(sref.to_string());
            out.push(finish(e, None, Vec::new(), Dropped::default(), vec![(shr.clone(), s.node.id.clone())]));
        }
    }
    out
}

/// The tool. `file` describes the `to` state; `from_rev` is the revision of `from`.
pub fn diff(
    file: &FileMeta,
    from_rev: u64,
    from: &DocumentState,
    to: &DocumentState,
    opts: &DiffOptions,
) -> Result<Response<DiffData>, AgentError> {
    if from_rev > file.rev {
        return Err(AgentError::BadRequest { field: "from" });
    }
    let max_bytes = resolve_max_bytes(opts.max_bytes, DIFF_DEFAULT_MAX_BYTES)?;
    let start = match opts.cursor.as_deref() {
        None => 0,
        Some(raw) => {
            let c: Cursor = raw.parse().map_err(|_| AgentError::BadRequest { field: "cursor" })?;
            c.check_rev(file.rev)?;
            if c.aux != Some(from_rev) {
                return Err(AgentError::BadRequest { field: "cursor" });
            }
            usize::try_from(c.pos).map_err(|_| AgentError::BadRequest { field: "cursor" })?
        }
    };
    let ops = workspace_merge::diff(from, to);
    let touched: BTreeSet<NodeId> = ops.iter().filter_map(|op| op.target_node_id().cloned()).collect();
    let idx = RefIndex::from_states(&[from, to]);
    let pending = match file.kind {
        FileKind::Document => diff_documents(&idx, from, to, &touched, opts.detail),
        FileKind::Spreadsheet => diff_sheets(&idx, from, to, &touched, opts.detail),
        FileKind::Presentation => diff_slides(&idx, from, to, &touched, opts.detail),
    };
    let count = |op: &str| pending.iter().filter(|p| p.entry.op == op).count();
    let (changed, added, removed, moved) = (count("~"), count("+"), count("-"), count(">"));
    let total = pending.len();
    let mut lead = vec![
        ("from", from_rev.to_string()),
        ("to", file.rev.to_string()),
        ("chain", file.chain8().to_string()),
        ("blocks", total.to_string()),
        ("changed", changed.to_string()),
        ("added", added.to_string()),
        ("removed", removed.to_string()),
    ];
    if moved > 0 {
        lead.push(("moved", moved.to_string()));
    }
    let (units, entries): (Vec<Unit>, Vec<DiffEntry>) = pending.into_iter().map(|p| (p.unit, p.entry)).unzip();
    let spec = PagedSpec {
        tool: Tool::Diff,
        file: Some(file),
        lead,
        quoted: Vec::new(),
        header_rev: false,
        show_n: false,
        tail: Vec::new(),
        base_warn: Warnings::default(),
        fixed: String::new(),
        fixed_dropped: Dropped::default(),
        cursor_aux: Some(from_rev),
    };
    paged(spec, &units, start, usize::MAX, max_bytes, move |taken, cursor| DiffData {
        from: from_rev,
        to: file.rev,
        blocks: total,
        changed,
        added,
        removed,
        moved,
        more: cursor.map(|c| c.to_string()),
        entries: entries.into_iter().skip(start).take(taken).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_diff_matches_the_spec_example() {
        let w = word_diff(
            "The rules describe how the policy handles complaints in the first week",
            "The rules describe how the procedure handles complaints in the first week",
        );
        assert_eq!(w.marked, "…describe how the [-policy-]{+procedure+} handles complaints in the first…");
        assert_eq!((w.del, w.ins), (6, 9));
    }

    #[test]
    fn contexts_and_elision() {
        let w = word_diff("a b c d e f g h i j k l m n", "A b c d e f g h i j k l m N");
        assert_eq!(w.marked, "[-a-]{+A+} b c d e…");
        let w = word_diff("a b c d e f g h i j k l m n", "a b c d e f g h i j k l m n");
        assert_eq!(w.marked, "a b c d e f g h i j k l m n");
        let w = word_diff("x 1 2 3 4 5 6 7 8 9 10 y", "z 1 2 3 4 5 6 7 8 9 10 w");
        assert_eq!(w.marked, "[-x-]{+z+} 1 2 3 4 … 7 8 9 10 [-y-]{+w+}");
    }

    #[test]
    fn markers_in_text_are_escaped() {
        let w = word_diff("a [-b-] c", "a {+d+} c");
        assert!(!w.marked.contains("[-b"), "{}", w.marked);
        assert!(w.marked.contains("\\[-"), "{}", w.marked);
    }

    #[test]
    fn word_helpers() {
        assert_eq!(after_words("a b  c", 2), 3);
        assert_eq!(&"a b  c"[last_words_start("a b  c", 2)..], "b  c");
        assert_eq!(last_words_start("a b", 5), 0);
    }
}
