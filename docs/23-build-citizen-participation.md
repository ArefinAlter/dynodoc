# 12 · Citizen Participation (vote / comment / suggest)

### Claude Code prompt to start this stage

> Add the three participation modes from `dynodoc_public_consultation_design.md` §8.1: vote (T1), comment (T2), suggest a change (T2). The engine endpoints already exist from stage 07; this stage builds the citizen-facing UX flows and the aggregation views. In `frontend/citizen/`:
>
> 1. **The Add Your View interaction**:
>    - From a clause's right rail, "Add your view" opens a modal/sheet.
>    - The modal first shows existing positions on this clause grouped by cluster (from the deliberation layer; in stage 12 these are bridge statements + a flat list; stage 15 adds full clustering).
>    - For each existing position, an "Agree with this" button. This calls the `VoteCast` endpoint with +1 against the position's source event id.
>    - At the bottom: "None of these fit my view — write your own." Opens a textarea (max 500 chars), enforces a 60-second cooldown between submissions for the same identity, runs client-side language detection to tag the contribution.
>    - On submit: `CommentAdded` event is posted; on success, the user sees "Your view is recorded. It will appear publicly in 30 minutes after a moderation check." (the first-time-gentling from stage 06).
>
> 2. **The Vote interaction**:
>    - Each rendered position/statement has agree/disagree/pass buttons.
>    - One-tap, no confirmation. Optimistic update; rollback on error.
>    - The button bar shows the user's current vote (visually distinct: green for agree, red for disagree).
>    - Rate limit: 100 votes/day at T1, enforced server-side; client shows remaining when approaching limit.
>
> 3. **The Suggest a Change interaction**:
>    - From a clause, "Suggest specific wording" opens a side-by-side editor: the current clause on the left, an editable copy on the right.
>    - Diff highlights in real time (inserted text underlined, deleted text strikethrough).
>    - The user adds a rationale (required, ≥20 chars).
>    - On submit: `SuggestionProposed{wrapped: FieldEdited{node_id, field: 'text', new_value, rationale}}` is posted.
>    - Confirmation: "Your suggestion is recorded. The sponsor will respond in the closeout report at the end of the consultation."
>
> 4. **Identity tier prompts**:
>    - T0 users hitting any participation button see a friendly "Sign in to participate. Reading is open to all" with a single big "Sign in with email" button.
>    - T1 users hitting Comment or Suggest see "Verify your phone to add a view (votes work now)" with the upgrade flow.
>    - T2 users see no friction.
>
> 5. **Personal participation page**:
>    - `/me/contributions` — a list of every comment, vote, and suggestion the logged-in user has made, with current status (visible, moderated, withdrawn) and the clauses they apply to. Lets a user edit or withdraw their own contributions (which produces a compensating event in the log; the original remains audit-attributable).
>
> 6. **The post-contribution loop** (the vTaiwan-inspired learning moment, §7.6):
>    - After a contribution lands, the user sees "Here's where your view sits in the public conversation": the cluster their position joined (if recognizable), the bridge statements they happen to agree with, the divisive issues they took a side on.
>    - In stage 12 this is sparse (no clustering yet); in stage 15 it becomes richer.
>
> 7. **Optimistic UI with reconcile**: a vote/comment/suggestion applies optimistically to the local state, then the server's response confirms. Failures (rate-limit, moderation block) roll back with a clear message.
>
> 8. **Server-side rate-limit checks**:
>    - All write endpoints already check rate limits (stage 06).
>    - The frontend additionally shows the user their remaining budget when low ("3 comments remaining today").
>
> Tests:
>    - End-to-end: log in as T1, cast 5 votes, switch to a fresh T1 account, verify state isolated.
>    - End-to-end: T2 user submits a comment, sees the "shadowed for 30 min" message, the comment doesn't appear to other users until the timer expires.
>    - End-to-end: T2 user submits a suggestion, the sponsor's proposals queue receives it within seconds.
>    - Optimistic update + rollback on simulated server error.

### Why the aggregate-by-default principle matters here

The §8.2 Principle A — *aggregate by default, individuate on demand* — is implemented in this stage's modal flow: a citizen wanting to share a view first sees existing views grouped, and is gently encouraged to join one rather than write a new one. This is the single most consequential UX choice for keeping signal-to-noise tractable.

Without this, a clause with 1,000 commenters has 1,000 distinct entries to read; with this, the same 1,000 commenters cluster into ~5 positions with 200 each. The deliberation layer (stage 15) makes this rigorous; this stage's UX is the lightweight precursor.

### The withdraw mechanism

A user withdrawing their contribution does *not* delete the event. The event is permanent in the audit log. What withdrawal does:

- Emits a `ContributionWithdrawn{event_id, withdrawn_by}` event.
- The materialized public view no longer includes the original contribution.
- The original is retained, encrypted-at-rest, for the audit trail.
- The closeout report's totals include withdrawn contributions in a separate count.

This is the "publicly hide, internally preserve" property from §6.3 made into a user-facing feature.

### Rate-limit feedback design

A user hitting the rate limit should *not* see a hostile error. The UI should show their budget remaining when below 25% ("8 votes remaining today"), and on hitting zero show "You've reached today's vote limit. Resets at midnight." with a clear time. The limit exists for Sybil defense; it shouldn't feel like punishment to a real user who's just enthusiastic.

### Watch out for

**Comment editing.** If you allow it, editing a comment is *not* an in-place update; it's a `CommentRevised` event referencing the original. The audit trail shows the original wording, the revised wording, and who/when. The public surface shows the latest. This is the same pattern as everything else in the system: events accumulate, derived state reflects the latest.

**Suggestion vs comment.** A *suggestion* is a wrapped `FieldEdited` proposal — concrete wording. A *comment* is general feedback. Keep these distinct in the UI; don't blur them. Suggestions feed the sponsor's proposals queue; comments feed the deliberation layer's clustering.

**Don't auto-translate user input before it's stored.** Store the original. Translate on display if needed. Auto-translating before storage destroys provenance.

### Verification checklist

- [ ] All three participation modes work end-to-end.
- [ ] Tier-gated prompts appear at the right friction points.
- [ ] Withdraw produces the right events; the original is hidden but audit-recoverable.
- [ ] Rate-limit feedback is clear and humane.
- [ ] Optimistic UI rolls back correctly on failure.
- [ ] Personal contributions page lists everything correctly.
