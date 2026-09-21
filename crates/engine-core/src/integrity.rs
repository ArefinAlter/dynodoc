//! Referential integrity pass over the materialized state (docs/16 part 4, extended in
//! docs/21 part 2).
//!
//! Items carry `relevance`, `constraint`, and `calculation` expressions (ODK-style
//! `${var}` references, docs/04 §C.3.6). Ways an instrument can be *silently* broken:
//!
//! 1. **Dangling reference** — an expression names a `var_name` that no live node
//!    defines (a typo, or the referenced item was deleted). At runtime the form
//!    skip-logic evaluates against a missing variable and misbehaves in the field.
//! 2. **Relevance cycle** — item A's visibility depends (transitively) on item B's,
//!    whose depends back on A's. No assignment of answers settles the skip logic.
//! 3. **Type mismatch** — a numeric operator applied to a non-numeric `${var}`
//!    (stage 21, conservative: equality and unknown types are never flagged).
//! 4. **Undefined choice** — a `selected(${q}, 'lit')` whose select-type `q` exists but
//!    has no live choice equal to `'lit'` (stage 21).
//!
//! [`validate_integrity`] finds every such violation in a `DocumentState` in one pass.
//! The governance gate (docs/16 part 3) runs it on every `Deployed` event and refuses
//! the deploy if any violation exists, and the stage-21 three-way merge
//! ([`crate::merge`]) runs it on every proposed merged state — the §C.3.6 "silent
//! semantic breakage" defense. The richer stage-21 checks parse expressions via
//! [`crate::expr`]; the existing `${var}` scan ([`extract_var_refs`]) still backs the
//! dangling/cycle logic, and expressions outside the supported grammar block deployment explicitly.

use std::collections::{BTreeMap, BTreeSet};

use engine_shared::NodeId;
use serde::{Deserialize, Serialize};

use crate::expr;
use crate::materializer::{DocumentState, MaterializedNode};

/// The `current_fields` keys whose string values are reference-bearing expressions
/// (docs/21 item 2: relevance, constraint, calculation). All are scanned for `${var}`
/// references and parsed for the richer type/choice checks.
const EXPRESSION_FIELDS: [&str; 4] = [
    "relevance",
    "constraint",
    "calculation",
    "required_expression",
];

/// The set of `current_fields["type"]` values that are non-numeric — applying an
/// arithmetic or ordering operator to a var of one of these types is a clear mismatch.
const NON_NUMERIC_TYPES: [&str; 3] = ["text", "select_one", "select_multiple"];

/// The `current_fields["type"]` values that carry a choice list (so a
/// `selected(${q}, 'lit')` reference is meaningful against them).
const SELECT_TYPES: [&str; 2] = ["select_one", "select_multiple"];

/// A single referential-integrity problem found in a [`DocumentState`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum IntegrityViolation {
    /// A branch operation could not be replayed against its proposed base.
    InvalidOperation { reason: String },
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
    /// An expression applies a numeric operator (arithmetic, or an ordering comparison
    /// `< <= > >=`) to a `${var}` whose defining node has a non-numeric data type
    /// (`text` / `select_one` / `select_multiple`). Equality (`= !=`) is never flagged
    /// — it is valid on any type — and a var whose type is absent/unknown is left
    /// alone, so the check is conservative and avoids false positives.
    InvalidExpression {
        node_id: NodeId,
        field: String,
        reason: String,
    },
    TypeMismatch {
        node_id: NodeId,
        /// Which expression field carried the operation.
        field: String,
        var_name: String,
        /// What the operator requires (`"numeric"`).
        expected: String,
        /// The referenced var's actual data type.
        actual: String,
    },
    /// A `selected(${q}, 'lit')` reference whose var `q` resolves to a live select
    /// node, but that node has no live `choice` child whose value equals `'lit'`. A
    /// dangling `q` is reported as [`IntegrityViolation::DanglingReference`] instead, so
    /// this variant never double-reports it.
    UndefinedChoice {
        node_id: NodeId,
        /// Which expression field carried the reference.
        field: String,
        var_name: String,
        /// The choice literal that resolved to no live choice on `var_name`'s node.
        choice: String,
    },
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

    // Type-mismatch and choice-reference checks. These need expression *shape*, so we
    // Parse supported expression syntax. Invalid/unsupported syntax must not be
    // interpreted as a successful validation. Imported vendor extensions remain
    // editable, but deployment requires validation in their destination platform.
    // Build the variable type map and
    // live choice values under their parent select node.
    let mut node_var_type: BTreeMap<&str, &str> = BTreeMap::new();
    for node in &live {
        if let (Some(var), Some(ty)) = (
            node.var_name.as_deref().filter(|v| !v.is_empty()),
            node.current_fields
                .get("question_type")
                .or_else(|| node.current_fields.get("type"))
                .and_then(|v| v.as_str()),
        ) {
            node_var_type.insert(var, ty);
        }
    }
    // var_name -> set of live choice values defined under that var's node.
    let mut choices_by_var: BTreeMap<&str, BTreeSet<String>> = BTreeMap::new();
    // node_id -> var_name, so we can attribute a choice child to its parent's var.
    let var_of_node: BTreeMap<&NodeId, &str> = live
        .iter()
        .filter_map(|n| {
            n.var_name
                .as_deref()
                .filter(|v| !v.is_empty())
                .map(|v| (&n.id, v))
        })
        .collect();
    for node in &live {
        if node.node_type != "choice" || state.removed_choices.contains(&node.id) {
            continue;
        }
        let Some(parent) = node.parent_id.as_ref() else {
            continue;
        };
        let Some(parent_var) = var_of_node.get(parent) else {
            continue;
        };
        if let Some(value) = choice_value(node) {
            choices_by_var.entry(parent_var).or_default().insert(value);
        }
    }

    for node in &live {
        for field in EXPRESSION_FIELDS {
            let Some(text) = node.current_fields.get(field).and_then(|v| v.as_str()) else {
                continue;
            };
            if text.trim().is_empty() {
                continue;
            }
            let ast = match expr::parse(text) {
                Ok(ast) => ast,
                Err(error) => {
                    violations.push(IntegrityViolation::InvalidExpression {
                        node_id: node.id.clone(),
                        field: field.to_string(),
                        reason: error.to_string(),
                    });
                    continue;
                }
            };

            // Type mismatches: a numeric operator over a non-numeric var.
            for (var_name, actual) in numeric_var_usages(&ast, &node_var_type) {
                violations.push(IntegrityViolation::TypeMismatch {
                    node_id: node.id.clone(),
                    field: field.to_string(),
                    var_name,
                    expected: "numeric".to_string(),
                    actual,
                });
            }

            // Choice references: `selected(${q}, 'lit')` must resolve to a live choice.
            for (var_name, choice) in expr::selected_choice_refs(&ast) {
                // A dangling var is already reported as DanglingReference; skip here.
                if !defined.contains(var_name.as_str()) {
                    continue;
                }
                // Only meaningful against a select-type node.
                let is_select = node_var_type
                    .get(var_name.as_str())
                    .is_some_and(|ty| SELECT_TYPES.contains(ty));
                if !is_select {
                    continue;
                }
                let has_choice = choices_by_var
                    .get(var_name.as_str())
                    .is_some_and(|set| set.contains(&choice));
                if !has_choice {
                    violations.push(IntegrityViolation::UndefinedChoice {
                        node_id: node.id.clone(),
                        field: field.to_string(),
                        var_name,
                        choice,
                    });
                }
            }
        }
    }

    violations
}

/// A node's choice value, by the documented convention: prefer `current_fields["value"]`,
/// fall back to `current_fields["name"]`. Both are read as strings.
fn choice_value(node: &MaterializedNode) -> Option<String> {
    node.current_fields
        .get("value")
        .and_then(|v| v.as_str())
        .or_else(|| node.current_fields.get("name").and_then(|v| v.as_str()))
        .map(str::to_string)
}

/// Collect `(var_name, actual_type)` pairs where a numeric operator (arithmetic or an
/// ordering comparison) is applied to a `${var}` whose declared type is non-numeric.
/// Equality is never numeric-requiring, so it never contributes. De-duplicated and
/// returned in `var_name` order for deterministic output.
fn numeric_var_usages(
    ast: &expr::Expr,
    node_var_type: &BTreeMap<&str, &str>,
) -> Vec<(String, String)> {
    let mut found: BTreeMap<String, String> = BTreeMap::new();
    walk_numeric(ast, node_var_type, &mut found);
    found.into_iter().collect()
}

fn walk_numeric(
    ast: &expr::Expr,
    node_var_type: &BTreeMap<&str, &str>,
    found: &mut BTreeMap<String, String>,
) {
    use expr::Expr;
    match ast {
        Expr::BinOp(op, lhs, rhs) => {
            if op.is_arithmetic() || op.is_ordering() {
                for operand in [lhs.as_ref(), rhs.as_ref()] {
                    if let Expr::VarRef(var) = operand {
                        if let Some(ty) = node_var_type.get(var.as_str()) {
                            if NON_NUMERIC_TYPES.contains(ty) {
                                found.insert(var.clone(), (*ty).to_string());
                            }
                        }
                    }
                }
            }
            walk_numeric(lhs, node_var_type, found);
            walk_numeric(rhs, node_var_type, found);
        }
        Expr::UnaryOp(_, operand) => walk_numeric(operand, node_var_type, found),
        Expr::Call(_, args) => {
            for arg in args {
                walk_numeric(arg, node_var_type, found);
            }
        }
        Expr::VarRef(_) | Expr::Literal(_) | Expr::Dot => {}
    }
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

    fn choice(id: &str, parent: &str, value: &str) -> MaterializedNode {
        MaterializedNode {
            id: NodeId(id.into()),
            parent_id: Some(NodeId(parent.into())),
            node_type: "choice".into(),
            pos: "a0".into(),
            current_fields: json!({ "value": value }),
            var_name: None,
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

    #[test]
    fn type_mismatch_numeric_op_on_text_var() {
        // q1 is text; `${q1} > 5` applies an ordering comparison to it.
        let state = state_of(vec![
            node(
                "01A",
                Some("q1"),
                json!({ "type": "text", "label": "Name" }),
            ),
            node("01B", Some("q2"), json!({ "relevance": "${q1} > 5" })),
        ]);
        let violations = validate_integrity(&state);
        assert!(violations.iter().any(|v| matches!(
            v,
            IntegrityViolation::TypeMismatch { var_name, actual, .. }
                if var_name == "q1" && actual == "text"
        )));
    }

    #[test]
    fn equality_on_text_var_is_not_a_mismatch() {
        // `=` is valid on any type, so no TypeMismatch is raised.
        let state = state_of(vec![
            node("01A", Some("q1"), json!({ "type": "text" })),
            node("01B", Some("q2"), json!({ "relevance": "${q1} = 'x'" })),
        ]);
        assert!(validate_integrity(&state)
            .iter()
            .all(|v| !matches!(v, IntegrityViolation::TypeMismatch { .. })));
    }

    #[test]
    fn unknown_type_is_not_flagged() {
        // q1 declares no `type` -> conservative: no TypeMismatch.
        let state = state_of(vec![
            node("01A", Some("q1"), json!({ "label": "Q" })),
            node("01B", Some("q2"), json!({ "relevance": "${q1} > 5" })),
        ]);
        assert!(validate_integrity(&state)
            .iter()
            .all(|v| !matches!(v, IntegrityViolation::TypeMismatch { .. })));
    }

    #[test]
    fn undefined_choice_reference_is_reported() {
        // q1 is select_one with choice 'no' only; constraint references 'yes'.
        let state = state_of(vec![
            node("01A", Some("q1"), json!({ "type": "select_one" })),
            choice("01A-c1", "01A", "no"),
            node(
                "01B",
                Some("q2"),
                json!({ "constraint": "selected(${q1}, 'yes')" }),
            ),
        ]);
        let violations = validate_integrity(&state);
        assert!(violations.iter().any(|v| matches!(
            v,
            IntegrityViolation::UndefinedChoice { var_name, choice, .. }
                if var_name == "q1" && choice == "yes"
        )));
    }

    #[test]
    fn present_choice_reference_is_clean() {
        // The 'yes' choice exists -> no UndefinedChoice, no other violation.
        let state = state_of(vec![
            node("01A", Some("q1"), json!({ "type": "select_one" })),
            choice("01A-c1", "01A", "yes"),
            node(
                "01B",
                Some("q2"),
                json!({ "constraint": "selected(${q1}, 'yes')" }),
            ),
        ]);
        assert!(validate_integrity(&state).is_empty());
    }

    #[test]
    fn choice_value_falls_back_to_name() {
        let mut ch = choice("01A-c1", "01A", "ignored");
        ch.current_fields = json!({ "name": "yes" });
        let state = state_of(vec![
            node("01A", Some("q1"), json!({ "type": "select_one" })),
            ch,
            node(
                "01B",
                Some("q2"),
                json!({ "constraint": "selected(${q1}, 'yes')" }),
            ),
        ]);
        assert!(validate_integrity(&state)
            .iter()
            .all(|v| !matches!(v, IntegrityViolation::UndefinedChoice { .. })));
    }

    #[test]
    fn dangling_choice_var_reported_once_not_double() {
        // q1 does not exist at all: a DanglingReference, but NOT also UndefinedChoice.
        let state = state_of(vec![node(
            "01B",
            Some("q2"),
            json!({ "constraint": "selected(${q1}, 'yes')" }),
        )]);
        let violations = validate_integrity(&state);
        assert!(violations
            .iter()
            .any(|v| matches!(v, IntegrityViolation::DanglingReference { .. })));
        assert!(violations
            .iter()
            .all(|v| !matches!(v, IntegrityViolation::UndefinedChoice { .. })));
    }

    #[test]
    fn malformed_logic_is_not_a_clean_instrument() {
        let state = state_of(vec![node(
            "01A",
            Some("age"),
            json!({"constraint":". > ("}),
        )]);
        assert!(validate_integrity(&state)
            .iter()
            .any(|v| matches!(v, IntegrityViolation::InvalidExpression { .. })));
    }
    #[test]
    fn workspace_question_type_participates_in_validation() {
        let state = state_of(vec![
            node("01A", Some("name"), json!({"question_type":"text"})),
            node("01B", Some("total"), json!({"calculation":"${name} + 2"})),
        ]);
        assert!(validate_integrity(&state)
            .iter()
            .any(|v| matches!(v, IntegrityViolation::TypeMismatch { .. })));
    }
    #[test]
    fn calculation_field_is_scanned_for_dangling_refs() {
        let state = state_of(vec![node(
            "01B",
            Some("total"),
            json!({ "calculation": "${a} + ${b}" }),
        )]);
        let violations = validate_integrity(&state);
        assert_eq!(
            violations
                .iter()
                .filter(|v| matches!(v, IntegrityViolation::DanglingReference { field, .. } if field == "calculation"))
                .count(),
            2
        );
    }
}
