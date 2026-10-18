//! `merge_with_policy` under `dynodoc.default.v1` is a no-op: over random histories
//! it returns, byte for byte, the plan `workspace_merge::merge` returns, for the
//! unresolved merge and for several sets of explicit resolutions. The histories
//! cover every event type (`support/gen.rs`), and the sides add rich-text edits,
//! formatting, page setup and field edits, so the rich-text and property strategies
//! and the explicit-resolution paths are all reached; the last test counts what the
//! generator reached.
#[path = "support/gen.rs"]
mod gen;

use engine_core::{
    governance::{authorize, Role},
    materializer::{apply_payload, DocumentState, Materializer},
    merge_policy::{default, evidence::DecisionOutcome, MergeEvidence},
    richtext,
    workspace_merge::{merge, merge_with_policy, MergePlan},
};
use engine_shared::{EventPayload, NodeId};
use serde_json::{json, Value};
use std::collections::BTreeMap;

struct Rng(u64);
impl Rng {
    fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }
}

fn apply(state: &mut DocumentState, op: EventPayload) -> bool {
    let ok = authorize(Role::Author, &op, state).is_ok();
    if ok {
        apply_payload(state, &op).unwrap();
    }
    ok
}

fn base(seed: u64) -> DocumentState {
    let mut rng = Rng(seed | 1);
    Materializer::fold(&gen::history(seed, 8 + rng.below(30))).unwrap()
}

const WORDS: [&str; 6] = ["north", "south", "east", "west", "up", "down"];

fn live_with(
    state: &DocumentState,
    keep: impl Fn(&engine_core::materializer::MaterializedNode) -> bool,
) -> Vec<NodeId> {
    state
        .nodes
        .values()
        .filter(|n| !n.deleted && keep(n))
        .map(|n| n.id.clone())
        .collect()
}

fn side(rng: &mut Rng, base: &DocumentState, seed: u64) -> DocumentState {
    let mut state = base.clone();
    let mut gen = gen::Gen::new(seed);
    for _ in 0..1 + rng.below(6) {
        match rng.below(10) {
            0..=4 => {
                if let Some(op) = gen.op(&state, &[]) {
                    apply(&mut state, op);
                }
            }
            // Replace one word of a rich-text paragraph, or format a span of it.
            5..=7 => {
                let rich = live_with(&state, |n| n.current_fields["content"].is_object());
                if rich.is_empty() {
                    continue;
                }
                let id = rich[rng.below(rich.len())].clone();
                let before = state.nodes[&id].current_fields["content"].clone();
                let text = richtext::plain_text(&before);
                let mut words: Vec<String> = text.split(' ').map(String::from).collect();
                let at = rng.below(words.len());
                let after = if rng.below(2) == 0 {
                    words[at] = WORDS[rng.below(WORDS.len())].into();
                    gen::paragraph(&words.join(" "))
                } else {
                    let (head, tail) = words.split_at(at);
                    let (mark, rest) = tail.split_first().unwrap();
                    let run = |t: String, marks: Option<Value>| {
                        let mut r = json!({"type":"text","text":t});
                        if let Some(m) = marks {
                            r["marks"] = m;
                        }
                        r
                    };
                    let mut content = Vec::new();
                    if !head.is_empty() {
                        content.push(run(format!("{} ", head.join(" ")), None));
                    }
                    let kind = ["bold", "italic"][rng.below(2)];
                    content.push(run(mark.clone(), Some(json!([{"type": kind}]))));
                    if !rest.is_empty() {
                        content.push(run(format!(" {}", rest.join(" ")), None));
                    }
                    json!({"type":"paragraph","content":content})
                };
                if let Some(patch) = richtext::make_patch(&before, &after) {
                    apply(
                        &mut state,
                        EventPayload::RichTextPatched { node_id: id, patch },
                    );
                }
            }
            // Page setup on a root, key by key.
            8 => {
                let roots = live_with(&state, |n| n.parent_id.is_none());
                if roots.is_empty() {
                    continue;
                }
                let id = roots[rng.below(roots.len())].clone();
                let mut setup = state.nodes[&id].current_fields["page_setup"].clone();
                if !setup.is_object() {
                    setup = json!({"size": "A4", "margin": 25});
                }
                let key = ["size", "margin", "landscape"][rng.below(3)];
                setup[key] = json!(rng.below(3));
                apply(
                    &mut state,
                    EventPayload::FieldEdited {
                        node_id: id,
                        field: "page_setup".into(),
                        value: setup,
                    },
                );
            }
            // A small alphabet, so equal edits and one-sided edits happen too.
            _ => {
                let nodes = live_with(&state, |n| n.node_type != "choice");
                if nodes.is_empty() {
                    continue;
                }
                apply(
                    &mut state,
                    EventPayload::FieldEdited {
                        node_id: nodes[rng.below(nodes.len())].clone(),
                        field: ["hint", "note"][rng.below(2)].into(),
                        value: json!(rng.below(3)),
                    },
                );
            }
        }
    }
    state
}

fn histories(seed: u64) -> (DocumentState, DocumentState, DocumentState) {
    let mut rng = Rng(seed.wrapping_mul(0x9e37_79b9_7f4a_7c15) | 1);
    let base = base(seed);
    let team = side(&mut rng, &base, seed ^ 0x1111);
    let draft = side(&mut rng, &base, seed ^ 0x2222);
    (base, team, draft)
}

/// A plan as bytes: state, conflicts and operations.
fn bytes(plan: &MergePlan) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "state": plan.state,
        "conflicts": plan.conflicts,
        "ops": plan.ops,
    }))
    .unwrap()
}

/// Resolution sets built from the conflicts a merge reports: none, everything for
/// the team, everything for the draft, and a mix that also renames variables.
fn resolution_sets(plan: &MergePlan) -> Vec<BTreeMap<String, String>> {
    let pick = |choose: &dyn Fn(usize, &str) -> String| -> BTreeMap<String, String> {
        plan.conflicts
            .iter()
            .enumerate()
            .map(|(i, c)| (c.key.clone(), choose(i, &c.field)))
            .collect()
    };
    vec![
        BTreeMap::new(),
        pick(&|_, _| "team".into()),
        pick(&|_, _| "draft".into()),
        pick(&|i, field| {
            if field == "var_name" {
                format!("renamed_{i}")
            } else if i % 2 == 0 {
                "draft".into()
            } else {
                "team".into()
            }
        }),
    ]
}

#[derive(Default, Debug)]
struct Seen {
    comparisons: u32,
    conflicted: u32,
    decisions: u32,
    merged_by: BTreeMap<String, u32>,
    conflicts_by: BTreeMap<String, u32>,
    explicit: u32,
    structural: u32,
    shortcuts: u64,
}

fn compare(seed: u64, seen: &mut Seen) {
    let (base, team, draft) = histories(seed);
    let policy = default::policy();
    let unresolved = merge(&base, &team, &draft, &BTreeMap::new());
    for resolutions in resolution_sets(&unresolved) {
        let expected = merge(&base, &team, &draft, &resolutions);
        let (actual, evidence) = merge_with_policy(&base, &team, &draft, &resolutions, policy);
        assert_eq!(
            String::from_utf8_lossy(&bytes(&actual)),
            String::from_utf8_lossy(&bytes(&expected)),
            "seed {seed}, resolutions {resolutions:?}"
        );
        seen.comparisons += 1;
        seen.conflicted += !expected.conflicts.is_empty() as u32;
        tally(&evidence, &actual, seen);
        // Every plan conflict is a recorded decision or a named tree check.
        for c in &actual.conflicts {
            let decided = evidence
                .decisions
                .iter()
                .any(|d| d.conflict_key.as_deref() == Some(c.key.as_str()));
            let structural = evidence.structural.iter().any(|s| s.key == c.key);
            assert!(decided || structural, "seed {seed}: unexplained {}", c.key);
        }
    }
}

fn tally(evidence: &MergeEvidence, plan: &MergePlan, seen: &mut Seen) {
    seen.decisions += evidence.decisions.len() as u32;
    seen.shortcuts += evidence.shortcuts.unchanged_or_team_only + evidence.shortcuts.draft_only;
    seen.structural += evidence.structural.len() as u32;
    for d in &evidence.decisions {
        let strategy = d.strategy.clone().unwrap_or_default();
        match d.outcome {
            DecisionOutcome::Merged => *seen.merged_by.entry(strategy).or_default() += 1,
            DecisionOutcome::Conflict => *seen.conflicts_by.entry(strategy).or_default() += 1,
            DecisionOutcome::Explicit => seen.explicit += 1,
            DecisionOutcome::Legacy => unreachable!("the default policy has no collection rule"),
        }
    }
    // The evidence never alters the plan: under the default policy it adds no conflict.
    assert!(plan.conflicts.iter().all(|c| c.field != "$invariant"));
}

/// Histories compared; `MERGE_POLICY_NOOP_SEEDS` raises it for a longer run.
fn seeds() -> u64 {
    std::env::var("MERGE_POLICY_NOOP_SEEDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1500)
}

#[test]
fn the_default_policy_returns_the_plan_merge_returns() {
    let mut seen = Seen::default();
    for seed in 1..=seeds() {
        compare(seed, &mut seen);
    }
    eprintln!("{seen:#?}");
    assert!(seen.comparisons as u64 >= 4 * seeds());
    // The comparison is not vacuous: the generator reaches each way a field is
    // decided. Floors are about half of what seeds 1..=1500 produced (see the printed
    // counts; a whole-value choice never "merges", the shortcuts take those).
    assert!(seen.conflicted > 200, "{seen:?}");
    assert!(seen.decisions > 1200, "{seen:?}");
    assert!(seen.explicit > 600, "{seen:?}");
    assert!(seen.structural >= 4, "{seen:?}");
    assert!(seen.shortcuts > 12_000, "{seen:?}");
    for (strategy, floor) in [
        ("dynodoc.default.richtext.v1", 300),
        ("dynodoc.default.property.v1", 8),
    ] {
        assert!(
            seen.merged_by.get(strategy).copied().unwrap_or(0) >= floor,
            "{strategy} merged: {seen:?}"
        );
    }
    for (strategy, floor) in [
        ("dynodoc.default.richtext.v1", 90),
        ("dynodoc.default.property.v1", 60),
        ("dynodoc.default.whole-value.v1", 60),
    ] {
        assert!(
            seen.conflicts_by.get(strategy).copied().unwrap_or(0) >= floor,
            "{strategy} conflicts: {seen:?}"
        );
    }
}

#[test]
fn the_policy_run_is_a_pure_function_of_its_inputs() {
    for seed in [3u64, 77, 1234] {
        let (base, team, draft) = histories(seed);
        let run = || merge_with_policy(&base, &team, &draft, &BTreeMap::new(), default::policy());
        let (a, ea) = run();
        let (b, eb) = run();
        assert_eq!(bytes(&a), bytes(&b));
        assert_eq!(ea, eb);
        assert_eq!(ea.to_json(), eb.to_json());
        assert_eq!(
            ea.render_text(Default::default()),
            eb.render_text(Default::default())
        );
    }
}

#[test]
fn merging_a_state_with_itself_or_an_unchanged_side_decides_nothing() {
    for seed in 1..=200u64 {
        let (base, team, draft) = histories(seed);
        for (t, d) in [
            (&team, &team),
            (&base, &draft),
            (&team, &base),
            (&base, &base),
        ] {
            let (plan, evidence) =
                merge_with_policy(&base, t, d, &BTreeMap::new(), default::policy());
            assert!(plan.conflicts.is_empty(), "seed {seed}");
            assert!(evidence.decisions.is_empty(), "seed {seed}");
            assert_eq!(bytes(&plan), bytes(&merge(&base, t, d, &BTreeMap::new())));
        }
    }
}
