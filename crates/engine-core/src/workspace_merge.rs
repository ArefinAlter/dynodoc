//! State-based three-way merge for document blocks and spreadsheet cells.
//! Deletion versus editing is a conflict, including structural edits. Canonical
//! history is never replaced: `diff` emits new semantic operations only.
use crate::materializer::{DocumentState, MaterializedNode};
use crate::merge_policy::{self, MergeEvidence, MergePolicy, Registry};
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
        // Two names for one variable may also be resolved with a new name.
        Some(name) if field == "var_name" && is_new_name(name) => json!(name),
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
/// Where a three-way difference is decided.
pub(crate) struct At<'a> {
    pub id: &'a NodeId,
    pub node_type: &'a str,
    pub field: &'a str,
}
/// How `merge_with` settles one field. The plain `merge` uses [`select`] directly;
/// a policy supplies strategies and records the decision.
pub(crate) trait Selector {
    fn select(
        &mut self,
        at: At<'_>,
        base: Value,
        team: Value,
        draft: Value,
        resolutions: &BTreeMap<String, String>,
        conflicts: &mut Vec<Conflict>,
    ) -> Value;
}
struct Direct;
impl Selector for Direct {
    fn select(
        &mut self,
        at: At<'_>,
        base: Value,
        team: Value,
        draft: Value,
        resolutions: &BTreeMap<String, String>,
        conflicts: &mut Vec<Conflict>,
    ) -> Value {
        select(at.id, at.field, base, team, draft, resolutions, conflicts)
    }
}
pub fn merge(
    base: &DocumentState,
    team: &DocumentState,
    draft: &DocumentState,
    resolutions: &BTreeMap<String, String>,
) -> MergePlan {
    merge_with(base, team, draft, resolutions, &mut Direct)
}
/// The same merge, with the decision for each differing field taken by the default
/// policy `dynodoc.default.v1`, which reproduces [`merge`] exactly, or by `policy`.
/// The plan has the same fields; the evidence says which rule and strategy decided
/// each field and which invariants the merged state breaks. Unless the policy asks
/// for it (`defaults.invariants: conflict`, or a gate that is not `auto`) the plan
/// is the one `merge` returns.
pub fn merge_with_policy(
    base: &DocumentState,
    team: &DocumentState,
    draft: &DocumentState,
    resolutions: &BTreeMap<String, String>,
    policy: &MergePolicy,
) -> (MergePlan, MergeEvidence) {
    merge_with_registry(base, team, draft, resolutions, policy, &Registry::builtin())
}
/// [`merge_with_policy`] with the strategies of `registry`. A rule that names a
/// strategy the registry lacks makes its decisions conflicts.
pub fn merge_with_registry(
    base: &DocumentState,
    team: &DocumentState,
    draft: &DocumentState,
    resolutions: &BTreeMap<String, String>,
    policy: &MergePolicy,
    registry: &Registry,
) -> (MergePlan, MergeEvidence) {
    let family = merge_policy::Family::of(&[base, team, draft]);
    let mut engine = merge_policy::engine::Engine::new(policy, registry, family);
    let mut plan = merge_with(base, team, draft, resolutions, &mut engine);
    let mut evidence = engine.finish();
    merge_policy::invariants::apply(
        &mut plan,
        &canonical(team),
        &canonical(draft),
        policy.defaults.invariants,
        &mut evidence,
    );
    (plan, evidence)
}
fn merge_with<S: Selector>(
    base: &DocumentState,
    team: &DocumentState,
    draft: &DocumentState,
    resolutions: &BTreeMap<String, String>,
    selector: &mut S,
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
            let picked = selector.select(
                At {
                    id,
                    node_type: &d.node_type,
                    field: "$node",
                },
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
        let at = |field| At {
            id,
            node_type: &t.node_type,
            field,
        };
        let loc = selector.select(
            at("$location"),
            location(b),
            location(t),
            location(d),
            resolutions,
            &mut conflicts,
        );
        n.parent_id = serde_json::from_value(loc["parent_id"].clone()).unwrap_or(None);
        n.pos = loc["pos"].as_str().unwrap_or(&t.pos).into();
        n.var_name = serde_json::from_value(selector.select(
            at("var_name"),
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
            // Mirrors `var_name`, which is merged above.
            if key == "var_name" || key == "varName" {
                if n.var_name != t.var_name {
                    n.current_fields["var_name"] = json!(n.var_name);
                }
                continue;
            }
            if key == "label"
                && b.current_fields["content"].is_object()
                && t.current_fields["content"].is_object()
                && d.current_fields["content"].is_object()
            {
                continue;
            }
            let v = selector.select(
                at(key),
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
    validate_merged_tree(&base, &team, &draft, &mut out, &mut conflicts, resolutions);
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
    validate_merged_tree(
        base,
        team,
        draft,
        &mut plan.state,
        &mut plan.conflicts,
        resolutions,
    );
    plan.ops = diff(team, &plan.state);
    plan
}

/// The first parent cycle, walking each parent chain once without copying ids.
pub(crate) fn find_parent_cycle(state: &DocumentState) -> Option<Vec<NodeId>> {
    let mut visited: BTreeSet<&NodeId> = BTreeSet::new();
    for start in state.nodes.keys() {
        let (mut path, mut on_path) = (Vec::new(), BTreeSet::new());
        let mut cursor = Some(start);
        while let Some(id) = cursor.filter(|id| !visited.contains(id)) {
            if !on_path.insert(id) {
                let at = path.iter().position(|p| *p == id).unwrap_or(0);
                return Some(
                    path[at..]
                        .iter()
                        .map(|id: &&NodeId| (*id).clone())
                        .collect(),
                );
            }
            path.push(id);
            cursor = state.nodes.get(id).and_then(|n| n.parent_id.as_ref());
        }
        visited.extend(path);
    }
    None
}

fn validate_merged_tree(
    base: &DocumentState,
    team: &DocumentState,
    draft: &DocumentState,
    out: &mut DocumentState,
    conflicts: &mut Vec<Conflict>,
    resolutions: &BTreeMap<String, String>,
) {
    let max_passes = 2 * out.nodes.len() + 1;
    let mut passes = 0;
    // Meeting the same repair twice means two explicit choices undo each other, so
    // the conflict is reported at once instead of looping to the pass budget.
    let mut seen: BTreeSet<(u8, NodeId, Option<NodeId>)> = BTreeSet::new();
    loop {
        passes += 1;
        let Some(cycle) = find_parent_cycle(out) else {
            let changed = if let Some((child, parent)) = find_orphan(out) {
                if passes > max_passes || !seen.insert((0, child.clone(), Some(parent.clone()))) {
                    let conflict = orphan_conflict(base, team, draft, &parent);
                    if !conflicts.iter().any(|c| c.key == conflict.key) {
                        conflicts.push(conflict);
                    }
                    break;
                }
                resolve_orphan(
                    base,
                    team,
                    draft,
                    out,
                    conflicts,
                    resolutions,
                    &child,
                    &parent,
                )
            } else if let Some(id) = find_duplicate_var_name(team, out) {
                if passes > max_passes || !seen.insert((1, id.clone(), None)) {
                    let conflict = var_name_conflict(base, team, draft, &id);
                    if !conflicts.iter().any(|c| c.key == conflict.key) {
                        conflicts.push(conflict);
                    }
                    break;
                }
                resolve_duplicate_var_name(base, team, draft, out, conflicts, resolutions, &id)
            } else {
                break;
            };
            if !changed {
                break;
            }
            continue;
        };

        // Pick a node in the loop whose merged location came from the draft (out_loc != team_loc).
        // A valid team tree has no loops, so every loop has such a node, and restoring
        // its location to team's location makes progress toward team's acyclic tree.
        // Mixed explicit choices can undo that progress, so the budget must fail closed.
        // A cycle through a draft-new node always holds a moved node the team also has;
        // choose that one, since a node the team lacks has no team location to restore.
        let moved =
            |id: &&NodeId| out.nodes.get(*id).map(location) != team.nodes.get(*id).map(location);
        let d_id = cycle
            .iter()
            .find(|id| team.nodes.contains_key(*id) && moved(id))
            .or_else(|| cycle.iter().find(moved))
            .unwrap_or(&cycle[0])
            .clone();

        let key = format!("{}:$location", d_id.0);
        let b_loc = base.nodes.get(&d_id).map(location).unwrap_or(Value::Null);
        let t_loc = team.nodes.get(&d_id).map(location).unwrap_or(Value::Null);
        let d_loc = draft.nodes.get(&d_id).map(location).unwrap_or(Value::Null);
        let conflict = Conflict {
            key: key.clone(),
            node_id: d_id.clone(),
            field: "$location".into(),
            ancestor: b_loc,
            team: t_loc,
            draft: d_loc,
        };
        if passes > max_passes || !seen.insert((2, d_id.clone(), None)) {
            // Every pass may change a location yet alternate between cycles.
            // A resolution is not successful while any parent cycle remains.
            if !conflicts.iter().any(|c| c.key == key) {
                conflicts.push(conflict);
            }
            break;
        }
        let locations = |out: &DocumentState| -> Vec<_> {
            cycle
                .iter()
                .map(|id| out.nodes.get(id).map(location))
                .collect()
        };
        let before = locations(out);

        match resolutions.get(&key).map(String::as_str) {
            Some("draft") => {
                if let Some(d_node) = draft.nodes.get(&d_id) {
                    if let Some(out_node) = out.nodes.get_mut(&d_id) {
                        out_node.parent_id = d_node.parent_id.clone();
                        out_node.pos = d_node.pos.clone();
                    }
                }
                for other_id in &cycle {
                    if other_id != &d_id {
                        if let Some(d_node) = draft.nodes.get(other_id) {
                            if let Some(out_node) = out.nodes.get_mut(other_id) {
                                out_node.parent_id = d_node.parent_id.clone();
                                out_node.pos = d_node.pos.clone();
                            }
                        }
                    }
                }
            }
            Some("team") => {
                if let Some(t_node) = team.nodes.get(&d_id) {
                    if let Some(out_node) = out.nodes.get_mut(&d_id) {
                        out_node.parent_id = t_node.parent_id.clone();
                        out_node.pos = t_node.pos.clone();
                    }
                }
            }
            _ => {
                if !conflicts.iter().any(|c| c.key == key) {
                    conflicts.push(conflict.clone());
                }
                if let Some(t_node) = team.nodes.get(&d_id) {
                    if let Some(out_node) = out.nodes.get_mut(&d_id) {
                        out_node.parent_id = t_node.parent_id.clone();
                        out_node.pos = t_node.pos.clone();
                    }
                }
            }
        }
        // A loop no choice here can break, such as one already in the team version,
        // is reported once instead of being retried.
        if locations(out) == before {
            if !conflicts.iter().any(|c| c.key == key) {
                conflicts.push(conflict);
            }
            break;
        }
    }
}
/// A live node whose parent is deleted or missing.
fn find_orphan(state: &DocumentState) -> Option<(NodeId, NodeId)> {
    let live = |id: &NodeId| state.nodes.get(id).is_some_and(|n| !n.deleted);
    state.nodes.values().find_map(|n| {
        let parent = n.parent_id.as_ref().filter(|p| !n.deleted && !live(p))?;
        Some((n.id.clone(), parent.clone()))
    })
}
fn orphan_conflict(
    base: &DocumentState,
    team: &DocumentState,
    draft: &DocumentState,
    parent: &NodeId,
) -> Conflict {
    Conflict {
        key: format!("{}:$node", parent.0),
        node_id: parent.clone(),
        field: "$node".into(),
        ancestor: value(base.nodes.get(parent)),
        team: value(team.nodes.get(parent)),
        draft: value(draft.nodes.get(parent)),
    }
}
/// One side deleted `parent` while the other placed `child` inside it. Like a
/// deletion against an edit, this is a `$node` conflict on the parent. The chosen
/// side's version either brings the parent back, with any deleted ancestors in
/// that version, or takes the block back out. Returns whether anything changed; if
/// not, the conflict is reported.
#[allow(clippy::too_many_arguments)]
fn resolve_orphan(
    base: &DocumentState,
    team: &DocumentState,
    draft: &DocumentState,
    out: &mut DocumentState,
    conflicts: &mut Vec<Conflict>,
    resolutions: &BTreeMap<String, String>,
    child: &NodeId,
    parent: &NodeId,
) -> bool {
    let conflict = orphan_conflict(base, team, draft, parent);
    let key = conflict.key.clone();
    let side = match resolutions.get(&key).map(String::as_str) {
        Some("draft") => draft,
        Some("team") => team,
        _ => {
            if !conflicts.iter().any(|c| c.key == key) {
                conflicts.push(conflict.clone());
            }
            team
        }
    };
    let before = (
        out.nodes.get(child).cloned(),
        out.nodes.get(parent).cloned(),
    );
    let live = |s: &DocumentState, id: &NodeId| s.nodes.get(id).filter(|n| !n.deleted).cloned();
    if live(side, parent).is_some() {
        // The chosen side's tree is valid, so its ancestors lead to a live node.
        let mut cursor = Some(parent.clone());
        while let Some(id) = cursor.filter(|id| live(out, id).is_none()) {
            let Some(n) = live(side, &id) else {
                break;
            };
            cursor = n.parent_id.clone();
            out.removed_choices.remove(&id);
            out.nodes.insert(id, n);
        }
    } else {
        // Every block placed in the missing container leaves it in this one pass, so
        // M orphans cost one scan instead of M.
        let inside: Vec<NodeId> = out
            .nodes
            .values()
            .filter(|n| !n.deleted && n.parent_id.as_ref() == Some(parent))
            .map(|n| n.id.clone())
            .collect();
        for id in inside {
            let Some(n) = out.nodes.get_mut(&id) else {
                continue;
            };
            match side.nodes.get(&id).filter(|s| !s.deleted) {
                Some(s) => {
                    n.parent_id = s.parent_id.clone();
                    n.pos = s.pos.clone();
                }
                None => n.deleted = true,
            }
        }
    }
    let changed = (
        out.nodes.get(child).cloned(),
        out.nodes.get(parent).cloned(),
    ) != before;
    if !changed && !conflicts.iter().any(|c| c.key == key) {
        conflicts.push(conflict);
    }
    changed
}
/// A live node sharing its `var_name` with another live node. Prefer a name
/// changed from the team, but still report clashes already present in the team.
fn find_duplicate_var_name(team: &DocumentState, state: &DocumentState) -> Option<NodeId> {
    let mut by_name: BTreeMap<&str, Vec<&MaterializedNode>> = BTreeMap::new();
    for n in state.nodes.values().filter(|n| !n.deleted) {
        if let Some(name) = n.var_name.as_deref().filter(|v| !v.is_empty()) {
            by_name.entry(name).or_default().push(n);
        }
    }
    let nodes = by_name.values().find(|nodes| nodes.len() > 1)?;
    nodes
        .iter()
        .find(|n| {
            team.nodes
                .get(&n.id)
                .is_none_or(|t| t.deleted || t.var_name != n.var_name)
        })
        .or_else(|| nodes.first())
        .map(|n| n.id.clone())
}
fn var_name_conflict(
    base: &DocumentState,
    team: &DocumentState,
    draft: &DocumentState,
    id: &NodeId,
) -> Conflict {
    let var_name = |s: &DocumentState| json!(s.nodes.get(id).and_then(|n| n.var_name.as_ref()));
    Conflict {
        key: format!("{}:var_name", id.0),
        node_id: id.clone(),
        field: "var_name".into(),
        ancestor: var_name(base),
        team: var_name(team),
        draft: var_name(draft),
    }
}
/// A resolution that is a typed variable name: not a side, not empty, no stray
/// whitespace, and bounded in length.
pub(crate) fn is_new_name(name: &str) -> bool {
    !matches!(name, "team" | "draft")
        && !name.is_empty()
        && name.trim() == name
        && name.len() <= 128
}
/// The block keeps the team's version of its name, or is left out when the team
/// has no such block. A resolution other than "team" or "draft" is a new name. Returns
/// whether anything changed.
#[allow(clippy::too_many_arguments)]
fn resolve_duplicate_var_name(
    base: &DocumentState,
    team: &DocumentState,
    draft: &DocumentState,
    out: &mut DocumentState,
    conflicts: &mut Vec<Conflict>,
    resolutions: &BTreeMap<String, String>,
    id: &NodeId,
) -> bool {
    let conflict = var_name_conflict(base, team, draft, id);
    let key = conflict.key.clone();
    let choice = resolutions.get(&key);
    let renamed = choice.filter(|name| {
        is_new_name(name)
            && !out
                .nodes
                .values()
                .any(|n| &n.id != id && !n.deleted && n.var_name.as_ref() == Some(name))
    });
    if renamed.is_none()
        && choice.map(String::as_str) != Some("team")
        && !conflicts.iter().any(|c| c.key == key)
    {
        conflicts.push(conflict.clone());
    }
    if renamed.is_none() && team.nodes.get(id).is_none_or(|t| t.deleted) {
        // Rejecting a draft-only creation must not invent a tombstone that the
        // emitted operations cannot reproduce. A rejected restore keeps the
        // team's original tombstone and fields.
        let before = out.nodes.remove(id);
        // Its draft-only children go too; left behind they would become orphans.
        let mut stack = vec![id.clone()];
        while let Some(p) = stack.pop() {
            let kids: Vec<NodeId> = out
                .nodes
                .values()
                .filter(|n| n.parent_id.as_ref() == Some(&p) && !team.nodes.contains_key(&n.id))
                .map(|n| n.id.clone())
                .collect();
            for kid in kids {
                out.nodes.remove(&kid);
                stack.push(kid);
            }
        }
        if let Some(t) = team.nodes.get(id) {
            out.nodes.insert(id.clone(), t.clone());
        }
        if team.removed_choices.contains(id) {
            out.removed_choices.insert(id.clone());
        } else {
            out.removed_choices.remove(id);
        }
        return before.as_ref() != team.nodes.get(id);
    }
    let Some(n) = out.nodes.get_mut(id) else {
        return false;
    };
    let before = (n.var_name.clone(), n.deleted);
    let team_node = team.nodes.get(id).filter(|t| !t.deleted);
    match (renamed, team_node) {
        (Some(name), _) => n.var_name = Some(name.clone()),
        (None, Some(t)) => n.var_name = t.var_name.clone(),
        (None, None) => n.deleted = true,
    }
    if let (None, Some(t), Some(fields)) = (renamed, team_node, n.current_fields.as_object_mut()) {
        if let Some(value) = t.current_fields.get("var_name") {
            fields.insert("var_name".into(), value.clone());
        } else {
            fields.remove("var_name");
        }
    } else if n.current_fields.get("var_name").is_some() {
        n.current_fields["var_name"] = json!(n.var_name);
    }
    let changed = (n.var_name.clone(), n.deleted) != before;
    if !changed && !conflicts.iter().any(|c| c.key == key) {
        conflicts.push(conflict);
    }
    changed
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
    // A block that takes a name another block holds gets it after the deletions and
    // renames that free it: (block, holder, the edit that gives the name).
    let held: BTreeMap<&str, &NodeId> = from
        .nodes
        .values()
        .filter(|n| !n.deleted)
        .filter_map(|n| Some((n.var_name.as_deref().filter(|v| !v.is_empty())?, &n.id)))
        .collect();
    let mut later: Vec<(NodeId, NodeId, EventPayload)> = Vec::new();
    let mut name_later = |n: &MaterializedNode| {
        let Some(holder) = n.var_name.as_deref().and_then(|v| held.get(v)) else {
            return false;
        };
        if *holder == &n.id {
            return false;
        }
        later.push((
            n.id.clone(),
            (*holder).clone(),
            EventPayload::FieldEdited {
                node_id: n.id.clone(),
                field: "var_name".into(),
                value: json!(n.var_name),
            },
        ));
        true
    };
    let to_depths = depths(&to);
    let mut nodes: Vec<_> = to.nodes.values().collect();
    nodes.sort_by_cached_key(|n| (to_depths[&n.id], &n.id));
    for n in nodes.into_iter().filter(|n| !n.deleted) {
        let prior = from.nodes.get(&n.id);
        if prior.is_none() {
            if let Ok(node_type) = n.node_type.parse() {
                let mut fields = n.current_fields.clone();
                let var_name = if name_later(n) {
                    fields.as_object_mut().map(|f| f.remove("var_name"));
                    None
                } else {
                    n.var_name.clone()
                };
                result.push(EventPayload::NodeCreated {
                    node_id: n.id.clone(),
                    node_type,
                    parent_id: n.parent_id.clone(),
                    pos: n.pos.clone(),
                    fields,
                    var_name,
                });
            }
            continue;
        }
        let p = prior.unwrap();
        // Removed choices cannot move or be edited until explicitly restored.
        // Ordinary tombstones still move/rename before their restore.
        let restored_choice = p.deleted && from.removed_choices.contains(&n.id);
        if restored_choice {
            result.push(EventPayload::NodeRestored {
                node_id: n.id.clone(),
            });
        }
        let renamed_later = (p.var_name != n.var_name || p.deleted) && name_later(n);
        if p.parent_id != n.parent_id || p.pos != n.pos {
            result.push(EventPayload::NodeMoved {
                node_id: n.id.clone(),
                new_parent_id: n.parent_id.clone(),
                new_pos: n.pos.clone(),
            });
        }
        // A restore checks the parent and name the block comes back with, so it moves
        // and takes its name first; a name another block still holds comes later.
        if p.deleted {
            let name = if renamed_later {
                ""
            } else {
                n.var_name.as_deref().unwrap_or("")
            };
            if name != p.var_name.as_deref().unwrap_or("") {
                result.push(EventPayload::FieldEdited {
                    node_id: n.id.clone(),
                    field: "var_name".into(),
                    value: json!(name),
                });
            }
            if !restored_choice {
                result.push(EventPayload::NodeRestored {
                    node_id: n.id.clone(),
                });
            }
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
            // The `var_name` edit below sets this field too.
            if field == "var_name" || field == "varName" {
                continue;
            }
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
        if p.var_name != n.var_name && !renamed_later && !p.deleted {
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
    // A name is free once its holder is deleted or renamed. Each block waits on at most
    // one holder, so release them in that order; only a cycle (two names swapped) is
    // left over, and still fails.
    let order = {
        let waiting: BTreeSet<&NodeId> = later.iter().map(|(id, _, _)| id).collect();
        let mut dependents: BTreeMap<&NodeId, Vec<usize>> = BTreeMap::new();
        let mut ready = std::collections::VecDeque::new();
        for (i, (_, holder, _)) in later.iter().enumerate() {
            if waiting.contains(holder) {
                dependents.entry(holder).or_default().push(i);
            } else {
                ready.push_back(i);
            }
        }
        let mut order = Vec::with_capacity(later.len());
        while let Some(i) = ready.pop_front() {
            order.push(i);
            ready.extend(dependents.remove(&later[i].0).into_iter().flatten());
        }
        let placed: BTreeSet<usize> = order.iter().copied().collect();
        order.extend((0..later.len()).filter(|i| !placed.contains(i)));
        order
    };
    let mut ops: Vec<_> = later.into_iter().map(|(_, _, op)| Some(op)).collect();
    result.extend(order.into_iter().filter_map(|i| ops[i].take()));
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

    #[test]
    fn cross_moves_conflict_instead_of_cycle() {
        use crate::ops::validate_op;

        let mut b = state();
        let mut a_node = b.nodes[&NodeId("root".into())].clone();
        a_node.id = NodeId("a".into());
        a_node.node_type = "section".into();
        a_node.parent_id = Some(NodeId("root".into()));
        let mut b_node = a_node.clone();
        b_node.id = NodeId("b".into());
        b.nodes.insert(a_node.id.clone(), a_node);
        b.nodes.insert(b_node.id.clone(), b_node);

        let mut t = b.clone();
        t.nodes.get_mut(&NodeId("a".into())).unwrap().parent_id = Some(NodeId("b".into()));

        let mut d = b.clone();
        d.nodes.get_mut(&NodeId("b".into())).unwrap().parent_id = Some(NodeId("a".into()));

        // Unresolved preview surfaces the location conflict on b and keeps state acyclic
        let plan = merge(&b, &t, &d, &BTreeMap::new());
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].key, "b:$location");
        assert!(find_parent_cycle(&plan.state).is_none());

        // Choosing team keeps team's location for b and remains acyclic
        let keep_team = merge(
            &b,
            &t,
            &d,
            &BTreeMap::from([("b:$location".into(), "team".into())]),
        );
        assert!(keep_team.conflicts.is_empty());
        assert!(find_parent_cycle(&keep_team.state).is_none());

        // Choosing draft moves b under a and safely reverts opposing team move on a
        let take_draft = merge(
            &b,
            &t,
            &d,
            &BTreeMap::from([("b:$location".into(), "draft".into())]),
        );
        assert!(take_draft.conflicts.is_empty());
        assert!(find_parent_cycle(&take_draft.state).is_none());
        assert_eq!(
            take_draft.state.nodes[&NodeId("b".into())].parent_id,
            Some(NodeId("a".into()))
        );

        // Applying take_draft ops to team validates and produces a valid state
        let mut applied = t.clone();
        for op in &take_draft.ops {
            validate_op(&applied, op).unwrap();
            crate::materializer::apply_payload(&mut applied, op).unwrap();
        }
        assert!(find_parent_cycle(&applied).is_none());
    }

    #[test]
    fn three_node_cycle_conflict_blames_divergent_move() {
        use crate::ops::validate_op;

        let mut b = state();
        let mut a_node = b.nodes[&NodeId("root".into())].clone();
        a_node.id = NodeId("a".into());
        a_node.node_type = "section".into();
        a_node.parent_id = Some(NodeId("root".into()));
        let mut b_node = a_node.clone();
        b_node.id = NodeId("b".into());
        let mut c_node = a_node.clone();
        c_node.id = NodeId("c".into());
        b.nodes.insert(a_node.id.clone(), a_node);
        b.nodes.insert(b_node.id.clone(), b_node);
        b.nodes.insert(c_node.id.clone(), c_node);

        // Team moves a under b and b under c
        let mut t = b.clone();
        t.nodes.get_mut(&NodeId("a".into())).unwrap().parent_id = Some(NodeId("b".into()));
        t.nodes.get_mut(&NodeId("b".into())).unwrap().parent_id = Some(NodeId("c".into()));

        // Draft also moves a under b, and moves c under a
        let mut d = b.clone();
        d.nodes.get_mut(&NodeId("a".into())).unwrap().parent_id = Some(NodeId("b".into()));
        d.nodes.get_mut(&NodeId("c".into())).unwrap().parent_id = Some(NodeId("a".into()));

        // Both moved a under b identically; only c diverged from team's tree.
        // The conflict must blame c:$location, not a.
        let plan = merge(&b, &t, &d, &BTreeMap::new());
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].key, "c:$location");
        assert!(find_parent_cycle(&plan.state).is_none());

        // Unresolved preview maintains a valid state that passes validate_op
        let mut preview_applied = t.clone();
        for op in &plan.ops {
            validate_op(&preview_applied, op).unwrap();
            crate::materializer::apply_payload(&mut preview_applied, op).unwrap();
        }

        // Choosing team keeps c at root, producing a valid acyclic tree
        let keep_team = merge(
            &b,
            &t,
            &d,
            &BTreeMap::from([("c:$location".into(), "team".into())]),
        );
        assert!(keep_team.conflicts.is_empty());
        assert!(find_parent_cycle(&keep_team.state).is_none());
        let mut team_applied = t.clone();
        for op in &keep_team.ops {
            validate_op(&team_applied, op).unwrap();
            crate::materializer::apply_payload(&mut team_applied, op).unwrap();
        }

        // Choosing draft moves c under a, undoes the team's move of b and keeps a under b
        let take_draft = merge(
            &b,
            &t,
            &d,
            &BTreeMap::from([("c:$location".into(), "draft".into())]),
        );
        assert!(take_draft.conflicts.is_empty());
        assert!(find_parent_cycle(&take_draft.state).is_none());
        assert_eq!(
            take_draft.state.nodes[&NodeId("c".into())].parent_id,
            Some(NodeId("a".into()))
        );
        assert_eq!(
            take_draft.state.nodes[&NodeId("b".into())].parent_id,
            Some(NodeId("root".into()))
        );
        assert_eq!(
            take_draft.state.nodes[&NodeId("a".into())].parent_id,
            Some(NodeId("b".into()))
        );
        let mut draft_applied = t.clone();
        for op in &take_draft.ops {
            validate_op(&draft_applied, op).unwrap();
            crate::materializer::apply_payload(&mut draft_applied, op).unwrap();
        }
        assert!(find_parent_cycle(&draft_applied).is_none());
    }

    #[test]
    fn incompatible_cycle_resolutions_remain_conflicted() {
        let mut base = state();
        for (id, parent) in [("a", "b"), ("b", "root"), ("c", "b")] {
            let mut node = base.nodes[&NodeId("root".into())].clone();
            node.id = NodeId(id.into());
            node.node_type = "section".into();
            node.parent_id = Some(NodeId(parent.into()));
            base.nodes.insert(node.id.clone(), node);
        }
        let mut team = base.clone();
        for (id, parent) in [("a", "c"), ("b", "c"), ("c", "root")] {
            team.nodes.get_mut(&NodeId(id.into())).unwrap().parent_id = Some(NodeId(parent.into()));
        }
        let mut draft = base.clone();
        draft.nodes.get_mut(&NodeId("c".into())).unwrap().parent_id = Some(NodeId("a".into()));
        for input in [&base, &team, &draft] {
            assert!(find_parent_cycle(input).is_none());
        }

        // Keeping a at team's location while taking c from draft alternates
        // between a -> c -> a and a -> b -> c -> a. Every pass changes
        // something, so the no-progress guard alone cannot stop this safely.
        let mut resolutions = BTreeMap::from([
            ("a:$location".into(), "team".into()),
            ("c:$location".into(), "draft".into()),
            ("c:$block".into(), "draft".into()),
        ]);
        for merge_fn in [merge, merge_strict] {
            let plan = merge_fn(&base, &team, &draft, &resolutions);
            assert!(find_parent_cycle(&plan.state).is_some());
            assert!(
                !plan.conflicts.is_empty(),
                "a remaining cycle must never be returned as a clean merge"
            );
            assert!(plan.conflicts.iter().all(|c| c.field == "$location"));
        }

        // A compatible choice still resolves normally and emits applicable ops.
        resolutions.insert("a:$location".into(), "draft".into());
        for merge_fn in [merge, merge_strict] {
            let plan = merge_fn(&base, &team, &draft, &resolutions);
            assert!(plan.conflicts.is_empty());
            assert!(find_parent_cycle(&plan.state).is_none());
            let mut applied = team.clone();
            for op in &plan.ops {
                crate::ops::validate_op(&applied, op).unwrap();
                crate::materializer::apply_payload(&mut applied, op).unwrap();
            }
            assert_eq!(applied, plan.state);
        }
    }

    #[test]
    fn a_cycle_already_in_the_team_version_is_reported_once() {
        // Typed objects can hand such a state to `converge()`; no choice here breaks it.
        let mut b = state();
        let mut a_node = b.nodes[&NodeId("root".into())].clone();
        a_node.id = NodeId("a".into());
        a_node.node_type = "section".into();
        a_node.parent_id = Some(NodeId("root".into()));
        let mut b_node = a_node.clone();
        b_node.id = NodeId("b".into());
        b.nodes.insert(a_node.id.clone(), a_node);
        b.nodes.insert(b_node.id.clone(), b_node);
        let mut t = b.clone();
        t.nodes.get_mut(&NodeId("a".into())).unwrap().parent_id = Some(NodeId("b".into()));
        t.nodes.get_mut(&NodeId("b".into())).unwrap().parent_id = Some(NodeId("a".into()));
        let mut d = b.clone();
        d.nodes
            .get_mut(&NodeId("root".into()))
            .unwrap()
            .current_fields["a"] = json!(2);

        let plan = merge(&b, &t, &d, &BTreeMap::new());
        assert_eq!(plan.conflicts.len(), 1);
        let key = plan.conflicts[0].key.clone();
        assert!(key.ends_with(":$location"));
        // A resolution cannot break it either, so the conflict stays.
        let resolved = merge(&b, &t, &d, &BTreeMap::from([(key.clone(), "team".into())]));
        assert_eq!(resolved.conflicts.len(), 1);
        assert_eq!(resolved.conflicts[0].key, key);
    }

    #[test]
    fn blocks_placed_in_a_deleted_container_conflict_with_the_deletion() {
        let id = |s: &str| NodeId(s.into());
        let add = |s: &mut DocumentState, n: &str, node_type: &str, parent: &str| {
            let mut node = s.nodes[&id("root")].clone();
            node.id = id(n);
            node.node_type = node_type.into();
            node.parent_id = Some(id(parent));
            s.nodes.insert(node.id.clone(), node);
        };
        let mut b = state();
        add(&mut b, "outer", "section", "root");
        add(&mut b, "inner", "section", "outer");
        for case in ["create", "move", "restore"] {
            let mut b = b.clone();
            match case {
                "move" => add(&mut b, "x", "item", "root"),
                "restore" => {
                    add(&mut b, "x", "item", "inner");
                    b.nodes.get_mut(&id("x")).unwrap().deleted = true;
                }
                _ => {}
            }
            let mut deleting = b.clone();
            for n in ["outer", "inner"] {
                deleting.nodes.get_mut(&id(n)).unwrap().deleted = true;
            }
            let mut placing = b.clone();
            match case {
                "create" => add(&mut placing, "x", "item", "inner"),
                "move" => placing.nodes.get_mut(&id("x")).unwrap().parent_id = Some(id("inner")),
                _ => placing.nodes.get_mut(&id("x")).unwrap().deleted = false,
            }
            for team_deletes in [true, false] {
                let (t, d) = if team_deletes {
                    (&deleting, &placing)
                } else {
                    (&placing, &deleting)
                };
                let plan = merge(&b, t, d, &BTreeMap::new());
                assert_eq!(plan.conflicts.len(), 1, "{case}");
                assert_eq!(plan.conflicts[0].key, "inner:$node");
                for choice in [None, Some("team"), Some("draft")] {
                    let resolutions = choice
                        .map(|c| BTreeMap::from([("inner:$node".into(), c.into())]))
                        .unwrap_or_default();
                    let plan = merge(&b, t, d, &resolutions);
                    assert_eq!(plan.conflicts.is_empty(), choice.is_some());
                    let mut applied = t.clone();
                    for op in &plan.ops {
                        crate::ops::validate_op(&applied, op).unwrap();
                        crate::materializer::apply_payload(&mut applied, op).unwrap();
                    }
                    let kept = (choice == Some("draft")) == team_deletes;
                    let live = |n: &str| !applied.nodes.get(&id(n)).is_none_or(|n| n.deleted);
                    assert_eq!(live("outer") && live("inner"), kept, "{case} {choice:?}");
                    let x = applied.nodes.get(&id("x"));
                    let at = x.filter(|x| !x.deleted).and_then(|x| x.parent_id.clone());
                    let expected = match (kept, case) {
                        (true, _) => Some(id("inner")),
                        (false, "move") => Some(id("root")),
                        _ => None,
                    };
                    assert_eq!(
                        at, expected,
                        "{case} {choice:?} team_deletes={team_deletes}"
                    );
                }
            }
        }
    }

    #[test]
    fn incompatible_orphan_resolutions_remain_conflicted() {
        let id = |s: &str| NodeId(s.into());
        let mut base = state();
        for name in ["a", "b", "x"] {
            let mut node = base.nodes[&id("root")].clone();
            node.id = id(name);
            node.node_type = "section".into();
            node.parent_id = Some(id("root"));
            base.nodes.insert(node.id.clone(), node);
        }
        let mut team = base.clone();
        team.nodes.get_mut(&id("b")).unwrap().deleted = true;
        team.nodes.get_mut(&id("x")).unwrap().parent_id = Some(id("a"));
        let mut draft = base.clone();
        draft.nodes.get_mut(&id("a")).unwrap().deleted = true;
        draft.nodes.get_mut(&id("x")).unwrap().parent_id = Some(id("b"));
        for input in [&base, &team, &draft] {
            assert!(find_parent_cycle(input).is_none());
            assert!(find_orphan(input).is_none());
        }

        // Both parents stay deleted, but x remains live. Fixing the orphan
        // under b moves it to a; fixing the orphan under a moves it back to b.
        let mut resolutions = BTreeMap::from([
            ("a:$node".into(), "draft".into()),
            ("b:$node".into(), "team".into()),
            ("x:$location".into(), "draft".into()),
            ("a:$block".into(), "draft".into()),
            ("x:$block".into(), "draft".into()),
        ]);
        for merge_fn in [merge, merge_strict] {
            let plan = merge_fn(&base, &team, &draft, &resolutions);
            assert!(
                !plan.conflicts.is_empty(),
                "incompatible deletion choices must remain conflicted"
            );
            assert!(plan.conflicts.iter().any(|c| c.field == "$node"));
        }

        resolutions.insert("a:$node".into(), "team".into());
        for merge_fn in [merge, merge_strict] {
            let plan = merge_fn(&base, &team, &draft, &resolutions);
            assert!(plan.conflicts.is_empty());
            assert!(find_parent_cycle(&plan.state).is_none());
            assert!(find_orphan(&plan.state).is_none());
            let mut applied = team.clone();
            for op in &plan.ops {
                crate::ops::validate_op(&applied, op).unwrap();
                crate::materializer::apply_payload(&mut applied, op).unwrap();
            }
            assert_eq!(applied, plan.state);
        }
    }

    #[test]
    fn an_orphan_already_in_the_team_version_is_reported_once() {
        let id = |s: &str| NodeId(s.into());
        let mut b = state();
        for (n, parent) in [("box", "root"), ("x", "box")] {
            let mut node = b.nodes[&id("root")].clone();
            node.id = id(n);
            node.node_type = "section".into();
            node.parent_id = Some(id(parent));
            b.nodes.insert(node.id.clone(), node);
        }
        let mut t = b.clone();
        t.nodes.get_mut(&id("box")).unwrap().deleted = true;
        let mut d = b.clone();
        d.nodes.get_mut(&id("root")).unwrap().current_fields["a"] = json!(2);

        let plan = merge(&b, &t, &d, &BTreeMap::new());
        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].key, "box:$node");
        // Keeping the team's version cannot fix it either, so the conflict stays.
        let resolved = merge(
            &b,
            &t,
            &d,
            &BTreeMap::from([("box:$node".into(), "team".into())]),
        );
        assert_eq!(resolved.conflicts.len(), 1);
    }

    #[test]
    fn duplicate_names_already_in_team_remain_conflicted() {
        let id = |s: &str| NodeId(s.into());
        let mut base = state();
        for name in ["a", "b"] {
            let mut node = base.nodes[&id("root")].clone();
            node.id = id(name);
            node.node_type = "item".into();
            node.parent_id = Some(id("root"));
            node.var_name = Some(name.into());
            node.current_fields = json!({"var_name": name});
            base.nodes.insert(node.id.clone(), node);
        }
        let mut team = base.clone();
        for name in ["a", "b"] {
            let node = team.nodes.get_mut(&id(name)).unwrap();
            node.var_name = Some("q".into());
            node.current_fields["var_name"] = json!("q");
        }
        let mut draft = base.clone();
        draft.nodes.get_mut(&id("root")).unwrap().current_fields["a"] = json!(2);
        for merge_fn in [merge, merge_strict] {
            for resolutions in [
                BTreeMap::new(),
                BTreeMap::from([("a:var_name".into(), "team".into())]),
            ] {
                let plan = merge_fn(&base, &team, &draft, &resolutions);
                assert_eq!(plan.conflicts.len(), 1);
                assert_eq!(plan.conflicts[0].key, "a:var_name");
            }
            let plan = merge_fn(
                &base,
                &team,
                &draft,
                &BTreeMap::from([("a:var_name".into(), "r".into())]),
            );
            assert!(plan.conflicts.is_empty());
            let mut applied = team.clone();
            for op in &plan.ops {
                crate::ops::validate_op(&applied, op).unwrap();
                crate::materializer::apply_payload(&mut applied, op).unwrap();
            }
            assert_eq!(applied, plan.state);
        }
    }

    #[test]
    fn keeping_an_unnamed_team_block_replays_to_the_planned_state() {
        let id = |s: &str| NodeId(s.into());
        let mut base = state();
        for name in ["a", "b"] {
            let mut node = base.nodes[&id("root")].clone();
            node.id = id(name);
            node.node_type = "item".into();
            node.parent_id = Some(id("root"));
            base.nodes.insert(node.id.clone(), node);
        }
        let rename = |s: &mut DocumentState, name: &str| {
            crate::materializer::apply_payload(
                s,
                &EventPayload::FieldEdited {
                    node_id: id(name),
                    field: "var_name".into(),
                    value: json!("q"),
                },
            )
            .unwrap();
        };
        let mut team = base.clone();
        rename(&mut team, "a");
        let mut draft = base.clone();
        rename(&mut draft, "b");
        for choice in [None, Some("team"), Some("r")] {
            let resolutions = choice
                .map(|c| BTreeMap::from([("b:var_name".into(), c.into())]))
                .unwrap_or_default();
            let plan = merge(&base, &team, &draft, &resolutions);
            assert_eq!(plan.conflicts.is_empty(), choice.is_some());
            let mut applied = team.clone();
            for op in &plan.ops {
                crate::ops::validate_op(&applied, op).unwrap();
                crate::materializer::apply_payload(&mut applied, op).unwrap();
            }
            assert_eq!(applied, plan.state, "choice {choice:?}");
        }

        // Older events also accept the camel-case field. Keeping that side
        // must preserve its field representation without inventing another one.
        base.nodes.get_mut(&id("b")).unwrap().var_name = Some("base".into());
        for (team_field, draft_field) in [("varName", "var_name"), ("var_name", "varName")] {
            let mut team = base.clone();
            let mut draft = base.clone();
            for (side, field, name) in [
                (&mut team, team_field, "team_name"),
                (&mut draft, draft_field, "draft_name"),
            ] {
                crate::materializer::apply_payload(
                    side,
                    &EventPayload::FieldEdited {
                        node_id: id("b"),
                        field: field.into(),
                        value: json!(name),
                    },
                )
                .unwrap();
            }
            for choice in [None, Some("team"), Some("draft"), Some("r")] {
                let resolutions = choice
                    .map(|c| BTreeMap::from([("b:var_name".into(), c.into())]))
                    .unwrap_or_default();
                let plan = merge(&base, &team, &draft, &resolutions);
                assert_eq!(plan.conflicts.is_empty(), choice.is_some());
                let mut applied = team.clone();
                for op in &plan.ops {
                    crate::ops::validate_op(&applied, op).unwrap();
                    crate::materializer::apply_payload(&mut applied, op).unwrap();
                }
                assert_eq!(applied, plan.state, "legacy field, choice {choice:?}");
            }
        }
    }

    #[test]
    fn duplicate_var_names_from_both_sides_conflict() {
        let id = |s: &str| NodeId(s.into());
        let add = |s: &mut DocumentState, n: &str, var: &str| {
            let mut node = s.nodes[&id("root")].clone();
            node.id = id(n);
            node.node_type = "item".into();
            node.parent_id = Some(id("root"));
            node.current_fields = json!({"label": n, "var_name": var});
            node.var_name = Some(var.into());
            s.nodes.insert(node.id.clone(), node);
        };
        let rename = |s: &mut DocumentState, n: &str, var: &str| {
            let node = s.nodes.get_mut(&id(n)).unwrap();
            node.current_fields["var_name"] = json!(var);
            node.var_name = Some(var.into());
        };
        let check =
            |b: &DocumentState, t: &DocumentState, d: &DocumentState, res: &[(&str, &str)]| {
                let res = res
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect();
                let plan = merge(b, t, d, &res);
                let mut applied = t.clone();
                for op in &plan.ops {
                    crate::ops::validate_op(&applied, op).unwrap();
                    crate::materializer::apply_payload(&mut applied, op).unwrap();
                }
                assert_eq!(applied, plan.state);
                let keys: Vec<_> = plan.conflicts.iter().map(|c| c.key.clone()).collect();
                (keys, plan.ops, applied)
            };
        let var = |s: &DocumentState, n: &str| {
            s.nodes
                .get(&id(n))
                .filter(|n| !n.deleted)
                .and_then(|n| n.var_name.clone())
        };

        // Both sides add a block named q.
        let b = state();
        let mut t = b.clone();
        add(&mut t, "team_q", "q");
        let mut d = b.clone();
        add(&mut d, "draft_q", "q");
        for res in [
            &[][..],
            &[("draft_q:var_name", "draft")],
            &[("draft_q:var_name", "q")],
        ] {
            let (keys, _, applied) = check(&b, &t, &d, res);
            assert_eq!(keys, ["draft_q:var_name"]);
            assert_eq!(var(&applied, "draft_q"), None);
        }
        let (keys, _, applied) = check(&b, &t, &d, &[("draft_q:var_name", "team")]);
        assert!(keys.is_empty());
        assert_eq!(var(&applied, "draft_q"), None);
        let (keys, _, applied) = check(&b, &t, &d, &[("draft_q:var_name", "q2")]);
        assert!(keys.is_empty());
        assert_eq!(var(&applied, "draft_q").as_deref(), Some("q2"));

        // Rejecting a restored block keeps the team's existing tombstone,
        // including its old fields and name, rather than changing history.
        let mut b = state();
        add(&mut b, "draft_q", "old_name");
        b.nodes.get_mut(&id("draft_q")).unwrap().deleted = true;
        let mut t = b.clone();
        add(&mut t, "team_q", "q");
        let mut d = b.clone();
        d.nodes.get_mut(&id("draft_q")).unwrap().deleted = false;
        rename(&mut d, "draft_q", "q");
        let (keys, _, applied) = check(&b, &t, &d, &[("draft_q:var_name", "team")]);
        assert!(keys.is_empty());
        assert_eq!(applied.nodes[&id("draft_q")], t.nodes[&id("draft_q")]);
        let (keys, _, applied) = check(&b, &t, &d, &[("draft_q:var_name", "q2")]);
        assert!(keys.is_empty());
        assert_eq!(var(&applied, "draft_q").as_deref(), Some("q2"));

        // The team renames a to q while the draft renames b to q; both rename c.
        let mut b = state();
        for n in ["a", "b", "c"] {
            add(&mut b, n, n);
        }
        let mut t = b.clone();
        rename(&mut t, "a", "q");
        rename(&mut t, "c", "x");
        let mut d = b.clone();
        rename(&mut d, "b", "q");
        rename(&mut d, "c", "y");
        let (keys, _, applied) = check(&b, &t, &d, &[]);
        assert_eq!(keys, ["c:var_name", "b:var_name"]);
        assert_eq!(var(&applied, "b").as_deref(), Some("b"));
        let (keys, ops, applied) =
            check(&b, &t, &d, &[("b:var_name", "r"), ("c:var_name", "draft")]);
        assert!(keys.is_empty());
        assert_eq!(var(&applied, "b").as_deref(), Some("r"));
        assert_eq!(var(&applied, "c").as_deref(), Some("y"));
        let renames = ops
            .iter()
            .filter(
                |op| matches!(op, EventPayload::FieldEdited { field, .. } if field == "var_name"),
            )
            .count();
        assert_eq!(renames, 2);
        let (keys, _, applied) = check(&b, &t, &d, &[("b:var_name", "team"), ("c:var_name", "z")]);
        assert!(keys.is_empty());
        assert_eq!(var(&applied, "c").as_deref(), Some("z"));
    }

    /// Applies a plan's operations to the team state and returns the result.
    fn applied(team: &DocumentState, plan: &MergePlan) -> DocumentState {
        let mut applied = team.clone();
        for op in &plan.ops {
            crate::ops::validate_op(&applied, op).unwrap();
            crate::materializer::apply_payload(&mut applied, op).unwrap();
        }
        applied
    }

    fn block(s: &mut DocumentState, id: &str, parent: &str, var: Option<&str>) {
        let mut node = s.nodes[&NodeId("root".into())].clone();
        node.id = NodeId(id.into());
        node.node_type = "section".into();
        node.parent_id = Some(NodeId(parent.into()));
        node.pos = id.into();
        node.current_fields = match var {
            Some(v) => json!({"label": id, "var_name": v}),
            None => json!({"label": id}),
        };
        node.var_name = var.map(str::to_owned);
        s.nodes.insert(node.id.clone(), node);
    }

    #[test]
    fn a_cycle_through_a_draft_new_block_is_fixed_at_a_block_the_team_has() {
        let mut b = state();
        for id in ["01M", "01X", "01Y"] {
            block(&mut b, id, "root", None);
        }
        // The team puts X under Y; the draft wraps Y and M in a new group under X.
        let mut t = b.clone();
        t.nodes.get_mut(&NodeId("01X".into())).unwrap().parent_id = Some(NodeId("01Y".into()));
        let mut d = b.clone();
        block(&mut d, "02N", "01X", None);
        for id in ["01Y", "01M"] {
            d.nodes.get_mut(&NodeId(id.into())).unwrap().parent_id = Some(NodeId("02N".into()));
        }
        let plan = merge(&b, &t, &d, &BTreeMap::new());
        let keys: Vec<_> = plan.conflicts.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, ["01Y:$location"]);
        assert!(find_parent_cycle(&plan.state).is_none());
        let resolved = merge(
            &b,
            &t,
            &d,
            &BTreeMap::from([("01Y:$location".into(), "team".into())]),
        );
        assert!(resolved.conflicts.is_empty());
        assert!(find_parent_cycle(&applied(&t, &resolved)).is_none());
    }

    #[test]
    fn every_block_placed_in_a_deleted_container_leaves_it_together() {
        let mut b = state();
        block(&mut b, "p", "root", None);
        let mut t = b.clone();
        t.nodes.get_mut(&NodeId("p".into())).unwrap().deleted = true;
        let mut d = b.clone();
        for i in 0..50 {
            block(&mut d, &format!("c{i:02}"), "p", None);
        }
        let plan = merge(&b, &t, &d, &BTreeMap::new());
        let keys: Vec<_> = plan.conflicts.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, ["p:$node"]);
        let resolved = merge(
            &b,
            &t,
            &d,
            &BTreeMap::from([("p:$node".into(), "team".into())]),
        );
        assert!(resolved.conflicts.is_empty());
        let end = applied(&t, &resolved);
        assert!(end.nodes.values().all(|n| n.deleted || n.id.0 == "root"));
    }

    #[test]
    fn a_typed_variable_name_must_be_usable() {
        let b = state();
        let mut t = b.clone();
        block(&mut t, "team_q", "root", Some("q"));
        let mut d = b.clone();
        block(&mut d, "draft_q", "root", Some("q"));
        for bad in ["", " q2", "q2 ", &"x".repeat(129)] {
            let plan = merge(
                &b,
                &t,
                &d,
                &BTreeMap::from([("draft_q:var_name".into(), bad.into())]),
            );
            let keys: Vec<_> = plan.conflicts.iter().map(|c| c.key.as_str()).collect();
            assert_eq!(keys, ["draft_q:var_name"], "{bad:?}");
        }
        let plan = merge(
            &b,
            &t,
            &d,
            &BTreeMap::from([("draft_q:var_name".into(), "q2".into())]),
        );
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn rejecting_a_draft_only_block_takes_its_draft_only_children_along() {
        let b = state();
        let mut t = b.clone();
        block(&mut t, "team_q", "root", Some("q"));
        let mut d = b.clone();
        block(&mut d, "draft_q", "root", Some("q"));
        block(&mut d, "child", "draft_q", None);
        let plan = merge(
            &b,
            &t,
            &d,
            &BTreeMap::from([("draft_q:var_name".into(), "team".into())]),
        );
        assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
        assert!(!plan.state.nodes.contains_key(&NodeId("draft_q".into())));
        assert!(!plan.state.nodes.contains_key(&NodeId("child".into())));
    }

    #[test]
    fn contradictory_choices_stop_at_the_first_repeat() {
        // Two blocks, each placed by one side inside a container the other side deleted;
        // choosing opposite sides for them re-creates each other's orphan forever.
        let mut b = state();
        block(&mut b, "p", "root", None);
        block(&mut b, "q", "root", None);
        let mut t = b.clone();
        t.nodes.get_mut(&NodeId("p".into())).unwrap().deleted = true;
        block(&mut t, "x", "q", None);
        let mut d = b.clone();
        d.nodes.get_mut(&NodeId("q".into())).unwrap().deleted = true;
        block(&mut d, "y", "p", None);
        let choices = BTreeMap::from([
            ("p:$node".into(), "draft".into()),
            ("q:$node".into(), "team".into()),
        ]);
        let plan = merge(&b, &t, &d, &choices);
        // Whatever the outcome, it must come from a bounded number of repairs and
        // leave no live block under a deleted parent that is not reported.
        let orphan = find_orphan(&plan.state);
        assert!(orphan.is_none() || !plan.conflicts.is_empty(), "{orphan:?}");
    }

    #[test]
    fn a_name_freed_in_the_same_change_can_be_taken() {
        let id = |s: &str| NodeId(s.into());
        let add = |s: &mut DocumentState, n: &str, var: &str| {
            let mut node = s.nodes[&id("root")].clone();
            node.id = id(n);
            node.node_type = "item".into();
            node.parent_id = Some(id("root"));
            node.current_fields = json!({"var_name": var});
            node.var_name = Some(var.into());
            s.nodes.insert(node.id.clone(), node);
        };
        let rename = |s: &mut DocumentState, n: &str, var: &str| {
            let node = s.nodes.get_mut(&id(n)).unwrap();
            node.current_fields["var_name"] = json!(var);
            node.var_name = Some(var.into());
        };
        let apply = |from: &DocumentState, to: &DocumentState| {
            let mut state = from.clone();
            for op in diff(from, to) {
                crate::ops::validate_op(&state, &op).unwrap();
                crate::materializer::apply_payload(&mut state, &op).unwrap();
            }
            let var = |n: &str| {
                state
                    .nodes
                    .get(&id(n))
                    .filter(|n| !n.deleted)
                    .and_then(|n| n.var_name.clone())
            };
            (var("a"), var("y"), var("z"))
        };
        // The block taking the name sorts first ("a"), so it used to claim it too early.
        let mut from = state();
        add(&mut from, "z", "q");
        let mut to = from.clone();
        to.nodes.get_mut(&id("z")).unwrap().deleted = true;
        add(&mut to, "a", "q");
        assert_eq!(apply(&from, &to), (Some("q".into()), None, None));
        let mut from = state();
        add(&mut from, "a", "r");
        add(&mut from, "z", "q");
        let mut to = from.clone();
        to.nodes.get_mut(&id("z")).unwrap().deleted = true;
        rename(&mut to, "a", "q");
        assert_eq!(apply(&from, &to), (Some("q".into()), None, None));
        // Names shift: y q -> r, z r -> s, and a new block takes q.
        let mut from = state();
        add(&mut from, "y", "q");
        add(&mut from, "z", "r");
        let mut to = from.clone();
        rename(&mut to, "y", "r");
        rename(&mut to, "z", "s");
        add(&mut to, "a", "q");
        assert_eq!(
            apply(&from, &to),
            (Some("q".into()), Some("r".into()), Some("s".into()))
        );
    }

    #[test]
    fn removed_choice_diff_restores_before_moving_or_editing() {
        let id = |s: &str| NodeId(s.into());
        let mut from = state();
        for (name, kind, parent) in [
            ("item", "item", "root"),
            ("other", "item", "root"),
            ("choice", "choice", "item"),
        ] {
            let mut node = from.nodes[&id("root")].clone();
            node.id = id(name);
            node.node_type = kind.into();
            node.parent_id = Some(id(parent));
            node.current_fields = json!({"label":"Original"});
            node.var_name = None;
            from.nodes.insert(node.id.clone(), node);
        }
        from.removed_choices.insert(id("choice"));
        for parent in ["item", "other"] {
            let mut to = from.clone();
            to.removed_choices.clear();
            let choice = to.nodes.get_mut(&id("choice")).unwrap();
            choice.parent_id = Some(id(parent));
            choice.pos = "moved".into();
            choice.current_fields["label"] = json!("Restored");
            let ops = diff(&from, &to);
            assert!(
                matches!(&ops[0], EventPayload::NodeRestored { node_id } if *node_id == id("choice"))
            );
            let mut applied = from.clone();
            for op in ops {
                crate::ops::validate_op(&applied, &op).unwrap();
                crate::materializer::apply_payload(&mut applied, &op).unwrap();
            }
            assert_eq!(applied, to);
        }
    }

    #[test]
    fn a_restored_block_can_come_back_moved_or_renamed() {
        let id = |s: &str| NodeId(s.into());
        let add = |s: &mut DocumentState, n: &str, parent: &str, var: Option<&str>| {
            let mut node = s.nodes[&id("root")].clone();
            node.id = id(n);
            node.node_type = if var.is_some() { "item" } else { "section" }.into();
            node.parent_id = Some(id(parent));
            node.current_fields = json!({ "var_name": var });
            node.var_name = var.map(Into::into);
            s.nodes.insert(node.id.clone(), node);
        };
        let set = |s: &mut DocumentState, n: &str, deleted: bool, parent: &str, var: &str| {
            let node = s.nodes.get_mut(&id(n)).unwrap();
            node.deleted = deleted;
            node.parent_id = Some(id(parent));
            node.current_fields["var_name"] = json!(var);
            node.var_name = Some(var.into());
        };
        let apply = |from: &DocumentState, to: &DocumentState| {
            let mut state = from.clone();
            for op in diff(from, to) {
                crate::ops::validate_op(&state, &op).unwrap();
                crate::materializer::apply_payload(&mut state, &op).unwrap();
            }
            let x = &state.nodes[&id("x")];
            (x.deleted, x.parent_id.clone(), x.var_name.clone())
        };
        // Back in another container, after its old one was deleted.
        let mut from = state();
        add(&mut from, "old", "root", None);
        add(&mut from, "new", "root", None);
        add(&mut from, "x", "old", Some("q"));
        set(&mut from, "x", true, "old", "q");
        from.nodes.get_mut(&id("old")).unwrap().deleted = true;
        let mut to = from.clone();
        set(&mut to, "x", false, "new", "q");
        assert_eq!(
            apply(&from, &to),
            (false, Some(id("new")), Some("q".into()))
        );
        // Back under a new name, while another block holds its old one.
        let mut from = state();
        add(&mut from, "x", "root", Some("q"));
        set(&mut from, "x", true, "root", "q");
        add(&mut from, "y", "root", Some("q"));
        let mut to = from.clone();
        set(&mut to, "x", false, "root", "r");
        assert_eq!(
            apply(&from, &to),
            (false, Some(id("root")), Some("r".into()))
        );
        // ...and when the new name is freed in the same change.
        add(&mut from, "w", "root", Some("r"));
        let mut to = from.clone();
        to.nodes.get_mut(&id("w")).unwrap().deleted = true;
        set(&mut to, "x", false, "root", "r");
        assert_eq!(
            apply(&from, &to),
            (false, Some(id("root")), Some("r".into()))
        );
    }

    #[test]
    fn restored_blocks_take_back_names_their_holders_have_just_left() {
        let mut b = state();
        for (id, var, gone) in [
            ("w1", "x", false),
            ("w2", "y", false),
            ("a", "x", true),
            ("c", "y", true),
        ] {
            block(&mut b, id, "root", Some(var));
            b.nodes.get_mut(&NodeId(id.into())).unwrap().deleted = gone;
        }
        // The draft deletes both holders and restores both blocks under their old names.
        let mut d = b.clone();
        for id in ["w1", "w2"] {
            d.nodes.get_mut(&NodeId(id.into())).unwrap().deleted = true;
        }
        for id in ["a", "c"] {
            d.nodes.get_mut(&NodeId(id.into())).unwrap().deleted = false;
        }
        let plan = merge(&b, &b, &d, &BTreeMap::new());
        assert!(plan.conflicts.is_empty());
        let end = applied(&b, &plan);
        let var = |id: &str| end.nodes[&NodeId(id.into())].var_name.clone();
        assert_eq!((var("a"), var("c")), (Some("x".into()), Some("y".into())));
        assert!(end.nodes[&NodeId("w1".into())].deleted);
        assert!(!end.nodes[&NodeId("a".into())].deleted);
    }

    #[test]
    fn renaming_a_restored_block_whose_old_name_is_taken_replays() {
        let mut b = state();
        block(&mut b, "q", "root", Some("q"));
        b.nodes.get_mut(&NodeId("q".into())).unwrap().deleted = true;
        let mut t = b.clone();
        block(&mut t, "taker", "root", Some("q"));
        let mut d = b.clone();
        d.nodes.get_mut(&NodeId("q".into())).unwrap().deleted = false;
        let plan = merge(&b, &t, &d, &BTreeMap::new());
        let keys: Vec<_> = plan.conflicts.iter().map(|c| c.key.as_str()).collect();
        assert_eq!(keys, ["q:var_name"]);
        let plan = merge(
            &b,
            &t,
            &d,
            &BTreeMap::from([("q:var_name".into(), "q2".into())]),
        );
        assert!(plan.conflicts.is_empty());
        let end = applied(&t, &plan);
        let q = &end.nodes[&NodeId("q".into())];
        assert!(!q.deleted);
        assert_eq!(q.var_name.as_deref(), Some("q2"));
    }
}
