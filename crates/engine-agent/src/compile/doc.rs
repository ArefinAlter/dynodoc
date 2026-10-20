//! Document operations (protocol spec 8.2, 8.3): `set_text`, `replace`, `replace_all`,
//! `insert`, `delete`, `move`, `set_type`.

use engine_core::richtext::{apply_patch, fingerprint, plain_text};
use engine_shared::richtext::{RichTextEdit, RichTextPatch};
use engine_shared::{EventPayload, NodeId, NodeType};
use serde_json::{json, Map, Value};

use super::{label_of, Work, R};
use crate::error::AgentError;
use crate::grammar::{Anchor, BlockType, NewBlock, Op, Place, Scope, Target, Which};
use crate::inline::{
    at, cell_path, containers, is_container_type, list_item_path, set_at, Matcher, PathError, Runs,
};
use crate::pos::{key_between, MAX_KEY_STEPS};
use crate::refs::{Ref, Tail};
use crate::text::analyze_block;

/// Blocks one `replace_all` may change (spec 8.2).
const MAX_AFFECTED_BLOCKS: usize = 5_000;
/// `replace_all` over more matching blocks than this needs `expect` (spec 8.6).
const EXPECT_REQUIRED_ABOVE: usize = 25;

fn bad(field: &'static str) -> AgentError {
    AgentError::BadRequest { field }
}

fn path_error(e: PathError, r: &Ref) -> AgentError {
    match e {
        PathError::Missing => AgentError::UnknownRef {
            refs: vec![r.clone()],
        },
        PathError::Structure => AgentError::UnsupportedStructure,
    }
}

/// A live block of the document: its id and ref. `redacted_block` when withheld,
/// `unsupported_structure` when the node is not a block (a section, a sheet row...).
fn block(w: &Work, field: &'static str, anchor: &Anchor) -> R<(NodeId, Ref)> {
    let id = w.resolve(field, anchor)?;
    let r = w.ref_of(&id);
    w.check_redacted(&id)?;
    let n = &w.work.nodes[&id];
    if n.parent_id != w.root || !n.current_fields["content"].is_object() {
        return Err(AgentError::UnsupportedStructure);
    }
    Ok((id, r))
}

/// The op's `base` against the team's hash of the block.
fn check_block_base(w: &mut Work, op: &Op, id: &NodeId, r: &Ref) -> R<()> {
    if op.base.is_none() {
        return Ok(());
    }
    // A block created in this call has no team version to compare with.
    if w.created.contains(id) {
        return Err(bad("base"));
    }
    let current = w.head_hash(id);
    w.check_base(op, r.to_string(), r, current)
}

/// Live blocks in the order readers use: children of the root with an object `content`.
fn blocks_in_order(w: &Work) -> Vec<(String, NodeId)> {
    let Some(root) = &w.root else {
        return Vec::new();
    };
    super::live_children(&w.work, root)
        .into_iter()
        .filter(|(_, id)| w.work.nodes[id].current_fields["content"].is_object())
        .collect()
}

fn content_of(w: &Work, id: &NodeId) -> Value {
    w.work.nodes[id].current_fields["content"].clone()
}

/// Write a block's new `content` and, when the engine keeps it as a whole-field edit, its
/// label (a patch updates the label itself; a redundant label edit is dropped).
fn write_content(w: &mut Work, id: &NodeId, after: Value) -> R<()> {
    let label = label_of(&after);
    w.emit(EventPayload::FieldEdited {
        node_id: id.clone(),
        field: "content".into(),
        value: after,
    })?;
    w.emit(EventPayload::FieldEdited {
        node_id: id.clone(),
        field: "label".into(),
        value: label,
    })
}

/// The text container `ref` / `ref.N` / `ref.rRcC` names.
fn container_path(content: &Value, tail: Option<&Tail>, r: &Ref) -> R<Vec<usize>> {
    match tail {
        None if is_container_type(content["type"].as_str().unwrap_or("")) => Ok(Vec::new()),
        None | Some(Tail::Shape(_)) => Err(AgentError::UnsupportedStructure),
        Some(Tail::Item(n)) => list_item_path(content, *n as usize).map_err(|e| path_error(e, r)),
        Some(Tail::Cell { row, col }) => {
            cell_path(content, *row as usize, *col as usize).map_err(|e| path_error(e, r))
        }
    }
}

// ------------------------------------------------------------------ set_text

pub(super) fn set_text(w: &mut Work, op: &Op, at_: &Target, text: &str) -> R<()> {
    let (id, r) = block(w, "ref", &at_.block)?;
    check_block_base(w, op, &id, &r)?;
    let content = content_of(w, &id);
    let path = container_path(&content, at_.tail.as_ref(), &r)?;
    let container = at(&content, &path).ok_or(AgentError::Internal)?;
    let runs = Runs::parse(container)?;
    let new = runs.set_text(text)?.ok_or(AgentError::NoChange)?;
    let mut after = content.clone();
    set_at(&mut after, &path, new.to_container(container));
    write_content(w, &id, after)
}

// ------------------------------------------------------------------ replace

/// The containers of one block that can be searched, with their runs and matches.
struct Found {
    path: Vec<usize>,
    runs: Runs,
    ranges: Vec<(usize, usize)>,
}

fn search(content: &Value, paths: Vec<Vec<usize>>, m: &Matcher, strict: bool) -> R<Vec<Found>> {
    let mut out = Vec::new();
    for path in paths {
        let container = at(content, &path).ok_or(AgentError::Internal)?;
        let runs = match Runs::parse(container) {
            Ok(r) => r,
            // A container the model cannot carry cannot be searched; one the agent named
            // explicitly is an error.
            Err(e) if strict => return Err(e),
            Err(_) => continue,
        };
        let ranges = m.ranges(&runs.plain());
        out.push(Found { path, runs, ranges });
    }
    Ok(out)
}

/// `content` with `picks[i]` (ranges of container `i`) replaced by `with`. `None` when no
/// container's cells change.
fn apply_replacements(
    content: &Value,
    found: &[Found],
    picks: &[Vec<(usize, usize)>],
    with: &str,
) -> Option<Value> {
    let mut after = content.clone();
    let mut changed = false;
    for (f, picked) in found.iter().zip(picks) {
        if picked.is_empty() {
            continue;
        }
        let new = f.runs.replace_ranges(picked, with);
        if new.cells != f.runs.cells {
            changed = true;
            let container = at(content, &f.path)?;
            set_at(&mut after, &f.path, new.to_container(container));
        }
    }
    changed.then_some(after)
}

pub(super) fn replace(
    w: &mut Work,
    op: &Op,
    at_: &Target,
    find: &str,
    with: &str,
    which: Which,
    expect: Option<u64>,
) -> R<()> {
    let (id, r) = block(w, "ref", &at_.block)?;
    check_block_base(w, op, &id, &r)?;
    let content = content_of(w, &id);
    let explicit = at_.tail.is_some();
    let paths = match at_.tail.as_ref() {
        None => containers(&content),
        Some(_) => vec![container_path(&content, at_.tail.as_ref(), &r)?],
    };
    let matcher = Matcher::new(find, true, false)?;
    let found = search(&content, paths, &matcher, explicit)?;
    let total: usize = found.iter().map(|f| f.ranges.len()).sum();
    if let Some(m) = expect {
        if m != total as u64 {
            return Err(AgentError::MatchCount {
                found: total as u64,
                expected: m,
                refs: vec![r],
            });
        }
    }
    if total == 0 {
        return Err(AgentError::NoMatch);
    }
    // Which matches, as indexes into the matches of the block in document order.
    let pick = |k: usize| match which {
        Which::All => true,
        Which::Nth(n) => k + 1 == n as usize,
        Which::Unspecified => total == 1 || expect.is_some(),
    };
    if let Which::Nth(n) = which {
        if n as usize > total {
            return Err(AgentError::NoMatch);
        }
    }
    if matches!(which, Which::Unspecified) && total > 1 && expect.is_none() {
        return Err(AgentError::MatchCount {
            found: total as u64,
            expected: 1,
            refs: vec![r],
        });
    }
    let mut k = 0usize;
    let picks: Vec<Vec<(usize, usize)>> = found
        .iter()
        .map(|f| {
            f.ranges
                .iter()
                .filter(|_| {
                    k += 1;
                    pick(k - 1)
                })
                .copied()
                .collect()
        })
        .collect();
    let after = apply_replacements(&content, &found, &picks, with).ok_or(AgentError::NoChange)?;
    write_content(w, &id, after)
}

// ------------------------------------------------------------------ replace_all

fn type_matches(label: &str, sel: &str) -> bool {
    match sel {
        "H" => label.len() == 2 && label.starts_with('H') && label.as_bytes()[1].is_ascii_digit(),
        "UL" | "OL" | "TBL" => label.strip_prefix(sel).is_some_and(|rest| {
            !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit() || b == b'x')
        }),
        s => label == s,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn replace_all(
    w: &mut Work,
    op: &Op,
    find: &str,
    with: &str,
    expect: Option<u64>,
    scope: &Scope,
    word: bool,
    case: bool,
) -> R<()> {
    // The targets are many blocks; a single `base` would guard none of them.
    if op.base.is_some() {
        return Err(bad("base"));
    }
    let all = blocks_in_order(w);
    // Block labels (outline types) and heading levels, in order.
    let info: Vec<(String, Option<u8>)> = all
        .iter()
        .map(|(_, id)| {
            let c = &w.work.nodes[id].current_fields["content"];
            let label = analyze_block(c, false).label;
            let level = label
                .strip_prefix('H')
                .and_then(|n| n.parse::<u8>().ok())
                .filter(|_| c["type"] == "heading");
            (label, level)
        })
        .collect();
    let in_scope: Vec<usize> = match scope {
        Scope::Everywhere => (0..all.len()).collect(),
        Scope::Under(a) => {
            let hid = w.resolve("args", a)?;
            let i = all
                .iter()
                .position(|(_, id)| *id == hid)
                .ok_or(AgentError::UnsupportedStructure)?;
            let level = info[i].1.ok_or(bad("args"))?;
            let end = info[i + 1..]
                .iter()
                .position(|(_, l)| l.is_some_and(|l| l <= level))
                .map_or(all.len(), |p| i + 1 + p);
            (i..end).collect()
        }
        Scope::Refs(list) => {
            let mut ids = std::collections::BTreeSet::new();
            for a in list {
                ids.insert(w.resolve("args", a)?);
            }
            (0..all.len())
                .filter(|i| ids.contains(&all[*i].1))
                .collect()
        }
        Scope::Types(types) => (0..all.len())
            .filter(|i| types.iter().any(|t| type_matches(&info[*i].0, t)))
            .collect(),
    };
    let matcher = Matcher::new(find, case, word)?;
    // Search every block in scope that the agent may touch; keep the ones with a match.
    struct Hit {
        id: NodeId,
        content: Value,
        found: Vec<Found>,
    }
    let mut hits: Vec<Hit> = Vec::new();
    let mut total = 0usize;
    for i in in_scope {
        let id = &all[i].1;
        if w.redacted(id) {
            continue;
        }
        let content = content_of(w, id);
        let found = search(&content, containers(&content), &matcher, false)?;
        let n: usize = found.iter().map(|f| f.ranges.len()).sum();
        if n > 0 {
            total += n;
            hits.push(Hit {
                id: id.clone(),
                content,
                found,
            });
        }
    }
    if let Some(m) = expect {
        if m != total as u64 {
            return Err(AgentError::MatchCount {
                found: total as u64,
                expected: m,
                refs: hits.iter().take(5).map(|h| w.ref_of(&h.id)).collect(),
            });
        }
    }
    if total == 0 {
        return Err(AgentError::NoMatch);
    }
    if hits.len() > MAX_AFFECTED_BLOCKS {
        return Err(AgentError::LimitExceeded {
            field: "blocks",
            limit: MAX_AFFECTED_BLOCKS as u64,
            value: hits.len() as u64,
        });
    }
    if hits.len() > EXPECT_REQUIRED_ABOVE && expect.is_none() {
        return Err(bad("expect"));
    }
    let mut changed = 0usize;
    for h in hits {
        let picks: Vec<Vec<(usize, usize)>> = h.found.iter().map(|f| f.ranges.clone()).collect();
        if let Some(after) = apply_replacements(&h.content, &h.found, &picks, with) {
            changed += 1;
            write_content(w, &h.id, after)?;
        }
    }
    if changed == 0 {
        return Err(AgentError::NoChange);
    }
    Ok(())
}

// ------------------------------------------------------------------ insert

/// Inline content for `text`: a text node per line with a `hardBreak` between lines.
fn inline_nodes(text: &str, breaks: bool) -> Vec<Value> {
    if !breaks {
        return if text.is_empty() {
            Vec::new()
        } else {
            vec![json!({"type": "text", "text": text})]
        };
    }
    let mut out = Vec::new();
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push(json!({"type": "hardBreak"}));
        }
        if !line.is_empty() {
            out.push(json!({"type": "text", "text": line}));
        }
    }
    out
}

fn with_content(mut node: Value, nodes: Vec<Value>) -> Value {
    if !nodes.is_empty() {
        node["content"] = Value::Array(nodes);
    }
    node
}

/// The `content` of a new block (spec 8.2): `attrs.blockId` is the new node id.
fn new_content(b: &NewBlock, id: &NodeId) -> Value {
    match b.t {
        BlockType::Paragraph => with_content(
            json!({"type": "paragraph", "attrs": {"blockId": id}}),
            inline_nodes(&b.text, true),
        ),
        BlockType::Heading(level) => with_content(
            json!({"type": "heading", "attrs": {"level": level, "blockId": id}}),
            inline_nodes(&b.text, true),
        ),
        BlockType::ListItem => json!({
            "type": "bulletList",
            "attrs": {"blockId": id},
            "content": [{
                "type": "listItem",
                "content": [with_content(json!({"type": "paragraph"}), inline_nodes(&b.text, true))]
            }]
        }),
        BlockType::Code => with_content(
            json!({"type": "codeBlock", "attrs": {"blockId": id}}),
            inline_nodes(&b.text, false),
        ),
    }
}

fn position(left: Option<&str>, right: Option<&str>) -> R<String> {
    key_between(left, right).ok_or(AgentError::LimitExceeded {
        field: "pos",
        limit: MAX_KEY_STEPS as u64,
        value: left.map_or(0, str::len).max(right.map_or(0, str::len)) as u64,
    })
}

pub(super) fn insert(w: &mut Work, op: &Op, place: &Place, blocks: &[NewBlock]) -> R<()> {
    // A new block has no earlier version to guard.
    if op.base.is_some() {
        return Err(bad("base"));
    }
    let root = w.root.clone().ok_or(AgentError::UnsupportedStructure)?;
    let (field, anchor, after) = match place {
        Place::After(a) => ("after", a, true),
        Place::Before(a) => ("before", a, false),
    };
    let (aid, _) = block(w, field, anchor)?;
    let kids = blocks_in_order(w);
    let i = kids
        .iter()
        .position(|(_, id)| *id == aid)
        .ok_or(AgentError::Internal)?;
    let (mut left, right) = if after {
        (
            Some(kids[i].0.clone()),
            kids.get(i + 1).map(|k| k.0.clone()),
        )
    } else {
        (
            i.checked_sub(1).map(|j| kids[j].0.clone()),
            Some(kids[i].0.clone()),
        )
    };
    for nb in blocks {
        let id = w.new_id()?;
        let pos = position(left.as_deref(), right.as_deref())?;
        let content = new_content(nb, &id);
        let label = plain_text(&content);
        w.emit(EventPayload::NodeCreated {
            node_id: id.clone(),
            node_type: NodeType::Item,
            parent_id: Some(root.clone()),
            pos: pos.clone(),
            fields: json!({"content": content, "label": label}),
            var_name: None,
        })?;
        w.created.push(id);
        left = Some(pos);
    }
    Ok(())
}

// ------------------------------------------------------------------ delete, move

pub(super) fn delete(w: &mut Work, op: &Op, at_: &Target) -> R<()> {
    let (id, r) = block(w, "ref", &at_.block)?;
    check_block_base(w, op, &id, &r)?;
    // A block that still has live children is not deleted here: `validate_op` refuses it
    // (`unsupported_structure`), so a container is never emptied silently.
    w.emit(EventPayload::NodeDeleted { node_id: id })
}

pub(super) fn move_block(w: &mut Work, op: &Op, at_: &Target, place: &Place) -> R<()> {
    let (id, r) = block(w, "ref", &at_.block)?;
    check_block_base(w, op, &id, &r)?;
    let root = w.root.clone().ok_or(AgentError::UnsupportedStructure)?;
    let (field, anchor, after) = match place {
        Place::After(a) => ("after", a, true),
        Place::Before(a) => ("before", a, false),
    };
    let (aid, _) = block(w, field, anchor)?;
    if aid == id {
        return Err(bad(field));
    }
    let all = blocks_in_order(w);
    // Already in place: nothing to do.
    let cur = all
        .iter()
        .position(|(_, b)| *b == id)
        .ok_or(AgentError::Internal)?;
    let neighbour = if after {
        cur.checked_sub(1)
    } else {
        Some(cur + 1)
    };
    if neighbour
        .and_then(|n| all.get(n))
        .is_some_and(|(_, b)| *b == aid)
    {
        return Err(AgentError::NoChange);
    }
    let kids: Vec<&(String, NodeId)> = all.iter().filter(|(_, b)| *b != id).collect();
    let i = kids
        .iter()
        .position(|(_, b)| *b == aid)
        .ok_or(AgentError::Internal)?;
    let (left, right) = if after {
        (
            Some(kids[i].0.as_str()),
            kids.get(i + 1).map(|k| k.0.as_str()),
        )
    } else {
        (
            i.checked_sub(1).map(|j| kids[j].0.as_str()),
            Some(kids[i].0.as_str()),
        )
    };
    let pos = position(left, right)?;
    w.emit(EventPayload::NodeMoved {
        node_id: id,
        new_parent_id: Some(root),
        new_pos: pos,
    })
}

// ------------------------------------------------------------------ set_type

/// `content` without its `content` array: the node properties a `props` edit carries.
fn props(content: &Value) -> Value {
    let mut m: Map<String, Value> = content.as_object().cloned().unwrap_or_default();
    m.remove("content");
    Value::Object(m)
}

pub(super) fn set_type(w: &mut Work, op: &Op, at_: &Target, to: BlockType) -> R<()> {
    let (id, r) = block(w, "ref", &at_.block)?;
    check_block_base(w, op, &id, &r)?;
    let content = content_of(w, &id);
    if !matches!(content["type"].as_str(), Some("paragraph" | "heading")) {
        return Err(AgentError::UnsupportedStructure);
    }
    let mut after = content.clone();
    let mut attrs = after["attrs"].as_object().cloned().unwrap_or_default();
    match to {
        BlockType::Heading(level) => {
            after["type"] = json!("heading");
            attrs.insert("level".into(), json!(level));
        }
        _ => {
            after["type"] = json!("paragraph");
            attrs.remove("level");
        }
    }
    if attrs.is_empty() {
        after.as_object_mut().map(|o| o.remove("attrs"));
    } else {
        after["attrs"] = Value::Object(attrs);
    }
    if after == content {
        return Err(AgentError::NoChange);
    }
    // One `props` edit at the root of the block keeps `attrs.blockId` and the content array
    // (spec 8.2). The engine keeps it when it is smaller than the whole field.
    let patch = RichTextPatch {
        version: 1,
        before: fingerprint(&content),
        after: fingerprint(&after),
        styles: Vec::new(),
        edits: vec![RichTextEdit::Props {
            path: Vec::new(),
            value: props(&after),
        }],
    };
    let patched = EventPayload::RichTextPatched {
        node_id: id.clone(),
        patch,
    };
    let whole = EventPayload::FieldEdited {
        node_id: id.clone(),
        field: "content".into(),
        value: after.clone(),
    };
    let exact = match &patched {
        EventPayload::RichTextPatched { patch, .. } => {
            apply_patch(&content, patch).is_ok_and(|v| v == after)
        }
        _ => false,
    };
    let size = |op: &EventPayload| serde_json::to_vec(op).map_or(usize::MAX, |b| b.len());
    if exact && size(&patched) < size(&whole) {
        w.emit(patched)
    } else {
        write_content(w, &id, after)
    }
}
