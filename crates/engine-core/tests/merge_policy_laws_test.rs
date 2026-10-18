//! The built-in strategies against the laws of design 4.1, over generated inputs.
//! Each run also requires that the generator reached merges and conflicts, so a
//! passing run is not an empty one.
use engine_core::merge_policy::{
    collections::ElementWise,
    default::{PropertyMerge, RichText, WholeValue},
    laws::{check, Report, Rng, Setup, Triple},
    Aspect, Strategy,
};
use serde_json::{json, Value};

const CASES: usize = 4000;

fn scalar(rng: &mut Rng) -> Value {
    [
        Value::Null,
        json!(0),
        json!(1),
        json!("a"),
        json!("b"),
        json!([1]),
        json!({"k": 1}),
        json!(true),
    ][rng.below(8)]
    .clone()
}

fn whole_values(rng: &mut Rng) -> Triple {
    Triple {
        base: scalar(rng),
        team: scalar(rng),
        draft: scalar(rng),
    }
}

fn page_setup(rng: &mut Rng) -> Triple {
    let base = json!({"size":"A4","landscape":false,"margin":25,"pageNumbers":true});
    let change = |rng: &mut Rng| {
        let mut v = base.clone();
        for _ in 0..rng.below(3) {
            let (key, values) = [
                ("size", vec![json!("A4"), json!("Letter")]),
                ("landscape", vec![json!(false), json!(true)]),
                ("margin", vec![json!(20), json!(25), json!(30)]),
                ("pageNumbers", vec![json!(true), json!(false)]),
                ("header", vec![json!("h1"), json!("h2")]),
            ][rng.below(5)]
            .clone();
            v[key] = values[rng.below(values.len())].clone();
        }
        v
    };
    Triple {
        base: if rng.below(8) == 0 {
            Value::Null
        } else {
            base.clone()
        },
        team: change(rng),
        draft: change(rng),
    }
}

const WORDS: [&str; 10] = [
    "alpha ", "beta ", "gamma ", "delta ", "epsilon ", "zeta ", "eta ", "theta ", "iota ", "kappa ",
];

type Runs = Vec<(String, usize)>;

fn marks_of(kind: usize) -> Option<Value> {
    match kind {
        1 => Some(json!([{"type":"bold"}])),
        2 => Some(json!([{"type":"italic"}])),
        3 => Some(json!([{"type":"textStyle","attrs":{"color":"#ff0000"}}])),
        4 => Some(json!([{"type":"bold"},{"type":"italic"}])),
        _ => None,
    }
}

/// One paragraph; adjacent runs with equal marks are one text node.
fn paragraph(runs: &Runs) -> Value {
    let mut nodes: Vec<(String, usize)> = Vec::new();
    for (text, kind) in runs {
        match nodes.last_mut() {
            Some((last, k)) if k == kind => last.push_str(text),
            _ => nodes.push((text.clone(), *kind)),
        }
    }
    let content: Vec<Value> = nodes
        .into_iter()
        .map(|(text, kind)| {
            let mut node = json!({"type":"text","text":text});
            if let Some(marks) = marks_of(kind) {
                node["marks"] = marks;
            }
            node
        })
        .collect();
    json!({"type":"paragraph","attrs":{"blockId":"b"},"content":content})
}

fn edit(rng: &mut Rng, runs: &Runs) -> Runs {
    let mut out = runs.clone();
    for _ in 0..rng.below(3) {
        let at = rng.below(out.len().max(1));
        match rng.below(5) {
            0 if !out.is_empty() => out[at].0 = WORDS[rng.below(WORDS.len())].to_string(),
            1 => out.insert(
                at,
                (WORDS[rng.below(WORDS.len())].to_string(), rng.below(5)),
            ),
            2 if out.len() > 1 => {
                out.remove(at);
            }
            _ if !out.is_empty() => out[at].1 = rng.below(5),
            _ => {}
        }
    }
    out
}

fn rich_text(rng: &mut Rng) -> Triple {
    let base: Runs = (0..6)
        .map(|_| (WORDS[rng.below(WORDS.len())].to_string(), rng.below(5)))
        .collect();
    Triple {
        team: paragraph(&edit(rng, &base)),
        draft: paragraph(&edit(rng, &base)),
        base: paragraph(&base),
    }
}

/// A paragraph the editor could have produced: only text children, none empty, and
/// no two neighbours with the same marks.
fn canonical_paragraph(v: &Value) -> bool {
    let Some(children) = v["content"].as_array() else {
        return false;
    };
    v["type"] == "paragraph"
        && children
            .iter()
            .all(|c| c["type"] == "text" && c["text"].as_str().is_some_and(|t| !t.is_empty()))
        && children.windows(2).all(|w| w[0]["marks"] != w[1]["marks"])
}

fn comment(rng: &mut Rng) -> Value {
    json!({"node_id": null, "body": format!("c{}", rng.below(5))})
}

fn comments(rng: &mut Rng) -> Triple {
    let base: Vec<Value> = (0..rng.below(4)).map(|_| comment(rng)).collect();
    let change = |rng: &mut Rng| {
        let mut v = base.clone();
        for _ in 0..rng.below(3) {
            if !v.is_empty() && rng.below(3) == 0 {
                v.remove(rng.below(v.len()));
            } else {
                v.push(comment(rng));
            }
        }
        Value::Array(v)
    };
    Triple {
        team: change(rng),
        draft: change(rng),
        base: Value::Array(base),
    }
}

fn suggestions(rng: &mut Rng) -> Triple {
    let one = |rng: &mut Rng| json!({"target_node_id":null,"detail":{"n":rng.below(3)},"accepted":rng.below(2)==0,"rejected_reason":null});
    let mut base = serde_json::Map::new();
    for i in 0..rng.below(3) {
        base.insert(format!("s{i}"), one(rng));
    }
    let change = |rng: &mut Rng| {
        let mut v = base.clone();
        for _ in 0..rng.below(3) {
            let key = format!("s{}", rng.below(4));
            if rng.below(4) == 0 {
                v.remove(&key);
            } else {
                v.insert(key, one(rng));
            }
        }
        Value::Object(v)
    };
    Triple {
        team: change(rng),
        draft: change(rng),
        base: Value::Object(base),
    }
}

fn run(
    strategy: &dyn Strategy,
    setup: &Setup<'_>,
    seed: u64,
    gen: fn(&mut Rng) -> Triple,
) -> Report {
    let mut rng = Rng(seed);
    let cases: Vec<Triple> = (0..CASES).map(|_| gen(&mut rng)).collect();
    check(strategy, setup, cases).unwrap_or_else(|v| {
        panic!(
            "{} broke {}: {}\nbase  {}\nteam  {}\ndraft {}",
            strategy.id(),
            v.law,
            v.detail,
            v.case.base,
            v.case.team,
            v.case.draft
        )
    })
}

#[test]
fn whole_value_keeps_every_law() {
    let report = run(
        &WholeValue,
        &Setup::field("label"),
        0x6c61_7773_01,
        whole_values,
    );
    assert!(report.merged > 200 && report.conflicts > 200, "{report:?}");
    assert_eq!(report.preferred, 0);
    assert_eq!(report.skipped, ["L6 invariants", "L8 bounded"]);
}

#[test]
fn page_setup_property_merge_keeps_every_law() {
    let report = run(
        &PropertyMerge,
        &Setup::field("page_setup"),
        0x6c61_7773_02,
        page_setup,
    );
    assert!(report.merged > 200 && report.conflicts > 200, "{report:?}");
}

/// The value with the marks of every run in type order and neighbouring runs with
/// equal marks joined, as the editor would write it.
fn marks_sorted(v: &Value) -> Value {
    let mut v = v.clone();
    let Some(children) = v["content"].as_array().cloned() else {
        return v;
    };
    let mut out: Vec<Value> = Vec::new();
    for mut child in children {
        if let Some(marks) = child.get_mut("marks").and_then(Value::as_array_mut) {
            marks.sort_by_key(|m| m["type"].as_str().unwrap_or("").to_string());
        }
        match out.last_mut() {
            Some(last) if last.get("marks") == child.get("marks") => {
                let joined = format!(
                    "{}{}",
                    last["text"].as_str().unwrap_or(""),
                    child["text"].as_str().unwrap_or("")
                );
                last["text"] = json!(joined);
            }
            _ => out.push(child),
        }
    }
    v["content"] = Value::Array(out);
    v
}

#[test]
fn rich_text_hunk_merge_is_symmetric_up_to_the_order_of_marks_on_a_run() {
    let mut rng = Rng(0x6c61_7773_07);
    let (mut merged, mut reordered) = (0, 0);
    for _ in 0..CASES {
        let t = rich_text(&mut rng);
        let (forward, backward) = (
            engine_core::richtext::merge(&t.base, &t.team, &t.draft),
            engine_core::richtext::merge(&t.base, &t.draft, &t.team),
        );
        assert_eq!(forward.is_some(), backward.is_some(), "{t:?}");
        if let (Some(f), Some(b)) = (forward, backward) {
            merged += 1;
            reordered += (f != b) as u32;
            assert_eq!(marks_sorted(&f), marks_sorted(&b), "{t:?}");
        }
    }
    // The generator reaches the case the promise is withdrawn for.
    assert!(merged > 500 && reordered > 0, "{merged} {reordered}");
}

#[test]
fn rich_text_hunk_merge_keeps_every_law_and_returns_canonical_paragraphs() {
    let valid = canonical_paragraph;
    let setup = Setup {
        valid: Some(&valid),
        ..Setup::field("content")
    };
    let report = run(&RichText, &setup, 0x6c61_7773_03, rich_text);
    assert!(report.merged > 500 && report.conflicts > 500, "{report:?}");
    assert!(report.checked.contains(&"L6 invariants"));
    assert!(report.skipped.contains(&"L1 symmetry"));
}

#[test]
fn element_wise_comments_and_suggestions_keep_every_law() {
    for (aspect, seed, gen) in [
        (
            Aspect::Comments,
            0x6c61_7773_04,
            comments as fn(&mut Rng) -> Triple,
        ),
        (Aspect::Suggestions, 0x6c61_7773_05, suggestions),
    ] {
        let setup = Setup {
            aspect: Some(aspect),
            field: "$collection",
            ..Setup::field("x")
        };
        let report = run(&ElementWise, &setup, seed, gen);
        assert!(report.merged > 500, "{aspect:?} {report:?}");
        assert!(report.checked.contains(&"L8 bounded"));
    }
    // Suggestions with one id changed on both sides are a conflict, never a guess.
    let setup = Setup {
        aspect: Some(Aspect::Suggestions),
        field: "$suggestions",
        ..Setup::field("x")
    };
    let report = run(&ElementWise, &setup, 0x6c61_7773_06, suggestions);
    assert!(report.conflicts > 100, "{report:?}");
}
