//! Presentation operations (protocol spec 8.5): `set_shape` and `set_notes`.
//!
//! A slide is a `section` with `kind=slide`; a shape is an `item` below it. Only the text of
//! a text shape and the notes of a slide change here; geometry, style, images, layouts and
//! the slide list are not touched.

use engine_shared::{EventPayload, NodeId};
use serde_json::json;

use super::{Work, R};
use crate::error::AgentError;
use crate::grammar::{Anchor, Op};
use crate::model::{field_string, Tree};
use crate::refs::{Ref, SlideSel};
use crate::slides::{select_slide, ShapeView};

/// Labels are plain text of at most 120 characters (`shapeLabel`, `slideLabel` of the editor).
fn label(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect()
}

/// The slide the agent named, by position (`s2`) or ref.
fn slide_of(w: &Work, sel: &SlideSel) -> R<NodeId> {
    let tree = Tree::new(&w.work);
    match select_slide(&tree, &w.idx, sel) {
        Ok((_, node)) => Ok(node.id.clone()),
        // A position past the last slide: the label parsed, no slide has it.
        Err(AgentError::BadRequest { .. }) => Err(AgentError::UnknownRef { refs: Vec::new() }),
        Err(e) => Err(e),
    }
}

pub(super) fn set_shape(w: &mut Work, op: &Op, slide: &SlideSel, shape: &Ref, text: &str) -> R<()> {
    let slide_id = slide_of(w, slide)?;
    let shape_id = w.resolve("shape", &Anchor::Ref(shape.clone()))?;
    w.check_redacted(&slide_id)?;
    w.check_redacted(&shape_id)?;
    let node = &w.work.nodes[&shape_id];
    if node.parent_id.as_ref() != Some(&slide_id) || node.node_type != "item" {
        return Err(AgentError::UnknownRef {
            refs: vec![shape.clone()],
        });
    }
    let view = ShapeView::new(node);
    if view.kind != "text" {
        return Err(AgentError::UnsupportedStructure);
    }
    let (old, is_title) = (view.text.clone(), view.role == Some("title"));
    let head = w.head_hash(&shape_id);
    w.check_base(op, shape.to_string(), shape, head)?;
    if old == text {
        return Err(AgentError::NoChange);
    }
    w.emit(EventPayload::FieldEdited {
        node_id: shape_id.clone(),
        field: "text".into(),
        value: json!(text),
    })?;
    // The labels follow the text, as the editor keeps them (`editShapeOps`).
    w.emit(EventPayload::FieldEdited {
        node_id: shape_id,
        field: "label".into(),
        value: json!(label(text)),
    })?;
    if is_title {
        let slide_label = Some(label(text)).filter(|l| !l.is_empty());
        w.emit(EventPayload::FieldEdited {
            node_id: slide_id,
            field: "label".into(),
            value: json!(slide_label.unwrap_or_else(|| "Untitled slide".into())),
        })?;
    }
    Ok(())
}

pub(super) fn set_notes(w: &mut Work, op: &Op, slide: &SlideSel, text: &str) -> R<()> {
    let slide_id = slide_of(w, slide)?;
    w.check_redacted(&slide_id)?;
    let sref = w.ref_of(&slide_id);
    let old = field_string(&w.work.nodes[&slide_id].current_fields["notes"]);
    let head = w.head_hash(&slide_id);
    w.check_base(op, sref.to_string(), &sref, head)?;
    if old == text {
        return Err(AgentError::NoChange);
    }
    w.emit(EventPayload::FieldEdited {
        node_id: slide_id,
        field: "notes".into(),
        value: json!(text),
    })
}
