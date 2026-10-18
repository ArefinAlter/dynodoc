//! What a policy changes and what it records: gates, unknown fields, unknown or
//! failing strategies, the post-merge invariant modes and the evidence rendering.
use engine_core::{
    materializer::{DocumentState, MaterializedNode},
    merge_policy::{
        default,
        evidence::{DecisionOutcome, Digest, Labels},
        strategy::{Context, Inputs, Outcome, Reason, Resolved, Side, Strategy},
        MergePolicy, Registry,
    },
    workspace_merge::{merge, merge_with_policy, merge_with_registry},
};
use engine_shared::NodeId;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn node(id: &str, kind: &str, parent: Option<&str>, fields: Value) -> MaterializedNode {
    MaterializedNode {
        id: NodeId(id.into()),
        parent_id: parent.map(|p| NodeId(p.into())),
        node_type: kind.into(),
        pos: "a".into(),
        current_fields: fields,
        var_name: None,
        deleted: false,
    }
}

fn state(nodes: Vec<MaterializedNode>) -> DocumentState {
    DocumentState {
        nodes: nodes.into_iter().map(|n| (n.id.clone(), n)).collect(),
        ..Default::default()
    }
}

fn para(text: &str) -> Value {
    json!({"type":"paragraph","content":[{"type":"text","text":text}]})
}

fn fields(text: &str, note: &str) -> Value {
    json!({"label": text, "content": para(text), "note": note})
}

const SENTENCE: &str = "The participants were interviewed in June.";

/// Base, plus a team edit and a draft edit of different words of one paragraph and
/// of the same note: rich text merges, the note conflicts.
fn scenario() -> (DocumentState, DocumentState, DocumentState) {
    let base = state(vec![
        node("root", "form", None, json!({})),
        node("p", "item", Some("root"), fields(SENTENCE, "n")),
    ]);
    let edit = |text: &str, note: &str| {
        let mut s = base.clone();
        s.nodes.get_mut(&NodeId("p".into())).unwrap().current_fields = fields(text, note);
        s
    };
    let team = edit(
        &SENTENCE.replace("participants", "respondents"),
        "team note",
    );
    let draft = edit(&SENTENCE.replace("June", "July"), "draft note");
    (base, team, draft)
}

fn policy(rules: &str, defaults: &str) -> MergePolicy {
    let text = format!(r#"{{"version":1,"id":"t","defaults":{defaults},"rules":[{rules}]}}"#);
    MergePolicy::from_json(text.as_bytes(), &Registry::builtin()).unwrap()
}

const CONTENT: &str = r#"{"id":"c","match":{"field":"content"},"strategy":"dynodoc.default.richtext.v1","gate":"GATE"}"#;
const REST: &str =
    r#"{"id":"rest","match":{},"strategy":"dynodoc.default.whole-value.v1","gate":"auto"}"#;

fn rules(gate: &str) -> String {
    format!("{},{REST}", CONTENT.replace("GATE", gate))
}

fn run(
    p: &MergePolicy,
    resolutions: &BTreeMap<String, String>,
) -> (
    engine_core::workspace_merge::MergePlan,
    engine_core::merge_policy::MergeEvidence,
) {
    let (base, team, draft) = scenario();
    merge_with_policy(&base, &team, &draft, resolutions, p)
}

fn decision<'a>(
    e: &'a engine_core::merge_policy::MergeEvidence,
    field: &str,
) -> &'a engine_core::merge_policy::evidence::Decision {
    e.decisions.iter().find(|d| d.field == field).unwrap()
}

#[test]
fn the_default_policy_merges_the_words_and_conflicts_on_the_note() {
    let (base, team, draft) = scenario();
    let (plan, evidence) = run(default::policy(), &BTreeMap::new());
    assert_eq!(plan.conflicts.len(), 1);
    assert_eq!(plan.conflicts[0].key, "p:note");
    let content = decision(&evidence, "content");
    assert_eq!(content.outcome, DecisionOutcome::Merged);
    assert_eq!(content.rule.as_deref(), Some("legacy-content"));
    assert_eq!(
        content.strategy.as_deref(),
        Some("dynodoc.default.richtext.v1")
    );
    assert_eq!(content.basis.as_deref(), Some("richtext-hunks"));
    assert!(!content.needs_review);
    let note = decision(&evidence, "note");
    assert_eq!(note.outcome, DecisionOutcome::Conflict);
    assert_eq!(note.reason, "both-changed");
    assert_eq!(note.conflict_key.as_deref(), Some("p:note"));
    // The fields the sides did not both change are not decisions: `label` is
    // derived, so the merge does not look at it.
    assert_eq!(evidence.decisions.len(), 2);
    assert_eq!(evidence.policy.id, "dynodoc.default.v1");
    assert_eq!(evidence.policy.hash, default::policy().hash_hex());
    assert!(evidence.invariants.introduced.is_empty());
    // Same as the merge without a policy.
    let plain = merge(&base, &team, &draft, &BTreeMap::new());
    assert_eq!(plain.conflicts.len(), 1);
    assert_eq!(plain.state, plan.state);
    // An explicit resolution settles the note and is recorded as such.
    let choice = BTreeMap::from([("p:note".to_string(), "draft".to_string())]);
    let (settled, evidence) = run(default::policy(), &choice);
    assert!(settled.conflicts.is_empty());
    let note = decision(&evidence, "note");
    assert_eq!(note.outcome, DecisionOutcome::Explicit);
    assert_eq!(note.choice.as_deref(), Some("draft"));
    assert_eq!(note.conflict_key, None);
}

#[test]
fn a_manual_gate_turns_a_merge_into_a_conflict_a_person_can_settle() {
    let p = policy(&rules("manual"), "{}");
    let (plan, evidence) = run(&p, &BTreeMap::new());
    let keys: Vec<_> = plan.conflicts.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(keys, ["p:content", "p:note"]);
    let content = decision(&evidence, "content");
    assert_eq!(content.outcome, DecisionOutcome::Conflict);
    assert_eq!(content.reason, "manual-gate");
    assert_eq!(
        content.strategy.as_deref(),
        Some("dynodoc.default.richtext.v1")
    );
    assert!(content.notes[0].contains("requires a human decision"));
    // The team's value stays until someone chooses.
    assert_eq!(
        plan.state.nodes[&NodeId("p".into())].current_fields["content"],
        para(&SENTENCE.replace("participants", "respondents"))
    );
    let choice = BTreeMap::from([("p:content".to_string(), "draft".to_string())]);
    let (settled, evidence) = run(&p, &choice);
    assert_eq!(settled.conflicts.len(), 1);
    assert_eq!(
        decision(&evidence, "content").outcome,
        DecisionOutcome::Explicit
    );
    assert_eq!(
        settled.state.nodes[&NodeId("p".into())].current_fields["content"],
        para(&SENTENCE.replace("June", "July"))
    );
}

#[test]
fn a_blocked_gate_never_runs_the_strategy() {
    let (plan, evidence) = run(&policy(&rules("blocked"), "{}"), &BTreeMap::new());
    assert_eq!(plan.conflicts.len(), 2);
    let content = decision(&evidence, "content");
    assert_eq!(content.reason, "blocked");
    assert_eq!(content.strategy, None);
    assert_eq!(content.basis, None);
}

#[test]
fn a_review_gate_applies_the_merge_and_marks_it_for_review() {
    let (default_plan, _) = run(default::policy(), &BTreeMap::new());
    let (plan, evidence) = run(&policy(&rules("auto-with-review"), "{}"), &BTreeMap::new());
    assert_eq!(plan.state, default_plan.state);
    assert_eq!(plan.conflicts.len(), default_plan.conflicts.len());
    let content = decision(&evidence, "content");
    assert_eq!(content.outcome, DecisionOutcome::Merged);
    assert!(content.needs_review);
    assert!(!decision(&evidence, "note").needs_review);
}

#[test]
fn a_difference_no_rule_covers_is_a_conflict_with_its_own_reason() {
    for (unknown, reason) in [("manual", "no-rule"), ("blocked", "no-rule-blocked")] {
        let p = policy(
            &CONTENT.replace("GATE", "auto"),
            &format!(r#"{{"unknown":"{unknown}"}}"#),
        );
        let (plan, evidence) = run(&p, &BTreeMap::new());
        assert_eq!(plan.conflicts.len(), 1);
        let note = decision(&evidence, "note");
        assert_eq!(
            (
                note.reason.as_str(),
                note.rule.clone(),
                note.strategy.clone()
            ),
            (reason, None, None)
        );
        assert_eq!(
            decision(&evidence, "content").outcome,
            DecisionOutcome::Merged
        );
    }
}

struct Named(&'static str, fn() -> Outcome);
impl Strategy for Named {
    fn id(&self) -> &'static str {
        self.0
    }
    fn resolve(&self, _: &Inputs<'_>, _: &Context<'_>) -> Resolved {
        Resolved {
            outcome: (self.1)(),
            used: Default::default(),
        }
    }
}

fn custom(id: &'static str, outcome: fn() -> Outcome) -> (MergePolicy, Registry) {
    let mut registry = Registry::builtin();
    registry.register(Box::new(Named(id, outcome))).unwrap();
    let text = format!(
        r#"{{"version":1,"id":"t","rules":[{{"id":"all","match":{{}},"strategy":"{id}","gate":"auto"}}]}}"#
    );
    (
        MergePolicy::from_json(text.as_bytes(), &registry).unwrap(),
        registry,
    )
}

#[test]
fn a_panicking_strategy_becomes_a_conflict_and_a_preference_is_not_applied() {
    let (base, team, draft) = scenario();
    let (p, registry) = custom("test.panics.v1", || panic!("a strategy bug"));
    let (plan, evidence) =
        merge_with_registry(&base, &team, &draft, &BTreeMap::new(), &p, &registry);
    let keys: Vec<_> = plan.conflicts.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(keys, ["p:content", "p:note"]);
    assert!(evidence
        .decisions
        .iter()
        .all(|d| d.reason == "internal-error"));

    let (p, registry) = custom("test.prefers.v1", || Outcome::Prefer(Side::Draft));
    let (plan, evidence) =
        merge_with_registry(&base, &team, &draft, &BTreeMap::new(), &p, &registry);
    assert_eq!(plan.conflicts.len(), 2);
    assert!(evidence
        .decisions
        .iter()
        .all(|d| d.reason == "prefer-unsupported"));
    // Nothing of the draft was applied.
    assert_eq!(plan.state, team);

    // A merged value from a strategy that returned one is used.
    let (p, registry) = custom("test.merges.v1", || Outcome::Merged(json!("x")));
    let (plan, _) = merge_with_registry(&base, &team, &draft, &BTreeMap::new(), &p, &registry);
    assert!(plan.conflicts.is_empty());
    assert_eq!(
        plan.state.nodes[&NodeId("p".into())].current_fields["note"],
        "x"
    );
    // A conflict reason is carried into the evidence.
    let (p, registry) = custom("test.declines.v1", || {
        Outcome::Conflict(Reason("custom-reason"))
    });
    let (_, evidence) = merge_with_registry(&base, &team, &draft, &BTreeMap::new(), &p, &registry);
    assert!(evidence
        .decisions
        .iter()
        .all(|d| d.reason == "custom-reason"));
}

#[test]
fn a_policy_naming_a_strategy_the_registry_lacks_fails_closed() {
    let (p, _) = custom("test.gone.v1", || Outcome::Merged(json!(null)));
    let (base, team, draft) = scenario();
    let (plan, evidence) = merge_with_policy(&base, &team, &draft, &BTreeMap::new(), &p);
    assert_eq!(plan.conflicts.len(), 2);
    assert!(evidence
        .decisions
        .iter()
        .all(|d| d.reason == "unknown-strategy" && d.strategy.is_none()));
}

/// A merge that makes a field combination neither side had: one side stops the
/// section being a worksheet, the other protects it.
fn protected_elsewhere() -> (DocumentState, DocumentState, DocumentState) {
    let base = state(vec![
        node("root", "form", None, json!({})),
        node("s", "section", Some("root"), json!({"kind": "sheet"})),
    ]);
    let edit = |field: &str, value: Value| {
        let mut s = base.clone();
        s.nodes.get_mut(&NodeId("s".into())).unwrap().current_fields[field] = value;
        s
    };
    (
        base.clone(),
        edit("kind", json!("notes")),
        edit("sheet_protected", json!(true)),
    )
}

#[test]
fn an_invariant_the_merge_breaks_is_flagged_and_only_a_stricter_policy_makes_it_a_conflict() {
    let (base, team, draft) = protected_elsewhere();
    let (plain, evidence) =
        merge_with_policy(&base, &team, &draft, &BTreeMap::new(), default::policy());
    // Flag mode: the plan is the one `merge` gives, the evidence names the problem.
    assert!(plain.conflicts.is_empty());
    assert_eq!(
        plain.state,
        merge(&base, &team, &draft, &BTreeMap::new()).state
    );
    let flagged: Vec<_> = evidence
        .invariants
        .introduced
        .iter()
        .map(|v| (v.invariant, v.subject.as_str()))
        .collect();
    assert_eq!(flagged, [("INV-G8", "s")]);
    assert_eq!(evidence.invariants.preexisting, 0);
    assert!(evidence.invariants.conflicts.is_empty());

    let strict = policy(REST, r#"{"invariants":"conflict"}"#);
    let (plan, evidence) = merge_with_policy(&base, &team, &draft, &BTreeMap::new(), &strict);
    assert_eq!(plan.conflicts.len(), 1);
    assert_eq!(plan.conflicts[0].key, "s:$invariant:INV-G8");
    assert_eq!(plan.conflicts[0].field, "$invariant");
    assert_eq!(evidence.invariants.conflicts, ["s:$invariant:INV-G8"]);
    // Never silently repaired: the merged state is the same either way.
    assert_eq!(plan.state, plain.state);
    assert_eq!(plan.ops, plain.ops);
    // A resolution key cannot clear it: only changing the inputs can.
    let all = BTreeMap::from([("s:$invariant:INV-G8".to_string(), "team".to_string())]);
    let (again, _) = merge_with_policy(&base, &team, &draft, &all, &strict);
    assert_eq!(again.conflicts.len(), 1);
}

#[test]
fn a_violation_an_input_already_had_is_counted_not_blamed() {
    let (base, team, mut draft) = protected_elsewhere();
    // The draft already breaks the invariant by itself.
    draft
        .nodes
        .get_mut(&NodeId("s".into()))
        .unwrap()
        .current_fields["kind"] = json!("notes");
    let strict = policy(REST, r#"{"invariants":"conflict"}"#);
    let (plan, evidence) = merge_with_policy(&base, &team, &draft, &BTreeMap::new(), &strict);
    assert!(plan.conflicts.is_empty());
    assert!(evidence.invariants.introduced.is_empty());
    assert_eq!(evidence.invariants.preexisting, 1);
}

#[test]
fn tree_conflicts_are_named_by_the_invariant_they_enforce() {
    // Both sides move different blocks so that together they close a parent loop.
    let base = state(vec![
        node("root", "form", None, json!({})),
        node("a", "section", Some("root"), json!({})),
        node("b", "section", Some("root"), json!({})),
    ]);
    let moved = |node: &str, parent: &str| {
        let mut s = base.clone();
        s.nodes.get_mut(&NodeId(node.into())).unwrap().parent_id = Some(NodeId(parent.into()));
        s
    };
    let (plan, evidence) = merge_with_policy(
        &base,
        &moved("a", "b"),
        &moved("b", "a"),
        &BTreeMap::new(),
        default::policy(),
    );
    assert_eq!(plan.conflicts.len(), 1);
    assert_eq!(evidence.structural.len(), 1);
    assert_eq!(evidence.structural[0].invariant, "INV-G1");
    assert_eq!(evidence.structural[0].key, plan.conflicts[0].key);
    // The repair left no cycle, so nothing is introduced on top.
    assert!(evidence.invariants.introduced.is_empty());
}

#[test]
fn a_stricter_gate_never_adds_an_automatic_outcome() {
    // Policy monotonicity (design P11) over the gate order, on a spread of merges.
    let weakest = default::policy();
    for gate in ["auto-with-review", "manual", "blocked"] {
        let stricter = policy(&rules(gate), "{}");
        for words in 0..6 {
            let (base, mut team, mut draft) = scenario();
            let edit = |s: &mut DocumentState, text: String| {
                s.nodes.get_mut(&NodeId("p".into())).unwrap().current_fields = fields(&text, "n");
            };
            let sentence: Vec<&str> = SENTENCE.split(' ').collect();
            let swap = |i: usize, w: &str| {
                let mut v = sentence.clone();
                v[i] = w;
                v.join(" ")
            };
            edit(&mut team, swap(words, "alpha"));
            edit(&mut draft, swap((words + 3) % 6, "beta"));
            let (_, loose) = merge_with_policy(&base, &team, &draft, &BTreeMap::new(), weakest);
            let (_, strict) = merge_with_policy(&base, &team, &draft, &BTreeMap::new(), &stricter);
            let automatic = |e: &engine_core::merge_policy::MergeEvidence| -> Vec<String> {
                e.decisions
                    .iter()
                    .filter(|d| d.outcome == DecisionOutcome::Merged && !d.needs_review)
                    .map(|d| d.field.clone())
                    .collect()
            };
            let (loose, strict) = (automatic(&loose), automatic(&strict));
            assert!(strict.iter().all(|f| loose.contains(f)), "{gate} {words}");
            if gate != "auto-with-review" {
                assert!(strict.len() <= loose.len());
            }
        }
    }
}

#[test]
fn the_text_rendering_is_stable() {
    let (_, evidence) = run(default::policy(), &BTreeMap::new());
    let text = evidence.render_text(Labels {
        team: "left",
        draft: "right",
    });
    // One line per input: the preview, the size and the start of the digest.
    let input = |name: &str, value: Value| {
        let d = Digest::of(&value);
        format!(
            "    {name:<5} {} ({} B, sha256:{})",
            d.preview,
            d.bytes,
            &d.sha256[..16]
        )
    };
    let team_text = SENTENCE.replace("participants", "respondents");
    let draft_text = SENTENCE.replace("June", "July");
    let expected = [
        format!(
            "policy dynodoc.default.v1 sha256:{}  family Document",
            &default::policy().hash_hex()[..16]
        ),
        "decisions: 2 (1 merged, 0 explicit, 1 conflict, 0 whole-collection); settled without a strategy: 2 unchanged or left-only, 0 right-only".into(),
        "- p.content: merged (merged)".into(),
        "    rule legacy-content  strategy dynodoc.default.richtext.v1  gate auto".into(),
        "    basis richtext-hunks".into(),
        input("base", para(SENTENCE)),
        input("left", para(&team_text)),
        input("right", para(&draft_text)),
        "- p.note: conflict (both-changed)".into(),
        "    rule legacy-whole-value  strategy dynodoc.default.whole-value.v1  gate auto".into(),
        "    basis whole-value".into(),
        input("base", json!("n")),
        input("left", json!("team note")),
        input("right", json!("draft note")),
        "invariants (flag): 0 introduced, 0 already in an input".into(),
    ]
    .join("\n")
        + "\n";
    assert_eq!(text, expected);
    // And the JSON rendering carries the same facts under stable names.
    let json = evidence.to_json();
    assert_eq!(json["schema"], "dynodoc.merge-evidence.v1");
    assert_eq!(json["policy"]["id"], "dynodoc.default.v1");
    assert_eq!(
        json["decisions"][0]["strategy"],
        "dynodoc.default.richtext.v1"
    );
    assert_eq!(json["decisions"][1]["conflict_key"], "p:note");
    assert_eq!(json["decisions"][1]["team"]["bytes"], 11);
    assert_eq!(json["shortcuts"]["unchanged_or_team_only"], 2);
    assert_eq!(json["invariants"]["mode"], "flag");
}
