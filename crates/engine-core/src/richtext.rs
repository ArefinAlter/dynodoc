//! Formatting-aware deltas and conservative three-way merging.
//!
//! Durable identity belongs to the containing node, never to a style or offset.
//! Metadata is interned per delta; scalar tokens exist only during computation.
//! This is a centralized revision algorithm, not a sequence CRDT.
use engine_shared::{
    richtext::{RichTextEdit, RichTextPatch, TextOp},
    EventPayload,
};
use ring::digest::{digest, SHA256};
use serde::Serialize;
use serde_json::{json, Map, Value};
use similar::{capture_diff_slices_deadline, Algorithm, DiffOp, DiffTag};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

use crate::materializer::DocumentState;

const MAX_SCALARS: usize = 100_000;
const MAX_DEPTH: usize = 64;

pub fn plain_text(value: &Value) -> String {
    if matches!(value["type"].as_str(), Some("inlineMath" | "blockMath")) {
        return value["attrs"]["latex"].as_str().unwrap_or("").into();
    }
    if let Some(text) = value["text"].as_str() {
        return text.into();
    }
    value["content"]
        .as_array()
        .map(|c| {
            c.iter()
                .map(plain_text)
                .collect::<Vec<_>>()
                .join(if value["type"] == "doc" { "\n" } else { "" })
        })
        .unwrap_or_default()
}

pub fn redundant_label(state: &DocumentState, op: &EventPayload) -> bool {
    matches!(op, EventPayload::FieldEdited { node_id, field, value } if field == "label"
        && state.nodes.get(node_id).is_some_and(|n| n.current_fields.get(field) == Some(value)))
}

#[derive(Debug, thiserror::Error)]
#[error("invalid rich-text patch: {0}")]
pub struct PatchError(pub &'static str);

pub fn fingerprint(value: &Value) -> String {
    let bytes = crate::log::canonical_json(value);
    digest(&SHA256, &bytes)
        .as_ref()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// The editor owns display semantics. Preserve every JSON property, even marks
/// unknown to this server; do not infer equivalence from how text looks today.
#[derive(Default)]
struct Styles {
    values: Vec<Value>,
    lookup: BTreeMap<String, usize>,
}
impl Styles {
    fn intern(&mut self, value: Value) -> usize {
        let key = String::from_utf8(crate::log::canonical_json(&value)).unwrap();
        if let Some(i) = self.lookup.get(&key) {
            return *i;
        }
        let i = self.values.len();
        self.values.push(value);
        self.lookup.insert(key, i);
        i
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Scalar {
    ch: char,
    style: usize,
}

fn props(value: &Value) -> Option<Value> {
    let mut map = value.as_object()?.clone();
    map.remove("content");
    Some(Value::Object(map))
}
fn inline(value: &Value, styles: &mut Styles) -> Option<Vec<Scalar>> {
    // Only known text containers may be interpreted as an empty inline sequence.
    if !matches!(
        value["type"].as_str(),
        Some("paragraph" | "heading" | "codeBlock")
    ) {
        return None;
    }
    let empty = vec![];
    let children = match value.get("content") {
        None => &empty,
        Some(v) => v.as_array()?,
    };
    let mut out = Vec::new();
    for child in children {
        if child["type"] != "text" || child.get("content").is_some() {
            return None;
        }
        let text = child["text"].as_str()?;
        let mut metadata = child.as_object()?.clone();
        metadata.remove("text");
        let style = styles.intern(Value::Object(metadata));
        for ch in text.chars() {
            if out.len() == MAX_SCALARS {
                return None;
            }
            out.push(Scalar { ch, style });
        }
    }
    Some(out)
}
fn children(tokens: &[Scalar], styles: &[Value]) -> Result<Vec<Value>, PatchError> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        let style = tokens[i].style;
        let mut metadata = styles
            .get(style)
            .and_then(Value::as_object)
            .cloned()
            .ok_or(PatchError("unknown style"))?;
        if metadata.get("type") != Some(&json!("text"))
            || metadata.contains_key("text")
            || metadata.contains_key("content")
        {
            return Err(PatchError("invalid text metadata"));
        }
        let start = i;
        while i < tokens.len() && tokens[i].style == style {
            i += 1;
        }
        metadata.insert(
            "text".into(),
            Value::String(tokens[start..i].iter().map(|t| t.ch).collect()),
        );
        out.push(Value::Object(metadata));
    }
    Ok(out)
}
fn text_diff(a: &[Scalar], b: &[Scalar]) -> Vec<DiffOp> {
    let a: Vec<_> = a.iter().map(|t| t.ch).collect();
    let b: Vec<_> = b.iter().map(|t| t.ch).collect();
    // The deadline bounds pathological edit distance. A coarse replacement remains
    // exact for replay; it merely produces a more conservative merge conflict.
    capture_diff_slices_deadline(
        Algorithm::Myers,
        &a,
        &b,
        Some(Instant::now() + Duration::from_millis(20)),
    )
}
fn delta(a: &[Scalar], b: &[Scalar]) -> Vec<TextOp> {
    let mut out = Vec::new();
    for op in text_diff(a, b) {
        let (tag, old_range, new_range) = op.as_tag_tuple();
        let (old, old_len, new, new_len) = (
            old_range.start,
            old_range.len(),
            new_range.start,
            new_range.len(),
        );
        if tag == DiffTag::Equal {
            for offset in 0..old_len {
                let style = (a[old + offset].style != b[new + offset].style)
                    .then_some(b[new + offset].style);
                match out.last_mut() {
                    Some(TextOp::Retain { count, style: prev }) if *prev == style => *count += 1,
                    _ => out.push(TextOp::Retain { count: 1, style }),
                }
            }
        } else {
            if old_len > 0 {
                out.push(TextOp::Delete { count: old_len });
            }
            for t in &b[new..new + new_len] {
                match out.last_mut() {
                    Some(TextOp::Insert { text, style }) if *style == t.style => text.push(t.ch),
                    _ => out.push(TextOp::Insert {
                        text: t.ch.to_string(),
                        style: t.style,
                    }),
                }
            }
        }
    }
    out
}
fn build(
    a: &Value,
    b: &Value,
    path: &mut Vec<usize>,
    styles: &mut Styles,
    edits: &mut Vec<RichTextEdit>,
) {
    if a == b {
        return;
    }
    if path.len() < MAX_DEPTH && a["type"] == b["type"] && props(a).is_some() && props(b).is_some()
    {
        if let (Some(x), Some(y)) = (inline(a, styles), inline(b, styles)) {
            if props(a) != props(b) {
                edits.push(RichTextEdit::Props {
                    path: path.clone(),
                    value: props(b).unwrap(),
                });
            }
            edits.push(RichTextEdit::Text {
                path: path.clone(),
                ops: delta(&x, &y),
            });
            return;
        }
        if let (Some(x), Some(y)) = (a["content"].as_array(), b["content"].as_array()) {
            if x.len() == y.len() {
                if props(a) != props(b) {
                    edits.push(RichTextEdit::Props {
                        path: path.clone(),
                        value: props(b).unwrap(),
                    });
                }
                for (i, (x, y)) in x.iter().zip(y).enumerate() {
                    path.push(i);
                    build(x, y, path, styles, edits);
                    path.pop();
                }
                return;
            }
        }
    }
    edits.push(RichTextEdit::Replace {
        path: path.clone(),
        value: b.clone(),
    });
}

pub fn make_patch(before: &Value, after: &Value) -> Option<RichTextPatch> {
    let mut styles = Styles::default();
    let mut edits = Vec::new();
    build(before, after, &mut Vec::new(), &mut styles, &mut edits);
    // Only transmit styles actually referenced by an operation, not base styles.
    let mut compact = Styles::default();
    for edit in &mut edits {
        if let RichTextEdit::Text { ops, .. } = edit {
            for op in ops {
                let index = match op {
                    TextOp::Insert { style, .. }
                    | TextOp::Retain {
                        style: Some(style), ..
                    } => style,
                    _ => continue,
                };
                *index = compact.intern(styles.values[*index].clone());
            }
        }
    }
    let patch = RichTextPatch {
        version: 1,
        before: fingerprint(before),
        after: fingerprint(after),
        styles: compact.values,
        edits,
    };
    // Coalescing text runs must not silently discard any source JSON distinction.
    (apply_patch(before, &patch).ok().as_ref() == Some(after)).then_some(patch)
}

pub fn apply_patch(before: &Value, patch: &RichTextPatch) -> Result<Value, PatchError> {
    if patch.version != 1 || fingerprint(before) != patch.before {
        return Err(PatchError("base/version mismatch"));
    }
    if patch.edits.len() > 10_000 || patch.styles.len() > 10_000 {
        return Err(PatchError("too many operations/styles"));
    }
    let mut out = before.clone();
    for edit in &patch.edits {
        let path = match edit {
            RichTextEdit::Text { path, .. }
            | RichTextEdit::Props { path, .. }
            | RichTextEdit::Replace { path, .. } => path,
        };
        if path.len() > MAX_DEPTH {
            return Err(PatchError("path too deep"));
        }
        let mut target = &mut out;
        for &i in path {
            target = target
                .get_mut("content")
                .and_then(Value::as_array_mut)
                .and_then(|v| v.get_mut(i))
                .ok_or(PatchError("invalid path"))?;
        }
        match edit {
            RichTextEdit::Replace { value, .. } => *target = value.clone(),
            RichTextEdit::Props { value, .. } => {
                let content = target.get("content").cloned();
                if !value.is_object() || value.get("content").is_some() {
                    return Err(PatchError("invalid properties"));
                }
                *target = value.clone();
                if let Some(content) = content {
                    target["content"] = content;
                }
            }
            RichTextEdit::Text { ops, .. } => {
                let mut styles = Styles::default();
                let input = inline(target, &mut styles)
                    .ok_or(PatchError("not an inline text container"))?;
                let added: Vec<_> = patch
                    .styles
                    .iter()
                    .map(|v| styles.intern(v.clone()))
                    .collect();
                let mut cursor = 0usize;
                let mut result = Vec::new();
                for op in ops {
                    match op {
                        TextOp::Retain { count, style } => {
                            let end = cursor
                                .checked_add(*count)
                                .filter(|end| *end <= input.len())
                                .ok_or(PatchError("retain out of bounds"))?;
                            let selected = style
                                .map(|s| added.get(s).copied().ok_or(PatchError("unknown style")))
                                .transpose()?;
                            result.extend(input[cursor..end].iter().map(|t| Scalar {
                                ch: t.ch,
                                style: selected.unwrap_or(t.style),
                            }));
                            cursor = end;
                        }
                        TextOp::Delete { count } => {
                            cursor = cursor
                                .checked_add(*count)
                                .filter(|end| *end <= input.len())
                                .ok_or(PatchError("delete out of bounds"))?
                        }
                        TextOp::Insert { text, style } => {
                            let style = *added.get(*style).ok_or(PatchError("unknown style"))?;
                            if text.len() > MAX_SCALARS * 4 {
                                return Err(PatchError("insert too large"));
                            }
                            result.extend(text.chars().map(|ch| Scalar { ch, style }));
                        }
                    }
                    if result.len() > MAX_SCALARS {
                        return Err(PatchError("text too large"));
                    }
                }
                if cursor != input.len() {
                    return Err(PatchError("incomplete consumption"));
                }
                target["content"] = json!(children(&result, &styles.values)?);
            }
        }
    }
    if fingerprint(&out) != patch.after {
        return Err(PatchError("result mismatch"));
    }
    Ok(out)
}

/// Existing clients may send whole fields; canonical history automatically stores
/// a checked delta only when it is smaller. Old events always remain replayable.
pub fn compact_event(state: &DocumentState, op: &EventPayload) -> EventPayload {
    if let EventPayload::FieldEdited {
        node_id,
        field,
        value,
    } = op
    {
        if field == "content" {
            if let Some(before) = state
                .nodes
                .get(node_id)
                .and_then(|n| n.current_fields.get(field))
            {
                if let Some(patch) = make_patch(before, value) {
                    let compact = EventPayload::RichTextPatched {
                        node_id: node_id.clone(),
                        patch,
                    };
                    if serde_json::to_vec(&compact).unwrap().len()
                        < serde_json::to_vec(op).unwrap().len()
                    {
                        return compact;
                    }
                }
            }
        }
    }
    op.clone()
}

/// Missing and explicit null are distinct. Merge marks by type and then property;
/// two different choices for the same property remain a human-review conflict.
fn merge_property(
    base: Option<&Value>,
    team: Option<&Value>,
    draft: Option<&Value>,
) -> Result<Option<Value>, ()> {
    if draft == base || draft == team {
        return Ok(team.cloned());
    }
    if team == base {
        return Ok(draft.cloned());
    }
    if let (Some(t), Some(d)) = (
        team.and_then(Value::as_object),
        draft.and_then(Value::as_object),
    ) {
        let empty = Map::new();
        let b = base.and_then(Value::as_object).unwrap_or(&empty);
        // Removing an object versus changing it must not resurrect it.
        if base.is_some_and(|v| !v.is_object()) {
            return Err(());
        }
        let keys: std::collections::BTreeSet<_> =
            b.keys().chain(t.keys()).chain(d.keys()).collect();
        let mut out = Map::new();
        for key in keys {
            let value = if key == "marks" {
                merge_marks(b.get(key), t.get(key), d.get(key))?
            } else {
                merge_property(b.get(key), t.get(key), d.get(key))?
            };
            if let Some(value) = value {
                out.insert(key.clone(), value);
            }
        }
        return Ok(Some(Value::Object(out)));
    }
    Err(())
}
fn marks(value: Option<&Value>) -> Result<Value, ()> {
    let mut out = Map::new();
    if let Some(value) = value {
        for mark in value.as_array().ok_or(())? {
            let kind = mark["type"].as_str().ok_or(())?;
            if out.insert(kind.into(), mark.clone()).is_some() {
                return Err(());
            }
        }
    }
    Ok(Value::Object(out))
}
fn merge_marks(
    b: Option<&Value>,
    t: Option<&Value>,
    d: Option<&Value>,
) -> Result<Option<Value>, ()> {
    if d == b || d == t {
        return Ok(t.cloned());
    }
    if t == b {
        return Ok(d.cloned());
    }
    let merged = merge_property(Some(&marks(b)?), Some(&marks(t)?), Some(&marks(d)?))?.ok_or(())?;
    // Preserve editor schema order from the branches, never alphabetically sort
    // marks (mark rank can affect HTML nesting and canonical editor output).
    let mut names = Vec::new();
    for v in [t, d].into_iter().flatten() {
        for mark in v.as_array().ok_or(())? {
            let name = mark["type"].as_str().ok_or(())?;
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    let result: Vec<_> = names
        .into_iter()
        .filter_map(|name| merged.get(name).cloned())
        .collect();
    Ok((!result.is_empty()).then(|| json!(result)))
}
#[derive(Clone)]
struct Hunk {
    start: usize,
    end: usize,
    insert: Vec<Scalar>,
}
fn alignment(a: &[Scalar], b: &[Scalar]) -> (Vec<Option<usize>>, Vec<Hunk>) {
    let mut map = vec![None; a.len()];
    let mut hunks = Vec::new();
    for op in text_diff(a, b) {
        let (tag, old_range, new_range) = op.as_tag_tuple();
        let (old, old_len, new, new_len) = (
            old_range.start,
            old_range.len(),
            new_range.start,
            new_range.len(),
        );
        if tag == DiffTag::Equal {
            for offset in 0..old_len {
                map[old + offset] = Some(new + offset);
            }
        } else {
            hunks.push(Hunk {
                start: old,
                end: old + old_len,
                insert: b[new..new + new_len].to_vec(),
            });
        }
    }
    (map, hunks)
}
fn overlaps(a: &Hunk, b: &Hunk) -> bool {
    if a.start == a.end {
        a.start >= b.start && a.start <= b.end
    } else if b.start == b.end {
        b.start >= a.start && b.start <= a.end
    } else {
        a.start < b.end && b.start < a.end
    }
}
fn merge_inline(base: &Value, team: &Value, draft: &Value) -> Option<Value> {
    let mut styles = Styles::default();
    let b = inline(base, &mut styles)?;
    let t = inline(team, &mut styles)?;
    let d = inline(draft, &mut styles)?;
    let bp = props(base)?;
    let tp = props(team)?;
    let dp = props(draft)?;
    let mut out = merge_property(Some(&bp), Some(&tp), Some(&dp)).ok()??;
    let (tm, th) = alignment(&b, &t);
    let (dm, dh) = alignment(&b, &d);
    // Linear overlap scan after Myers; no pairwise all-hunks comparison.
    let (mut i, mut j) = (0, 0);
    while i < th.len() && j < dh.len() {
        if overlaps(&th[i], &dh[j]) {
            return None;
        }
        if th[i].end <= dh[j].start {
            i += 1;
        } else {
            j += 1;
        }
    }
    for (hunks, map, other) in [(&th, &dm, &d), (&dh, &tm, &t)] {
        for h in hunks {
            // Delete/replace versus formatting, and insertion at a concurrently
            // reformatted boundary, need review rather than guessed intent.
            let range = if h.start == h.end {
                h.start.saturating_sub(1)..(h.start + 1).min(b.len())
            } else {
                h.start..h.end
            };
            for i in range {
                if map[i].is_some_and(|k| b[i].style != other[k].style) {
                    return None;
                }
            }
        }
    }
    let mut hunks = th;
    hunks.extend(dh);
    hunks.sort_by_key(|h| h.start);
    let mut result = Vec::new();
    let mut cursor = 0;
    for h in hunks.into_iter().chain(std::iter::once(Hunk {
        start: b.len(),
        end: b.len(),
        insert: vec![],
    })) {
        for i in cursor..h.start {
            let ts = t[*tm.get(i)?.as_ref()?].style;
            let ds = d[*dm.get(i)?.as_ref()?].style;
            let metadata = merge_property(
                Some(&styles.values[b[i].style]),
                Some(&styles.values[ts]),
                Some(&styles.values[ds]),
            )
            .ok()??;
            result.push(Scalar {
                ch: b[i].ch,
                style: styles.intern(metadata),
            });
        }
        result.extend(h.insert);
        cursor = h.end;
    }
    out["content"] = json!(children(&result, &styles.values).ok()?);
    Some(out)
}

pub fn merge(base: &Value, team: &Value, draft: &Value) -> Option<Value> {
    if draft == base || draft == team {
        return Some(team.clone());
    }
    if team == base {
        return Some(draft.clone());
    }
    // Nested structural edits remain explicit conflicts until nested stable IDs
    // are implemented. Never align moved table/list children by array position.
    merge_inline(base, team, draft)
}

#[derive(Debug, Default, Serialize)]
pub struct Analysis {
    pub schema: &'static str,
    pub containers: usize,
    pub text_scalars: usize,
    pub formatting_runs: usize,
    pub distinct_styles: usize,
    pub kinds: BTreeMap<String, usize>,
}
/// One pass over the tree; no LLM or graph database in the classification path.
/// Inline runs with equal metadata coalesce only when adjacent in one container.
pub fn analyze(value: &Value) -> Analysis {
    let mut result = Analysis {
        schema: "dynodoc-richtext-analysis-v1",
        ..Default::default()
    };
    let mut styles = Styles::default();
    let mut stack = vec![value];
    while let Some(node) = stack.pop() {
        if let Some(kind) = node["type"].as_str() {
            *result.kinds.entry(kind.into()).or_default() += 1;
            if kind != "text" {
                result.containers += 1;
            }
        }
        if let Some(content) = node["content"].as_array() {
            let mut previous = None;
            for child in content {
                if child["type"] == "text" {
                    if let (Some(text), Some(mut meta)) =
                        (child["text"].as_str(), child.as_object().cloned())
                    {
                        meta.remove("text");
                        let style = styles.intern(Value::Object(meta));
                        if !text.is_empty() {
                            result.text_scalars += text.chars().count();
                            if previous != Some(style) {
                                result.formatting_runs += 1;
                            }
                            previous = Some(style);
                        }
                    }
                } else {
                    previous = None;
                }
            }
            stack.extend(content.iter().rev());
        }
    }
    result.distinct_styles = styles.values.len();
    result
}
