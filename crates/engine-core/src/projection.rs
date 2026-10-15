//! Read-only check that the derived `node` table matches a verified replay. Two
//! encodings differ by design and are reported as known: a removed choice (table
//! `deleted`, replay `removed_choices`) and null fields (table `{}`, replay
//! `{"value": null}`).

use std::collections::HashSet;

use engine_shared::{DocumentId, Event, NodeId};
use futures::TryStreamExt;
use serde_json::Value;
use sqlx::PgPool;

use crate::{
    log::{self, ZERO_HASH},
    materializer::{DocumentState, MaterializedNode, Materializer},
    snapshot::SnapshotError,
};

pub const KNOWN_REMOVED_CHOICE: &str =
    "removed choice: the table sets deleted, replay lists it in removed_choices";
pub const KNOWN_NULL_FIELDS: &str =
    "null fields: the table stores {}, replay stores {\"value\": null}";

/// One column, or a whole `"row"`, where the table and replay disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionDifference {
    pub node_id: NodeId,
    pub field: &'static str,
    pub replayed: Value,
    pub stored: Value,
    /// Set for a documented encoding difference; `None` is a defect.
    pub known: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectionReport {
    pub through_seq: i64,
    pub stored_rows: usize,
    pub differences: Vec<ProjectionDifference>,
}

impl ProjectionReport {
    pub fn unexplained(&self) -> impl Iterator<Item = &ProjectionDifference> {
        self.differences.iter().filter(|d| d.known.is_none())
    }
}

type NodeRow = (
    String,
    Option<String>,
    String,
    String,
    Value,
    Option<String>,
    bool,
);

/// Replay the verified chain and compare every `node` row in one read-only
/// REPEATABLE READ transaction. Events stream; replayed state stays in memory.
pub async fn verify_projection(
    pool: &PgPool,
    document: DocumentId,
) -> Result<ProjectionReport, SnapshotError> {
    let mut tx = pool.begin().await?;
    sqlx::query("set transaction isolation level repeatable read, read only")
        .execute(&mut *tx)
        .await?;
    sqlx::query_scalar::<_, uuid::Uuid>("select id from document where id=$1")
        .bind(document.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(log::EventError::DocumentNotFound(document.0))?;

    let mut state = DocumentState::default();
    let mut seq = 0;
    let mut chain = ZERO_HASH.to_vec();
    {
        let mut events =
            sqlx::query_as::<_, Event>("select * from event where document_id=$1 order by seq")
                .bind(document.0)
                .fetch(&mut *tx);
        while let Some(event) = events.try_next().await? {
            log::verify_event(&event, document, seq + 1, &chain)?;
            Materializer::fold_into(&mut state, std::slice::from_ref(&event))?;
            seq = event.seq;
            chain = event.chain_hash;
        }
    }

    let mut differences = Vec::new();
    let mut seen = HashSet::new();
    {
        let mut rows = sqlx::query_as::<_, NodeRow>(
            "select id, parent_id, type, pos, current_fields, var_name, deleted
             from node where document_id=$1",
        )
        .bind(document.0)
        .fetch(&mut *tx);
        while let Some(row) = rows.try_next().await? {
            let id = NodeId(row.0.clone());
            match state.nodes.get(&id) {
                Some(node) => compare_node(&state, node, &row, &mut differences),
                None => differences.push(row_on_one_side(&id, false)),
            }
            seen.insert(id);
        }
    }
    for id in state.nodes.keys().filter(|id| !seen.contains(*id)) {
        differences.push(row_on_one_side(id, true));
    }
    tx.rollback().await?;

    differences.sort_by(|a, b| (&a.node_id, a.field).cmp(&(&b.node_id, b.field)));
    Ok(ProjectionReport {
        through_seq: seq,
        stored_rows: seen.len(),
        differences,
    })
}

fn row_on_one_side(id: &NodeId, in_replay: bool) -> ProjectionDifference {
    let present = Value::String("present".into());
    ProjectionDifference {
        node_id: id.clone(),
        field: "row",
        replayed: if in_replay {
            present.clone()
        } else {
            Value::Null
        },
        stored: if in_replay { Value::Null } else { present },
        known: None,
    }
}

fn compare_node(
    state: &DocumentState,
    node: &MaterializedNode,
    row: &NodeRow,
    out: &mut Vec<ProjectionDifference>,
) {
    let (_, parent_id, node_type, pos, fields, var_name, deleted) = row;
    let mut push = |field, replayed: Value, stored: Value, known| {
        out.push(ProjectionDifference {
            node_id: node.id.clone(),
            field,
            replayed,
            stored,
            known,
        })
    };
    let replayed_parent = node.parent_id.as_ref().map(|p| p.0.as_str());
    if replayed_parent != parent_id.as_deref() {
        push(
            "parent_id",
            replayed_parent.into(),
            parent_id.as_deref().into(),
            None,
        );
    }
    if &node.node_type != node_type {
        push(
            "type",
            node.node_type.as_str().into(),
            node_type.as_str().into(),
            None,
        );
    }
    if &node.pos != pos {
        push("pos", node.pos.as_str().into(), pos.as_str().into(), None);
    }
    if node.var_name.as_ref() != var_name.as_ref() {
        push(
            "var_name",
            node.var_name.as_deref().into(),
            var_name.as_deref().into(),
            None,
        );
    }
    if node.deleted != *deleted {
        let known = (!node.deleted && *deleted && state.removed_choices.contains(&node.id))
            .then_some(KNOWN_REMOVED_CHOICE);
        push("deleted", node.deleted.into(), (*deleted).into(), known);
    }
    if &node.current_fields != fields {
        let known = null_fields_only(&node.current_fields, fields).then_some(KNOWN_NULL_FIELDS);
        push(
            "current_fields",
            node.current_fields.clone(),
            fields.clone(),
            known,
        );
    }
}

/// True when replay differs only by a `"value": null` key that the table never wrote.
fn null_fields_only(replayed: &Value, stored: &Value) -> bool {
    match (replayed.as_object(), stored.as_object()) {
        (Some(replayed), Some(stored))
            if replayed.get("value") == Some(&Value::Null) && !stored.contains_key("value") =>
        {
            let mut without = replayed.clone();
            without.remove("value");
            &without == stored
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_a_missing_null_value_key_is_a_known_fields_difference() {
        assert!(null_fields_only(
            &json!({"value": null, "label": "Q"}),
            &json!({"label": "Q"})
        ));
        assert!(!null_fields_only(&json!({"value": 1}), &json!({})));
        assert!(!null_fields_only(
            &json!({"value": null, "label": "Q"}),
            &json!({"label": "other"})
        ));
    }
}
