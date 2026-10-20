//! Hostile input to the operation compiler (AI-AGENT-SECURITY P1 to P3, AG1; protocol spec
//! 8.1, 10, 11): huge counts, deep or malformed refs, control and hidden characters, the
//! injection corpus, and what an op must never be able to do. Nothing here may panic, every
//! input is rejected with the spec's code or accepted and stored as plain data, and text the
//! agent writes can never forge structure when a later `read` shows it.

use std::collections::BTreeSet;
use std::time::Instant;

use engine_agent::compile::{compile, CompileInput, Compiled, IdSource};
use engine_agent::dct::split_response;
use engine_agent::diff::Detail;
use engine_agent::error::{AgentError, CompileError};
use engine_agent::escape::escape;
use engine_agent::fixtures::{self, Fixture};
use engine_agent::grammar::parse_ops;
use engine_agent::model::FileKind;
use engine_agent::outline::{outline, OutlineOptions};
use engine_agent::parse;
use engine_agent::preview::{preview, PreviewOptions};
use engine_agent::read::{read, ReadOptions};
use engine_agent::refs::RefIndex;
use engine_core::governance::Role;
use engine_core::materializer::DocumentState;
use engine_shared::{EventPayload, NodeId, NodeType};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use serde_json::{json, Value};

fn ids() -> impl IdSource {
    let mut n = 0u64;
    move || {
        n += 1;
        fixtures::ulid(5_000_000 + n)
    }
}

fn run(fx: &Fixture, ops: Value) -> Result<Compiled, CompileError> {
    let ops = parse_ops(&ops)?;
    compile(
        &CompileInput::new(&fx.meta, &fx.state, Role::Author),
        &ops,
        &mut ids(),
    )
}

fn rf(fx: &Fixture, id: &NodeId) -> String {
    RefIndex::from_state(&fx.state)
        .ref_of(id)
        .unwrap()
        .to_string()
}

fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(100);
    let mut config = ProptestConfig::with_cases(cases);
    if std::env::var_os("PROPTEST_RNG_SEED").is_none() {
        config.rng_seed = RngSeed::Fixed(0x636f_6d70_696c);
    }
    config.failure_persistence = None;
    config
}

/// The text of a block as `read` shows it, rebuilt from the JSON: text nodes joined, a
/// `hardBreak` as a line feed.
fn block_text(content: &Value) -> String {
    content["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .map(|n| match n["type"].as_str() {
            Some("hardBreak") => "\n".to_string(),
            _ => n["text"].as_str().unwrap_or("").to_string(),
        })
        .collect()
}

fn normalise(s: &str) -> String {
    s.replace("\r\n", "\n")
        .replace(['\r', '\u{85}', '\u{2028}', '\u{2029}'], "\n")
}

fn rejected(s: &str) -> bool {
    let (_, d) = escape(&normalise(s));
    d.ctrl > 0 || d.hidden > 0
}

/// Every tool's view of `after` parses back to the structure that was written, and the
/// preview of the call does too.
fn assert_reads_parse(before: &Fixture, c: &Compiled) {
    let meta = &before.meta;
    let idx = RefIndex::from_state(&c.after);
    let refs: Vec<String> = c
        .after
        .nodes
        .values()
        .filter(|n| !n.deleted && n.current_fields["content"].is_object())
        .map(|n| idx.ref_of(&n.id).unwrap().to_string())
        .collect();
    assert!(refs.len() <= 100, "the fixture fits one read");
    let r = read(
        meta,
        &c.after,
        &ReadOptions {
            refs: refs.clone(),
            max_bytes: Some(64_000),
            ..Default::default()
        },
    )
    .unwrap();
    let (_, lines) = split_response(&r.text).unwrap();
    assert!(lines.iter().all(|l| !l.starts_with("dd/1")));
    let blocks = parse::read_blocks(&lines).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
    assert_eq!(blocks.len(), refs.len());
    let o = outline(
        meta,
        &c.after,
        &OutlineOptions {
            limit: Some(1000),
            max_bytes: Some(64_000),
            ..Default::default()
        },
    )
    .unwrap();
    let (_, lines) = split_response(&o.text).unwrap();
    assert_eq!(parse::outline_blocks(&lines, 1).unwrap().len(), refs.len());
    for detail in [Detail::Summary, Detail::Text] {
        let p = preview(
            meta,
            &before.state,
            c,
            &PreviewOptions {
                detail,
                max_bytes: Some(64_000),
                ..Default::default()
            },
        )
        .unwrap();
        let (h, lines) = split_response(&p.text).unwrap();
        assert_eq!(h.get("trust"), Some("data"));
        let entries = parse::diff_lines(&lines).unwrap_or_else(|e| panic!("{e}\n{}", p.text));
        assert_eq!(entries.len(), p.data.entries.len());
    }
}

// ---------------------------------------------------------------- hostile text

fn targets(d: &fixtures::DocumentFixture) -> (NodeId, NodeId) {
    (d.ids["purpose"].clone(), d.ids["long"].clone())
}

/// Matches of "policy" in the fixture document, as the compiler counts them.
fn policy_count(d: &fixtures::DocumentFixture) -> u64 {
    static COUNT: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *COUNT.get_or_init(|| {
        match run(
            &d.base,
            json!([{"op": "replace_all", "find": "policy", "with": "x", "expect": 0}]),
        ) {
            Err(CompileError {
                error: AgentError::MatchCount { found, .. },
                ..
            }) => found,
            other => panic!("{other:?}"),
        }
    })
}

/// Compile hostile text through every op that carries text. Returns how many of the four
/// compiles were accepted (all of them, unless the text is refused).
fn assert_stored_or_refused(d: &fixtures::DocumentFixture, s: &str) -> usize {
    let (p, anchor) = targets(d);
    let total = policy_count(d);
    let mut accepted = 0;
    let fields: Vec<(&str, Value)> = vec![
        (
            "text",
            json!([{"op": "set_text", "ref": rf(&d.base, &p), "text": s}]),
        ),
        (
            "with",
            json!([{"op": "replace", "ref": rf(&d.base, &p), "find": "policy", "with": s}]),
        ),
        (
            "text",
            json!([{"op": "insert", "after": rf(&d.base, &anchor), "blocks": [{"t": "p", "text": s}]}]),
        ),
        (
            "with",
            json!([{"op": "replace_all", "find": "policy", "with": s, "expect": total}]),
        ),
    ];
    for (field, ops) in fields {
        let res = run(&d.base, ops.clone());
        if rejected(s) {
            let e = res.unwrap_err();
            assert!(
                matches!(&e.error, AgentError::BadRequest { field: f } if *f == field),
                "{s:?} -> {e}"
            );
            continue;
        }
        let c = match res {
            Ok(c) => c,
            // Text equal to what is already there changes nothing; nothing else may fail.
            Err(e) => {
                assert_eq!(e.error, AgentError::NoChange, "{s:?} -> {e}");
                continue;
            }
        };
        accepted += 1;
        // Stored exactly (line breaks as hard breaks), in the block the op named.
        let want = normalise(s);
        let stored: Vec<String> = c
            .ops
            .iter()
            .filter_map(EventPayload::target_node_id)
            .filter(|id| c.after.nodes[*id].current_fields["content"].is_object())
            .map(|id| block_text(&c.after.nodes[id].current_fields["content"]))
            .collect();
        assert!(
            stored.iter().any(|t| t.contains(&want)),
            "{s:?} not stored in {stored:?}"
        );
        assert_reads_parse(&d.base, &c);
    }
    accepted
}

#[test]
fn the_injection_corpus_is_stored_as_data_or_refused() {
    let d = fixtures::document();
    let corpus = fixtures::injection_corpus();
    assert!(corpus.len() >= 25);
    let (mut storable, mut accepted) = (0, 0);
    for s in &corpus {
        if !rejected(s) {
            storable += 1;
        }
        accepted += assert_stored_or_refused(&d, s);
    }
    // Some strings are plain-looking injections that must be storable (they are data), and
    // some carry hidden characters that must not be. Every storable string went through all
    // four ops (a string equal to the current text of the block would be a no-change).
    assert!(
        storable >= 15 && storable < corpus.len(),
        "{storable} of {}",
        corpus.len()
    );
    assert!(
        accepted >= storable * 4 - 4,
        "{accepted} accepted for {storable} storable strings"
    );
}

fn fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        6 => any::<char>().prop_map(|c| c.to_string()),
        8 => proptest::sample::select(vec![
            "@", "#", "=", "-", " ", "\\", "\n", "\r\n", "\t", "\"", "⟦", "⟧", ".", "~", "+", ">", "[-",
            "-]", "{+", "+}", "\u{202e}", "\u{200b}", "\u{e0041}", "\u{1b}", "\u{2028}", "\u{0}",
            "dd/1 read ", "@7C02 P #b7a210", "# ref type hash words preview", ".A1B2 title", "r1\t",
            "⟦fn:X⟧", "é", "ক্ষ", "👨\u{200d}👩", "**", "`", "q=", "trust=data", "more=c_1.2",
        ]).prop_map(String::from),
    ]
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn arbitrary_hostile_text_is_stored_as_data_or_refused(
        parts in proptest::collection::vec(fragment(), 0..8),
    ) {
        let d = fixtures::document();
        let _ = assert_stored_or_refused(&d, &parts.concat());
    }
}

#[test]
fn hostile_text_in_cells_shapes_and_notes() {
    let w = fixtures::workbook();
    let k = fixtures::deck();
    let corpus = fixtures::injection_corpus();
    let (body, s2) = (&k.ids["s2_body"], &k.ids["s2"]);
    let mut accepted = 0;
    for s in &corpus {
        let want = normalise(s);
        let cases = [
            (
                &w.base,
                "values",
                json!([{"op": "set_cells", "sheet": "Budget", "at": "A2", "values": [[s]]}]),
            ),
            (
                &k.base,
                "text",
                json!([{"op": "set_shape", "slide": "s2", "shape": rf(&k.base, body), "text": s}]),
            ),
            (
                &k.base,
                "text",
                json!([{"op": "set_notes", "slide": rf(&k.base, s2), "text": s}]),
            ),
        ];
        for (fx, field, ops) in cases {
            match run(fx, ops) {
                Err(e) if rejected(s) => {
                    assert!(
                        matches!(&e.error, AgentError::BadRequest { field: f } if *f == field),
                        "{s:?}: {e}"
                    )
                }
                Err(e) => assert!(
                    matches!(
                        e.error,
                        AgentError::NoChange | AgentError::BadRequest { field: "values" }
                    ),
                    "{s:?}: {e}"
                ),
                Ok(c) => {
                    accepted += 1;
                    assert!(!rejected(s), "{s:?}");
                    let stored = c.ops.iter().any(|o| match o {
                        EventPayload::FieldEdited { value, .. } => value.as_str() == Some(&want),
                        _ => false,
                    });
                    assert!(stored, "{s:?} not stored: {:?}", c.ops);
                    // The preview of the call parses with the strict diff grammar.
                    let p = preview(
                        &fx.meta,
                        &fx.state,
                        &c,
                        &PreviewOptions {
                            detail: Detail::Text,
                            max_bytes: Some(64_000),
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    let (_, lines) = split_response(&p.text).unwrap();
                    parse::diff_lines(&lines).unwrap_or_else(|e| panic!("{e}\n{}", p.text));
                }
            }
        }
    }
    let storable = corpus.iter().filter(|s| !rejected(s)).count();
    assert!(
        accepted >= storable * 3 - 6,
        "{accepted} accepted, {storable} storable"
    );
}

// ---------------------------------------------------------------- huge and deep input

#[test]
fn counts_beyond_the_limits_are_refused_before_any_work() {
    let d = fixtures::document();
    let r = rf(&d.base, &d.ids["purpose"]);
    let op = json!({"op": "set_text", "ref": r, "text": "x"});
    for (n, ok) in [(200usize, true), (201, false), (5_000, false)] {
        let ops = Value::Array((0..n).map(|_| op.clone()).collect());
        let res = run(&d.base, ops);
        if ok {
            // 200 identical set_text ops: the second finds no change; the point is the count.
            assert!(!matches!(
                res,
                Err(CompileError {
                    error: AgentError::LimitExceeded { field: "ops", .. },
                    ..
                })
            ));
        } else {
            assert!(
                matches!(
                    res.unwrap_err().error,
                    AgentError::LimitExceeded {
                        field: "ops",
                        limit: 200,
                        ..
                    }
                ),
                "{n}"
            );
        }
    }
    // Huge but legal numbers must not overflow anything.
    for ops in [
        json!([{"op": "replace", "ref": r, "find": "policy", "with": "x", "expect": u64::MAX}]),
        json!([{"op": "replace", "ref": r, "find": "policy", "with": "x", "nth": u32::MAX as u64 + 1}]),
        json!([{"op": "replace", "ref": r, "find": "policy", "with": "x", "nth": 1_000_000}]),
        json!([{"op": "replace_all", "find": "policy", "with": "x", "expect": u64::MAX}]),
        json!([{"op": "set_cells", "sheet": "S", "at": "A1", "values": [["x"]], "nth": 1}]),
    ] {
        let e = run(&d.base, ops).unwrap_err();
        assert!(
            matches!(
                e.error,
                AgentError::BadRequest { .. } | AgentError::MatchCount { .. } | AgentError::NoMatch
            ),
            "{e}"
        );
    }
    // Not an array, not objects, wrong types everywhere.
    for ops in [
        json!({"op": "set_text"}),
        json!("set_text"),
        json!(null),
        json!([null]),
        json!([[]]),
        json!([1]),
        json!([{"op": ["set_text"]}]),
        json!([{"op": "set_text", "ref": {"a": 1}, "text": "x"}]),
        json!([{"op": "set_text", "ref": r, "text": 12}]),
        json!([{"op": "set_text", "ref": r, "text": "x", "base": 12}]),
        json!([{"op": "replace", "ref": r, "find": "a", "with": "b", "nth": 1.5}]),
        json!([{"op": "replace", "ref": r, "find": "a", "with": "b", "all": "yes"}]),
    ] {
        let e = run(&d.base, ops.clone()).unwrap_err();
        assert!(
            matches!(e.error, AgentError::BadRequest { .. }),
            "{ops}: {e}"
        );
    }
}

#[test]
fn deep_and_malformed_refs_never_panic() {
    let d = fixtures::document();
    let long = "A".repeat(100_000);
    let digits = format!("${}", "9".repeat(1_000_000));
    let dots = vec!["7C02"; 10_000].join(".");
    let mut refs: Vec<String> = vec![
        long.clone(),
        digits,
        dots,
        "$".into(),
        "$0".into(),
        "$-1".into(),
        "$1.2".into(),
        "7C02.99999999999999999999".into(),
        "7C02.r99999c99999".into(),
        "7C02.r0c0".into(),
        "7C02.-1".into(),
        "R91W!B7".into(),
        "!!!!".into(),
        "7C 02".into(),
        "7C02\n".into(),
        "７Ｃ０２".into(),
        "\u{0}\u{0}\u{0}\u{0}".into(),
        String::new(),
        "7C02.".into(),
        ".7C02".into(),
    ];
    // A deep chain of `$n` references to blocks that do not exist yet.
    refs.extend((1..50).map(|n| format!("${n}")));
    for r in &refs {
        for op in [
            json!({"op": "set_text", "ref": r, "text": "x"}),
            json!({"op": "replace", "ref": r, "find": "a", "with": "b", "all": true}),
            json!({"op": "delete", "ref": r}),
            json!({"op": "move", "ref": r, "after": r}),
            json!({"op": "set_type", "ref": r, "t": "h1"}),
            json!({"op": "insert", "after": r, "blocks": [{"t": "p", "text": "x"}]}),
            json!({"op": "replace_all", "find": "a", "with": "b", "args": {"scope": {"refs": [r]}}}),
        ] {
            let e = run(&d.base, json!([op])).expect_err(&format!("{r:.40}"));
            assert!(
                matches!(
                    e.error,
                    AgentError::BadRequest { .. }
                        | AgentError::UnknownRef { .. }
                        | AgentError::UnsupportedStructure
                        | AgentError::LimitExceeded { .. }
                ),
                "{r:.40}: {e}"
            );
        }
    }
}

#[test]
fn nesting_deeper_than_json_allows_is_refused_not_followed() {
    let d = fixtures::document();
    let r = rf(&d.base, &d.ids["purpose"]);
    let mut deep = json!("x");
    for _ in 0..120 {
        deep = json!([deep]);
    }
    for ops in [
        json!([{"op": "set_cells", "sheet": "S", "at": "A1", "values": deep.clone()}]),
        json!([{"op": "replace_all", "find": "a", "with": "b", "args": deep.clone()}]),
        json!([{"op": "set_text", "ref": deep.clone(), "text": "x"}]),
        json!([{"op": "insert", "after": r, "blocks": deep.clone()}]),
        json!([{"op": "replace_all", "find": "a", "with": "b", "args": {"scope": {"refs": deep.clone()}}}]),
    ] {
        let e = run(&d.base, ops).unwrap_err();
        assert!(matches!(e.error, AgentError::BadRequest { .. }), "{e}");
    }
}

#[test]
fn large_texts_finish_quickly_and_stay_within_the_scalar_limit() {
    let d = fixtures::document();
    let p = &d.ids["purpose"];
    let r = rf(&d.base, p);
    let started = Instant::now();
    // The largest allowed text, replacing a short paragraph: one coarse replacement.
    let big = "ab ".repeat(33_333);
    let c = run(&d.base, json!([{"op": "set_text", "ref": r, "text": big}])).unwrap();
    assert_eq!(block_text(&c.after.nodes[p].current_fields["content"]), big);
    assert!(c.ops.len() <= 2);
    // A second edit of the 100,000-character block: 33,333 matches in one op.
    let fx = Fixture {
        meta: d.base.meta.clone(),
        state: c.after.clone(),
    };
    let c2 = run(&fx, json!([{"op": "replace", "ref": r, "find": "ab", "with": "c", "all": true, "expect": 33_333}])).unwrap();
    assert_eq!(
        block_text(&c2.after.nodes[p].current_fields["content"]),
        "c ".repeat(33_333) + ""
    );
    // Case-insensitive and whole-word searches over it stay linear.
    let c3 = run(&fx, json!([{"op": "replace_all", "find": "AB", "with": "q", "args": {"case": false, "word": true}, "expect": 33_333}])).unwrap();
    assert!(c3.ops.len() <= 2);
    // Over the limit: refused, nothing compiled.
    let over = "x".repeat(100_001);
    let e = run(&d.base, json!([{"op": "set_text", "ref": r, "text": over}])).unwrap_err();
    assert!(matches!(
        e.error,
        AgentError::LimitExceeded {
            field: "text",
            limit: 100_000,
            value: 100_001
        }
    ));
    // A find the size of the limit and a pattern that would hurt a backtracking engine.
    let find = "a".repeat(2_000);
    let e = run(
        &d.base,
        json!([{"op": "replace_all", "find": find, "with": "b", "args": {"case": false}}]),
    )
    .unwrap_err();
    assert_eq!(e.error, AgentError::NoMatch);
    let evil = "(a+)+$";
    let e = run(&d.base, json!([{"op": "replace_all", "find": evil, "with": "b", "args": {"case": false, "word": true}}])).unwrap_err();
    assert_eq!(e.error, AgentError::NoMatch);
    let e = run(
        &d.base,
        json!([{"op": "replace_all", "find": "a".repeat(2_001), "with": "b"}]),
    )
    .unwrap_err();
    assert!(matches!(
        e.error,
        AgentError::LimitExceeded { field: "find", .. }
    ));
    assert!(started.elapsed().as_secs() < 60, "{:?}", started.elapsed());
}

// ---------------------------------------------------------------- what an op must never do

/// The only engine ops the compiler may emit, with the only fields they may touch.
fn assert_safe(fx: &Fixture, ops: &[EventPayload]) {
    let root = fx
        .state
        .nodes
        .values()
        .find(|n| n.parent_id.is_none())
        .map(|n| n.id.clone());
    let mut seen_new: BTreeSet<NodeId> = BTreeSet::new();
    for op in ops {
        match op {
            EventPayload::NodeCreated {
                node_id,
                node_type,
                parent_id,
                fields,
                var_name,
                ..
            } => {
                assert_eq!(*node_type, NodeType::Item);
                assert!(
                    var_name.is_none(),
                    "a created node carries no variable name"
                );
                let parent = parent_id.as_ref().expect("a created node has a parent");
                assert!(Some(parent) == root.as_ref() || fx.state.nodes.contains_key(parent));
                for k in fields.as_object().expect("fields are an object").keys() {
                    assert!(k == "content" || k == "label" || cell_field(k), "field {k}");
                }
                seen_new.insert(node_id.clone());
            }
            EventPayload::FieldEdited { node_id, field, .. } => {
                assert!(fx.state.nodes.contains_key(node_id) || seen_new.contains(node_id));
                assert!(
                    matches!(
                        field.as_str(),
                        "content" | "label" | "columns" | "text" | "notes"
                    ) || cell_field(field),
                    "field {field}"
                );
            }
            EventPayload::RichTextPatched { node_id, .. }
            | EventPayload::NodeMoved { node_id, .. }
            | EventPayload::NodeDeleted { node_id } => {
                assert!(fx.state.nodes.contains_key(node_id) || seen_new.contains(node_id));
            }
            other => panic!("the compiler emitted {other:?}"),
        }
    }
}

fn cell_field(k: &str) -> bool {
    k.strip_prefix("cell_")
        .and_then(|n| n.parse::<u32>().ok())
        .is_some_and(|n| n < 100)
}

#[test]
fn compiled_ops_stay_inside_a_small_vocabulary() {
    let d = fixtures::document();
    let (p, l) = (rf(&d.base, &d.ids["purpose"]), rf(&d.base, &d.ids["long"]));
    let c = run(
        &d.base,
        json!([
            {"op": "set_text", "ref": p, "text": "a"},
            {"op": "insert", "after": l, "blocks": [{"t": "h2", "text": "n"}, {"t": "li", "text": "m"}]},
            {"op": "replace_all", "find": "the", "with": "THE"},
            {"op": "set_type", "ref": "$1", "t": "p"},
            {"op": "move", "ref": "$2", "before": p},
            {"op": "delete", "ref": l},
        ]),
    )
    .unwrap();
    assert_safe(&d.base, &c.ops);
    let w = fixtures::workbook();
    let c = run(
        &w.base,
        json!([
            {"op": "set_cells", "sheet": "Budget", "at": "A100", "values": [["a", "b"]]},
            {"op": "clear", "sheet": "Budget", "range": "A1:B2"},
        ]),
    )
    .unwrap();
    assert_safe(&w.base, &c.ops);
    let k = fixtures::deck();
    let c = run(&k.base, json!([
        {"op": "set_shape", "slide": "s2", "shape": rf(&k.base, &k.ids["s2_body"]), "text": "t"},
        {"op": "set_notes", "slide": "s2", "text": "n"},
    ]))
    .unwrap();
    assert_safe(&k.base, &c.ops);
}

#[test]
fn an_op_cannot_name_a_field_a_node_type_or_a_variable() {
    let d = fixtures::document();
    let r = rf(&d.base, &d.ids["purpose"]);
    // The grammar has no way to say "var_name", "varName", "kind", "sheet_protected" ...
    for field in [
        "var_name",
        "varName",
        "field",
        "fields",
        "value",
        "node_type",
        "parent_id",
        "pos",
        "kind",
        "type",
        "sheet_protected",
        "columns",
        "label",
        "content",
    ] {
        let mut op = json!({"op": "set_text", "ref": r, "text": "x"});
        op[field] = json!("anything");
        let e = run(&d.base, json!([op])).unwrap_err();
        assert!(
            matches!(e.error, AgentError::BadRequest { .. }),
            "{field}: {e}"
        );
    }
    // A block that holds a variable name keeps it through every kind of edit.
    let mut state = d.base.state.clone();
    let id = d.ids["purpose"].clone();
    engine_core::materializer::apply_payload(
        &mut state,
        &EventPayload::FieldEdited {
            node_id: id.clone(),
            field: "var_name".into(),
            value: json!("q1"),
        },
    )
    .unwrap();
    let fx = Fixture {
        meta: d.base.meta.clone(),
        state,
    };
    let c = run(
        &fx,
        json!([
            {"op": "set_text", "ref": r, "text": "reworded"},
            {"op": "set_type", "ref": r, "t": "h3"},
            {"op": "move", "ref": r, "before": rf(&fx, &d.ids["title"])},
        ]),
    )
    .unwrap();
    assert_eq!(c.after.nodes[&id].var_name.as_deref(), Some("q1"));
    assert_safe(&fx, &c.ops);
    // A sheet name that is also a variable name or a ref is only a selector.
    let w = fixtures::workbook();
    assert!(run(
        &w.base,
        json!([{"op": "set_cells", "sheet": "q1", "at": "A1", "values": [["x"]]}])
    )
    .is_err());
}

#[test]
fn rows_shapes_and_blocks_of_other_containers_are_unreachable() {
    // A row ref from one sheet cannot address a cell of another; a shape of one slide cannot
    // be edited through another; a sheet or slide cannot be deleted or emptied by a document op.
    let w = fixtures::workbook();
    let idea = rf(&w.base, &w.ids["idea1"]);
    let e = run(&w.base, json!([{"op": "set_cells", "sheet": "Budget", "at": format!("{idea}:A"), "values": [["x"]]}])).unwrap_err();
    assert!(matches!(e.error, AgentError::UnknownRef { .. }), "{e}");
    for op in [
        json!({"op": "delete", "ref": rf(&w.base, &w.ids["budget"])}),
        json!({"op": "delete", "ref": rf(&w.base, &w.ids["row5"])}),
        json!({"op": "delete", "sheet": "Budget"}),
        json!({"op": "set_text", "ref": rf(&w.base, &w.ids["budget"]), "text": "x"}),
        json!({"op": "insert", "after": rf(&w.base, &w.ids["budget"]), "blocks": [{"t": "p", "text": "x"}]}),
    ] {
        assert!(run(&w.base, json!([op.clone()])).is_err(), "{op}");
    }
    let k = fixtures::deck();
    for op in [
        json!({"op": "delete", "ref": rf(&k.base, &k.ids["s2"])}),
        json!({"op": "delete", "ref": rf(&k.base, &k.ids["s2_body"])}),
        json!({"op": "move", "ref": rf(&k.base, &k.ids["s3"]), "after": rf(&k.base, &k.ids["s2"])}),
        json!({"op": "delete_slide", "slide": "s2"}),
        json!({"op": "add_slide", "after": "s2", "t": "blank", "blocks": []}),
        json!({"op": "insert", "into": "s2", "blocks": [{"t": "body", "text": "x"}]}),
        json!({"op": "fill", "sheet": "Budget", "from": "A1", "to": "A9", "dir": "down"}),
        json!({"op": "macro", "name": "sort_rows", "args": {}}),
    ] {
        assert!(run(&k.base, json!([op.clone()])).is_err(), "{op}");
    }
    // Deleting a document block whose node still has live children is refused whole.
    let mut b = fixtures::Builder::new(FileKind::Document, "container", 0xc0);
    let root = b.node(NodeType::Form, None, 0, json!({"label": "C"}));
    let section = b.node(NodeType::Section, Some(&root), 10, json!({"label": "S"}));
    b.node(
        NodeType::Item,
        Some(&section),
        1,
        json!({"label": "inner", "content": fixtures::para("inner")}),
    );
    let fx = b.snapshot();
    let e = run(&fx, json!([{"op": "delete", "ref": rf(&fx, &section)}])).unwrap_err();
    assert_eq!(e.error, AgentError::UnsupportedStructure);
}

#[test]
fn the_state_of_a_failed_call_is_untouched() {
    // Atomic: a failure leaves nothing behind, and the input state is never mutated.
    let d = fixtures::document();
    let before: DocumentState = d.base.state.clone();
    let r = rf(&d.base, &d.ids["purpose"]);
    let e = run(
        &d.base,
        json!([
            {"op": "set_text", "ref": r, "text": "changed"},
            {"op": "insert", "after": r, "blocks": [{"t": "p", "text": "n"}]},
            {"op": "replace", "ref": "$1", "find": "absent", "with": "x"},
        ]),
    )
    .unwrap_err();
    assert_eq!((e.op, e.error), (Some(2), AgentError::NoMatch));
    assert_eq!(d.base.state, before);
}
