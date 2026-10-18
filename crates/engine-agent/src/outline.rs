//! `outline` (protocol spec 6.3, 6.7, 7.3): progressive disclosure. Block refs, types,
//! hashes and short previews for documents; sheets and a column profile for
//! spreadsheets; slide titles for presentations.

use engine_core::materializer::DocumentState;
use serde::Serialize;

use crate::budget::{
    resolve_limit, resolve_max_bytes, start_position, Cursor, Unit, DEFAULT_MAX_BYTES,
    OUTLINE_DEFAULT_LIMIT, OUTLINE_MAX_LIMIT, PREVIEW_DEFAULT, PREVIEW_MAX,
};
use crate::dct::{quote_counted, HeaderBuilder, Tool};
use crate::doc::{doc_blocks, find_heading, section_end};
use crate::error::AgentError;
use crate::escape::Dropped;
use crate::hash::{outline_hash, Hash, ShortHash};
use crate::model::{human_size, FileKind, FileMeta, Tree};
use crate::paging::{paged, PagedSpec};
use crate::refs::{Ref, RefIndex};
use crate::response::{Meta, Response, Warnings};
use crate::sheet::{profile, profile_unit, select_sheet, Profile, SheetView};
use crate::slides::SlideView;
use crate::text::Inline;

#[derive(Debug, Clone, Default)]
pub struct OutlineOptions {
    /// `0` (headings, sheets, slide titles) or `1` (default; every block with a preview).
    pub depth: Option<u8>,
    /// Characters of preview per block (0 to 120, default 48).
    pub preview: Option<usize>,
    /// Heading ref (documents) or sheet name or ref (spreadsheets).
    pub under: Option<String>,
    /// An `oh=` (or sheet `h=`) from an earlier outline: unchanged files return one line.
    pub if_none_match: Option<String>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BlockRow {
    #[serde(rename = "ref")]
    pub r: String,
    pub t: String,
    pub h: String,
    pub w: usize,
    pub p: String,
    /// Depth 0 only: blocks and words under the heading.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub under: Option<(usize, usize)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SheetRow {
    #[serde(rename = "ref")]
    pub r: String,
    pub name: String,
    pub rows: usize,
    pub cols: usize,
    pub h: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShapeRow {
    #[serde(rename = "ref")]
    pub r: String,
    pub kind: String,
    pub role: Option<String>,
    pub h: String,
    pub w: usize,
    pub p: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SlideRow {
    pub pos: usize,
    #[serde(rename = "ref")]
    pub r: String,
    pub layout: String,
    pub title: String,
    pub body: usize,
    pub notes_words: usize,
    pub images: usize,
    pub image_bytes: usize,
    pub hidden: bool,
    pub shapes: Vec<ShapeRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OutlineData {
    pub kind: &'static str,
    pub depth: u8,
    /// Blocks, sheets or slides in the selection (before paging).
    pub count: usize,
    pub words: Option<usize>,
    pub oh: Option<String>,
    pub unchanged: bool,
    pub more: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocks: Option<Vec<BlockRow>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sheets: Option<Vec<SheetRow>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile: Option<Profile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slides: Option<Vec<SlideRow>>,
}

impl OutlineData {
    fn empty(kind: &'static str, depth: u8, count: usize) -> Self {
        Self {
            kind,
            depth,
            count,
            words: None,
            oh: None,
            unchanged: false,
            more: None,
            blocks: None,
            sheets: None,
            profile: None,
            slides: None,
        }
    }
}

struct Common {
    depth: u8,
    preview: usize,
    limit: usize,
    max_bytes: usize,
    start: usize,
}

fn common(file: &FileMeta, opts: &OutlineOptions) -> Result<Common, AgentError> {
    let depth = match opts.depth {
        None | Some(1) => 1,
        Some(0) => 0,
        Some(_) => return Err(AgentError::BadRequest { field: "depth" }),
    };
    let preview = match opts.preview {
        None => PREVIEW_DEFAULT,
        Some(p) if p <= PREVIEW_MAX => p,
        Some(p) => {
            return Err(AgentError::LimitExceeded {
                field: "preview",
                limit: PREVIEW_MAX as u64,
                value: p as u64,
            })
        }
    };
    Ok(Common {
        depth,
        preview,
        limit: resolve_limit(opts.limit, OUTLINE_DEFAULT_LIMIT, OUTLINE_MAX_LIMIT)?,
        max_bytes: resolve_max_bytes(opts.max_bytes, DEFAULT_MAX_BYTES)?,
        start: start_position(opts.cursor.as_deref(), file.rev)?.0,
    })
}

fn parse_inm(opts: &OutlineOptions) -> Result<Option<ShortHash>, AgentError> {
    opts.if_none_match
        .as_deref()
        .map(|s| ShortHash::parse(s).map_err(|_| AgentError::BadRequest { field: "if_none_match" }))
        .transpose()
}

/// `dd/1 outline file=… rev=… chain=… unchanged=1 oh=…`
fn unchanged_response(file: &FileMeta, kind: FileKind, depth: u8, oh: &Hash, max_bytes: usize) -> Response<OutlineData> {
    let h = HeaderBuilder::new(Tool::Outline)
        .kv("file", file.short_id())
        .kv("rev", file.rev)
        .kv("chain", file.chain8())
        .kv("unchanged", 1)
        .kv("oh", oh.h6());
    let text = h.finish_line();
    let mut data = OutlineData::empty(kind.as_str(), depth, 0);
    data.unchanged = true;
    data.oh = Some(oh.h6().to_string());
    let meta = Meta::new(file, text.len(), max_bytes, None, Default::default(), &Warnings::default());
    Response { text, meta, data }
}

/// The tool: dispatch on the file kind.
pub fn outline(
    file: &FileMeta,
    state: &DocumentState,
    opts: &OutlineOptions,
) -> Result<Response<OutlineData>, AgentError> {
    let c = common(file, opts)?;
    let tree = Tree::new(state);
    let idx = RefIndex::from_state(state);
    match file.kind {
        FileKind::Document => outline_document(file, &tree, &idx, opts, &c),
        FileKind::Spreadsheet => outline_sheets(file, &tree, &idx, opts, &c),
        FileKind::Presentation => outline_slides(file, &tree, &idx, opts, &c),
    }
}

fn outline_document(
    file: &FileMeta,
    tree: &Tree,
    idx: &RefIndex,
    opts: &OutlineOptions,
    c: &Common,
) -> Result<Response<OutlineData>, AgentError> {
    let blocks = doc_blocks(tree, idx, false);
    let (from, to) = match &opts.under {
        Some(u) => {
            let i = find_heading(&blocks, idx, "under", u)?;
            (i, section_end(&blocks, i))
        }
        None => (0, blocks.len()),
    };
    let sel = &blocks[from..to];
    let pairs: Vec<(Ref, Hash)> = sel.iter().map(|b| (b.r.clone(), b.hash.clone())).collect();
    let oh = outline_hash(&pairs);
    if parse_inm(opts)?.is_some_and(|s| s.matches(&oh)) {
        return Ok(unchanged_response(file, FileKind::Document, c.depth, &oh, c.max_bytes));
    }
    let words: usize = sel.iter().map(|b| b.text.words()).sum();

    let mut rows = Vec::new();
    let mut units = Vec::new();
    for (k, b) in sel.iter().enumerate() {
        if c.depth == 0 && b.level.is_none() {
            continue;
        }
        let inline = b.text.preview_inline();
        let (preview, dropped) = if c.preview == 0 {
            (String::new(), Dropped::default())
        } else {
            inline.preview(c.preview)
        };
        let under = (c.depth == 0).then(|| {
            let end = section_end(&blocks, from + k);
            let inner = &blocks[from + k + 1..end];
            (inner.len(), inner.iter().map(|x| x.text.words()).sum::<usize>())
        });
        let mut line = format!("{} {} {} {}w", b.r, b.text.label, b.hash.h6(), b.text.words());
        if !preview.is_empty() {
            line.push(' ');
            line.push_str(&preview);
        }
        if let Some((n, w)) = under {
            line.push_str(&format!(" [{n}b {w}w]"));
        }
        line.push('\n');
        rows.push(BlockRow {
            r: b.r.to_string(),
            t: b.text.label.clone(),
            h: b.hash.h6().to_string(),
            w: b.text.words(),
            p: if c.preview == 0 { String::new() } else { inline.preview_raw(c.preview) },
            under,
        });
        units.push(Unit {
            text: line,
            dropped,
            ids: vec![(b.r.clone(), b.node.id.clone())],
        });
    }

    let spec = PagedSpec {
        tool: Tool::Outline,
        file: Some(file),
        lead: vec![
            ("kind", "document".into()),
            ("blocks", sel.len().to_string()),
            ("words", words.to_string()),
            ("oh", oh.h6().to_string()),
            ("depth", c.depth.to_string()),
        ],
        show_n: false,
        tail: Vec::new(),
        base_warn: Warnings::default(),
        fixed: "# ref type hash words preview\n".into(),
        fixed_dropped: Dropped::default(),
        cursor_aux: None,
    };
    let (depth, count, oh6) = (c.depth, sel.len(), oh.h6().to_string());
    let start = c.start;
    paged(spec, &units, start, c.limit, c.max_bytes, move |taken, cursor| {
        let mut d = OutlineData::empty("document", depth, count);
        d.words = Some(words);
        d.oh = Some(oh6);
        d.more = cursor.map(|c| c.to_string());
        d.blocks = Some(rows.into_iter().skip(start).take(taken).collect());
        d
    })
}

fn outline_sheets(
    file: &FileMeta,
    tree: &Tree,
    idx: &RefIndex,
    opts: &OutlineOptions,
    c: &Common,
) -> Result<Response<OutlineData>, AgentError> {
    let sheets = tree.sheets();
    let views: Vec<SheetView> = sheets.iter().map(|s| SheetView::new(tree, s)).collect();
    if c.depth == 1 && !views.is_empty() {
        let node = match &opts.under {
            Some(u) => select_sheet(tree, idx, u, "under")?,
            None => sheets[0],
        };
        let view = views.iter().find(|v| v.node.id == node.id).expect("sheet of this tree");
        return profile_response(file, idx, view, opts, c);
    }
    let mut rows = Vec::new();
    let mut units = Vec::new();
    let mut pairs = Vec::new();
    for v in &views {
        let Some(r) = idx.ref_of(&v.node.id) else { continue };
        let h = v.hash();
        let (name, dropped) = quote_counted(&v.name);
        units.push(Unit {
            text: format!("{r} {name} rows={} cols={} h={}\n", v.rows.len(), v.cols, h.h6()),
            dropped,
            ids: vec![(r.clone(), v.node.id.clone())],
        });
        rows.push(SheetRow {
            r: r.to_string(),
            name: crate::escape::sanitize(&v.name),
            rows: v.rows.len(),
            cols: v.cols,
            h: h.h6().to_string(),
        });
        pairs.push((r, h));
    }
    let oh = outline_hash(&pairs);
    if parse_inm(opts)?.is_some_and(|s| s.matches(&oh)) {
        return Ok(unchanged_response(file, FileKind::Spreadsheet, c.depth, &oh, c.max_bytes));
    }
    let spec = PagedSpec {
        tool: Tool::Outline,
        file: Some(file),
        lead: vec![
            ("kind", "spreadsheet".into()),
            ("sheets", pairs.len().to_string()),
            ("oh", oh.h6().to_string()),
            ("depth", c.depth.to_string()),
        ],
        show_n: false,
        tail: Vec::new(),
        base_warn: Warnings::default(),
        fixed: "# ref name rows cols hash\n".into(),
        fixed_dropped: Dropped::default(),
        cursor_aux: None,
    };
    let (depth, count, oh6, start) = (c.depth, pairs.len(), oh.h6().to_string(), c.start);
    paged(spec, &units, start, c.limit, c.max_bytes, move |taken, cursor| {
        let mut d = OutlineData::empty("spreadsheet", depth, count);
        d.oh = Some(oh6);
        d.more = cursor.map(|c| c.to_string());
        d.sheets = Some(rows.into_iter().skip(start).take(taken).collect());
        d
    })
}

/// The column profile of one sheet as an `outline` response (spec 6.7).
fn profile_response(
    file: &FileMeta,
    idx: &RefIndex,
    view: &SheetView,
    opts: &OutlineOptions,
    c: &Common,
) -> Result<Response<OutlineData>, AgentError> {
    let h = view.hash();
    if parse_inm(opts)?.is_some_and(|s| s.matches(&h)) {
        return Ok(unchanged_response(file, FileKind::Spreadsheet, c.depth, &h, c.max_bytes));
    }
    let depth = c.depth;
    profile_paged(
        Tool::Outline,
        file,
        idx,
        view,
        Some(depth),
        (c.start, c.limit, c.max_bytes),
        move |p, h, cursor| {
            let mut d = OutlineData::empty("spreadsheet", depth, 1);
            d.oh = Some(h.h6().to_string());
            d.more = cursor.map(|c| c.to_string());
            d.profile = Some(p);
            d
        },
    )
}

/// A profile response under `tool`: `outline` at depth 1, or `read` of a sheet ref.
pub(crate) fn profile_paged<T>(
    tool: Tool,
    file: &FileMeta,
    idx: &RefIndex,
    view: &SheetView,
    depth: Option<u8>,
    (start, limit, max_bytes): (usize, usize, usize),
    data: impl FnOnce(Profile, Hash, Option<Cursor>) -> T,
) -> Result<Response<T>, AgentError> {
    let sref = idx
        .ref_of(&view.node.id)
        .ok_or(AgentError::BadRequest { field: "under" })?;
    let h = view.hash();
    let p = profile(view);
    let units: Vec<Unit> = p.columns.iter().map(profile_unit).collect();
    let (name, name_dropped) = quote_counted(&view.name);
    let mut lead = profile_lead(&sref, &p, &h);
    if let Some(d) = depth {
        lead.push(("depth", d.to_string()));
    }
    let spec = PagedSpec {
        tool,
        file: Some(file),
        lead,
        show_n: false,
        tail: Vec::new(),
        base_warn: Warnings::default(),
        fixed: format!("sheet {name}\n# col \"header\" type stats\n"),
        fixed_dropped: name_dropped,
        cursor_aux: None,
    };
    paged(spec, &units, start, limit, max_bytes, move |_, cursor| data(p, h, cursor))
}

/// Header pairs of a profile response.
fn profile_lead(sref: &Ref, p: &Profile, h: &Hash) -> Vec<(&'static str, String)> {
    vec![
        ("kind", "spreadsheet".into()),
        ("s", sref.to_string()),
        ("rows", p.rows.to_string()),
        ("cols", p.cols.to_string()),
        ("header", p.header.clone()),
        ("freeze", format!("{}x{}", p.freeze.0, p.freeze.1)),
        ("merges", p.merges.to_string()),
        ("calc", "none".into()),
        ("h", h.h6().to_string()),
    ]
}

fn outline_slides(
    file: &FileMeta,
    tree: &Tree,
    idx: &RefIndex,
    opts: &OutlineOptions,
    c: &Common,
) -> Result<Response<OutlineData>, AgentError> {
    let slides = tree.slides();
    let views: Vec<SlideView> = slides.iter().map(|s| SlideView::new(tree, s)).collect();
    let mut rows = Vec::new();
    let mut units = Vec::new();
    let mut pairs = Vec::new();
    for (i, v) in views.iter().enumerate() {
        let Some(r) = idx.ref_of(&v.node.id) else { continue };
        let (title_q, mut dropped) = quote_counted(&v.title());
        let (imgs, bytes) = v.images();
        let mut line = format!(
            "s{} {r} {} {title_q} body={}b notes={}w",
            i + 1,
            v.layout,
            v.body_shapes(),
            v.notes_words()
        );
        if imgs > 0 {
            line.push_str(&format!(" img={imgs}({} omitted)", human_size(bytes)));
        }
        if v.hidden {
            line.push_str(" hidden=1");
        }
        line.push('\n');
        let mut shape_rows = Vec::new();
        let mut ids = vec![(r.clone(), v.node.id.clone())];
        if c.depth == 1 {
            for s in &v.shapes {
                let Some(sr) = idx.ref_of(&s.node.id) else { continue };
                ids.push((sr.clone(), s.node.id.clone()));
                let tag = s.role.unwrap_or(s.kind);
                let h = s.hash();
                let inline = Inline::text(s.text.clone());
                let words = s.text.split_whitespace().count();
                match s.kind {
                    "text" => {
                        let (pv, d) = if c.preview == 0 {
                            (String::new(), Dropped::default())
                        } else {
                            inline.preview(c.preview)
                        };
                        dropped.add(d);
                        line.push_str(&format!(" .{sr} {tag} #{} {words}w", h.h6()));
                        if !pv.is_empty() {
                            line.push(' ');
                            line.push_str(&pv);
                        }
                        line.push('\n');
                    }
                    "image" => line.push_str(&format!(" .{sr} image {} omitted\n", human_size(s.src_len))),
                    _ => line.push_str(&format!(" .{sr} shape\n")),
                }
                shape_rows.push(ShapeRow {
                    r: sr.to_string(),
                    kind: s.kind.into(),
                    role: s.role.map(String::from),
                    h: h.h6().to_string(),
                    w: words,
                    p: if s.kind == "text" && c.preview > 0 { inline.preview_raw(c.preview) } else { String::new() },
                });
            }
        }
        rows.push(SlideRow {
            pos: i + 1,
            r: r.to_string(),
            layout: v.layout.into(),
            title: crate::escape::sanitize(&v.title()),
            body: v.body_shapes(),
            notes_words: v.notes_words(),
            images: imgs,
            image_bytes: bytes,
            hidden: v.hidden,
            shapes: shape_rows,
        });
        pairs.push((r, v.hash()));
        units.push(Unit {
            text: line,
            dropped,
            ids,
        });
    }
    let oh = outline_hash(&pairs);
    if parse_inm(opts)?.is_some_and(|s| s.matches(&oh)) {
        return Ok(unchanged_response(file, FileKind::Presentation, c.depth, &oh, c.max_bytes));
    }
    let spec = PagedSpec {
        tool: Tool::Outline,
        file: Some(file),
        lead: vec![
            ("kind", "presentation".into()),
            ("slides", pairs.len().to_string()),
            ("oh", oh.h6().to_string()),
            ("depth", c.depth.to_string()),
        ],
        show_n: false,
        tail: Vec::new(),
        base_warn: Warnings::default(),
        fixed: String::new(),
        fixed_dropped: Dropped::default(),
        cursor_aux: None,
    };
    let (depth, count, oh6, start) = (c.depth, pairs.len(), oh.h6().to_string(), c.start);
    paged(spec, &units, start, c.limit, c.max_bytes, move |taken, cursor| {
        let mut d = OutlineData::empty("presentation", depth, count);
        d.oh = Some(oh6);
        d.more = cursor.map(|c| c.to_string());
        d.slides = Some(rows.into_iter().skip(start).take(taken).collect());
        d
    })
}
