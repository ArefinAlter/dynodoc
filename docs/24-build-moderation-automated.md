# 13 · Moderation Pipeline (Stages 1–3 automated)

### Claude Code prompt to start this stage

> Build the automated portions of the moderation pipeline: hard filters, soft classifiers, publish-with-flagging. Stage 4 (human review) comes in stage 14. Reference: `research_ide_public_consultation_design.md` §6.1–§6.3. In `services/moderation/`:
>
> 1. **Service interface**: a FastAPI service exposing:
>    - `POST /moderate` — accepts `{event_id, op_type, text, language, author_id, author_tier, author_first_n_contributions}`; returns `{stage1: pass|block|hold, stage2_classification: {hate: 0.0-1.0, harassment: 0.0-1.0, spam: 0.0-1.0, off_topic: 0.0-1.0}, action: publish|hold|block, reason_code: str?}`.
>    - `POST /flag` — accepts a community flag `{event_id, flagger_id, flagger_tier, reason_code, note?}`. Updates flag aggregates; if thresholds tripped, escalates to human review queue.
>
> 2. **Stage 1 — hard filters** (rule-based, fast):
>    - URL block-list: maintained list of known-bad domains (phishing, CSAM, etc.). Reject if any URL in the text matches.
>    - Pattern filters: regex for unambiguously illegal patterns (specific terror-org branding, child-exploitation indicators). Be conservative; false positives at this stage are politically costly.
>    - Image hashing: for any uploaded images, compute PhotoDNA-compatible hash and check against the NCMEC database via their API (registered as a relying party). Blocked images do not even reach storage.
>    - Decision: `block` (event still recorded with `action: block`; never displayed; forwarded to law enforcement channel if applicable) or `pass` to stage 2.
>
> 3. **Stage 2 — soft classifiers** (ML-based):
>    - Per-language model selection. English: `unitary/toxic-bert` or similar (published, evaluatable). Bangla: fine-tune `csebuetnlp/banglabert` on labeled Bangla hate-speech datasets (BHSC, MUBASE).
>    - Classify into: hate speech, harassment, spam, off-topic. Each returns a confidence score 0.0–1.0.
>    - Thresholds:
>      - confidence ≥ 0.85 in any category → `hold` for human review (Stage 4); shadow-hide from public until reviewed.
>      - confidence 0.5–0.85 → `pass` with elevated flag-sensitivity (a single community flag triggers human review immediately, vs. the normal threshold of 3 flags).
>      - confidence < 0.5 in all → `pass` cleanly.
>    - All classifications are logged with `classifier_version` so the public can audit which model made which decision over time.
>
> 4. **Stage 3 — publish with community flagging**:
>    - Pass events are visible to the public.
>    - Any reader can flag any contribution with a reason code (`hate_speech`, `harassment`, `personal_attack`, `off_topic`, `spam`, `misinformation`, `other`).
>    - Flag aggregation:
>      - Flags are weighted by flagger tier: T1=1, T2=2, T3=5.
>      - Total weighted flags ≥ 10, OR one T3 flag alone, OR ≥ 3 T2 flags → escalate to Stage 4 human review and shadow-hide pending decision.
>    - Per-flagger rate limit: 20 flags per day, prevents flag-brigading.
>    - Suspicious flagging patterns (one flagger consistently flagging one author's contributions, coordinated flag bursts) generate `sybil_signal` rows.
>
> 5. **The pipeline integration with the engine**:
>    - When a contribution event is appended via the engine API, the API immediately calls the moderation service.
>    - The moderation service's decision is recorded as a `ModerationDecided{stage, action, reason_code, classifier_version, confidence}` event in the same document.
>    - The frontend's read path checks `ModerationState::current_visibility(event_id)` to determine whether to render.
>
> 6. **Observability**:
>    - Every classification logged with input hash (not text), output scores, latency, model version.
>    - Daily report job: aggregates classification distribution, flag rate, false-positive estimates (from human-overturned decisions in stage 14).
>
> Tests:
>    - Stage 1: known-bad URL is blocked; clean text passes.
>    - Stage 2: a clearly toxic English string scores high; a benign string scores low; thresholds enforce.
>    - Stage 3: 3 T2 flags escalate to human review; flagger rate limit enforces.
>    - The full pipeline: post a comment → see it visible → flag it 3 times from 3 T2 accounts → see it shadow-hidden pending review.

### Why classifiers, not LLM judgments

For high-volume routine moderation, dedicated classifiers (small, fast, evaluatable) beat LLM-prompted moderation:

- Cost: classifier inference is microseconds and pennies; LLM calls are seconds and dollars.
- Latency: a citizen submitting a comment needs the moderation decision in under 1 second.
- Auditability: a classifier with a published evaluation can be defended; an LLM's "judgment" is opaque.
- Determinism: same input → same score, every time. An LLM's output varies.

Reserve LLM use for the *interesting* parts: cluster summaries (stage 16), edge-case review assists (a moderator's optional tool, stage 14).

### The published-policy commitment

The §6.2 commitment matters: the moderation taxonomy (categories, examples, thresholds, appeal rights) is published before the consultation opens, in plain language, on `/c/[slug]/about`. Moderators train on it. Every decision in the log cites the category. This is what makes the pipeline defensible in court and in parliament.

### Watch out for

**Bangla classifier weakness.** Open-source Bangla hate-speech classifiers are years behind English ones. Recall on Bangla hate speech will be lower. Compensate by:
- Lower the threshold for Stage 4 escalation on Bangla text (e.g. 0.7 instead of 0.85).
- Stronger reliance on community flagging for Bangla content.
- Periodic re-training using the platform's own labeled data (decisions in stage 14 produce labeled examples).

**Coordinated flagging.** A small group flagging a critic's comments to silence them is a real attack pattern. The Sybil signal jobs in stage 06 should detect "single account flagging same author repeatedly" and "tight network of accounts all flagging the same target" and downweight those flags. Don't auto-discard, but reduce their contribution to the threshold.

**Don't expose classifier scores publicly.** The score itself is internal. The public sees the decision (and can appeal). Showing "your comment was rejected with 0.91 hate-speech confidence" invites gaming.

### Verification checklist

- [ ] Stage 1 blocks URLs from the block-list.
- [ ] Stage 2 classifications log with version + scores.
- [ ] Stage 3 community flagging escalates correctly per the thresholds.
- [ ] Bangla and English content both flow through with appropriate models.
- [ ] Flagger rate limits enforce.
- [ ] All decisions create `ModerationDecided` events in the engine log.
