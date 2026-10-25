//! Engine-only replay/merge coverage; this is not native-editor round-trip or
//! grapheme-aware editing certification. Patch offsets remain Unicode scalars.
use engine_core::{
    materializer::{apply_payload, DocumentState},
    richtext, workspace_merge,
};
use engine_shared::{EventPayload, NodeId, NodeType};
use proptest::{prelude::*, test_runner::RngSeed};
use serde_json::{json, Value};

fn paragraph(text: &str, marks: Value) -> Value {
    json!({"type":"paragraph", "attrs":{"blockId":"stable"}, "content":[{"type":"text", "text":text, "marks":marks}]})
}

fn nested(text: &str, marks: Value) -> Value {
    json!({"type":"doc", "content":[
        {"type":"bulletList", "content":[{"type":"listItem", "content":[paragraph(text, marks.clone())]}]},
        {"type":"table", "content":[{"type":"tableRow", "content":[
            {"type":"tableCell", "attrs":{"colspan":1,"rowspan":1,"unknown":"preserve"}, "content":[paragraph(text, marks)]}
        ]}]}
    ]})
}

fn state(content: Value) -> DocumentState {
    let mut state = DocumentState::default();
    apply_payload(
        &mut state,
        &EventPayload::NodeCreated {
            node_id: NodeId("stable".into()),
            node_type: NodeType::Clause,
            parent_id: None,
            pos: "a0".into(),
            fields: json!({"label":richtext::plain_text(&content),"content":content}),
            var_name: None,
        },
    )
    .unwrap();
    state
}

fn replay(before: &Value, after: &Value) {
    let patch = richtext::make_patch(before, after).expect("supported text/mark edit");
    assert_eq!(richtext::apply_patch(before, &patch).unwrap(), *after);
    let mut folded = state(before.clone());
    apply_payload(
        &mut folded,
        &EventPayload::RichTextPatched {
            node_id: NodeId("stable".into()),
            patch,
        },
    )
    .unwrap();
    assert_eq!(folded, state(after.clone()));
}

#[test]
fn bengali_conjuncts_normalization_forms_and_joiners_replay_exactly() {
    let cases: Value =
        serde_json::from_str(include_str!("fixtures/richtext-bengali.json")).unwrap();
    for case in cases.as_array().unwrap() {
        let before = case["before"].as_str().unwrap();
        let after = case["after"].as_str().unwrap();
        assert_ne!(before.as_bytes(), after.as_bytes(), "{}", case["name"]);
        replay(&paragraph(before, json!([])), &paragraph(after, json!([])));
        replay(
            &nested(before, json!([])),
            &nested(after, json!([{"type":"bold"}])),
        );
    }
}

#[test]
fn conjunct_edit_merges_with_separate_word_change_and_replays() {
    let base = state(paragraph("ক্ষ পরে লেখা", json!([])));
    let team = state(paragraph("ক্খ পরে লেখা", json!([])));
    let draft = state(paragraph("ক্ষ শেষে লেখা", json!([])));
    let plan = workspace_merge::merge(&base, &team, &draft, &Default::default());
    assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
    assert_eq!(plan.state, state(paragraph("ক্খ শেষে লেখা", json!([]))));
    let mut applied = team;
    for op in plan.ops {
        apply_payload(&mut applied, &op).unwrap();
    }
    assert_eq!(applied, plan.state);
}

#[test]
fn competing_consonant_edits_inside_same_conjunct_conflict() {
    let base = paragraph("ক্ষ", json!([]));
    let team = paragraph("ক্খ", json!([]));
    let draft = paragraph("ক্গ", json!([]));
    assert!(richtext::merge(&base, &team, &draft).is_none());
}

#[test]
fn nested_concurrent_edits_require_resolution_but_selected_result_replays_exactly() {
    let before = nested("ক্ষ গবেষণা", json!([]));
    let team = nested("ক্ষ গবেষণা", json!([{"type":"bold"}]));
    let draft = nested("ক্ষ প্রতিবেদন", json!([]));
    let expected = nested("ক্ষ প্রতিবেদন", json!([{"type":"bold"}]));
    // Nested stable identity/alignment is not implemented. Refuse to infer which
    // table/list child received an edit; an explicitly selected result still
    // has to preserve every supported attribute during patch replay.
    assert!(richtext::merge(&before, &team, &draft).is_none());
    replay(&team, &expected);
    replay(&draft, &expected);
}

fn tokens() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop::sample::select(vec![
            "ক্ষ",
            "য়",
            "য়",
            "ৎ",
            "ক্\u{200d}ষ",
            "ক্\u{200c}ষ",
            "বাংলা",
            "👩🏽‍🔬",
            "e\u{301}",
        ]),
        1..12,
    )
    .prop_map(|parts| parts.join(" "))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, rng_seed: RngSeed::Fixed(0x42454e47414c49), .. ProptestConfig::default() })]
    #[test]
    fn formatted_nested_unicode_edits_roundtrip(a in tokens(), b in tokens(), bold in any::<bool>(), italic in any::<bool>()) {
        let marks = |enabled, kind| if enabled { json!([{"type":kind}]) } else { json!([]) };
        let before = nested(&a, marks(bold, "bold"));
        let after = nested(&b, marks(italic, "italic"));
        let patch = richtext::make_patch(&before, &after).unwrap();
        prop_assert_eq!(richtext::apply_patch(&before, &patch).unwrap(), after.clone());
        replay(&before, &after);
    }
}
