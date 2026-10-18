//! Takes each three-way difference to the strategy a policy names, applies the
//! gate, lets an explicit resolution settle a conflict the way `workspace_merge`
//! always has, and records the decision.

use super::{
    evidence::{Decision, DecisionOutcome, Digest, MergeEvidence, Shortcuts},
    policy::{MergePolicy, Probe, Unknown},
    registry::Registry,
    strategy::{Context, Inputs, Outcome},
    Aspect, Family, Gate,
};
use crate::workspace_merge::{is_new_name, At, Conflict, Selector};
use engine_shared::NodeId;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    panic::{catch_unwind, AssertUnwindSafe},
};

pub(crate) struct Engine<'a> {
    policy: &'a MergePolicy,
    registry: &'a Registry,
    family: Family,
    decisions: Vec<Decision>,
    shortcuts: Shortcuts,
}

struct Decided {
    value: Option<Value>,
    decision: Decision,
}

/// What a document-level collection does after its rule was consulted.
pub(crate) enum Collected {
    /// No rule applied: keep the whole-collection three-way choice.
    Whole,
    Merged(Value),
    /// Strategy declined; the collection stays a conflict.
    Conflict,
}

impl<'a> Engine<'a> {
    pub(crate) fn new(policy: &'a MergePolicy, registry: &'a Registry, family: Family) -> Self {
        Self {
            policy,
            registry,
            family,
            decisions: Vec::new(),
            shortcuts: Shortcuts::default(),
        }
    }

    pub(crate) fn finish(self) -> MergeEvidence {
        let mut evidence = MergeEvidence::new(self.policy, self.family);
        evidence.decisions = self.decisions;
        evidence.shortcuts = self.shortcuts;
        evidence
    }

    #[allow(clippy::too_many_arguments)]
    fn decide(
        &self,
        node: Option<&NodeId>,
        node_type: &str,
        field: &str,
        aspect: Option<Aspect>,
        base: &Value,
        team: &Value,
        draft: &Value,
    ) -> Decided {
        // For the content field the rules may name the block type of the value.
        let block_type = if field == "content" {
            [team, draft, base]
                .into_iter()
                .find_map(|v| v["type"].as_str())
        } else {
            None
        };
        let probe = Probe {
            family: self.family,
            node_type,
            block_type,
            field,
            aspect,
        };
        let mut d = Decision {
            node: node.cloned(),
            field: field.into(),
            rule: None,
            strategy: None,
            gate: None,
            outcome: DecisionOutcome::Conflict,
            choice: None,
            reason: String::new(),
            basis: None,
            notes: Vec::new(),
            needs_review: false,
            conflict_key: None,
            base: Digest::of(base),
            team: Digest::of(team),
            draft: Digest::of(draft),
            lost: Vec::new(),
        };
        let none = |d: Decision| Decided {
            value: None,
            decision: d,
        };
        let Some(rule) = self.policy.find_rule(&probe) else {
            d.reason = match self.policy.defaults.unknown {
                Unknown::Manual => "no-rule",
                Unknown::Blocked => "no-rule-blocked",
            }
            .into();
            return none(d);
        };
        d.rule = Some(rule.id.clone());
        let gate = self.policy.effective_gate(rule);
        d.gate = Some(gate);
        if gate == Gate::Blocked {
            d.reason = "blocked".into();
            return none(d);
        }
        let Some(strategy) = self.registry.get(&rule.strategy) else {
            d.reason = "unknown-strategy".into();
            return none(d);
        };
        d.strategy = Some(rule.strategy.clone());
        let inputs = Inputs { base, team, draft };
        let ctx = Context {
            node,
            node_type,
            field,
            aspect,
            family: self.family,
            params: &rule.params.0,
        };
        // A strategy is total by law L4; a bug that breaks that becomes a conflict
        // with an internal-error marker instead of taking the merge down.
        let Ok(resolved) = catch_unwind(AssertUnwindSafe(|| strategy.resolve(&inputs, &ctx)))
        else {
            d.reason = "internal-error".into();
            return none(d);
        };
        d.basis = Some(resolved.used.basis.into());
        d.notes = resolved.used.notes;
        d.lost = resolved.used.lost.iter().map(Digest::of).collect();
        match resolved.outcome {
            Outcome::Merged(_) if gate == Gate::Manual => {
                d.reason = "manual-gate".into();
                d.notes.push(
                    "the strategy proposed a merged value; the gate requires a human decision"
                        .into(),
                );
                none(d)
            }
            Outcome::Merged(value) => {
                d.outcome = DecisionOutcome::Merged;
                d.reason = "merged".into();
                d.needs_review = gate == Gate::AutoWithReview;
                Decided {
                    value: Some(value),
                    decision: d,
                }
            }
            // Keeping one side means the other must be preserved (design 7); that
            // is phase N2, so a preference is not applied.
            Outcome::Prefer(side) => {
                d.reason = "prefer-unsupported".into();
                d.notes.push(format!(
                    "the strategy preferred the {side:?} side; loser preservation is not built, so nothing is applied"
                ));
                none(d)
            }
            Outcome::Conflict(reason) => {
                d.reason = reason.0.into();
                none(d)
            }
        }
    }

    /// Decide one document-level collection of the converged states. Equal or
    /// one-sided collections are left to the whole-collection choice, which gives
    /// the same value; only a real three-way difference reaches a rule.
    pub(crate) fn collection(
        &mut self,
        aspect: Aspect,
        base: &Value,
        team: &Value,
        draft: &Value,
    ) -> Collected {
        let field = match aspect {
            Aspect::Comments => "$comments",
            Aspect::Suggestions => "$suggestions",
            Aspect::Marks | Aspect::Structure => return Collected::Whole,
        };
        if draft == base || draft == team || team == base {
            return Collected::Whole;
        }
        let probe = Probe {
            family: self.family,
            node_type: "",
            block_type: None,
            field,
            aspect: Some(aspect),
        };
        if self.policy.find_rule(&probe).is_none() {
            return Collected::Whole;
        }
        let Decided {
            value,
            mut decision,
        } = self.decide(None, "", field, Some(aspect), base, team, draft);
        let out = match value {
            Some(v) => Collected::Merged(v),
            // The rule did not apply (gate, unknown strategy, internal error): the
            // whole-collection choice stands, so this is not itself a conflict.
            None if decision.strategy.is_none()
                || matches!(decision.reason.as_str(), "manual-gate" | "internal-error") =>
            {
                decision.outcome = DecisionOutcome::Legacy;
                Collected::Whole
            }
            None => Collected::Conflict,
        };
        self.decisions.push(decision);
        out
    }
}

impl Selector for Engine<'_> {
    fn select(
        &mut self,
        at: At<'_>,
        base: Value,
        team: Value,
        draft: Value,
        resolutions: &BTreeMap<String, String>,
        conflicts: &mut Vec<Conflict>,
    ) -> Value {
        if draft == base || draft == team {
            self.shortcuts.unchanged_or_team_only += 1;
            return team;
        }
        if team == base {
            self.shortcuts.draft_only += 1;
            return draft;
        }
        let Decided {
            value,
            mut decision,
        } = self.decide(
            Some(at.id),
            at.node_type,
            at.field,
            None,
            &base,
            &team,
            &draft,
        );
        if let Some(merged) = value {
            self.decisions.push(decision);
            return merged;
        }
        let key = format!("{}:{}", at.id.0, at.field);
        let settled = match resolutions.get(&key).map(String::as_str) {
            Some("draft") => {
                decision.choice = Some("draft".into());
                draft
            }
            Some("team") => {
                decision.choice = Some("team".into());
                team
            }
            // Two names for one variable may also be resolved with a new name.
            Some(name) if at.field == "var_name" && is_new_name(name) => {
                decision.choice = Some(name.into());
                json!(name)
            }
            _ => {
                conflicts.push(Conflict {
                    key: key.clone(),
                    node_id: at.id.clone(),
                    field: at.field.into(),
                    ancestor: base,
                    team: team.clone(),
                    draft,
                });
                decision.conflict_key = Some(key);
                team
            }
        };
        if decision.choice.is_some() {
            decision.outcome = DecisionOutcome::Explicit;
        }
        self.decisions.push(decision);
        settled
    }
}
