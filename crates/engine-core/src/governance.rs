//! The governance gate and proposal lifecycle (docs/16 parts 2–3).
//!
//! Three things stand between a proposed operation and the canonical log:
//!
//! 1. **Capability** — [`can_perform`] decides whether an actor's [`Role`] is allowed
//!    to perform an op at all. The PoC has three roles (PRD §6.8 FR-27): `Author`
//!    (full authoring), `Reviewer` (comment + suggest only), `Auditor` (read-only).
//!    This replaces the full platform's T0–T3/Sponsor/Moderator tier matrix, which is
//!    consultation-layer scope (PRD §3.2 NG2).
//! 2. **Validity** — [`crate::ops::validate_op`] checks the op against current state.
//! 3. **Integrity** — a `Deployed` event additionally runs the referential-integrity
//!    pass ([`crate::integrity::validate_integrity`]) and is refused if the instrument
//!    has dangling references or relevance cycles (PRD §6.6, docs/16 part 4).
//!
//! [`authorize`] composes all three as a pure decision over `(Role, op, state)`, and
//! [`plan_acceptance`] decides the now-canonical op an accept must append. These are
//! pure functions of materialized state — no I/O. The stage-18 API surface drives
//! them: it reads current state, calls these gates, and appends (an accept appends the
//! planned wrapped event and the `SuggestionAccepted` event in **one transaction** via
//! [`crate::log::append_in_tx`], satisfying FR-19 "atomically").
//!
//! ### The proposal lifecycle
//!
//! A content change from a `Reviewer` is never written to canonical state directly.
//! It arrives wrapped in a `SuggestionProposed` whose `detail` is the serialized
//! wrapped op. The proposal is appended immediately — audit-recorded and attributable
//! forever — but the materializer treats it as side-state (`accepted = false`) that
//! does not change what readers see. Only when an `Author` accepts does the wrapped
//! op become a canonical event that folds into state. A rejection records the reason
//! and leaves the proposal in the log, unapplied.

use engine_shared::EventPayload;
use serde::{Deserialize, Serialize};

use crate::integrity::{validate_integrity, IntegrityViolation};
use crate::materializer::DocumentState;
use crate::ops::{self, OpError};

/// An actor's role on a document (PRD §6.8 FR-27). The PoC access list maps each
/// identity to exactly one of these per document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    /// Owns the instrument: full authoring, accept/reject suggestions, deploy.
    Author,
    /// Peer reviewer: may comment and propose suggestions; no direct write access.
    Reviewer,
    /// Read-only access to state and the audit tools; appends nothing.
    Auditor,
}

impl Role {
    pub const fn as_str(self) -> &'static str {
        match self {
            Role::Author => "author",
            Role::Reviewer => "reviewer",
            Role::Auditor => "auditor",
        }
    }
}

impl std::str::FromStr for Role {
    type Err = engine_shared::ParseEnumError;

    /// Parse the `document_access.role` text (its [`Role::as_str`] form) back into a
    /// `Role` at the API boundary.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "author" => Ok(Role::Author),
            "reviewer" => Ok(Role::Reviewer),
            "auditor" => Ok(Role::Auditor),
            other => Err(engine_shared::ParseEnumError {
                kind: "role",
                value: other.to_string(),
            }),
        }
    }
}

/// The proposal lifecycle status of a suggestion, derived from materialized state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProposalStatus {
    /// Recorded in the log, not yet applied to canonical state.
    Pending,
    /// Accepted by an author; its wrapped op was appended as a canonical event.
    Accepted,
    /// Rejected by an author; recorded with a reason, never applied.
    Rejected,
}

/// Why the governance gate refused an operation.
#[derive(Debug, thiserror::Error)]
pub enum GovernanceError {
    /// The actor's role may not perform this op (capability check).
    #[error("role {role} is not permitted to perform {op}")]
    Unauthorized {
        role: &'static str,
        op: &'static str,
    },
    /// The op is not valid against the current state.
    #[error("operation invalid: {0}")]
    Op(#[from] OpError),
    /// A `Deployed` was refused because the instrument has integrity violations.
    #[error("deploy refused: {} referential-integrity violation(s)", .0.len())]
    Integrity(Vec<IntegrityViolation>),
}

/// Whether `role` is permitted to perform `op` at all (capability matrix, docs/16
/// part 3, trimmed to the PoC role model and op vocabulary).
pub fn can_perform(role: Role, op: &EventPayload) -> bool {
    use EventPayload::*;
    match (role, op) {
        // Auditor is read-only: it appends nothing.
        (Role::Auditor, _) => false,
        // Reviewer may comment and propose; nothing else touches the log.
        (Role::Reviewer, CommentAdded { .. }) => true,
        (Role::Reviewer, CommentReplied { .. }) => true,
        (Role::Reviewer, SuggestionProposed { .. }) => true,
        (Role::Reviewer, _) => false,
        // Author owns the instrument: the full PoC op vocabulary.
        (Role::Author, _) => true,
    }
}

/// The pure governance decision: capability, then validity, then (for `Deployed`)
/// referential integrity. `Ok(())` means the op may be appended.
pub fn authorize(
    role: Role,
    op: &EventPayload,
    state: &DocumentState,
) -> Result<(), GovernanceError> {
    if !can_perform(role, op) {
        return Err(GovernanceError::Unauthorized {
            role: role.as_str(),
            op: op.event_type().as_str(),
        });
    }
    ops::validate_op(state, op)?;
    if matches!(op, EventPayload::Deployed { .. }) {
        let violations = validate_integrity(state);
        if !violations.is_empty() {
            return Err(GovernanceError::Integrity(violations));
        }
    }
    Ok(())
}

/// Derive a proposal's lifecycle status from materialized state, if it exists.
pub fn proposal_status(state: &DocumentState, suggestion_id: &str) -> Option<ProposalStatus> {
    state.suggestions.get(suggestion_id).map(|s| {
        if s.accepted {
            ProposalStatus::Accepted
        } else if s.rejected_reason.is_some() {
            ProposalStatus::Rejected
        } else {
            ProposalStatus::Pending
        }
    })
}

/// Decide the now-canonical op an `Author` must append to accept `suggestion_id`.
///
/// This is the pure half of FR-19's atomic accept: the stage-18 API appends the
/// returned op **and** a `SuggestionAccepted { suggestion_id }` in one transaction
/// (via [`crate::log::append_in_tx`]), so both land or neither does — the now-canonical
/// wrapped event folds into state, the lifecycle event records the acceptance.
///
/// The decision is made against **current** state, so a proposal that was valid when
/// made but would now create a cycle or dangle a reference is refused before anything
/// is appended (docs/16 "a proposal that would create a cycle when accepted is
/// rejected"). Errors: `Unauthorized` (non-author), `Op` (not pending, or the wrapped
/// op is no longer valid).
pub fn plan_acceptance(
    role: Role,
    state: &DocumentState,
    suggestion_id: &str,
) -> Result<EventPayload, GovernanceError> {
    let accept_op = EventPayload::SuggestionAccepted {
        suggestion_id: suggestion_id.to_string(),
    };
    // Capability + "suggestion is pending" (validate_op for SuggestionAccepted).
    authorize(role, &accept_op, state)?;

    let suggestion = state
        .suggestions
        .get(suggestion_id)
        .expect("authorize() confirmed the suggestion is pending, so it exists");
    let wrapped = ops::decode_wrapped_op(&suggestion.detail)?;
    // Re-validate the wrapped op against current state before it becomes canonical.
    ops::validate_op(state, &wrapped)?;
    Ok(wrapped)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashSet};

    use engine_shared::{NodeId, NodeType};
    use serde_json::json;

    use super::*;
    use crate::materializer::{DocumentState, MaterializedNode, SuggestionState};

    fn empty_state() -> DocumentState {
        DocumentState {
            nodes: BTreeMap::new(),
            comments: Vec::new(),
            suggestions: BTreeMap::new(),
            removed_choices: HashSet::new(),
        }
    }

    fn form_state() -> DocumentState {
        let mut s = empty_state();
        s.nodes.insert(
            NodeId("01F".into()),
            MaterializedNode {
                id: NodeId("01F".into()),
                parent_id: None,
                node_type: "form".into(),
                pos: "a0".into(),
                current_fields: json!({}),
                var_name: None,
                deleted: false,
            },
        );
        s
    }

    fn comment() -> EventPayload {
        EventPayload::CommentAdded {
            node_id: None,
            body: "looks good".into(),
        }
    }

    fn field_edit() -> EventPayload {
        EventPayload::FieldEdited {
            node_id: NodeId("01F".into()),
            field: "title".into(),
            value: json!("x"),
        }
    }

    #[test]
    fn capability_matrix_matches_poc_roles() {
        // Auditor can do nothing.
        assert!(!can_perform(Role::Auditor, &comment()));
        assert!(!can_perform(Role::Auditor, &field_edit()));

        // Reviewer: comment + propose only.
        assert!(can_perform(Role::Reviewer, &comment()));
        assert!(can_perform(
            Role::Reviewer,
            &EventPayload::SuggestionProposed {
                suggestion_id: "s".into(),
                target_node_id: None,
                detail: json!({}),
            }
        ));
        assert!(!can_perform(Role::Reviewer, &field_edit()));
        assert!(!can_perform(
            Role::Reviewer,
            &EventPayload::Deployed { snapshot_id: None }
        ));
        assert!(!can_perform(
            Role::Reviewer,
            &EventPayload::SuggestionAccepted {
                suggestion_id: "s".into()
            }
        ));

        // Author: everything in the PoC vocabulary.
        for op in [
            field_edit(),
            comment(),
            EventPayload::Deployed { snapshot_id: None },
            EventPayload::SuggestionAccepted {
                suggestion_id: "s".into(),
            },
        ] {
            assert!(
                can_perform(Role::Author, &op),
                "author should perform {op:?}"
            );
        }
    }

    #[test]
    fn authorize_blocks_unauthorized_before_validation() {
        // A reviewer editing a field: unauthorized regardless of validity.
        let err = authorize(Role::Reviewer, &field_edit(), &form_state()).unwrap_err();
        assert!(matches!(err, GovernanceError::Unauthorized { .. }));
    }

    #[test]
    fn authorize_runs_validation_for_permitted_ops() {
        // Author editing a missing node: permitted, but invalid -> Op error.
        let edit = EventPayload::FieldEdited {
            node_id: NodeId("missing".into()),
            field: "f".into(),
            value: json!(1),
        };
        assert!(matches!(
            authorize(Role::Author, &edit, &empty_state()).unwrap_err(),
            GovernanceError::Op(_)
        ));
    }

    #[test]
    fn deploy_refused_when_integrity_violations_exist() {
        let mut s = form_state();
        s.nodes.insert(
            NodeId("01I".into()),
            MaterializedNode {
                id: NodeId("01I".into()),
                parent_id: Some(NodeId("01F".into())),
                node_type: "item".into(),
                pos: "a1".into(),
                current_fields: json!({ "relevance": "${ghost} = 1" }),
                var_name: Some("here".into()),
                deleted: false,
            },
        );
        let deploy = EventPayload::Deployed { snapshot_id: None };
        match authorize(Role::Author, &deploy, &s).unwrap_err() {
            GovernanceError::Integrity(v) => assert!(!v.is_empty()),
            other => panic!("expected Integrity, got {other:?}"),
        }
    }

    #[test]
    fn deploy_allowed_on_clean_state() {
        assert!(authorize(
            Role::Author,
            &EventPayload::Deployed { snapshot_id: None },
            &form_state()
        )
        .is_ok());
    }

    #[test]
    fn proposal_status_reflects_side_state() {
        let mut s = empty_state();
        s.suggestions.insert(
            "p".into(),
            SuggestionState {
                target_node_id: None,
                detail: json!({}),
                accepted: false,
                rejected_reason: None,
            },
        );
        assert_eq!(proposal_status(&s, "p"), Some(ProposalStatus::Pending));
        assert_eq!(proposal_status(&s, "missing"), None);

        s.suggestions.get_mut("p").unwrap().accepted = true;
        assert_eq!(proposal_status(&s, "p"), Some(ProposalStatus::Accepted));

        let s2 = {
            let mut s = empty_state();
            s.suggestions.insert(
                "r".into(),
                SuggestionState {
                    target_node_id: None,
                    detail: json!({}),
                    accepted: false,
                    rejected_reason: Some("no".into()),
                },
            );
            s
        };
        assert_eq!(proposal_status(&s2, "r"), Some(ProposalStatus::Rejected));
    }

    #[test]
    fn build_node_type_is_used() {
        // Guard that NodeType import stays meaningful as the vocab evolves.
        assert_eq!(NodeType::Form.as_str(), "form");
    }

    // --- fold-level checks: what accept/reject do to materialized state ----------
    //
    // These fold a hand-built event sequence — the exact events the DB
    // orchestration appends — and assert the resulting state, so the checklist
    // items "accepted proposals fold into state" and "rejected proposals do not
    // change state but remain in the log" are covered without a database.

    use engine_shared::{DocumentId, Event, EventId, IdentityId};
    use uuid::Uuid;

    use crate::materializer::Materializer;

    fn ev(seq: i64, payload: EventPayload) -> Event {
        Event {
            id: EventId(format!("01GOV{seq:08}")),
            document_id: DocumentId(Uuid::nil()),
            seq,
            event_type: payload.event_type().as_str().to_string(),
            actor_id: IdentityId(Uuid::nil()),
            target_node_id: payload.target_node_id().cloned(),
            payload: serde_json::to_value(&payload).unwrap(),
            content_hash: vec![0; 32],
            prev_chain_hash: vec![0; 32],
            chain_hash: vec![0; 32],
            created_at: chrono::Utc::now(),
        }
    }

    /// A form + one item, then a suggestion to edit the item's label.
    fn proposed_field_edit() -> (Vec<Event>, EventPayload) {
        let wrapped = EventPayload::FieldEdited {
            node_id: NodeId("01I".into()),
            field: "label".into(),
            value: json!("Accepted label"),
        };
        let setup = vec![
            ev(
                1,
                EventPayload::NodeCreated {
                    node_id: NodeId("01F".into()),
                    node_type: NodeType::Form,
                    parent_id: None,
                    pos: "a0".into(),
                    fields: json!({}),
                    var_name: None,
                },
            ),
            ev(
                2,
                EventPayload::NodeCreated {
                    node_id: NodeId("01I".into()),
                    node_type: NodeType::Item,
                    parent_id: Some(NodeId("01F".into())),
                    pos: "a1".into(),
                    fields: json!({ "label": "Original label" }),
                    var_name: Some("q1".into()),
                },
            ),
            ev(
                3,
                EventPayload::SuggestionProposed {
                    suggestion_id: "s1".into(),
                    target_node_id: Some(NodeId("01I".into())),
                    detail: serde_json::to_value(&wrapped).unwrap(),
                },
            ),
        ];
        (setup, wrapped)
    }

    #[test]
    fn proposed_change_is_not_yet_canonical() {
        let (events, _) = proposed_field_edit();
        let state = Materializer::fold(&events).unwrap();
        // The proposal is recorded but the label is untouched.
        assert_eq!(
            state.nodes[&NodeId("01I".into())].current_fields["label"],
            json!("Original label")
        );
        assert_eq!(proposal_status(&state, "s1"), Some(ProposalStatus::Pending));
    }

    #[test]
    fn plan_acceptance_returns_wrapped_op_for_author_only() {
        let (events, wrapped) = proposed_field_edit();
        let state = Materializer::fold(&events).unwrap();

        // Author: returns exactly the wrapped content op to append as canonical.
        assert_eq!(
            plan_acceptance(Role::Author, &state, "s1").unwrap(),
            wrapped
        );

        // Reviewer: unauthorized to accept.
        assert!(matches!(
            plan_acceptance(Role::Reviewer, &state, "s1").unwrap_err(),
            GovernanceError::Unauthorized { .. }
        ));

        // Unknown / already-resolved proposal: not pending.
        assert!(matches!(
            plan_acceptance(Role::Author, &state, "ghost").unwrap_err(),
            GovernanceError::Op(_)
        ));
    }

    #[test]
    fn plan_acceptance_rejects_wrapped_op_invalid_against_current_state() {
        // Propose an edit, then delete the target node before accepting: the wrapped
        // op is no longer valid, so the accept is refused (no cycle/dangle slips in).
        let (mut events, _) = proposed_field_edit();
        events.push(ev(
            4,
            EventPayload::NodeDeleted {
                node_id: NodeId("01I".into()),
            },
        ));
        let state = Materializer::fold(&events).unwrap();
        assert!(matches!(
            plan_acceptance(Role::Author, &state, "s1").unwrap_err(),
            GovernanceError::Op(_)
        ));
    }

    #[test]
    fn accept_appends_wrapped_event_and_folds_into_state() {
        let (mut events, wrapped) = proposed_field_edit();
        // accept_suggestion appends the wrapped canonical event, then SuggestionAccepted.
        events.push(ev(4, wrapped));
        events.push(ev(
            5,
            EventPayload::SuggestionAccepted {
                suggestion_id: "s1".into(),
            },
        ));
        let state = Materializer::fold(&events).unwrap();
        assert_eq!(
            state.nodes[&NodeId("01I".into())].current_fields["label"],
            json!("Accepted label"),
            "the wrapped edit is now canonical"
        );
        assert_eq!(
            proposal_status(&state, "s1"),
            Some(ProposalStatus::Accepted)
        );
    }

    #[test]
    fn reject_leaves_state_unchanged_but_records_the_proposal() {
        let (mut events, _) = proposed_field_edit();
        events.push(ev(
            4,
            EventPayload::SuggestionRejected {
                suggestion_id: "s1".into(),
                reason: Some("wording is fine".into()),
            },
        ));
        let state = Materializer::fold(&events).unwrap();
        assert_eq!(
            state.nodes[&NodeId("01I".into())].current_fields["label"],
            json!("Original label"),
            "rejection does not change canonical state"
        );
        assert_eq!(
            proposal_status(&state, "s1"),
            Some(ProposalStatus::Rejected)
        );
    }

    // --- property: capability is checked before validity, for any state ----------

    use proptest::prelude::*;

    /// One representative op per variant — the full PoC vocabulary.
    fn sample_ops() -> Vec<EventPayload> {
        vec![
            EventPayload::NodeCreated {
                node_id: NodeId("01N".into()),
                node_type: NodeType::Item,
                parent_id: Some(NodeId("01F".into())),
                pos: "a0".into(),
                fields: json!({}),
                var_name: None,
            },
            EventPayload::NodeMoved {
                node_id: NodeId("01N".into()),
                new_parent_id: Some(NodeId("01F".into())),
                new_pos: "a0".into(),
            },
            EventPayload::NodeDeleted {
                node_id: NodeId("01N".into()),
            },
            field_edit(),
            EventPayload::ChoiceAdded {
                node_id: NodeId("01N".into()),
                choice_id: NodeId("01C".into()),
                fields: json!({}),
            },
            EventPayload::ChoiceRemoved {
                node_id: NodeId("01N".into()),
                choice_id: NodeId("01C".into()),
            },
            comment(),
            EventPayload::SuggestionProposed {
                suggestion_id: "s".into(),
                target_node_id: None,
                detail: json!({}),
            },
            EventPayload::SuggestionAccepted {
                suggestion_id: "s".into(),
            },
            EventPayload::SuggestionRejected {
                suggestion_id: "s".into(),
                reason: None,
            },
            EventPayload::Deployed { snapshot_id: None },
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        /// `authorize` returns `Unauthorized` **iff** `can_perform` is false — the
        /// capability gate runs first and is independent of the state or the op's
        /// validity. So a capability violation always surfaces as the right error,
        /// never as a validation error that leaks what the op would have done.
        #[test]
        fn capability_violation_always_unauthorized(
            role_idx in 0usize..3,
            op_idx in 0usize..11,
        ) {
            let role = [Role::Author, Role::Reviewer, Role::Auditor][role_idx];
            let ops = sample_ops();
            let op = &ops[op_idx];
            let state = form_state();

            let allowed = can_perform(role, op);
            let is_unauthorized = matches!(
                authorize(role, op, &state),
                Err(GovernanceError::Unauthorized { .. })
            );
            prop_assert_eq!(is_unauthorized, !allowed);
        }
    }
}
