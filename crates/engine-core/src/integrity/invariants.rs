//! Read-only structural diagnostics, separate from questionnaire expressions and
//! append validation. This does not reinterpret or reject historical events.
use std::collections::{BTreeMap, BTreeSet};

use engine_shared::NodeId;
use serde::{Deserialize, Serialize};

use crate::materializer::{DocumentState, MaterializedNode};

/// A broken identity, live parent link, parent cycle or live-name uniqueness rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum InvariantViolation {
    NodeIdMismatch {
        key: NodeId,
        node_id: NodeId,
    },
    MissingParent {
        node_id: NodeId,
        parent_id: NodeId,
    },
    NonLiveParent {
        node_id: NodeId,
        parent_id: NodeId,
    },
    ParentCycle {
        node_ids: Vec<NodeId>,
    },
    DuplicateVarName {
        var_name: String,
        node_ids: Vec<NodeId>,
    },
}

/// Diagnose structural invariants in deterministic node-key order without mutation.
/// "Live" excludes both tombstones and the historical removed-choice marker.
/// Multiple roots and equal sibling positions are deliberately allowed: this is
/// not a new root/order policy or a complete format/semantic validator.
/// Parent walks are iterative and visit each live node at most once.
pub fn check_invariants(state: &DocumentState) -> Vec<InvariantViolation> {
    let live = |key: &NodeId, node: &MaterializedNode| {
        !node.deleted && !state.removed_choices.contains(key)
    };
    let mut violations = Vec::new();
    let mut names: BTreeMap<&str, Vec<NodeId>> = BTreeMap::new();
    for (key, node) in &state.nodes {
        if key != &node.id {
            violations.push(InvariantViolation::NodeIdMismatch {
                key: key.clone(),
                node_id: node.id.clone(),
            });
        }
        if !live(key, node) {
            continue;
        }
        if let Some(parent_id) = &node.parent_id {
            match state.nodes.get(parent_id) {
                None => violations.push(InvariantViolation::MissingParent {
                    node_id: key.clone(),
                    parent_id: parent_id.clone(),
                }),
                Some(parent) if !live(parent_id, parent) => {
                    violations.push(InvariantViolation::NonLiveParent {
                        node_id: key.clone(),
                        parent_id: parent_id.clone(),
                    });
                }
                _ => {}
            }
        }
        if let Some(name) = node.var_name.as_deref().filter(|name| !name.is_empty()) {
            names.entry(name).or_default().push(key.clone());
        }
    }
    let mut visited = BTreeSet::new();
    for (start, node) in &state.nodes {
        if !live(start, node) || visited.contains(start) {
            continue;
        }
        let mut path = Vec::new();
        let mut on_path = BTreeMap::new();
        let mut cursor = Some(start);
        while let Some(key) = cursor {
            let Some(node) = state.nodes.get(key).filter(|node| live(key, node)) else {
                break;
            };
            if let Some(&index) = on_path.get(key) {
                let mut cycle: Vec<NodeId> = path[index..]
                    .iter()
                    .map(|id: &&NodeId| (*id).clone())
                    .collect();
                let min = cycle
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, id)| *id)
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                cycle.rotate_left(min);
                violations.push(InvariantViolation::ParentCycle { node_ids: cycle });
                break;
            }
            if visited.contains(key) {
                break;
            }
            on_path.insert(key, path.len());
            path.push(key);
            cursor = node.parent_id.as_ref();
        }
        visited.extend(path);
    }
    for (name, ids) in names {
        if ids.len() > 1 {
            violations.push(InvariantViolation::DuplicateVarName {
                var_name: name.into(),
                node_ids: ids,
            });
        }
    }
    violations
}
