//! The operation compiler (protocol spec 8, 9, 17 "compile property"): every op against the
//! fixture corpus, the rejections of spec 10, atomicity, staleness, and the guarantee that
//! what comes out applies and round-trips through `workspace_merge::diff`.

use std::collections::BTreeSet;

use engine_agent::compile::{
    compile, ChangeKind, CompileInput, Compiled, DerivedIds, IdSource, MAX_ENGINE_OPS,
};
use engine_agent::error::{AgentError, CompileError};
use engine_agent::fixtures::{self, Fixture};
use engine_agent::grammar::parse_ops;
use engine_agent::hash::{block_hash, range_hash, shape_hash};
use engine_agent::refs::{Ref, RefIndex};
use engine_core::governance::{authorize, Role};
use engine_core::materializer::{apply_payload, DocumentState};
use engine_core::ops::validate_op;
use engine_core::richtext::plain_text;
use engine_core::workspace_merge;
use engine_shared::{EventPayload, NodeId};
use serde_json::{json, Value};

// ---------------------------------------------------------------- helpers

/// New ids: ULID-shaped, distinct from the fixtures' (their stream starts elsewhere).
fn ids() -> impl IdSource {
    let mut n = 0u64;
    move || {
        n += 1;
        fixtures::ulid(7_000_000 + n)
    }
}

fn run_with(
    fx: &Fixture,
    head: Option<&DocumentState>,
    strict: bool,
    redacted: &[&NodeId],
    ops: Value,
) -> Result<Compiled, CompileError> {
    let ops = parse_ops(&ops)?;
    let mut input = CompileInput::new(&fx.meta, &fx.state, Role::Author);
    input.head = head;
    input.strict = strict;
    input.redacted = redacted.iter().map(|id| (*id).clone()).collect();
    compile(&input, &ops, &mut ids())
}

fn run(fx: &Fixture, ops: Value) -> Result<Compiled, CompileError> {
    run_with(fx, None, false, &[], ops)
}

fn ok(fx: &Fixture, ops: Value) -> Compiled {
    run(fx, ops).unwrap_or_else(|e| panic!("{e}"))
}

fn err(fx: &Fixture, ops: Value) -> CompileError {
    run(fx, ops).expect_err("the call must fail")
}

fn rf(fx: &Fixture, id: &NodeId) -> String {
    RefIndex::from_state(&fx.state)
        .ref_of(id)
        .unwrap()
        .to_string()
}

fn content(state: &DocumentState, id: &NodeId) -> Value {
    state.nodes[id].current_fields["content"].clone()
}

fn text(state: &DocumentState, id: &NodeId) -> String {
    plain_text(&content(state, id))
}

/// What the engine would do with `ops`: every one is valid and authorised in turn, applies,
/// and the result is the state the compiler reports.
fn assert_applies(fx: &Fixture, c: &Compiled) {
    let mut state = fx.state.clone();
    for op in &c.ops {
        authorize(Role::Author, op, &state).unwrap_or_else(|e| panic!("{e}: {op:?}"));
        validate_op(&state, op).unwrap();
        apply_payload(&mut state, op).unwrap();
    }
    assert_eq!(
        state, c.after,
        "the compiled ops reproduce the reported state"
    );
    assert!(c.ops.len() <= MAX_ENGINE_OPS);
}

/// No phantom ops: diffing the states gives ops that rebuild `after` from `before` and touch
/// nothing the compiled ops did not touch.
fn assert_round_trip(fx: &Fixture, c: &Compiled) {
    let d = workspace_merge::diff(&fx.state, &c.after);
    let mut replay = fx.state.clone();
    for op in &d {
        authorize(Role::Author, op, &replay).unwrap_or_else(|e| panic!("{e}: {op:?}"));
        apply_payload(&mut replay, op).unwrap();
    }
    assert_eq!(
        live(&replay),
        live(&c.after),
        "diff replays to the compiled state"
    );
    let touched: BTreeSet<&NodeId> = c
        .ops
        .iter()
        .filter_map(EventPayload::target_node_id)
        .collect();
    for op in &d {
        let id = op.target_node_id().unwrap();
        assert!(touched.contains(id), "phantom op on {id:?}: {op:?}");
    }
}

/// The comparable part of a state: live nodes with their parent, position and fields.
fn live(s: &DocumentState) -> Vec<(NodeId, Option<NodeId>, String, Value)> {
    s.nodes
        .values()
        .filter(|n| !n.deleted)
        .map(|n| {
            (
                n.id.clone(),
                n.parent_id.clone(),
                n.pos.clone(),
                n.current_fields.clone(),
            )
        })
        .collect()
}

fn check(fx: &Fixture, c: &Compiled) {
    assert_applies(fx, c);
    assert_round_trip(fx, c);
}

fn code(e: &CompileError) -> &'static str {
    e.error.agent_code()
}

/// Live blocks of a document in reading order.
fn blocks(state: &DocumentState) -> Vec<(NodeId, String)> {
    let root = state
        .nodes
        .values()
        .find(|n| !n.deleted && n.parent_id.is_none())
        .unwrap();
    let mut v: Vec<_> = state
        .nodes
        .values()
        .filter(|n| {
            !n.deleted
                && n.parent_id.as_ref() == Some(&root.id)
                && n.current_fields["content"].is_object()
        })
        .collect();
    v.sort_by(|a, b| (&a.pos, &a.id).cmp(&(&b.pos, &b.id)));
    v.into_iter()
        .map(|n| (n.id.clone(), plain_text(&n.current_fields["content"])))
        .collect()
}

fn doc() -> fixtures::DocumentFixture {
    fixtures::document()
}

// ---------------------------------------------------------------- set_text

#[test]
fn set_text_keeps_the_marks_of_unchanged_spans() {
    let d = doc();
    let intro = &d.ids["intro"];
    let before = content(&d.base.state, intro);
    assert_eq!(before["content"][1]["marks"], json!([{"type": "bold"}]));
    let c = ok(
        &d.base,
        json!([{
            "op": "set_text", "ref": rf(&d.base, intro), "id": "intro",
            "text": "This audit covers the third quarter and links to the policy site.",
            "base": block_hash(&before).h12(), "why": "tone"
        }]),
    );
    check(&d.base, &c);
    let after = content(&c.after, intro);
    assert_eq!(
        plain_text(&after),
        "This audit covers the third quarter and links to the policy site."
    );
    // The bold phrase and the link are the same characters with the same marks.
    let runs: Vec<(&str, bool, bool)> = after["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["text"].as_str().unwrap(),
                r["marks"][0]["type"] == "bold",
                r["marks"][0]["type"] == "link",
            )
        })
        .collect();
    assert!(runs.contains(&("third quarter", true, false)), "{runs:?}");
    assert!(runs.contains(&("policy site", false, true)), "{runs:?}");
    // A whole-field edit with its label or, when smaller, one checked patch whose result
    // carries the label itself: never a bare patch plus a label edit.
    match c.ops.as_slice() {
        [EventPayload::RichTextPatched { .. }] => {}
        [EventPayload::FieldEdited { field: a, .. }, EventPayload::FieldEdited { field: b, .. }] => {
            assert_eq!((a.as_str(), b.as_str()), ("content", "label"));
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        c.after.nodes[intro].current_fields["label"],
        json!(plain_text(&after))
    );
    let r = &c.results[0];
    assert_eq!(
        (r.index, r.id.as_deref(), r.op, r.engine_ops),
        (0, Some("intro"), "set_text", c.ops.len())
    );
    assert_eq!(r.changes.len(), 1);
    assert_eq!(r.changes[0].kind, ChangeKind::Changed);
    assert_eq!(r.changes[0].old, Some(block_hash(&before)));
    assert_eq!(r.changes[0].new, Some(block_hash(&after)));
    assert_eq!(r.changes[0].at.to_string(), rf(&d.base, intro));
    assert!(c.stale.is_empty());
}

#[test]
fn set_text_with_unchanged_text_is_no_change() {
    let d = doc();
    let p = &d.ids["purpose"];
    let same = text(&d.base.state, p);
    let e = err(
        &d.base,
        json!([{"op": "set_text", "ref": rf(&d.base, p), "text": same}]),
    );
    assert_eq!(e.error, AgentError::NoChange);
    assert_eq!(e.op, Some(0));
}

#[test]
fn set_text_on_bengali_and_hard_breaks() {
    let d = doc();
    let b = &d.ids["bengali"];
    let c = ok(
        &d.base,
        json!([{"op": "set_text", "ref": rf(&d.base, b), "text": "নতুন বাক্য। মোট: ১২৩৪ টাকা।"}]),
    );
    check(&d.base, &c);
    assert_eq!(text(&c.after, b), "নতুন বাক্য। মোট: ১২৩৪ টাকা।");
    // A line feed becomes a hardBreak and reads back as a line feed.
    let c = ok(
        &d.base,
        json!([{"op": "set_text", "ref": rf(&d.base, b), "text": "one\ntwo"}]),
    );
    check(&d.base, &c);
    let after = content(&c.after, b);
    assert!(after["content"]
        .as_array()
        .unwrap()
        .iter()
        .any(|n| n["type"] == "hardBreak"));
}

#[test]
fn set_text_on_a_rich_block_needs_the_placeholders() {
    let d = doc();
    let atoms = &d.ids["atoms"];
    let r = rf(&d.base, atoms);
    let read = text(&d.base.state, atoms);
    assert!(read.contains("a^2+b^2=c^2"));
    // Dropping the footnote and the formula would destroy them.
    let e = err(
        &d.base,
        json!([{"op": "set_text", "ref": r, "text": "Plain only."}]),
    );
    assert_eq!(e.error, AgentError::RichBlock);
    // Keeping both placeholders in order edits the words around them.
    let c = ok(
        &d.base,
        json!([{
            "op": "set_text", "ref": r,
            "text": "Footnote there⟦fn:01K7Q9XM2AFOOTNOTE0000000A⟧ and then math ⟦math:a^2+b^2=c^2⟧."
        }]),
    );
    check(&d.base, &c);
    let after = content(&c.after, atoms);
    let kinds: Vec<&str> = after["content"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| n["type"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        ["text", "documentNote", "text", "inlineMath", "text"]
    );
    assert_eq!(
        after["content"][1],
        content(&d.base.state, atoms)["content"][1]
    );
}

#[test]
fn set_text_sub_addresses_list_items_and_table_cells() {
    let d = doc();
    let bullets = &d.ids["bullets"];
    let r = rf(&d.base, bullets);
    // Item 3 depth-first is the first nested item "Nested one".
    let c = ok(
        &d.base,
        json!([{"op": "set_text", "ref": format!("{r}.3"), "text": "Nested uno"}]),
    );
    check(&d.base, &c);
    assert!(text(&c.after, bullets).contains("Nested uno"));
    assert!(text(&c.after, bullets).contains("Nested two"));
    let table = &d.ids["table"];
    let t = rf(&d.base, table);
    let c = ok(
        &d.base,
        json!([{"op": "set_text", "ref": format!("{t}.r3c2"), "text": "Rajesh"}]),
    );
    check(&d.base, &c);
    assert!(text(&c.after, table).contains("Rajesh"));
    // Out of range, and the block itself is not a text container.
    assert_eq!(
        code(&err(
            &d.base,
            json!([{"op": "set_text", "ref": format!("{r}.9"), "text": "x"}])
        )),
        "unknown_ref"
    );
    assert_eq!(
        code(&err(
            &d.base,
            json!([{"op": "set_text", "ref": t, "text": "x"}])
        )),
        "unsupported_structure"
    );
    assert_eq!(
        code(&err(
            &d.base,
            json!([{"op": "set_text", "ref": format!("{r}.r1c1"), "text": "x"}])
        )),
        "unsupported_structure"
    );
}

#[test]
fn text_in_a_non_text_block_is_unsupported() {
    let d = doc();
    for name in ["quote", "math", "image", "rule", "toc", "code_missing"] {
        let Some(id) = d.ids.get(name) else { continue };
        let e = err(
            &d.base,
            json!([{"op": "set_text", "ref": rf(&d.base, id), "text": "x"}]),
        );
        assert_eq!(code(&e), "unsupported_structure", "{name}");
    }
    // A code block is a text container; its line feeds are characters.
    let code_id = &d.ids["code"];
    let c = ok(
        &d.base,
        json!([{"op": "set_text", "ref": rf(&d.base, code_id), "text": "fn main() {\n}"}]),
    );
    check(&d.base, &c);
    assert_eq!(text(&c.after, code_id), "fn main() {\n}");
}

// ---------------------------------------------------------------- replace

#[test]
fn replace_changes_only_the_matched_span() {
    let d = doc();
    let p = &d.ids["purpose"];
    let r = rf(&d.base, p);
    let before = text(&d.base.state, p);
    assert!(before.contains("policy"));
    let c = ok(
        &d.base,
        json!([{"op": "replace", "ref": r, "find": "policy", "with": "procedure"}]),
    );
    check(&d.base, &c);
    assert_eq!(text(&c.after, p), before.replace("policy", "procedure"));
    // The engine keeps the smaller form: a whole-field edit (with its label) for a short
    // paragraph, a checked patch for a long one.
    assert!(matches!(c.ops.len(), 1 | 2), "{:?}", c.ops);
    assert_eq!(c.results[0].engine_ops, c.ops.len());
    // Idempotent: the second run finds nothing.
    let again = run(
        &d.edited,
        json!([{"op": "replace", "ref": rf(&d.edited, p), "find": "policy", "with": "procedure"}]),
    );
    assert!(again.is_err());
}

#[test]
fn replace_match_rules() {
    let d = doc();
    let long = &d.ids["long"];
    let r = rf(&d.base, long);
    let t = text(&d.base.state, long);
    let word = t.split_whitespace().next().unwrap().to_string();
    let n = t.matches(&word).count();
    assert!(n >= 2, "the fixture paragraph repeats {word:?}");
    // More than one match and nothing to choose among them.
    let e = err(
        &d.base,
        json!([{"op": "replace", "ref": r, "find": word, "with": "X"}]),
    );
    assert_eq!(
        e.error,
        AgentError::MatchCount {
            found: n as u64,
            expected: 1,
            refs: vec![Ref::parse(&r).unwrap()]
        }
    );
    // nth, all and expect.
    let first = ok(
        &d.base,
        json!([{"op": "replace", "ref": r, "find": word, "with": "X", "nth": 1}]),
    );
    assert_eq!(text(&first.after, long), t.replacen(&word, "X", 1));
    let last = ok(
        &d.base,
        json!([{"op": "replace", "ref": r, "find": word, "with": "X", "nth": n}]),
    );
    let idx = t.match_indices(&word).last().unwrap().0;
    assert_eq!(
        text(&last.after, long),
        format!("{}X{}", &t[..idx], &t[idx + word.len()..])
    );
    let all = ok(
        &d.base,
        json!([{"op": "replace", "ref": r, "find": word, "with": "X", "all": true, "expect": n}]),
    );
    assert_eq!(text(&all.after, long), t.replace(&word, "X"));
    check(&d.base, &all);
    // `expect` alone with the right count replaces all of them.
    let by_count = ok(
        &d.base,
        json!([{"op": "replace", "ref": r, "find": word, "with": "X", "expect": n}]),
    );
    assert_eq!(by_count.after, all.after);
    // Wrong counts and absent text.
    let e = err(
        &d.base,
        json!([{"op": "replace", "ref": r, "find": word, "with": "X", "expect": n + 1}]),
    );
    assert!(matches!(e.error, AgentError::MatchCount { expected, .. } if expected == n as u64 + 1));
    assert_eq!(
        err(
            &d.base,
            json!([{"op": "replace", "ref": r, "find": "zzzz-not-there", "with": "X"}])
        )
        .error,
        AgentError::NoMatch
    );
    assert_eq!(
        err(
            &d.base,
            json!([{"op": "replace", "ref": r, "find": word, "with": "X", "nth": n + 1}])
        )
        .error,
        AgentError::NoMatch
    );
    // Replacing text with itself changes nothing.
    assert_eq!(
        err(
            &d.base,
            json!([{"op": "replace", "ref": r, "find": word, "with": word, "all": true}])
        )
        .error,
        AgentError::NoChange
    );
    // The find is literal and case-sensitive.
    assert_eq!(
        err(
            &d.base,
            json!([{"op": "replace", "ref": r, "find": word.to_uppercase(), "with": "X"}])
        )
        .error,
        AgentError::NoMatch
    );
    assert_eq!(
        err(
            &d.base,
            json!([{"op": "replace", "ref": r, "find": ".*", "with": "X"}])
        )
        .error,
        AgentError::NoMatch
    );
}

#[test]
fn replace_in_a_block_with_several_containers_counts_across_them() {
    let d = doc();
    let table = &d.ids["table"];
    let r = rf(&d.base, table);
    let c = ok(
        &d.base,
        json!([{"op": "replace", "ref": r, "find": "Ana", "with": "Anna"}]),
    );
    check(&d.base, &c);
    assert!(text(&c.after, table).contains("Anna"));
    // Only the addressed cell when a sub-address is given.
    let e = err(
        &d.base,
        json!([{"op": "replace", "ref": format!("{r}.r1c1"), "find": "Ana", "with": "Anna"}]),
    );
    assert_eq!(e.error, AgentError::NoMatch);
}

#[test]
fn replace_never_touches_an_inline_object() {
    let d = doc();
    let atoms = &d.ids["atoms"];
    let r = rf(&d.base, atoms);
    let before = content(&d.base.state, atoms);
    let c = ok(
        &d.base,
        json!([{"op": "replace", "ref": r, "find": "Footnote here", "with": "Note here"}]),
    );
    check(&d.base, &c);
    let after = content(&c.after, atoms);
    assert_eq!(after["content"][1], before["content"][1]);
    assert_eq!(after["content"][3], before["content"][3]);
    // The LaTeX of the formula is not paragraph text.
    assert_eq!(
        err(
            &d.base,
            json!([{"op": "replace", "ref": r, "find": "a^2+b^2", "with": "x"}])
        )
        .error,
        AgentError::NoMatch
    );
}

// ---------------------------------------------------------------- replace_all

#[test]
fn replace_all_sweeps_the_document_and_is_idempotent() {
    let d = doc();
    let want: Vec<(NodeId, String)> = blocks(&d.base.state);
    let total: usize = want.iter().map(|(_, t)| t.matches("policy").count()).sum();
    let hit_blocks = want.iter().filter(|(_, t)| t.contains("policy")).count();
    assert!(
        total >= 3 && hit_blocks >= 2,
        "total {total} in {hit_blocks} blocks"
    );
    let ops =
        json!([{"op": "replace_all", "find": "policy", "with": "procedure", "expect": total}]);
    let c = ok(&d.base, ops.clone());
    check(&d.base, &c);
    for (id, t) in &want {
        assert_eq!(&text(&c.after, id), &t.replace("policy", "procedure"));
    }
    // One result per op, one change per affected block.
    assert_eq!(c.results.len(), 1);
    assert_eq!(c.results[0].changes.len(), hit_blocks);
    // The same call again finds nothing: zero matches is no_match, not a silent success.
    let fx2 = Fixture {
        meta: d.base.meta.clone(),
        state: c.after.clone(),
    };
    assert_eq!(
        err(&fx2, ops).error,
        AgentError::MatchCount {
            found: 0,
            expected: total as u64,
            refs: vec![]
        }
    );
    assert_eq!(
        err(
            &fx2,
            json!([{"op": "replace_all", "find": "policy", "with": "procedure"}])
        )
        .error,
        AgentError::NoMatch
    );
    // A wrong expectation fails the whole call and names a few of the blocks.
    let e = err(
        &d.base,
        json!([{"op": "replace_all", "find": "policy", "with": "procedure", "expect": total + 1}]),
    );
    assert!(
        matches!(&e.error, AgentError::MatchCount { found, refs, .. } if *found == total as u64 && !refs.is_empty())
    );
}

#[test]
fn replace_all_equals_the_replaces_it_expands_to() {
    let d = doc();
    let all = ok(
        &d.base,
        json!([{"op": "replace_all", "find": "the", "with": "THE", "args": {"word": true}}]),
    );
    // The same edit as one `replace` per matching block.
    let mut ops = Vec::new();
    for (id, t) in blocks(&d.base.state) {
        let words = t
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| *w == "the")
            .count();
        // Table, list and quote text is reached through their own containers, which the
        // single-block replace also searches; only whole-word matches count here.
        let _ = (id, words);
    }
    for (id, _) in blocks(&d.base.state) {
        ops.push(json!({"op": "replace_all", "find": "the", "with": "THE",
                        "args": {"word": true, "scope": {"refs": [rf(&d.base, &id)]}}}));
    }
    // Per-block calls fail when a block has no match; run only the ones that match.
    let mut state = d.base.state.clone();
    let mut fx = Fixture {
        meta: d.base.meta.clone(),
        state: state.clone(),
    };
    for op in ops {
        if let Ok(c) = run(&fx, json!([op])) {
            state = c.after;
            fx = Fixture {
                meta: d.base.meta.clone(),
                state: state.clone(),
            };
        }
    }
    assert_eq!(live(&state), live(&all.after));
    check(&d.base, &all);
}

#[test]
fn replace_all_scopes_and_options() {
    let d = doc();
    let heading = rf(&d.base, &d.ids["purpose_h"]);
    // Under a heading: the section up to the next heading of the same or higher level.
    let under = ok(
        &d.base,
        json!([{"op": "replace_all", "find": "policy", "with": "rule", "args": {"scope": {"under": heading}}}]),
    );
    let touched: Vec<&NodeId> = under
        .ops
        .iter()
        .filter_map(EventPayload::target_node_id)
        .collect();
    assert!(touched.contains(&&d.ids["purpose"]));
    assert!(!touched.contains(&&d.ids["intro"]));
    // Types.
    let only_h = run(
        &d.base,
        json!([{"op": "replace_all", "find": "Purpose", "with": "Aim", "args": {"scope": {"types": ["h"]}}}]),
    );
    let c = only_h.unwrap();
    assert_eq!(text(&c.after, &d.ids["purpose_h"]), "Aim");
    assert_eq!(
        err(&d.base, json!([{"op": "replace_all", "find": "Purpose", "with": "Aim", "args": {"scope": {"types": ["P"]}}}])).error,
        AgentError::NoMatch
    );
    // Case-insensitive and whole words.
    let ci = ok(
        &d.base,
        json!([{"op": "replace_all", "find": "PURPOSE", "with": "Aim", "args": {"case": false, "scope": {"refs": [heading]}}}]),
    );
    assert_eq!(text(&ci.after, &d.ids["purpose_h"]), "Aim");
    let w = ok(
        &d.base,
        json!([{"op": "replace_all", "find": "polic", "with": "x"}]),
    );
    check(&d.base, &w);
    assert_eq!(
        err(
            &d.base,
            json!([{"op": "replace_all", "find": "polic", "with": "x", "args": {"word": true}}])
        )
        .error,
        AgentError::NoMatch
    );
    // `base` has no meaning for many blocks.
    assert_eq!(
        err(
            &d.base,
            json!([{"op": "replace_all", "find": "policy", "with": "x", "base": "abcdef"}])
        )
        .error,
        AgentError::BadRequest { field: "base" }
    );
}

#[test]
fn replace_all_over_many_blocks_needs_expect() {
    // 30 blocks that each contain the word.
    let mut b = fixtures::Builder::new(engine_agent::model::FileKind::Document, "big", 0xb16);
    let root = b.node(
        engine_shared::NodeType::Form,
        None,
        0,
        json!({"label": "Big"}),
    );
    for i in 0..30u64 {
        let content = fixtures::para("the old word here");
        b.node(
            engine_shared::NodeType::Item,
            Some(&root),
            (i + 1) * 10,
            json!({"label": plain_text(&content), "content": content}),
        );
    }
    let fx = b.snapshot();
    let e = err(
        &fx,
        json!([{"op": "replace_all", "find": "old", "with": "new"}]),
    );
    assert_eq!(e.error, AgentError::BadRequest { field: "expect" });
    let c = ok(
        &fx,
        json!([{"op": "replace_all", "find": "old", "with": "new", "expect": 30}]),
    );
    assert_eq!(c.results[0].changes.len(), 30);
    assert!((30..=60).contains(&c.ops.len()), "{}", c.ops.len());
    check(&fx, &c);
    assert!(blocks(&c.after)
        .iter()
        .all(|(_, t)| t == "the new word here"));
}

// ---------------------------------------------------------------- insert, delete, move, set_type

#[test]
fn insert_places_blocks_between_their_neighbours() {
    let d = doc();
    let anchor = &d.ids["purpose"];
    let order = blocks(&d.base.state);
    let i = order.iter().position(|(id, _)| id == anchor).unwrap();
    let c = ok(
        &d.base,
        json!([{"op": "insert", "after": rf(&d.base, anchor), "blocks": [
            {"t": "h2", "text": "Background"}, {"t": "p", "text": "First line\nsecond line"}, {"t": "li", "text": "an item"}, {"t": "code", "text": "x = 1"}
        ]}]),
    );
    check(&d.base, &c);
    let after = blocks(&c.after);
    assert_eq!(after.len(), order.len() + 4);
    // The four new blocks sit right after the anchor, in order, and nothing else moved.
    let texts: Vec<&str> = after[i + 1..i + 5]
        .iter()
        .map(|(_, t)| t.as_str())
        .collect();
    assert_eq!(
        texts,
        ["Background", "First linesecond line", "an item", "x = 1"]
    );
    let before_ids: Vec<&NodeId> = order.iter().map(|(id, _)| id).collect();
    let kept: Vec<&NodeId> = after
        .iter()
        .map(|(id, _)| id)
        .filter(|id| before_ids.contains(id))
        .collect();
    assert_eq!(kept, before_ids);
    // Shape of the new nodes (spec 8.2).
    for op in &c.ops {
        let EventPayload::NodeCreated {
            node_id,
            node_type,
            parent_id,
            fields,
            var_name,
            ..
        } = op
        else {
            panic!("{op:?}")
        };
        assert_eq!(node_type.as_str(), "item");
        assert_eq!(
            parent_id.as_ref(),
            Some(&NodeId(
                c.after.nodes[&d.ids["title"]].parent_id.clone().unwrap().0
            ))
        );
        assert_eq!(fields["content"]["attrs"]["blockId"], json!(node_id));
        assert_eq!(fields["label"], json!(plain_text(&fields["content"])));
        assert!(var_name.is_none());
    }
    let new_ids: Vec<&NodeId> = c
        .ops
        .iter()
        .filter_map(EventPayload::target_node_id)
        .collect();
    assert_eq!(
        c.results[0]
            .changes
            .iter()
            .map(|ch| ch.kind)
            .collect::<Vec<_>>(),
        vec![ChangeKind::Created; 4]
    );
    assert_eq!(c.results[0].changes.len(), new_ids.len());
    // Positions are strictly between the neighbours.
    let pos = |id: &NodeId| c.after.nodes[id].pos.clone();
    let mut keys = vec![pos(&order[i].0)];
    keys.extend(new_ids.iter().map(|id| pos(id)));
    if let Some(next) = order.get(i + 1) {
        keys.push(pos(&next.0));
    }
    assert!(keys.windows(2).all(|w| w[0] < w[1]), "{keys:?}");
}

#[test]
fn insert_before_the_first_block_and_dollar_references() {
    let d = doc();
    let first = &blocks(&d.base.state)[0].0.clone();
    let c = ok(
        &d.base,
        json!([
            {"op": "insert", "before": rf(&d.base, first), "blocks": [{"t": "p", "text": "Preface"}]},
            {"op": "set_text", "ref": "$1", "text": "Preface, revised"},
            {"op": "insert", "after": "$1", "blocks": [{"t": "p", "text": "Second"}]},
            {"op": "set_type", "ref": "$2", "t": "h3"},
        ]),
    );
    check(&d.base, &c);
    let after = blocks(&c.after);
    assert_eq!(after[0].1, "Preface, revised");
    assert_eq!(after[1].1, "Second");
    assert_eq!(content(&c.after, &after[1].0)["type"], "heading");
    assert_eq!(content(&c.after, &after[1].0)["attrs"]["level"], 3);
    // Forward references and unknown numbers.
    assert_eq!(
        code(&err(
            &d.base,
            json!([{"op": "set_text", "ref": "$1", "text": "x"}])
        )),
        "bad_request"
    );
    // A block made in this call has no earlier version to guard.
    let e = err(
        &d.base,
        json!([
            {"op": "insert", "before": rf(&d.base, first), "blocks": [{"t": "p", "text": "A"}]},
            {"op": "set_text", "ref": "$1", "text": "B", "base": "abcdef"},
        ]),
    );
    assert_eq!(
        (e.op, e.error),
        (Some(1), AgentError::BadRequest { field: "base" })
    );
}

#[test]
fn delete_tombstones_a_block() {
    let d = doc();
    let victim = &d.ids["scope_h"];
    let c = ok(
        &d.base,
        json!([{"op": "delete", "ref": rf(&d.base, victim)}]),
    );
    check(&d.base, &c);
    assert!(c.after.nodes[victim].deleted);
    assert_eq!(
        c.ops,
        vec![EventPayload::NodeDeleted {
            node_id: victim.clone()
        }]
    );
    assert_eq!(c.results[0].changes[0].kind, ChangeKind::Deleted);
    // The tombstone keeps its ref meaning, but a deleted block cannot be edited again.
    let fx = Fixture {
        meta: d.base.meta.clone(),
        state: c.after.clone(),
    };
    let e = err(&fx, json!([{"op": "delete", "ref": rf(&d.base, victim)}]));
    assert_eq!(code(&e), "not_found");
    // The removed fixture block is already a tombstone.
    let gone = &d.ids["removed"];
    assert_eq!(
        code(&err(
            &d.base,
            json!([{"op": "set_text", "ref": rf(&d.base, gone), "text": "x"}])
        )),
        "not_found"
    );
}

#[test]
fn a_block_with_live_children_is_never_deleted_silently() {
    // An item that holds a choice: validate_op refuses to tombstone it.
    let mut b = fixtures::Builder::new(engine_agent::model::FileKind::Document, "kids", 0xc41d);
    let root = b.node(
        engine_shared::NodeType::Form,
        None,
        0,
        json!({"label": "K"}),
    );
    let content = fixtures::para("A question");
    let item = b.node(
        engine_shared::NodeType::Item,
        Some(&root),
        10,
        json!({"label": "A question", "content": content}),
    );
    b.node(
        engine_shared::NodeType::Choice,
        Some(&item),
        1,
        json!({"label": "yes"}),
    );
    let fx = b.snapshot();
    let r = RefIndex::from_state(&fx.state)
        .ref_of(&item)
        .unwrap()
        .to_string();
    assert_eq!(
        err(&fx, json!([{"op": "delete", "ref": r}])).error,
        AgentError::UnsupportedStructure
    );
}

#[test]
fn move_keeps_ids_and_orders_the_rest() {
    let d = doc();
    let order = blocks(&d.base.state);
    let (mv, anchor) = (&d.ids["appendix_h"], &d.ids["purpose_h"]);
    let c = ok(
        &d.base,
        json!([{"op": "move", "ref": rf(&d.base, mv), "before": rf(&d.base, anchor)}]),
    );
    check(&d.base, &c);
    let after = blocks(&c.after);
    assert_eq!(after.len(), order.len());
    // Same ids, only the moved one changed place, and it sits right before the anchor.
    let mut expected: Vec<NodeId> = order
        .iter()
        .map(|(id, _)| id.clone())
        .filter(|id| id != mv)
        .collect();
    let at = expected.iter().position(|id| id == anchor).unwrap();
    expected.insert(at, mv.clone());
    assert_eq!(
        after.iter().map(|(id, _)| id.clone()).collect::<Vec<_>>(),
        expected
    );
    assert_eq!(c.ops.len(), 1);
    assert!(matches!(c.ops[0], EventPayload::NodeMoved { .. }));
    assert_eq!(
        c.after.nodes[mv].current_fields,
        d.base.state.nodes[mv].current_fields
    );
    assert_eq!(c.results[0].changes[0].kind, ChangeKind::Moved);
    // After the anchor, and no-ops.
    let c = ok(
        &d.base,
        json!([{"op": "move", "ref": rf(&d.base, mv), "after": rf(&d.base, anchor)}]),
    );
    check(&d.base, &c);
    let pos = blocks(&c.after)
        .iter()
        .position(|(id, _)| id == mv)
        .unwrap();
    assert_eq!(blocks(&c.after)[pos - 1].0, *anchor);
    let (first, second) = (&order[0].0, &order[1].0);
    assert_eq!(
        err(
            &d.base,
            json!([{"op": "move", "ref": rf(&d.base, second), "after": rf(&d.base, first)}])
        )
        .error,
        AgentError::NoChange
    );
    assert_eq!(
        err(
            &d.base,
            json!([{"op": "move", "ref": rf(&d.base, first), "before": rf(&d.base, second)}])
        )
        .error,
        AgentError::NoChange
    );
    assert_eq!(
        err(
            &d.base,
            json!([{"op": "move", "ref": rf(&d.base, first), "after": rf(&d.base, first)}])
        )
        .error,
        AgentError::BadRequest { field: "after" }
    );
}

#[test]
fn set_type_changes_the_block_type_and_nothing_else() {
    let d = doc();
    let p = &d.ids["purpose"];
    let c = ok(
        &d.base,
        json!([{"op": "set_type", "ref": rf(&d.base, p), "t": "h4"}]),
    );
    check(&d.base, &c);
    let (before, after) = (content(&d.base.state, p), content(&c.after, p));
    assert_eq!(
        (after["type"].as_str(), after["attrs"]["level"].clone()),
        (Some("heading"), json!(4))
    );
    assert_eq!(after["content"], before["content"]);
    assert_eq!(plain_text(&after), plain_text(&before));
    // Back to a paragraph drops the level and keeps the rest.
    let fx = Fixture {
        meta: d.base.meta.clone(),
        state: c.after.clone(),
    };
    let back = ok(
        &fx,
        json!([{"op": "set_type", "ref": rf(&d.base, p), "t": "p"}]),
    );
    assert_eq!(content(&back.after, p)["type"], "paragraph");
    assert!(content(&back.after, p)["attrs"].get("level").is_none());
    // Heading to heading changes only the level; the same type is no change.
    let title = &d.ids["title"];
    let h2 = ok(
        &d.base,
        json!([{"op": "set_type", "ref": rf(&d.base, title), "t": "h2"}]),
    );
    assert_eq!(content(&h2.after, title)["attrs"]["level"], 2);
    assert_eq!(
        err(
            &d.base,
            json!([{"op": "set_type", "ref": rf(&d.base, title), "t": "h1"}])
        )
        .error,
        AgentError::NoChange
    );
    // Lists, tables, code and math are not converted.
    for name in ["bullets", "table", "code", "math", "quote"] {
        let e = err(
            &d.base,
            json!([{"op": "set_type", "ref": rf(&d.base, &d.ids[name]), "t": "p"}]),
        );
        assert_eq!(e.error, AgentError::UnsupportedStructure, "{name}");
    }
}

// ---------------------------------------------------------------- refs, atomicity, roles

#[test]
fn refs_resolve_as_the_read_path_resolves_them() {
    let d = doc();
    let p = &d.ids["purpose"];
    let r = rf(&d.base, p);
    // Lower case, and the full ULID, name the same node.
    for form in [r.to_lowercase(), p.0.clone(), p.0.to_lowercase()] {
        let c = ok(
            &d.base,
            json!([{"op": "set_text", "ref": form, "text": "x"}]),
        );
        assert_eq!(
            c.ops.iter().filter_map(EventPayload::target_node_id).next(),
            Some(p)
        );
    }
    // Unknown and malformed.
    assert_eq!(
        code(&err(
            &d.base,
            json!([{"op": "set_text", "ref": "ZZZZ", "text": "x"}])
        )),
        "unknown_ref"
    );
    assert_eq!(
        code(&err(
            &d.base,
            json!([{"op": "set_text", "ref": "no", "text": "x"}])
        )),
        "bad_request"
    );
    // A ref from another document is just an unknown ref here.
    let other = fixtures::workbook();
    let foreign = other.ids["budget"].0.clone();
    assert_eq!(
        code(&err(
            &d.base,
            json!([{"op": "set_text", "ref": foreign, "text": "x"}])
        )),
        "unknown_ref"
    );
    // A node that is not a block of this file.
    let root = d
        .base
        .state
        .nodes
        .values()
        .find(|n| n.parent_id.is_none())
        .unwrap();
    assert_eq!(
        code(&err(
            &d.base,
            json!([{"op": "set_text", "ref": root.id.0.clone(), "text": "x"}])
        )),
        "unsupported_structure"
    );
}

#[test]
fn ambiguous_refs_list_candidates() {
    // Two blocks whose ids end alike.
    let mut b = fixtures::Builder::new(engine_agent::model::FileKind::Document, "amb", 0xa3b);
    let root = b.node(
        engine_shared::NodeType::Form,
        None,
        0,
        json!({"label": "A"}),
    );
    let mk = |b: &mut fixtures::Builder, id: &str, pos: u64| {
        let content = fixtures::para("text");
        b.apply(EventPayload::NodeCreated {
            node_id: NodeId(id.into()),
            node_type: engine_shared::NodeType::Item,
            parent_id: Some(root.clone()),
            pos: format!("{pos:08}"),
            fields: json!({"label": "text", "content": content}),
            var_name: None,
        });
    };
    mk(&mut b, "01J9ZQ00000000000000XAB123", 1);
    mk(&mut b, "01J9ZQ00000000000000YAB123", 2);
    let fx = b.snapshot();
    let e = err(
        &fx,
        json!([{"op": "set_text", "ref": "AB123", "text": "x"}]),
    );
    assert_eq!(code(&e), "ambiguous_ref");
    let ok_ref = ok(
        &fx,
        json!([{"op": "set_text", "ref": "XAB123", "text": "x"}]),
    );
    assert_eq!(
        ok_ref.ops[0].target_node_id().unwrap().0,
        "01J9ZQ00000000000000XAB123"
    );
}

#[test]
fn a_failing_op_fails_the_whole_call_and_names_the_op() {
    let d = doc();
    let (a, b) = (rf(&d.base, &d.ids["purpose"]), rf(&d.base, &d.ids["long"]));
    let e = err(
        &d.base,
        json!([
            {"op": "set_text", "ref": a, "text": "first is fine", "id": "one"},
            {"op": "replace", "ref": b, "find": "no-such-text", "with": "x", "id": "two"},
            {"op": "delete", "ref": a, "id": "three"},
        ]),
    );
    assert_eq!(
        (e.op, e.id.as_deref(), code(&e)),
        (Some(1), Some("two"), "no_match")
    );
    assert!(
        e.to_dct().contains("code=no_match op=1 id=two"),
        "{}",
        e.to_dct()
    );
    // A later op sees the earlier one: replacing text the first op wrote works.
    let c = ok(
        &d.base,
        json!([
            {"op": "set_text", "ref": a, "text": "A brand new sentence."},
            {"op": "replace", "ref": a, "find": "brand new", "with": "different"},
        ]),
    );
    assert_eq!(text(&c.after, &d.ids["purpose"]), "A different sentence.");
    assert_eq!(
        c.results.iter().map(|r| r.index).collect::<Vec<_>>(),
        [0, 1]
    );
}

#[test]
fn only_the_author_role_compiles() {
    let d = doc();
    let ops =
        parse_ops(&json!([{"op": "set_text", "ref": rf(&d.base, &d.ids["purpose"]), "text": "x"}]))
            .unwrap();
    for role in [Role::Reviewer, Role::Auditor] {
        let input = CompileInput::new(&d.base.meta, &d.base.state, role);
        let e = compile(&input, &ops, &mut ids()).unwrap_err();
        assert_eq!(e.error, AgentError::Forbidden, "{role:?}");
    }
}

#[test]
fn withheld_blocks_are_redacted_errors_and_not_searched() {
    let d = doc();
    let p = &d.ids["purpose"];
    let e = run_with(
        &d.base,
        None,
        false,
        &[p],
        json!([{"op": "set_text", "ref": rf(&d.base, p), "text": "x"}]),
    )
    .unwrap_err();
    assert!(matches!(e.error, AgentError::RedactedBlock { .. }));
    // replace_all skips them: the count and the result leave the block alone.
    let total: usize = blocks(&d.base.state)
        .iter()
        .filter(|(id, _)| id != p)
        .map(|(_, t)| t.matches("policy").count())
        .sum();
    let c = run_with(
        &d.base,
        None,
        false,
        &[p],
        json!([{"op": "replace_all", "find": "policy", "with": "rule", "expect": total}]),
    )
    .unwrap();
    assert_eq!(text(&c.after, p), text(&d.base.state, p));
    assert!(c
        .ops
        .iter()
        .filter_map(EventPayload::target_node_id)
        .all(|id| id != p));
}

// ---------------------------------------------------------------- staleness

#[test]
fn a_stale_base_is_recorded_or_rejected() {
    let d = doc();
    let p = &d.ids["purpose"];
    let r = rf(&d.base, p);
    // The agent read the base revision; the team has moved on to `edited`.
    let old = block_hash(&content(&d.base.state, p));
    let now = block_hash(&content(&d.edited.state, p));
    assert_ne!(old, now);
    let ops =
        json!([{"op": "replace", "ref": r, "find": "policy", "with": "rule", "base": old.h12()}]);
    // Warning mode: compiled against the state given, recorded as stale with the new hash.
    let c = run_with(&d.base, Some(&d.edited.state), false, &[], ops.clone()).unwrap();
    assert_eq!(c.stale.len(), 1);
    assert_eq!(
        (
            c.stale[0].op,
            c.stale[0].at.as_str(),
            c.stale[0].current.clone()
        ),
        (0, r.as_str(), Some(now.clone()))
    );
    check(&d.base, &c);
    // Strict mode rejects the call and gives the current hash.
    let e = run_with(&d.base, Some(&d.edited.state), true, &[], ops.clone()).unwrap_err();
    assert_eq!(
        e.error,
        AgentError::StaleBase {
            target: Ref::parse(&r).unwrap(),
            current: engine_agent::hash::ShortHash::parse(now.h6()).unwrap()
        }
    );
    // Current base: nothing stale, in either mode. A 6-character hash is accepted.
    let fresh =
        json!([{"op": "replace", "ref": r, "find": "policy", "with": "rule", "base": old.h6()}]);
    assert!(run_with(&d.base, None, true, &[], fresh.clone())
        .unwrap()
        .stale
        .is_empty());
    assert!(run_with(&d.base, Some(&d.base.state), true, &[], fresh)
        .unwrap()
        .stale
        .is_empty());
    // A block the team deleted: stale in warning mode, a conflict in strict mode.
    let gone = &d.ids["quote"];
    assert!(d.edited.state.nodes[gone].deleted);
    let ops = json!([{"op": "delete", "ref": rf(&d.base, gone), "base": block_hash(&content(&d.base.state, gone)).h12()}]);
    let c = run_with(&d.base, Some(&d.edited.state), false, &[], ops.clone()).unwrap();
    assert_eq!(c.stale[0].current, None);
    let e = run_with(&d.base, Some(&d.edited.state), true, &[], ops).unwrap_err();
    assert!(matches!(e.error, AgentError::Conflict { .. }));
}

// ---------------------------------------------------------------- ids and determinism

#[test]
fn derived_ids_make_a_dry_run_and_the_real_call_agree() {
    let d = doc();
    let anchor = rf(&d.base, &d.ids["purpose"]);
    let ops = parse_ops(&json!([{"op": "insert", "after": anchor, "blocks": [{"t": "p", "text": "a"}, {"t": "p", "text": "b"}]}])).unwrap();
    let input = CompileInput::new(&d.base.meta, &d.base.state, Role::Author);
    let a = compile(
        &input,
        &ops,
        &mut DerivedIds::new(1_790_000_000_000, "5c1d0f4e-0000-4000-8000-000000000002"),
    )
    .unwrap();
    let b = compile(
        &input,
        &ops,
        &mut DerivedIds::new(1_790_000_000_000, "5c1d0f4e-0000-4000-8000-000000000002"),
    )
    .unwrap();
    let c = compile(
        &input,
        &ops,
        &mut DerivedIds::new(1_790_000_000_000, "another request"),
    )
    .unwrap();
    assert_eq!(a, b);
    assert_ne!(a.ops, c.ops);
    let created: Vec<&NodeId> = a
        .ops
        .iter()
        .filter_map(EventPayload::target_node_id)
        .collect();
    assert_eq!(created.len(), 2);
    assert_ne!(created[0], created[1]);
    for id in created {
        assert_eq!(id.0.len(), 26);
        assert!(id
            .0
            .bytes()
            .all(|b| b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".contains(&b)));
        assert!(id.0.as_bytes()[0] <= b'7');
        assert!(id.0.starts_with("01"), "{}", id.0);
    }
    // A source that hands out something that is not a ULID, or an id in use, is refused.
    let mut bad_ids = || NodeId("short".into());
    assert_eq!(
        compile(&input, &ops, &mut bad_ids).unwrap_err().error,
        AgentError::Internal
    );
    let taken = d.ids["title"].clone();
    let mut reuse = move || taken.clone();
    assert_eq!(
        compile(&input, &ops, &mut reuse).unwrap_err().error,
        AgentError::Internal
    );
}

#[test]
fn the_engine_op_budget_is_enforced() {
    // 5,000 short blocks: one replace_all rewrites every one of them with two engine ops each
    // (the whole field and its label), which is exactly the cap of 10,000.
    let mut b = fixtures::Builder::new(engine_agent::model::FileKind::Document, "cap", 0xca9);
    let root = b.node(
        engine_shared::NodeType::Form,
        None,
        0,
        json!({"label": "Cap"}),
    );
    let content = fixtures::para("aaa");
    for i in 0..5000u64 {
        b.node(
            engine_shared::NodeType::Item,
            Some(&root),
            (i + 1) * 10,
            json!({"label": "aaa", "content": content}),
        );
    }
    let fx = b.snapshot();
    let one = json!({"op": "replace_all", "find": "aaa", "with": "bbb", "expect": 5000});
    let c = ok(&fx, json!([one]));
    assert_eq!(c.ops.len(), MAX_ENGINE_OPS);
    assert_applies(&fx, &c);
    // A second sweep needs 10,000 more.
    let two = json!({"op": "replace_all", "find": "bbb", "with": "ccc", "expect": 5000});
    let e = err(&fx, json!([one, two]));
    assert_eq!(e.op, Some(1));
    assert!(
        matches!(
            e.error,
            AgentError::LimitExceeded {
                field: "engine_ops",
                limit: 10_000,
                ..
            }
        ),
        "{e}"
    );
    // More than 5,000 affected blocks is refused before any work.
    b = fixtures::Builder::new(engine_agent::model::FileKind::Document, "cap2", 0xca8);
    let root = b.node(
        engine_shared::NodeType::Form,
        None,
        0,
        json!({"label": "Cap"}),
    );
    for i in 0..5001u64 {
        b.node(
            engine_shared::NodeType::Item,
            Some(&root),
            (i + 1) * 10,
            json!({"label": "aaa", "content": content}),
        );
    }
    let big = b.snapshot();
    let e = err(
        &big,
        json!([{"op": "replace_all", "find": "aaa", "with": "bbb", "expect": 5001}]),
    );
    assert!(
        matches!(
            e.error,
            AgentError::LimitExceeded {
                field: "blocks",
                limit: 5000,
                value: 5001
            }
        ),
        "{e}"
    );
}

#[test]
fn many_inserts_at_one_spot_end_in_a_refusal_not_a_misordered_key() {
    // Always inserting before the first block squeezes every key below the last one; after
    // about a thousand the key has used up its 200 characters and the call fails whole.
    let d = doc();
    let first = rf(&d.base, &blocks(&d.base.state)[0].0);
    let fifty: Vec<Value> = (0..50)
        .map(|i| json!({"t": "p", "text": format!("b{i}")}))
        .collect();
    let mut ops = Vec::new();
    for _ in 0..30 {
        ops.push(json!({"op": "insert", "before": first, "blocks": fifty}));
    }
    let e = err(&d.base, Value::Array(ops));
    assert!(
        matches!(
            e.error,
            AgentError::LimitExceeded {
                field: "pos",
                limit: 200,
                ..
            }
        ),
        "{e}"
    );
    assert!(e.op.unwrap() >= 16, "refused at op {:?}", e.op);
    // Fewer inserts are fine and ordered.
    let ok_ops: Vec<Value> = (0..10)
        .map(|_| json!({"op": "insert", "before": first, "blocks": fifty}))
        .collect();
    let c = ok(&d.base, Value::Array(ok_ops));
    assert_eq!(c.ops.len(), 500);
    let live_blocks = blocks(&c.after);
    assert_eq!(live_blocks.len(), blocks(&d.base.state).len() + 500);
    assert_applies(&d.base, &c);
}

// ---------------------------------------------------------------- spreadsheets

fn sheet_text(state: &DocumentState, sheet: &NodeId) -> Vec<Vec<String>> {
    let mut rows: Vec<_> = state
        .nodes
        .values()
        .filter(|n| !n.deleted && n.parent_id.as_ref() == Some(sheet))
        .collect();
    rows.sort_by(|a, b| (&a.pos, &a.id).cmp(&(&b.pos, &b.id)));
    rows.iter()
        .map(|r| {
            (0..12)
                .map(|c| engine_agent::model::field_string(&r.current_fields[format!("cell_{c}")]))
                .collect()
        })
        .collect()
}

#[test]
fn set_cells_writes_changed_cells_and_creates_rows() {
    let w = fixtures::workbook();
    let sheet = &w.ids["budget"];
    let rows = sheet_text(&w.base.state, sheet);
    let n = rows.len();
    let c = ok(
        &w.base,
        json!([{"op": "set_cells", "sheet": "Budget", "at": "E3", "values": [["111", "222"], ["333", rows[3][5].clone()]]}]),
    );
    check(&w.base, &c);
    let after = sheet_text(&c.after, sheet);
    assert_eq!(&after[2][4..6], ["111", "222"]);
    assert_eq!(after[3][4], "333");
    // The unchanged value produced no op: three cells changed.
    assert_eq!(c.ops.len(), 3);
    assert!(c.ops.iter().all(
        |o| matches!(o, EventPayload::FieldEdited { field, .. } if field.starts_with("cell_"))
    ));
    // Rows beyond the end are created below the last one, with their cells.
    let c = ok(
        &w.base,
        json!([{"op": "set_cells", "sheet": "Budget", "at": format!("A{}", n + 1), "values": [["North", "Omega"], ["", ""], ["South", "Psi", "x", "y", "z", "w", "v", "u", "t", "s", "r", "q", "p"]]}]),
    );
    check(&w.base, &c);
    let after = sheet_text(&c.after, sheet);
    assert_eq!(after.len(), n + 3);
    assert_eq!(&after[n][..2], ["North", "Omega"]);
    assert_eq!(&after[n + 2][..2], ["South", "Psi"]);
    // Row positions follow the last row; the 13th column grew the sheet to 13 columns.
    let created: Vec<&NodeId> = c.results[0]
        .changes
        .iter()
        .filter(|ch| ch.kind == ChangeKind::Created)
        .map(|_| sheet)
        .collect();
    assert_eq!(created.len(), 3);
    assert!(c.ops.iter().any(|o| matches!(o, EventPayload::FieldEdited { field, value, .. } if field == "columns" && *value == json!(13))));
    assert_eq!(c.after.nodes[sheet].current_fields["columns"], json!(13));
    // Setting what is already there is no change.
    let same = rows[0][..2].to_vec();
    assert_eq!(
        err(
            &w.base,
            json!([{"op": "set_cells", "sheet": "Budget", "at": "A1", "values": [same]}])
        )
        .error,
        AgentError::NoChange
    );
}

#[test]
fn set_cells_by_row_ref_and_sheet_ref() {
    let w = fixtures::workbook();
    let row = &w.ids["row8"];
    let r = rf(&w.base, row);
    let sref = rf(&w.base, &w.ids["budget"]);
    let c = ok(
        &w.base,
        json!([{"op": "set_cells", "sheet": sref, "at": format!("{r}:C"), "values": [["Mei", "2027-01-01"]]}]),
    );
    check(&w.base, &c);
    assert_eq!(c.after.nodes[row].current_fields["cell_2"], json!("Mei"));
    assert_eq!(
        c.after.nodes[row].current_fields["cell_3"],
        json!("2027-01-01")
    );
    // The row ref belongs to the row, wherever it sits now.
    assert_eq!(
        code(&err(
            &w.base,
            json!([{"op": "set_cells", "sheet": "Budget", "at": "ZZZZ:A", "values": [["x"]]}])
        )),
        "unknown_ref"
    );
    assert_eq!(
        code(&err(
            &w.base,
            json!([{"op": "set_cells", "sheet": "No such sheet", "at": "A1", "values": [["x"]]}])
        )),
        "bad_request"
    );
    // A name that is also spelled like a ref is tried as a ref.
    assert_eq!(
        code(&err(
            &w.base,
            json!([{"op": "set_cells", "sheet": "Missing", "at": "A1", "values": [["x"]]}])
        )),
        "unknown_ref"
    );
}

#[test]
fn set_cells_limits_and_formulas() {
    let w = fixtures::workbook();
    let e = err(
        &w.base,
        json!([{"op": "set_cells", "sheet": "Budget", "at": "CV1", "values": [["a", "b"]]}]),
    );
    assert!(
        matches!(
            e.error,
            AgentError::LimitExceeded {
                field: "at",
                limit: 100,
                value: 101
            }
        ),
        "{e}"
    );
    let e = err(
        &w.base,
        json!([{"op": "set_cells", "sheet": "Budget", "at": "A2999", "values": [["a"], ["b"], ["c"]]}]),
    );
    assert!(
        matches!(
            e.error,
            AgentError::LimitExceeded {
                field: "at",
                limit: 3000,
                value: 3001
            }
        ),
        "{e}"
    );
    // A formula is stored as written.
    let c = ok(
        &w.base,
        json!([{"op": "set_cells", "sheet": "Budget", "at": "H2", "values": [["=SUM(E2:F2)+1"]]}]),
    );
    assert_eq!(
        c.after.nodes[&fixtures_row(&w, 1)].current_fields["cell_7"],
        json!("=SUM(E2:F2)+1")
    );
}

fn fixtures_row(w: &fixtures::WorkbookFixture, i: usize) -> NodeId {
    let mut rows: Vec<_> = w
        .base
        .state
        .nodes
        .values()
        .filter(|n| n.parent_id.as_ref() == Some(&w.ids["budget"]))
        .collect();
    rows.sort_by(|a, b| (&a.pos, &a.id).cmp(&(&b.pos, &b.id)));
    rows[i].id.clone()
}

#[test]
fn clear_empties_cells_and_keeps_formats() {
    let w = fixtures::workbook();
    let sheet = &w.ids["budget"];
    let before = sheet_text(&w.base.state, sheet);
    let c = ok(
        &w.base,
        json!([{"op": "clear", "sheet": "Budget", "range": "A2:C4"}]),
    );
    check(&w.base, &c);
    let after = sheet_text(&c.after, sheet);
    for r in 1..4 {
        assert_eq!(&after[r][..3], ["", "", ""]);
        assert_eq!(after[r][3..], before[r][3..]);
    }
    assert_eq!(after[0], before[0]);
    assert_eq!(c.ops.len(), 9);
    assert!(c
        .ops
        .iter()
        .all(|o| matches!(o, EventPayload::FieldEdited { value, .. } if *value == json!(""))));
    // Qualified range, and nothing to clear.
    let c2 = ok(&w.base, json!([{"op": "clear", "range": "Budget!A2:C4"}]));
    assert_eq!(c2.after, c.after);
    assert_eq!(
        err(
            &w.base,
            json!([{"op": "clear", "sheet": "Budget", "range": "Z500:Z600"}])
        )
        .error,
        AgentError::NoChange
    );
    assert_eq!(
        err(
            &w.base,
            json!([{"op": "clear", "sheet": "Budget", "range": "Other!A1"}])
        )
        .error,
        AgentError::BadRequest { field: "range" }
    );
}

#[test]
fn a_protected_sheet_cannot_be_edited() {
    let w = fixtures::workbook();
    let sheet = w.ids["budget"].clone();
    let mut state = w.base.state.clone();
    apply_payload(
        &mut state,
        &EventPayload::FieldEdited {
            node_id: sheet,
            field: "sheet_protected".into(),
            value: json!(true),
        },
    )
    .unwrap();
    let fx = Fixture {
        meta: w.base.meta.clone(),
        state,
    };
    for op in [
        json!({"op": "set_cells", "sheet": "Budget", "at": "A1", "values": [["x"]]}),
        json!({"op": "set_cells", "sheet": "Budget", "at": "A99", "values": [["x"]]}),
        json!({"op": "clear", "sheet": "Budget", "range": "A1:B2"}),
    ] {
        assert_eq!(err(&fx, json!([op])).error, AgentError::Protected);
    }
    // The other sheet is open.
    assert!(run(
        &fx,
        json!([{"op": "set_cells", "sheet": "Notes & 'Ideas'", "at": "A1", "values": [["x"]]}])
    )
    .is_ok());
}

#[test]
fn a_changed_range_is_stale() {
    let w = fixtures::workbook();
    let sheet = &w.ids["budget"];
    // The agent read E6:F6 at the base revision; row 5 (0-based) changed at the head.
    let rect = |fx: &Fixture| {
        let rows = sheet_text(&fx.state, sheet);
        vec![rows[5][4..6].to_vec()]
    };
    let old = range_hash(&rect(&w.base));
    assert_ne!(old, range_hash(&rect(&w.edited)));
    let ops = json!([{"op": "set_cells", "sheet": "Budget", "at": "E6", "values": [["1", "2"]], "base": old.h12()}]);
    let c = run_with(&w.base, Some(&w.edited.state), false, &[], ops.clone()).unwrap();
    assert_eq!(c.stale.len(), 1);
    assert!(c.stale[0].at.ends_with("!E6:F6"), "{}", c.stale[0].at);
    assert!(matches!(
        run_with(&w.base, Some(&w.edited.state), true, &[], ops)
            .unwrap_err()
            .error,
        AgentError::StaleBase { .. }
    ));
    let fresh = json!([{"op": "set_cells", "sheet": "Budget", "at": "E6", "values": [["1", "2"]], "base": old.h12()}]);
    assert!(run_with(&w.base, None, true, &[], fresh)
        .unwrap()
        .stale
        .is_empty());
}

#[test]
fn sheet_ops_need_a_spreadsheet() {
    let d = doc();
    let e = err(
        &d.base,
        json!([{"op": "set_cells", "sheet": "Budget", "at": "A1", "values": [["x"]]}]),
    );
    assert_eq!(e.error, AgentError::UnsupportedStructure);
    let w = fixtures::workbook();
    assert_eq!(
        err(
            &w.base,
            json!([{"op": "set_text", "ref": "AAAA", "text": "x"}])
        )
        .error,
        AgentError::UnsupportedStructure
    );
}

// ---------------------------------------------------------------- presentations

#[test]
fn set_shape_and_notes() {
    let k = fixtures::deck();
    let (s2, body) = (&k.ids["s2"], &k.ids["s2_body"]);
    let c = ok(
        &k.base,
        json!([
            {"op": "set_shape", "slide": "s2", "shape": rf(&k.base, body), "text": "Revenue up 5 percent\nThree contracts renewed"},
            {"op": "set_notes", "slide": rf(&k.base, s2), "text": "Lead with the audit."},
        ]),
    );
    check(&k.base, &c);
    assert_eq!(
        c.after.nodes[body].current_fields["text"],
        json!("Revenue up 5 percent\nThree contracts renewed")
    );
    assert_eq!(
        c.after.nodes[body].current_fields["label"],
        json!("Revenue up 5 percent Three contracts renewed")
    );
    assert_eq!(
        c.after.nodes[s2].current_fields["notes"],
        json!("Lead with the audit.")
    );
    // Geometry and style are untouched.
    assert_eq!(
        c.after.nodes[body].current_fields["x"],
        k.base.state.nodes[body].current_fields["x"]
    );
    assert_eq!(
        c.results[0].changes[0].new,
        Some(shape_hash(
            "text",
            Some("body"),
            "Revenue up 5 percent\nThree contracts renewed"
        ))
    );
    // Editing a title shape renames the slide.
    let title = k
        .base
        .state
        .nodes
        .values()
        .find(|n| n.parent_id.as_ref() == Some(s2) && n.current_fields["role"] == "title")
        .unwrap();
    let c = ok(
        &k.base,
        json!([{"op": "set_shape", "slide": "s2", "shape": rf(&k.base, &title.id), "text": "Key  results"}]),
    );
    assert_eq!(
        c.after.nodes[s2].current_fields["label"],
        json!("Key results")
    );
    assert_eq!(
        c.after.nodes[&title.id].current_fields["text"],
        json!("Key  results")
    );
    // No change, wrong slide, image shapes.
    assert_eq!(
        err(
            &k.base,
            json!([{"op": "set_notes", "slide": "s2", "text": "Mention the audit finding first."}])
        )
        .error,
        AgentError::NoChange
    );
    let s3_title = k
        .base
        .state
        .nodes
        .values()
        .find(|n| n.parent_id.as_ref() == Some(&k.ids["s3"]) && n.current_fields["role"] == "title")
        .unwrap();
    assert_eq!(
        code(&err(
            &k.base,
            json!([{"op": "set_shape", "slide": "s2", "shape": rf(&k.base, &s3_title.id), "text": "x"}])
        )),
        "unknown_ref"
    );
    let image = k
        .base
        .state
        .nodes
        .values()
        .find(|n| n.current_fields["kind"] == "image")
        .unwrap();
    let slide = k
        .base
        .state
        .nodes
        .values()
        .find(|n| n.id == *image.parent_id.as_ref().unwrap())
        .unwrap();
    assert_eq!(
        err(&k.base, json!([{"op": "set_shape", "slide": rf(&k.base, &slide.id), "shape": rf(&k.base, &image.id), "text": "x"}])).error,
        AgentError::UnsupportedStructure
    );
    assert_eq!(
        code(&err(
            &k.base,
            json!([{"op": "set_notes", "slide": "s99", "text": "x"}])
        )),
        "unknown_ref"
    );
}

#[test]
fn a_changed_shape_and_changed_notes_are_stale() {
    let k = fixtures::deck();
    let body = &k.ids["s2_body"];
    let old = shape_hash(
        "text",
        Some("body"),
        "Revenue up 4 percent\nTwo contracts renewed",
    );
    let mut head = k.base.state.clone();
    apply_payload(
        &mut head,
        &EventPayload::FieldEdited {
            node_id: body.clone(),
            field: "text".into(),
            value: json!("Changed at the head"),
        },
    )
    .unwrap();
    let ops = json!([{"op": "set_shape", "slide": "s2", "shape": rf(&k.base, body), "text": "mine", "base": old.h12()}]);
    assert_eq!(
        run_with(&k.base, Some(&head), false, &[], ops.clone())
            .unwrap()
            .stale
            .len(),
        1
    );
    assert!(matches!(
        run_with(&k.base, Some(&head), true, &[], ops)
            .unwrap_err()
            .error,
        AgentError::StaleBase { .. }
    ));
    let notes = engine_agent::hash::notes_hash("Mention the audit finding first.");
    let ops = json!([{"op": "set_notes", "slide": "s2", "text": "mine", "base": notes.h12()}]);
    assert!(run_with(&k.base, None, true, &[], ops)
        .unwrap()
        .stale
        .is_empty());
}
