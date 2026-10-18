//! Round-trip and law tests for refs, addresses, hashes, revision tokens, cursors and
//! A1 ranges (protocol spec 5, 17). Seeds are fixed, so a failure reproduces; set
//! `PROPTEST_CASES` or `PROPTEST_RNG_SEED` to explore.

use std::collections::BTreeSet;

use engine_agent::a1::{A1Range, CellRef, MAX_COLS, MAX_ROWS};
use engine_agent::budget::Cursor;
use engine_agent::hash::{block_hash, row_hash, Hash, ShortHash};
use engine_agent::refs::{Address, Ref, RefIndex, ResolveError, SlideSel, Tail};
use engine_agent::rev::{Chain8, RevToken};
use engine_shared::NodeId;
use proptest::prelude::*;
use proptest::test_runner::RngSeed;
use serde_json::{json, Value};

fn config() -> ProptestConfig {
    let cases = std::env::var("PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(1000);
    let mut config = ProptestConfig::with_cases(cases);
    if std::env::var_os("PROPTEST_RNG_SEED").is_none() {
        config.rng_seed = RngSeed::Fixed(0x7072_6f70_7300);
    }
    config.failure_persistence = None;
    config
}

fn ref_text() -> impl Strategy<Value = String> {
    "[0-9A-Za-z]{4,26}"
}

fn crockford(n: usize) -> impl Strategy<Value = String> {
    proptest::collection::vec(proptest::sample::select(b"0123456789ABCDEFGHJKMNPQRSTVWXYZ".to_vec()), n)
        .prop_map(|v| v.into_iter().map(char::from).collect())
}

fn cell() -> impl Strategy<Value = CellRef> {
    (0..MAX_COLS, 1..=MAX_ROWS).prop_map(|(col, row)| CellRef { col, row })
}

fn sheet_name() -> impl Strategy<Value = Option<String>> {
    let pieces = vec!["Budget", " ", "'", "''", "!", "A1", ":", "$", "Q3 plan", "é", "ক", "\n", "\t", "\\", "\"", ".", "_", "[", "]", "-"];
    proptest::option::of(
        proptest::collection::vec(proptest::sample::select(pieces), 0..5).prop_map(|v| v.concat()),
    )
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn refs_round_trip(s in ref_text()) {
        let r = Ref::parse(&s).unwrap();
        prop_assert_eq!(r.as_str(), s.to_uppercase());
        prop_assert_eq!(Ref::parse(&r.to_string()).unwrap(), r);
    }

    #[test]
    fn addresses_round_trip(base in ref_text(), item in 1u32..=999, row in 1u32..=99_999, col in 1u32..=1000, shape in ref_text(), c in cell(), which in 0..5usize) {
        let base = Ref::parse(&base).unwrap();
        let a = match which {
            0 => Address::Block(base),
            1 => Address::Sub { base, tail: Tail::Item(item) },
            2 => Address::Sub { base, tail: Tail::Cell { row, col } },
            3 => Address::Sub { base, tail: Tail::Shape(Ref::parse(&shape).unwrap()) },
            _ => Address::SheetCell { sheet: base, at: c },
        };
        let text = a.to_string();
        let parsed: Address = text.parse().unwrap();
        // The normal form is stable: parsing what was printed gives the same address.
        prop_assert_eq!(parsed.to_string().parse::<Address>().unwrap(), parsed.clone());
        // A shape ref spelled like `R1C1` reads as a table cell (the reader reinterprets
        // it by node type), so only the other forms keep their exact text.
        if which != 3 {
            prop_assert_eq!(parsed.to_string(), text);
            prop_assert_eq!(parsed, a);
        }
    }

    #[test]
    fn revision_tokens_round_trip(rev in any::<u64>(), chain in "[0-9a-fA-F]{8,64}") {
        let t = RevToken::new(rev, Chain8::from_hex(&chain).unwrap());
        prop_assert_eq!(RevToken::parse_etag(&t.etag()).unwrap(), t.clone());
        prop_assert_eq!(t.to_string().parse::<RevToken>().unwrap(), t.clone());
        prop_assert_eq!(t.chain.as_str(), chain[..8].to_lowercase());
    }

    #[test]
    fn cursors_round_trip(rev in any::<u64>(), pos in any::<u64>(), aux in proptest::option::of(any::<u64>())) {
        let c = Cursor { rev, pos, aux };
        prop_assert_eq!(c.to_string().parse::<Cursor>().unwrap(), c);
    }

    #[test]
    fn short_hashes_round_trip(h in "[0-9a-fA-F]{6,64}") {
        let s = ShortHash::parse(&h).unwrap();
        prop_assert_eq!(s.as_str(), h.to_lowercase());
        prop_assert_eq!(ShortHash::parse(&format!("#{s}")).unwrap(), s);
    }

    #[test]
    fn a1_ranges_round_trip(a in cell(), b in cell(), sheet in sheet_name()) {
        let range = A1Range {
            sheet,
            from: CellRef { col: a.col.min(b.col), row: a.row.min(b.row) },
            to: CellRef { col: a.col.max(b.col), row: a.row.max(b.row) },
        };
        let text = range.to_string();
        prop_assert_eq!(text.parse::<A1Range>().unwrap(), range, "{}", text);
    }

    #[test]
    fn slide_selectors_round_trip(n in 1u32..=9999, r in ref_text()) {
        let p = SlideSel::Position(n);
        prop_assert_eq!(p.to_string().parse::<SlideSel>().unwrap(), p);
        let sel: SlideSel = r.parse().unwrap();
        prop_assert_eq!(sel.to_string().parse::<SlideSel>().unwrap(), sel);
    }
}

// ---------------------------------------------------------------- ref index laws

fn ids_strategy() -> impl Strategy<Value = Vec<NodeId>> {
    // A shared timestamp prefix, a random tail, and a few forced suffix collisions.
    proptest::collection::vec((crockford(16), 0..4usize), 1..60).prop_map(|v| {
        let mut out = BTreeSet::new();
        for (tail, collide) in v {
            let mut t = tail;
            if collide == 0 {
                t.replace_range(12..16, "ABCD");
            } else if collide == 1 {
                t.replace_range(8..16, "00000EF3");
            }
            out.insert(NodeId(format!("01K7Q9XM2A{t}")));
        }
        out.into_iter().collect()
    })
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn refs_are_minimal_unique_suffixes(ids in ids_strategy()) {
        let idx = RefIndex::build(&ids);
        for id in &ids {
            let r = idx.ref_of(id).unwrap();
            prop_assert!(r.as_str().len() >= 4);
            prop_assert!(id.0.ends_with(r.as_str()));
            // Unique: no other id ends with it.
            prop_assert_eq!(ids.iter().filter(|o| o.0.ends_with(r.as_str())).count(), 1);
            // Minimal: dropping the first character makes it ambiguous, unless at the floor.
            if r.as_str().len() > 4 {
                let shorter = &r.as_str()[1..];
                prop_assert!(ids.iter().filter(|o| o.0.ends_with(shorter)).count() > 1);
            }
            // It resolves back, in any case.
            prop_assert_eq!(idx.resolve(&r), Ok(id.clone()));
            prop_assert_eq!(idx.resolve(&Ref::parse(&r.as_str().to_lowercase()).unwrap()), Ok(id.clone()));
            // A full ULID always resolves.
            prop_assert_eq!(idx.resolve(&Ref::parse(&id.0).unwrap()), Ok(id.clone()));
        }
    }

    #[test]
    fn ambiguity_lists_resolving_candidates(ids in ids_strategy(), probe in "[0-9A-HJKMNP-TV-Z]{4}") {
        let idx = RefIndex::build(&ids);
        let r = Ref::parse(&probe).unwrap();
        let matching = ids.iter().filter(|i| i.0.ends_with(&probe)).count();
        match idx.resolve(&r) {
            Err(ResolveError::Unknown) => prop_assert_eq!(matching, 0),
            Ok(id) => {
                prop_assert_eq!(matching, 1);
                prop_assert!(id.0.ends_with(&probe));
            }
            Err(ResolveError::Ambiguous(c)) => {
                prop_assert!(matching > 1);
                prop_assert!(c.len() <= 5);
                let resolved: BTreeSet<_> = c.iter().map(|x| idx.resolve(x).unwrap()).collect();
                prop_assert_eq!(resolved.len(), c.len());
            }
        }
    }

    #[test]
    fn later_nodes_never_rebind_an_old_ref(old in ids_strategy(), new in ids_strategy()) {
        let old_idx = RefIndex::build(&old);
        let union: Vec<NodeId> = old.iter().chain(new.iter()).cloned().collect::<BTreeSet<_>>().into_iter().collect();
        let new_idx = RefIndex::build(&union);
        for id in &old {
            let r_old = old_idx.ref_of(id).unwrap();
            let r_new = new_idx.ref_of(id).unwrap();
            // A ref only gets longer as nodes appear, and the old ref still never points at a different node.
            prop_assert!(r_new.as_str().len() >= r_old.as_str().len());
            if let Ok(found) = new_idx.resolve(&r_old) {
                prop_assert_eq!(&found, id);
            }
            // Resolution at the old revision does not see nodes of the new one.
            prop_assert_eq!(old_idx.resolve(&r_old), Ok(id.clone()));
        }
    }
}

// ---------------------------------------------------------------- hash laws

fn json_value() -> impl Strategy<Value = Value> {
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::from),
        any::<i32>().prop_map(Value::from),
        "[a-z]{0,6}".prop_map(Value::from),
    ];
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            proptest::collection::vec(inner.clone(), 0..4).prop_map(Value::Array),
            proptest::collection::vec(("[a-d]{1,2}", inner), 0..4)
                .prop_map(|kv| Value::Object(kv.into_iter().collect())),
        ]
    })
}

fn reversed_keys(v: &Value) -> Value {
    match v {
        Value::Object(m) => {
            let mut keys: Vec<_> = m.keys().cloned().collect();
            keys.reverse();
            Value::Object(keys.into_iter().map(|k| (k.clone(), reversed_keys(&m[&k]))).collect())
        }
        Value::Array(a) => Value::Array(a.iter().map(reversed_keys).collect()),
        other => other.clone(),
    }
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn block_hash_is_the_fingerprint_and_ignores_key_order(v in json_value()) {
        let h = block_hash(&v);
        prop_assert_eq!(h.as_hex(), engine_core::richtext::fingerprint(&v));
        prop_assert_eq!(block_hash(&reversed_keys(&v)), h.clone());
        prop_assert_eq!(h.h6(), &h.as_hex()[..6]);
        prop_assert_eq!(h.h12(), &h.as_hex()[..12]);
        prop_assert_eq!(Hash::from_hex(h.as_hex()).unwrap(), h);
    }

    #[test]
    fn row_hash_depends_on_every_cell(cells in proptest::collection::vec("[a-z]{0,4}", 1..6), i in 0..6usize) {
        let h = row_hash(&cells);
        let mut changed = cells.clone();
        let i = i % cells.len();
        changed[i].push('x');
        prop_assert_ne!(row_hash(&changed), h.clone());
        // The hash is the fingerprint of the cell_N object.
        let obj: serde_json::Map<String, Value> = cells.iter().enumerate().map(|(k, c)| (format!("cell_{k}"), json!(c))).collect();
        prop_assert_eq!(h, Hash::of(&Value::Object(obj)));
    }
}
