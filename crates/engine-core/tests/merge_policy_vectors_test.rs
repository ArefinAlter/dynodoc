//! Reproduces `fixtures/merge-policy-v1.json` byte for byte. The fixture is written
//! by `fixtures/gen_merge_policy_vectors.py`, an independent pure-Python
//! implementation of the canonical form and hash; never regenerate it to make one of
//! these checks pass. Regeneration must be a no-op:
//! `python -I gen_merge_policy_vectors.py` followed by `git diff --exit-code`.
use engine_core::merge_policy::{
    default,
    policy::{HASH_DOMAIN, MAX_CANONICAL_BYTES},
    strategy::{ParamSpec, ParamType},
    Context, Inputs, MergePolicy, Registry, Resolved, Strategy,
};
use ring::digest::{digest, SHA256};
use serde_json::Value;

/// The strategy the vectors name for parameters: declared here, not in the engine.
struct ParamsStrategy;
impl Strategy for ParamsStrategy {
    fn id(&self) -> &'static str {
        "test.params.v1"
    }
    fn params(&self) -> &'static [ParamSpec] {
        &[
            ParamSpec {
                name: "flag",
                ty: ParamType::Bool,
            },
            ParamSpec {
                name: "count",
                ty: ParamType::Int { min: 1, max: 200 },
            },
            ParamSpec {
                name: "mode",
                ty: ParamType::OneOf(&["dedupe", "keep"]),
            },
        ]
    }
    fn resolve(&self, _: &Inputs<'_>, _: &Context<'_>) -> Resolved {
        unreachable!("the vectors only load policies")
    }
}

fn fixture() -> Value {
    serde_json::from_str(include_str!("fixtures/merge-policy-v1.json")).unwrap()
}

fn registry() -> Registry {
    let mut registry = Registry::builtin();
    registry.register(Box::new(ParamsStrategy)).unwrap();
    registry
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn every_vector_loads_to_the_recorded_canonical_form_and_hash() {
    let f = fixture();
    assert_eq!(f["schema"], "dynodoc.merge-policy.vectors.v1");
    assert_eq!(f["domain_hex"], hex(HASH_DOMAIN));
    let registry = registry();
    let vectors = f["vectors"].as_array().unwrap();
    assert!(vectors.len() >= 6);
    for v in vectors {
        let name = v["name"].as_str().unwrap();
        let policy = MergePolicy::from_json(v["input"].as_str().unwrap().as_bytes(), &registry)
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        let canonical = v["canonical"].as_str().unwrap();
        assert_eq!(
            String::from_utf8(policy.canonical_bytes()).unwrap(),
            canonical,
            "{name}"
        );
        assert!(canonical.len() <= MAX_CANONICAL_BYTES, "{name}");
        // The hash again, from first principles rather than the module's method.
        let mut input = HASH_DOMAIN.to_vec();
        input.extend(canonical.as_bytes());
        let expected = v["sha256"].as_str().unwrap();
        assert_eq!(hex(digest(&SHA256, &input).as_ref()), expected, "{name}");
        assert_eq!(policy.hash_hex(), expected, "{name}");
    }
}

#[test]
fn respelled_policies_share_a_canonical_form_and_distinct_ones_do_not() {
    let f = fixture();
    let vectors = f["vectors"].as_array().unwrap();
    let by_name = |name: &str| vectors.iter().find(|v| v["name"] == name).unwrap();
    let mut pairs = 0;
    for v in vectors.iter().filter(|v| v.get("same_as").is_some()) {
        let other = by_name(v["same_as"].as_str().unwrap());
        assert_eq!(v["canonical"], other["canonical"]);
        assert_eq!(v["sha256"], other["sha256"]);
        assert_ne!(v["input"], other["input"], "a respelling must differ in text");
        pairs += 1;
    }
    assert!(pairs >= 1);
    let mut hashes: Vec<_> = vectors
        .iter()
        .filter(|v| v.get("same_as").is_none())
        .map(|v| v["sha256"].as_str().unwrap())
        .collect();
    let all = hashes.len();
    hashes.sort_unstable();
    hashes.dedup();
    assert_eq!(hashes.len(), all);
}

#[test]
fn the_default_policy_is_the_recorded_vector() {
    let f = fixture();
    let v = f["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["name"] == "default")
        .unwrap();
    // The generator carries its own copy of the text; the two must not drift.
    assert_eq!(v["input"].as_str().unwrap(), default::POLICY_JSON);
    assert_eq!(default::policy().hash_hex(), v["sha256"].as_str().unwrap());
    assert_eq!(
        String::from_utf8(default::policy().canonical_bytes()).unwrap(),
        v["canonical"].as_str().unwrap()
    );
}
