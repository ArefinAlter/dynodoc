//! Report-only same-document convergence for project comparison.
//!
//! Project planning treats a path edited on both sides as one whole-file conflict.
//! When both sides still name the same stable document UUID, this module reads
//! the three typed states, runs the existing node/field merge (`workspace_merge`,
//! including rich-text run merging) and reports whether it is clean, the address a
//! merged state *would* have and node/field-level conflicts. Nothing is stored,
//! signed or published, and a conflicting file is never resolved automatically.
//!
//! Order independence: the merge is run in both orientations and is clean only if
//! both are. The address comes from one canonical orientation (the side with the
//! smaller state address is the "team" side), so swapping left and right gives the
//! same address and the same conflicts with left/right values exchanged.

use super::{compare::ProjectPlan, local::LocalRepository, Error, FileEntry};
use crate::{
    materializer::DocumentState,
    shared_checkpoint::{
        self,
        typed::{self, SemanticLimits},
        Address, Limits, ObjectStore,
    },
    workspace_merge,
};
use engine_shared::{DocumentId, NodeId};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Same-document conflicting files examined per comparison; the rest are counted.
pub const MAX_REPORTED_FILES: usize = 64;

/// Strategy id signed into a v3 commit: the node/field three-way merge of this
/// module (`converge`), clean in both orientations. A different algorithm needs a
/// different id; importers accept only ids they know.
pub const STRATEGY: &str = "dynodoc.same-file.node-field.v1";

/// Signed provenance of one automatically merged path: the three input state
/// roots under one stable document identity and the stored merged state root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Resolution {
    pub path: String,
    pub document: DocumentId,
    /// State at the unique best common ancestor.
    pub base: Address,
    /// State in the first parent.
    pub left: Address,
    /// State in the second parent.
    pub right: Address,
    pub merged: Address,
    pub strategy: String,
}

/// One node/field (or document-level) disagreement. `node` is the stable node id;
/// document-level collections (`$comments`, `$suggestions`) have none.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct StateConflict {
    pub node: Option<NodeId>,
    pub field: String,
    pub base: Value,
    pub left: Value,
    pub right: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FileConvergence {
    pub path: String,
    pub document: DocumentId,
    pub base: Address,
    pub left: Address,
    pub right: Address,
    pub clean: bool,
    /// Present only when `clean`: the typed-state root a merged state would have.
    /// The state is not stored and no commit exists.
    pub merged_state: Option<Address>,
    pub conflicts: Vec<StateConflict>,
}

#[derive(Debug, Serialize)]
pub struct SameFileReport {
    pub files: Vec<FileConvergence>,
    /// Same-document conflicts beyond `MAX_REPORTED_FILES`, not examined.
    pub not_examined: usize,
}

struct RawConflict {
    node: Option<NodeId>,
    field: String,
    base: Value,
    team: Value,
    draft: Value,
}

struct Merged {
    state: DocumentState,
    conflicts: Vec<RawConflict>,
}

fn node_json(state: &DocumentState, id: &NodeId) -> Value {
    state.nodes.get(id).map_or(Value::Null, |node| json!(node))
}

/// Three-way choice for a whole collection: equal sides or one unchanged side
/// merge; two different changes are a conflict.
fn three_way<T: Clone + PartialEq>(base: &T, team: &T, draft: &T) -> Option<T> {
    if draft == base || draft == team {
        Some(team.clone())
    } else if team == base {
        Some(draft.clone())
    } else {
        None
    }
}

/// Node/field merge with the symmetric handling workspace_merge omits: it only
/// visits the draft's nodes, so a node missing from the draft and the draft's
/// removed choices, comments and suggestions would otherwise depend on side order.
fn merge_states(base: &DocumentState, team: &DocumentState, draft: &DocumentState) -> Merged {
    let mut conflicts = Vec::new();
    // The merge indexes into field maps; refuse shapes it cannot handle.
    for id in base
        .nodes
        .keys()
        .chain(team.nodes.keys())
        .chain(draft.nodes.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let odd = [base, team, draft].iter().any(|state| {
            state
                .nodes
                .get(id)
                .is_some_and(|n| !(n.current_fields.is_object() || n.current_fields.is_null()))
        });
        if odd {
            conflicts.push(RawConflict {
                node: Some(id.clone()),
                field: "$fields".into(),
                base: node_json(base, id),
                team: node_json(team, id),
                draft: node_json(draft, id),
            });
        }
    }
    if !conflicts.is_empty() {
        return Merged {
            state: team.clone(),
            conflicts,
        };
    }
    // A node absent from the draft: unchanged in the team means the removal wins;
    // changed in the team is a delete/edit conflict.
    let mut adjusted: Option<DocumentState> = None;
    for (id, node) in &base.nodes {
        if draft.nodes.contains_key(id) {
            continue;
        }
        match team.nodes.get(id) {
            Some(current) if current == node => {
                adjusted
                    .get_or_insert_with(|| team.clone())
                    .nodes
                    .remove(id);
            }
            Some(_) => conflicts.push(RawConflict {
                node: Some(id.clone()),
                field: "$node".into(),
                base: json!(node),
                team: node_json(team, id),
                draft: Value::Null,
            }),
            None => {}
        }
    }
    let team = adjusted.as_ref().unwrap_or(team);
    let plan = workspace_merge::merge(base, team, draft, &BTreeMap::new());
    conflicts.extend(plan.conflicts.into_iter().map(|c| RawConflict {
        node: Some(c.node_id),
        field: c.field,
        base: c.ancestor,
        team: c.team,
        draft: c.draft,
    }));
    let mut state = plan.state;
    match three_way(&base.comments, &team.comments, &draft.comments) {
        Some(comments) => state.comments = comments,
        None => conflicts.push(RawConflict {
            node: None,
            field: "$comments".into(),
            base: json!(base.comments),
            team: json!(team.comments),
            draft: json!(draft.comments),
        }),
    }
    match three_way(&base.suggestions, &team.suggestions, &draft.suggestions) {
        Some(suggestions) => state.suggestions = suggestions,
        None => conflicts.push(RawConflict {
            node: None,
            field: "$suggestions".into(),
            base: json!(base.suggestions),
            team: json!(team.suggestions),
            draft: json!(draft.suggestions),
        }),
    }
    // Union of both sides' removed choices, kept only for nodes that end deleted.
    state.removed_choices = team
        .removed_choices
        .union(&draft.removed_choices)
        .filter(|id| state.nodes.get(*id).is_some_and(|n| n.deleted))
        .cloned()
        .collect();
    Merged { state, conflicts }
}

pub struct Convergence {
    pub merged: Option<DocumentState>,
    pub conflicts: Vec<StateConflict>,
}

/// Pure three-way convergence of one document. `left_root`/`right_root` are the
/// typed state addresses; they only decide the canonical orientation.
pub fn converge(
    base: &DocumentState,
    left: &DocumentState,
    right: &DocumentState,
    left_root: &Address,
    right_root: &Address,
) -> Convergence {
    let team_is_left = left_root <= right_root;
    let (team, draft) = if team_is_left {
        (left, right)
    } else {
        (right, left)
    };
    let forward = merge_states(base, team, draft);
    let backward = merge_states(base, draft, team);
    let mut conflicts: BTreeMap<(Option<NodeId>, String), StateConflict> = BTreeMap::new();
    // `team_value` is the value on the forward team side.
    let mut add = |raw: RawConflict, forward_orientation: bool| {
        let (team_value, draft_value) = if forward_orientation {
            (raw.team, raw.draft)
        } else {
            (raw.draft, raw.team)
        };
        let (left, right) = if team_is_left {
            (team_value, draft_value)
        } else {
            (draft_value, team_value)
        };
        conflicts
            .entry((raw.node.clone(), raw.field.clone()))
            .or_insert(StateConflict {
                node: raw.node,
                field: raw.field,
                base: raw.base,
                left,
                right,
            });
    };
    let clean = forward.conflicts.is_empty() && backward.conflicts.is_empty();
    for raw in forward.conflicts {
        add(raw, true);
    }
    for raw in backward.conflicts {
        add(raw, false);
    }
    Convergence {
        merged: clean.then_some(forward.state),
        conflicts: conflicts.into_values().collect(),
    }
}

/// Computes an address without retaining or storing any object.
struct DiscardStore;

impl ObjectStore for DiscardStore {
    async fn put(
        &mut self,
        _: DocumentId,
        _: &Address,
        _: &[u8],
    ) -> Result<bool, shared_checkpoint::Error> {
        Ok(true)
    }
    async fn get(
        &mut self,
        _: DocumentId,
        address: &Address,
    ) -> Result<Vec<u8>, shared_checkpoint::Error> {
        Err(shared_checkpoint::Error::Missing(address.as_str().into()))
    }
}

/// The typed-state root `state` would have for `document`, without storing it.
pub async fn state_address(document: DocumentId, state: &DocumentState) -> Result<Address, Error> {
    Ok(typed::write_state(
        &mut DiscardStore,
        document,
        state,
        Limits::default(),
        SemanticLimits::default(),
    )
    .await?
    .0)
}

/// One conflicting path that all three commits keep under one document id.
pub(super) type Eligible<'a> = (&'a String, &'a FileEntry, &'a FileEntry, &'a FileEntry);

/// Conflicts whose base, left and right all keep one document identity and whose
/// two sides differ: the only paths convergence may examine.
pub(super) fn same_document_conflicts(plan: &ProjectPlan) -> Vec<Eligible<'_>> {
    plan.conflicts
        .iter()
        .filter_map(|conflict| {
            let (base, left, right) = (
                conflict.base.as_ref()?,
                conflict.left.as_ref()?,
                conflict.right.as_ref()?,
            );
            (base.document == left.document
                && left.document == right.document
                && left.state != right.state)
                .then_some((&conflict.path, base, left, right))
        })
        .collect()
}

impl LocalRepository {
    /// Report same-document convergence for path conflicts of a project plan.
    /// Only conflicts whose base, left and right all keep one document id are
    /// examined; returns `None` when there are none. Writes nothing.
    pub async fn same_file_convergence(
        &self,
        plan: &ProjectPlan,
    ) -> Result<Option<SameFileReport>, Error> {
        let eligible = same_document_conflicts(plan);
        if eligible.is_empty() {
            return Ok(None);
        }
        let mut files = Vec::new();
        for (path, base, left, right) in eligible.iter().take(MAX_REPORTED_FILES) {
            let document = base.document;
            let base_state = self.read_state(document, &base.state).await?;
            let left_state = self.read_state(document, &left.state).await?;
            let right_state = self.read_state(document, &right.state).await?;
            let result = converge(
                &base_state,
                &left_state,
                &right_state,
                &left.state,
                &right.state,
            );
            let merged_state = match &result.merged {
                Some(state) => Some(state_address(document, state).await?),
                None => None,
            };
            files.push(FileConvergence {
                path: (*path).clone(),
                document,
                base: base.state.clone(),
                left: left.state.clone(),
                right: right.state.clone(),
                clean: result.merged.is_some(),
                merged_state,
                conflicts: result.conflicts,
            });
        }
        Ok(Some(SameFileReport {
            files,
            not_examined: eligible.len().saturating_sub(MAX_REPORTED_FILES),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materializer::{CommentState, MaterializedNode, SuggestionState};

    fn node(id: &str, fields: Value) -> MaterializedNode {
        MaterializedNode {
            id: NodeId(id.into()),
            parent_id: None,
            node_type: "paragraph".into(),
            pos: format!("p-{id}"),
            var_name: None,
            deleted: false,
            current_fields: fields,
        }
    }
    fn document(nodes: Vec<MaterializedNode>) -> DocumentState {
        DocumentState {
            nodes: nodes.into_iter().map(|n| (n.id.clone(), n)).collect(),
            ..Default::default()
        }
    }
    fn plain(label: &str, note: &str) -> Value {
        json!({"label": label, "note": note})
    }
    fn rich(text: &str, marks: Option<Value>) -> Value {
        let mut run = json!({"type":"text","text":text});
        if let Some(marks) = marks {
            run["marks"] = marks;
        }
        let content = json!({"type":"paragraph","attrs":{"blockId":"b"},"content":[run]});
        json!({"label": crate::richtext::plain_text(&content), "content": content})
    }
    fn field<'a>(state: &'a DocumentState, id: &str, name: &str) -> &'a Value {
        &state.nodes[&NodeId(id.into())].current_fields[name]
    }
    fn set(state: &mut DocumentState, id: &str, name: &str, value: Value) {
        state
            .nodes
            .get_mut(&NodeId(id.into()))
            .unwrap()
            .current_fields[name] = value;
    }
    fn doc_id() -> DocumentId {
        DocumentId(uuid::Uuid::from_u128(7))
    }

    #[derive(Debug)]
    struct Report {
        clean: bool,
        address: Option<Address>,
        merged: Option<DocumentState>,
        conflicts: Vec<StateConflict>,
    }
    async fn run(base: &DocumentState, left: &DocumentState, right: &DocumentState) -> Report {
        let (lr, rr) = (
            state_address(doc_id(), left).await.unwrap(),
            state_address(doc_id(), right).await.unwrap(),
        );
        let result = converge(base, left, right, &lr, &rr);
        let address = match &result.merged {
            Some(state) => Some(state_address(doc_id(), state).await.unwrap()),
            None => None,
        };
        Report {
            clean: result.merged.is_some(),
            address,
            merged: result.merged,
            conflicts: result.conflicts,
        }
    }
    /// Left/right swapped must give the same address and mirrored conflicts.
    async fn mirrored(base: &DocumentState, left: &DocumentState, right: &DocumentState) -> Report {
        let forward = run(base, left, right).await;
        let reverse = run(base, right, left).await;
        assert_eq!(forward.clean, reverse.clean);
        assert_eq!(forward.address, reverse.address);
        assert_eq!(forward.merged, reverse.merged);
        assert_eq!(forward.conflicts.len(), reverse.conflicts.len());
        for (a, b) in forward.conflicts.iter().zip(&reverse.conflicts) {
            assert_eq!((&a.node, &a.field, &a.base), (&b.node, &b.field, &b.base));
            assert_eq!((&a.left, &a.right), (&b.right, &b.left));
        }
        forward
    }

    #[tokio::test]
    async fn disjoint_field_and_node_edits_merge_to_one_address_in_either_order() {
        let base = document(vec![node("a", plain("A", "n")), node("b", plain("B", "m"))]);
        let mut left = base.clone();
        set(&mut left, "a", "note", json!("left note"));
        set(&mut left, "b", "label", json!("B left"));
        let mut right = base.clone();
        set(&mut right, "a", "label", json!("A right"));
        let report = mirrored(&base, &left, &right).await;
        assert!(report.clean && report.conflicts.is_empty());
        let merged = report.merged.unwrap();
        assert_eq!(field(&merged, "a", "label"), "A right");
        assert_eq!(field(&merged, "a", "note"), "left note");
        assert_eq!(field(&merged, "b", "label"), "B left");
    }

    #[tokio::test]
    async fn same_field_edits_conflict_with_stable_ids_and_mirrored_values() {
        let base = document(vec![node("a", plain("A", "n"))]);
        let mut left = base.clone();
        set(&mut left, "a", "label", json!("from left"));
        let mut right = base.clone();
        set(&mut right, "a", "label", json!("from right"));
        let report = mirrored(&base, &left, &right).await;
        assert!(!report.clean);
        assert_eq!(report.address, None);
        assert_eq!(report.conflicts.len(), 1);
        let conflict = &report.conflicts[0];
        assert_eq!(conflict.node, Some(NodeId("a".into())));
        assert_eq!(conflict.field, "label");
        assert_eq!(conflict.base, "A");
        assert_eq!(conflict.left, "from left");
        assert_eq!(conflict.right, "from right");
        let swapped = run(&base, &right, &left).await;
        assert_eq!(swapped.conflicts[0].left, "from right");
    }

    #[tokio::test]
    async fn concurrent_inserts_deletes_and_moves_follow_the_node_rules() {
        let base = document(vec![node("a", plain("A", "")), node("b", plain("B", ""))]);
        // Different new nodes merge; the same id with different content conflicts.
        let mut left = base.clone();
        left.nodes
            .insert(NodeId("x".into()), node("x", plain("X left", "")));
        left.nodes
            .insert(NodeId("same".into()), node("same", plain("one", "")));
        let mut right = base.clone();
        right
            .nodes
            .insert(NodeId("y".into()), node("y", plain("Y right", "")));
        let report = mirrored(&base, &left, &right).await;
        assert!(report.clean);
        let merged = report.merged.unwrap();
        assert!(merged.nodes.contains_key(&NodeId("x".into())));
        assert!(merged.nodes.contains_key(&NodeId("y".into())));
        right
            .nodes
            .insert(NodeId("same".into()), node("same", plain("two", "")));
        let report = mirrored(&base, &left, &right).await;
        assert!(!report.clean);
        assert_eq!(report.conflicts.len(), 1);
        assert_eq!(report.conflicts[0].field, "$node");
        // Delete versus edit conflicts; deleting on both sides merges.
        let mut deleted = base.clone();
        deleted.nodes.get_mut(&NodeId("a".into())).unwrap().deleted = true;
        let mut edited = base.clone();
        set(&mut edited, "a", "label", json!("edited"));
        let report = mirrored(&base, &deleted, &edited).await;
        assert!(!report.clean);
        assert_eq!(report.conflicts[0].field, "$node");
        assert!(mirrored(&base, &deleted, &deleted).await.clean);
        // A move on one side and an edit of another node on the other merge.
        let mut moved = base.clone();
        moved.nodes.get_mut(&NodeId("b".into())).unwrap().pos = "z".into();
        let report = mirrored(&base, &moved, &edited).await;
        assert!(report.clean);
        // Two different moves conflict at the location.
        let mut elsewhere = base.clone();
        elsewhere.nodes.get_mut(&NodeId("b".into())).unwrap().pos = "y".into();
        let report = mirrored(&base, &moved, &elsewhere).await;
        assert_eq!(report.conflicts[0].field, "$location");
    }

    #[tokio::test]
    async fn nodes_missing_from_one_map_are_handled_symmetrically() {
        let base = document(vec![node("a", plain("A", "")), node("b", plain("B", ""))]);
        let mut removed = base.clone();
        removed.nodes.remove(&NodeId("b".into()));
        let mut edited_a = base.clone();
        set(&mut edited_a, "a", "label", json!("A edited"));
        let report = mirrored(&base, &removed, &edited_a).await;
        assert!(report.clean);
        let merged = report.merged.unwrap();
        assert!(!merged.nodes.contains_key(&NodeId("b".into())));
        assert_eq!(field(&merged, "a", "label"), "A edited");
        let mut edited_b = base.clone();
        set(&mut edited_b, "b", "label", json!("B edited"));
        let report = mirrored(&base, &removed, &edited_b).await;
        assert!(!report.clean);
        assert_eq!(report.conflicts[0].field, "$node");
        assert_eq!(report.conflicts[0].node, Some(NodeId("b".into())));
    }

    #[tokio::test]
    async fn rich_text_edits_merge_by_run_and_conflict_when_they_overlap() {
        let sentence = "The participants were interviewed in June.";
        let base = document(vec![node("p", rich(sentence, None))]);
        let with = |fields: Value| {
            let mut state = base.clone();
            state
                .nodes
                .get_mut(&NodeId("p".into()))
                .unwrap()
                .current_fields = fields;
            state
        };
        let edit = |from: &str, to: &str| with(rich(&sentence.replace(from, to), None));
        let left = edit("participants", "respondents");
        let right = edit("June", "July");
        let report = mirrored(&base, &left, &right).await;
        assert!(report.clean, "{:?}", report.conflicts);
        assert_eq!(
            field(report.merged.as_ref().unwrap(), "p", "label"),
            "The respondents were interviewed in July."
        );
        let overlapping = edit("participants", "subjects");
        let report = mirrored(&base, &left, &overlapping).await;
        assert!(!report.clean);
        assert_eq!(report.conflicts[0].field, "content");
        // Concurrent formatting of the same words is order independent.
        let bold = with(rich(sentence, Some(json!([{"type":"bold"}]))));
        let italic = with(rich(sentence, Some(json!([{"type":"italic"}]))));
        let report = mirrored(&base, &bold, &italic).await;
        assert!(report.clean, "{:?}", report.conflicts);
        let merged = report.merged.unwrap();
        let run = &field(&merged, "p", "content")["content"][0];
        assert_eq!(run["marks"].as_array().unwrap().len(), 2);
        // Formatting a trailing span and inserting text in the first span.
        let formatted = with(json!({
            "label": sentence,
            "content": {"type":"paragraph","attrs":{"blockId":"b"},"content":[
                {"type":"text","text":"The participants"},
                {"type":"text","text":" were interviewed in June.","marks":[{"type":"bold"}]}
            ]}
        }));
        let report = mirrored(&base, &formatted, &edit("The ", "The two ")).await;
        assert!(report.clean, "{:?}", report.conflicts);
    }

    #[tokio::test]
    async fn comments_suggestions_and_removed_choices_are_order_independent() {
        let base = document(vec![node("a", plain("A", "")), node("c", plain("C", ""))]);
        let comment = |body: &str| CommentState {
            node_id: None,
            body: body.into(),
        };
        let mut left = base.clone();
        left.comments.push(comment("left"));
        let report = mirrored(&base, &left, &base).await;
        assert!(report.clean);
        assert_eq!(report.merged.unwrap().comments, left.comments);
        let mut right = base.clone();
        right.comments.push(comment("right"));
        let report = mirrored(&base, &left, &right).await;
        assert!(!report.clean);
        assert_eq!(report.conflicts[0].node, None);
        assert_eq!(report.conflicts[0].field, "$comments");
        let suggestion = |value: i64| SuggestionState {
            target_node_id: None,
            detail: json!({"a": value}),
            accepted: false,
            rejected_reason: None,
        };
        let mut left = base.clone();
        left.suggestions.insert("s".into(), suggestion(1));
        let mut right = base.clone();
        right.suggestions.insert("s".into(), suggestion(2));
        let report = mirrored(&base, &left, &right).await;
        assert_eq!(report.conflicts[0].field, "$suggestions");
        // Removed choices from both sides are all retained, whichever side is "team".
        let mut left = base.clone();
        left.removed_choices.insert(NodeId("a".into()));
        let mut right = base.clone();
        right.removed_choices.insert(NodeId("c".into()));
        let report = mirrored(&base, &left, &right).await;
        assert!(report.clean, "{:?}", report.conflicts);
        let merged = report.merged.unwrap();
        assert_eq!(merged.removed_choices.len(), 2);
        assert!(merged.nodes.values().all(|n| n.deleted));
    }

    #[tokio::test]
    async fn unsupported_field_shapes_are_reported_not_panicked_on() {
        let base = document(vec![node("a", json!("not an object"))]);
        let mut changed = base.clone();
        changed
            .nodes
            .get_mut(&NodeId("a".into()))
            .unwrap()
            .current_fields = json!([1]);
        let report = mirrored(&base, &changed, &base).await;
        assert!(!report.clean);
        assert_eq!(report.conflicts[0].field, "$fields");
    }

    /// Small deterministic generator so failures replay exactly.
    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, bound: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) % bound
        }
    }

    fn random_edit(rng: &mut Lcg, state: &mut DocumentState, allow_choices: bool) {
        let ids: Vec<NodeId> = state.nodes.keys().cloned().collect();
        let target = ids[rng.next(ids.len() as u64) as usize].clone();
        match rng.next(if allow_choices { 8 } else { 7 }) {
            0 | 1 => {
                let field = ["label", "note"][rng.next(2) as usize];
                let value = format!("v{}", rng.next(4));
                if let Some(n) = state.nodes.get_mut(&target) {
                    n.current_fields[field] = json!(value);
                }
            }
            2 => {
                if let Some(n) = state.nodes.get_mut(&target) {
                    n.deleted = !n.deleted;
                }
            }
            3 => {
                if let Some(n) = state.nodes.get_mut(&target) {
                    n.pos = format!("m{}", rng.next(3));
                }
            }
            4 => {
                let id = format!("new{}", rng.next(4));
                let label = format!("n{}", rng.next(2));
                state
                    .nodes
                    .insert(NodeId(id.clone()), node(&id, plain(&label, "")));
            }
            5 => {
                state.nodes.remove(&target);
                if state.nodes.is_empty() {
                    state
                        .nodes
                        .insert(NodeId("keep".into()), node("keep", plain("", "")));
                }
            }
            6 => state.comments.push(CommentState {
                node_id: None,
                body: format!("c{}", rng.next(3)),
            }),
            _ => {
                state.removed_choices.insert(target);
            }
        }
    }

    fn random_base(count: usize) -> DocumentState {
        document(
            (0..count)
                .map(|i| node(&format!("n{i}"), plain(&format!("L{i}"), "base")))
                .collect(),
        )
    }

    #[tokio::test]
    async fn random_concurrent_edits_are_order_independent_and_identity_preserving() {
        let mut rng = Lcg(0xD15C0);
        let (mut clean_count, mut conflict_count) = (0, 0);
        for _ in 0..400 {
            let base = random_base(5);
            let (mut left, mut right) = (base.clone(), base.clone());
            for _ in 0..rng.next(4) {
                random_edit(&mut rng, &mut left, true);
            }
            for _ in 0..rng.next(4) {
                random_edit(&mut rng, &mut right, true);
            }
            let report = mirrored(&base, &left, &right).await;
            if report.clean {
                clean_count += 1;
            } else {
                conflict_count += 1;
                assert_eq!(report.address, None);
                assert!(!report.conflicts.is_empty());
            }
        }
        assert!(
            clean_count > 40 && conflict_count > 40,
            "{clean_count}/{conflict_count}"
        );
        // A side that did nothing merges to the other side.
        for _ in 0..200 {
            let base = random_base(4);
            let mut changed = base.clone();
            for _ in 0..1 + rng.next(3) {
                random_edit(&mut rng, &mut changed, false);
            }
            for (left, right) in [(&changed, &base), (&base, &changed), (&changed, &changed)] {
                let report = mirrored(&base, left, right).await;
                assert!(report.clean, "{:?}", report.conflicts);
                assert_eq!(report.merged.as_ref().unwrap(), &changed);
            }
        }
    }
}
