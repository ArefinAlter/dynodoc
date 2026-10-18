//! Merge laws over random histories. Both sides start from one base and make only
//! changes an author may make. One known limit is left out: a side never takes a name
//! that a block it keeps still held in the base (so no two names are swapped).
use engine_core::{
    governance::{authorize, Role},
    materializer::{apply_payload, DocumentState},
    ops::validate_op,
    workspace_merge::merge,
};
use engine_shared::{EventPayload, NodeId, NodeType};
use proptest::{prelude::*, test_runner::RngSeed};
use serde_json::json;
use std::collections::BTreeMap;

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

/// Applies `op` if an author may make it.
fn apply(state: &mut DocumentState, op: EventPayload) -> bool {
    let ok = authorize(Role::Author, &op, state).is_ok();
    if ok {
        apply_payload(state, &op).unwrap();
    }
    ok
}

fn ids(
    state: &DocumentState,
    keep: impl Fn(&engine_core::materializer::MaterializedNode) -> bool,
) -> Vec<NodeId> {
    state
        .nodes
        .values()
        .filter(|n| keep(n))
        .map(|n| n.id.clone())
        .collect()
}

fn create(
    id: String,
    node_type: NodeType,
    parent: NodeId,
    rng: &mut Rng,
    var_name: Option<String>,
) -> EventPayload {
    EventPayload::NodeCreated {
        node_id: NodeId(id),
        node_type,
        parent_id: Some(parent),
        pos: format!("p{}", rng.below(4)),
        fields: json!({"label": format!("L{}", rng.below(3))}),
        var_name,
    }
}

fn base(rng: &mut Rng) -> DocumentState {
    let mut state = DocumentState::default();
    let root = NodeId("root".into());
    apply(
        &mut state,
        EventPayload::NodeCreated {
            node_id: root.clone(),
            node_type: NodeType::Form,
            parent_id: None,
            pos: "a0".into(),
            fields: json!({"label": "Root"}),
            var_name: None,
        },
    );
    for i in 0..3 + rng.below(6) {
        let parents = ids(&state, |n| !n.deleted && n.node_type != "item");
        let parent = parents[rng.below(parents.len())].clone();
        let node_type = if rng.below(2) == 0 {
            NodeType::Section
        } else {
            NodeType::Item
        };
        let var_name = (rng.below(2) == 0).then(|| format!("v{i}"));
        apply(
            &mut state,
            create(format!("b{i}"), node_type, parent, rng, var_name),
        );
    }
    // Some deleted blocks, so that restores happen.
    for _ in 0..rng.below(3) {
        let leaves = ids(&state, |n| n.parent_id.is_some() && !n.deleted);
        let leaf = leaves[rng.below(leaves.len())].clone();
        apply(&mut state, EventPayload::NodeDeleted { node_id: leaf });
    }
    state
}

fn side(rng: &mut Rng, base: &DocumentState, prefix: &str) -> DocumentState {
    let mut state = base.clone();
    let names: Vec<String> = (0..4)
        .map(|i| format!("p{i}"))
        .chain((0..8).map(|i| format!("v{i}")))
        .collect();
    for step in 0..1 + rng.below(6) {
        let name = names[rng.below(names.len())].clone();
        // A name held in the base may be taken only from a block this side deleted.
        let free = !base.nodes.values().any(|b| {
            !b.deleted
                && b.var_name.as_deref() == Some(&name)
                && state.nodes.get(&b.id).is_some_and(|n| !n.deleted)
        });
        let live = ids(&state, |n| !n.deleted && n.parent_id.is_some());
        let containers = ids(&state, |n| !n.deleted && n.node_type != "item");
        let op = match rng.below(6) {
            0 => {
                let node_type = if rng.below(2) == 0 {
                    NodeType::Section
                } else {
                    NodeType::Item
                };
                let parent = containers[rng.below(containers.len())].clone();
                create(
                    format!("{prefix}{step}"),
                    node_type,
                    parent,
                    rng,
                    free.then_some(name),
                )
            }
            1 if !live.is_empty() => EventPayload::NodeMoved {
                node_id: live[rng.below(live.len())].clone(),
                new_parent_id: Some(containers[rng.below(containers.len())].clone()),
                new_pos: format!("m{}", rng.below(4)),
            },
            2 if !live.is_empty() => EventPayload::NodeDeleted {
                node_id: live[rng.below(live.len())].clone(),
            },
            3 => {
                let deleted = ids(&state, |n| n.deleted);
                if deleted.is_empty() {
                    continue;
                }
                EventPayload::NodeRestored {
                    node_id: deleted[rng.below(deleted.len())].clone(),
                }
            }
            4 if !live.is_empty() => EventPayload::FieldEdited {
                node_id: live[rng.below(live.len())].clone(),
                field: "label".into(),
                value: json!(format!("{prefix}{step}")),
            },
            5 if !live.is_empty() && free => EventPayload::FieldEdited {
                node_id: live[rng.below(live.len())].clone(),
                field: "var_name".into(),
                value: json!(name),
            },
            _ => continue,
        };
        apply(&mut state, op);
    }
    state
}

/// A fixed seed makes a red run reproducible; `PROPTEST_RNG_SEED` and `PROPTEST_CASES`
/// still override it (for example in a scheduled run with a fresh seed). Nothing is
/// persisted: this directory has no lib.rs or main.rs for proptest to place a file by.
fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);
    let mut config = ProptestConfig::with_cases(cases);
    if std::env::var_os("PROPTEST_RNG_SEED").is_none() {
        config.rng_seed = RngSeed::Fixed(0x6d65_7267_6500);
    }
    config.failure_persistence = None;
    config
}

fn histories(seed: u64) -> (DocumentState, DocumentState, DocumentState) {
    let mut rng = Rng(seed);
    let base = base(&mut rng);
    let team = side(&mut rng, &base, "t");
    let draft = side(&mut rng, &base, "d");
    (base, team, draft)
}

proptest! {
    #![proptest_config(config())]
    #[test]
    fn merge_laws(seed in 1..u64::MAX) {
        let (base, team, draft) = histories(seed);

        // (b) The same change on both sides merges to that change, without conflicts.
        let same = merge(&base, &team, &team, &BTreeMap::new());
        prop_assert!(same.conflicts.is_empty());
        prop_assert!(same.ops.is_empty());

        // (d) One side unchanged: the other side's version, without conflicts.
        let only_draft = merge(&base, &base, &draft, &BTreeMap::new());
        prop_assert!(only_draft.conflicts.is_empty());
        prop_assert_eq!(&only_draft.state.nodes, &draft.nodes);
        let only_team = merge(&base, &team, &base, &BTreeMap::new());
        prop_assert!(only_team.conflicts.is_empty());
        prop_assert_eq!(&only_team.state.nodes, &team.nodes);

        // (a) A field changed on one side takes that side's value; two different
        // changes to one field are a conflict, never a silent loss.
        let plan = merge(&base, &team, &draft, &BTreeMap::new());
        for (id, b) in &base.nodes {
            let (Some(t), Some(d), Some(m)) = (team.nodes.get(id), draft.nodes.get(id), plan.state.nodes.get(id)) else {
                continue;
            };
            if [b, t, d, m].iter().any(|n| n.deleted) {
                continue;
            }
            let label = |n: &engine_core::materializer::MaterializedNode| n.current_fields["label"].clone();
            if label(t) == label(b) {
                prop_assert_eq!(label(m), label(d));
            } else if label(d) == label(b) {
                prop_assert_eq!(label(m), label(t));
            } else if label(t) != label(d) {
                let key = format!("{}:label", id.0);
                prop_assert!(plan.conflicts.iter().any(|c| c.key == key), "seed {}: {}", seed, key);
            }
        }

        // The merge is a pure function of its inputs.
        let again = merge(&base, &team, &draft, &BTreeMap::new());
        prop_assert_eq!(&again.state.nodes, &plan.state.nodes);
        prop_assert_eq!(&again.ops, &plan.ops);

        // (c) Without conflicts, or once every conflict is resolved for the team, the
        // operations apply to the team version under the same validation. A future
        // outcome that no choice can clear (for example a blocked or oversized merge)
        // must relax this law on purpose.
        let mut plan = plan;
        let mut resolutions = BTreeMap::new();
        for _ in 0..10 {
            if plan.conflicts.is_empty() {
                break;
            }
            for c in &plan.conflicts {
                resolutions.insert(c.key.clone(), "team".to_string());
            }
            plan = merge(&base, &team, &draft, &resolutions);
        }
        prop_assert!(plan.conflicts.is_empty());
        let mut applied = team.clone();
        for op in &plan.ops {
            prop_assert!(validate_op(&applied, op).is_ok(), "{:?} on seed {}", validate_op(&applied, op), seed);
            apply_payload(&mut applied, op).unwrap();
        }
        // The operations reproduce the planned state. What a deleted block holds is not
        // observable (it cannot be edited), and a block the draft created and deleted
        // never reaches the team, so tombstones compare by presence only.
        let visible = |s: &DocumentState| -> BTreeMap<_, _> {
            s.nodes
                .iter()
                .filter(|(id, n)| !n.deleted || team.nodes.contains_key(*id))
                .map(|(id, n)| {
                    // `var_name` also lives in the fields once it has been edited; the
                    // column is what matters.
                    let mut n = n.clone();
                    if let Some(fields) = n.current_fields.as_object_mut() {
                        fields.remove("var_name");
                    }
                    (id.clone(), (!n.deleted).then_some(n))
                })
                .collect()
        };
        prop_assert_eq!(visible(&applied), visible(&plan.state), "seed {}", seed);
    }
}

/// The laws mean little if the generator never reaches the cases they guard, so
/// this counts what a fixed range of seeds produces.
#[test]
fn the_generator_reaches_the_cases_the_laws_guard() {
    let (mut conflicted, mut location, mut node, mut name, mut label) = (0, 0, 0, 0, 0);
    for seed in 1..=2000u64 {
        let (base, team, draft) = histories(seed);
        let plan = merge(&base, &team, &draft, &BTreeMap::new());
        conflicted += !plan.conflicts.is_empty() as u32;
        let has = |suffix: &str| plan.conflicts.iter().any(|c| c.key.ends_with(suffix));
        location += has(":$location") as u32;
        node += has(":$node") as u32;
        name += has(":var_name") as u32;
        label += has(":label") as u32;
    }
    eprintln!("conflicted={conflicted} location={location} node={node} name={name} label={label}");
    assert!(conflicted >= MIN_CONFLICTED, "conflicted {conflicted}");
    assert!(location >= MIN_LOCATION, "location {location}");
    assert!(node >= MIN_NODE, "node {node}");
    assert!(name >= MIN_NAME, "name {name}");
    assert!(label >= MIN_LABEL, "label {label}");
}
// Floors at about half of what seeds 1..=2000 produced (701, 62, 397, 195, 125).
const MIN_CONFLICTED: u32 = 350;
const MIN_LOCATION: u32 = 30;
const MIN_NODE: u32 = 200;
const MIN_NAME: u32 = 100;
const MIN_LABEL: u32 = 60;
