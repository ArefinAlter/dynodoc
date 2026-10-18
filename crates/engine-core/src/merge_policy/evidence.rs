//! What a policy-driven merge decided and why: one [`Decision`] per field the
//! strategies were asked about, the post-merge invariant report, and a
//! deterministic JSON and text rendering of both. Evidence is returned beside the
//! plan and never changes it.
//!
//! Inputs are not copied into the record: each is a SHA-256 digest of its canonical
//! JSON, its size and a short preview. A reader with the three states can re-derive
//! the digests; the record alone cannot reproduce a value, which keeps it small and
//! free of document content beyond the preview.

use super::{
    invariants::Violation,
    policy::{InvariantMode, MergePolicy},
    Family, Gate,
};
use engine_shared::NodeId;
use serde::Serialize;
use serde_json::Value;
use std::fmt::Write as _;

pub const SCHEMA: &str = "dynodoc.merge-evidence.v1";
const PREVIEW_CHARS: usize = 60;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Digest {
    pub sha256: String,
    pub bytes: usize,
    pub preview: String,
}

impl Digest {
    pub fn of(value: &Value) -> Self {
        let canonical = crate::log::canonical_json(value);
        let sha = ring::digest::digest(&ring::digest::SHA256, &canonical);
        let text = serde_json::to_string(value).unwrap_or_default();
        let mut preview: String = text.chars().take(PREVIEW_CHARS).collect();
        if text.chars().count() > PREVIEW_CHARS {
            preview.push('…');
        }
        Self {
            sha256: super::hex(sha.as_ref()),
            bytes: canonical.len(),
            preview,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DecisionOutcome {
    /// A strategy produced the value that is in the merged state.
    Merged,
    /// Left to a human: the plan carries a conflict (or a structural check does).
    Conflict,
    /// A conflict the caller settled with an explicit resolution.
    Explicit,
    /// A collection whose rule did not apply, so the whole-collection choice that
    /// existed before policies was used.
    Legacy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Decision {
    /// `None` for a document-level collection.
    pub node: Option<NodeId>,
    /// The field, `$node` or `$location`, or `$comments` / `$suggestions`.
    pub field: String,
    /// The rule that matched; `None` when no rule did.
    pub rule: Option<String>,
    /// The strategy that ran; `None` when none did (no rule, or a blocked gate).
    pub strategy: Option<String>,
    /// The effective gate of the matching rule.
    pub gate: Option<Gate>,
    pub outcome: DecisionOutcome,
    /// For an explicit resolution: `team`, `draft` or the new name.
    pub choice: Option<String>,
    /// A short stable code: why the strategy declined or what settled it.
    pub reason: String,
    /// The strategy's own account of the branch it took.
    pub basis: Option<String>,
    pub notes: Vec<String>,
    /// The gate asks for a review of this automatic result (acted on in N5).
    pub needs_review: bool,
    /// The key of the plan's conflict, when the outcome is a conflict.
    pub conflict_key: Option<String>,
    pub base: Digest,
    pub team: Digest,
    pub draft: Digest,
    /// Information the strategy reported as absent from its output.
    pub lost: Vec<Digest>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PolicyRef {
    pub id: String,
    pub hash: String,
}

/// Differences settled before any strategy was consulted. They are not decisions:
/// the value is unchanged, or only one side changed it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Shortcuts {
    /// Equal on both sides, or changed only on the team side.
    pub unchanged_or_team_only: u64,
    pub draft_only: u64,
}

/// A conflict raised by the tree checks after the field loop (a parent cycle, an
/// orphan, a duplicate variable name), named by the invariant it enforces.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StructuralConflict {
    pub invariant: &'static str,
    pub key: String,
    pub node: NodeId,
    pub field: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct InvariantReport {
    pub mode: InvariantMode,
    /// Violations in the merged state that neither input had.
    pub introduced: Vec<Violation>,
    /// Violations the merged state shares with an input; not the merge's doing.
    pub preexisting: usize,
    /// Keys of the `$invariant` conflicts added (only in `conflict` mode).
    pub conflicts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MergeEvidence {
    pub schema: &'static str,
    pub policy: PolicyRef,
    pub family: Family,
    pub decisions: Vec<Decision>,
    pub shortcuts: Shortcuts,
    pub structural: Vec<StructuralConflict>,
    pub invariants: InvariantReport,
}

/// What to call the two sides in the text rendering.
#[derive(Clone, Copy, Debug)]
pub struct Labels<'a> {
    pub team: &'a str,
    pub draft: &'a str,
}

impl Default for Labels<'static> {
    fn default() -> Self {
        Self {
            team: "team",
            draft: "draft",
        }
    }
}

impl MergeEvidence {
    pub(crate) fn new(policy: &MergePolicy, family: Family) -> Self {
        Self {
            schema: SCHEMA,
            policy: PolicyRef {
                id: policy.id.clone(),
                hash: policy.hash_hex(),
            },
            family,
            decisions: Vec::new(),
            shortcuts: Shortcuts::default(),
            structural: Vec::new(),
            invariants: InvariantReport {
                mode: policy.defaults.invariants,
                introduced: Vec::new(),
                preexisting: 0,
                conflicts: Vec::new(),
            },
        }
    }

    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).expect("evidence is plain JSON data")
    }

    /// A human-readable account, in the order the decisions were taken. The same
    /// evidence always renders the same text.
    pub fn render_text(&self, labels: Labels<'_>) -> String {
        let count = |o| self.decisions.iter().filter(|d| d.outcome == o).count();
        let legacy = count(DecisionOutcome::Legacy);
        let mut out = String::new();
        let _ = writeln!(
            out,
            "policy {} sha256:{}  family {:?}",
            self.policy.id,
            &self.policy.hash[..16],
            self.family
        );
        let _ = writeln!(
            out,
            "decisions: {} ({} merged, {} explicit, {} conflict, {} whole-collection); settled without a strategy: {} unchanged or {}-only, {} {}-only",
            self.decisions.len(),
            count(DecisionOutcome::Merged),
            count(DecisionOutcome::Explicit),
            count(DecisionOutcome::Conflict),
            legacy,
            self.shortcuts.unchanged_or_team_only,
            labels.team,
            self.shortcuts.draft_only,
            labels.draft,
        );
        for d in &self.decisions {
            let at = match &d.node {
                Some(node) => format!("{}.{}", node.0, d.field),
                None => d.field.clone(),
            };
            let verdict = match (d.outcome, &d.choice) {
                (DecisionOutcome::Merged, _) => "merged".to_string(),
                (DecisionOutcome::Conflict, _) => "conflict".to_string(),
                (DecisionOutcome::Legacy, _) => "whole-collection choice".to_string(),
                (DecisionOutcome::Explicit, Some(c)) => format!("explicit {}", label(c, labels)),
                (DecisionOutcome::Explicit, None) => "explicit".to_string(),
            };
            let _ = writeln!(out, "- {at}: {verdict} ({})", d.reason);
            let _ = writeln!(
                out,
                "    rule {}  strategy {}  gate {}{}",
                d.rule.as_deref().unwrap_or("(none)"),
                d.strategy.as_deref().unwrap_or("(not run)"),
                d.gate.map_or("(none)", Gate::name),
                if d.needs_review { "  needs review" } else { "" }
            );
            if let Some(basis) = &d.basis {
                let _ = writeln!(out, "    basis {basis}");
            }
            for note in &d.notes {
                let _ = writeln!(out, "    note {note}");
            }
            for (name, digest) in [
                ("base", &d.base),
                (labels.team, &d.team),
                (labels.draft, &d.draft),
            ] {
                let _ = writeln!(
                    out,
                    "    {name:<5} {} ({} B, sha256:{})",
                    digest.preview,
                    digest.bytes,
                    &digest.sha256[..16]
                );
            }
            for lost in &d.lost {
                let _ = writeln!(
                    out,
                    "    lost  {} (sha256:{})",
                    lost.preview,
                    &lost.sha256[..16]
                );
            }
        }
        for s in &self.structural {
            let _ = writeln!(
                out,
                "- {}.{}: conflict ({}) raised by the tree checks, key {}",
                s.node.0, s.field, s.invariant, s.key
            );
        }
        let report = &self.invariants;
        let _ = writeln!(
            out,
            "invariants ({}): {} introduced, {} already in an input",
            match report.mode {
                InvariantMode::Flag => "flag",
                InvariantMode::Conflict => "conflict",
            },
            report.introduced.len(),
            report.preexisting
        );
        for v in &report.introduced {
            let _ = writeln!(out, "! {} {}: {}", v.invariant, v.subject, v.detail);
        }
        for key in &report.conflicts {
            let _ = writeln!(out, "  conflict added: {key}");
        }
        out
    }
}

fn label<'a>(choice: &'a str, labels: Labels<'a>) -> &'a str {
    match choice {
        "team" => labels.team,
        "draft" => labels.draft,
        other => other,
    }
}
