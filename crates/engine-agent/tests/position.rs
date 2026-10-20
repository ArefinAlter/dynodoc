//! Position keys (protocol spec 8.2, 17 "between vectors"): the Rust `between` against the
//! vectors produced by running the web editor's function, plus order and length laws.

use std::path::PathBuf;

use engine_agent::pos::{between, key_between, try_between, MAX_KEY_STEPS};
use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use serde_json::Value;

fn vectors() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("vectors")
        .join("between.json");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).expect("between.json is JSON")
}

fn opt(v: &Value) -> Option<&str> {
    v.as_str()
}

#[test]
fn rust_reproduces_every_typescript_case() {
    let v = vectors();
    let cases = v["cases"].as_array().unwrap();
    // Spec 17: 10,000 random pairs.
    assert!(cases.len() >= 10_000, "{} cases", cases.len());
    let mut fallbacks = 0;
    for c in cases {
        let (left, right, key) = (c[0].as_str().unwrap(), opt(&c[1]), c[2].as_str().unwrap());
        assert_eq!(between(left, right), key, "between({left:?}, {right:?})");
        assert_eq!(try_between(left, right).as_deref(), Some(key));
        if key == format!("{left}P") {
            fallbacks += 1;
        }
    }
    // The vectors reach the 200-position fallback and the loop exit alike.
    assert!(fallbacks >= 3, "{fallbacks} fallback cases");
}

#[test]
// The mirror's newer Clippy prefers is_multiple_of, which the 1.85 baseline lacks.
#[allow(clippy::manual_is_multiple_of)]
fn rust_reproduces_every_typescript_chain() {
    let v = vectors();
    let mut longest_of_all = 0;
    for chain in v["chains"].as_array().unwrap() {
        let mode = chain["mode"].as_str().unwrap();
        let mut left = chain["left"].as_str().unwrap().to_string();
        let mut right = opt(&chain["right"]).map(str::to_string);
        let steps = chain["steps"].as_u64().unwrap() as usize;
        let every = chain["every"].as_u64().unwrap() as usize;
        let keys = chain["keys"].as_array().unwrap();
        assert_eq!(keys.len(), steps / every);
        let mut seen = 0;
        for i in 1..=steps {
            let k = between(&left, right.as_deref());
            match mode {
                "left_chases" => left = k.clone(),
                "right_chases" => right = Some(k.clone()),
                other => panic!("mode {other}"),
            }
            if i % every == 0 {
                assert_eq!(k, keys[seen].as_str().unwrap(), "{mode} step {i}");
                seen += 1;
            }
        }
        let longest = keys
            .iter()
            .map(|k| k.as_str().unwrap().chars().count())
            .max()
            .unwrap();
        assert!(longest >= 100, "{mode} {left:?}: longest key {longest}");
        longest_of_all = longest_of_all.max(longest);
    }
    // The chains cross the 200-character mark, where the original changes behaviour.
    assert!(
        longest_of_all >= MAX_KEY_STEPS,
        "longest key {longest_of_all}"
    );
}

#[test]
fn the_vectors_come_from_the_current_typescript_source() {
    // The function text is stored next to the vectors. When the web editor source is in the
    // tree (it is not in the standalone engine mirror) it must still contain that exact text,
    // otherwise the vectors describe a function that no longer ships.
    let v = vectors();
    let stored = v["between_source"].as_str().unwrap();
    assert!(stored.starts_with("export function between("));
    let ts = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(v["source"].as_str().unwrap());
    let Ok(text) = std::fs::read_to_string(&ts) else {
        eprintln!("{} not present: source drift check skipped", ts.display());
        return;
    };
    let text = text.replace("\r\n", "\n");
    assert!(
        text.contains(stored),
        "between() in {} changed: regenerate tests/vectors/between.json with between-gen.mjs",
        ts.display()
    );
}

fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2000);
    let mut config = ProptestConfig::with_cases(cases);
    if std::env::var_os("PROPTEST_RNG_SEED").is_none() {
        config.rng_seed = RngSeed::Fixed(0x7061_6972_7300);
    }
    config.failure_persistence = None;
    config
}

/// Printable ASCII without the space: U+0021..U+007E, the domain in which the original
/// keeps its order guarantee (see the module documentation of `pos`).
fn key(max: usize) -> impl Strategy<Value = String> {
    let alphabets: Vec<&'static str> = vec![
        "[!-~]",
        "[a-b]",
        "[x-~]",
        "[0-2]",
        "[!-#]",
        "[|-~]",
        "[A-Za-z0-9]",
    ];
    proptest::sample::select(alphabets).prop_flat_map(move |class| {
        proptest::string::string_regex(&format!("{class}{{0,{max}}}")).unwrap()
    })
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn a_key_sorts_strictly_between_its_neighbours(a in key(14), b in key(14), open in 0u8..8) {
        let (mut left, right) = if a <= b { (a, b) } else { (b, a) };
        prop_assume!(left != right);
        // A tail of spaces on `left` is allowed; on `right` it is outside the domain.
        left.push_str(["", "", "", " "][(open % 4) as usize]);
        prop_assume!(left < right);
        let k = between(&left, Some(&right));
        prop_assert!(left < k, "{left:?} < {k:?}");
        prop_assert!(k < right, "{k:?} < {right:?}");
        let checked = key_between(Some(&left), Some(&right));
        prop_assert_eq!(checked.as_deref(), Some(k.as_str()));
        prop_assert!(k.chars().count() <= MAX_KEY_STEPS);
    }

    #[test]
    fn a_key_at_either_end_of_the_list(a in key(14)) {
        let k = between(&a, None);
        prop_assert!(a < k, "{a:?} < {k:?}");
        let k = between("", Some(&a));
        if a.is_empty() {
            prop_assert_eq!(key_between(None, Some(&a)), None);
        } else {
            prop_assert!(k < a, "{k:?} < {a:?}");
        }
    }

    #[test]
    fn length_never_exceeds_the_limit_except_for_the_documented_fallback(
        a in key(260),
        b in key(260),
    ) {
        let (left, right) = if a <= b { (a, b) } else { (b, a) };
        let k = between(&left, Some(&right));
        // Found inside the loop: at most 200 characters. The fallback is `left + "P"`.
        let limit = MAX_KEY_STEPS.max(left.chars().count() + 1);
        prop_assert!(k.chars().count() <= limit, "{} chars", k.chars().count());
        // Whatever it returns, key_between only passes keys that sort strictly between.
        if let Some(checked) = key_between(Some(&left), Some(&right)) {
            prop_assert!(left < checked && checked < right);
        }
    }
}

#[test]
fn the_length_boundary() {
    // `}` is 125, one below the default 126, so no gap ever opens inside the left key.
    for n in 190..=205usize {
        let left = "}".repeat(n);
        let k = between(&left, None);
        if n <= 199 {
            // Position n (a missing unit, 32) opens a gap: the key is left + "O", 200 at most.
            assert_eq!(k, format!("{left}O"), "n={n}");
            assert!(k.chars().count() <= MAX_KEY_STEPS, "n={n}");
        } else {
            // 200 positions without a gap: the fallback appends "P" to the whole of left.
            assert_eq!(k, format!("{left}P"), "n={n}");
        }
    }
}

#[test]
fn thousands_of_inserts_at_one_place_stay_in_order_until_the_boundary() {
    // The editor's pattern at its worst: every new block goes to the same spot, so each new
    // key is squeezed between the anchor and the key inserted before it (or grows from the
    // last one). The list must stay strictly increasing for as long as key_between hands out
    // keys, and a refusal must come only when the key has used up the 200-character window,
    // never earlier. The original's own fallback would go on appending a character per call
    // at the end of the list (and mis-order at its start); the compiler stops instead.
    for at_start in [true, false] {
        let mut keys: Vec<String> = vec!["O".to_string()];
        let mut refused = None;
        for i in 0..4000 {
            let (l, r): (Option<String>, Option<String>) = if at_start {
                (None, Some(keys[0].clone()))
            } else {
                (keys.last().cloned(), None)
            };
            match key_between(l.as_deref(), r.as_deref()) {
                Some(k) if at_start => keys.insert(0, k),
                Some(k) => keys.push(k),
                None => {
                    refused = Some(i);
                    break;
                }
            }
        }
        for w in keys.windows(2) {
            assert!(w[0] < w[1], "{:?} < {:?}", w[0], w[1]);
        }
        let i = refused.unwrap_or_else(|| panic!("at_start={at_start}: boundary not reached"));
        let edge = if at_start {
            &keys[0]
        } else {
            keys.last().unwrap()
        };
        // About one extra character per 6 inserts: far beyond what one proposal creates.
        assert!(
            i > 800,
            "at_start={at_start}: refused after only {i} inserts"
        );
        assert!(
            edge.chars().count() >= MAX_KEY_STEPS - 1,
            "at_start={at_start}: {} chars",
            edge.chars().count()
        );
        // The refused key is exactly what the original returns, and it is not acceptable:
        // too long, or out of order.
        let (l, r) = if at_start {
            (None, Some(edge.as_str()))
        } else {
            (Some(edge.as_str()), None)
        };
        let raw = between(l.unwrap_or(""), r);
        assert!(
            raw.chars().count() > MAX_KEY_STEPS
                || l.is_some_and(|l| raw.as_str() <= l)
                || r.is_some_and(|r| raw.as_str() >= r),
            "at_start={at_start}: {raw:?}"
        );
    }
}
