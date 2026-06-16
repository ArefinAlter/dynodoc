# 05 · Semantic Operation Vocabulary and Governance Gate

### Claude Code prompt to start this stage

> Stages 03–04 built the mechanics of writing and reading events. This stage gives them meaning by implementing the semantic operations and the governance layer that decides which proposed events become canonical. Reference: Volume II §D.2 (operations) and §D.3 (the L4 governance layer). In `crates/engine-core`:
>
> 1. In `engine-core/src/ops.rs`, implement one handler per `EventPayload` variant. Each handler validates the operation against the current `DocumentState` and returns either a fully-formed event ready to append or a typed error. Examples:
>    - `NodeCreated`: validate parent exists and has correct type (a `choice` cannot contain a `section`). Validate `varName` is unique within the document.
>    - `FieldEdited`: validate node exists and is not deleted, validate field is editable for this node type, validate field content (e.g. `relevance` expression parses).
>    - `NodeMoved`: validate new parent exists, validate the move would not create a cycle (run the move-CRDT cycle check from Vol II §C.2.2 — even though we don't have CRDT yet, this validation is the same).
> 2. In `engine-core/src/governance.rs`, implement the **proposal lifecycle**:
>    - Content operations from non-author identities arrive as `SuggestionProposed{wrapped_event}`. They are appended to the log immediately (the proposal is audit-recorded) but are not yet "canonical" — they don't change the materialized current state until accepted.
>    - The document maintains a `proposal_state: HashMap<EventId, ProposalStatus>` where ProposalStatus is `Pending | Accepted | Rejected | Withdrawn`.
>    - `SuggestionAccepted{event_id}` from an authorized identity marks the proposal accepted, AND emits the wrapped event as a now-canonical event (which folds into state on next materialization).
>    - `SuggestionRejected{event_id, reason}` marks the proposal rejected without applying it.
> 3. Implement the **capability check** at the governance gate: before accepting any operation, verify the author's identity tier is permitted to perform it (using the capability matrix from `dynodoc_public_consultation_design.md` §5.4). Examples:
>    - T0 can read; can't append anything.
>    - T1 can `VoteCast`; can't `CommentAdded`.
>    - T2 can `CommentAdded`, `SuggestionProposed`, `AlternativeCreated`; can't `Deployed`.
>    - Author/sponsor role can `Deployed`, `SuggestionAccepted`, `ConsultationOpened`.
> 4. Add the **referential integrity pass** as a separate function `validate_integrity(state: &DocumentState) -> Vec<IntegrityViolation>`:
>    - For every `relevance` and `constraint` expression in any node, parse it and check every referenced `varName` exists.
>    - Build the reference graph; check for cycles (an item's visibility cannot depend transitively on itself).
>    - Run this on every `Deployed` event and refuse if violations exist. This is the §C.3.6 "silent semantic breakage" defense.
> 5. Property tests:
>    - Capability violations always raise the right error.
>    - A proposal that would create a cycle when accepted is rejected before append.
>    - A `Deployed` event on a state with dangling references is rejected.
>    - Accepted proposals correctly fold into the materialized state.

### The proposal lifecycle is the key insight here

A naive design might let any contributor write directly to the canonical state. We do not. Every contribution from outside the author/sponsor role is a *proposal* — an immutable event in the log, with a `Pending` status, that does not affect what readers see as the "current document." Only when the authorized author accepts the proposal does the wrapped event become canonical.

This separates two things that should be separate:
- **Audit attribution**: the proposal exists in the log forever, attributable to its proposer, even if rejected.
- **State change**: only sponsor-approved events shape the visible state.

For the public consultation case (stages 11+), this means a citizen's suggestion is recorded — they can prove they made it — but the sponsor decides whether to incorporate it. The closeout report enumerates which proposals were accepted and which weren't, with reasons.

### The capability matrix in code

```rust
// engine-core/src/governance.rs
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tier { T0, T1, T2, T3, Sponsor, Moderator }

pub fn can_perform(tier: Tier, op: &EventPayload) -> bool {
    use EventPayload::*;
    match (tier, op) {
        // Read-only
        (Tier::T0, _) => false,
        // T1: votes only
        (Tier::T1, VoteCast { .. }) => true,
        (Tier::T1, _) => false,
        // T2: comments, suggestions, votes
        (Tier::T2, VoteCast { .. }) => true,
        (Tier::T2, CommentAdded { .. }) => true,
        (Tier::T2, SuggestionProposed { .. }) => true,
        (Tier::T2, _) => false,
        // T3: same as T2 (the difference is *weight* in counts, not capability)
        (Tier::T3, op) => can_perform(Tier::T2, op),
        // Sponsor: anything except moderation
        (Tier::Sponsor, ModerationDecided { .. }) => false,
        (Tier::Sponsor, _) => true,
        // Moderator: moderation only
        (Tier::Moderator, ModerationDecided { .. }) => true,
        (Tier::Moderator, _) => false,
    }
}
```

### Watch out for

**Don't let validation run after the append.** Capability check, referential integrity, cycle check all happen *before* the event is appended. Once appended, an event is immutable; you can only emit a compensating event.

**Two-event semantics for accepts.** When a `SuggestionAccepted{event_id}` is processed, it does *two* things atomically: marks the proposal accepted, and appends a *new* event (the wrapped content event, now canonical). Both must be in the same database transaction. Keep them in lockstep.

**Don't conflate "Pending" with "missing".** A pending proposal exists in the log. The materializer skips it when computing canonical state, but it's queryable for the suggestions side-state.

### Verification checklist

- [ ] Capability matrix enforces all blocked combinations.
- [ ] A citizen at T2 submitting a `CommentAdded` is accepted; a citizen at T2 submitting a `Deployed` is rejected.
- [ ] An accepted proposal folds correctly into the materialized state.
- [ ] A rejected proposal does not change state but remains in the log.
- [ ] Referential integrity violation prevents a `Deployed` event.
- [ ] All property tests pass.
