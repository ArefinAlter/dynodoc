//! Byte benchmark (docs/AI-AGENT-TOKEN-EFFICIENCY.md section 3, Phase -1): how many bytes
//! the Dynodoc Compact Text of each golden tool call takes next to the JSON mirror of the
//! same answer, and, for larger synthetic files, next to the raw state and a plain-text
//! baseline.
//!
//! ```text
//! cargo run -p engine-agent --example byte_benchmark -- --date 2026-10-18 --out docs/benchmarks/agent-dct-bytes-20261018.json
//! ```
//!
//! It counts UTF-8 bytes and estimates tokens; it never tokenises. Everything is
//! deterministic, so a rerun gives the same file; there is no clock (the date is an
//! argument) and no randomness.

use std::collections::BTreeMap;

use engine_agent::corpus::golden_calls;
use engine_agent::dct::Tool;
use engine_agent::diff::{diff, DiffOptions};
use engine_agent::find::{find, FindOptions};
use engine_agent::fixtures::{self, Fixture};
use engine_agent::outline::{outline, OutlineOptions};
use engine_agent::read::{read, ReadOptions};
use engine_agent::refs::RefIndex;
use engine_agent::response::Response;
use serde::Serialize;
use serde_json::{json, Value};

/// `ceil(bytes / 4)`: the `~tok` estimate of spec 6.2 and 11.
fn tok_bytes4(bytes: usize) -> usize {
    bytes.div_ceil(4)
}

/// `ceil(Unicode scalar values / 4)`.
fn tok_chars4(chars: usize) -> usize {
    chars.div_ceil(4)
}

fn ratio(a: usize, b: usize) -> f64 {
    ((a as f64 / b.max(1) as f64) * 1000.0).round() / 1000.0
}

#[derive(Serialize, Clone)]
struct Size {
    bytes: usize,
    chars: usize,
    est_tokens_bytes4: usize,
    est_tokens_chars4: usize,
}

fn size(s: &str) -> Size {
    let (bytes, chars) = (s.len(), s.chars().count());
    Size {
        bytes,
        chars,
        est_tokens_bytes4: tok_bytes4(bytes),
        est_tokens_chars4: tok_chars4(chars),
    }
}

#[derive(Serialize)]
struct Row {
    name: String,
    tool: &'static str,
    dct: Size,
    json_mirror: Size,
    /// JSON mirror bytes divided by DCT bytes.
    json_over_dct_bytes: f64,
}

fn row(name: &str, tool: Tool, dct: &str, mirror: &str) -> Row {
    let (d, j) = (size(dct), size(mirror));
    Row {
        name: name.into(),
        tool: tool.as_str(),
        json_over_dct_bytes: ratio(j.bytes, d.bytes),
        dct: d,
        json_mirror: j,
    }
}

fn totals(rows: &[&Row]) -> Value {
    let (d, j): (usize, usize) = rows.iter().fold((0, 0), |a, r| {
        (a.0 + r.dct.bytes, a.1 + r.json_mirror.bytes)
    });
    let (dc, jc): (usize, usize) = rows.iter().fold((0, 0), |a, r| {
        (a.0 + r.dct.chars, a.1 + r.json_mirror.chars)
    });
    json!({
        "calls": rows.len(),
        "dct_bytes": d,
        "json_mirror_bytes": j,
        "json_over_dct_bytes": ratio(j, d),
        "dct_est_tokens_bytes4": tok_bytes4(d),
        "json_mirror_est_tokens_bytes4": tok_bytes4(j),
        "dct_est_tokens_chars4": tok_chars4(dc),
        "json_mirror_est_tokens_chars4": tok_chars4(jc),
    })
}

fn golden() -> Value {
    let rows: Vec<Row> = golden_calls()
        .iter()
        .filter_map(|c| c.mirror.as_ref().map(|m| row(&c.name, c.tool, &c.text, m)))
        .collect();
    let mut by_tool: BTreeMap<&str, Vec<&Row>> = BTreeMap::new();
    for r in &rows {
        by_tool.entry(r.tool).or_default().push(r);
    }
    let by_tool: BTreeMap<&str, Value> = by_tool.iter().map(|(k, v)| (*k, totals(v))).collect();
    let all: Vec<&Row> = rows.iter().collect();
    json!({
        "note": "The calls of tests/golden/ (error lines excluded: they have no JSON mirror, the HTTP envelope differs). Small files: 28 to 32 blocks, 41 rows, 5 slides.",
        "totals": totals(&all),
        "by_tool": by_tool,
        "calls": rows,
    })
}

// ---------------------------------------------------------------- scaled files

fn mirror_row<T: Serialize>(name: &str, tool: Tool, r: &Response<T>) -> Row {
    row(name, tool, &r.text, &r.json_mirror(tool))
}

fn csv(rows: &[Vec<String>]) -> String {
    let field = |f: &str| {
        if f.contains([',', '"', '\n']) {
            format!("\"{}\"", f.replace('"', "\"\""))
        } else {
            f.to_string()
        }
    };
    rows.iter()
        .map(|r| r.iter().map(|f| field(f)).collect::<Vec<_>>().join(","))
        .collect::<Vec<_>>()
        .join("\n")
}

fn reference(name: &str, what: &str, text: &str) -> Value {
    json!({"name": name, "what": what, "size": size(text)})
}

fn scaled_document() -> Value {
    let fx: Fixture = fixtures::scaled_document(200);
    let idx = RefIndex::from_state(&fx.state);
    let blocks: Vec<_> = fx
        .state
        .nodes
        .values()
        .filter(|n| n.current_fields["content"].is_object())
        .collect();
    let words: usize = blocks
        .iter()
        .map(|n| {
            n.current_fields["label"]
                .as_str()
                .unwrap_or("")
                .split_whitespace()
                .count()
        })
        .sum();
    let eight: Vec<String> = blocks
        .iter()
        .skip(40)
        .take(8)
        .map(|n| idx.ref_of(&n.id).expect("ref").to_string())
        .collect();
    let o1 = outline(&fx.meta, &fx.state, &OutlineOptions::default()).expect("outline");
    let o0 = outline(
        &fx.meta,
        &fx.state,
        &OutlineOptions {
            depth: Some(0),
            ..Default::default()
        },
    )
    .expect("outline");
    let r8 = read(
        &fx.meta,
        &fx.state,
        &ReadOptions {
            refs: eight.clone(),
            ..Default::default()
        },
    )
    .expect("read");
    let have = r8
        .data
        .items
        .iter()
        .map(|i| (i.r.clone(), i.h.clone().expect("hash")))
        .collect();
    let again = read(
        &fx.meta,
        &fx.state,
        &ReadOptions {
            refs: eight,
            have,
            ..Default::default()
        },
    )
    .expect("read");
    let f = find(
        &fx.meta,
        &fx.state,
        &FindOptions {
            q: "clause".into(),
            ..Default::default()
        },
    )
    .expect("find");
    let rows = [
        mirror_row(
            "outline depth 1 (200 blocks, 48-char previews)",
            Tool::Outline,
            &o1,
        ),
        mirror_row("outline depth 0 (headings only)", Tool::Outline, &o0),
        mirror_row("read 8 blocks", Tool::Read, &r8),
        mirror_row("re-read the same 8 blocks with have", Tool::Read, &again),
        mirror_row("find 'clause' (20 hits)", Tool::Find, &f),
    ];
    let plain: String = blocks
        .iter()
        .map(|n| n.current_fields["label"].as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    let raw = serde_json::to_string(&fx.state).expect("state");
    json!({
        "blocks": blocks.len(),
        "words": words,
        "calls": rows,
        "references": [
            reference("raw state", "serde_json of DocumentState (about what the current document route returns as `state`)", &raw),
            reference("plain text", "every block's text, one line per block (a thoughtful wrapper's best case)", &plain),
        ],
    })
}

fn scaled_workbook() -> Value {
    let fx = fixtures::scaled_workbook(480);
    let profile = outline(&fx.meta, &fx.state, &OutlineOptions::default()).expect("profile");
    let range = read(
        &fx.meta,
        &fx.state,
        &ReadOptions {
            range: Some("Budget!A2:H41".into()),
            ..Default::default()
        },
    )
    .expect("range");
    let f = find(
        &fx.meta,
        &fx.state,
        &FindOptions {
            q: "SUM(D4".into(),
            scope: engine_agent::find::Scope::Formulas,
            ..Default::default()
        },
    )
    .expect("find");
    let rows = [
        mirror_row("profile (480 rows x 12 columns)", Tool::Outline, &profile),
        mirror_row("range A2:H41 (40 x 8)", Tool::Read, &range),
        mirror_row("find formulas 'SUM(D4'", Tool::Find, &f),
    ];
    let sheet = fx
        .state
        .nodes
        .values()
        .find(|n| n.current_fields["kind"] == "sheet")
        .expect("sheet");
    let mut grid: Vec<(String, Vec<String>)> = fx
        .state
        .nodes
        .values()
        .filter(|n| n.parent_id.as_ref() == Some(&sheet.id))
        .map(|n| {
            (
                n.pos.clone(),
                (0..12)
                    .map(|c| {
                        n.current_fields[format!("cell_{c}")]
                            .as_str()
                            .unwrap_or("")
                            .to_string()
                    })
                    .collect(),
            )
        })
        .collect();
    grid.sort();
    let grid: Vec<Vec<String>> = grid.into_iter().map(|(_, r)| r).collect();
    json!({
        "rows": grid.len(),
        "columns": 12,
        "calls": rows,
        "references": [
            reference("raw state", "serde_json of DocumentState", &serde_json::to_string(&fx.state).expect("state")),
            reference("whole sheet as CSV", "stored cell strings, no evaluation (a thoughtful wrapper's best case)", &csv(&grid)),
        ],
    })
}

fn scaled_deck() -> Value {
    let fx = fixtures::scaled_deck(14);
    let o1 = outline(&fx.meta, &fx.state, &OutlineOptions::default()).expect("outline");
    let o0 = outline(
        &fx.meta,
        &fx.state,
        &OutlineOptions {
            depth: Some(0),
            ..Default::default()
        },
    )
    .expect("outline");
    let slides = read(
        &fx.meta,
        &fx.state,
        &ReadOptions {
            slides: vec!["s1".into(), "s2".into(), "s3".into()],
            ..Default::default()
        },
    )
    .expect("read");
    let rows = [
        mirror_row("outline depth 0 (14 slides)", Tool::Outline, &o0),
        mirror_row("outline depth 1 (shapes)", Tool::Outline, &o1),
        mirror_row("read 3 slides", Tool::Read, &slides),
    ];
    let text: String = fx
        .state
        .nodes
        .values()
        .filter(|n| n.node_type == "item")
        .map(|n| n.current_fields["text"].as_str().unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n");
    json!({
        "slides": 14,
        "calls": rows,
        "references": [
            reference("raw state", "serde_json of DocumentState (inline images dominate)", &serde_json::to_string(&fx.state).expect("state")),
            reference("slide text", "the text of every text shape, one line each", &text),
        ],
    })
}

fn diff_row() -> Value {
    let d = fixtures::document();
    let r = diff(
        &d.edited.meta,
        d.base.meta.rev,
        &d.base.state,
        &d.edited.state,
        &DiffOptions::default(),
    )
    .expect("diff");
    json!(mirror_row(
        "diff summary of the edit session",
        Tool::Diff,
        &r
    ))
}

fn main() {
    let mut date = "unknown".to_string();
    let mut out: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--date" => date = args.next().unwrap_or(date),
            "--out" => out = args.next(),
            other => panic!("unknown argument {other}"),
        }
    }
    let report = json!({
        "benchmark": "agent-dct-bytes",
        "version": 1,
        "date": date,
        "platform": format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
        "build": if cfg!(debug_assertions) { "debug" } else { "release" },
        "command": "cargo run -p engine-agent --example byte_benchmark -- --date <YYYY-MM-DD> --out <path>",
        "measurement": "UTF-8 byte and Unicode scalar counts of the text each tool call returns (DCT) and of the JSON mirror of the same typed answer (protocol spec 12.3, compact serde_json). One deterministic run; sizes do not vary between runs. No tokeniser was run.",
        "token_estimators": {
            "bytes4": "ceil(utf8 bytes / 4): the ~tok estimate of the DCT header (spec 6.2, 11); the ratio of two such estimates equals the byte ratio, so it adds no information beyond bytes",
            "chars4": "ceil(Unicode scalar values / 4): shown to expose the effect of non-ASCII text (Bengali and emoji take 3 to 4 bytes per character in UTF-8 and are typically tokenised differently)",
        },
        "limitations": [
            "Payload bytes are not tokens: model tokenisers split JSON punctuation, identifiers and non-Latin text differently from prose, and neither estimator was calibrated against one.",
            "Per the claims policy of docs/AI-AGENT-TOKEN-EFFICIENCY.md section 9, no figure here may be quoted as a token saving; the byte ratios overstate real savings because tool definitions, system prompts and history are fixed costs per turn (section 3.4 of that document).",
            "The JSON mirror is this crate's own serialisation of its typed results, not a measured competitor: another JSON encoding of the same facts could be smaller or larger. It is not the raw REST response (B0) and not a plain-text wrapper (B1); the scaled `references` give raw-state and plain-text sizes for scale only.",
            "Golden corpus: small synthetic files, mixed English, Bengali, hostile strings and escapes; outline previews and structure lines dominate, so ratios differ from large real documents. The scaled files use a word list of 40 words with uniform 35 to 55 word paragraphs, no tables or lists, 8 percent formatted runs; real files differ.",
            "The scaled files resemble, but are not, the fixture of the token-efficiency document (different text); compare orders of magnitude only.",
            "Sizes depend on the format version dd/1 at this commit; any format change changes them (golden files make that visible).",
        ],
        "golden": golden(),
        "scaled": {
            "document": scaled_document(),
            "workbook": scaled_workbook(),
            "deck": scaled_deck(),
            "diff": diff_row(),
        },
    });
    let text = serde_json::to_string_pretty(&report).expect("report") + "\n";
    match out {
        Some(path) => std::fs::write(&path, text).unwrap_or_else(|e| panic!("write {path}: {e}")),
        None => print!("{text}"),
    }
}
