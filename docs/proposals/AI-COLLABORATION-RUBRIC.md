# Rubric: AI-collaborative work (process) — draft with evidence rules

Status: **draft for the pilot**, 30 September 2026. It grades *how* a student worked
with AI; the quality of the finished work is graded separately with the subject's own
rubric. Written before building anything because the rubric decides what the engine
must record. Source and rationale: [AI-IN-EDUCATION-RESEARCH.md §8](../research/AI-IN-EDUCATION-RESEARCH.md);
product context: [AI-ASSESSMENT-PROPOSAL.md](AI-ASSESSMENT-PROPOSAL.md).

Wording here is original. It is aligned by level number with the AI Assessment Scale
(Perkins, Roe & Furze, JUTLP 22(7), 2025; CC BY-NC-SA) but does not reproduce it —
permission is needed before embedding the AIAS text or graphics in a paid product.

## How to use it

| Assignment level | Criteria used |
| --- | --- |
| 1 — No AI (supervised) | None; the record only confirms the mode was respected |
| 2 — AI for planning | A, B, E, applied to planning work |
| 3 — AI collaboration | A–E, equal weight suggested |
| 4 — Full AI | A–E, with A and B weighted higher |
| 5 — AI exploration | Co-designed with students, starting from A–E |

Levels: **1 Beginning, 2 Developing, 3 Proficient, 4 Advanced**, plus **NE — not
enough evidence**, which leads to a conversation, never a low score.

## General evidence rules (all criteria)

1. Judge **patterns across at least two sessions or several decisions**, never one event.
2. **Counts, time, prompt length and English fluency are never evidence on their own.**
   More prompts, drafts or reverts are not better.
3. Bangla, English and mixed (Banglish) prompts are equally valid.
4. Only **consequential** decisions count (claims, argument, analysis, data, sources);
   accepting a correct minor fix unchanged is not poor judgement.
5. Text written outside Dynodoc (uploads, pastes) is context, not evidence of AI use;
   declared outside help carries no penalty.
6. For summative use, confirm levels for sampled students with a 2–3 minute oral about
   specific logged decisions; if the oral contradicts the log, the oral wins and the
   teacher records why.
7. The teacher chooses every level and writes the reasons; the system only links evidence.

## Evidence the record provides

| Evidence | Source in Dynodoc |
| --- | --- |
| Prompt text, intent tag, target paragraphs | `agent_turn` (role person), assistant panel |
| Whether the student wrote a paragraph before the first AI proposal on it | Origin projection + event order on the stable block ID |
| AI proposals | `agent_turn` (assistant) + suggestion/draft events |
| Accept / modify (diff) / reject, reason | Per-block decisions on the change request or suggestion |
| Verification | Sources added, notes, comments, reasons mentioning checks |
| Drafts compared, restores with reasons | Draft and restore events; commit and change-request messages |
| AI-touch map of the final text | Origin projection (per paragraph and word) |
| Reflection and declarations | Submission record |
| Oral confirmation | Viva notes linked to the decisions discussed |

## A. Deciding when and how to ask (framing)

*Evidence:* prompts and intents; whether the student's own version of a paragraph existed
before AI input on it; the mode.

| Level | Descriptor | Evidence rule |
| --- | --- | --- |
| 1 | For assessed parts, mostly asks for finished text; prompts lack context ("write my conclusion"). | Most AI proposals on assessed paragraphs come before any student version; intents mostly "write"/"rewrite". |
| 2 | Some own attempts first; prompts generic ("is this good?", "improve this"). | Mixed order; prompts rarely state a goal or criterion. |
| 3 | Usually drafts first, then asks targeted questions stating goal, context and constraints; asks for explanation or critique more than replacement. | Most assessed paragraphs have a student version before AI input; most prompts name a goal or criterion. |
| 4 | Strategic: consults AI on approach, sets how it should help ("question me, don't rewrite"), targets specific weaknesses, can justify not using AI. | Approach and mode-setting prompts, confirmed by the reflection or oral. |

## B. Judging AI output (discernment)

*Evidence:* decisions with reasons; verification; errors in proposals caught or missed.

| Level | Descriptor | Evidence rule |
| --- | --- | --- |
| 1 | Accepts proposals wholesale, including wrong or off-task ones; no checking. | A consequential error accepted unchanged; no reasons. |
| 2 | Mostly cosmetic changes; rejections without reasons; some errors get through. | Surface-level modifications; substantive errors partly caught. |
| 3 | Modifies or rejects with reasons; checks key facts; catches most substantive errors. | Reasons on consequential decisions; visible verification. |
| 4 | Interrogates the AI's reasoning, spots missing context or bias, verifies against sources, improves on the proposal. | Explicit challenges in prompts or reasons; final text better than the proposal. |

## C. Integrating and owning the work (integration and voice)

*Evidence:* AI-touch map; diff between each proposal and the final text; explanations;
oral.

| Level | Descriptor | Evidence rule |
| --- | --- | --- |
| 1 | Core parts (claim, argument, analysis) AI-originated with little change; cannot explain them. | Core paragraphs AI-originated with minimal diff; oral fails. |
| 2 | AI text lightly edited; inconsistent voice; partial explanations. | Small diffs on AI-originated paragraphs; oral partial. |
| 3 | AI contributions reshaped to serve the student's own argument; can explain any part. | Substantial diffs or AI limited to explanation; oral succeeds. |
| 4 | AI clearly subordinate to the student's thinking — extended, critiqued, recombined; precise explanations. | Reasoning beyond the proposals; oral shows depth. |

The AI share is **context, not a score** — at level 4 assignments it may legitimately be high.

## D. Iterating and managing the process (steering)

*Evidence:* follow-up prompts; drafts compared; restores with reasons; messages.

| Level | Descriptor | Evidence rule |
| --- | --- | --- |
| 1 | Single-shot use; no revision after AI input; empty messages. | No follow-ups; boilerplate or no messages. |
| 2 | Some follow-ups that repeat rather than steer; vague messages. | Follow-ups restate the request. |
| 3 | Iterates with specific feedback toward a goal; uses drafts or restores purposefully; messages say what changed and why. | At least one steered sequence ending in an improvement; meaningful messages. |
| 4 | Deliberate experimentation: compares alternatives and chooses with a rationale; changes strategy when AI help isn't working. | Draft comparison or restore with explicit rationale; visible strategy shifts. |

## E. Transparency and reflection

*Evidence:* reflection compared with the log; declarations; mode compliance.

| Level | Descriptor | Evidence rule |
| --- | --- | --- |
| 1 | No account, or one that contradicts the log; undeclared outside material. | Reflection conflicts with the log; undeclared outside uploads. |
| 2 | Generic account ("used AI for grammar"). | Consistent with the log but unspecific. |
| 3 | Accurate, specific account of where and how AI shaped the work. | Names sections and decisions that match the log. |
| 4 | Accurate, specific and evaluative: what the AI did well or badly, its limits and biases, what they would change. | As 3, plus evaluative insight. |

Declared outside AI use or uploads carry no penalty here; undeclared outside text leads
to a conversation, not an automatic misconduct finding.

## What this rubric cannot establish

That the student did not use other devices or tools; authorship of text written
outside Dynodoc; effort, honesty or emotions; anything from typing behaviour (which is
never recorded). Reports must say this in plain words.

## Validation plan

Before any summative use: two teachers score the same anonymised records blind; target
weighted kappa ≥ 0.6 per criterion; check whether levels predict delayed unaided
performance or viva quality; revise descriptors where teachers disagree; test with Bangla
and Banglish prompts and with students using input methods, dictation or assistive tools.
