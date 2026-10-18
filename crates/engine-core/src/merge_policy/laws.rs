//! A harness that holds a [`Strategy`] to the laws of design section 4.1.
//!
//! Give it a strategy, the context to run it in and a stream of `(base, team,
//! draft)` triples; it checks each promised law on every triple and stops at the
//! first violation with the triple that broke it. It is test support, public so
//! that the strategies of later phases are checked by the same code.
//!
//! | Law | What is checked |
//! |---|---|
//! | L1 symmetry | `resolve(b, t, d)` and `resolve(b, d, t)` agree: both merge to one value, both prefer one value, or both decline for the same reason |
//! | L2 idempotence | `resolve(b, x, x)` merges to `x` |
//! | L3 identity | `resolve(b, b, x)` and `resolve(b, x, b)` merge to `x` |
//! | L4 totality | no panic on any triple |
//! | L5 determinism | two runs on one triple give equal results |
//! | L6 invariants | if all three inputs satisfy the caller's `valid`, a selected value does |
//! | L7 no silent loss | a preferred side reports the other side's value as lost |
//! | L8 bounded | an input over `max_input_bytes` is declined with `too-large` |
//!
//! The harness cannot judge L7 for a merged value (that needs knowledge of the
//! strategy's data) or the cost bound of L8 beyond the declared size; those stay
//! with the strategy's own tests.

use super::{
    strategy::{selected, Context, Inputs, Laws, Outcome, Reason, Resolved, Strategy},
    Aspect, Family,
};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    panic::{catch_unwind, AssertUnwindSafe},
};

#[derive(Clone, Debug)]
pub struct Triple {
    pub base: Value,
    pub team: Value,
    pub draft: Value,
}

/// The context every case runs in.
pub struct Setup<'a> {
    pub node_type: &'a str,
    pub field: &'a str,
    pub aspect: Option<Aspect>,
    pub family: Family,
    pub params: BTreeMap<String, Value>,
    /// The invariant a valid value of the field satisfies, for L6.
    pub valid: Option<&'a dyn Fn(&Value) -> bool>,
}

impl Setup<'_> {
    /// A node field of a text document with no parameters and no invariant.
    pub fn field(field: &'static str) -> Setup<'static> {
        Setup {
            node_type: "item",
            field,
            aspect: None,
            family: Family::Document,
            params: BTreeMap::new(),
            valid: None,
        }
    }
}

#[derive(Debug)]
pub struct Violation {
    pub law: &'static str,
    pub detail: String,
    pub case: Triple,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub cases: usize,
    pub merged: usize,
    pub preferred: usize,
    pub conflicts: usize,
    /// The laws that were checked, by name.
    pub checked: Vec<&'static str>,
    /// Laws the strategy does not promise, or that cannot apply to it.
    pub skipped: Vec<&'static str>,
}

/// Outcomes compared by what they select, so side labels do not matter.
#[derive(Debug, PartialEq)]
enum Shape {
    Merged(Value),
    Prefer(Value),
    Conflict(Reason),
}

fn shape(outcome: &Outcome, inputs: &Inputs<'_>) -> Shape {
    match outcome {
        Outcome::Merged(v) => Shape::Merged(v.clone()),
        Outcome::Prefer(side) => Shape::Prefer(inputs.side(*side).clone()),
        Outcome::Conflict(reason) => Shape::Conflict(*reason),
    }
}

fn run(strategy: &dyn Strategy, setup: &Setup<'_>, t: &Triple) -> Result<Resolved, ()> {
    let ctx = Context {
        node: None,
        node_type: setup.node_type,
        field: setup.field,
        aspect: setup.aspect,
        family: setup.family,
        params: &setup.params,
    };
    let inputs = Inputs {
        base: &t.base,
        team: &t.team,
        draft: &t.draft,
    };
    catch_unwind(AssertUnwindSafe(|| strategy.resolve(&inputs, &ctx))).map_err(|_| ())
}

const LAWS: [(&str, Laws); 8] = [
    ("L1 symmetry", Laws::SYMMETRY),
    ("L2 idempotence", Laws::IDEMPOTENCE),
    ("L3 identity", Laws::IDENTITY),
    ("L4 totality", Laws::TOTALITY),
    ("L5 determinism", Laws::DETERMINISM),
    ("L6 invariants", Laws::INVARIANTS),
    ("L7 no silent loss", Laws::NO_SILENT_LOSS),
    ("L8 bounded", Laws::BOUNDED),
];

/// Check every law the strategy promises on every case.
pub fn check(
    strategy: &dyn Strategy,
    setup: &Setup<'_>,
    cases: impl IntoIterator<Item = Triple>,
) -> Result<Report, Box<Violation>> {
    let promises = strategy.promises();
    let fail = |law, detail: String, case: &Triple| -> Box<Violation> {
        Box::new(Violation {
            law,
            detail,
            case: case.clone(),
        })
    };
    let mut report = Report::default();
    let bound = strategy
        .max_input_bytes()
        .filter(|_| promises.contains(Laws::BOUNDED));

    if let Some(limit) = bound {
        let big = |tail: &str| Value::String(format!("{}{tail}", "x".repeat(limit)));
        let case = Triple {
            base: Value::String(String::new()),
            team: big("a"),
            draft: big("b"),
        };
        let r = run(strategy, setup, &case)
            .map_err(|()| fail("L4 totality", "panic on an oversize input".into(), &case))?;
        if r.outcome != Outcome::Conflict(Reason::TOO_LARGE) {
            return Err(fail(
                "L8 bounded",
                format!(
                    "an input over {limit} bytes gave {:?}, not too-large",
                    r.outcome
                ),
                &case,
            ));
        }
    }

    for case in cases {
        report.cases += 1;
        let inputs = Inputs {
            base: &case.base,
            team: &case.team,
            draft: &case.draft,
        };
        let resolved = match run(strategy, setup, &case) {
            Ok(r) => r,
            Err(()) if promises.contains(Laws::TOTALITY) => {
                return Err(fail("L4 totality", "the strategy panicked".into(), &case));
            }
            Err(()) => continue,
        };
        match resolved.outcome {
            Outcome::Merged(_) => report.merged += 1,
            Outcome::Prefer(_) => report.preferred += 1,
            Outcome::Conflict(_) => report.conflicts += 1,
        }

        if promises.contains(Laws::DETERMINISM) {
            match run(strategy, setup, &case) {
                Ok(again) if again == resolved => {}
                other => {
                    return Err(fail(
                        "L5 determinism",
                        format!("a second run gave {other:?}, the first {resolved:?}"),
                        &case,
                    ));
                }
            }
        }

        if promises.contains(Laws::SYMMETRY) {
            let swapped = Triple {
                base: case.base.clone(),
                team: case.draft.clone(),
                draft: case.team.clone(),
            };
            match run(strategy, setup, &swapped) {
                Ok(other) => {
                    let (a, b) = (
                        shape(&resolved.outcome, &inputs),
                        shape(&other.outcome, &inputs.swapped()),
                    );
                    if a != b {
                        return Err(fail(
                            "L1 symmetry",
                            format!("{a:?} against {b:?} with the sides swapped"),
                            &case,
                        ));
                    }
                }
                Err(()) => {
                    return Err(fail(
                        "L4 totality",
                        "panic with the sides swapped".into(),
                        &swapped,
                    ))
                }
            }
        }

        let merges_to = |t: &Triple, expected: &Value| -> Result<bool, ()> {
            run(strategy, setup, t)
                .map(|r| matches!(&r.outcome, Outcome::Merged(v) if v == expected))
        };
        for x in [&case.team, &case.draft] {
            if promises.contains(Laws::IDEMPOTENCE) {
                let t = Triple {
                    base: case.base.clone(),
                    team: x.clone(),
                    draft: x.clone(),
                };
                if merges_to(&t, x) != Ok(true) {
                    return Err(fail(
                        "L2 idempotence",
                        "resolve(b, x, x) is not Merged(x)".into(),
                        &t,
                    ));
                }
            }
            if promises.contains(Laws::IDENTITY) {
                for t in [
                    Triple {
                        base: case.base.clone(),
                        team: case.base.clone(),
                        draft: x.clone(),
                    },
                    Triple {
                        base: case.base.clone(),
                        team: x.clone(),
                        draft: case.base.clone(),
                    },
                ] {
                    if merges_to(&t, x) != Ok(true) {
                        return Err(fail(
                            "L3 identity",
                            "resolve(b, b, x) or resolve(b, x, b) is not Merged(x)".into(),
                            &t,
                        ));
                    }
                }
            }
        }

        if let (true, Some(valid)) = (promises.contains(Laws::INVARIANTS), setup.valid) {
            let all_valid = valid(&case.base) && valid(&case.team) && valid(&case.draft);
            if let (true, Some(value)) = (all_valid, selected(&resolved.outcome, &inputs)) {
                if !valid(value) {
                    return Err(fail(
                        "L6 invariants",
                        format!("valid inputs gave the invalid value {value}"),
                        &case,
                    ));
                }
            }
        }

        if promises.contains(Laws::NO_SILENT_LOSS) {
            if let Outcome::Prefer(side) = resolved.outcome {
                let loser = inputs.side(side.other());
                if !resolved.used.lost.contains(loser) {
                    return Err(fail(
                        "L7 no silent loss",
                        format!(
                            "the {:?} side lost and the evidence does not carry its value",
                            side.other()
                        ),
                        &case,
                    ));
                }
            }
        }
    }

    for (name, law) in LAWS {
        let applies = promises.contains(law)
            && match law {
                Laws::BOUNDED => bound.is_some(),
                Laws::INVARIANTS => setup.valid.is_some(),
                _ => true,
            };
        if applies {
            report.checked.push(name);
        } else {
            report.skipped.push(name);
        }
    }
    Ok(report)
}

/// A small deterministic generator (xorshift) for building cases.
pub struct Rng(pub u64);

impl Rng {
    pub fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n.max(1) as u64) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::merge_policy::strategy::{ParamSpec, Side};
    use serde_json::json;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn cases() -> Vec<Triple> {
        let vals = [json!(1), json!(2), json!(3), json!("a"), Value::Null];
        let mut out = Vec::new();
        for b in &vals {
            for t in &vals {
                for d in &vals {
                    out.push(Triple {
                        base: b.clone(),
                        team: t.clone(),
                        draft: d.clone(),
                    });
                }
            }
        }
        out
    }

    /// The shortcuts every correct strategy takes, so a test strategy can break
    /// exactly one law.
    fn shortcut(i: &Inputs<'_>) -> Option<Resolved> {
        if i.draft == i.base || i.draft == i.team {
            Some(Resolved::merged(i.team.clone(), "shortcut"))
        } else if i.team == i.base {
            Some(Resolved::merged(i.draft.clone(), "shortcut"))
        } else {
            None
        }
    }

    macro_rules! strategy {
        ($name:ident, $laws:expr, $body:expr) => {
            struct $name;
            impl Strategy for $name {
                fn id(&self) -> &'static str {
                    "test.law.v1"
                }
                fn params(&self) -> &'static [ParamSpec] {
                    &[]
                }
                fn promises(&self) -> Laws {
                    $laws
                }
                fn resolve(&self, i: &Inputs<'_>, _: &Context<'_>) -> Resolved {
                    if let Some(done) = shortcut(i) {
                        return done;
                    }
                    let body: fn(&Inputs<'_>) -> Resolved = $body;
                    body(i)
                }
            }
        };
    }

    strategy!(Sound, Laws::ALL, |_| Resolved::conflict(
        Reason::BOTH_CHANGED,
        "x"
    ));
    strategy!(Asymmetric, Laws::ALL, |i| Resolved::merged(
        i.team.clone(),
        "team wins"
    ));
    strategy!(Panics, Laws::ALL, |_| panic!("boom"));
    // Prefers the side whose text sorts first, so only the missing evidence is wrong.
    strategy!(Forgets, Laws::ALL, |i| {
        let (team, draft) = (i.team.to_string(), i.draft.to_string());
        Resolved {
            outcome: Outcome::Prefer(if team <= draft {
                Side::Team
            } else {
                Side::Draft
            }),
            used: crate::merge_policy::Used {
                basis: "prefer",
                ..Default::default()
            },
        }
    });
    strategy!(Invalid, Laws::ALL, |_| Resolved::merged(
        Value::Null,
        "null"
    ));

    fn setup() -> Setup<'static> {
        Setup::field("label")
    }

    #[test]
    fn a_sound_strategy_passes_and_the_report_counts_what_it_saw() {
        let report = check(&Sound, &setup(), cases()).unwrap();
        assert_eq!(report.cases, 125);
        assert_eq!(report.merged + report.conflicts, 125);
        assert!(report.merged > 0 && report.conflicts > 0 && report.preferred == 0);
        assert!(report.checked.contains(&"L1 symmetry"));
        // Nothing is bounded or constrained here, so those laws say they were skipped.
        assert_eq!(report.skipped, ["L6 invariants", "L8 bounded"]);
    }

    #[test]
    fn each_law_catches_a_strategy_that_breaks_it() {
        let law = |r: Result<Report, Box<Violation>>| r.unwrap_err().law;
        assert_eq!(law(check(&Asymmetric, &setup(), cases())), "L1 symmetry");
        assert_eq!(law(check(&Panics, &setup(), cases())), "L4 totality");
        assert_eq!(law(check(&Forgets, &setup(), cases())), "L7 no silent loss");
        let valid = |v: &Value| !v.is_null();
        let s = Setup {
            valid: Some(&valid),
            ..setup()
        };
        assert_eq!(law(check(&Invalid, &s, cases())), "L6 invariants");
    }

    #[test]
    fn idempotence_and_identity_are_checked_on_their_own() {
        // Without the shortcuts these laws fail even though symmetry holds.
        struct NoShortcuts;
        impl Strategy for NoShortcuts {
            fn id(&self) -> &'static str {
                "test.law.v1"
            }
            fn promises(&self) -> Laws {
                Laws::IDEMPOTENCE
            }
            fn resolve(&self, _: &Inputs<'_>, _: &Context<'_>) -> Resolved {
                Resolved::conflict(Reason::BOTH_CHANGED, "x")
            }
        }
        let v = check(&NoShortcuts, &setup(), cases()).unwrap_err();
        assert_eq!(v.law, "L2 idempotence");
        struct Identity;
        impl Strategy for Identity {
            fn id(&self) -> &'static str {
                "test.law.v1"
            }
            fn promises(&self) -> Laws {
                Laws::IDENTITY
            }
            fn resolve(&self, i: &Inputs<'_>, _: &Context<'_>) -> Resolved {
                // Takes the team value even when the team did not change it.
                Resolved::merged(i.team.clone(), "x")
            }
        }
        assert_eq!(
            check(&Identity, &setup(), cases()).unwrap_err().law,
            "L3 identity"
        );
    }

    #[test]
    fn a_strategy_that_changes_between_runs_breaks_determinism() {
        struct Flaky(AtomicUsize);
        impl Strategy for Flaky {
            fn id(&self) -> &'static str {
                "test.law.v1"
            }
            fn promises(&self) -> Laws {
                Laws::DETERMINISM
            }
            fn resolve(&self, _: &Inputs<'_>, _: &Context<'_>) -> Resolved {
                let n = self.0.fetch_add(1, Ordering::SeqCst);
                Resolved::merged(json!(n), "x")
            }
        }
        let v = check(&Flaky(AtomicUsize::new(0)), &setup(), cases()).unwrap_err();
        assert_eq!(v.law, "L5 determinism");
    }

    #[test]
    fn a_declared_bound_must_be_enforced() {
        struct Bounded(bool);
        impl Strategy for Bounded {
            fn id(&self) -> &'static str {
                "test.law.v1"
            }
            fn promises(&self) -> Laws {
                Laws::BOUNDED
            }
            fn max_input_bytes(&self) -> Option<usize> {
                Some(64)
            }
            fn resolve(&self, i: &Inputs<'_>, _: &Context<'_>) -> Resolved {
                if self.0 && i.team.to_string().len() > 64 {
                    return Resolved::conflict(Reason::TOO_LARGE, "bound");
                }
                Resolved::conflict(Reason::BOTH_CHANGED, "x")
            }
        }
        let report = check(&Bounded(true), &setup(), cases()).unwrap();
        assert!(report.checked.contains(&"L8 bounded"));
        assert_eq!(
            check(&Bounded(false), &setup(), cases()).unwrap_err().law,
            "L8 bounded"
        );
    }

    #[test]
    fn unpromised_laws_are_not_held_against_the_strategy() {
        struct Modest;
        impl Strategy for Modest {
            fn id(&self) -> &'static str {
                "test.law.v1"
            }
            fn promises(&self) -> Laws {
                Laws::ALL.without(Laws::SYMMETRY).without(Laws::IDENTITY)
            }
            fn resolve(&self, i: &Inputs<'_>, _: &Context<'_>) -> Resolved {
                Resolved::merged(i.team.clone(), "x")
            }
        }
        // Idempotence holds for "always the team value"; symmetry and identity do not.
        let report = check(&Modest, &setup(), cases()).unwrap();
        assert!(report.skipped.contains(&"L1 symmetry"));
        assert!(report.skipped.contains(&"L3 identity"));
        assert!(
            Laws::ALL.contains(Laws::SYMMETRY)
                && !Laws::ALL.without(Laws::SYMMETRY).contains(Laws::SYMMETRY)
        );
    }
}
