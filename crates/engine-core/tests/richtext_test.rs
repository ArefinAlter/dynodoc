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

#[test]
fn image_attr_edit_event_size() {
    let src = format!("data:image/png;base64,{}", "A".repeat(375_000));
    let mut content = json!({"type":"image","attrs":{"src":src,"alt":"Original","width":100}});
    let mut current = state(content.clone());
    for (attribute, value) in [
        ("alt", json!("Revised alternative text")),
        ("width", json!(640)),
    ] {
        content["attrs"][attribute] = value;
        let edit = EventPayload::FieldEdited {
            node_id: NodeId("stable".into()),
            field: "content".into(),
            value: content.clone(),
        };
        let compacted = richtext::compact_event(&current, &edit);
        let bytes = serde_json::to_vec(&compacted).unwrap().len();
        eprintln!("image {attribute} edit: {bytes} serialized payload bytes");
        // V1 replaces the attribute object, including the embedded source. A future
        // attribute-only patch format must deliberately revise this known limit.
        assert!(matches!(compacted, EventPayload::FieldEdited { .. }));
        assert!(
            bytes > 375_000,
            "the current event still contains the image: {bytes}"
        );
        assert!(bytes < 376_024, "unexpected overhead: {bytes}");
        apply_payload(&mut current, &compacted).unwrap();
        assert_eq!(
            current.nodes[&NodeId("stable".into())].current_fields["content"],
            content
        );
        assert_eq!(content["attrs"]["src"], src);
    }
}

#[test]
fn image_attributes_merge_key_by_key() {
    let img = json!({"type":"image","attrs":{"src":"a.png","width":100,"alt":""}});
    let mut width = img.clone();
    width["attrs"]["width"] = json!(200);
    let mut alt = img.clone();
    alt["attrs"]["alt"] = json!("A chart");
    assert_eq!(
        richtext::merge(&img, &width, &alt).unwrap()["attrs"],
        json!({"src":"a.png","width":200,"alt":"A chart"})
    );
    let mut narrow = img.clone();
    narrow["attrs"]["width"] = json!(50);
    assert!(richtext::merge(&img, &width, &narrow).is_none());
}

#[test]
fn edits_around_an_inline_atom_merge() {
    let para = |c: Value| json!({"type":"paragraph","content":c});
    let t = |s: &str| json!({"type":"text","text":s});
    for atom in [
        json!({"type":"hardBreak"}),
        json!({"type":"inlineMath","attrs":{"latex":"x^2"}}),
        json!({"type":"footnoteReference","attrs":{"id":"1"}}),
    ] {
        let base = para(json!([t("first line"), atom.clone(), t("second line")]));
        let team = para(json!([t("FIRST line"), atom.clone(), t("second line")]));
        let draft = para(json!([t("first line"), atom.clone(), t("second LINE")]));
        assert_eq!(
            richtext::merge(&base, &team, &draft),
            Some(para(json!([
                t("FIRST line"),
                atom.clone(),
                t("second LINE")
            ]))),
            "{atom}"
        );
    }
}

#[test]
fn same_atom_edited_differently_conflicts() {
    let para = |c: Value| json!({"type":"paragraph","content":c});
    let t = |s: &str| json!({"type":"text","text":s});
    let math = |latex: &str| json!({"type":"inlineMath","attrs":{"latex":latex}});
    let base = para(json!([t("alpha "), math("x^2"), t(" omega")]));
    let squared = para(json!([t("alpha "), math("x^3"), t(" omega")]));
    let cubed = para(json!([t("alpha "), math("x^4"), t(" omega")]));
    assert!(richtext::merge(&base, &squared, &cubed).is_none());
    // One side's change to the atom still combines with the other side's wording.
    let reworded = para(json!([t("alpha "), math("x^2"), t(" OMEGA")]));
    assert_eq!(
        richtext::merge(&base, &squared, &reworded),
        Some(para(json!([t("alpha "), math("x^3"), t(" OMEGA")])))
    );
}

#[test]
fn adjacent_or_reordered_atoms_are_not_merged_by_position() {
    let para = |c: Value| json!({"type":"paragraph","content":c});
    let img = |s: &str, w: u32| json!({"type":"image","attrs":{"src":s,"width":w}});
    let base = para(json!([img("a.png", 100), img("b.png", 100)]));
    let resized = para(json!([img("a.png", 200), img("b.png", 100)]));
    // One side deletes the first image while the other resizes it.
    let deleted = para(json!([img("b.png", 100)]));
    assert!(richtext::merge(&base, &deleted, &resized).is_none());
    // One side swaps the images while the other resizes the first.
    let swapped = para(json!([img("b.png", 100), img("a.png", 100)]));
    assert!(richtext::merge(&base, &swapped, &resized).is_none());
}

#[test]
fn a_literal_object_replacement_character_is_not_an_atom() {
    let para = |c: Value| json!({"type":"paragraph","content":c});
    let t = |s: &str| json!({"type":"text","text":s});
    let base = para(json!([t("a\u{fffc}b"), json!({"type":"hardBreak"})]));
    let team = para(json!([t("A\u{fffc}b"), json!({"type":"hardBreak"})]));
    let draft = para(json!([t("a\u{fffc}B"), json!({"type":"hardBreak"})]));
    assert!(richtext::merge(&base, &team, &draft).is_none());
}

#[test]
fn paragraph_layout_merges_with_spacing_and_wording_but_competing_indents_conflict() {
    let base = p("alpha beta");
    let mut indent = base.clone();
    indent["attrs"]["indent"] = json!(2);
    let mut spacing = base.clone();
    spacing["attrs"]["spaceBefore"] = json!(12);
    let mut expected = indent.clone();
    expected["attrs"]["spaceBefore"] = json!(12);
    for (left, right) in [(&indent, &spacing), (&spacing, &indent)] {
        assert_eq!(richtext::merge(&base, left, right), Some(expected.clone()));
    }
    let wording = p("alpha revised");
    let mut expected = wording.clone();
    expected["attrs"]["indent"] = json!(2);
    for (left, right) in [(&indent, &wording), (&wording, &indent)] {
        assert_eq!(richtext::merge(&base, left, right), Some(expected.clone()));
    }
    let mut other_indent = base.clone();
    other_indent["attrs"]["indent"] = json!(4);
    assert!(richtext::merge(&base, &indent, &other_indent).is_none());
    assert!(richtext::merge(&base, &other_indent, &indent).is_none());
}

#[test]
fn underline_and_highlight_merge_with_independent_marks() {
    for (left, right) in [
        (json!({"type":"bold"}), json!({"type":"underline"})),
        (
            json!({"type":"highlight","attrs":{"color":"#ffff00"}}),
            json!({"type":"italic"}),
        ),
    ] {
        let base = p("Findings");
        let a = styled("Findings", json!([left]));
        let b = styled("Findings", json!([right]));
        for (a, b) in [(&a, &b), (&b, &a)] {
            let merged = richtext::merge(&base, a, b).unwrap();
            let marks = merged["content"][0]["marks"].as_array().unwrap();
            assert_eq!(marks.len(), 2);
            assert!(marks.contains(&left) && marks.contains(&right));
            assert_eq!(richtext::plain_text(&merged), "Findings");
        }
    }
}

#[test]
fn bold_and_link_changes_preserve_a_reword_elsewhere() {
    let base = p("alpha beta");
    for mark in [
        json!({"type":"bold"}),
        json!({"type":"link","attrs":{"href":"https://example.test/"}}),
    ] {
        let mut marked = base.clone();
        marked["content"] = json!([
            {"type":"text","text":"alpha","marks":[mark]},
            {"type":"text","text":" beta"},
        ]);
        let wording = p("alpha revised");
        let mut expected = marked.clone();
        expected["content"][1]["text"] = json!(" revised");
        for (left, right) in [(&marked, &wording), (&wording, &marked)] {
            assert_eq!(richtext::merge(&base, left, right), Some(expected.clone()));
        }
    }
}

#[test]
fn same_word_replaced_differently_conflicts() {
    let base = p("alpha beta");
    let team = p("alpha team");
    let draft = p("alpha draft");
    assert!(richtext::merge(&base, &team, &draft).is_none());
    assert!(richtext::merge(&base, &draft, &team).is_none());
}

#[test]
fn concurrent_nested_table_cell_edits_require_explicit_resolution() {
    let cell = |text| json!({"type":"tableCell","content":[p(text)]});
    let base = json!({"type":"table","content":[{"type":"tableRow","content":[cell("left"),cell("right")]}]});
    let mut team = base.clone();
    team["content"][0]["content"][0] = cell("team");
    let mut draft = base.clone();
    draft["content"][0]["content"][1] = cell("draft");
    // Nested cells have no stable merge identity in the current patch contract.
    assert!(richtext::merge(&base, &team, &draft).is_none());
    assert!(richtext::merge(&base, &draft, &team).is_none());
}
