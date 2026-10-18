//! Merge policy, strategy framework and evidence for the node-conflict design
//! (`docs/DESIGN-NODE-CONFLICT-RESOLUTION.md`, phases N0 and N1).
//!
//! What is built here is a behavioural no-op: the policy `dynodoc.default.v1`
//! ([`default::policy`]) routes every decision of `workspace_merge::merge` through
//! strategies that call the existing logic, so
//! [`workspace_merge::merge_with_policy`](crate::workspace_merge::merge_with_policy)
//! with it returns the plan `merge` returns, and the evidence says how each
//! decision was taken. The strategies of N2 onwards (grapheme text, marks, lists
//! and tables, sheets, slides), loser preservation and review gating are not built.
//!
//! * [`policy`]: the `dynodoc.merge-policy.v1` data format, strict loader,
//!   canonical form and hash.
//! * [`strategy`]: the [`Strategy`] trait and what it returns.
//! * [`registry`]: the closed set of strategies a policy may name.
//! * [`default`], [`collections`]: the strategies that exist.
//! * [`laws`]: the harness that checks a strategy against the laws of design 4.1.
//! * [`evidence`]: the record of each decision and its deterministic rendering.
//! * [`invariants`]: the post-merge checks INV-G1 to INV-G8.

pub mod collections;
pub mod default;
pub(crate) mod engine;
pub mod evidence;
pub mod invariants;
pub mod laws;
pub mod policy;
pub mod registry;
pub mod strategy;

pub use evidence::MergeEvidence;
pub use policy::{MergePolicy, PolicyError};
pub use registry::Registry;
pub use strategy::{Context, Inputs, Outcome, Reason, Resolved, Side, Strategy, Used};

use crate::materializer::DocumentState;
use serde::{Deserialize, Serialize};

/// How strictly a decision must be reviewed (design 6.2). The derived order is the
/// strictness order: `Blocked` is the strictest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Gate {
    /// Applied without review.
    Auto,
    /// Applied, and the result is marked for review.
    AutoWithReview,
    /// Never applied automatically: the decision stays a conflict.
    Manual,
    /// The strategy does not run; the decision stays a conflict.
    Blocked,
}

impl Gate {
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::AutoWithReview => "auto-with-review",
            Self::Manual => "manual",
            Self::Blocked => "blocked",
        }
    }
}

/// The family of document a decision belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Family {
    Document,
    Spreadsheet,
    Presentation,
}

impl Family {
    /// The family is not stored: it is read from the section kinds present. Any
    /// `slide` section makes a presentation, otherwise any `sheet` section makes a
    /// spreadsheet, otherwise the document is text. Evaluated over every state of
    /// the merge, so all three agree on the answer.
    pub fn of(states: &[&DocumentState]) -> Self {
        let mut family = Self::Document;
        for node in states.iter().flat_map(|s| s.nodes.values()) {
            match node.current_fields["kind"].as_str() {
                Some("slide") => return Self::Presentation,
                Some("sheet") => family = Self::Spreadsheet,
                _ => {}
            }
        }
        family
    }
}

/// A document-level collection a rule can decide, as opposed to a node field.
/// `Marks` and `Structure` are reserved for strategies of later phases; no strategy
/// that exists decides them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Aspect {
    Marks,
    Structure,
    Comments,
    Suggestions,
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
