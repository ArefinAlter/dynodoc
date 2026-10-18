//! `find` (protocol spec 7.5): literal or regular-expression search inside one file,
//! answering with refs or cells and short snippets rather than whole blocks.
//!
//! Matching uses the `regex` crate, whose engine runs in time linear in the text (no
//! backtracking); look-around and back-references do not exist in it, and a pattern the
//! engine cannot bound within its size limits is rejected with `bad_request field=q`.
//! One hit is reported per match, in document order; the hits of one block follow each
//! other, so their order is the `nth` of a later `replace`. Text is matched in its plain
//! form (inline atoms contribute what `plain_text` gives).

use engine_core::materializer::DocumentState;
use regex::{Regex, RegexBuilder};
use serde::Serialize;

use crate::a1::CellRef;
use crate::budget::{
    resolve_limit, resolve_max_bytes, start_position, Unit, CONTEXT_DEFAULT, CONTEXT_MAX,
    DEFAULT_MAX_BYTES, FIND_DEFAULT_LIMIT, FIND_MAX_LIMIT, MAX_QUERY_CHARS,
};
use crate::dct::Tool;
use crate::doc::doc_blocks;
use crate::error::AgentError;
use crate::escape::{escape, sanitize, Dropped};
use crate::model::{field_string, FileKind, FileMeta, Tree};
use crate::paging::{paged, PagedSpec};
use crate::refs::{Ref, RefIndex};
use crate::response::{Response, Warnings};
use crate::sheet::SheetView;
use crate::slides::SlideView;
use crate::text::Body;

/// Scan at most this many hits; `total` is then a lower bound.
pub const MAX_SCANNED_HITS: usize = 10_000;
/// Characters of the match itself kept in a snippet.
const MATCH_CHARS: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    /// Block text, cell values (not formulas), slide shape text.
    #[default]
    Text,
    /// Formula strings (spreadsheets only).
    Formulas,
    /// Cell notes (spreadsheets) or slide notes (presentations).
    Notes,
}

impl Scope {
    pub fn parse(s: &str) -> Option<Scope> {
        match s {
            "text" => Some(Scope::Text),
            "formulas" => Some(Scope::Formulas),
            "notes" => Some(Scope::Notes),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct FindOptions {
    pub q: String,
    pub regex: bool,
    /// Case-sensitive (default: insensitive).
    pub case: bool,
    pub word: bool,
    pub scope: Scope,
    pub context: Option<usize>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hit {
    /// `7C02`, `7C02.2`, `3B77.r2c1`, `R91W!C14` or `W2SK.A1B2`.
    pub at: String,
    /// Sanitised snippet, `…` marking cut sides.
    pub snippet: String,
    /// Character offset of the match in the searched text.
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FindData {
    pub n: usize,
    pub total: usize,
    /// `total` stopped at [`MAX_SCANNED_HITS`].
    pub total_capped: bool,
    pub more: Option<String>,
    pub hits: Vec<Hit>,
}

struct Target {
    at: String,
    prefix: &'static str,
    /// Raw text from `targets`, sanitised before searching.
    text: String,
    id: (Ref, engine_shared::NodeId),
}

fn raw(s: &str) -> String {
    s.to_string()
}

fn build_regex(o: &FindOptions) -> Result<Regex, AgentError> {
    let base = if o.regex {
        o.q.clone()
    } else {
        regex::escape(&o.q)
    };
    let pattern = if o.word {
        format!(r"\b(?:{base})\b")
    } else {
        base
    };
    RegexBuilder::new(&pattern)
        .case_insensitive(!o.case)
        .size_limit(1 << 20)
        .dfa_size_limit(1 << 20)
        .nest_limit(64)
        .build()
        .map_err(|_| AgentError::BadRequest { field: "q" })
}

fn targets(
    file: &FileMeta,
    tree: &Tree,
    idx: &RefIndex,
    scope: Scope,
) -> Result<Vec<Target>, AgentError> {
    let mut out = Vec::new();
    match (file.kind, scope) {
        (FileKind::Document, Scope::Text) => {
            for b in doc_blocks(tree, idx, false) {
                let id = (b.r.clone(), b.node.id.clone());
                match &b.text.body {
                    Body::Text(i) => out.push(Target {
                        at: b.r.to_string(),
                        prefix: "@",
                        text: raw(&i.plain()),
                        id,
                    }),
                    Body::List(items) => {
                        for (n, it) in items.iter().enumerate() {
                            out.push(Target {
                                at: format!("{}.{}", b.r, n + 1),
                                prefix: "@",
                                text: raw(&it.text.plain()),
                                id: id.clone(),
                            });
                        }
                    }
                    Body::Table(rows) => {
                        for (r, row) in rows.iter().enumerate() {
                            for (c, cell) in row.iter().enumerate() {
                                out.push(Target {
                                    at: format!("{}.r{}c{}", b.r, r + 1, c + 1),
                                    prefix: "@",
                                    text: raw(&cell.plain()),
                                    id: id.clone(),
                                });
                            }
                        }
                    }
                }
            }
        }
        (FileKind::Spreadsheet, Scope::Text | Scope::Formulas | Scope::Notes) => {
            for sheet in tree.sheets() {
                let Some(sref) = idx.ref_of(&sheet.id) else {
                    continue;
                };
                let view = SheetView::new(tree, sheet);
                for (r, row) in view.rows.iter().enumerate() {
                    // Notes can sit on cells beyond the used columns.
                    let width = if scope == Scope::Notes {
                        100
                    } else {
                        view.cols
                    };
                    for c in 0..width {
                        let text = if scope == Scope::Notes {
                            field_string(&row.current_fields[&format!("format_{c}")]["note"])
                        } else {
                            view.cell(r, c)
                        };
                        let is_formula = text.trim_start().starts_with('=');
                        let wanted = match scope {
                            Scope::Text => !text.is_empty() && !is_formula,
                            Scope::Formulas => is_formula,
                            Scope::Notes => !text.is_empty(),
                        };
                        if wanted {
                            out.push(Target {
                                at: format!(
                                    "{sref}!{}",
                                    CellRef {
                                        col: c as u16,
                                        row: r as u32 + 1
                                    }
                                ),
                                prefix: "",
                                text: raw(&text),
                                id: (sref.clone(), sheet.id.clone()),
                            });
                        }
                    }
                }
            }
        }
        (FileKind::Presentation, Scope::Text | Scope::Notes) => {
            for slide in tree.slides() {
                let Some(sref) = idx.ref_of(&slide.id) else {
                    continue;
                };
                let view = SlideView::new(tree, slide);
                if scope == Scope::Notes {
                    if !view.notes.is_empty() {
                        out.push(Target {
                            at: sref.to_string(),
                            prefix: "@",
                            text: raw(&view.notes),
                            id: (sref.clone(), slide.id.clone()),
                        });
                    }
                    continue;
                }
                for s in view.shapes.iter().filter(|s| s.kind == "text") {
                    let Some(shape) = idx.ref_of(&s.node.id) else {
                        continue;
                    };
                    out.push(Target {
                        at: format!("{sref}.{shape}"),
                        prefix: "@",
                        text: raw(&s.text),
                        id: (shape, s.node.id.clone()),
                    });
                }
            }
        }
        _ => return Err(AgentError::BadRequest { field: "in" }),
    }
    Ok(out)
}

/// `…left MATCH right…` around the byte range `s..e` of `text`.
fn snippet(text: &str, s: usize, e: usize, ctx: usize) -> String {
    let left_start = if ctx == 0 {
        s
    } else {
        text[..s]
            .char_indices()
            .rev()
            .take(ctx)
            .last()
            .map_or(s, |(i, _)| i)
    };
    let mut matched: String = text[s..e].chars().take(MATCH_CHARS + 1).collect();
    let long = matched.chars().count() > MATCH_CHARS;
    if long {
        matched = matched.chars().take(MATCH_CHARS).collect();
    }
    let right_end = if long {
        e
    } else {
        text[e..]
            .char_indices()
            .nth(ctx)
            .map_or(text.len(), |(i, _)| e + i)
    };
    let mut out = String::new();
    if left_start > 0 {
        out.push('…');
    }
    out.push_str(&text[left_start..s]);
    out.push_str(&matched);
    if long {
        out.push('…');
    } else {
        out.push_str(&text[e..right_end]);
        if right_end < text.len() {
            out.push('…');
        }
    }
    out
}

/// The tool.
pub fn find(
    file: &FileMeta,
    state: &DocumentState,
    opts: &FindOptions,
) -> Result<Response<FindData>, AgentError> {
    let qlen = opts.q.chars().count();
    if qlen == 0 {
        return Err(AgentError::BadRequest { field: "q" });
    }
    if qlen > MAX_QUERY_CHARS {
        return Err(AgentError::LimitExceeded {
            field: "q",
            limit: MAX_QUERY_CHARS as u64,
            value: qlen as u64,
        });
    }
    let ctx = match opts.context {
        None => CONTEXT_DEFAULT,
        Some(c) if c <= CONTEXT_MAX => c,
        Some(c) => {
            return Err(AgentError::LimitExceeded {
                field: "context",
                limit: CONTEXT_MAX as u64,
                value: c as u64,
            })
        }
    };
    let limit = resolve_limit(opts.limit, FIND_DEFAULT_LIMIT, FIND_MAX_LIMIT)?;
    let max_bytes = resolve_max_bytes(opts.max_bytes, DEFAULT_MAX_BYTES)?;
    let (start, _) = start_position(opts.cursor.as_deref(), file.rev)?;
    let re = build_regex(opts)?;
    let tree = Tree::new(state);
    let idx = RefIndex::from_state(state);
    let targets = targets(file, &tree, &idx, opts.scope)?;

    let mut units = Vec::new();
    let mut hits = Vec::new();
    let mut capped = false;
    'scan: for t in &targets {
        // What sanitising drops is reported with the target's first hit.
        let mut pending = escape(&t.text).1;
        let text = sanitize(&t.text);
        for m in re.find_iter(&text) {
            if m.is_empty() {
                continue;
            }
            if units.len() >= MAX_SCANNED_HITS {
                capped = true;
                break 'scan;
            }
            let snip = snippet(&text, m.start(), m.end(), ctx);
            let (esc, _) = escape(&snip);
            units.push(Unit {
                text: format!("{}{} {esc}\n", t.prefix, t.at),
                dropped: std::mem::take(&mut pending),
                ids: vec![t.id.clone()],
            });
            hits.push(Hit {
                at: t.at.clone(),
                snippet: snip,
                offset: text[..m.start()].chars().count(),
            });
        }
    }
    let total = units.len();
    let total_text = if capped {
        format!("{total}+")
    } else {
        total.to_string()
    };
    let spec = PagedSpec {
        tool: Tool::Find,
        file: Some(file),
        lead: Vec::new(),
        quoted: vec![("q", opts.q.clone())],
        header_rev: true,
        show_n: true,
        tail: vec![("total", total_text)],
        base_warn: Warnings::default(),
        fixed: String::new(),
        fixed_dropped: Dropped::default(),
        cursor_aux: None,
    };
    paged(
        spec,
        &units,
        start,
        limit,
        max_bytes,
        move |taken, cursor| FindData {
            n: taken,
            total,
            total_capped: capped,
            more: cursor.map(|c| c.to_string()),
            hits: hits.into_iter().skip(start).take(taken).collect(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::snippet;

    #[test]
    fn snippets_cut_on_characters() {
        let t = "héllo the second clause of the agreement states more";
        let s = t.find("clause").unwrap();
        assert_eq!(snippet(t, s, s + 6, 8), "… second clause of the …");
        assert_eq!(snippet(t, s, s + 6, 0), "…clause…");
        assert_eq!(snippet("clause", 0, 6, 30), "clause");
        let long = "a".repeat(300);
        let s = snippet(&long, 0, 300, 5);
        assert_eq!(s.chars().count(), 121);
        assert!(s.ends_with('…'));
    }
}
