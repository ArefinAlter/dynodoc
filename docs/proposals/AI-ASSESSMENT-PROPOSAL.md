# Assessing how students work with AI — product proposal

Status: **proposal for a future product line; nothing here is implemented.** Prepared
30 September 2026 from the owner's idea: instead of resisting students' AI use, let
them use AI inside Dynodoc and let teachers evaluate *how* they used it — amount,
where it touched the work, prompts, strategies, turns and judgement — from the
version-control record.

Evidence: [research/AI-IN-EDUCATION-RESEARCH.md](../research/AI-IN-EDUCATION-RESEARCH.md)
(learning science, assessment frameworks, products, detector bias, Bangladesh) and
[research/AI-GOVERNANCE-RESEARCH.md](../research/AI-GOVERNANCE-RESEARCH.md) (EU AI Act,
GDPR, UK exam rules, US, Bangladesh PDP Act 2026; not legal advice). Technical
foundation: [AGENT-PROVENANCE-PROPOSAL.md](AGENT-PROVENANCE-PROPOSAL.md). Rubric:
[AI-COLLABORATION-RUBRIC.md](AI-COLLABORATION-RUBRIC.md).

## 1. The proposition, stated honestly

**Say:** "A transparent record of how you worked with AI, which your teacher can grade
for judgement."
**Never say:** "Proves authorship", "detects cheating", "AI grades AI use".

Why this wording matters:

- Controlled studies find AI help raises scores while available but lowers later
  unaided performance when it *replaces* the student's thinking, and does much less
  harm when it gives hints, explanations or critique after the student's own attempt
  (Bastani et al., PNAS 2025; Lehmann et al.; Liu et al. 2026; Kumar et al., CHI 2025).
  The case is for guardrails and visibility, not bans. (MIT's "Your Brain on ChatGPT" is
  an unreviewed preprint; the most-cited positive meta-analysis was retracted in April
  2026 — neither should be used in marketing.)
- The AI Assessment Scale (AIAS) authors write that document-tracking and version-control
  systems "are fallible and can be circumvented by determined students" and that
  "proving" authorship can be inequitable. Assessment frameworks agree that only
  **supervised** tasks can restrict AI; take-home work is "open" (Sydney's Lane 2,
  AIAS levels 2–5), where the question is *how* AI was used.
- AI detectors are unreliable and biased against non-native writers (seven detectors
  averaged a 61.3% false-positive rate on TOEFL essays; Liang et al., *Patterns*, 2023);
  several universities switched Turnitin's detector off in 2023–2026.
- Existing products (Turnitin Clarity, Grammarly Authorship, GPTZero, Draftback, Brisk,
  Cursive, PowerNotes, Khanmigo Writing Coach, MagicSchool) record where text came from
  and sometimes the chat. None found turns *the student's decisions about each AI
  proposal* into rubric-linked evidence of judgement. Turnitin Clarity's chat does not
  support Bangla; Khanmigo Writing Coach is US-only.

Dynodoc's differentiator is **decision data**: what the AI proposed, on which stable
paragraph, what the student did with it (accept, modify, reject), why, in which draft,
and what they reverted — recorded by an append-only, tamper-evident engine.

## 2. Capture boundary

The record covers only what happens inside Dynodoc. It cannot see phones, other
devices, other AI tools, or text retyped or auto-typed from elsewhere (autotyper
extensions openly advertise defeating version-history review). Every report says so.
Uploaded or pasted outside text is "written outside Dynodoc", never "AI" or
"plagiarised". Bengali input methods, dictation, word prediction and screen-reader
workflows are legitimate input and must not be mislabelled (Grammarly mislabelled voice
typing as pasted).

## 3. Assignment modes (policy, enforced on the server)

A mode is part of the assignment, snapshotted into the record; students cannot switch it.

| Mode | Assistant can | Closest AIAS level | Honest claim |
| --- | --- | --- | --- |
| **Off** | Nothing | 1 No AI — only in a supervised room | "No AI inside Dynodoc." Does not stop AI at home. Exam mode *blocks*; it never watches behaviour. |
| **Ask only** | Explain, question, critique; never write or replace assessed text (enforced by refusing insertion tools and limiting output) | 2 AI Planning (or constrained 3) | The assistant can't write the work. |
| **Unlock after first draft** | Full collaboration on a section once the student has submitted their own first draft of it | 3 AI Collaboration | Own attempt first (supported by timing and productive-failure evidence). |
| **Open, process graded** | Full collaboration; the rubric grades how AI was directed and checked | 4 Full AI (5 if co-designed) | Grades critical thinking shown in directing AI. |

Recommended pairing: a supervised first draft in class (Off) followed by a take-home
revision (Unlock/Open) gives a trustworthy baseline against which the AI-assisted change
is visible.

## 4. Student experience

- The teacher's assignment creates each student's own document (their "fork") inside a
  course project (see [Projects](PROJECTS-WORKSPACE-HANDOFF.md)); the student and the
  teacher can open it, classmates cannot.
- An **assistant panel** with optional intent buttons — *Explain*, *Critique my draft*,
  *Suggest structure*, *Check facts*, *Rewrite* — which double as metacognitive
  scaffolding. Bangla, English and Banglish prompts are equally valid.
- AI output arrives as **proposals** (suggestions) on specific paragraphs. The student
  accepts, modifies or rejects; a one-line reason is optional except for a few
  consequential decisions in assessed modes.
- **Personal drafts** to try alternatives; **restore** with an optional reason;
  checkpoints (*first draft*, *final*); a short **reflection** at submission.
- **Scratch space:** a private conversation area. Its content stays private (only a
  salted hash is chained); anything moved from it into the document keeps its AI origin.
- **No secret dashboards:** students see exactly what the teacher will see before
  submitting, can annotate any entry, and have a **no-penalty declaration** channel for
  outside tools or uploads.
- Mobile-first (only 9.1% of Bangladeshi households have a computer; 75.1% have
  smartphone access — BBS, April–June 2026), low-bandwidth, tolerant of offline work.

## 5. What is recorded — and what is never recorded

Recorded (decision-level, on the existing chain and agent-session tables):
assistant session (assignment, mode, model and version, system-prompt version,
language); each prompt (text, target paragraphs, intent tag); each proposal (text,
type, target); each decision (accept / modify with the resulting diff / reject; reason);
drafts, rebases, restores with reasons; change-request and commit messages; checkpoints;
reflection; span-level origin of the final text; coarse (minute-level) times.

Never recorded: keystroke timing, inter-key intervals, typing rhythm, mouse paths,
webcam, microphone, screen, browser activity outside Dynodoc. Inferring emotions from
keystrokes in education is **prohibited** under the EU AI Act (Art. 5(1)(f), since
February 2025), and keystroke dynamics are behavioural biometrics (sensitive data under
Bangladesh's PDP Act 2026).

## 6. Teacher experience

1. **Process card** per student: mode and level; an AI-touch map by paragraph (none,
   explanation only, suggestion accepted, suggestion modified, AI-originated); prompt
   intents with two or three representative prompts (first, most consequential, one
   rejected); decisions with example reasons; drafts and restores timeline; the
   reflection next to what the log shows.
2. **Evidence-linked rubric**: for each criterion, links to the log excerpts that are
   evidence. The **teacher chooses the level and writes the reasons**; the system never
   proposes a grade (JCQ and Ofqual forbid AI as the sole or primary marker; automated
   decisions about children are restricted).
3. **Drill-down**: full transcript and diffs; replay optional.
4. **Micro-viva picker**: randomly selects 10–20% of students and one or two logged
   decisions for a 2–3 minute conversation ("Why did you reject this suggestion?"). If
   the conversation contradicts the log, the conversation wins and the teacher records why.
5. **Class view**: aggregate prompt intents and common weaknesses (Turnitin reported that
   29% of Clarity prompts asked for general feedback), to *teach* prompting once.

Any summary shown to teachers **extracts and orders observed events** ("prompt 4 → AI
text accepted into paragraph 2 → edited"). It never characterises the student
("over-reliant", "shallow engagement") — that would be profiling and makes the system
high-risk under the EU AI Act (Annex III 3(b)).

## 7. What the system never infers

Stated in the product and the documentation: that pasted or uploaded text is AI or
plagiarised; authorship or identity from typing; effort or honesty from time, sessions
or edit counts; prompting skill from the number, length or English fluency of prompts;
learning from the AI share alone; misconduct from any single signal; any "AI-likeness"
score of text.

## 8. Gaming and its answers

| Risk | Answer |
| --- | --- |
| Prompts written for the grader | Grade decisions and their consequences, not prompt eloquence; rubric states that volume and length are not evidence |
| Outside AI + retyping or autotypers | Capture boundary stated; supervised baselines; micro-vivas; declaration channel so undeclared outside text becomes a conversation, not an automatic zero |
| Performative "rejections" | Only consequential decisions count; viva on sampled decisions |
| Students hiding legitimate AI use | Formative assignments whose logs are not graded; explicit safety of "naive" questions |

## 9. Legal, privacy and ethics requirements (from the governance research)

- **EU AI Act:** keep origin marking rule-based and documented (a deterministic ledger is
  probably not an "AI system"). No automated scoring of prompts or AI use; summaries only
  extract and order events; document an Art. 6(4) assessment if any summariser ships.
  Education high-risk obligations now apply from **2 December 2027** (Reg. (EU)
  2026/1744); Article 50 transparency from 2 August 2026. Tell students the assistant is
  AI. Review marketing copy: intended purpose includes claims.
- **GDPR/UK:** DPIA before launch (evaluation/scoring, vulnerable data subjects,
  systematic monitoring); processor contracts that forbid own-purpose use; host EU/UK
  school data in the EU/UK; full self-access and export; Art. 22 safeguards (teacher
  decides, UI avoids automation bias: no default grades, no red flags).
- **Bangladesh PDP Act 2026 (Act No. 63):** under-18s need guardian consent; biometric
  (including behavioural) data is sensitive; erasure and rectification must propagate to
  backups (s.14(6)); complaint and fine sections start no earlier than about May 2027;
  no explicit data-localisation rule was found. Whether decision-level logs of minors are
  "profiling" and whether a child-profiling ban applies (reported for the 2025
  Ordinance; present in the *draft* National AI Policy) needs a Bangladeshi lawyer.
  Present the log as the **student's own evidence**, not behavioural tracking.
- **Erasure:** crypto-shredding — personal content encrypted with per-student keys,
  salted hashes in the chain — so erasure reaches backups while the chain still verifies.
- **Retention:** delete after the grading and appeal window; no model training,
  analytics or marketing on student content (California AB 1159 bars training on
  student information; the draft Bangladesh policy would too).
- **Fairness:** IME, dictation and assistive input recognised or declarable; Bangla and
  code-mixed prompts equally valid; model pinned per assessment; AI collaboration taught
  before it is graded.
- **Misconduct use:** reports carry a limits-of-observation statement, rule definitions,
  the complete event list available to the student, integrity verification, the
  student's annotations, who accessed the record, the teacher's separate reasons, and the
  appeal route (OIA and JCQ practice: balance of probabilities, full disclosure).
- **Licensing:** the AIAS and the 4D AI Fluency framework are CC BY-NC-SA; cite them and
  let institutions choose a level, but get permission before embedding their wording or
  graphics in a paid product. The rubric in this folder uses original wording.

## 10. Architecture

Built on the agent-provenance work (sessions, turns, origin projection, sealing at
merge) and Projects:

| Concept | Implementation |
| --- | --- |
| Course | Project (workspace kind `project`) with teacher as owner/manager and students as contributors |
| Assignment | Template document + `assignment` row: mode policy, AIAS level, rubric, due dates, checkpoint rules, viva sampling rate |
| Submission | Per-student document created from the template; submit = named version + `ChangeRequestMerged`-style seal of sessions and decisions |
| Assistant | In-editor agent writing proposals only, under the mode policy (server-enforced tool list and output limits) |
| Decisions | Per-block accept / modify / reject with reason (extension of change-request review) |
| Grading | `rubric_evaluation` (teacher, criterion, level or "not enough evidence", reasons, evidence event IDs) — append-only, visible to the student |
| Vivas | `viva_selection` (random, seeded, recorded) and outcome notes |
| Declarations | Student statements of outside tools/uploads, no penalty |

New tables need new migrations; the in-editor assistant needs a scope decision
(decision 000 excludes LLM features from the PoC).

## 11. Bangladesh fit

- **Where it fits:** coursework, school-based assessment, university assignments,
  theses. **Not** SSC/HSC board exams, which are supervised, handwritten Lane-1 exams.
- **Partners and channels:** a2i (MuktoPaath, 2M+ learners; planned Bangla AI-in-education
  teacher course), EdTech Hub, UNESCO Dhaka, universities with AI-use declarations (BRAC
  reportedly uses one — unverified), local platforms (10 Minute School, Shikho) as
  potential partners rather than competitors. No UGC or Ministry guidance on AI in
  assessment was found as of September 2026; the UGC was reported to be drafting one.
- **Language:** Bangla, English and Banglish in the UI and assistant; frontier models
  score ~91–92 on Bengali Global-MMLU-Lite but still lag English on reasoning; validate
  any intent tagging on Bangla prompts before teachers see it.

## 12. Pilot

One university course (and later one school group with guardian consent), AIAS level 3
assignment, ethics approval and informed consent. Measure:
1. inter-rater reliability of rubric levels assigned from logs (two teachers, blind);
2. whether levels predict delayed unaided performance or viva quality;
3. teacher time per submission (target: under 10 minutes with the process card);
4. student perceptions and chilling effects (help-seeking before/after);
5. mislabelled input (IME, dictation) incidents.
Success: acceptable reliability (e.g. weighted kappa ≥ 0.6), no increase in avoidance
of help-seeking, teacher time within target.

## 13. Phasing

1. Agent-provenance prototype (origin projection, sessions) — prerequisite.
2. Projects phase 1 (courses as projects).
3. Assignment modes, assistant panel (propose-only), decisions with reasons, process
   card, rubric evaluation, student view, declarations — English and Bangla.
4. Micro-vivas, class view, exports for misconduct procedures, retention automation.
5. Pilot, then iterate before any wider sale.

## 14. Decisions needed

1. Target first: universities (lower legal risk) or schools (guardian consent)?
2. Model provider(s) for Bangla quality, cost and data location.
3. Draft visibility: teachers see personal drafts only at submission, or never?
4. Whether to seek AIAS permission or keep original descriptors.
5. Whether the intent tag is student-chosen only (recommended) or also machine-suggested
   (needs validation on Bangla and would be an AI component).
6. Counsel in Bangladesh (PDP Act, minors, profiling) and the EU (AI Act classification,
   DPIA) before any pilot with real students.
