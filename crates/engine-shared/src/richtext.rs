//! Versioned wire format. Offsets/counts are Unicode scalar values, not UTF-16.
//! Paths address child indices in ProseMirror `content` arrays at the patch base.
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichTextPatch {
    pub version: u8,
    pub before: String,
    pub after: String,
    /// Interned text-node metadata (including marks and unknown attributes).
    pub styles: Vec<Value>,
    pub edits: Vec<RichTextEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RichTextEdit {
    Text { path: Vec<usize>, ops: Vec<TextOp> },
    Props { path: Vec<usize>, value: Value },
    Replace { path: Vec<usize>, value: Value },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TextOp {
    Retain {
        count: usize,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        style: Option<usize>,
    },
    Delete {
        count: usize,
    },
    Insert {
        text: String,
        style: usize,
    },
}
