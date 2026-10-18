//! `dynodoc.default.v1`: today's `workspace_merge::merge`, expressed as a policy.
//!
//! Three strategies, one per behaviour `select` has: a rich-text field merges by
//! hunks, `page_setup` merges key by key, everything else (`$node`, `$location`,
//! `var_name` and every other field) is a whole-value three-way choice. Each calls
//! the code `select` calls, so the policy changes no result; what it adds is that
//! the choice of rule is data and every decision is recorded.

use super::{
    policy::MergePolicy,
    registry::Registry,
    strategy::{Context, Inputs, Laws, Reason, Resolved, Strategy},
};
use serde_json::Value;
use std::sync::OnceLock;

/// The policy text. It is loaded through the ordinary loader, so it is validated
/// like any other policy and its hash is pinned by `fixtures/merge-policy-v1.json`.
pub const POLICY_JSON: &str = r#"{
  "version": 1,
  "id": "dynodoc.default.v1",
  "description": "Reproduces workspace_merge::merge: rich text and page setup merge by their own rules; every other overlap is a conflict.",
  "defaults": {
    "unknown": "manual",
    "loser": "none",
    "roles_rank": ["owner", "manager", "editor", "contributor", "reviewer"],
    "invariants": "flag"
  },
  "rules": [
    {
      "id": "legacy-content",
      "match": {"field": "content"},
      "strategy": "dynodoc.default.richtext.v1",
      "gate": "auto"
    },
    {
      "id": "legacy-page-setup",
      "match": {"field": "page_setup"},
      "strategy": "dynodoc.default.property.v1",
      "gate": "auto"
    },
    {
      "id": "legacy-whole-value",
      "match": {},
      "strategy": "dynodoc.default.whole-value.v1",
      "gate": "auto"
    }
  ],
  "floors": {"gate_at_least": {}}
}"#;

/// The default policy.
pub fn policy() -> &'static MergePolicy {
    static POLICY: OnceLock<MergePolicy> = OnceLock::new();
    POLICY.get_or_init(|| {
        MergePolicy::from_json(POLICY_JSON.as_bytes(), &Registry::builtin())
            .expect("the built-in default policy is valid")
    })
}

pub(super) fn strategies() -> Vec<Box<dyn Strategy>> {
    vec![
        Box::new(WholeValue),
        Box::new(RichText),
        Box::new(PropertyMerge),
    ]
}

/// The shortcuts every three-way choice takes first (`select`): equal sides or a
/// side that did not change.
fn unchanged_side(i: &Inputs<'_>) -> Option<Resolved> {
    if i.draft == i.base || i.draft == i.team {
        Some(Resolved::merged(i.team.clone(), "unchanged-or-team-only"))
    } else if i.team == i.base {
        Some(Resolved::merged(i.draft.clone(), "draft-only"))
    } else {
        None
    }
}

/// A three-way choice of the whole value: two different changes are a conflict.
pub struct WholeValue;

impl Strategy for WholeValue {
    fn id(&self) -> &'static str {
        "dynodoc.default.whole-value.v1"
    }
    fn resolve(&self, i: &Inputs<'_>, _: &Context<'_>) -> Resolved {
        unchanged_side(i).unwrap_or_else(|| Resolved::conflict(Reason::BOTH_CHANGED, "whole-value"))
    }
}

/// `richtext::merge`: non-overlapping hunks of an inline container, with the
/// marks of each run merged property by property.
pub struct RichText;

impl Strategy for RichText {
    fn id(&self) -> &'static str {
        "dynodoc.default.richtext.v1"
    }
    /// Symmetry holds only up to the order of marks on a run: when both sides add a
    /// mark to the same run, the merged marks are listed in the team's order first
    /// (`richtext::merge_marks`). The set of marks is the same either way, which
    /// `tests/merge_policy_laws_test.rs` checks; `convergence` hides the order by
    /// merging from one canonical orientation. Frozen behaviour, so it is not a
    /// promise here.
    fn promises(&self) -> Laws {
        Laws::ALL.without(Laws::SYMMETRY)
    }
    fn resolve(&self, i: &Inputs<'_>, _: &Context<'_>) -> Resolved {
        if let Some(done) = unchanged_side(i) {
            return done;
        }
        match crate::richtext::merge(i.base, i.team, i.draft) {
            Some(merged) => Resolved::merged(merged, "richtext-hunks"),
            None => Resolved::conflict(Reason::OVERLAP, "richtext-hunks"),
        }
    }
}

/// `richtext::merge_property`: independent keys of an object merge key by key.
pub struct PropertyMerge;

impl Strategy for PropertyMerge {
    fn id(&self) -> &'static str {
        "dynodoc.default.property.v1"
    }
    fn resolve(&self, i: &Inputs<'_>, _: &Context<'_>) -> Resolved {
        if let Some(done) = unchanged_side(i) {
            return done;
        }
        let base: Option<&Value> = (!i.base.is_null()).then_some(i.base);
        match crate::richtext::merge_property(base, Some(i.team), Some(i.draft)) {
            Ok(Some(merged)) => Resolved::merged(merged, "property-keys"),
            _ => Resolved::conflict(Reason::OVERLAP, "property-keys"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge_policy::{
        policy::{Probe, Unknown},
        strategy::Outcome,
        Family, Gate,
    };
    use serde_json::json;

    fn probe<'a>(field: &'a str) -> Probe<'a> {
        Probe {
            family: Family::Document,
            node_type: "item",
            block_type: None,
            field,
            aspect: None,
        }
    }

    #[test]
    fn the_default_policy_is_valid_and_routes_by_field() {
        let p = policy();
        assert_eq!(p.id, "dynodoc.default.v1");
        assert_eq!(p.defaults.unknown, Unknown::Manual);
        let strategy = |field| p.find_rule(&probe(field)).unwrap().strategy.as_str();
        assert_eq!(strategy("content"), "dynodoc.default.richtext.v1");
        assert_eq!(strategy("page_setup"), "dynodoc.default.property.v1");
        for other in ["label", "$node", "$location", "var_name", "anything"] {
            assert_eq!(strategy(other), "dynodoc.default.whole-value.v1", "{other}");
        }
        assert!(p.rules.iter().all(|r| p.effective_gate(r) == Gate::Auto));
        // The collections have no rule, so they keep their whole-collection choice.
        assert!(p
            .find_rule(&Probe {
                aspect: Some(crate::merge_policy::Aspect::Comments),
                ..probe("$comments")
            })
            .is_none());
    }

    #[test]
    fn the_default_policy_hash_is_stable_across_calls() {
        assert_eq!(policy().hash(), policy().hash());
        assert_eq!(
            MergePolicy::from_json(POLICY_JSON.as_bytes(), &Registry::builtin())
                .unwrap()
                .canonical_bytes(),
            policy().canonical_bytes()
        );
    }

    fn run(strategy: &dyn Strategy, field: &str, b: Value, t: Value, d: Value) -> Outcome {
        let params = Default::default();
        let ctx = Context {
            node: None,
            node_type: "item",
            field,
            aspect: None,
            family: Family::Document,
            params: &params,
        };
        strategy
            .resolve(
                &Inputs {
                    base: &b,
                    team: &t,
                    draft: &d,
                },
                &ctx,
            )
            .outcome
    }

    #[test]
    fn whole_value_takes_the_changed_side_and_conflicts_on_two_changes() {
        let s = WholeValue;
        let merged = |v: Value| Outcome::Merged(v);
        assert_eq!(
            run(&s, "label", json!("a"), json!("a"), json!("b")),
            merged(json!("b"))
        );
        assert_eq!(
            run(&s, "label", json!("a"), json!("b"), json!("a")),
            merged(json!("b"))
        );
        assert_eq!(
            run(&s, "label", json!("a"), json!("b"), json!("b")),
            merged(json!("b"))
        );
        assert_eq!(
            run(&s, "label", json!("a"), json!("b"), json!("c")),
            Outcome::Conflict(Reason::BOTH_CHANGED)
        );
    }

    #[test]
    fn page_setup_merges_independent_keys_and_conflicts_on_one_key() {
        let s = PropertyMerge;
        let b = json!({"size":"A4","margin":25});
        let t = json!({"size":"A4","margin":20});
        let d = json!({"size":"A4","margin":25,"landscape":true});
        assert_eq!(
            run(&s, "page_setup", b.clone(), t.clone(), d),
            Outcome::Merged(json!({"size":"A4","margin":20,"landscape":true}))
        );
        let clash = json!({"size":"A4","margin":30});
        assert_eq!(
            run(&s, "page_setup", b, t, clash),
            Outcome::Conflict(Reason::OVERLAP)
        );
    }

    #[test]
    fn richtext_merges_disjoint_hunks_and_conflicts_on_overlap() {
        let para = |text: &str| json!({"type":"paragraph","content":[{"type":"text","text":text}]});
        let s = RichText;
        let sentence = "The participants were interviewed in June.";
        let left = sentence.replace("participants", "respondents");
        let right = sentence.replace("June", "July");
        match run(&s, "content", para(sentence), para(&left), para(&right)) {
            Outcome::Merged(v) => {
                assert_eq!(
                    crate::richtext::plain_text(&v),
                    "The respondents were interviewed in July."
                );
            }
            other => panic!("{other:?}"),
        }
        let clash = sentence.replace("participants", "subjects");
        assert_eq!(
            run(&s, "content", para(sentence), para(&left), para(&clash)),
            Outcome::Conflict(Reason::OVERLAP)
        );
    }
}
