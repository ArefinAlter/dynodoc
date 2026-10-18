//! The file model the agent protocol reads (protocol spec 3, 4): a read-only view over
//! [`DocumentState`].
//!
//! * document: root `form` -> `item` blocks whose `content` is a ProseMirror-style object
//!   (`frontend/researcher/src/lib/workspace/files.ts`; `/structure` lists exactly the
//!   live nodes with an object `content`);
//! * spreadsheet: root `form` -> `section` with `kind=sheet` -> `item` rows with
//!   `cell_N` strings (`sheet-model.ts`);
//! * presentation: root `form` -> `section` with `kind=slide` -> `item` shapes
//!   (`presentation.ts`).
//!
//! Siblings are ordered by `(pos, id)` like the editors do.

use std::collections::BTreeMap;
use std::fmt;

use engine_core::materializer::{DocumentState, MaterializedNode};
use engine_shared::{DocumentId, NodeId};
use serde_json::Value;

use crate::rev::Chain8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileKind {
    Document,
    Spreadsheet,
    Presentation,
}

impl FileKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FileKind::Document => "document",
            FileKind::Spreadsheet => "spreadsheet",
            FileKind::Presentation => "presentation",
        }
    }

    pub fn parse(s: &str) -> Option<FileKind> {
        match s {
            "document" => Some(FileKind::Document),
            "spreadsheet" => Some(FileKind::Spreadsheet),
            "presentation" => Some(FileKind::Presentation),
            _ => None,
        }
    }
}

impl fmt::Display for FileKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What the caller knows about the file (from the `document` row and the event log); the
/// state itself does not carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    pub id: DocumentId,
    pub kind: FileKind,
    /// Event `seq` the state was materialised at; `0` is the empty initial state.
    pub rev: u64,
    /// Hex `chain_hash` of that event (at least 8 hex characters; all zeros at `rev=0`).
    pub chain: String,
}

impl FileMeta {
    /// First 8 hex characters of the document UUID (the `file=` header value).
    pub fn short_id(&self) -> String {
        self.id.0.simple().to_string()[..8].to_string()
    }

    pub fn chain8(&self) -> Chain8 {
        Chain8::from_hex(&self.chain).unwrap_or_else(|_| Chain8::zero())
    }
}

fn order(a: &MaterializedNode, b: &MaterializedNode) -> std::cmp::Ordering {
    (a.pos.as_str(), &a.id).cmp(&(b.pos.as_str(), &b.id))
}

/// Live children of every node, in `(pos, id)` order, built once per call.
pub struct Tree<'a> {
    pub state: &'a DocumentState,
    pub root: Option<&'a MaterializedNode>,
    children: BTreeMap<&'a NodeId, Vec<&'a MaterializedNode>>,
}

impl<'a> Tree<'a> {
    pub fn new(state: &'a DocumentState) -> Self {
        let root = state
            .nodes
            .values()
            .find(|n| !n.deleted && n.parent_id.is_none() && n.node_type == "form");
        let mut children: BTreeMap<&NodeId, Vec<&MaterializedNode>> = BTreeMap::new();
        for n in state.nodes.values().filter(|n| !n.deleted) {
            if let Some(p) = &n.parent_id {
                children.entry(p).or_default().push(n);
            }
        }
        for list in children.values_mut() {
            list.sort_by(|a, b| order(a, b));
        }
        Self {
            state,
            root,
            children,
        }
    }

    pub fn children(&self, id: &NodeId) -> &[&'a MaterializedNode] {
        self.children.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    fn root_children(&self) -> &[&'a MaterializedNode] {
        self.root.map(|r| self.children(&r.id)).unwrap_or(&[])
    }

    /// File title: the root's `label`.
    pub fn title(&self) -> String {
        self.root
            .and_then(|r| r.current_fields["label"].as_str())
            .unwrap_or("")
            .to_string()
    }

    /// Document blocks: live children of the root with an object `content`.
    pub fn blocks(&self) -> Vec<&'a MaterializedNode> {
        self.root_children()
            .iter()
            .copied()
            .filter(|n| n.current_fields["content"].is_object())
            .collect()
    }

    pub fn sheets(&self) -> Vec<&'a MaterializedNode> {
        self.root_children()
            .iter()
            .copied()
            .filter(|n| n.node_type == "section" && n.current_fields["kind"] == "sheet")
            .collect()
    }

    pub fn slides(&self) -> Vec<&'a MaterializedNode> {
        self.root_children()
            .iter()
            .copied()
            .filter(|n| n.node_type == "section" && n.current_fields["kind"] == "slide")
            .collect()
    }

    /// Best-effort kind from the structure, for callers without `settings.kind`.
    pub fn detect_kind(&self) -> FileKind {
        if !self.sheets().is_empty() {
            FileKind::Spreadsheet
        } else if !self.slides().is_empty() {
            FileKind::Presentation
        } else {
            FileKind::Document
        }
    }
}

/// Text of a JSON field like the editors' `String(x ?? '')`.
pub fn field_string(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

/// `88KB`-style size of an omitted payload.
pub fn human_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes}B")
    } else if bytes < 1024 * 1024 {
        format!("{}KB", (bytes + 512) / 1024)
    } else {
        let tenths = (bytes as u64 * 10 + 524_288) / 1_048_576;
        format!("{}.{}MB", tenths / 10, tenths % 10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        assert_eq!(human_size(0), "0B");
        assert_eq!(human_size(1023), "1023B");
        assert_eq!(human_size(1024), "1KB");
        assert_eq!(human_size(88 * 1024), "88KB");
        assert_eq!(human_size(1024 * 1024), "1.0MB");
        assert_eq!(human_size(1_572_864), "1.5MB");
    }

    #[test]
    fn field_strings() {
        assert_eq!(field_string(&serde_json::json!(null)), "");
        assert_eq!(field_string(&serde_json::json!(12)), "12");
        assert_eq!(field_string(&serde_json::json!(true)), "true");
        assert_eq!(field_string(&serde_json::json!("x")), "x");
    }
}
