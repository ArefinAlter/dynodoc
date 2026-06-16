# 16 · AI Summary with Validation Loop

### Claude Code prompt to start this stage

> Add the LLM summarization layer with the vTaiwan-style validation loop: AI generates a cluster summary, the cluster's members vote on whether it represents them, only validated summaries are published. Reference: §7.5. In `services/summarizer/`:
>
> 1. **LLM client abstraction**:
>    - Single `LlmClient` interface with implementations: `OpenAIClient`, `AnthropicClient`, `VllmLocalClient`.
>    - Configured by environment variable; production for sovereignty uses vLLM with Llama 3.1 8B or Mistral 7B.
>    - All calls include a request hash logged for reproducibility audits.
>
> 2. **Cluster summary generation**:
>    - For each cluster of size ≥ 20 within a clause that has ≥ 5 high-agreement statements:
>      - Gather: clause text, top 5 statements (by intra-cluster agreement), cluster size, cluster's typical cross-cluster behavior (agree with bridges, disagree with which positions).
>      - Prompt template (per language):
>        > Below is a clause from a public consultation, followed by statements that one opinion group within the consultation broadly agreed on. Write a 2-sentence summary, in the first person plural ("we believe..." / "we are concerned that..."), that represents this group's position on the clause. Be specific. Do not invent positions not in the statements. Maximum 280 characters.
>      - Generate with low temperature (0.2) for consistency.
>      - Store as a draft `summary_text` in `deliberation_cluster` with `summary_validated = false`.
>
> 3. **Validation loop**:
>    - After 24 hours of being live (or immediately when the cluster reaches 50 members, whichever is later), present the draft summary to members of that cluster:
>      - A banner appears: "Here's how we summarized your group's position on this clause. Does this accurately represent your view? [Yes — represents me] [No — doesn't represent me] [Skip]".
>      - Track in `deliberation_summary_vote` table.
>    - Acceptance criteria:
>      - ≥ 30 cluster members have voted on the summary.
>      - ≥ 70% voted "Yes".
>      - If criteria met → set `summary_validated = true`; summary becomes publicly visible.
>      - If criteria not met after 48 hours → regenerate with a slight prompt variation (or fallback to a hand-edited summary by sponsor team).
>
> 4. **Cross-cluster bridge statement summarization**:
>    - For statements flagged `is_bridge = true`, optionally generate a one-sentence "why this matters" explanation: a synthesis of why this statement got cross-cluster support.
>    - These are sponsor-facing, not citizen-facing, and feed the closeout report (stage 18).
>
> 5. **Multilingual summarization**:
>    - For consultations in Bangla, the prompt template is in Bangla; the model output is Bangla.
>    - For English clauses with multilingual contributions, generate per-language versions.
>    - Use the language tag from the embedding step (stage 15) to route correctly.
>
> 6. **Cost controls**:
>    - Cache summaries by input hash; never regenerate for the same input.
>    - Batch generation: process all clusters needing a fresh summary in one job (every 6 hours).
>    - Budget alarm: if monthly LLM spend exceeds a configurable threshold, summaries pause and an alert fires.
>
> 7. **Audit trail**:
>    - Each generated summary is recorded with: input statements (by event_id), prompt template version, model name and version, temperature, output text, generation timestamp.
>    - The audit query "show me the inputs to this summary" must always be answerable.
>
> Tests:
>    - Generation produces in-language output for Bangla and English inputs.
>    - Validation flow: synthetic cluster with synthetic members, generated summary, simulated votes → validated state set correctly.
>    - Regeneration triggered on insufficient acceptance.
>    - Cost tracking accumulates per consultation.

### Why the validation loop is non-negotiable

The vTaiwan caution (§7.5) is real: an unvalidated AI summary putting words in citizens' mouths is worse than no summary at all. The loop is the discipline that prevents this:

- Summary is shown to members of the cluster it summarizes (not the public).
- 30+ members vote on it.
- 70% acceptance threshold.
- Only then does the summary become public, with a footnote: "This summary was generated and validated by 32 of 40 members of this opinion group."

This is a *practice*, not just a feature. Document it. Publish the methodology. The transparency report (stage 19) includes validation statistics.

### Watch out for

**Model selection for Bangla.** Llama 3.1 8B is okay at Bangla generation; Llama 3.1 70B (if you can afford the GPU) is noticeably better. Test with native speakers before deploying to a Bangla consultation. Worth fine-tuning a small adapter on policy-domain Bangla text if budget allows.

**Hallucinated positions.** Even with low temperature, LLMs can generate plausible-sounding statements that don't reflect the input. The validation loop catches this empirically, but also include a "this doesn't represent me" reason picker to learn the failure modes.

**Don't show unvalidated summaries publicly.** Ever. Even labeled "draft." The risk of citizens reading and internalizing a wrong summary before validation is too high.

**Prompt versioning.** Treat prompts as code. Version-control them; require code review for changes; log the prompt version with each generation. Changes to summarization prompts can substantively change consultation outputs.

### Cost model

For a national consultation (§2 scale):
- ~5 clauses × 5 clusters × 1 summary per (validation + regen) = ~50 summaries.
- Each summary: ~2000 input tokens + 100 output tokens.
- On a commercial API: ~$5 total per consultation.
- On self-hosted vLLM: GPU time only, free at this volume.

The cost is dominated by the validation UI traffic, not the generation. Don't over-engineer the model side.

### Verification checklist

- [ ] Summaries generated in correct language for input.
- [ ] Validation loop: insufficient acceptance triggers regeneration.
- [ ] Validated summaries published to citizen UI with attribution footnote.
- [ ] Audit query returns inputs for any summary.
- [ ] Cost tracking accumulates correctly.
