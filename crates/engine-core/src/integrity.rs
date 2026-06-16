//! Referential integrity pass over the materialized state (docs/16 part 4).
//!
//! Items carry `relevance` and `constraint` expressions (ODK-style `${var}`
//! references, docs/04 §C.3.6). Two ways an instrument can be *silently* broken:
//!
//! 1. **Dangling reference** — an expression names a `var_name` that no live node
//!    defines (a typo, or the referenced item was deleted). At runtime the form
//!    skip-logic evaluates against a missing variable and misbehaves in the field.
//! 2. **Relevance cycle** — item A's visibility depends (transitively) on item B's,
//!    whose depends back on A's. No assignment of answers settles the skip logic.
//!
//! [`validate_integrity`] finds every such violation in a `DocumentState`. The
//! governance gate (docs/16 part 3) runs it on every `Deployed` event and refuses
//! the deploy if any violation exists — the §C.3.6 "silent semantic breakage"
//! defense. The richer three-way semantic merge that also leans on this graph lands
//! in stage 21.

use std::collections::{BTreeMap, BTreeSet};

use engine_shared::NodeId;
use serde::{Deserialize, Serialize};

use crate::materializer::{DocumentState, MaterializedNode};

/// The two `current_fields` keys whose string values are reference-bearing
/// expressions. Both are scanned for `${var}` references.
const EXPRESSION_FIELDS: [&str; 2] = ["relevance", "constraint"];

/// A single referential-integrity problem found in a [`DocumentState`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum IntegrityViolation {
    /// An expression on `node_id` references a `var_name` no live node defines.
    DanglingReference {
        node_id: NodeId,
        /// Which expression field carried the reference (`relevance` | `constraint`).
        field: String,
        var_name: String,
    },
    /// Two or more live nodes define the same `var_name` (ambiguous reference target).
    DuplicateVarName {
        var_name: String,
        node_ids: Vec<NodeId>,
    },
    /// A relevance dependency cycle: each node's visibility depends, transitively, on
    /// its own. `cycle` lists the nodes on the cycle in dependency order.
    RelevanceCycle { cycle: Vec<NodeId> },
}

/// Find every referential-integrity violation in `state`. An empty vector means the
/// instrument is internally consistent and safe to deploy.
///
/// Only **live** (non-deleted) nodes participate: a tombstoned node neither defines a
/// `var_name` nor contributes an expression. The result is deterministic — nodes are
/// visited in `NodeId` order and violations are emitted in a stable order — so two
/// runs over equal state produce equal vectors (callers may compare them directly).
pub fn validate_integrity(state: &DocumentState) -> Vec<IntegrityViolation> {
    let live: Vec<&MaterializedNode> = state.nodes.values().filter(|node| !node.deleted).collect();

    let mut violations = Vec::new();

    // var_name -> the live nodes defining it (BTreeMap keeps output deterministic).
    let mut by_var: BTreeMap<String, Vec<NodeId>> = BTreeMap::new();
    for node in &live {
        if let Some(var) = node.var_name.as_ref().filter(|v| !v.is_empty()) {
            by_var.entry(var.clone()).or_default().push(node.id.clone());
        }
    }

    for (var_name, node_ids) in &by_var {
        if node_ids.len() > 1 {
            violations.push(IntegrityViolation::DuplicateVarName {
                var_name: var_name.clone(),
                node_ids: node_ids.clone(),
            });
        }
    }
    let defined: BTreeSet<&str> = by_var.keys().map(String::as_str).collect();

    // Dangling references: every `${var}` in every expression field must be defined.
    for node in &live {
        for field in EXPRESSION_FIELDS {
            let Some(expr) = node.current_fields.get(field).and_then(|v| v.as_str()) else {
                continue;
            };
            for var in extract_var_refs(expr) {
                if !defined.contains(var.as_str()) {
                    violations.push(IntegrityViolation::DanglingReference {
                        node_id: node.id.clone(),
                        field: field.to_string(),
                        var_name: var,
                    });
                }
            }
        }
    }

    violations.extend(find_relevance_cycles(&live, &by_var));
    violations
}

/// Extract the `var` names from every `${var}` reference in an expression, in order
/// of appearance, de-duplicated. Unclosed `${` (no matching `}`) yields no reference.
///
/// This is the ODK/XLSForm reference convention, not a full expression parse: stage
/// 16 only needs the *set of referenced variables*. A complete expression grammar
/// (operator validation, type checking) is later hardening, not the deploy gate.
pub fn extract_var_refs(expr: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut seen = BTreeSet::new();
    let bytes = expr.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'$' && bytes[i + 1] == b'{' {
            if let Some(end) = expr[i + 2..].find('}') {
                let name = expr[i + 2..i + 2 + end].trim();
                if !name.is_empty() && seen.insert(name.to_string()) {
                    refs.push(name.to_string());
                }
                i = i + 2 + end + 1;
                continue;
            }
            break; // unterminated `${` — nothing more to extract
        }
        i += 1;
    }
    refs
}

/// Detect cycles in the relevance dependency graph. An edge `A -> B` means node A's
/// `relevance` references a `var_name` that node B defines (A's visibility depends on
/// B). A cycle on this graph is a relevance that depends transitively on itself.
fn find_relevance_cycles(
    live: &[&MaterializedNode],
    by_var: &BTreeMap<String, Vec<NodeId>>,
) -> Vec<IntegrityViolation> {
    // Build adjacency over node ids. Only `relevance` forms visibility dependencies;
    // a `constraint` validates an answer, it does not gate visibility, so it cannot
    // form a relevance cycle.
    let mut adjacency: BTreeMap<NodeId, Vec<NodeId>> = BTreeMap::new();
    for node in live {
        let mut targets = Vec::new();
        if let Some(expr) = node
            .current_fields
            .get("relevance")
            .and_then(|v| v.as_str())
        {
            for var in extract_var_refs(expr) {
                if let Some(defs) = by_var.get(&var) {
                    targets.extend(defs.iter().cloned());
                }
            }
        }
        adjacency.insert(node.id.clone(), targets);
    }

    let mut violations = Vec::new();
    let mut state: BTreeMap<NodeId, VisitState> = BTreeMap::new();
    let mut stack: Vec<NodeId> = Vec::new();

    // Iterative DFS over a deterministic node order; report each distinct cycle once.
    let mut reported: BTreeSet<Vec<NodeId>> = BTreeSet::new();
    for node in adjacency.keys() {
        dfs_cycles(
            node,
            &adjacency,
            &mut state,
            &mut stack,
            &mut reported,
            &mut violations,
        );
    }
    violations
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    InProgress,
    Done,
}

fn dfs_cycles(
    node: &NodeId,
    adjacency: &BTreeMap<NodeId, Vec<NodeId>>,
    state: &mut BTreeMap<NodeId, VisitState>,
    stack: &mut Vec<NodeId>,
    reported: &mut BTreeSet<Vec<NodeId>>,
    violations: &mut Vec<IntegrityViolation>,
) {
    match state.get(node) {
        Some(VisitState::Done) => return,
        Some(VisitState::InProgress) => {
            // Found a back-edge: the cycle is the stack slice from `node` to the top.
            if let Some(pos) = stack.iter().position(|n| n == node) {
                let cycle = canonical_cycle(&stack[pos..]);
                if reported.insert(cycle.clone()) {
                    violations.push(IntegrityViolation::RelevanceCycle { cycle });
                }
            }
            return;
        }
        None => {}
    }

    state.insert(node.clone(), VisitState::InProgress);
    stack.push(node.clone());
    if let Some(targets) = adjacency.get(node) {
        for next in targets {
            dfs_cycles(next, adjacency, state, stack, reported, violations);
        }
    }
    stack.pop();
    state.insert(node.clone(), VisitState::Done);
}

/// Rotate a detected cycle to start at its smallest `NodeId` so the same cycle
/// discovered from different entry points compares equal (stable de-duplication).
fn canonical_cycle(nodes: &[NodeId]) -> Vec<NodeId> {
    let min = nodes
        .iter()
        .enumerate()
        .min_by(|(_, a), (_, b)| a.cmp(b))
        .map(|(idx, _)| idx)
        .unwrap_or(0);
    let mut out = Vec::with_capacity(nodes.len());
    out.extend_from_slice(&nodes[min..]);
    out.extend_from_slice(&nodes[..min]);
    out
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};

    use serde_json::json;

    use super::*;
    use crate::materializer::DocumentState;

    fn node(id: &str, var: Option<&str>, fields: serde_json::Value) -> MaterializedNode {
        MaterializedNode {
            id: NodeId(id.into()),
            parent_id: None,
            node_type: "item".into(),
            pos: "a0".into(),
            current_fields: fields,
            var_name: var.map(str::to_string),
            deleted: false,
        }
    }

    fn state_of(nodes: Vec<MaterializedNode>) -> DocumentState {
        let mut map = BTreeMap::new();
        for n in nodes {
            map.insert(n.id.clone(), n);
        }
        DocumentState {
            nodes: map,
            comments: Vec::new(),
            suggestions: BTreeMap::new(),
            removed_choices: HashSet::new(),
        }
    }

    #[test]
    fn extracts_var_references_in_order_deduped() {
        assert_eq!(
            extract_var_refs("${age} > 18 and ${age} < ${max}"),
            vec!["age".to_string(), "max".to_string()]
        );
        assert_eq!(extract_var_refs("no refs here"), Vec::<String>::new());
        // Unterminated reference yields nothing rather than panicking.
        assert_eq!(extract_var_refs("${oops"), Vec::<String>::new());
        // Whitespace inside the braces is trimmed.
        assert_eq!(extract_var_refs("${ spaced }"), vec!["spaced".to_string()]);
    }

    #[test]
    fn clean_instrument_has_no_violations() {
        let state = state_of(vec![
            node("01A", Some("age"), json!({ "label": "Age" })),
            node(
                "01B",
                Some("adult"),
                json!({ "label": "Adult?", "relevance": "${age} >= 18" }),
            ),
        ]);
        assert!(validate_integrity(&state).is_empty());
    }

    #[test]
    fn dangling_reference_is_reported() {
        let state = state_of(vec![node(
            "01B",
            Some("adult"),
            json!({ "relevance": "${age} >= 18" }),
        )]);
        let violations = validate_integrity(&state);
        assert_eq!(
            violations,
            vec![IntegrityViolation::DanglingReference {
                node_id: NodeId("01B".into()),
                field: "relevance".into(),
                var_name: "age".into(),
            }]
        );
    }

    #[test]
    fn deleted_node_neither_defines_nor_references() {
        let mut age = node("01A", Some("age"), json!({ "label": "Age" }));
        age.deleted = true;
        // 01B references age, but age's node is tombstoned -> dangling.
        let state = state_of(vec![
            age,
            node("01B", Some("adult"), json!({ "relevance": "${age} >= 18" })),
        ]);
        let violations = validate_integrity(&state);
        assert!(violations
            .iter()
            .any(|v| matches!(v, IntegrityViolation::DanglingReference { var_name, .. } if var_name == "age")));
    }

    #[test]
    fn duplicate_var_name_is_reported() {
        let state = state_of(vec![
            node("01A", Some("dup"), json!({})),
            node("01B", Some("dup"), json!({})),
        ]);
        let violations = validate_integrity(&state);
        assert!(violations.iter().any(|v| matches!(
            v,
            IntegrityViolation::DuplicateVarName { var_name, node_ids }
                if var_name == "dup" && node_ids.len() == 2
        )));
    }

    #[test]
    fn relevance_cycle_is_detected_once() {
        // A depends on B's var, B depends on A's var -> a 2-cycle.
        let state = state_of(vec![
            node("01A", Some("a"), json!({ "relevance": "${b} = 1" })),
            node("01B", Some("b"), json!({ "relevance": "${a} = 1" })),
        ]);
        let cycles: Vec<_> = validate_integrity(&state)
            .into_iter()
            .filter(|v| matches!(v, IntegrityViolation::RelevanceCycle { .. }))
            .collect();
        assert_eq!(cycles.len(), 1, "the cycle is reported exactly once");
        if let IntegrityViolation::RelevanceCycle { cycle } = &cycles[0] {
            assert_eq!(cycle.len(), 2);
            // Canonicalized to start at the smallest id.
            assert_eq!(cycle[0], NodeId("01A".into()));
        }
    }

    #[test]
    fn self_relevance_is_a_cycle() {
        let state = state_of(vec![node(
            "01A",
            Some("a"),
            json!({ "relevance": "${a} = 1" }),
        )]);
        assert!(validate_integrity(&state)
            .iter()
            .any(|v| matches!(v, IntegrityViolation::RelevanceCycle { .. })));
    }

    #[test]
    fn constraint_reference_does_not_form_a_cycle() {
        // A constraint that references its own var is a dangling/no-op, but never a
        // *relevance* cycle (constraints don't gate visibility).
        let state = state_of(vec![node(
            "01A",
            Some("a"),
            json!({ "constraint": ". > ${a}" }),
        )]);
        let cycles = validate_integrity(&state)
            .into_iter()
            .filter(|v| matches!(v, IntegrityViolation::RelevanceCycle { .. }))
            .count();
        assert_eq!(cycles, 0);
    }
}
