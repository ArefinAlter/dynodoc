//! State-based three-way merge for document blocks and spreadsheet cells.
//! Deletion versus editing is a conflict, including structural edits. Canonical
//! history is never replaced: `diff` emits new semantic operations only.
use crate::materializer::{DocumentState, MaterializedNode};
use engine_shared::{EventPayload, NodeId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conflict {
    pub key: String,
    pub node_id: NodeId,
    pub field: String,
    pub ancestor: Value,
    pub team: Value,
    pub draft: Value,
}
#[derive(Debug, Serialize)]
pub struct MergePlan {
    pub state: DocumentState,
    pub conflicts: Vec<Conflict>,
    pub ops: Vec<EventPayload>,
}
fn canonical(s: &DocumentState) -> DocumentState {
    let mut s = s.clone();
    for id in &s.removed_choices {
        if let Some(n) = s.nodes.get_mut(id) {
            n.deleted = true;
        }
    }
    s
}
fn location(n: &MaterializedNode) -> Value {
    json!({"parent_id":n.parent_id,"pos":n.pos})
}
fn value(n: Option<&MaterializedNode>) -> Value {
    n.map_or(Value::Null, |n| json!(n))
}
fn select(
    id: &NodeId,
    field: &str,
    base: Value,
    team: Value,
    draft: Value,
    resolutions: &BTreeMap<String, String>,
    conflicts: &mut Vec<Conflict>,
) -> Value {
    if draft == base || draft == team {
        return team;
    }
    if team == base {
        return draft;
    }
    if field == "content" {
        if let Some(merged) = crate::richtext::merge(&base, &team, &draft) {
            return merged;
        }
    }
    let key = format!("{}:{field}", id.0);
    match resolutions.get(&key).map(String::as_str) {
        Some("draft") => draft,
        Some("team") => team,
        _ => {
            conflicts.push(Conflict {
                key,
                node_id: id.clone(),
                field: field.into(),
                ancestor: base,
                team: team.clone(),
                draft,
            });
            team
        }
    }
}
pub fn merge(
    base: &DocumentState,
    team: &DocumentState,
    draft: &DocumentState,
    resolutions: &BTreeMap<String, String>,
) -> MergePlan {
    let base = canonical(base);
    let team = canonical(team);
    let draft = canonical(draft);
    let mut out = team.clone();
    let mut conflicts = Vec::new();
    for (id, d) in &draft.nodes {
        let b = base.nodes.get(id);
        let t = team.nodes.get(id);
        if b == Some(d) {
            continue;
        }
        // Existence or a tombstone crosses the entire node, so delete/edit cannot
        // be silently merged by treating the deleted flag as an independent field.
        if b.is_none()
            || t.is_none()
            || b.is_some_and(|b| b.deleted != d.deleted)
            || t.zip(b).is_some_and(|(t, b)| t.deleted != b.deleted)
        {
            let picked = select(
                id,
                "$node",
                value(b),
                value(t),
                value(Some(d)),
                resolutions,
                &mut conflicts,
            );
            if let Ok(node) = serde_json::from_value(picked) {
                out.nodes.insert(id.clone(), node);
            }
            continue;
        }
        let b = b.unwrap();
        let t = t.unwrap();
        let mut n = t.clone();
        let loc = select(
            id,
            "$location",
            location(b),
            location(t),
            location(d),
            resolutions,
            &mut conflicts,
        );
        n.parent_id = serde_json::from_value(loc["parent_id"].clone()).unwrap_or(None);
        n.pos = loc["pos"].as_str().unwrap_or(&t.pos).into();
        n.var_name = serde_json::from_value(select(
            id,
            "var_name",
            json!(b.var_name),
            json!(t.var_name),
            json!(d.var_name),
            resolutions,
            &mut conflicts,
        ))
        .unwrap_or(None);
        let keys: BTreeSet<_> = b
            .current_fields
            .as_object()
            .into_iter()
            .flat_map(|o| o.keys())
            .chain(
                d.current_fields
                    .as_object()
                    .into_iter()
                    .flat_map(|o| o.keys()),
            )
            .collect();
        for key in keys {
            if key == "label"
                && b.current_fields["content"].is_object()
                && t.current_fields["content"].is_object()
                && d.current_fields["content"].is_object()
            {
                continue;
            }
            let v = select(
                id,
                key,
                b.current_fields[key].clone(),
                t.current_fields[key].clone(),
                d.current_fields[key].clone(),
                resolutions,
                &mut conflicts,
            );
            if !v.is_null() || n.current_fields.get(key).is_some() {
                n.current_fields[key] = v;
            }
        }
        if b.current_fields["content"].is_object()
            && t.current_fields["content"].is_object()
            && d.current_fields["content"].is_object()
        {
            n.current_fields["label"] =
                json!(crate::richtext::plain_text(&n.current_fields["content"]));
        }
        out.nodes.insert(id.clone(), n);
    }
    out.removed_choices
        .retain(|id| out.nodes.get(id).is_some_and(|n| n.deleted));
    let ops = diff(&team, &out);
    MergePlan {
        state: out,
        conflicts,
        ops,
    }
}
fn depth(state: &DocumentState, n: &MaterializedNode) -> usize {
    let mut d = 0;
    let mut p = n.parent_id.as_ref();
    while let Some(id) = p {
        d += 1;
        if d > state.nodes.len() {
            break;
        }
        p = state.nodes.get(id).and_then(|n| n.parent_id.as_ref());
    }
    d
}
pub fn diff(from: &DocumentState, to: &DocumentState) -> Vec<EventPayload> {
    let from = canonical(from);
    let to = canonical(to);
    let mut result = Vec::new();
    let mut nodes: Vec<_> = to.nodes.values().collect();
    nodes.sort_by_key(|n| (depth(&to, n), n.id.clone()));
    for n in nodes.into_iter().filter(|n| !n.deleted) {
        let prior = from.nodes.get(&n.id);
        if prior.is_none() {
            if let Ok(node_type) = n.node_type.parse() {
                result.push(EventPayload::NodeCreated {
                    node_id: n.id.clone(),
                    node_type,
                    parent_id: n.parent_id.clone(),
                    pos: n.pos.clone(),
                    fields: n.current_fields.clone(),
                    var_name: n.var_name.clone(),
                });
            }
            continue;
        }
        let p = prior.unwrap();
        if p.deleted {
            result.push(EventPayload::NodeRestored {
                node_id: n.id.clone(),
            });
        }
        if p.parent_id != n.parent_id || p.pos != n.pos {
            result.push(EventPayload::NodeMoved {
                node_id: n.id.clone(),
                new_parent_id: n.parent_id.clone(),
                new_pos: n.pos.clone(),
            });
        }
        let keys: BTreeSet<_> = p
            .current_fields
            .as_object()
            .into_iter()
            .flat_map(|o| o.keys())
            .chain(
                n.current_fields
                    .as_object()
                    .into_iter()
                    .flat_map(|o| o.keys()),
            )
            .collect();
        let mut derived_label = None;
        for field in keys {
            let before = if field == "label" {
                derived_label.as_ref().unwrap_or(&p.current_fields[field])
            } else {
                &p.current_fields[field]
            };
            if *before != n.current_fields[field] {
                let op = EventPayload::FieldEdited {
                    node_id: n.id.clone(),
                    field: field.clone(),
                    value: n.current_fields[field].clone(),
                };
                let compact = crate::richtext::compact_event(&from, &op);
                if matches!(compact, EventPayload::RichTextPatched { .. }) {
                    derived_label = Some(json!(crate::richtext::plain_text(
                        &n.current_fields["content"]
                    )));
                }
                result.push(compact);
            }
        }
        if p.var_name != n.var_name {
            result.push(EventPayload::FieldEdited {
                node_id: n.id.clone(),
                field: "var_name".into(),
                value: json!(n.var_name.as_deref().unwrap_or("")),
            });
        }
    }
    let mut removed: Vec<_> = from
        .nodes
        .values()
        .filter(|n| !n.deleted && to.nodes.get(&n.id).is_none_or(|n| n.deleted))
        .collect();
    removed.sort_by_key(|n| std::cmp::Reverse(depth(&from, n)));
    for n in removed {
        result.push(EventPayload::NodeDeleted {
            node_id: n.id.clone(),
        });
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> DocumentState {
        let mut s = DocumentState::default();
        s.nodes.insert(
            NodeId("root".into()),
            MaterializedNode {
                id: NodeId("root".into()),
                parent_id: None,
                node_type: "form".into(),
                pos: "a0".into(),
                current_fields: json!({"a":1,"b":1}),
                var_name: None,
                deleted: false,
            },
        );
        s
    }
    #[test]
    fn separate_cells_merge_and_overlaps_require_a_choice() {
        let b = state();
        let mut t = b.clone();
        let mut d = b.clone();
        t.nodes
            .get_mut(&NodeId("root".into()))
            .unwrap()
            .current_fields["a"] = json!(2);
        d.nodes
            .get_mut(&NodeId("root".into()))
            .unwrap()
            .current_fields["b"] = json!(3);
        let p = merge(&b, &t, &d, &BTreeMap::new());
        assert!(p.conflicts.is_empty());
        assert_eq!(p.ops.len(), 1);
        d.nodes
            .get_mut(&NodeId("root".into()))
            .unwrap()
            .current_fields["a"] = json!(4);
        let p = merge(&b, &t, &d, &BTreeMap::new());
        assert_eq!(p.conflicts.len(), 1);
        let p = merge(
            &b,
            &t,
            &d,
            &BTreeMap::from([("root:a".into(), "draft".into())]),
        );
        assert!(p.conflicts.is_empty());
        assert_eq!(p.state.nodes[&NodeId("root".into())].current_fields["a"], 4);
    }
    #[test]
    fn deletion_and_editing_conflict() {
        let b = state();
        let mut t = b.clone();
        let mut d = b.clone();
        t.nodes.get_mut(&NodeId("root".into())).unwrap().deleted = true;
        d.nodes
            .get_mut(&NodeId("root".into()))
            .unwrap()
            .current_fields["a"] = json!(2);
        assert_eq!(merge(&b, &t, &d, &BTreeMap::new()).conflicts.len(), 1);
    }
    #[test]
    fn restoration_keeps_the_id() {
        let b = state();
        let mut deleted = b.clone();
        deleted
            .nodes
            .get_mut(&NodeId("root".into()))
            .unwrap()
            .deleted = true;
        assert!(
            matches!(&diff(&deleted,&b)[0],EventPayload::NodeRestored{node_id} if node_id.0=="root")
        );
    }
}
