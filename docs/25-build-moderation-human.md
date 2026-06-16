# 14 · Moderation Pipeline (Stage 4 human review + appeals)

### Claude Code prompt to start this stage

> Add the human-review interface and the appeal workflow. This stage completes the moderation pipeline. Reference: §6.1 (Stage 4) and §6.2 (the appeal architecture). A new frontend area for moderators in `frontend/researcher/` (reuse the researcher app's auth and chrome) at `/mod/`:
>
> 1. **Moderator queue view** at `/mod/queue`:
>    - Lists all contributions in `held` or `flagged` state, sorted by time-in-queue (oldest first) with priority bumps for T3 flags and consultations near deadline.
>    - Each item shows: the contribution text, the classifier scores (for context), the flag reasons, the author's tier and prior moderation history (number of previously-removed contributions), the clause it belongs to with one-tap to view in context.
>    - Bulk actions: select multiple from the same author for batch decision.
>    - Decision buttons: `Keep`, `Hide`, `Remove`, `Escalate to panel`. Each requires selecting a `reason_code` from the published policy taxonomy.
>    - Optional `rationale` field for documenting non-obvious decisions; required for `Remove` and `Escalate`.
>
> 2. **The decision flow**:
>    - On a decision, post a `ModerationDecided` event.
>    - `Keep`: the contribution becomes/stays publicly visible.
>    - `Hide`: visible only to author and moderators; can be appealed.
>    - `Remove`: the strongest action; recorded in audit log but not publicly displayed. Author is notified with the reason.
>    - `Escalate`: forwards to the moderation review panel (a smaller, more senior group, possibly including a civil-society representative per §12.1).
>
> 3. **Author notification**:
>    - When a contribution is hidden or removed, the author gets an in-app notification + optional email: "Your contribution to clause [X] was [hidden|removed] under [reason_code]: [public policy excerpt]. You can appeal once within 7 days."
>    - The notification is not a separate email-based system; it's a `Notification` event in the engine log, attributable like everything else.
>
> 4. **Appeal flow**:
>    - The author clicks "Appeal" within 7 days of the decision.
>    - Provides a written basis (≥40 chars).
>    - Appeal posts a `ModerationAppealed` event; the contribution returns to the queue with an `appealed: true` flag.
>    - The appeal is reviewed by a *different* moderator (the queue's rotation enforces this; reviewers cannot see their own original decisions).
>    - On the second decision, if it goes against the author again, the appeal record indicates "upheld"; if reversed, "overturned" and the contribution is restored.
>    - One appeal per decision. Subsequent contests go through institutional process, not platform UI.
>
> 5. **The moderation review panel** (for escalations and contentious appeals):
>    - A separate role (`Tier::PanelMember`) in the identity system.
>    - At least one panel member must be from outside the sponsoring institution.
>    - Panel decisions are public (with author anonymization on request) in the transparency report (stage 19).
>
> 6. **Queue throughput tools**:
>    - Keyboard shortcuts for moderator efficiency: `k` keep, `h` hide, `r` remove, `e` escalate, `n` next, `p` previous.
>    - "Time in queue" budget visible per item; consultation-level dashboard shows median time-to-decision.
>    - LLM-assisted *suggestions* (optional, off by default): a moderator can request "summarize this contribution and the relevant policy section" to speed review. The suggestion is informational; the human still makes the call.
>
> 7. **Moderator audit trail**:
>    - Every moderator action is itself an event in the document's log, attributable.
>    - A moderator's decision history is queryable (with privacy considerations) by oversight bodies.
>    - Mandatory rotation: no single moderator handles >30% of one consultation's queue, to prevent personal bias from dominating.
>
> Tests:
>    - End-to-end: contribution → held by Stage 2 → moderator opens queue → hides with reason → author notified → author appeals → different moderator reverses → contribution restored.
>    - Bulk decision: select 5 contributions from one Sybil-account, batch remove with one reason code.
>    - Rotation constraint: a moderator cannot review their own prior decision; queue UI hides those items.

### Why the appeal layer is essential

The §6.2 commitment matters: "Appeal is real and structurally separate." A moderation pipeline without functional appeal is one a political opponent can credibly call censorship; one with appeal — and a published overturn rate — has the receipts to defend itself. Build it well.

Two design subtleties:

**Appeal goes to a different moderator.** Enforced by the queue logic — a moderator who decided a case cannot see its appeal. Builds trust that appeals are heard fresh, not by the same person.

**Appeal records are public** in the transparency report (stage 19): how many appeals filed, how many overturned, by category. A high overturn rate is a signal that initial moderation is too aggressive; a low rate that it's calibrated or that appeals aren't being granted fairly. Either reading is valuable.

### The moderator UI matters more than the algorithms

A pipeline that asks moderators to make 50 decisions per day needs to be *fast and humane*. Keyboard shortcuts, batch operations, side-by-side policy reference, clear "why I'm seeing this" context — all of this is what turns a 5-hour moderation shift into a 90-minute one. Invest here.

### The panel for escalations

§12.1 specified an oversight body with civil-society representation. The platform UI surfaces this concretely: escalated contributions go to a panel queue accessible only by `PanelMember` accounts. Panel deliberation can be asynchronous (Slack-like discussion thread attached to the case) before a vote. The decision is final at the platform level; further contest goes through institutional process (parliament, courts, ombudsman).

### Watch out for

**Don't let moderators see author identity unnecessarily.** For most decisions, moderators see the contribution and the prior-moderation-count, not the author's name. This prevents personal bias from creeping in. Only for repeat-offender cases do they see the author's identifier.

**Notification fatigue.** A noisy notification system is one citizens mute. Keep notifications meaningful: moderation actions, appeal outcomes, closeout report publication. Not "new bridge statement emerged."

**Audit trail privacy.** Moderator decision history is sensitive — both for the moderator (harassment risk if their identity is exposed) and for authors (who'd be re-identifiable from decision patterns). Access is restricted to oversight bodies via lawful process; the public transparency report aggregates without identifying.

**Time-to-decision SLO.** Set and publish a target: e.g. "median time from hold to decision: 4 hours; 95th percentile: 24 hours." Track it. Miss it → either add moderators or tighten thresholds.

### Verification checklist

- [ ] Queue renders, sortable, filterable.
- [ ] Keyboard shortcuts work.
- [ ] Bulk decisions log correctly.
- [ ] Appeal flow end-to-end: same contribution, two moderators, decision reversed correctly.
- [ ] Rotation constraint prevents self-review.
- [ ] Notifications fire (in-app + email) on author-affecting actions.
- [ ] Transparency-report aggregates query correctly.
