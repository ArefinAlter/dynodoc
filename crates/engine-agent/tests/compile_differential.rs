//! Differential tests of the compiler (protocol spec 17 "compile property", "macro
//! equivalence"): random edit scripts, generated from fixed seeds, are compiled and applied
//! with `apply_payload`, and the result must equal what an independent reference model of
//! the same edits computes on plain blocks of styled characters. After every call the
//! compiled ops must apply, round-trip through `workspace_merge::diff` without phantom ops,
//! and leave every block the script did not name byte-for-byte as it was.

use std::collections::{BTreeMap, BTreeSet};

use engine_agent::compile::{compile, CompileInput, Compiled, IdSource};
use engine_agent::error::{AgentError, CompileError};
use engine_agent::fixtures::{self, Fixture};
use engine_agent::grammar::parse_ops;
use engine_agent::model::FileKind;
use engine_agent::refs::RefIndex;
use engine_core::governance::{authorize, Role};
use engine_core::materializer::{apply_payload, DocumentState};
use engine_core::workspace_merge;
use engine_shared::{EventPayload, NodeId, NodeType};
use serde_json::{json, Value};

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    fn chance(&mut self, percent: usize) -> bool {
        self.below(100) < percent
    }

    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len())]
    }
}

// ---------------------------------------------------------------- the reference model

/// Character styles: index 0 is plain text, the rest carry one mark each.
fn palette() -> Vec<Value> {
    vec![
        json!({"type": "text"}),
        json!({"type": "text", "marks": [{"type": "bold"}]}),
        json!({"type": "text", "marks": [{"type": "italic"}]}),
        json!({"type": "text", "marks": [{"type": "link", "attrs": {"href": "https://example.org/a"}}]}),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    P,
    H(u8),
}

#[derive(Clone, Debug, PartialEq)]
struct Block {
    id: NodeId,
    kind: Kind,
    chars: Vec<(char, usize)>,
}

fn text_of(chars: &[(char, usize)]) -> String {
    chars.iter().map(|(c, _)| *c).collect()
}

fn content_of_block(b: &Block) -> Value {
    let pal = palette();
    let mut nodes: Vec<Value> = Vec::new();
    let mut i = 0;
    while i < b.chars.len() {
        let style = b.chars[i].1;
        let mut j = i;
        let mut s = String::new();
        while j < b.chars.len() && b.chars[j].1 == style {
            s.push(b.chars[j].0);
            j += 1;
        }
        let mut node = pal[style].clone();
        node["text"] = json!(s);
        nodes.push(node);
        i = j;
    }
    let mut v = match b.kind {
        Kind::P => json!({"type": "paragraph", "attrs": {"blockId": b.id}}),
        Kind::H(l) => json!({"type": "heading", "attrs": {"level": l, "blockId": b.id}}),
    };
    v["content"] = Value::Array(nodes);
    v
}

fn parse_block(id: &NodeId, content: &Value) -> Block {
    let pal = palette();
    let kind = match content["type"].as_str().unwrap() {
        "paragraph" => Kind::P,
        "heading" => Kind::H(content["attrs"]["level"].as_u64().unwrap() as u8),
        other => panic!("unexpected block type {other}"),
    };
    let mut chars = Vec::new();
    for n in content["content"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
    {
        let mut meta = n.as_object().unwrap().clone();
        let text = meta.remove("text").unwrap();
        let meta = Value::Object(meta);
        let style = pal
            .iter()
            .position(|p| *p == meta)
            .unwrap_or_else(|| panic!("style outside the palette: {meta}"));
        chars.extend(text.as_str().unwrap().chars().map(|c| (c, style)));
    }
    Block {
        id: id.clone(),
        kind,
        chars,
    }
}

/// Live blocks in reading order, from a state.
fn blocks_of(state: &DocumentState) -> Vec<Block> {
    let root = state
        .nodes
        .values()
        .find(|n| n.parent_id.is_none())
        .unwrap();
    let mut v: Vec<_> = state
        .nodes
        .values()
        .filter(|n| !n.deleted && n.parent_id.as_ref() == Some(&root.id))
        .collect();
    v.sort_by(|a, b| (&a.pos, &a.id).cmp(&(&b.pos, &b.id)));
    v.into_iter()
        .map(|n| parse_block(&n.id, &n.current_fields["content"]))
        .collect()
}

/// Char-index ranges of the non-overlapping, left-to-right matches of `find` in `text`.
fn matches(text: &str, find: &str) -> Vec<(usize, usize)> {
    let flen = find.chars().count();
    let chars: Vec<char> = text.chars().collect();
    let f: Vec<char> = find.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + flen <= chars.len() {
        if chars[i..i + flen] == f[..] {
            out.push((i, i + flen));
            i += flen;
        } else {
            i += 1;
        }
    }
    out
}

/// The reference `replace`: the matched spans become `with`, in the style of the first
/// replaced character; everything else is untouched.
fn splice(chars: &[(char, usize)], picked: &[(usize, usize)], with: &str) -> Vec<(char, usize)> {
    let mut out = Vec::new();
    let mut at = 0;
    for &(s, e) in picked {
        out.extend_from_slice(&chars[at..s]);
        out.extend(with.chars().map(|c| (c, chars[s].1)));
        at = e;
    }
    out.extend_from_slice(&chars[at..]);
    out
}

/// Length of the longest common subsequence.
fn lcs<T: PartialEq>(a: &[T], b: &[T]) -> usize {
    let mut row = vec![0usize; b.len() + 1];
    for x in a {
        let mut diag = 0;
        for (j, y) in b.iter().enumerate() {
            let up = row[j + 1];
            row[j + 1] = if x == y { diag + 1 } else { up.max(row[j]) };
            diag = up;
        }
    }
    row[b.len()]
}

const WORDS: [&str; 12] = [
    "alpha",
    "beta",
    "gamma",
    "delta",
    "policy",
    "review",
    "épée",
    "ক্ষমতা",
    "x",
    "ab",
    "ba",
    "aaa",
];

fn sentence(rng: &mut Rng, min: usize, extra: usize) -> String {
    let words = min + rng.below(extra);
    (0..words)
        .map(|_| *rng.pick(&WORDS))
        .collect::<Vec<_>>()
        .join(" ")
}

fn random_block(rng: &mut Rng, id: NodeId) -> Block {
    let text: Vec<char> = sentence(rng, 2, 8).chars().collect();
    // Cut into 1 to 4 runs of random styles (adjacent runs may share one).
    let mut chars = Vec::new();
    let mut at = 0;
    while at < text.len() {
        let len = 1 + rng.below(text.len() - at);
        let style = rng.below(4);
        chars.extend(text[at..at + len].iter().map(|c| (*c, style)));
        at += len;
    }
    Block {
        id,
        kind: if rng.chance(25) {
            Kind::H(1 + rng.below(3) as u8)
        } else {
            Kind::P
        },
        chars,
    }
}

fn fixture_of(blocks: &[Block], tag: u128) -> Fixture {
    let mut b = fixtures::Builder::new(FileKind::Document, "differential", 0xd1ff_0000 + tag);
    let root = b.node(NodeType::Form, None, 0, json!({"label": "Differential"}));
    for (i, blk) in blocks.iter().enumerate() {
        let content = content_of_block(blk);
        b.apply(EventPayload::NodeCreated {
            node_id: blk.id.clone(),
            node_type: NodeType::Item,
            parent_id: Some(root.clone()),
            pos: format!("{:08}", (i + 1) * 10),
            fields: json!({"label": text_of(&blk.chars), "content": content}),
            var_name: None,
        });
    }
    b.snapshot()
}

// ---------------------------------------------------------------- one scripted call

struct Step {
    ops: Vec<Value>,
    /// The model after the call, or the error the call must fail with.
    expect: Result<Vec<Block>, AgentError>,
    /// Blocks whose content the call may change.
    touched: BTreeSet<NodeId>,
}

fn pick_word(rng: &mut Rng, b: &Block) -> Option<String> {
    let t = text_of(&b.chars);
    let words: Vec<&str> = t.split(' ').filter(|w| !w.is_empty()).collect();
    (!words.is_empty()).then(|| rng.pick(&words).to_string())
}

/// How an op names a block: its ref at the start of the call, or `$n` when the call made it.
fn name(refs: &RefIndex, made: &[NodeId], id: &NodeId) -> String {
    match made.iter().position(|n| n == id) {
        Some(k) => format!("${}", k + 1),
        None => refs.ref_of(id).unwrap().to_string(),
    }
}

/// Generate one call of 1 to 4 ops against `model` and compute the expected result.
fn step(rng: &mut Rng, model: &[Block], next_id: &mut u64, refs: &RefIndex) -> Step {
    let mut m: Vec<Block> = model.to_vec();
    let mut ops = Vec::new();
    let mut touched = BTreeSet::new();
    let mut new_ids: Vec<NodeId> = Vec::new();
    let r = |b: &Block| name(refs, &[], &b.id);
    // A single deliberate no-op call, now and then.
    if rng.chance(8) {
        let i = rng.below(m.len());
        let b = &m[i];
        let (op, err) = match rng.below(4) {
            0 => (
                json!({"op": "set_text", "ref": r(b), "text": text_of(&b.chars)}),
                AgentError::NoChange,
            ),
            1 => match pick_word(rng, b) {
                Some(w) => (
                    json!({"op": "replace", "ref": r(b), "find": w, "with": w, "all": true}),
                    AgentError::NoChange,
                ),
                None => (
                    json!({"op": "set_type", "ref": r(b), "t": match b.kind { Kind::P => "p".to_string(), Kind::H(l) => format!("h{l}") }}),
                    AgentError::NoChange,
                ),
            },
            2 => (
                json!({"op": "set_type", "ref": r(b), "t": match b.kind { Kind::P => "p".to_string(), Kind::H(l) => format!("h{l}") }}),
                AgentError::NoChange,
            ),
            _ => (
                json!({"op": "replace", "ref": r(b), "find": "zzzz-absent", "with": "y"}),
                AgentError::NoMatch,
            ),
        };
        return Step {
            ops: vec![op],
            expect: Err(err),
            touched,
        };
    }
    for _ in 0..1 + rng.below(4) {
        match rng.below(8) {
            // set_text: reword, append, drop a word, or rewrite everything.
            0 | 1 => {
                let i = rng.below(m.len());
                let old = text_of(&m[i].chars);
                let new = match rng.below(4) {
                    0 => format!("{old} {}", sentence(rng, 1, 1)),
                    1 => format!("{} {old}", sentence(rng, 1, 1)),
                    2 => old
                        .split_once(' ')
                        .map_or(String::new(), |(_, rest)| rest.to_string()),
                    _ => sentence(rng, 1, 6),
                };
                if new == old {
                    continue;
                }
                ops.push(
                    json!({"op": "set_text", "ref": name(refs, &new_ids, &m[i].id), "text": new}),
                );
                // Reference for the text only; the styles are checked by property afterwards.
                let style = m[i].chars.first().map_or(0, |c| c.1);
                m[i].chars = new.chars().map(|c| (c, style)).collect();
                touched.insert(m[i].id.clone());
                // Marks the model cannot predict: mark the block as "styles free".
                touched.insert(NodeId(format!("free:{}", m[i].id.0)));
            }
            // replace one word: nth, all or the only match.
            2 | 3 => {
                let i = rng.below(m.len());
                let Some(w) = pick_word(rng, &m[i]) else {
                    continue;
                };
                let all = matches(&text_of(&m[i].chars), &w);
                let with = *rng.pick(&["Z", "zeta", "", "é é", "alpha"]);
                if with == w {
                    continue;
                }
                let (op, picked): (Value, Vec<(usize, usize)>) = match rng.below(3) {
                    0 if all.len() > 1 => {
                        let k = 1 + rng.below(all.len());
                        (
                            json!({"op": "replace", "ref": name(refs, &new_ids, &m[i].id), "find": w, "with": with, "nth": k}),
                            vec![all[k - 1]],
                        )
                    }
                    1 => (
                        json!({"op": "replace", "ref": name(refs, &new_ids, &m[i].id), "find": w, "with": with, "all": true, "expect": all.len()}),
                        all.clone(),
                    ),
                    _ if all.len() == 1 => (
                        json!({"op": "replace", "ref": name(refs, &new_ids, &m[i].id), "find": w, "with": with}),
                        all.clone(),
                    ),
                    _ => continue,
                };
                let chars = splice(&m[i].chars, &picked, with);
                if chars == m[i].chars {
                    continue;
                }
                ops.push(op);
                m[i].chars = chars;
                touched.insert(m[i].id.clone());
            }
            // replace_all over the whole document.
            4 => {
                let i = rng.below(m.len());
                let Some(w) = pick_word(rng, &m[i]) else {
                    continue;
                };
                let with = *rng.pick(&["Z", "zeta", "", "é é"]);
                if with == w {
                    continue;
                }
                let mut total = 0;
                let mut changed = Vec::new();
                for (k, b) in m.iter().enumerate() {
                    let hits = matches(&text_of(&b.chars), &w);
                    if !hits.is_empty() {
                        total += hits.len();
                        changed.push((k, splice(&b.chars, &hits, with)));
                    }
                }
                if changed.len() > 25 {
                    continue;
                }
                ops.push(json!({"op": "replace_all", "find": w, "with": with, "expect": total}));
                for (k, chars) in changed {
                    m[k].chars = chars;
                    touched.insert(m[k].id.clone());
                }
            }
            // insert 1 to 3 blocks after or before a block.
            5 => {
                let i = rng.below(m.len());
                let n = 1 + rng.below(3);
                let mut specs = Vec::new();
                let mut fresh = Vec::new();
                for _ in 0..n {
                    let (t, kind) = *rng.pick(&[
                        ("p", Kind::P),
                        ("h1", Kind::H(1)),
                        ("h2", Kind::H(2)),
                        ("h3", Kind::H(3)),
                    ]);
                    let text = sentence(rng, 1, 4);
                    *next_id += 1;
                    let id = fixtures::ulid(6_000_000 + *next_id);
                    fresh.push(Block {
                        id,
                        kind,
                        chars: text.chars().map(|c| (c, 0)).collect(),
                    });
                    specs.push(json!({"t": t, "text": text}));
                }
                let after = rng.chance(50);
                ops.push(if after {
                    json!({"op": "insert", "after": name(refs, &new_ids, &m[i].id), "blocks": specs})
                } else {
                    json!({"op": "insert", "before": name(refs, &new_ids, &m[i].id), "blocks": specs})
                });
                new_ids.extend(fresh.iter().map(|b| b.id.clone()));
                let at = if after { i + 1 } else { i };
                for (k, b) in fresh.into_iter().enumerate() {
                    touched.insert(b.id.clone());
                    m.insert(at + k, b);
                }
            }
            // delete (keeping at least two blocks).
            6 if m.len() > 2 => {
                let i = rng.below(m.len());
                ops.push(json!({"op": "delete", "ref": name(refs, &new_ids, &m[i].id)}));
                touched.insert(m[i].id.clone());
                m.remove(i);
            }
            // move after/before another block.
            6 => {}
            _ => {
                if rng.chance(50) && m.len() > 1 {
                    let (i, j) = (rng.below(m.len()), rng.below(m.len()));
                    if i == j {
                        continue;
                    }
                    let after = rng.chance(50);
                    let mut rest = m.clone();
                    let moved = rest.remove(i);
                    let jj = rest.iter().position(|b| b.id == m[j].id).unwrap();
                    let at = if after { jj + 1 } else { jj };
                    // Already there: leave it to the dedicated NoChange cases.
                    let mut probe = rest.clone();
                    probe.insert(at, moved.clone());
                    if probe == m {
                        continue;
                    }
                    ops.push(if after {
                        json!({"op": "move", "ref": name(refs, &new_ids, &moved.id), "after": name(refs, &new_ids, &m[j].id)})
                    } else {
                        json!({"op": "move", "ref": name(refs, &new_ids, &moved.id), "before": name(refs, &new_ids, &m[j].id)})
                    });
                    touched.insert(moved.id.clone());
                    m = probe;
                } else {
                    let i = rng.below(m.len());
                    let to = match rng.below(4) {
                        0 => Kind::P,
                        n => Kind::H(n as u8),
                    };
                    if m[i].kind == to {
                        continue;
                    }
                    ops.push(json!({"op": "set_type", "ref": name(refs, &new_ids, &m[i].id), "t": match to { Kind::P => "p".to_string(), Kind::H(l) => format!("h{l}") }}));
                    m[i].kind = to;
                    touched.insert(m[i].id.clone());
                }
            }
        }
    }
    if ops.is_empty() {
        // Nothing was generated: a harmless op that always changes something.
        let i = rng.below(m.len());
        let new = format!("{} !", text_of(&m[i].chars));
        ops.push(json!({"op": "set_text", "ref": r(&m[i]), "text": new}));
        let style = m[i].chars.first().map_or(0, |c| c.1);
        m[i].chars = new.chars().map(|c| (c, style)).collect();
        touched.insert(m[i].id.clone());
        touched.insert(NodeId(format!("free:{}", m[i].id.0)));
    }
    Step {
        ops,
        expect: Ok(m),
        touched,
    }
}

fn run(fx: &Fixture, ops: &[Value]) -> Result<Compiled, CompileError> {
    let parsed = parse_ops(&Value::Array(ops.to_vec()))?;
    let mut n = 0u64;
    let mut ids = move || {
        n += 1;
        // A stream that cannot collide with an earlier call on the same file.
        fixtures::ulid(3_000_000 + (fx.state.nodes.len() as u64) * 1000 + n)
    };
    compile(
        &CompileInput::new(&fx.meta, &fx.state, Role::Author),
        &parsed,
        &mut ids,
    )
}

/// The engine ops a compile may emit, and the fields they may touch: no variable names, no
/// node types but items, nothing outside content, label, cells, columns, text and notes.
fn assert_vocabulary(ops: &[EventPayload]) {
    let cell = |k: &str| {
        k.strip_prefix("cell_")
            .and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| n < 100)
    };
    for op in ops {
        match op {
            EventPayload::NodeCreated {
                node_type,
                fields,
                var_name,
                ..
            } => {
                assert_eq!(*node_type, NodeType::Item);
                assert!(var_name.is_none());
                for k in fields.as_object().unwrap().keys() {
                    assert!(k == "content" || k == "label" || cell(k), "{k}");
                }
            }
            EventPayload::FieldEdited { field, .. } => assert!(
                matches!(
                    field.as_str(),
                    "content" | "label" | "columns" | "text" | "notes"
                ) || cell(field),
                "{field}"
            ),
            EventPayload::RichTextPatched { .. }
            | EventPayload::NodeMoved { .. }
            | EventPayload::NodeDeleted { .. } => {}
            other => panic!("{other:?}"),
        }
    }
}

fn check_ops(fx: &Fixture, c: &Compiled) {
    assert_vocabulary(&c.ops);
    let mut state = fx.state.clone();
    for op in &c.ops {
        authorize(Role::Author, op, &state).unwrap_or_else(|e| panic!("{e}: {op:?}"));
        apply_payload(&mut state, op).unwrap();
    }
    assert_eq!(state, c.after);
    // The diff of the two states rebuilds the result and touches nothing else.
    let d = workspace_merge::diff(&fx.state, &c.after);
    let mut replay = fx.state.clone();
    for op in &d {
        apply_payload(&mut replay, op).unwrap();
    }
    let live = |s: &DocumentState| -> Vec<_> {
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
    };
    assert_eq!(live(&replay), live(&c.after));
    let touched: BTreeSet<&NodeId> = c
        .ops
        .iter()
        .filter_map(EventPayload::target_node_id)
        .collect();
    assert!(
        d.iter()
            .all(|op| touched.contains(op.target_node_id().unwrap())),
        "phantom op in {d:?}"
    );
}

#[test]
fn random_document_scripts_match_the_reference_model() {
    let mut calls = 0;
    let mut kinds: BTreeMap<String, usize> = BTreeMap::new();
    for seed in 1..=60u64 {
        let mut rng = Rng::new(seed);
        let n = 3 + rng.below(8);
        let mut model: Vec<Block> = (0..n)
            .map(|i| random_block(&mut rng, fixtures::ulid(100 + i as u64)))
            .collect();
        let mut fx = fixture_of(&model, u128::from(seed));
        let mut created = 0u64;
        for _ in 0..12 {
            let refs = RefIndex::from_state(&fx.state);
            let mut counter = created;
            let st = step(&mut rng, &model, &mut counter, &refs);
            calls += 1;
            for o in &st.ops {
                *kinds
                    .entry(o["op"].as_str().unwrap().to_string())
                    .or_default() += 1;
            }
            // The compiler's id stream restarts per call; give the model the same ids.
            let result = run_with_ids(&fx, &st.ops, &mut |n| {
                fixtures::ulid(6_000_000 + created + n)
            });
            match (&st.expect, result) {
                (Err(want), Err(got)) => {
                    assert_eq!(&got.error, want, "seed {seed} ops {:?}", st.ops)
                }
                (Err(want), Ok(_)) => panic!("seed {seed}: expected {want:?} for {:?}", st.ops),
                (Ok(_), Err(e)) => panic!("seed {seed}: {e} for {:?}", st.ops),
                (Ok(want), Ok(c)) => {
                    check_ops(&fx, &c);
                    let got = blocks_of(&c.after);
                    assert_eq!(got.len(), want.len(), "seed {seed} ops {:?}", st.ops);
                    for (g, w) in got.iter().zip(want) {
                        assert_eq!(g.id, w.id, "seed {seed} order, ops {:?}", st.ops);
                        assert_eq!(g.kind, w.kind, "seed {seed} kind of {:?}", w.id);
                        assert_eq!(
                            text_of(&g.chars),
                            text_of(&w.chars),
                            "seed {seed} text of {:?}",
                            w.id
                        );
                        let free = st.touched.contains(&NodeId(format!("free:{}", w.id.0)));
                        if !free {
                            assert_eq!(
                                g.chars, w.chars,
                                "seed {seed} styles of {:?}, ops {:?}",
                                w.id, st.ops
                            );
                        }
                    }
                    // Blocks the call did not name are byte-for-byte the same.
                    for (id, node) in &fx.state.nodes {
                        if !st.touched.contains(id) && !node.deleted && node.node_type == "item" {
                            assert_eq!(
                                c.after.nodes[id].current_fields, node.current_fields,
                                "seed {seed}: {id:?} changed without being named"
                            );
                        }
                    }
                    created += c
                        .ops
                        .iter()
                        .filter(|o| matches!(o, EventPayload::NodeCreated { .. }))
                        .count() as u64;
                    // The styles of rewritten blocks are not predicted: adopt the actual ones.
                    model = got;
                    fx = Fixture {
                        meta: fx.meta.clone(),
                        state: c.after,
                    };
                }
            }
        }
    }
    assert!(calls >= 700, "{calls} calls");
    for op in [
        "set_text",
        "replace",
        "replace_all",
        "insert",
        "delete",
        "move",
        "set_type",
    ] {
        assert!(
            kinds.get(op).copied().unwrap_or(0) >= 30,
            "{op} was generated {:?} times",
            kinds.get(op)
        );
    }
}

fn run_with_ids(
    fx: &Fixture,
    ops: &[Value],
    ids: &mut dyn FnMut(u64) -> NodeId,
) -> Result<Compiled, CompileError> {
    let parsed = parse_ops(&Value::Array(ops.to_vec()))?;
    let mut n = 0u64;
    let mut source = || {
        n += 1;
        ids(n)
    };
    let source: &mut dyn IdSource = &mut source;
    compile(
        &CompileInput::new(&fx.meta, &fx.state, Role::Author),
        &parsed,
        source,
    )
}

#[test]
fn set_text_keeps_every_unchanged_character_styled_as_it_was() {
    // Random rewrites of styled paragraphs: the common prefix and suffix keep their styles,
    // the text is exactly the new text, and no style outside the original ones appears.
    for seed in 1..=200u64 {
        let mut rng = Rng::new(seed + 1000);
        let block = random_block(&mut rng, fixtures::ulid(1));
        let fx = fixture_of(std::slice::from_ref(&block), u128::from(seed));
        let old = text_of(&block.chars);
        let chars: Vec<char> = old.chars().collect();
        let a = rng.below(chars.len() + 1);
        let b = a + rng.below(chars.len() - a + 1);
        let ins: String = if rng.chance(30) {
            String::new()
        } else {
            sentence(&mut rng, 1, 1)
        };
        let new: String = chars[..a]
            .iter()
            .chain(ins.chars().collect::<Vec<_>>().iter())
            .chain(chars[b..].iter())
            .collect();
        let r = RefIndex::from_state(&fx.state)
            .ref_of(&block.id)
            .unwrap()
            .to_string();
        let res = run(&fx, &[json!({"op": "set_text", "ref": r, "text": new})]);
        if new == old {
            assert_eq!(res.unwrap_err().error, AgentError::NoChange);
            continue;
        }
        let c = res.unwrap();
        check_ops(&fx, &c);
        let got = &blocks_of(&c.after)[0];
        assert_eq!(text_of(&got.chars), new, "seed {seed}");
        // The result retains an optimal alignment of the old text with its styles: the longest
        // common subsequence of (character, style) pairs is as long as that of the plain text.
        let text_lcs = lcs(
            &old.chars().collect::<Vec<_>>(),
            &new.chars().collect::<Vec<_>>(),
        );
        let styled_lcs = lcs(&block.chars, &got.chars);
        assert!(
            styled_lcs >= text_lcs,
            "seed {seed}: {styled_lcs} < {text_lcs}"
        );
        let used: BTreeSet<usize> = block.chars.iter().map(|c| c.1).collect();
        assert!(
            got.chars.iter().all(|c| used.contains(&c.1) || c.1 == 0),
            "seed {seed}"
        );
    }
}

#[test]
fn replace_all_is_idempotent_and_matches_its_expansion() {
    for seed in 1..=60u64 {
        let mut rng = Rng::new(seed + 5000);
        let model: Vec<Block> = (0..6)
            .map(|i| random_block(&mut rng, fixtures::ulid(200 + i)))
            .collect();
        let fx = fixture_of(&model, u128::from(seed));
        let w = loop {
            let b = rng.pick(&model);
            if let Some(w) = pick_word(&mut rng, b) {
                break w;
            }
        };
        let with = "QQ"; // shares no character with the vocabulary (all lower case)
        let total: usize = model
            .iter()
            .map(|b| matches(&text_of(&b.chars), &w).len())
            .sum();
        let ops = [json!({"op": "replace_all", "find": w, "with": with, "expect": total})];
        let c = run(&fx, &ops).unwrap();
        check_ops(&fx, &c);
        // Reference: the same edit on the model.
        let want: Vec<Vec<(char, usize)>> = model
            .iter()
            .map(|b| splice(&b.chars, &matches(&text_of(&b.chars), &w), with))
            .collect();
        let got = blocks_of(&c.after);
        assert_eq!(
            got.iter().map(|b| b.chars.clone()).collect::<Vec<_>>(),
            want,
            "seed {seed} {w:?}"
        );
        // Zero matches afterwards: the second sweep reports it instead of succeeding silently.
        let fx2 = Fixture {
            meta: fx.meta.clone(),
            state: c.after.clone(),
        };
        let again = run(
            &fx2,
            &[json!({"op": "replace_all", "find": w, "with": with})],
        );
        assert_eq!(again.unwrap_err().error, AgentError::NoMatch, "seed {seed}");
        // The expansion: one `replace all` per matching block gives the same state.
        let mut state = fx.state.clone();
        for b in &model {
            if matches(&text_of(&b.chars), &w).is_empty() {
                continue;
            }
            let step_fx = Fixture {
                meta: fx.meta.clone(),
                state: state.clone(),
            };
            let r = RefIndex::from_state(&state)
                .ref_of(&b.id)
                .unwrap()
                .to_string();
            state = run(
                &step_fx,
                &[json!({"op": "replace", "ref": r, "find": w, "with": with, "all": true})],
            )
            .unwrap()
            .after;
        }
        assert_eq!(blocks_of(&state), got, "seed {seed}");
    }
}

#[test]
fn move_keeps_ids_and_the_order_of_the_rest() {
    for seed in 1..=60u64 {
        let mut rng = Rng::new(seed + 9000);
        let model: Vec<Block> = (0..8)
            .map(|i| random_block(&mut rng, fixtures::ulid(300 + i)))
            .collect();
        let fx = fixture_of(&model, u128::from(seed));
        // Ten moves in one call; the model keeps a plain list.
        let mut list = model.clone();
        let mut ops = Vec::new();
        for _ in 0..10 {
            let (i, j) = (rng.below(list.len()), rng.below(list.len()));
            if i == j {
                continue;
            }
            let after = rng.chance(50);
            let refs = RefIndex::from_state(&fx.state);
            let r = |b: &Block| refs.ref_of(&b.id).unwrap().to_string();
            let mut rest = list.clone();
            let moved = rest.remove(i);
            let jj = rest.iter().position(|b| b.id == list[j].id).unwrap();
            let at = if after { jj + 1 } else { jj };
            let mut probe = rest.clone();
            probe.insert(at, moved.clone());
            if probe == list {
                continue; // already there: that is the NoChange case
            }
            ops.push(if after {
                json!({"op": "move", "ref": r(&moved), "after": r(&list[j])})
            } else {
                json!({"op": "move", "ref": r(&moved), "before": r(&list[j])})
            });
            list = probe;
        }
        if ops.is_empty() {
            continue;
        }
        let c = run(&fx, &ops).unwrap();
        check_ops(&fx, &c);
        let got = blocks_of(&c.after);
        assert_eq!(
            got.iter().map(|b| &b.id).collect::<Vec<_>>(),
            list.iter().map(|b| &b.id).collect::<Vec<_>>(),
            "seed {seed}"
        );
        // Moves change position keys only: contents are untouched.
        for (g, m) in got.iter().zip(&list) {
            assert_eq!(g.chars, m.chars);
        }
        assert!(c
            .ops
            .iter()
            .all(|o| matches!(o, EventPayload::NodeMoved { .. })));
        assert_eq!(c.ops.len(), ops.len());
    }
}

// ---------------------------------------------------------------- spreadsheets

fn grid(state: &DocumentState, sheet: &NodeId) -> Vec<Vec<String>> {
    let mut rows: Vec<_> = state
        .nodes
        .values()
        .filter(|n| !n.deleted && n.parent_id.as_ref() == Some(sheet))
        .collect();
    rows.sort_by(|a, b| (&a.pos, &a.id).cmp(&(&b.pos, &b.id)));
    rows.iter()
        .map(|r| {
            (0..20)
                .map(|c| engine_agent::model::field_string(&r.current_fields[format!("cell_{c}")]))
                .collect()
        })
        .collect()
}

#[test]
fn random_sheet_scripts_match_a_grid_model() {
    let vals = [
        "",
        "1",
        "22",
        "x y",
        "=A1+1",
        "é",
        "tab\there",
        "line\nbreak",
        "0",
    ];
    let mut calls = 0;
    for seed in 1..=40u64 {
        let mut rng = Rng::new(seed + 20_000);
        let mut b = fixtures::Builder::new(
            FileKind::Spreadsheet,
            "sheet-diff",
            0x5ee7_0000 + u128::from(seed),
        );
        let root = b.node(NodeType::Form, None, 0, json!({"label": "S"}));
        let sheet = b.node(
            NodeType::Section,
            Some(&root),
            1,
            json!({"label": "Data", "kind": "sheet", "columns": 12}),
        );
        let mut model: Vec<Vec<String>> = Vec::new();
        for r in 0..3 + rng.below(5) {
            let row: Vec<String> = (0..20)
                .map(|c| {
                    if c < 6 {
                        rng.pick(&vals).to_string()
                    } else {
                        String::new()
                    }
                })
                .collect();
            let fields: serde_json::Map<String, Value> = row
                .iter()
                .enumerate()
                .filter(|(_, v)| !v.is_empty())
                .map(|(c, v)| (format!("cell_{c}"), json!(v)))
                .collect();
            b.node(
                NodeType::Item,
                Some(&sheet),
                r as u64,
                Value::Object(fields),
            );
            model.push(row);
        }
        let mut fx = b.snapshot();
        for _ in 0..10 {
            let op;
            let mut next = model.clone();
            if rng.chance(70) {
                let (nr, nc) = (1 + rng.below(3), 1 + rng.below(4));
                let (r0, c0) = (rng.below(model.len() + 3), rng.below(8));
                let values: Vec<Vec<String>> = (0..nr)
                    .map(|_| (0..nc).map(|_| rng.pick(&vals).to_string()).collect())
                    .collect();
                op = json!({"op": "set_cells", "sheet": "Data", "at": format!("{}{}", engine_agent::a1::column_name(c0 as u16), r0 + 1), "values": values});
                // Reference: existing rows change in place; rows past the end appear up to the
                // last one that holds a value (filler rows are blank).
                let last_needed = (0..nr)
                    .filter(|r| r0 + r >= model.len() && values[*r].iter().any(|v| !v.is_empty()))
                    .map(|r| r0 + r)
                    .max();
                if let Some(last) = last_needed {
                    while next.len() <= last {
                        next.push(vec![String::new(); 20]);
                    }
                }
                for (r, row) in values.iter().enumerate() {
                    if let Some(target) = next.get_mut(r0 + r) {
                        for (c, v) in row.iter().enumerate() {
                            target[c0 + c] = v.clone();
                        }
                    }
                }
            } else {
                let (r0, c0) = (rng.below(model.len() + 1), rng.below(8));
                let (r1, c1) = (r0 + rng.below(4), c0 + rng.below(4));
                op = json!({"op": "clear", "sheet": "Data", "range": format!("{}{}:{}{}",
                    engine_agent::a1::column_name(c0 as u16), r0 + 1, engine_agent::a1::column_name(c1 as u16), r1 + 1)});
                for row in next.iter_mut().take(r1 + 1).skip(r0) {
                    for cell in row.iter_mut().take(c1 + 1).skip(c0) {
                        cell.clear();
                    }
                }
            }
            calls += 1;
            let res = run(&fx, std::slice::from_ref(&op));
            if next == model {
                assert_eq!(
                    res.unwrap_err().error,
                    AgentError::NoChange,
                    "seed {seed} {op}"
                );
                continue;
            }
            let c = res.unwrap_or_else(|e| panic!("seed {seed} {op}: {e}"));
            check_ops(&fx, &c);
            assert_eq!(grid(&c.after, &sheet), next, "seed {seed} {op}");
            model = next;
            fx = Fixture {
                meta: fx.meta.clone(),
                state: c.after,
            };
        }
    }
    assert!(calls >= 350);
}
