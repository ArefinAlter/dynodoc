//! Spreadsheet operations (protocol spec 8.4): `set_cells` and `clear`.
//!
//! A sheet is a `section` with `kind=sheet`; a row is an `item` below it whose cells are
//! the strings `cell_0`, `cell_1`, ... Values are strings, as stored; a leading `=` is a
//! formula that nothing here evaluates. Formats are never touched.

use engine_core::materializer::DocumentState;
use engine_shared::{EventPayload, NodeId, NodeType};
use serde_json::{json, Map, Value};

use super::{live_children, Work, R};
use crate::a1::{A1Range, CellRef, MAX_COLS, MAX_ROWS};
use crate::error::AgentError;
use crate::grammar::{Anchor, CellAt, Op};
use crate::hash::range_hash;
use crate::model::{field_string, Tree};
use crate::pos::{key_between, MAX_KEY_STEPS};
use crate::refs::Ref;
use crate::sheet::select_sheet;

/// The editor's default width when a sheet has no `columns` field.
const DEFAULT_COLUMNS: u64 = 12;

fn limit(field: &'static str, limit: u64, value: u64) -> AgentError {
    AgentError::LimitExceeded {
        field,
        limit,
        value,
    }
}

/// The sheet the agent named (by name or ref) and its short ref.
fn sheet_of(w: &Work, q: &str) -> R<(NodeId, Ref)> {
    let tree = Tree::new(&w.work);
    let node = select_sheet(&tree, &w.idx, q, "sheet")?;
    Ok((node.id.clone(), w.ref_of(&node.id)))
}

/// Live rows of a sheet in order.
fn rows_of(state: &DocumentState, sheet: &NodeId) -> Vec<NodeId> {
    live_children(state, sheet)
        .into_iter()
        .filter(|(_, id)| state.nodes[id].node_type == "item")
        .map(|(_, id)| id)
        .collect()
}

fn cell(state: &DocumentState, row: Option<&NodeId>, col: usize) -> String {
    row.and_then(|r| state.nodes.get(r))
        .map(|n| field_string(&n.current_fields[&format!("cell_{col}")]))
        .unwrap_or_default()
}

/// The `rows` x `cols` rectangle at `(row0, col0)` as strings (0-based).
fn rect(
    state: &DocumentState,
    rows: &[NodeId],
    row0: usize,
    col0: usize,
    nrows: usize,
    ncols: usize,
) -> Vec<Vec<String>> {
    (0..nrows)
        .map(|r| {
            (0..ncols)
                .map(|c| cell(state, rows.get(row0 + r), col0 + c))
                .collect()
        })
        .collect()
}

/// The op's `base` against the team's hash of the same rectangle.
#[allow(clippy::too_many_arguments)]
fn check_range_base(
    w: &mut Work,
    op: &Op,
    sheet: &NodeId,
    sref: &Ref,
    row0: usize,
    col0: usize,
    nrows: usize,
    ncols: usize,
) -> R<()> {
    if op.base.is_none() {
        return Ok(());
    }
    let head = w.head();
    let live = head.nodes.get(sheet).is_some_and(|n| !n.deleted);
    let current = live.then(|| {
        let rows = rows_of(head, sheet);
        range_hash(&rect(head, &rows, row0, col0, nrows, ncols))
    });
    let range = A1Range {
        sheet: Some(sref.to_string()),
        from: CellRef {
            col: col0 as u16,
            row: row0 as u32 + 1,
        },
        to: CellRef {
            col: (col0 + ncols - 1) as u16,
            row: (row0 + nrows) as u32,
        },
    };
    w.check_base(op, range.to_string(), sref, current)
}

pub(super) fn set_cells(
    w: &mut Work,
    op: &Op,
    sheet_q: &str,
    at: &CellAt,
    values: &[Vec<String>],
) -> R<()> {
    let (sheet, sref) = sheet_of(w, sheet_q)?;
    w.check_redacted(&sheet)?;
    let rows = rows_of(&w.work, &sheet);
    let (row0, col0) = match at {
        CellAt::A1(c) => ((c.row - 1) as usize, usize::from(c.col)),
        CellAt::Row { row, col } => {
            let id = w.resolve("at", &Anchor::Ref(row.clone()))?;
            let i = rows
                .iter()
                .position(|r| *r == id)
                .ok_or(AgentError::UnknownRef {
                    refs: vec![row.clone()],
                })?;
            (i, usize::from(*col))
        }
    };
    let nrows = values.len();
    let width = values.iter().map(Vec::len).max().unwrap_or(0);
    if row0 + nrows > MAX_ROWS as usize {
        return Err(limit("at", u64::from(MAX_ROWS), (row0 + nrows) as u64));
    }
    if col0 + width > usize::from(MAX_COLS) {
        return Err(limit("at", u64::from(MAX_COLS), (col0 + width) as u64));
    }
    check_range_base(w, op, &sheet, &sref, row0, col0, nrows, width)?;

    // Existing rows: one edit per changed cell.
    let mut edits: Vec<EventPayload> = Vec::new();
    let mut widest: Option<usize> = None;
    let mut note = |col: usize, v: &str| {
        if !v.is_empty() {
            widest = Some(widest.map_or(col, |m| m.max(col)));
        }
    };
    for (r, row) in values.iter().enumerate() {
        let Some(rid) = rows.get(row0 + r) else {
            continue;
        };
        for (c, v) in row.iter().enumerate() {
            let col = col0 + c;
            if cell(&w.work, Some(rid), col) != *v {
                note(col, v);
                edits.push(EventPayload::FieldEdited {
                    node_id: rid.clone(),
                    field: format!("cell_{col}"),
                    value: json!(v),
                });
            }
        }
    }
    // Missing rows are created after the last one, up to the last row that holds a value.
    let last_needed = (0..nrows)
        .filter(|r| row0 + r >= rows.len() && values[*r].iter().any(|v| !v.is_empty()))
        .map(|r| row0 + r)
        .max();
    let mut creates: Vec<Map<String, Value>> = Vec::new();
    if let Some(last) = last_needed {
        for g in rows.len()..=last {
            let mut fields = Map::new();
            if let Some(row) = g.checked_sub(row0).and_then(|r| values.get(r)) {
                for (c, v) in row.iter().enumerate().filter(|(_, v)| !v.is_empty()) {
                    note(col0 + c, v);
                    fields.insert(format!("cell_{}", col0 + c), json!(v));
                }
            }
            creates.push(fields);
        }
    }
    if edits.is_empty() && creates.is_empty() {
        return Err(AgentError::NoChange);
    }

    // The sheet grows to hold the widest written column.
    let columns = match w.work.nodes[&sheet].current_fields["columns"].as_u64() {
        Some(n) if n > 0 => n,
        _ => DEFAULT_COLUMNS,
    };
    if let Some(m) = widest.filter(|m| *m as u64 + 1 > columns) {
        w.emit(EventPayload::FieldEdited {
            node_id: sheet.clone(),
            field: "columns".into(),
            value: json!(m + 1),
        })?;
    }
    let mut last_pos = rows.last().map(|r| w.work.nodes[r].pos.clone());
    for fields in creates {
        let id = w.new_id()?;
        let pos = key_between(last_pos.as_deref(), None).ok_or(limit(
            "pos",
            MAX_KEY_STEPS as u64,
            last_pos.as_deref().map_or(0, str::len) as u64,
        ))?;
        w.emit(EventPayload::NodeCreated {
            node_id: id,
            node_type: NodeType::Item,
            parent_id: Some(sheet.clone()),
            pos: pos.clone(),
            fields: Value::Object(fields),
            var_name: None,
        })?;
        last_pos = Some(pos);
    }
    for e in edits {
        w.emit(e)?;
    }
    Ok(())
}

pub(super) fn clear(w: &mut Work, op: &Op, sheet: Option<&str>, range: &A1Range) -> R<()> {
    let q = match (sheet, range.sheet.as_deref()) {
        (Some(a), Some(b)) if a != b => return Err(AgentError::BadRequest { field: "range" }),
        (Some(s), _) | (None, Some(s)) => s,
        (None, None) => return Err(AgentError::BadRequest { field: "sheet" }),
    };
    let (sheet, sref) = sheet_of(w, q)?;
    w.check_redacted(&sheet)?;
    let rows = rows_of(&w.work, &sheet);
    let (row0, col0) = ((range.from.row - 1) as usize, usize::from(range.from.col));
    check_range_base(
        w,
        op,
        &sheet,
        &sref,
        row0,
        col0,
        range.rows() as usize,
        range.cols() as usize,
    )?;
    let mut edits = Vec::new();
    for g in row0..(range.to.row as usize).min(rows.len()) {
        for col in col0..=usize::from(range.to.col) {
            if !cell(&w.work, rows.get(g), col).is_empty() {
                edits.push(EventPayload::FieldEdited {
                    node_id: rows[g].clone(),
                    field: format!("cell_{col}"),
                    value: json!(""),
                });
            }
        }
    }
    if edits.is_empty() {
        return Err(AgentError::NoChange);
    }
    for e in edits {
        w.emit(e)?;
    }
    Ok(())
}
