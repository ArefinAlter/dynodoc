//! `dynodoc.collection.element-wise.v1`: comments and suggestions merged element by
//! element instead of as one value (design 4.8). Today `convergence` takes a whole
//! collection from one side and conflicts when both sides changed it, so two
//! reviewers adding a comment each make the file unclean.
//!
//! * Suggestions have an identifier: each is decided on its own, and only the same
//!   identifier changed differently on both sides is a conflict.
//! * A comment has no identifier. Its identity here is its whole content (target and
//!   body), counted as a multiset: an addition on either side is kept once per
//!   occurrence the larger addition has, a removal on both sides is one removal.
//!   The result is the base comments in base order, then new ones in content order,
//!   so it depends on neither side's order.
//!
//! It is opt-in: no rule of `dynodoc.default.v1` selects it, because it changes the
//! merged result and a v3 commit cannot record a strategy other than
//! `dynodoc.same-file.node-field.v1`.

use super::{
    strategy::{Context, Inputs, Reason, Resolved, Strategy},
    Aspect,
};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

const BOUND: usize = 1 << 20;

pub struct ElementWise;

/// The value a three-way choice settles on, `None` inside meaning "absent".
fn settle<'a>(
    base: Option<&'a Value>,
    team: Option<&'a Value>,
    draft: Option<&'a Value>,
) -> Option<Option<&'a Value>> {
    if draft == base || draft == team {
        Some(team)
    } else if team == base {
        Some(draft)
    } else {
        None
    }
}

fn size(v: &Value) -> usize {
    serde_json::to_vec(v).map_or(usize::MAX, |b| b.len())
}

impl Strategy for ElementWise {
    fn id(&self) -> &'static str {
        "dynodoc.collection.element-wise.v1"
    }
    fn aspects(&self) -> &'static [Aspect] {
        &[Aspect::Comments, Aspect::Suggestions]
    }
    fn max_input_bytes(&self) -> Option<usize> {
        Some(BOUND)
    }
    fn resolve(&self, i: &Inputs<'_>, ctx: &Context<'_>) -> Resolved {
        if i.draft == i.base || i.draft == i.team {
            return Resolved::merged(i.team.clone(), "unchanged-or-team-only");
        }
        if i.team == i.base {
            return Resolved::merged(i.draft.clone(), "draft-only");
        }
        if [i.base, i.team, i.draft].iter().any(|v| size(v) > BOUND) {
            return Resolved::conflict(Reason::TOO_LARGE, "bound");
        }
        match ctx.aspect {
            Some(Aspect::Suggestions) => suggestions(i),
            Some(Aspect::Comments) => comments(i),
            _ => Resolved::conflict(Reason::UNSUPPORTED_SHAPE, "aspect"),
        }
    }
}

fn suggestions(i: &Inputs<'_>) -> Resolved {
    let (Some(b), Some(t), Some(d)) = (i.base.as_object(), i.team.as_object(), i.draft.as_object())
    else {
        return Resolved::conflict(Reason::UNSUPPORTED_SHAPE, "suggestions");
    };
    let mut out = Map::new();
    let mut differ = Vec::new();
    for key in b.keys().chain(t.keys()).chain(d.keys()) {
        if out.contains_key(key) || differ.contains(key) {
            continue;
        }
        match settle(b.get(key), t.get(key), d.get(key)) {
            Some(Some(value)) => {
                out.insert(key.clone(), value.clone());
            }
            Some(None) => {}
            None => differ.push(key.clone()),
        }
    }
    if differ.is_empty() {
        return Resolved::merged(Value::Object(out), "suggestions-by-id");
    }
    differ.sort();
    Resolved::conflict(Reason::ELEMENT_DIFFERS, "suggestions-by-id").with_notes(
        differ
            .into_iter()
            .map(|id| format!("suggestion `{id}` was changed differently on both sides"))
            .collect(),
    )
}

/// `(count in base, count in team, count in draft)` of one comment content.
type Counts = (i64, i64, i64);

fn comments(i: &Inputs<'_>) -> Resolved {
    let (Some(b), Some(t), Some(d)) = (i.base.as_array(), i.team.as_array(), i.draft.as_array())
    else {
        return Resolved::conflict(Reason::UNSUPPORTED_SHAPE, "comments");
    };
    let key = |v: &Value| String::from_utf8_lossy(&crate::log::canonical_json(v)).into_owned();
    let mut counts: BTreeMap<String, (Counts, &Value)> = BTreeMap::new();
    for (side, items) in [b, t, d].into_iter().enumerate() {
        for item in items {
            let entry = counts.entry(key(item)).or_insert(((0, 0, 0), item));
            match side {
                0 => entry.0 .0 += 1,
                1 => entry.0 .1 += 1,
                _ => entry.0 .2 += 1,
            }
        }
    }
    // How many of each content the result holds.
    let mut want: BTreeMap<&str, i64> = BTreeMap::new();
    for (k, ((b, t, d), _)) in &counts {
        let (dt, dd) = (t - b, d - b);
        let delta = if dt >= 0 && dd >= 0 {
            dt.max(dd)
        } else if dt <= 0 && dd <= 0 {
            dt.min(dd)
        } else {
            dt + dd
        };
        want.insert(k, b + delta);
    }
    let mut out = Vec::new();
    for item in b {
        let left = want.get_mut(key(item).as_str());
        if let Some(n) = left.filter(|n| **n > 0) {
            *n -= 1;
            out.push(item.clone());
        }
    }
    for (k, n) in want {
        for _ in 0..n.max(0) {
            out.push(counts[k].1.clone());
        }
    }
    Resolved::merged(Value::Array(out), "comments-multiset")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge_policy::Family;
    use serde_json::json;

    fn run(aspect: Aspect, b: Value, t: Value, d: Value) -> Resolved {
        let params = Default::default();
        let ctx = Context {
            node: None,
            node_type: "",
            field: "$collection",
            aspect: Some(aspect),
            family: Family::Document,
            params: &params,
        };
        ElementWise.resolve(
            &Inputs {
                base: &b,
                team: &t,
                draft: &d,
            },
            &ctx,
        )
    }

    fn comment(body: &str) -> Value {
        json!({"node_id": null, "body": body})
    }

    #[test]
    fn two_reviewers_adding_comments_keep_both_in_either_order() {
        let base = json!([comment("old")]);
        let team = json!([comment("old"), comment("zed")]);
        let draft = json!([comment("old"), comment("alpha")]);
        let expected = json!([comment("old"), comment("alpha"), comment("zed")]);
        for (t, d) in [(&team, &draft), (&draft, &team)] {
            let r = run(Aspect::Comments, base.clone(), t.clone(), d.clone());
            assert_eq!(r.outcome, super::super::Outcome::Merged(expected.clone()));
        }
    }

    #[test]
    fn the_same_comment_added_on_both_sides_is_kept_once_and_removals_count_once() {
        let base = json!([comment("a"), comment("b")]);
        let both = run(
            Aspect::Comments,
            base.clone(),
            json!([comment("a"), comment("b"), comment("n")]),
            json!([comment("a"), comment("b"), comment("n"), comment("m")]),
        );
        assert_eq!(
            both.outcome,
            super::super::Outcome::Merged(json!([
                comment("a"),
                comment("b"),
                comment("m"),
                comment("n")
            ]))
        );
        // Both removed `a`; one also added `c`.
        let removed = run(
            Aspect::Comments,
            base,
            json!([comment("b")]),
            json!([comment("b"), comment("c")]),
        );
        assert_eq!(
            removed.outcome,
            super::super::Outcome::Merged(json!([comment("b"), comment("c")]))
        );
        // One side removed a comment the other added again: net unchanged.
        let net = run(
            Aspect::Comments,
            json!([comment("a")]),
            json!([]),
            json!([comment("a"), comment("a")]),
        );
        assert_eq!(
            net.outcome,
            super::super::Outcome::Merged(json!([comment("a")]))
        );
    }

    fn suggestion(n: i64) -> Value {
        json!({"target_node_id": null, "detail": {"n": n}, "accepted": false, "rejected_reason": null})
    }

    #[test]
    fn suggestions_conflict_only_when_the_same_id_changed_differently() {
        let base = json!({"s0": suggestion(0)});
        let team = json!({"s0": suggestion(0), "s1": suggestion(1)});
        let draft = json!({"s0": suggestion(0), "s2": suggestion(2)});
        let r = run(
            Aspect::Suggestions,
            base.clone(),
            team.clone(),
            draft.clone(),
        );
        assert_eq!(
            r.outcome,
            super::super::Outcome::Merged(
                json!({"s0": suggestion(0), "s1": suggestion(1), "s2": suggestion(2)})
            )
        );
        // The same id with different content on both sides.
        let clash = json!({"s0": suggestion(0), "s1": suggestion(9)});
        let r = run(Aspect::Suggestions, base.clone(), team.clone(), clash);
        assert_eq!(
            r.outcome,
            super::super::Outcome::Conflict(Reason::ELEMENT_DIFFERS)
        );
        assert_eq!(
            r.used.notes,
            ["suggestion `s1` was changed differently on both sides"]
        );
        // One side accepted s0, the other removed it: that is a conflict too.
        let accepted = json!({"s0": {"target_node_id": null, "detail": {"n": 0}, "accepted": true, "rejected_reason": null}});
        let r = run(Aspect::Suggestions, base, accepted, json!({}));
        assert_eq!(
            r.outcome,
            super::super::Outcome::Conflict(Reason::ELEMENT_DIFFERS)
        );
    }

    #[test]
    fn shape_size_and_aspect_are_checked() {
        let r = run(Aspect::Comments, json!([]), json!({"a": 1}), json!([1]));
        assert_eq!(
            r.outcome,
            super::super::Outcome::Conflict(Reason::UNSUPPORTED_SHAPE)
        );
        let big = "x".repeat(BOUND + 1);
        let r = run(
            Aspect::Comments,
            json!([]),
            json!([big]),
            json!([comment("a")]),
        );
        assert_eq!(
            r.outcome,
            super::super::Outcome::Conflict(Reason::TOO_LARGE)
        );
        let r = run(Aspect::Marks, json!([]), json!([1]), json!([2]));
        assert_eq!(
            r.outcome,
            super::super::Outcome::Conflict(Reason::UNSUPPORTED_SHAPE)
        );
    }
}
