use engine_core::{
    materializer::{apply_payload, DocumentState},
    richtext, workspace_merge,
};
use engine_shared::{
    richtext::{RichTextEdit, TextOp},
    EventPayload, NodeId, NodeType,
};
use proptest::prelude::*;
use serde_json::{json, Value};

fn p(text: &str) -> Value {
    json!({"type":"paragraph","attrs":{"blockId":"stable"},"content":[{"type":"text","text":text}]})
}
fn styled(text: &str, marks: Value) -> Value {
    json!({"type":"paragraph","attrs":{"blockId":"stable"},"content":[{"type":"text","text":text,"marks":marks}]})
}
fn state(content: Value) -> DocumentState {
    let mut state = DocumentState::default();
    apply_payload(
        &mut state,
        &EventPayload::NodeCreated {
            node_id: NodeId("stable".into()),
            node_type: NodeType::Item,
            parent_id: None,
            pos: "a".into(),
            fields: json!({"label":richtext::plain_text(&content),"content":content}),
            var_name: None,
        },
    )
    .unwrap();
    state
}

#[test]
fn tiny_format_delta_for_a_long_passage_and_exact_replay() {
    let before = p(&"Research findings. ".repeat(1500));
    let after = styled(
        &richtext::plain_text(&before),
        json!([{"type":"bold"},{"type":"textStyle","attrs":{"fontFamily":"Noto Sans","color":"#005bbb","custom":7}}]),
    );
    let patch = richtext::make_patch(&before, &after).unwrap();
    assert!(serde_json::to_vec(&patch).unwrap().len() < 700);
    assert_eq!(richtext::apply_patch(&before, &patch).unwrap(), after);
    assert_eq!(patch.styles.len(), 1);
    let old = state(before);
    let op = richtext::compact_event(
        &old,
        &EventPayload::FieldEdited {
            node_id: NodeId("stable".into()),
            field: "content".into(),
            value: after.clone(),
        },
    );
    assert!(matches!(op, EventPayload::RichTextPatched { .. }));
    let mut replay = old;
    apply_payload(&mut replay, &op).unwrap();
    assert_eq!(
        replay.nodes[&NodeId("stable".into())].current_fields["content"],
        after
    );
}

#[test]
fn unicode_and_nested_tables_roundtrip_without_utf16_offsets() {
    let before = json!({"type":"table","content":[{"type":"tableRow","content":[{"type":"tableCell","attrs":{"colspan":1},"content":[p("বাংলা 👩🏽‍🔬 e\u{301} العربية")]}]}]});
    let mut after = before.clone();
    after["content"][0]["content"][0]["content"][0]["content"][0]["text"] =
        json!("বাংলা 👩🏽‍🔬 revised e\u{301} العربية");
    let patch = richtext::make_patch(&before, &after).unwrap();
    assert_eq!(richtext::apply_patch(&before, &patch).unwrap(), after);
}

#[test]
fn rejects_stale_base_forged_result_bad_style_and_unconsumed_text() {
    let before = p("hello");
    let after = p("hello!");
    let patch = richtext::make_patch(&before, &after).unwrap();
    assert!(richtext::apply_patch(&p("other"), &patch).is_err());
    let mut bad = patch.clone();
    bad.after = "forged".into();
    assert!(richtext::apply_patch(&before, &bad).is_err());
    let mut bad = patch.clone();
    bad.edits = vec![RichTextEdit::Text {
        path: vec![],
        ops: vec![TextOp::Retain {
            count: usize::MAX,
            style: None,
        }],
    }];
    assert!(richtext::apply_patch(&before, &bad).is_err());
    let mut bad = patch.clone();
    bad.edits = vec![RichTextEdit::Text {
        path: vec![],
        ops: vec![TextOp::Retain {
            count: 5,
            style: Some(999),
        }],
    }];
    assert!(richtext::apply_patch(&before, &bad).is_err());
    let mut bad = patch;
    bad.edits = vec![RichTextEdit::Text {
        path: vec![],
        ops: vec![],
    }];
    assert!(richtext::apply_patch(&before, &bad).is_err());
}

#[test]
fn disjoint_same_paragraph_edits_merge_without_derived_label_conflicts() {
    let base = state(p("The participants were interviewed in June."));
    let team = state(p("The respondents were interviewed in June."));
    let draft = state(p("The participants were interviewed in July."));
    let plan = workspace_merge::merge(&base, &team, &draft, &Default::default());
    assert!(plan.conflicts.is_empty(), "{:?}", plan.conflicts);
    assert_eq!(
        plan.state.nodes[&NodeId("stable".into())].current_fields["label"],
        "The respondents were interviewed in July."
    );
    let mut replay = team;
    for op in plan.ops {
        apply_payload(&mut replay, &op).unwrap();
    }
    assert_eq!(replay, plan.state);
}

#[test]
fn multiple_noncontiguous_edits_can_merge() {
    let b = p("alpha one beta two gamma three omega");
    let t = p("ALPHA one beta two gamma three OMEGA");
    let d = p("alpha one BETA two gamma three omega");
    assert_eq!(
        richtext::plain_text(&richtext::merge(&b, &t, &d).unwrap()),
        "ALPHA one BETA two gamma three OMEGA"
    );
}

#[test]
fn an_adjacent_hunk_cannot_hide_a_later_overlap() {
    assert!(richtext::merge(&p("abcdefghij"), &p("abUVWXYZij"), &p("ABcdeZghij")).is_none());
}

#[test]
fn merges_independent_marks_and_properties_but_not_competing_colors() {
    let base = p("Findings");
    let bold = styled("Findings", json!([{"type":"bold"}]));
    let italic = styled("Findings", json!([{"type":"italic"}]));
    let merged = richtext::merge(&base, &bold, &italic).unwrap();
    assert_eq!(merged["content"][0]["marks"].as_array().unwrap().len(), 2);
    let font = styled(
        "Findings",
        json!([{"type":"textStyle","attrs":{"fontFamily":"Arial"}}]),
    );
    let blue = styled(
        "Findings",
        json!([{"type":"textStyle","attrs":{"color":"blue"}}]),
    );
    let red = styled(
        "Findings",
        json!([{"type":"textStyle","attrs":{"color":"red"}}]),
    );
    let merged = richtext::merge(&base, &font, &blue).unwrap();
    assert_eq!(
        merged["content"][0]["marks"][0]["attrs"],
        json!({"fontFamily":"Arial","color":"blue"})
    );
    assert!(richtext::merge(&base, &red, &blue).is_none());
}

#[test]
fn conflicts_for_competing_insertions_delete_format_and_structural_edits() {
    assert!(richtext::merge(&p("ab"), &p("axb"), &p("ayb")).is_none());
    assert!(richtext::merge(
        &p("abc"),
        &p("ac"),
        &styled("abc", json!([{"type":"bold"}]))
    )
    .is_none());
    let base = json!({"type":"bulletList","content":[p("one"),p("two")]});
    let mut team = base.clone();
    team["content"][0] = p("ONE");
    let mut draft = base.clone();
    draft["content"][1] = p("TWO");
    assert!(richtext::merge(&base, &team, &draft).is_none());
}

#[test]
fn classification_coalesces_only_adjacent_equal_runs() {
    let doc = json!({"type":"doc","content":[{"type":"paragraph","content":[{"type":"text","text":"a"},{"type":"text","text":"b"},{"type":"text","text":"c","marks":[{"type":"bold"}]},{"type":"text","text":"d"}]},p("e")]});
    let report = richtext::analyze(&doc);
    assert_eq!(
        (
            report.formatting_runs,
            report.distinct_styles,
            report.text_scalars
        ),
        (4, 2, 5)
    );
}

#[test]
fn restoration_preserves_a_legacy_label_that_differs_from_derived_text() {
    let original = state(p(&"original paragraph ".repeat(100)));
    let mut target = state(p(&"changed paragraph ".repeat(100)));
    let id = NodeId("stable".into());
    target.nodes.get_mut(&id).unwrap().current_fields["label"] =
        original.nodes[&id].current_fields["label"].clone();
    let mut replay = original.clone();
    for op in workspace_merge::diff(&original, &target) {
        apply_payload(&mut replay, &op).unwrap();
    }
    assert_eq!(replay, target);
}

proptest! {
    #[test]
    fn arbitrary_unicode_text_edits_roundtrip(a in ".{1,100}", b in ".{1,100}") {
        let before = p(&a); let after = p(&b);
        let patch = richtext::make_patch(&before,&after).unwrap();
        prop_assert_eq!(richtext::apply_patch(&before,&patch).unwrap(),after);
    }
}

#[test]
fn golden_browser_replay_fixture() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../frontend/researcher/src/lib/workspace/richtext-fixtures.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let patch = serde_json::from_value(case["patch"].clone()).unwrap();
        assert_eq!(
            richtext::apply_patch(&case["before"], &patch).unwrap(),
            case["after"]
        );
    }
}
