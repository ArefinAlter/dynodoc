//! `read` (protocol spec 6.4, 6.6 to 6.8, 7.4): the only tool that returns document text.
//!
//! Exactly one selector: `refs`, `section`, `range` or `slides`. Documents take refs
//! (blocks, `ref.N` list items, `ref.rRcC` table cells) or a section; spreadsheets take a
//! range, a sheet ref (profile) or row refs; presentations take slides or slide refs and
//! `slideRef.shapeRef`. Unchanged blocks the caller already holds (`have`) come back as
//! one `@ref = #hash` line. Text is escaped by [`crate::escape`] and the header carries
//! `trust=data`.

use std::collections::{BTreeMap, BTreeSet};
use std::str::FromStr;

use engine_core::materializer::DocumentState;
use engine_shared::NodeId;
use serde::Serialize;

use crate::a1::{column_name, A1Range, CellRef, MAX_RANGE_CELLS};
use crate::budget::{
    resolve_max_bytes, start_position, Unit, DEFAULT_MAX_BYTES, MAX_REFS, MAX_SLIDES,
};
use crate::dct::{quote_counted, Tool};
use crate::doc::{doc_blocks, find_heading, section_end, DocBlock};
use crate::error::AgentError;
use crate::escape::{guard_line_start, sanitize, Ctx, Dropped};
use crate::hash::{notes_hash, range_hash, row_hash, ShortHash};
use crate::model::{human_size, FileKind, FileMeta, Tree};
use crate::outline::profile_paged;
use crate::paging::{paged, PagedSpec};
use crate::refs::{Address, Ref, RefIndex, ResolveError, SlideSel, Tail};
use crate::response::{Response, Warnings};
use crate::sheet::{select_sheet, SheetView};
use crate::slides::{select_slide, ShapeView, SlideView};
use crate::text::{body_lines, Body, Fmt, Inline};

#[derive(Debug, Clone, Default)]
pub struct ReadOptions {
    /// Block refs, sub-addresses or full ULIDs (at most 100, answered in request order).
    pub refs: Vec<String>,
    /// Heading ref: the heading and every block to the next heading of the same or a
    /// higher level.
    pub section: Option<String>,
    /// A1 range with a sheet name or ref, such as `Budget!A1:F40`.
    pub range: Option<String>,
    /// Slide positions (`s2`) or refs (at most 50).
    pub slides: Vec<String>,
    pub fmt: Fmt,
    /// `ref -> hash` the client already holds.
    pub have: BTreeMap<String, String>,
    /// `rows=refs`: a ref column in range reads.
    pub row_refs: bool,
    /// Nodes a data-loss rule withholds (the caller decides; spec `redacted_block`).
    pub redacted: BTreeSet<NodeId>,
    pub max_bytes: Option<usize>,
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReadItem {
    #[serde(rename = "ref")]
    pub r: String,
    pub t: String,
    pub h: Option<String>,
    /// The client's `have` hash matched: no text was sent.
    pub equal: bool,
    pub redacted: bool,
    /// The `{m=1,fn=1}` braces, or empty.
    pub features: String,
    /// Sanitised plain lines (a text block: one; a list: one per item; a table: one per
    /// row with TAB between cells).
    pub lines: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShapeRead {
    #[serde(rename = "ref")]
    pub r: String,
    pub kind: String,
    pub role: Option<String>,
    pub h: Option<String>,
    pub equal: bool,
    pub redacted: bool,
    pub text: String,
    pub alt: Option<String>,
    pub omitted_bytes: usize,
    pub bullets: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SlideRead {
    pub pos: usize,
    #[serde(rename = "ref")]
    pub r: String,
    pub layout: String,
    pub hidden: bool,
    pub redacted: bool,
    pub shapes: Vec<ShapeRead>,
    pub notes: Option<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RowRead {
    pub row: u32,
    #[serde(rename = "ref", skip_serializing_if = "Option::is_none")]
    pub r: Option<String>,
    pub cells: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RangeRead {
    /// Ref of the sheet.
    pub sheet: String,
    /// Effective range (clamped to the used cells), as `REF!A1:E4`.
    pub range: String,
    pub h: String,
    pub calc: &'static str,
    pub columns: Vec<String>,
    pub rows: Vec<RowRead>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReadData {
    pub n: usize,
    pub more: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<ReadItem>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub slides: Vec<SlideRead>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<RangeRead>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<crate::sheet::Profile>,
}

impl ReadData {
    fn new(n: usize, more: Option<String>) -> Self {
        Self {
            n,
            more,
            items: Vec::new(),
            slides: Vec::new(),
            range: None,
            profile: None,
        }
    }
}

struct Ctl {
    max_bytes: usize,
    start: usize,
    have: BTreeMap<String, ShortHash>,
}

fn parse_have(have: &BTreeMap<String, String>) -> Result<BTreeMap<String, ShortHash>, AgentError> {
    have.iter()
        .map(|(k, v)| {
            let key = Address::from_str(k).map_err(|_| AgentError::BadRequest { field: "have" })?;
            let hash = ShortHash::parse(v).map_err(|_| AgentError::BadRequest { field: "have" })?;
            Ok((key.to_string(), hash))
        })
        .collect()
}

/// The tool.
pub fn read(
    file: &FileMeta,
    state: &DocumentState,
    opts: &ReadOptions,
) -> Result<Response<ReadData>, AgentError> {
    let chosen = [
        !opts.refs.is_empty(),
        opts.section.is_some(),
        opts.range.is_some(),
        !opts.slides.is_empty(),
    ];
    if chosen.iter().filter(|c| **c).count() != 1 {
        return Err(AgentError::BadRequest { field: "selector" });
    }
    let ctl = Ctl {
        max_bytes: resolve_max_bytes(opts.max_bytes, DEFAULT_MAX_BYTES)?,
        start: start_position(opts.cursor.as_deref(), file.rev)?.0,
        have: parse_have(&opts.have)?,
    };
    let tree = Tree::new(state);
    let idx = RefIndex::from_state(state);
    match (file.kind, chosen) {
        (FileKind::Document, [true, _, _, _]) | (FileKind::Document, [_, true, _, _]) => {
            read_document(file, &tree, &idx, opts, &ctl)
        }
        (FileKind::Spreadsheet, [_, _, true, _]) => read_range(file, &tree, &idx, opts, &ctl),
        (FileKind::Spreadsheet, [true, _, _, _]) => read_sheet_refs(file, &tree, &idx, opts, &ctl),
        (FileKind::Presentation, [true, _, _, _]) | (FileKind::Presentation, [_, _, _, true]) => {
            read_slides(file, &tree, &idx, opts, &ctl)
        }
        _ => Err(AgentError::BadRequest {
            field: if chosen[1] {
                "section"
            } else if chosen[2] {
                "range"
            } else if chosen[3] {
                "slides"
            } else {
                "refs"
            },
        }),
    }
}

fn raw_lines(b: &DocBlock) -> Vec<String> {
    match &b.text.body {
        Body::Text(i) => vec![sanitize(&i.plain())],
        Body::List(items) => items.iter().map(|i| sanitize(&i.text.plain())).collect(),
        Body::Table(rows) => rows
            .iter()
            .map(|r| {
                r.iter()
                    .map(|c| sanitize(&c.plain()))
                    .collect::<Vec<_>>()
                    .join("\t")
            })
            .collect(),
    }
}

fn redacted_unit(label: &str, r: &Ref, id: &NodeId) -> (Unit, ReadItem) {
    (
        Unit {
            text: format!("@{label} X redacted\n"),
            dropped: Dropped::default(),
            ids: vec![(r.clone(), id.clone())],
        },
        ReadItem {
            r: label.to_string(),
            t: "X".into(),
            h: None,
            equal: false,
            redacted: true,
            features: String::new(),
            lines: Vec::new(),
        },
    )
}

fn block_unit(b: &DocBlock, label: &str, ctl: &Ctl, opts: &ReadOptions) -> (Unit, ReadItem) {
    if opts.redacted.contains(&b.node.id) {
        return redacted_unit(label, &b.r, &b.node.id);
    }
    let ids = vec![(b.r.clone(), b.node.id.clone())];
    let h6 = b.hash.h6().to_string();
    if ctl.have.get(label).is_some_and(|s| s.matches(&b.hash)) {
        return (
            Unit {
                text: format!("@{label} = #{h6}\n"),
                dropped: Dropped::default(),
                ids,
            },
            ReadItem {
                r: label.to_string(),
                t: b.text.label.clone(),
                h: Some(h6),
                equal: true,
                redacted: false,
                features: String::new(),
                lines: Vec::new(),
            },
        );
    }
    let features = b.text.features.render();
    let mut text = format!("@{label} {} #{h6}", b.text.label);
    if !features.is_empty() {
        text.push(' ');
        text.push_str(&features);
    }
    text.push('\n');
    let (lines, dropped) = body_lines(&b.text, opts.fmt == Fmt::Md, Ctx::Block);
    for l in &lines {
        text.push_str(l);
        text.push('\n');
    }
    (
        Unit { text, dropped, ids },
        ReadItem {
            r: label.to_string(),
            t: b.text.label.clone(),
            h: Some(h6),
            equal: false,
            redacted: false,
            features,
            lines: raw_lines(b),
        },
    )
}

/// `@ref.N` / `@ref.rRcC`: the text of one list item or table cell, against the hash of
/// the whole block (a sub-address is valid only against the block's `h`, spec 5.3).
fn sub_unit(
    b: &DocBlock,
    label: &str,
    tail: &Tail,
    ctl: &Ctl,
    opts: &ReadOptions,
) -> Result<(Unit, ReadItem), AgentError> {
    if opts.redacted.contains(&b.node.id) {
        return Ok(redacted_unit(label, &b.r, &b.node.id));
    }
    let unknown = || AgentError::UnknownRef {
        refs: vec![b.r.clone()],
    };
    let inline: &Inline = match (&b.text.body, tail) {
        (Body::List(items), Tail::Item(n)) => &items.get(*n as usize - 1).ok_or_else(unknown)?.text,
        (Body::Table(rows), Tail::Cell { row, col }) => rows
            .get(*row as usize - 1)
            .and_then(|r| r.get(*col as usize - 1))
            .ok_or_else(unknown)?,
        _ => return Err(AgentError::UnsupportedStructure),
    };
    let ids = vec![(b.r.clone(), b.node.id.clone())];
    let h6 = b.hash.h6().to_string();
    if ctl.have.get(label).is_some_and(|s| s.matches(&b.hash)) {
        return Ok((
            Unit {
                text: format!("@{label} = #{h6}\n"),
                dropped: Dropped::default(),
                ids,
            },
            ReadItem {
                r: label.to_string(),
                t: "P".into(),
                h: Some(h6),
                equal: true,
                redacted: false,
                features: String::new(),
                lines: Vec::new(),
            },
        ));
    }
    let (line, dropped) = inline.render_line(opts.fmt == Fmt::Md, Ctx::Block);
    Ok((
        Unit {
            text: format!("@{label} P #{h6}\n{line}\n"),
            dropped,
            ids,
        },
        ReadItem {
            r: label.to_string(),
            t: "P".into(),
            h: Some(h6),
            equal: false,
            redacted: false,
            features: String::new(),
            lines: vec![sanitize(&inline.plain())],
        },
    ))
}

fn read_document(
    file: &FileMeta,
    tree: &Tree,
    idx: &RefIndex,
    opts: &ReadOptions,
    ctl: &Ctl,
) -> Result<Response<ReadData>, AgentError> {
    let blocks = doc_blocks(tree, idx, opts.fmt == Fmt::Md);
    let by_id: BTreeMap<&NodeId, usize> = blocks
        .iter()
        .enumerate()
        .map(|(i, b)| (&b.node.id, i))
        .collect();
    let mut sel: Vec<(Address, usize)> = Vec::new();
    if let Some(s) = &opts.section {
        let i = find_heading(&blocks, idx, "section", s)?;
        let end = section_end(&blocks, i);
        for (k, b) in blocks.iter().enumerate().take(end).skip(i) {
            sel.push((Address::Block(b.r.clone()), k));
        }
    } else {
        if opts.refs.len() > MAX_REFS {
            return Err(AgentError::LimitExceeded {
                field: "refs",
                limit: MAX_REFS as u64,
                value: opts.refs.len() as u64,
            });
        }
        let (mut unknown, mut ambiguous, mut gone) = (Vec::new(), Vec::new(), Vec::new());
        for raw in &opts.refs {
            let addr =
                Address::from_str(raw).map_err(|_| AgentError::BadRequest { field: "refs" })?;
            if matches!(addr, Address::SheetCell { .. }) {
                return Err(AgentError::BadRequest { field: "refs" });
            }
            match idx.resolve(addr.base()) {
                Ok(id) => match by_id.get(&id) {
                    Some(&i) => sel.push((addr, i)),
                    None if tree.state.nodes.get(&id).is_some_and(|n| n.deleted) => {
                        gone.push(addr.base().clone());
                    }
                    None => return Err(AgentError::BadRequest { field: "refs" }),
                },
                Err(ResolveError::Unknown) => unknown.push(addr.base().clone()),
                Err(ResolveError::Ambiguous(c)) => ambiguous.extend(c),
            }
        }
        if !unknown.is_empty() {
            return Err(AgentError::UnknownRef { refs: unknown });
        }
        if !ambiguous.is_empty() {
            return Err(AgentError::AmbiguousRef { refs: ambiguous });
        }
        if !gone.is_empty() {
            return Err(AgentError::NotFound { refs: gone });
        }
    }
    let mut units = Vec::with_capacity(sel.len());
    let mut items = Vec::with_capacity(sel.len());
    for (addr, i) in &sel {
        let label = addr.to_string();
        let (u, it) = match addr {
            Address::Sub { tail, .. } => sub_unit(&blocks[*i], &label, tail, ctl, opts)?,
            _ => block_unit(&blocks[*i], &label, ctl, opts),
        };
        units.push(u);
        items.push(it);
    }
    let withheld = items.iter().filter(|i| i.redacted).count();
    if !items.is_empty() && withheld == items.len() {
        return Err(AgentError::RedactedBlock {
            refs: sel.iter().map(|(a, _)| a.base().clone()).collect(),
        });
    }
    let spec = PagedSpec {
        tool: Tool::Read,
        file: Some(file),
        lead: vec![("fmt", opts.fmt.as_str().into())],
        quoted: Vec::new(),
        header_rev: true,
        show_n: true,
        tail: Vec::new(),
        base_warn: Warnings {
            redacted: withheld > 0,
            ..Warnings::default()
        },
        fixed: String::new(),
        fixed_dropped: Dropped::default(),
        cursor_aux: None,
    };
    let start = ctl.start;
    paged(
        spec,
        &units,
        start,
        usize::MAX,
        ctl.max_bytes,
        move |taken, cursor| {
            let mut d = ReadData::new(taken, cursor.map(|c| c.to_string()));
            d.items = items.into_iter().skip(start).take(taken).collect();
            d
        },
    )
}

fn sheet_ref_error(r: &Ref, e: ResolveError) -> AgentError {
    match e {
        ResolveError::Unknown => AgentError::UnknownRef {
            refs: vec![r.clone()],
        },
        ResolveError::Ambiguous(c) => AgentError::AmbiguousRef { refs: c },
    }
}

/// `refs` of a spreadsheet: one sheet ref (profile) or row refs (row text).
fn read_sheet_refs(
    file: &FileMeta,
    tree: &Tree,
    idx: &RefIndex,
    opts: &ReadOptions,
    ctl: &Ctl,
) -> Result<Response<ReadData>, AgentError> {
    if opts.refs.len() > MAX_REFS {
        return Err(AgentError::LimitExceeded {
            field: "refs",
            limit: MAX_REFS as u64,
            value: opts.refs.len() as u64,
        });
    }
    let sheets = tree.sheets();
    let mut resolved: Vec<(Ref, NodeId)> = Vec::new();
    for raw in &opts.refs {
        let r = Ref::parse(raw).map_err(|_| AgentError::BadRequest { field: "refs" })?;
        let id = idx.resolve(&r).map_err(|e| sheet_ref_error(&r, e))?;
        resolved.push((r, id));
    }
    if let Some((_, id)) = resolved
        .iter()
        .find(|(_, id)| sheets.iter().any(|s| &s.id == id))
    {
        if resolved.len() != 1 {
            return Err(AgentError::BadRequest { field: "refs" });
        }
        let node = sheets.iter().find(|s| &s.id == id).expect("found above");
        let view = SheetView::new(tree, node);
        return profile_paged(
            Tool::Read,
            file,
            idx,
            &view,
            None,
            (ctl.start, usize::MAX, ctl.max_bytes),
            |p, _, cursor| {
                let mut d = ReadData::new(p.columns.len(), cursor.map(|c| c.to_string()));
                d.profile = Some(p);
                d
            },
        );
    }
    let mut units = Vec::new();
    let mut items = Vec::new();
    for (r, id) in &resolved {
        let node = tree.state.nodes.get(id).filter(|n| !n.deleted);
        let Some(node) = node else {
            return Err(AgentError::NotFound {
                refs: vec![r.clone()],
            });
        };
        let sheet = node
            .parent_id
            .as_ref()
            .and_then(|p| sheets.iter().find(|s| &s.id == p))
            .ok_or(AgentError::BadRequest { field: "refs" })?;
        let view = SheetView::new(tree, sheet);
        let pos = view
            .rows
            .iter()
            .position(|n| n.id == node.id)
            .ok_or(AgentError::BadRequest { field: "refs" })?;
        let cells = view.row_cells(pos);
        let h = row_hash(&cells);
        let label = r.to_string();
        if opts.redacted.contains(id) {
            let (u, it) = redacted_unit(&label, r, id);
            units.push(u);
            items.push(it);
            continue;
        }
        let ids = vec![(r.clone(), id.clone())];
        if ctl.have.get(&label).is_some_and(|s| s.matches(&h)) {
            units.push(Unit {
                text: format!("@{label} = #{}\n", h.h6()),
                dropped: Dropped::default(),
                ids,
            });
            items.push(ReadItem {
                r: label,
                t: "ROW".into(),
                h: Some(h.h6().to_string()),
                equal: true,
                redacted: false,
                features: String::new(),
                lines: Vec::new(),
            });
            continue;
        }
        let mut dropped = Dropped::default();
        let escaped: Vec<String> = cells
            .iter()
            .map(|c| {
                let (e, d) = crate::escape::escape(c);
                dropped.add(d);
                e
            })
            .collect();
        units.push(Unit {
            text: format!("@{label} ROW #{}\n{}\n", h.h6(), escaped.join("\t")),
            dropped,
            ids,
        });
        items.push(ReadItem {
            r: label,
            t: "ROW".into(),
            h: Some(h.h6().to_string()),
            equal: false,
            redacted: false,
            features: String::new(),
            lines: vec![cells
                .iter()
                .map(|c| sanitize(c))
                .collect::<Vec<_>>()
                .join("\t")],
        });
    }
    let spec = PagedSpec {
        tool: Tool::Read,
        file: Some(file),
        lead: vec![("kind", "spreadsheet".into())],
        quoted: Vec::new(),
        header_rev: true,
        show_n: true,
        tail: Vec::new(),
        base_warn: Warnings {
            redacted: items.iter().any(|i| i.redacted),
            ..Warnings::default()
        },
        fixed: String::new(),
        fixed_dropped: Dropped::default(),
        cursor_aux: None,
    };
    let start = ctl.start;
    paged(
        spec,
        &units,
        start,
        usize::MAX,
        ctl.max_bytes,
        move |taken, cursor| {
            let mut d = ReadData::new(taken, cursor.map(|c| c.to_string()));
            d.items = items.into_iter().skip(start).take(taken).collect();
            d
        },
    )
}

fn read_range(
    file: &FileMeta,
    tree: &Tree,
    idx: &RefIndex,
    opts: &ReadOptions,
    ctl: &Ctl,
) -> Result<Response<ReadData>, AgentError> {
    let a1: A1Range = opts
        .range
        .as_deref()
        .unwrap_or("")
        .parse()
        .map_err(|_| AgentError::BadRequest { field: "range" })?;
    let node = match &a1.sheet {
        Some(q) => select_sheet(tree, idx, q, "range")?,
        None => match tree.sheets().as_slice() {
            [one] => one,
            _ => return Err(AgentError::BadRequest { field: "range" }),
        },
    };
    let sref = idx
        .ref_of(&node.id)
        .ok_or(AgentError::BadRequest { field: "range" })?;
    let view = SheetView::new(tree, node);
    // Clamp to the used cells: the answer for cells nobody wrote is "empty", and
    // spelling them out would only spend the budget.
    let last_row = (a1.to.row as usize).min(view.rows.len());
    let last_col = (usize::from(a1.to.col) + 1).min(view.cols);
    let first_row = a1.from.row as usize;
    let first_col = usize::from(a1.from.col);
    let (rows, cols) = if first_row > last_row || first_col >= last_col {
        (0, 0)
    } else {
        (last_row - first_row + 1, last_col - first_col)
    };
    if (rows as u64) * (cols as u64) > MAX_RANGE_CELLS {
        return Err(AgentError::LimitExceeded {
            field: "range",
            limit: MAX_RANGE_CELLS,
            value: (rows as u64) * (cols as u64),
        });
    }
    let grid: Vec<Vec<String>> = (0..rows)
        .map(|r| {
            (0..cols)
                .map(|c| view.cell(first_row - 1 + r, first_col + c))
                .collect()
        })
        .collect();
    let h = range_hash(&grid);
    let effective = if rows == 0 {
        A1Range {
            sheet: Some(sref.to_string()),
            from: a1.from,
            to: a1.to,
        }
    } else {
        A1Range {
            sheet: Some(sref.to_string()),
            from: CellRef {
                col: first_col as u16,
                row: first_row as u32,
            },
            to: CellRef {
                col: (first_col + cols - 1) as u16,
                row: last_row as u32,
            },
        }
    };
    let letters: Vec<String> = (0..cols)
        .map(|c| column_name((first_col + c) as u16))
        .collect();
    let mut units = Vec::with_capacity(rows);
    let mut typed = Vec::with_capacity(rows);
    for (r, row) in grid.iter().enumerate() {
        let row_no = (first_row + r) as u32;
        let node_id = view.rows[first_row - 1 + r].id.clone();
        let mut line = row_no.to_string();
        let mut dropped = Dropped::default();
        let mut ids = Vec::new();
        let rref = if opts.row_refs {
            idx.ref_of(&node_id)
        } else {
            None
        };
        if opts.row_refs {
            line.push('\t');
            if let Some(rr) = &rref {
                line.push_str(rr.as_str());
                ids.push((rr.clone(), node_id.clone()));
            }
        }
        for cell in row {
            let (e, d) = crate::escape::escape(cell);
            dropped.add(d);
            line.push('\t');
            line.push_str(&e);
        }
        line.push('\n');
        units.push(Unit {
            text: line,
            dropped,
            ids,
        });
        typed.push(RowRead {
            row: row_no,
            r: rref.map(|r| r.to_string()),
            cells: row.iter().map(|c| sanitize(c)).collect(),
        });
    }
    let mut head = String::new();
    if opts.row_refs {
        head.push('\t');
    }
    for l in &letters {
        head.push('\t');
        head.push_str(l);
    }
    head.push('\n');
    let mut lead = vec![
        ("range", effective.to_string()),
        ("h", h.h6().to_string()),
        ("calc", "none".to_string()),
    ];
    if opts.row_refs {
        lead.push(("rows", "refs".into()));
    }
    let spec = PagedSpec {
        tool: Tool::Read,
        file: Some(file),
        lead,
        quoted: Vec::new(),
        header_rev: true,
        show_n: false,
        tail: Vec::new(),
        base_warn: Warnings::default(),
        fixed: head,
        fixed_dropped: Dropped::default(),
        cursor_aux: None,
    };
    let start = ctl.start;
    let range_text = effective.to_string();
    let (sheet, h6) = (sref.to_string(), h.h6().to_string());
    paged(
        spec,
        &units,
        start,
        usize::MAX,
        ctl.max_bytes,
        move |taken, cursor| {
            let mut d = ReadData::new(taken, cursor.map(|c| c.to_string()));
            d.range = Some(RangeRead {
                sheet,
                range: range_text,
                h: h6,
                calc: "none",
                columns: letters,
                rows: typed.into_iter().skip(start).take(taken).collect(),
            });
            d
        },
    )
}

fn shape_tag(s: &ShapeView) -> &'static str {
    s.role.unwrap_or(s.kind)
}

fn shape_lines(s: &ShapeView, text: &mut String, dropped: &mut Dropped) {
    for line in s.text.split('\n') {
        let (e, d) = Inline::text(line).render(false);
        dropped.add(d);
        if s.bullets {
            text.push_str(&format!(" - {e}\n"));
        } else {
            text.push_str(&format!(" {}\n", guard_line_start(e, Ctx::Slide)));
        }
    }
}

/// One slide, or only `only` of its shapes.
#[allow(clippy::too_many_arguments)]
fn slide_unit(
    idx: &RefIndex,
    pos: usize,
    v: &SlideView,
    only: Option<&NodeId>,
    ctl: &Ctl,
    opts: &ReadOptions,
) -> Result<(Unit, SlideRead), AgentError> {
    let sref = idx
        .ref_of(&v.node.id)
        .ok_or(AgentError::BadRequest { field: "slides" })?;
    let mut ids = vec![(sref.clone(), v.node.id.clone())];
    let mut dropped = Dropped::default();
    let mut typed = SlideRead {
        pos,
        r: sref.to_string(),
        layout: v.layout.into(),
        hidden: v.hidden,
        redacted: false,
        shapes: Vec::new(),
        notes: None,
    };
    if opts.redacted.contains(&v.node.id) {
        typed.redacted = true;
        return Ok((
            Unit {
                text: format!("@s{pos} {sref} X redacted\n"),
                dropped,
                ids,
            },
            typed,
        ));
    }
    let mut text = format!(
        "@s{pos} {sref} {} hidden={}\n",
        v.layout,
        u8::from(v.hidden)
    );
    for s in &v.shapes {
        if only.is_some_and(|o| o != &s.node.id) {
            continue;
        }
        let sr = idx
            .ref_of(&s.node.id)
            .ok_or(AgentError::BadRequest { field: "slides" })?;
        ids.push((sr.clone(), s.node.id.clone()));
        let key = format!("{sref}.{sr}");
        let h = s.hash();
        let mut shape = ShapeRead {
            r: sr.to_string(),
            kind: s.kind.into(),
            role: s.role.map(String::from),
            h: None,
            equal: false,
            redacted: false,
            text: String::new(),
            alt: None,
            omitted_bytes: s.src_len,
            bullets: s.bullets,
        };
        if opts.redacted.contains(&s.node.id) {
            text.push_str(&format!(" .{sr} X redacted\n"));
            shape.redacted = true;
            typed.shapes.push(shape);
            continue;
        }
        match s.kind {
            "text" => {
                shape.h = Some(h.h6().to_string());
                if ctl.have.get(&key).is_some_and(|x| x.matches(&h)) {
                    text.push_str(&format!(" .{sr} = #{}\n", h.h6()));
                    shape.equal = true;
                } else {
                    text.push_str(&format!(" .{sr} {} #{}\n", shape_tag(s), h.h6()));
                    shape_lines(s, &mut text, &mut dropped);
                    shape.text = sanitize(&s.text);
                }
            }
            kind => {
                text.push_str(&format!(" .{sr} {kind}"));
                if let Some(alt) = s.alt.as_deref().filter(|a| !a.is_empty()) {
                    let (q, d) = quote_counted(alt);
                    dropped.add(d);
                    text.push_str(&format!(" alt={q}"));
                    shape.alt = Some(sanitize(alt));
                }
                if kind == "image" {
                    text.push_str(&format!(" {} omitted", human_size(s.src_len)));
                }
                text.push('\n');
            }
        }
        typed.shapes.push(shape);
    }
    if only.is_none() && !v.notes.is_empty() {
        let nh = notes_hash(&v.notes);
        let (e, d) = Inline::text(v.notes.clone()).render(false);
        dropped.add(d);
        text.push_str(&format!("notes #{}: {e}\n", nh.h6()));
        typed.notes = Some((nh.h6().to_string(), sanitize(&v.notes)));
    }
    Ok((Unit { text, dropped, ids }, typed))
}

fn read_slides(
    file: &FileMeta,
    tree: &Tree,
    idx: &RefIndex,
    opts: &ReadOptions,
    ctl: &Ctl,
) -> Result<Response<ReadData>, AgentError> {
    let raw: &[String] = if opts.slides.is_empty() {
        &opts.refs
    } else {
        &opts.slides
    };
    let field = if opts.slides.is_empty() {
        "refs"
    } else {
        "slides"
    };
    let cap = if opts.slides.is_empty() {
        MAX_REFS
    } else {
        MAX_SLIDES
    };
    if raw.len() > cap {
        return Err(AgentError::LimitExceeded {
            field,
            limit: cap as u64,
            value: raw.len() as u64,
        });
    }
    let mut units = Vec::new();
    let mut typed = Vec::new();
    let mut labels = Vec::new();
    for s in raw {
        let (pos, node, only) = if opts.slides.is_empty() {
            match Address::from_str(s).map_err(|_| AgentError::BadRequest { field })? {
                Address::Block(r) => {
                    let (i, n) = select_slide(tree, idx, &SlideSel::Ref(r))?;
                    (i, n, None)
                }
                Address::Sub { base, tail } => {
                    // A tail spelled like `R1C1` parses as a table cell; here it is a shape ref.
                    let shape = tail.as_ref_text().ok_or(AgentError::UnsupportedStructure)?;
                    let (i, n) = select_slide(tree, idx, &SlideSel::Ref(base))?;
                    let sid = idx
                        .resolve(&shape)
                        .map_err(|e| sheet_ref_error(&shape, e))?;
                    (i, n, Some((shape, sid)))
                }
                Address::SheetCell { .. } => return Err(AgentError::BadRequest { field }),
            }
        } else {
            let sel: SlideSel = s.parse().map_err(|_| AgentError::BadRequest { field })?;
            let (i, n) = select_slide(tree, idx, &sel)?;
            (i, n, None)
        };
        let view = SlideView::new(tree, node);
        if let Some((shape, sid)) = &only {
            if !view.shapes.iter().any(|x| &x.node.id == sid) {
                return Err(AgentError::UnknownRef {
                    refs: vec![shape.clone()],
                });
            }
        }
        let (u, t) = slide_unit(
            idx,
            pos + 1,
            &view,
            only.as_ref().map(|(_, id)| id),
            ctl,
            opts,
        )?;
        labels.push(format!("s{}", pos + 1));
        units.push(u);
        typed.push(t);
    }
    let withheld = typed.iter().filter(|t| t.redacted).count();
    if !typed.is_empty() && withheld == typed.len() {
        return Err(AgentError::RedactedBlock {
            refs: typed.iter().filter_map(|t| Ref::parse(&t.r).ok()).collect(),
        });
    }
    labels.dedup();
    let spec = PagedSpec {
        tool: Tool::Read,
        file: Some(file),
        lead: vec![("slides", labels.join(","))],
        quoted: Vec::new(),
        header_rev: true,
        show_n: true,
        tail: Vec::new(),
        base_warn: Warnings {
            redacted: withheld > 0,
            ..Warnings::default()
        },
        fixed: String::new(),
        fixed_dropped: Dropped::default(),
        cursor_aux: None,
    };
    let start = ctl.start;
    paged(
        spec,
        &units,
        start,
        usize::MAX,
        ctl.max_bytes,
        move |taken, cursor| {
            let mut d = ReadData::new(taken, cursor.map(|c| c.to_string()));
            d.slides = typed.into_iter().skip(start).take(taken).collect();
            d
        },
    )
}
