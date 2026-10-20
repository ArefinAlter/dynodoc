//! The text model behind `set_text` and `replace` (protocol spec 8.3): a text container
//! (paragraph, heading, code block) as a list of cells, so an edit changes characters and
//! leaves every other run, mark and inline object as it was.
//!
//! A cell is one character with the metadata of its run (every property of the `text` node
//! except `text`, interned as `richtext` does), or one inline object (`hardBreak`, a footnote
//! reference, a formula, an image) kept verbatim. The edited container is rebuilt from the
//! cells and handed to `richtext::make_patch`, which derives and checks the patch; nothing
//! here writes a patch by hand.
//!
//! * `set_text` aligns the old and the new text with a Myers character diff (the engine's own
//!   size cap: past 4,000 changed characters the change is one replacement) and keeps the cell
//!   of every retained character. Inserted characters take the style of the preceding
//!   retained character, else of the following one, else of the block's dominant run.
//! * `replace` changes only the matched spans. The replacement takes the style of the first
//!   replaced character, which is where an editor would put the cursor; the spec's rule for
//!   `set_text` would let a replaced word at the start of a bold or linked run lose the mark.
//! * A line feed in new text is a `hardBreak` object in paragraphs and headings (that is how
//!   `read` shows one) and a plain character in code blocks.
//! * Inline objects other than `hardBreak` appear in `read` as placeholders such as
//!   `⟦fn:N1⟧`. `set_text` must carry every placeholder back in order (spec 6.11), else the
//!   block is `rich_block`; `replace` sees an object as one opaque character that no `find`
//!   can match, so it never touches one.

use std::collections::BTreeMap;

use serde_json::{json, Value};
use similar::{capture_diff_slices, Algorithm, DiffOp, DiffTag};

use crate::error::AgentError;
use crate::grammar::{MAX_FIND_CHARS, MAX_TEXT_SCALARS};
use crate::text::{inline_of, Inline, Seg};

/// Changed characters (old plus new, after the common prefix and suffix) beyond which the
/// change is one replacement. The same figure as `richtext::DIFF_LIMIT`: a size, not a
/// clock, so every machine derives the same patch.
const DIFF_LIMIT: usize = 4_000;
/// Containers nest at most this deep (`richtext::MAX_DEPTH`).
const MAX_DEPTH: usize = 64;
/// Stand-in for an inline object in the text that `replace` searches.
pub(crate) const OBJECT: char = '\u{FFFC}';

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Cell {
    Ch { ch: char, style: usize },
    Obj(Value),
}

fn is_hard_break(v: &Value) -> bool {
    v["type"] == "hardBreak"
}

pub(crate) fn is_container_type(t: &str) -> bool {
    matches!(t, "paragraph" | "heading" | "codeBlock")
}

fn is_list_type(t: &str) -> bool {
    matches!(t, "bulletList" | "orderedList" | "taskList")
}

/// A parsed text container.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Runs {
    pub cells: Vec<Cell>,
    styles: Vec<Value>,
    /// A code block: a line feed is a character, not a `hardBreak`.
    code: bool,
}

impl Runs {
    /// Cells of a `paragraph`, `heading` or `codeBlock`. Anything the model cannot carry
    /// through an edit (nested inline structure, a child without a type) is `rich_block`.
    pub fn parse(container: &Value) -> Result<Runs, AgentError> {
        let kind = container["type"].as_str().unwrap_or("");
        if !is_container_type(kind) {
            return Err(AgentError::UnsupportedStructure);
        }
        let children: &[Value] = match container.get("content") {
            None => &[],
            Some(Value::Array(a)) => a,
            Some(_) => return Err(AgentError::RichBlock),
        };
        let mut cells = Vec::new();
        let mut styles: Vec<Value> = Vec::new();
        let mut lookup: BTreeMap<String, usize> = BTreeMap::new();
        for child in children {
            let kind = child["type"].as_str().ok_or(AgentError::RichBlock)?;
            if child.get("content").is_some() {
                return Err(AgentError::RichBlock);
            }
            if kind != "text" {
                cells.push(Cell::Obj(child.clone()));
            } else {
                let text = child["text"].as_str().ok_or(AgentError::RichBlock)?;
                let mut meta = child.as_object().ok_or(AgentError::RichBlock)?.clone();
                meta.remove("text");
                let meta = Value::Object(meta);
                let key = meta.to_string();
                let style = *lookup.entry(key).or_insert_with(|| {
                    styles.push(meta);
                    styles.len() - 1
                });
                cells.extend(text.chars().map(|ch| Cell::Ch { ch, style }));
            }
            if cells.len() > MAX_TEXT_SCALARS {
                return Err(AgentError::LimitExceeded {
                    field: "text",
                    limit: MAX_TEXT_SCALARS as u64,
                    value: cells.len() as u64,
                });
            }
        }
        Ok(Runs {
            cells,
            styles,
            code: kind == "codeBlock",
        })
    }

    /// The text a `find` matches: characters, a line feed for a `hardBreak`, [`OBJECT`] for
    /// any other inline object. One `char` per cell, so a `char` index is a cell index.
    pub fn plain(&self) -> String {
        self.cells
            .iter()
            .map(|c| match c {
                Cell::Ch { ch, .. } => *ch,
                Cell::Obj(v) if is_hard_break(v) => '\n',
                Cell::Obj(_) => OBJECT,
            })
            .collect()
    }

    /// Inline objects other than `hardBreak` (the `⟦...⟧` placeholders of `read`).
    pub fn objects(&self) -> Vec<&Value> {
        self.cells
            .iter()
            .filter_map(|c| match c {
                Cell::Obj(v) if !is_hard_break(v) => Some(v),
                _ => None,
            })
            .collect()
    }

    fn default_style(&mut self) -> usize {
        let meta = json!({"type": "text"});
        if let Some(i) = self.styles.iter().position(|s| *s == meta) {
            return i;
        }
        self.styles.push(meta);
        self.styles.len() - 1
    }

    /// The style used by the most characters (the first one on a tie), or the plain style.
    fn dominant_style(&mut self) -> usize {
        let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
        for c in &self.cells {
            if let Cell::Ch { style, .. } = c {
                *counts.entry(*style).or_default() += 1;
            }
        }
        let best = counts
            .iter()
            .fold(None, |best: Option<(usize, usize)>, (s, n)| match best {
                Some((_, bn)) if bn >= *n => best,
                _ => Some((*s, *n)),
            });
        match best {
            Some((s, _)) => s,
            None => self.default_style(),
        }
    }

    /// Cells for new text in `style`.
    fn cells_for(&self, text: &str, style: usize) -> Vec<Cell> {
        text.chars()
            .map(|ch| {
                if ch == '\n' && !self.code {
                    Cell::Obj(json!({"type": "hardBreak"}))
                } else {
                    Cell::Ch { ch, style }
                }
            })
            .collect()
    }

    /// Replace the char ranges `matches` (sorted, disjoint) with `with`; the replacement takes
    /// the style of the first replaced character.
    pub fn replace_ranges(&self, matches: &[(usize, usize)], with: &str) -> Runs {
        let mut out = self.clone();
        let fallback = out.dominant_style();
        let mut cells = Vec::with_capacity(self.cells.len());
        let mut at = 0usize;
        for &(s, e) in matches {
            cells.extend_from_slice(&self.cells[at..s]);
            // Replacing a span with the very same text changes nothing, formatting included.
            let same = self.cells[s..e].iter().map(|c| match c {
                Cell::Ch { ch, .. } => *ch,
                Cell::Obj(v) if is_hard_break(v) => '\n',
                Cell::Obj(_) => OBJECT,
            });
            if same.eq(with.chars()) {
                cells.extend_from_slice(&self.cells[s..e]);
                at = e;
                continue;
            }
            let style = self.cells[s..e]
                .iter()
                .find_map(|c| match c {
                    Cell::Ch { style, .. } => Some(*style),
                    Cell::Obj(_) => None,
                })
                .or_else(|| neighbour_style(&self.cells, s))
                .unwrap_or(fallback);
            cells.extend(out.cells_for(with, style));
            at = e;
        }
        cells.extend_from_slice(&self.cells[at..]);
        out.cells = cells;
        out
    }

    /// The container with its `content` rebuilt from the cells (adjacent characters of one
    /// style form one `text` node, as `richtext::apply_patch` builds them).
    pub fn to_container(&self, container: &Value) -> Value {
        let mut children: Vec<Value> = Vec::new();
        let mut run: Option<(usize, String)> = None;
        let flush =
            |run: &mut Option<(usize, String)>, children: &mut Vec<Value>, styles: &[Value]| {
                if let Some((style, text)) = run.take() {
                    let mut node = styles[style].as_object().cloned().unwrap_or_default();
                    node.insert("text".into(), Value::String(text));
                    children.push(Value::Object(node));
                }
            };
        for c in &self.cells {
            match c {
                Cell::Ch { ch, style } => {
                    if let Some((s, text)) = &mut run {
                        if s == style {
                            text.push(*ch);
                            continue;
                        }
                    }
                    flush(&mut run, &mut children, &self.styles);
                    run = Some((*style, ch.to_string()));
                }
                Cell::Obj(v) => {
                    flush(&mut run, &mut children, &self.styles);
                    children.push(v.clone());
                }
            }
        }
        flush(&mut run, &mut children, &self.styles);
        let mut out = container.clone();
        out["content"] = Value::Array(children);
        out
    }

    /// The text `new` as the edit of this container (spec 8.3): the container with the
    /// cells of the retained characters kept. `Ok(None)` when nothing changes. A container
    /// that holds inline objects other than `hardBreak` must find every placeholder of
    /// `read` in `new`, in order, else `rich_block`.
    pub fn set_text(&self, new: &str) -> Result<Option<Runs>, AgentError> {
        // Tokens: a character is itself, a `hardBreak` is a line feed, any other object is
        // `OBJECT_BASE + its ordinal` so the same object in both texts aligns.
        let objects = self.objects();
        let placeholders = placeholders(self, &objects);
        let old_tokens: Vec<u32> = {
            let mut k = 0u32;
            self.cells
                .iter()
                .map(|c| match c {
                    Cell::Ch { ch, .. } => *ch as u32,
                    Cell::Obj(v) if is_hard_break(v) => '\n' as u32,
                    Cell::Obj(_) => {
                        k += 1;
                        OBJECT_BASE + k - 1
                    }
                })
                .collect()
        };
        let new_tokens = new_tokens(new, &placeholders).ok_or(AgentError::RichBlock)?;
        if new_tokens.len() > MAX_TEXT_SCALARS {
            return Err(AgentError::LimitExceeded {
                field: "text",
                limit: MAX_TEXT_SCALARS as u64,
                value: new_tokens.len() as u64,
            });
        }
        if old_tokens == new_tokens {
            return Ok(None);
        }
        let ops = align(&old_tokens, &new_tokens);
        let mut me = self.clone();
        let dominant = me.dominant_style();
        let mut cells: Vec<Cell> = Vec::with_capacity(new_tokens.len());
        let mut last: Option<usize> = None;
        for (i, op) in ops.iter().enumerate() {
            let (tag, old, new_range) = op.as_tag_tuple();
            if tag == DiffTag::Equal {
                for o in old {
                    if let Cell::Ch { style, .. } = &self.cells[o] {
                        last = Some(*style);
                    }
                    cells.push(self.cells[o].clone());
                }
                continue;
            }
            if new_range.is_empty() {
                continue;
            }
            let style = last
                .or_else(|| following_style(self, &ops[i + 1..]))
                .unwrap_or(dominant);
            for n in new_range {
                let t = new_tokens[n];
                cells.push(if t >= OBJECT_BASE {
                    Cell::Obj(objects[(t - OBJECT_BASE) as usize].clone())
                } else if t == '\n' as u32 && !self.code {
                    Cell::Obj(json!({"type": "hardBreak"}))
                } else {
                    Cell::Ch {
                        ch: char::from_u32(t).unwrap_or(char::REPLACEMENT_CHARACTER),
                        style,
                    }
                });
            }
        }
        me.cells = cells;
        Ok(Some(me))
    }
}

/// First `Cell::Ch` style before `pos`, else at or after it.
fn neighbour_style(cells: &[Cell], pos: usize) -> Option<usize> {
    let ch = |c: &Cell| match c {
        Cell::Ch { style, .. } => Some(*style),
        Cell::Obj(_) => None,
    };
    cells[..pos]
        .iter()
        .rev()
        .find_map(ch)
        .or_else(|| cells[pos..].iter().find_map(ch))
}

/// The style of the first retained character after `ops`.
fn following_style(runs: &Runs, ops: &[DiffOp]) -> Option<usize> {
    ops.iter()
        .filter(|op| op.tag() == DiffTag::Equal)
        .flat_map(|op| op.old_range())
        .find_map(|o| match &runs.cells[o] {
            Cell::Ch { style, .. } => Some(*style),
            Cell::Obj(_) => None,
        })
}

/// Tokens at or above this value stand for the inline objects of the old text.
const OBJECT_BASE: u32 = 0x11_0000;

/// The placeholder `read` shows for each inline object other than `hardBreak`, in order:
/// `⟦kind⟧` or `⟦kind:value⟧`, written by the same code that writes `read`.
fn placeholders(runs: &Runs, objects: &[&Value]) -> Vec<String> {
    if objects.is_empty() {
        return Vec::new();
    }
    let container = runs.to_container(&json!({"type": "paragraph"}));
    let (inline, _) = inline_of(&container, false);
    inline
        .segs
        .iter()
        .filter(|s| matches!(s, Seg::Atom { .. }))
        .map(|s| {
            Inline {
                segs: vec![s.clone()],
            }
            .render(false)
            .0
        })
        .collect()
}

/// Tokens of `text`; `None` when the placeholders of `expected` do not all appear, in order.
fn new_tokens(text: &str, expected: &[String]) -> Option<Vec<u32>> {
    let mut out = Vec::with_capacity(text.len());
    let mut k = 0usize;
    let mut rest = text;
    while let Some(ch) = rest.chars().next() {
        if let Some(p) = expected.get(k).filter(|p| rest.starts_with(p.as_str())) {
            out.push(OBJECT_BASE + k as u32);
            k += 1;
            rest = &rest[p.len()..];
            continue;
        }
        out.push(ch as u32);
        rest = &rest[ch.len_utf8()..];
    }
    (k == expected.len()).then_some(out)
}

/// The engine's alignment (`richtext::text_diff`): Myers, or one replacement of the middle
/// when it is longer than [`DIFF_LIMIT`].
fn align(a: &[u32], b: &[u32]) -> Vec<DiffOp> {
    let prefix = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (old_len, new_len) = (a.len() - prefix - suffix, b.len() - prefix - suffix);
    if old_len == 0 || new_len == 0 || old_len + new_len <= DIFF_LIMIT {
        return capture_diff_slices(Algorithm::Myers, a, b);
    }
    let mut ops = Vec::new();
    if prefix > 0 {
        ops.push(DiffOp::Equal {
            old_index: 0,
            new_index: 0,
            len: prefix,
        });
    }
    ops.push(DiffOp::Replace {
        old_index: prefix,
        old_len,
        new_index: prefix,
        new_len,
    });
    if suffix > 0 {
        ops.push(DiffOp::Equal {
            old_index: prefix + old_len,
            new_index: prefix + new_len,
            len: suffix,
        });
    }
    ops
}

// ------------------------------------------------------------------ finding text

/// A literal search (the `find` of `replace` and `replace_all`), case-sensitive unless told
/// otherwise, optionally whole words. Linear-time: the pattern is escaped, never interpreted.
pub(crate) struct Matcher {
    literal: Option<String>,
    regex: Option<regex::Regex>,
}

impl Matcher {
    pub fn new(find: &str, case: bool, word: bool) -> Result<Matcher, AgentError> {
        if find.is_empty() || find.chars().count() > MAX_FIND_CHARS {
            return Err(AgentError::BadRequest { field: "find" });
        }
        if case && !word {
            return Ok(Matcher {
                literal: Some(find.to_string()),
                regex: None,
            });
        }
        let base = regex::escape(find);
        let pattern = if word {
            format!(r"\b(?:{base})\b")
        } else {
            base
        };
        let regex = regex::RegexBuilder::new(&pattern)
            .case_insensitive(!case)
            .size_limit(1 << 20)
            .dfa_size_limit(1 << 20)
            .nest_limit(64)
            .build()
            .map_err(|_| AgentError::BadRequest { field: "find" })?;
        Ok(Matcher {
            literal: None,
            regex: Some(regex),
        })
    }

    /// Non-overlapping matches, left to right, as ranges of `char` indexes.
    pub fn ranges(&self, text: &str) -> Vec<(usize, usize)> {
        let bytes: Vec<(usize, usize)> = match (&self.literal, &self.regex) {
            (Some(l), _) => text
                .match_indices(l.as_str())
                .map(|(s, m)| (s, s + m.len()))
                .collect(),
            (None, Some(r)) => r.find_iter(text).map(|m| (m.start(), m.end())).collect(),
            (None, None) => Vec::new(),
        };
        if bytes.is_empty() {
            return Vec::new();
        }
        let offsets: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
        let to_char = |b: usize| offsets.partition_point(|o| *o < b);
        bytes
            .into_iter()
            .map(|(s, e)| (to_char(s), to_char(e)))
            .collect()
    }
}

// ------------------------------------------------------------------ paths in a block

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PathError {
    /// There is no such item or cell.
    Missing,
    /// It exists but is not one editable text container.
    Structure,
}

pub(crate) fn at<'a>(v: &'a Value, path: &[usize]) -> Option<&'a Value> {
    path.iter()
        .try_fold(v, |node, i| node.get("content")?.as_array()?.get(*i))
}

pub(crate) fn set_at(v: &mut Value, path: &[usize], new: Value) {
    let mut node = v;
    for i in path {
        node = &mut node["content"][*i];
    }
    *node = new;
}

fn kind(v: &Value) -> &str {
    v["type"].as_str().unwrap_or("")
}

/// Paths to every text container in `block`, in document order, `block` itself included.
pub(crate) fn containers(block: &Value) -> Vec<Vec<usize>> {
    fn walk(node: &Value, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        if is_container_type(kind(node)) {
            out.push(path.clone());
            return;
        }
        if path.len() >= MAX_DEPTH {
            return;
        }
        if let Some(children) = node["content"].as_array() {
            for (i, child) in children.iter().enumerate() {
                path.push(i);
                walk(child, path, out);
                path.pop();
            }
        }
    }
    let mut out = Vec::new();
    walk(block, &mut Vec::new(), &mut out);
    out
}

/// The only child of `node` that is not a nested list, when it is a text container.
fn single_container(node: &Value) -> Option<usize> {
    let children = node["content"].as_array()?;
    let mut own = children
        .iter()
        .enumerate()
        .filter(|(_, c)| !is_list_type(kind(c)));
    let (i, c) = own.next()?;
    (own.next().is_none() && is_container_type(kind(c))).then_some(i)
}

/// Path of the paragraph of the `n`th item (1-based, depth-first, the numbering of `read`).
pub(crate) fn list_item_path(block: &Value, n: usize) -> Result<Vec<usize>, PathError> {
    fn walk(
        list: &Value,
        left: &mut usize,
        path: &mut Vec<usize>,
        depth: usize,
    ) -> Option<Option<Vec<usize>>> {
        for (i, li) in list["content"].as_array()?.iter().enumerate() {
            *left -= 1;
            if *left == 0 {
                let mut p = path.clone();
                p.push(i);
                return Some(single_container(li).map(|c| {
                    p.push(c);
                    p
                }));
            }
            if depth >= MAX_DEPTH {
                continue;
            }
            for (j, child) in li["content"].as_array().into_iter().flatten().enumerate() {
                if is_list_type(kind(child)) {
                    path.extend([i, j]);
                    let found = walk(child, left, path, depth + 1);
                    path.truncate(path.len() - 2);
                    if found.is_some() {
                        return found;
                    }
                }
            }
        }
        None
    }
    if !is_list_type(kind(block)) || n == 0 {
        return Err(PathError::Structure);
    }
    let mut left = n;
    match walk(block, &mut left, &mut Vec::new(), 0) {
        None => Err(PathError::Missing),
        Some(None) => Err(PathError::Structure),
        Some(Some(p)) => Ok(p),
    }
}

/// Path of the paragraph of table cell `row`, `col` (1-based physical cells).
pub(crate) fn cell_path(block: &Value, row: usize, col: usize) -> Result<Vec<usize>, PathError> {
    if kind(block) != "table" || row == 0 || col == 0 {
        return Err(PathError::Structure);
    }
    let cell = block["content"]
        .as_array()
        .and_then(|r| r.get(row - 1))
        .and_then(|r| r["content"].as_array())
        .and_then(|c| c.get(col - 1))
        .ok_or(PathError::Missing)?;
    let c = single_container(cell).ok_or(PathError::Structure)?;
    Ok(vec![row - 1, col - 1, c])
}

#[cfg(test)]
mod tests {
    use engine_core::richtext::{apply_patch, make_patch, plain_text};

    use super::*;

    fn run(text: &str, marks: Option<Value>) -> Value {
        match marks {
            Some(m) => json!({"type": "text", "text": text, "marks": m}),
            None => json!({"type": "text", "text": text}),
        }
    }

    fn para(runs: Vec<Value>) -> Value {
        json!({"type": "paragraph", "attrs": {"blockId": "B1"}, "content": runs})
    }

    fn bold() -> Option<Value> {
        Some(json!([{"type": "bold"}]))
    }

    /// Apply an edit the way the compiler does: rebuild, make_patch, apply the patch.
    fn edited(before: &Value, after: &Runs) -> Value {
        let after = after.to_container(before);
        let patch = make_patch(before, &after).expect("a checked patch exists");
        let applied = apply_patch(before, &patch).unwrap();
        assert_eq!(applied, after);
        applied
    }

    /// `[(text, bold?)]` of a paragraph.
    fn spans(p: &Value) -> Vec<(String, bool)> {
        p["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                (
                    c["text"].as_str().unwrap_or("<obj>").to_string(),
                    c["marks"] == json!([{"type": "bold"}]),
                )
            })
            .collect()
    }

    #[test]
    fn set_text_keeps_marks_on_unchanged_spans() {
        let before = para(vec![
            run("This covers the ", None),
            run("third quarter", bold()),
            run(" and more.", None),
        ]);
        let runs = Runs::parse(&before).unwrap();
        let after = runs
            .set_text("This review covers the third quarter and more!")
            .unwrap()
            .unwrap();
        let p = edited(&before, &after);
        assert_eq!(
            plain_text(&p),
            "This review covers the third quarter and more!"
        );
        let s = spans(&p);
        assert!(s.contains(&("third quarter".to_string(), true)), "{s:?}");
        assert_eq!(s.iter().filter(|(_, b)| *b).count(), 1);
        assert_eq!(p["attrs"], json!({"blockId": "B1"}));
        // No change at all.
        assert_eq!(
            runs.set_text("This covers the third quarter and more.")
                .unwrap(),
            None
        );
    }

    #[test]
    fn inserted_text_takes_the_preceding_style_then_the_following_then_the_dominant() {
        let before = para(vec![run("Bold ", bold()), run("plain text", None)]);
        let runs = Runs::parse(&before).unwrap();
        // After the bold run's last character: inherits bold (spec 8.3 rule). The space
        // after "Bold" is part of the bold run, so the new "!" lands inside it.
        let a = runs.set_text("Bold! plain text").unwrap().unwrap();
        assert_eq!(
            spans(&a.to_container(&before)),
            [("Bold! ".into(), true), ("plain text".into(), false)]
        );
        let a = runs.set_text("Bold plain text!").unwrap().unwrap();
        assert_eq!(
            spans(&a.to_container(&before)),
            [("Bold ".into(), true), ("plain text!".into(), false)]
        );
        // At the start of the block: the following retained character.
        let a = runs.set_text(">>Bold plain text").unwrap().unwrap();
        assert_eq!(
            spans(&a.to_container(&before))[0],
            (">>Bold ".to_string(), true)
        );
        // The whole text replaced: the dominant run (plain, 10 characters against 5).
        let a = runs.set_text("zzz").unwrap().unwrap();
        assert_eq!(spans(&a.to_container(&before)), [("zzz".into(), false)]);
        // Deleting everything leaves an empty container.
        let a = runs.set_text("").unwrap().unwrap();
        let p = edited(&before, &a);
        assert_eq!(p["content"], json!([]));
    }

    #[test]
    fn an_empty_container_takes_the_plain_style() {
        let before = json!({"type": "paragraph"});
        let runs = Runs::parse(&before).unwrap();
        assert!(runs.cells.is_empty());
        let a = runs.set_text("Hello").unwrap().unwrap();
        let p = edited(&before, &a);
        assert_eq!(p["content"], json!([{"type": "text", "text": "Hello"}]));
        assert_eq!(runs.set_text("").unwrap(), None);
    }

    #[test]
    fn line_feeds_are_hard_breaks_except_in_code() {
        let before = para(vec![run("one", None)]);
        let a = Runs::parse(&before)
            .unwrap()
            .set_text("one\ntwo")
            .unwrap()
            .unwrap();
        let p = a.to_container(&before);
        assert_eq!(
            p["content"],
            json!([run("one", None), {"type": "hardBreak"}, run("two", None)])
        );
        // The break reads back as a line feed and survives an unchanged round trip.
        let again = Runs::parse(&p).unwrap();
        assert_eq!(again.plain(), "one\ntwo");
        assert_eq!(again.set_text("one\ntwo").unwrap(), None);
        let code = json!({"type": "codeBlock", "content": [run("a", None)]});
        let a = Runs::parse(&code)
            .unwrap()
            .set_text("a\nb")
            .unwrap()
            .unwrap();
        assert_eq!(a.to_container(&code)["content"], json!([run("a\nb", None)]));
    }

    #[test]
    fn objects_need_their_placeholders() {
        let note =
            json!({"type": "documentNote", "attrs": {"id": "N1", "kind": "footnote", "body": "x"}});
        let math = json!({"type": "inlineMath", "attrs": {"latex": "a^2"}});
        let before = para(vec![
            run("See", None),
            note.clone(),
            run(" and ", bold()),
            math.clone(),
            run(".", None),
        ]);
        let runs = Runs::parse(&before).unwrap();
        assert_eq!(runs.objects().len(), 2);
        let ph = placeholders(&runs, &runs.objects());
        assert_eq!(ph, ["⟦fn:N1⟧", "⟦math:a^2⟧"]);
        // Dropping a placeholder would destroy the object.
        assert_eq!(
            runs.set_text("See and ⟦math:a^2⟧."),
            Err(AgentError::RichBlock)
        );
        assert_eq!(runs.set_text("See."), Err(AgentError::RichBlock));
        // Reordering them is refused too (they must come back in order).
        assert_eq!(
            runs.set_text("See ⟦math:a^2⟧ and ⟦fn:N1⟧."),
            Err(AgentError::RichBlock)
        );
        // Keeping them lets the rest change; the objects are the originals.
        let a = runs
            .set_text("Look⟦fn:N1⟧ and then ⟦math:a^2⟧!")
            .unwrap()
            .unwrap();
        let p = a.to_container(&before);
        let kinds: Vec<&str> = p["content"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c["type"].as_str().unwrap())
            .collect();
        assert_eq!(
            kinds,
            ["text", "documentNote", "text", "inlineMath", "text"]
        );
        assert_eq!(p["content"][1], note);
        assert_eq!(p["content"][3], math);
        edited(&before, &a);
        // Unchanged text with placeholders is no change.
        assert_eq!(runs.set_text("See⟦fn:N1⟧ and ⟦math:a^2⟧.").unwrap(), None);
    }

    #[test]
    fn replace_changes_only_the_matched_spans_and_keeps_their_style() {
        let before = para(vec![
            run("The ", None),
            run("policy site", bold()),
            run(" and the policy page", None),
        ]);
        let runs = Runs::parse(&before).unwrap();
        let m = Matcher::new("policy", true, false).unwrap();
        let r = m.ranges(&runs.plain());
        assert_eq!(r, [(4, 10), (24, 30)]);
        let a = runs.replace_ranges(&r, "procedure");
        let p = edited(&before, &a);
        assert_eq!(plain_text(&p), "The procedure site and the procedure page");
        // The first replacement sits at the start of the bold run and stays bold.
        assert_eq!(
            spans(&p),
            [
                ("The ".into(), false),
                ("procedure site".into(), true),
                (" and the procedure page".into(), false)
            ]
        );
        // Deleting a match.
        let a = runs.replace_ranges(&r[..1], "");
        assert_eq!(a.plain(), "The  site and the policy page");
    }

    #[test]
    fn replacing_a_span_with_itself_changes_nothing_even_over_mixed_styles() {
        let before = para(vec![run("del", bold()), run("ta force", None)]);
        let runs = Runs::parse(&before).unwrap();
        let r = Matcher::new("delta", true, false)
            .unwrap()
            .ranges(&runs.plain());
        assert_eq!(r, [(0, 5)]);
        assert_eq!(runs.replace_ranges(&r, "delta").cells, runs.cells);
        // A different text over the same span takes the style of its first character.
        let a = runs.replace_ranges(&r, "deltas");
        assert_eq!(
            spans(&a.to_container(&before)),
            [("deltas".to_string(), true), (" force".to_string(), false)]
        );
    }

    #[test]
    fn replace_never_touches_an_object() {
        let math = json!({"type": "inlineMath", "attrs": {"latex": "policy"}});
        let before = para(vec![
            run("a policy ", None),
            math.clone(),
            run(" policy", None),
        ]);
        let runs = Runs::parse(&before).unwrap();
        // The LaTeX text "policy" is not text of the paragraph; the object is one opaque cell.
        let m = Matcher::new("policy", true, false).unwrap();
        let r = m.ranges(&runs.plain());
        assert_eq!(r.len(), 2);
        let a = runs.replace_ranges(&r, "rule");
        assert_eq!(a.objects(), [&math]);
        edited(&before, &a);
        // A find can never match across the object.
        let m = Matcher::new("policy  policy", true, false).unwrap();
        assert!(m.ranges(&runs.plain()).is_empty());
    }

    #[test]
    fn matching_modes() {
        let m = |f: &str, case, word| {
            Matcher::new(f, case, word)
                .unwrap()
                .ranges("Policy policy POLICY policies")
        };
        assert_eq!(m("policy", true, false), [(7, 13)]);
        assert_eq!(m("policy", false, false).len(), 3);
        assert_eq!(m("policy", false, true), [(0, 6), (7, 13), (14, 20)]);
        assert_eq!(m("policy", true, true), [(7, 13)]);
        // Metacharacters are literal.
        let lit = Matcher::new("a.c(", true, false).unwrap();
        assert_eq!(lit.ranges("abc( a.c("), [(5, 9)]);
        let lit = Matcher::new("a.c(", false, false).unwrap();
        assert_eq!(lit.ranges("abc( A.C("), [(5, 9)]);
        // Offsets are characters, not bytes.
        assert_eq!(
            Matcher::new("ক", true, false).unwrap().ranges("এক ক"),
            [(1, 2), (3, 4)]
        );
        assert!(Matcher::new("", true, false).is_err());
    }

    #[test]
    fn containers_lists_and_cells() {
        let li = |t: &str, nested: Option<Value>| {
            let mut c = vec![json!({"type": "paragraph", "content": [run(t, None)]})];
            c.extend(nested);
            json!({"type": "listItem", "content": c})
        };
        let list = json!({"type": "bulletList", "content": [
            li("one", None),
            li("two", Some(json!({"type": "orderedList", "content": [li("two.a", None), li("two.b", None)]}))),
            li("three", None),
        ]});
        // Depth-first, the numbering of `read`: 1 one, 2 two, 3 two.a, 4 two.b, 5 three.
        let text_at = |n: usize| {
            let p = list_item_path(&list, n).unwrap();
            plain_text(at(&list, &p).unwrap())
        };
        assert_eq!(
            (1..=5).map(text_at).collect::<Vec<_>>(),
            ["one", "two", "two.a", "two.b", "three"]
        );
        assert_eq!(list_item_path(&list, 6), Err(PathError::Missing));
        assert_eq!(list_item_path(&list, 0), Err(PathError::Structure));
        let two_paras = json!({"type": "bulletList", "content": [{"type": "listItem", "content": [
            {"type": "paragraph"}, {"type": "paragraph"}]}]});
        assert_eq!(list_item_path(&two_paras, 1), Err(PathError::Structure));
        let table = json!({"type": "table", "content": [
            {"type": "tableRow", "content": [
                {"type": "tableHeader", "content": [{"type": "paragraph", "content": [run("A", None)]}]},
                {"type": "tableHeader", "content": [{"type": "paragraph", "content": [run("B", None)]}]}]},
            {"type": "tableRow", "content": [
                {"type": "tableCell", "content": [{"type": "paragraph", "content": [run("C", None)]}]}]},
        ]});
        assert_eq!(
            plain_text(at(&table, &cell_path(&table, 1, 2).unwrap()).unwrap()),
            "B"
        );
        assert_eq!(
            plain_text(at(&table, &cell_path(&table, 2, 1).unwrap()).unwrap()),
            "C"
        );
        assert_eq!(cell_path(&table, 2, 2), Err(PathError::Missing));
        assert_eq!(cell_path(&list, 1, 1), Err(PathError::Structure));
        assert_eq!(containers(&table).len(), 3);
        assert_eq!(containers(&list).len(), 5);
        assert_eq!(
            containers(&json!({"type": "paragraph"})),
            [Vec::<usize>::new()]
        );
        assert!(containers(&json!({"type": "horizontalRule"})).is_empty());
        // set_at replaces a container in place.
        let mut t = table.clone();
        set_at(
            &mut t,
            &[0, 1, 0],
            json!({"type": "paragraph", "content": [run("Z", None)]}),
        );
        assert_eq!(plain_text(&t["content"][0]["content"][1]), "Z");
    }

    #[test]
    fn unsupported_containers_are_refused() {
        assert_eq!(
            Runs::parse(&json!({"type": "blockquote"})),
            Err(AgentError::UnsupportedStructure)
        );
        assert_eq!(
            Runs::parse(
                &json!({"type": "paragraph", "content": [{"type": "link", "content": []}]})
            ),
            Err(AgentError::RichBlock)
        );
        assert_eq!(
            Runs::parse(&json!({"type": "paragraph", "content": [{"text": "x"}]})),
            Err(AgentError::RichBlock)
        );
        assert_eq!(
            Runs::parse(&json!({"type": "paragraph", "content": "x"})),
            Err(AgentError::RichBlock)
        );
    }

    // ------------------------------------------------------------ differential

    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    /// A container of random runs over a small alphabet, three possible mark sets.
    fn random_container(rng: &mut Rng) -> Value {
        let marks = [
            None,
            Some(json!([{"type": "bold"}])),
            Some(json!([{"type": "link", "attrs": {"href": "https://example.org/x"}}])),
        ];
        let mut runs = Vec::new();
        let mut prev = usize::MAX;
        for _ in 0..rng.below(6) {
            let mut style = rng.below(3);
            if style == prev {
                style = (style + 1) % 3;
            }
            prev = style;
            let text: String = (0..1 + rng.below(8))
                .map(|_| ['a', 'b', 'c', ' ', 'é', 'ক'][rng.below(6)])
                .collect();
            runs.push(run(&text, marks[style].clone()));
        }
        para(runs)
    }

    fn model_of(p: &Value) -> Vec<(char, Value)> {
        p["content"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .flat_map(|c| {
                let mut meta = c.as_object().unwrap().clone();
                let text = meta.remove("text").unwrap();
                let meta = Value::Object(meta);
                text.as_str()
                    .unwrap()
                    .chars()
                    .map(move |ch| (ch, meta.clone()))
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn random_set_text_edits_match_the_reference_model() {
        let mut rng = Rng(0x5eed_0001);
        for case in 0..400 {
            let before = random_container(&mut rng);
            let old: Vec<(char, Value)> = model_of(&before);
            let old_text: String = old.iter().map(|(c, _)| *c).collect();
            // Edit: replace a random slice of the old text with random text.
            let chars: Vec<char> = old_text.chars().collect();
            let (s, e) = {
                let a = rng.below(chars.len() + 1);
                let b = rng.below(chars.len() + 1);
                (a.min(b), a.max(b))
            };
            let ins: String = (0..rng.below(5))
                .map(|_| ['x', 'y', ' ', 'é'][rng.below(4)])
                .collect();
            let new_text: String = chars[..s]
                .iter()
                .chain(ins.chars().collect::<Vec<_>>().iter())
                .chain(chars[e..].iter())
                .collect();
            let runs = Runs::parse(&before).unwrap();
            let got = runs.set_text(&new_text).unwrap();
            if new_text == old_text {
                assert!(got.is_none(), "case {case}");
                continue;
            }
            let after = edited(&before, &got.unwrap());
            // 1. The text is exactly the new text.
            assert_eq!(plain_text(&after), new_text, "case {case}");
            // 2. The common prefix and suffix keep the metadata they had.
            let new_model = model_of(&after);
            let (ol, nl) = (old.len(), new_model.len());
            let prefix = old_text
                .chars()
                .zip(new_text.chars())
                .take_while(|(a, b)| a == b)
                .count();
            let suffix = old_text
                .chars()
                .rev()
                .zip(new_text.chars().rev())
                .take_while(|(a, b)| a == b)
                .count()
                .min(ol - prefix)
                .min(nl - prefix);
            for i in 0..prefix {
                assert_eq!(new_model[i].1, old[i].1, "case {case} prefix {i}");
            }
            for i in 1..=suffix {
                assert_eq!(new_model[nl - i].1, old[ol - i].1, "case {case} suffix {i}");
            }
            // 3. Every character that is not new carries a style the old text had.
            let old_styles: Vec<&Value> = old.iter().map(|(_, m)| m).collect();
            for (_, m) in &new_model {
                assert!(
                    old_styles.contains(&m) || old.is_empty() || m == &json!({"type": "text"}),
                    "case {case}"
                );
            }
        }
    }

    #[test]
    fn random_replacements_match_a_string_model() {
        let mut rng = Rng(0x5eed_0002);
        for case in 0..400 {
            let before = random_container(&mut rng);
            let old_text = plain_text(&before);
            if old_text.is_empty() {
                continue;
            }
            let find: String = old_text
                .chars()
                .skip(rng.below(old_text.chars().count()))
                .take(1 + rng.below(3))
                .collect();
            let with = ["", "Z", "ZZ", "é é"][rng.below(4)];
            let runs = Runs::parse(&before).unwrap();
            let m = Matcher::new(&find, true, false).unwrap();
            let r = m.ranges(&runs.plain());
            assert!(!r.is_empty(), "case {case}: {find:?} in {old_text:?}");
            let after = edited(&before, &runs.replace_ranges(&r, with));
            // The reference: str::replace, non-overlapping from the left, the same thing.
            assert_eq!(
                plain_text(&after),
                old_text.replace(&find, with),
                "case {case}"
            );
            // Idempotence when `with` shares no character with `find`: no new match can form
            // across a replacement, and nothing else is left to replace.
            if !with.is_empty() && with.chars().all(|c| !find.contains(c)) {
                let again = Runs::parse(&after).unwrap();
                assert!(m.ranges(&again.plain()).is_empty(), "case {case}");
            }
        }
    }
}
