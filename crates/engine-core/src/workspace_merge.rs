//! State-based three-way merge for document blocks and spreadsheet cells.
//! Deletion versus editing is a conflict, including structural edits. Canonical
//! history is never replaced: `diff` emits new semantic operations only.
use crate::materializer::{DocumentState, MaterializedNode};
use engine_shared::{EventPayload, NodeId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::borrow::Cow;
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
fn canonical(s: &DocumentState) -> Cow<'_, DocumentState> {
    if s.removed_choices.is_empty() {
        return Cow::Borrowed(s);
    }
    let mut s = s.clone();
    for id in &s.removed_choices {
        if let Some(n) = s.nodes.get_mut(id) {
            n.deleted = true;
        }
    }
    Cow::Owned(s)
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
    // Page setup holds independent settings, so it merges key by key.
    if field == "page_setup" {
        let base = (!base.is_null()).then_some(&base);
        if let Ok(Some(merged)) = crate::richtext::merge_property(base, Some(&team), Some(&draft)) {
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
    let mut out = team.clone().into_owned();
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
/// Merge for content that does not share history with the team version, such as a
/// different file uploaded under the same name. Besides ordinary overlaps, every
/// change to a block that already exists in the team version needs an explicit
/// choice (`"<node>:$block"` → `"team"` or `"draft"`), as Git asks for both sides of
/// an add/add conflict. New blocks merge without a choice.
pub fn merge_strict(
    base: &DocumentState,
    team: &DocumentState,
    draft: &DocumentState,
    resolutions: &BTreeMap<String, String>,
) -> MergePlan {
    let mut plan = merge(base, team, draft, resolutions);
    let canonical_team = canonical(team);
    let canonical_draft = canonical(draft);
    let touched: BTreeSet<NodeId> = diff(base, draft)
        .iter()
        .filter_map(|op| op.target_node_id().cloned())
        .collect();
    let already: BTreeSet<NodeId> = plan.conflicts.iter().map(|c| c.node_id.clone()).collect();
    for id in touched.difference(&already) {
        let Some(t) = canonical_team.nodes.get(id).filter(|t| !t.deleted) else {
            continue;
        };
        let d = canonical_draft.nodes.get(id);
        if d == Some(t) || t.parent_id.is_none() {
            continue;
        }
        let key = format!("{}:$block", id.0);
        match resolutions.get(&key).map(String::as_str) {
            Some("draft") => {}
            choice => {
                if choice != Some("team") {
                    plan.conflicts.push(Conflict {
                        key,
                        node_id: id.clone(),
                        field: "$block".into(),
                        ancestor: value(base.nodes.get(id)),
                        team: value(Some(t)),
                        draft: value(d),
                    });
                }
                plan.state.nodes.insert(id.clone(), t.clone());
            }
        }
    }
    plan.state
        .removed_choices
        .retain(|id| plan.state.nodes.get(id).is_some_and(|n| n.deleted));
    plan.ops = diff(team, &plan.state);
    plan
}
// Each parent chain is traversed once. Iterative traversal avoids stack growth
// for deeply nested imports; malformed cycles retain the old capped sort depth.
fn depths(state: &DocumentState) -> BTreeMap<NodeId, usize> {
    let mut depths = BTreeMap::new();
    let cap = state.nodes.len() + 1;
    for id in state.nodes.keys() {
        if depths.contains_key(id) {
            continue;
        }
        let mut path = Vec::new();
        let mut seen = BTreeSet::new();
        let mut cursor = id;
        let mut depth = loop {
            if let Some(depth) = depths.get(cursor) {
                break *depth;
            }
            let Some(node) = state.nodes.get(cursor) else {
                break 0;
            };
            let Some(parent) = &node.parent_id else {
                depths.insert(cursor.clone(), 0);
                break 0;
            };
            if !seen.insert(cursor) {
                break cap;
            }
            path.push(cursor.clone());
            cursor = parent;
        };
        for id in path.into_iter().rev() {
            depth = (depth + 1).min(cap);
            depths.insert(id, depth);
        }
    }
    depths
}
pub fn diff(from: &DocumentState, to: &DocumentState) -> Vec<EventPayload> {
    let from = canonical(from);
    let to = canonical(to);
    let mut result = Vec::new();
    let to_depths = depths(&to);
    let mut nodes: Vec<_> = to.nodes.values().collect();
    nodes.sort_by_cached_key(|n| (to_depths[&n.id], &n.id));
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
    if !removed.is_empty() {
        let from_depths = depths(&from);
        removed.sort_by_cached_key(|n| std::cmp::Reverse(from_depths[&n.id]));
    }
    for n in removed {
        result.push(EventPayload::NodeDeleted {
            node_id: n.id.clone(),
        });
    }
    // A snapshot can enable protection together with content changes. Apply that
    // flag last; explicit unprotection of an existing live sheet must happen first.
    // Never implicitly unprotect an already-protected worksheet.
    let mut unlocks = Vec::new();
    let mut content = Vec::new();
    let mut locks = Vec::new();
    for mut op in result {
        match &mut op {
            EventPayload::NodeCreated {
                node_id, fields, ..
            } if fields["kind"] == "sheet" && fields["sheet_protected"] == true => {
                fields["sheet_protected"] = json!(false);
                locks.push(EventPayload::FieldEdited {
                    node_id: node_id.clone(),
                    field: "sheet_protected".into(),
                    value: json!(true),
                });
            }
            EventPayload::FieldEdited {
                node_id,
                field,
                value,
            } if field == "sheet_protected" => {
                if *value == true {
                    locks.push(op);
                    continue;
                }
                if (value.is_null() || *value == false)
                    && from.nodes.get(node_id).is_some_and(|n| !n.deleted)
                {
                    unlocks.push(op);
                    continue;
                }
            }
            _ => {}
        }
        content.push(op);
    }
    unlocks.extend(content);
    unlocks.extend(locks);
    unlocks
}
/// Extends a selection of changed nodes with what their operations need to apply:
/// a parent that `ops` creates, restores or moves into place, and the children
/// that must be deleted or moved out before a selected node is deleted.
pub fn with_dependencies<'a>(
    ops: &'a [EventPayload],
    team: &DocumentState,
    mut selected: BTreeSet<&'a str>,
) -> BTreeSet<&'a str> {
    let live = |id: &NodeId| {
        team.nodes
            .get(id)
            .is_some_and(|n| !n.deleted && !team.removed_choices.contains(id))
    };
    let mut by_node: BTreeMap<&str, Vec<&EventPayload>> = BTreeMap::new();
    for op in ops {
        if let Some(id) = op.target_node_id() {
            by_node.entry(id.0.as_str()).or_default().push(op);
        }
    }
    let mut children: BTreeMap<&NodeId, Vec<&NodeId>> = BTreeMap::new();
    for n in team.nodes.values().filter(|n| live(&n.id)) {
        if let Some(parent) = &n.parent_id {
            children.entry(parent).or_default().push(&n.id);
        }
    }
    let mut pending: Vec<&str> = selected.iter().copied().collect();
    while let Some(id) = pending.pop() {
        for op in by_node.get(id).into_iter().flatten() {
            let parent = match op {
                EventPayload::NodeCreated {
                    parent_id: Some(p), ..
                }
                | EventPayload::NodeMoved {
                    new_parent_id: Some(p),
                    ..
                } => Some(p),
                EventPayload::NodeRestored { node_id } => {
                    team.nodes.get(node_id).and_then(|n| n.parent_id.as_ref())
                }
                _ => None,
            };
            let mut needs: Vec<&NodeId> = parent.into_iter().filter(|p| !live(p)).collect();
            if let EventPayload::NodeDeleted { node_id } = op {
                needs.extend(children.get(node_id).into_iter().flatten());
            }
            for need in needs {
                if let Some((&changed, _)) = by_node.get_key_value(need.0.as_str()) {
                    if selected.insert(changed) {
                        pending.push(changed);
                    }
                }
            }
        }
    }
    selected
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_depths_match_legacy_order_for_cycles_missing_parents_and_deep_trees() {
        fn legacy(state: &DocumentState, node: &MaterializedNode) -> usize {
            let mut count = 0;
            let mut parent = node.parent_id.as_ref();
            while let Some(id) = parent {
                count += 1;
                if count > state.nodes.len() {
                    break;
                }
                parent = state.nodes.get(id).and_then(|node| node.parent_id.as_ref());
            }
            count
        }
        let template = state().nodes.values().next().unwrap().clone();
        let mut random = 17u64;
        for _ in 0..100 {
            let mut graph = DocumentState::default();
            for i in 0..64 {
                random = random.wrapping_mul(6364136223846793005).wrapping_add(1);
                let mut node = template.clone();
                node.id = NodeId(i.to_string());
                let parent = (random >> 32) % 70;
                node.parent_id = (parent != 69).then(|| NodeId(parent.to_string()));
                graph.nodes.insert(node.id.clone(), node);
            }
            let actual = depths(&graph);
            for node in graph.nodes.values() {
                assert_eq!(actual[&node.id], legacy(&graph, node));
            }
        }
        let mut deep = DocumentState::default();
        for i in 0..20_000 {
            let mut node = template.clone();
            node.id = NodeId(format!("{i:026}"));
            node.parent_id = (i > 0).then(|| NodeId(format!("{:026}", i - 1)));
            deep.nodes.insert(node.id.clone(), node);
        }
        let actual = depths(&deep);
        assert_eq!(actual[&NodeId(format!("{:026}", 19_999))], 19_999);
    }
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
    fn protected_snapshot_diffs_apply_content_before_lock_and_explicit_unlock_before_edits() {
        use crate::governance::{authorize, Role};
        let base = state();
        let mut target = base.clone();
        target.nodes.insert(
            NodeId("sheet".into()),
            MaterializedNode {
                id: NodeId("sheet".into()),
                parent_id: Some(NodeId("root".into())),
                node_type: "section".into(),
                pos: "0".into(),
                current_fields: json!({"kind":"sheet","sheet_protected":true,"label":"Old"}),
                var_name: None,
                deleted: false,
            },
        );
        target.nodes.insert(
            NodeId("row".into()),
            MaterializedNode {
                id: NodeId("row".into()),
                parent_id: Some(NodeId("sheet".into())),
                node_type: "item".into(),
                pos: "0".into(),
                current_fields: json!({"cell_0":"saved"}),
                var_name: None,
                deleted: false,
            },
        );
        let apply = |from: &DocumentState, to: &DocumentState| {
            let mut state = from.clone();
            for op in diff(from, to) {
                authorize(Role::Author, &op, &state).unwrap();
                crate::materializer::apply_payload(&mut state, &op).unwrap();
            }
            state
        };
        let created = apply(&base, &target);
        assert_eq!(
            created.nodes[&NodeId("sheet".into())].current_fields["sheet_protected"],
            true
        );
        let mut unprotected = target.clone();
        unprotected
            .nodes
            .get_mut(&NodeId("sheet".into()))
            .unwrap()
            .current_fields = json!({"kind":"sheet","label":"New"});
        unprotected
            .nodes
            .get_mut(&NodeId("row".into()))
            .unwrap()
            .current_fields["cell_0"] = json!("edited");
        let unlocked = apply(&target, &unprotected);
        assert!(
            unlocked.nodes[&NodeId("sheet".into())].current_fields["sheet_protected"].is_null()
        );
        let restored = apply(&unlocked, &target);
        assert_eq!(
            restored.nodes[&NodeId("row".into())].current_fields["cell_0"],
            "saved"
        );
        assert_eq!(
            restored.nodes[&NodeId("sheet".into())].current_fields["sheet_protected"],
            true
        );
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
    fn unrelated_content_needs_a_choice_for_every_existing_block() {
        let b = state();
        let mut para = b.nodes[&NodeId("root".into())].clone();
        para.id = NodeId("p1".into());
        para.parent_id = Some(NodeId("root".into()));
        para.current_fields = json!({"label":"Team paragraph"});
        let mut base = b.clone();
        base.nodes.insert(para.id.clone(), para.clone());
        let team = base.clone();
        let mut draft = base.clone();
        draft
            .nodes
            .get_mut(&NodeId("p1".into()))
            .unwrap()
            .current_fields = json!({"label":"Their unrelated paragraph"});
        let mut added = para.clone();
        added.id = NodeId("p2".into());
        added.current_fields = json!({"label":"Their new paragraph"});
        draft.nodes.insert(added.id.clone(), added);
        // The ordinary merge would take the edit silently; the strict merge asks.
        assert!(merge(&base, &team, &draft, &BTreeMap::new())
            .conflicts
            .is_empty());
        let plan = merge_strict(&base, &team, &draft, &BTreeMap::new());
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].key, "p1:$block");
        // Until chosen, the team's paragraph stays and the new one is still offered.
        assert_eq!(plan.ops.len(), 1);
        let keep = merge_strict(
            &base,
            &team,
            &draft,
            &BTreeMap::from([("p1:$block".into(), "team".into())]),
        );
        assert!(keep.conflicts.is_empty());
        assert_eq!(keep.ops.len(), 1);
        let take = merge_strict(
            &base,
            &team,
            &draft,
            &BTreeMap::from([("p1:$block".into(), "draft".into())]),
        );
        assert!(take.conflicts.is_empty());
        assert_eq!(
            take.state.nodes[&NodeId("p1".into())].current_fields["label"],
            "Their unrelated paragraph"
        );
        assert_eq!(take.ops.len(), 2);
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

    #[test]
    fn canonical_borrows_when_empty_and_owns_when_choices_removed() {
        let mut s = state();
        assert!(matches!(canonical(&s), Cow::Borrowed(_)));
        s.removed_choices.insert(NodeId("root".into()));
        let c = canonical(&s);
        assert!(matches!(c, Cow::Owned(_)));
        assert!(c.nodes[&NodeId("root".into())].deleted);
    }
    #[test]
    fn selected_child_brings_its_new_parent() {
        let mut team = state();
        let add = |s: &mut DocumentState, id: &str, node_type: &str, parent: &str| {
            let mut n = s.nodes[&NodeId("root".into())].clone();
            n.id = NodeId(id.into());
            n.node_type = node_type.into();
            n.parent_id = Some(NodeId(parent.into()));
            s.nodes.insert(n.id.clone(), n);
        };
        add(&mut team, "moved", "item", "root");
        add(&mut team, "old", "section", "root");
        add(&mut team, "restored", "item", "old");
        add(&mut team, "gone", "section", "root");
        add(&mut team, "kept", "item", "gone");
        let mut target = team.clone();
        for id in ["old", "restored"] {
            team.nodes.get_mut(&NodeId(id.into())).unwrap().deleted = true;
        }
        for id in ["gone", "kept"] {
            target.nodes.get_mut(&NodeId(id.into())).unwrap().deleted = true;
        }
        add(&mut target, "section", "section", "root");
        add(&mut target, "child", "item", "section");
        target
            .nodes
            .get_mut(&NodeId("moved".into()))
            .unwrap()
            .parent_id = Some(NodeId("section".into()));
        let ops = diff(&team, &target);
        for (selected, expected) in [
            ("child", vec!["child", "section"]),
            ("moved", vec!["moved", "section"]),
            ("restored", vec!["old", "restored"]),
            ("gone", vec!["gone", "kept"]),
        ] {
            let included = with_dependencies(&ops, &team, BTreeSet::from([selected]));
            assert_eq!(included, BTreeSet::from_iter(expected));
            let mut shared = team.clone();
            for op in ops.iter().filter(|op| {
                op.target_node_id()
                    .is_some_and(|n| included.contains(n.0.as_str()))
            }) {
                crate::ops::validate_op(&shared, op).unwrap();
                crate::materializer::apply_payload(&mut shared, op).unwrap();
            }
        }
    }
    #[test]
    fn page_setup_merges_key_by_key() {
        let ps = json!({"size":"A4","landscape":false,"margin":25.4,"pageNumbers":true});
        let with = |page_setup: Value| {
            let mut s = state();
            s.nodes
                .get_mut(&NodeId("root".into()))
                .unwrap()
                .current_fields["page_setup"] = page_setup;
            s
        };
        let setup = |plan: &MergePlan| {
            plan.state.nodes[&NodeId("root".into())].current_fields["page_setup"].clone()
        };
        let mut margin = ps.clone();
        margin["margin"] = json!(20);
        let mut landscape = ps.clone();
        landscape["landscape"] = json!(true);
        let plan = merge(
            &with(ps.clone()),
            &with(margin.clone()),
            &with(landscape),
            &BTreeMap::new(),
        );
        assert!(plan.conflicts.is_empty());
        assert_eq!(
            setup(&plan),
            json!({"size":"A4","landscape":true,"margin":20,"pageNumbers":true})
        );
        // Both sides set it for the first time.
        let plan = merge(
            &state(),
            &with(json!({"margin":20})),
            &with(json!({"landscape":true})),
            &BTreeMap::new(),
        );
        assert!(plan.conflicts.is_empty());
        assert_eq!(setup(&plan), json!({"margin":20,"landscape":true}));
        // The same setting changed to different values still needs a choice.
        let mut other = ps.clone();
        other["margin"] = json!(30);
        let plan = merge(&with(ps), &with(margin), &with(other), &BTreeMap::new());
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].key, "root:page_setup");
    }
}
