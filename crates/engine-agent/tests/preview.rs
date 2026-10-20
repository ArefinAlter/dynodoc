//! The preview of a compiled call (protocol spec 7.7, 8.7): the DCT response, the per-op
//! results, the dry-run header, and the byte bound.

use engine_agent::budget::{MAX_MAX_BYTES, MIN_MAX_BYTES};
use engine_agent::compile::{compile, CompileInput, Compiled, IdSource};
use engine_agent::dct::{parse_header, split_response};
use engine_agent::diff::Detail;
use engine_agent::error::AgentError;
use engine_agent::fixtures::{self, Fixture};
use engine_agent::grammar::parse_ops;
use engine_agent::model::FileKind;
use engine_agent::parse;
use engine_agent::preview::{preview, PreviewOptions};
use engine_agent::refs::RefIndex;
use engine_core::governance::Role;
use engine_core::richtext::plain_text;
use engine_shared::NodeType;
use serde_json::{json, Value};
use uuid::Uuid;

fn ids() -> impl IdSource {
    let mut n = 0u64;
    move || {
        n += 1;
        fixtures::ulid(8_000_000 + n)
    }
}

fn build(fx: &Fixture, ops: Value) -> Compiled {
    let ops = parse_ops(&ops).unwrap();
    compile(
        &CompileInput::new(&fx.meta, &fx.state, Role::Author),
        &ops,
        &mut ids(),
    )
    .unwrap_or_else(|e| panic!("{e}"))
}

fn rf(fx: &Fixture, id: &engine_shared::NodeId) -> String {
    RefIndex::from_state(&fx.state)
        .ref_of(id)
        .unwrap()
        .to_string()
}

#[test]
fn the_preview_reads_like_the_spec_example() {
    let d = fixtures::document();
    let (p, anchor) = (&d.ids["purpose"], &d.ids["long"]);
    let c = build(
        &d.base,
        json!([
            {"op": "replace", "ref": rf(&d.base, p), "find": "policy", "with": "procedure"},
            {"op": "insert", "after": rf(&d.base, anchor), "blocks": [{"t": "p", "text": "A new paragraph."}]},
            {"op": "delete", "ref": rf(&d.base, &d.ids["scope_h"])},
        ]),
    );
    let opts = PreviewOptions {
        proposal: Some(Uuid::from_u128(0x4d0e_9b1c_0000_4000_8000_0000_0000_0001)),
        state: Some("draft"),
        revision: Some(2),
        ..PreviewOptions::default()
    };
    let r = preview(&d.base.meta, &d.base.state, &c, &opts).unwrap();
    let (h, lines) = split_response(&r.text).unwrap();
    assert_eq!(h.get("file"), Some("7034d450"));
    assert_eq!(h.get("proposal"), Some("4d0e9b1c"));
    assert_eq!(h.get("rev"), Some(d.base.meta.rev.to_string().as_str()));
    assert_eq!(h.get("applied"), Some("3"));
    assert_eq!(h.get("expanded"), Some(c.ops.len().to_string().as_str()));
    assert_eq!(h.get("stale"), Some("0"));
    assert_eq!(
        (h.get("state"), h.get("revision")),
        (Some("draft"), Some("2"))
    );
    assert_eq!(h.get("trust"), Some("data"));
    assert!(h.get("warn").is_none() && h.get("would_apply").is_none());
    // The body is the strict diff grammar: one changed, one added, one removed block.
    let entries = parse::diff_lines(&lines).unwrap();
    let ops: Vec<char> = entries.iter().map(|e| e.op).collect();
    assert_eq!(ops, ['~', '+', '-']);
    assert_eq!(entries[0].r, rf(&d.base, p));
    assert_eq!(entries[0].chars.map(|(d, i)| d > 0 && i > 0), Some(true));
    assert_eq!(
        entries[1].after.as_deref(),
        Some(rf(&d.base, anchor).as_str())
    );
    assert!(entries[1]
        .preview
        .as_deref()
        .unwrap()
        .starts_with("A new paragraph."));
    // Typed data: per-op results with new refs and the hashes before and after.
    let ds = &r.data;
    assert_eq!(
        (ds.applied, ds.would_apply, ds.expanded, ds.stale.len()),
        (3, 3, c.ops.len(), 0)
    );
    assert_eq!(ds.entries_total, 3);
    assert_eq!(
        ds.ops.iter().map(|o| o.op).collect::<Vec<_>>(),
        ["replace", "insert", "delete"]
    );
    assert_eq!(ds.ops[0].changes[0].kind, "changed");
    assert_ne!(ds.ops[0].changes[0].old, ds.ops[0].changes[0].new);
    assert_eq!(ds.ops[1].changes[0].kind, "created");
    assert_eq!(ds.ops[1].changes[0].old, None);
    // The ref of the new block is the one the text shows, and meta maps it to the node id.
    assert_eq!(ds.ops[1].changes[0].r, entries[1].r);
    assert!(r.meta.ids.contains_key(&entries[1].r));
    assert_eq!(r.meta.budget.bytes, r.text.len());
    // The JSON mirror serialises.
    let mirror: Value =
        serde_json::from_str(&r.json_mirror(engine_agent::dct::Tool::Propose)).unwrap();
    assert_eq!(mirror["tool"], "propose");
    assert_eq!(mirror["expanded"], json!(c.ops.len()));
}

#[test]
fn a_dry_run_says_applied_zero_and_would_apply() {
    let d = fixtures::document();
    let c = build(
        &d.base,
        json!([{"op": "set_type", "ref": rf(&d.base, &d.ids["purpose"]), "t": "h2"}]),
    );
    let opts = PreviewOptions {
        dry_run: true,
        ..PreviewOptions::default()
    };
    let r = preview(&d.base.meta, &d.base.state, &c, &opts).unwrap();
    let (h, _) = split_response(&r.text).unwrap();
    assert_eq!(
        (h.get("applied"), h.get("would_apply")),
        (Some("0"), Some("1"))
    );
    assert!(h.get("state").is_none() && h.get("proposal").is_none());
    assert_eq!((r.data.applied, r.data.would_apply), (0, 1));
    // detail=text adds the word-level changes of a text edit.
    let c = build(
        &d.base,
        json!([{"op": "replace", "ref": rf(&d.base, &d.ids["purpose"]), "find": "policy", "with": "procedure"}]),
    );
    let opts = PreviewOptions {
        dry_run: true,
        detail: Detail::Text,
        ..PreviewOptions::default()
    };
    let r = preview(&d.base.meta, &d.base.state, &c, &opts).unwrap();
    assert!(r.text.contains("[-policy-]{+procedure+}"), "{}", r.text);
}

#[test]
fn stale_targets_are_counted_and_named() {
    let d = fixtures::document();
    let p = &d.ids["purpose"];
    let ops = parse_ops(&json!([{"op": "replace", "ref": rf(&d.base, p), "find": "policy", "with": "rule", "base": "000000"}])).unwrap();
    let mut input = CompileInput::new(&d.base.meta, &d.base.state, Role::Author);
    input.head = Some(&d.edited.state);
    let c = compile(&input, &ops, &mut ids()).unwrap();
    let r = preview(&d.base.meta, &d.base.state, &c, &PreviewOptions::default()).unwrap();
    let (h, _) = split_response(&r.text).unwrap();
    assert_eq!(h.get("stale"), Some("1"));
    assert_eq!(h.get("stale_refs"), Some(rf(&d.base, p).as_str()));
    assert!(h.get("warn").unwrap().contains("stale"));
    assert_eq!(r.data.stale.len(), 1);
    assert!(r.data.stale[0].current.is_some());
}

/// 80 blocks that all contain the word, as in the token-efficiency fixture.
fn eighty() -> Fixture {
    let mut b = fixtures::Builder::new(FileKind::Document, "eighty", 0xe80);
    let root = b.node(NodeType::Form, None, 0, json!({"label": "Eighty"}));
    for i in 0..80u64 {
        let content = fixtures::para(&format!(
            "Paragraph {i}: {} the TERMX applies.",
            fixtures::words(i, 12)
        ));
        b.node(
            NodeType::Item,
            Some(&root),
            (i + 1) * 10,
            json!({"label": plain_text(&content), "content": content}),
        );
    }
    b.snapshot()
}

#[test]
fn a_preview_never_exceeds_max_bytes() {
    let fx = eighty();
    let c = build(
        &fx,
        json!([{"op": "replace_all", "find": "TERMX", "with": "procedure", "expect": 80}]),
    );
    assert_eq!(c.results[0].changes.len(), 80);
    let full = preview(
        &fx.meta,
        &fx.state,
        &c,
        &PreviewOptions {
            max_bytes: Some(MAX_MAX_BYTES),
            ..PreviewOptions::default()
        },
    )
    .unwrap();
    assert!(!full.data.truncated);
    assert_eq!(full.data.entries.len(), 80);
    let mut last_shown = 0;
    for max in [
        MIN_MAX_BYTES,
        520,
        700,
        1_000,
        2_000,
        4_096,
        8_000,
        20_000,
        MAX_MAX_BYTES,
    ] {
        for detail in [Detail::Summary, Detail::Text] {
            let opts = PreviewOptions {
                max_bytes: Some(max),
                detail,
                ..PreviewOptions::default()
            };
            let r = preview(&fx.meta, &fx.state, &c, &opts).unwrap();
            assert!(r.text.len() <= max, "{} > {max}", r.text.len());
            assert_eq!(r.meta.budget.bytes, r.text.len());
            let (h, lines) = split_response(&r.text).unwrap();
            let entries = parse::diff_lines(&lines).unwrap_or_else(|e| panic!("{e}\n{}", r.text));
            // Whole entries only, a prefix of the full list, and `truncated` says so.
            assert_eq!(entries.len(), r.data.entries.len());
            let all = preview(
                &fx.meta,
                &fx.state,
                &c,
                &PreviewOptions {
                    max_bytes: Some(MAX_MAX_BYTES),
                    detail,
                    ..PreviewOptions::default()
                },
            )
            .unwrap();
            assert_eq!(r.data.entries, all.data.entries[..entries.len()]);
            assert_eq!(r.data.truncated, entries.len() < all.data.entries.len());
            assert_eq!(
                h.get("warn").is_some_and(|w| w.contains("truncated")),
                r.data.truncated
            );
            assert_eq!(r.data.entries_total, 80);
            // The counts of the call do not depend on the budget.
            assert_eq!(
                (h.get("applied"), h.get("expanded")),
                (Some("1"), Some(c.ops.len().to_string().as_str()))
            );
            if detail == Detail::Summary {
                assert!(
                    entries.len() >= last_shown,
                    "a larger budget shows at least as much"
                );
                last_shown = entries.len();
            }
        }
    }
    // The smallest budget still carries the header and says what was left out.
    let r = preview(
        &fx.meta,
        &fx.state,
        &c,
        &PreviewOptions {
            max_bytes: Some(MIN_MAX_BYTES),
            ..PreviewOptions::default()
        },
    )
    .unwrap();
    assert!(r.data.truncated && r.text.starts_with("dd/1 propose "));
    // Out-of-range budgets are refused like any other tool's.
    assert_eq!(
        preview(
            &fx.meta,
            &fx.state,
            &c,
            &PreviewOptions {
                max_bytes: Some(499),
                ..PreviewOptions::default()
            }
        )
        .unwrap_err(),
        AgentError::BadRequest { field: "max_bytes" }
    );
    assert!(matches!(
        preview(
            &fx.meta,
            &fx.state,
            &c,
            &PreviewOptions {
                max_bytes: Some(64_001),
                ..PreviewOptions::default()
            }
        )
        .unwrap_err(),
        AgentError::LimitExceeded {
            field: "max_bytes",
            ..
        }
    ));
}

#[test]
fn a_preview_is_far_smaller_than_the_state() {
    // The token argument of spec 8.7 / AI-AGENT-TOKEN-EFFICIENCY: the response to a one-line
    // edit is a few hundred bytes however large the file is.
    let fx = eighty();
    let c = build(
        &fx,
        json!([{"op": "replace", "ref": rf(&fx, &blocks_first(&fx)), "find": "TERMX", "with": "rule"}]),
    );
    let r = preview(&fx.meta, &fx.state, &c, &PreviewOptions::default()).unwrap();
    let state_bytes = serde_json::to_vec(&fx.state).unwrap().len();
    assert!(r.text.len() < 400, "{} bytes:\n{}", r.text.len(), r.text);
    assert!(r.text.len() * 50 < state_bytes);
}

fn blocks_first(fx: &Fixture) -> engine_shared::NodeId {
    let mut v: Vec<_> = fx
        .state
        .nodes
        .values()
        .filter(|n| n.current_fields["content"].is_object())
        .collect();
    v.sort_by(|a, b| (&a.pos, &a.id).cmp(&(&b.pos, &b.id)));
    v[0].id.clone()
}

#[test]
fn spreadsheet_and_deck_previews_parse() {
    let w = fixtures::workbook();
    let c = build(
        &w.base,
        json!([
            {"op": "set_cells", "sheet": "Budget", "at": "A2", "values": [["East", "Beta"]]},
            {"op": "clear", "sheet": "Budget", "range": "G3:G9"},
        ]),
    );
    let r = preview(&w.base.meta, &w.base.state, &c, &PreviewOptions::default()).unwrap();
    let (h, lines) = split_response(&r.text).unwrap();
    assert_eq!(h.get("applied"), Some("2"));
    let entries = parse::diff_lines(&lines).unwrap();
    assert!(
        !entries.is_empty() && entries.iter().all(|e| e.op == '~' && e.row.is_some()),
        "{}",
        r.text
    );
    let k = fixtures::deck();
    let c = build(
        &k.base,
        json!([{"op": "set_notes", "slide": "s2", "text": "New notes."}]),
    );
    let r = preview(&k.base.meta, &k.base.state, &c, &PreviewOptions::default()).unwrap();
    let (_, lines) = split_response(&r.text).unwrap();
    assert!(!parse::diff_lines(&lines).unwrap().is_empty());
    // The header always parses with the strict DCT grammar.
    assert!(parse_header(r.text.lines().next().unwrap()).is_ok());
}
