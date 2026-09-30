# Process-visible AI writing in Dynodoc: research brief

> Research input for the [AI assessment proposal](../proposals/AI-ASSESSMENT-PROPOSAL.md) and [rubric](../proposals/AI-COLLABORATION-RUBRIC.md), compiled by a research agent on 30 September 2026. Labels [P]/[S]/[U] mark primary, secondary and unverified sources.
>
> **Reconciliation note (Bangladesh law).** Section 6.2 relies on the Daily Star's summary of the 2025 *Ordinance*, which reported a ban on "profiling, behavioural tracking, or targeted advertising directed at minors". The [governance brief](AI-GOVERNANCE-RESEARCH.md) read the Bengali text of the enacted *Personal Data Protection Act, 2026* (Act No. 63) and reports guardian consent for under-18s, a definition of profiling and rights against automated decisions, but attributes the explicit child-profiling and behavioural-tracking ban to the *draft* National AI Policy 2026–2030, which is not adopted. Treat the ban as unconfirmed for the Act until a Bangladeshi lawyer checks it.

Prepared 30 September 2026 for the Dynodoc owner. Scope: evidence and precedents for a product in which students write with an AI assistant inside Dynodoc, every prompt / AI proposal / student decision / draft / revert / commit message is recorded, and teachers assess *how* AI was used.

**Evidence labels used throughout**

- **[P]** primary source opened and read for this brief (paper, official page, vendor documentation, store listing).
- **[S]** taken from a search-engine summary or a secondary report; the primary page was not opened (often because it blocked automated access). Treat as probable, re-check before quoting publicly.
- **[U]** unverified: I looked and could not confirm it.
- **Inference:** my reasoning, not a sourced fact.

Dates are publication dates unless stated. "As of" means the state I found on 30 September 2026.

---

## Executive summary

1. **How students use AI matters more than whether they use it.** Controlled studies repeatedly find that AI raises performance while it is available, but lowers later unaided performance when it *replaces* the student's thinking (asking for answers or finished text), and much less so when it *complements* it (hints, explanations, critique after an own attempt): Bastani et al., PNAS 2025 [P]; Lehmann et al. 2024/25 [P]; Liu et al. 2026 [P]; Kumar et al., CHI 2025 [P]; Contractor & Reyes 2026 [P]. There is counter-evidence that AI practice can improve skill (Lira et al., "Coach not crutch", 2025–26 [P]), so the case is for *guardrails and visibility*, not bans.
2. **The best-known alarming study is weak evidence.** MIT's "Your Brain on ChatGPT" (Kosmyna et al.) is still an unreviewed arXiv preprint (v2, 31 Dec 2025), with n = 54 (18 in the crossover session) and a published methodological critique. Its authors ask people not to describe it as "harm" or "damage" [P]. Its one design-relevant hint (write first, then use the LLM) agrees with better studies on AI *timing* (Qin et al., CHI 2025 [P]).
3. **The meta-analytic literature is shaky.** The most-cited ChatGPT meta-analysis (Wang & Fan 2025, g = 0.867) was **retracted on 22 April 2026** [S]. Critiques show that most primary studies have confounded control groups and short-term outcomes (Weidlich et al., JCAL 2025 [S]). Do not market Dynodoc on "AI improves learning by X".
4. **Assessment frameworks agree on one thing: only supervised tasks can restrict AI.** Sydney's "two-lane" model [P], AIAS Level 1 [P], TEQSA's "security at meaningful points" [S] and JCQ's supervision requirement [P] all say so. Dynodoc's logs therefore belong to *open* (Lane 2 / AIAS 2–5) assessment, where the question is *how* AI was used. They do not make take-home work secure.
5. **The AI Assessment Scale's own authors warn against the pitch as usually phrased.** The 2025 AIAS paper says "document tracking or version control systems ... are fallible and can be circumvented by determined students", and that "proving" authorship can be inequitable [P]. Dynodoc should present its record as evidence for feedback, dialogue and grading of judgement, not as proof of authorship.
6. **Log analytics are promising but not a validated measurement instrument.** Interaction patterns (iterative vs linear, explanation vs substitution) are associated with outcomes in small studies (e.g., Nguyen et al. 2024, 10 doctoral students [P]). Keystroke-process features add only "marginally" to predicting writing quality (Zhang & Deane 2015 [P]), and in one study they barely beat chance (AUC 0.57, Conijn et al. 2022 [P]). I found no validated rubric for grading human–AI collaboration from logs.
7. **Existing products capture composition signals but do not grade judgement.** Turnitin Clarity, Grammarly Authorship, GPTZero, Brisk, Revision History, Cursive, PowerNotes, MagicSchool and Khanmigo record combinations of pastes, typing, replay and (sometimes) AI chats. None that I found turns logged AI decisions into rubric-linked grading of judgement. All can be circumvented: autotyper extensions openly advertise bypassing Draftback, GPTZero and Originality.ai [P], and Grammarly's "Humanize" relabelled AI text as "Typed by a Human" [P].
8. **AI detectors are unreliable and biased against non-native writers.** Liang et al. (Patterns, July 2023) found that seven detectors averaged a 61.3% false-positive rate on TOEFL essays [P]. Vanderbilt (2023), Waterloo (Sept 2025) and Curtin (Jan 2026) turned off Turnitin's AI detector, and Washington State dropped its contract (Feb 2026) [P/S]. That supports a "no detection" stance, especially for Bangla-medium students writing in English.
9. **In Bangladesh, three facts constrain the design.** (a) The National AI Policy 2026–2030 is still a draft: consultation closed in February 2026, and in July 2026 the minister described it as still being formulated [P]. (b) The personal-data law (Ordinance 2025, reportedly enacted as an Act in April 2026) requires parental consent for under-18s. It is reported to prohibit "profiling, behavioural tracking" of minors [S], which directly affects SSC/HSC-age logging. (c) Only 9.1% of households have a computer, against 75.1% with smartphone access (BBS, Apr–Jun 2026 [P]). Also, Turnitin Clarity's AI chat supports 20 languages, but not Bangla [P].
10. **Design direction.** Log *decisions*, not keystrokes. Give teachers a one-page "process card" linked to a five-criterion rubric with explicit evidence rules. Never infer authorship, cheating or effort from timing or pastes. Prevent gaming structurally: no volume metrics, plus short oral checks drawn from logged decisions. Get legal advice on minors' data. Note that the AIAS and the 4D AI Fluency framework are licensed **CC BY-NC-SA** (non-commercial) [P], so a commercial product needs permission to reuse their wording or graphics.

---

## 1. Evidence on AI, learning and cognition

### 1.1 Kosmyna et al., "Your Brain on ChatGPT" (MIT Media Lab)

- **Design [P].** Participants were split into three conditions: LLM (ChatGPT), search engine, and "Brain-only". EEG was recorded while they wrote essays. 54 participants completed sessions 1–3; 18 completed a crossover session 4, in which LLM users wrote unaided and Brain-only users switched to the LLM. Essays were scored by NLP analysis and human raters. Source: arXiv 2506.08872, v1 10 June 2025, v2 31 December 2025, 216 pages, no journal reference. https://arxiv.org/abs/2506.08872
- **Findings, as stated in the abstract [P].**
  - "Brain-only participants exhibited the strongest, most distributed networks; Search Engine users showed moderate engagement; and LLM users displayed the weakest connectivity."
  - In session 4, LLM-to-Brain participants showed "reduced alpha and beta connectivity, indicating under-engagement", while Brain-to-LLM participants "exhibited higher memory recall and activation of occipito-parietal and prefrontal areas".
  - Self-reported essay ownership was lowest in the LLM group, and "LLM users also struggled to accurately quote their own work".
  - A widely repeated figure is that 83.3% of LLM users could not quote their own essay in session 1 [S, via search summary of the paper].
- **Publication status [P].** Still a preprint. The project FAQ says peer review "has begun" but will take "months to years". As of September 2026 I found no journal version (a Frontiers 2026 article and a May 2026 review both describe it as a preprint [S]). https://www.media.mit.edu/projects/your-brain-on-chatgpt/overview/
- **Limitations the authors acknowledge [P].**
  - A small sample from one geographic area.
  - ChatGPT only; an essay task only.
  - Limited EEG analysis.
  - The FAQ asks readers not to use words like "stupid", "dumb", "brain rot", "harm" or "damage".
- **External critique [P].** Stanković, Hirche, Kollatzsch & Doetsch (arXiv 2601.00856, 29 December 2025) raise concerns about statistical power, reproducibility, the EEG analysis, reporting inconsistencies and transparency, and argue that results "could be interpreted more conservatively". https://arxiv.org/abs/2601.00856
- **Use for Dynodoc.** Inference: cite it only as a hypothesis-generating study. Its session-4 asymmetry (writing unaided first, then using AI, went better than the reverse) is suggestive support for an "unlock after first draft" mode, but n = 18 is far too small to rely on.

### 1.2 Bastani et al., "Generative AI without guardrails can harm learning" (PNAS, 25 June 2025)

- **Design [P].** A field RCT with nearly 1,000 students in grades 9–11 at a large high school in Turkey, run over four 90-minute sessions in Fall 2023 (about 15% of the semester's maths curriculum). Two arms used GPT-4:
  - **GPT Base:** a ChatGPT-like interface.
  - **GPT Tutor:** prompted to give hints rather than answers, supplied with teacher-written solutions to reduce hallucination, and given common student errors with feedback strategies.
  
  PNAS 122(26). https://pmc.ncbi.nlm.nih.gov/articles/PMC12232635/
- **Results [P].**

  | Arm | Practice, with AI (vs control) | Later exam, without AI (vs control) |
  |---|---|---|
  | GPT Base | +48% | −17% |
  | GPT Tutor | +127% | Statistically indistinguishable from control |

- **Chat logs [P].** "Students in GPT Base most often simply ask for the answer; in contrast, students in GPT Tutor learn to interact more substantively ... by asking for help and independently attempting to solve the problem." Messages were clustered into "Ask for Answers" and "Repeat Question Text" (coded superficial) versus "Attempted Answers" and "Ask for Help" (non-superficial).
- **Self-perception [P].** GPT Base students "did not perceive that they performed worse or learned less".
- **Correction [P].** The correction of 26 August 2025 fixed an author affiliation only; it did not change findings. https://pmc.ncbi.nlm.nih.gov/articles/PMC12403119/
- **Relevance.** This is the strongest evidence that the *interaction design* of the assistant, and the *type* of student request, determine learning. It also shows students cannot judge the effect themselves, which is an argument for teacher-visible process data.

### 1.3 Lee et al., CHI 2025 (Microsoft Research and Carnegie Mellon)

- **What it is [P].** A survey of 319 knowledge workers (not students), who shared 936 examples of using GenAI at work. https://www.microsoft.com/en-us/research/publication/the-impact-of-generative-ai-on-critical-thinking-self-reported-reductions-in-cognitive-effort-and-confidence-effects-from-a-survey-of-knowledge-workers/
- **Confidence effects [P].** Higher confidence in GenAI was associated with *less* self-reported critical thinking; higher self-confidence was associated with *more*.
- **Where the effort goes [P].** Critical-thinking effort shifts from information gathering to verification, from problem-solving to integrating AI responses, and from task execution to "task stewardship".
- **Limits.** Self-report, cross-sectional, adults at work. Inference: it supports rubric criteria on *verification* and *integration* of AI output, rather than on generation.

### 1.4 Gerlich 2025 (Societies 15(1):6, January 2025)

- **What it found [S].** 666 UK participants, surveys plus interviews. AI-tool use correlated with cognitive offloading (r = +0.72), and offloading correlated negatively with critical thinking (r = −0.75). Younger participants showed more dependence. https://papers.ssrn.com/sol3/papers.cfm?abstract_id=5082524
- **Correction.** A correction was published as Societies 15(9):252 (https://doi.org/10.3390/soc15090252). I could not open it, so its content is [U].
- **Limits.** Correlational, self-report and convenience-sampled; it cannot show causation.

### 1.5 Fan et al., "Beware of metacognitive laziness" (BJET 56(2):489–530, 2025)

- **Design [P].** A lab RCT with 117 university students on a writing task. Four conditions: ChatGPT, a human expert, a writing-analytics checklist, and no support. https://eric.ed.gov/?id=EJ1460793
- **Results [P].**
  - No difference in intrinsic motivation.
  - Significant differences in the frequency and sequence of self-regulated-learning processes.
  - The ChatGPT group improved essay scores most, but knowledge gain and transfer did not differ.
  - The authors warn ChatGPT may "promote learners' dependence on technology and potentially trigger 'metacognitive laziness'".
- **Relevance.** A better product is not the same as more learning. Inference: this supports grading the process and the student's own understanding (for example via a short oral check), not just the final essay.

### 1.6 Other controlled studies directly relevant to assistant design

| Study | Design | Finding relevant to Dynodoc |
|---|---|---|
| Lehmann, Cornelius & Sting, "AI Meets the Classroom" (arXiv 2409.09047, 2024) [P] | Lab and field studies | Asking the LLM for **solutions** (substitution) increased the number of topics covered but decreased understanding. Asking for **explanations** (complement) increased understanding. In a field setting with only substitutive use, there was a long-term decline in learning. |
| Kumar et al., CHI 2025 (arXiv 2410.03703) [P] | Two pre-registered experiments, n ≈ 1,100 | LLM help boosted creativity while it was available but could hinder later **unassisted** creative performance. The study also tested a "coach-like" LLM condition. |
| Qin et al., "Timing Matters", CHI 2025 (arXiv 2502.06197) [P] | n = 60 | Using an LLM **from the start** of ideation reduced original ideas, creative self-efficacy and self-credit. Introducing it **after** independent ideation increased idea quantity and reduced similarity to LLM ideas. The authors recommend delaying LLM use. |
| Liu, Christian, Dumbalska, Bakker & Dubey, "AI assistance reduces persistence..." (arXiv 2604.04721, 2026) [P] | Three RCTs, n = 1,222 | After AI was removed, participants gave up more often and performed worse. The harm was strongest among those who sought **direct solutions** rather than hints. |
| Contractor & Reyes, "Experimental evidence on the learning impact of generative AI" (arXiv 2607.08849, July 2026) [P] | RCT with undergraduates, immediate and one-week-delayed tests | AI access raised immediate scores by 0.27 SD. "Augmentation" users (AI to explain concepts) had larger delayed gains. "Automation" users (AI generating text) saw their quality gains vanish without AI. |
| Lira, Rogers, Goldstein, Ungar & Duckworth, "Coach not crutch" (arXiv 2502.02880, v4 18 February 2026) [P] | Pre-registered experiments on cover-letter writing | AI-assisted practice reduced effort **but improved** later no-AI writing. Viewing AI-revised examples helped nearly as much. This is important counter-evidence: less effort does not always mean less learning. |
| Christenson et al., "Effects of varying LLM access on essay writing" (arXiv 2606.00250, 2026) [P] | Pilot, n = 24 | Quality was the same across no, limited (3 prompts, 100-word caps) and unlimited access. The limited-access group reported more ownership and used more revision-focused prompting. Pilot only. |
| Kestin et al., Scientific Reports, 2 June 2025 [S] | RCT in a Harvard physics course | A pedagogically designed AI tutor beat in-class active learning on learning gains in less time. This shows a *designed* tutor can help. https://www.nature.com/articles/s41598-025-97652-6 |

### 1.7 Meta-analyses: use with great caution

- **Deng et al.** (Computers & Education 227:105224, 2025) [S]. 69 experiments, 2022–2024, with an overall g ≈ 0.71 on performance. The authors themselves flag the lack of power analysis and concerns about post-test designs. https://www.sciencedirect.com/science/article/pii/S0360131524002380
- **Wang & Fan** (Humanities & Social Sciences Communications 12:621, May 2025). 51 studies; g = 0.867 on performance and ≈ 0.46 on higher-order thinking. **Retracted 22 April 2026** over "discrepancies" that "undermine the confidence the editor can place in the validity of the analysis". It had been cited about 270 times (GovTech, 6 May 2026 [P]). https://www.govtech.com/education/nature-retracts-oft-cited-paper-on-positive-impact-of-chatgpt
- **Liu, Zuo & Lu** (JCAL 41(4):e70096, 2025) [S]. 37 studies; g = 0.577. https://onlinelibrary.wiley.com/doi/10.1111/jcal.70096
- **A 2026 HSS Communications meta-analysis** (s41599-026-07019-z) [S]. 35 studies, 4,193 participants; g = 0.670. I could not open the page.
- **Weidlich, Gašević et al., "ChatGPT in Education: An Effect in Search of a Cause"** (JCAL, 2025) [S]. They audited primary studies in the Deng meta-analysis and found that "only a small minority" met three basic conditions: a clear description of the treatment, a clear description of what the control group did, and outcomes that are valid indicators of durable learning. Their conclusion is that gains "cannot, at this time, be confidently attributed to ChatGPT". https://onlinelibrary.wiley.com/doi/10.1111/jcal.70105
- **Microsoft Research review** (October 2025) [S]. It notes that higher-order thinking "can be compromised if GenAI is used in ways that bypass necessary struggle", and that general-purpose tools "without scaffolding or other pedagogical guardrails" can be detrimental. https://www.microsoft.com/en-us/research/publication/learning-outcomes-with-genai-in-the-classroom-a-review-of-empirical-evidence/
- **Productive struggle base rate.** Sinha & Kapur's meta-analysis of productive failure (Review of Educational Research, 2021; 53 studies, 166 comparisons) found problem-solving *before* instruction improved conceptual understanding and transfer (d = 0.36) without hurting procedural knowledge [S]. https://journals.sagepub.com/doi/10.3102/00346543211019105

### 1.8 Anthropic education reports

- **"How university students use Claude" (8 April 2025) [P].**
  - Data: 574,740 anonymised conversations from Claude.ai accounts linked to higher-education email addresses, over an 18-day window.
  - Four interaction patterns: Direct vs Collaborative crossed with Problem Solving vs Output Creation. Each appeared "at similar rates (each between 23% and 29% of conversations)". About 47% were "Direct".
  - Bloom's taxonomy was applied to **Claude's** responses, not to students' thinking: Creating 39.8%, Analyzing 30.2%, Applying 10.9%, Understanding 10.0%, Remembering 1.8%.
  - The report worries this "inverted pyramid" means AI does the higher-order work ("An inverted pyramid, after all, can topple over"). It acknowledges examples of seeking direct test answers and rewriting text to evade plagiarism detection.
  - Limits: early adopters, a short window, and no data on learning outcomes.
  - https://www.anthropic.com/news/anthropic-education-report-how-university-students-use-claude
- **"The AI Fluency Index" (23 February 2026) [P].** 9,830 conversations from January 2026, coded for 11 observable behaviours from the 4D AI Fluency Framework. Prevalence of each behaviour:

  | Behaviour | Share of conversations |
  |---|---|
  | Iterates and refines | 85.7% |
  | Clarifies goal before asking for help | 51.1% |
  | Provides examples of what good looks like | 41.1% |
  | Specifies format and structure needed | 30% |
  | Sets interaction mode | 30% |
  | Communicates tone and style preferences | 22.7% |
  | Identifies when AI might be missing context | 20.3% |
  | Defines audience for the output | 17.6% |
  | Questions when AI reasoning doesn't hold up | 15.8% |
  | Consults AI on approach before execution | 10.1% |
  | Checks facts and claims that matter | 8.7% |

  Other points:
  - When Claude produced an artifact (a finished document or code), users were less likely to question its reasoning (−3.1 points) or to flag missing context (−5.2 points).
  - Limits: early adopters; only 11 of 24 indicators are observable in chat; binary coding; correlational.
  - https://academy.claude.com/tutorials/the-ai-fluency-index
  - Inference: these 11 behaviours are the closest published, *observable-from-logs* operationalisation of "prompting competence", and a good starting vocabulary for Dynodoc's rubric.

### 1.9 Vendor "learning modes" (2025–2026)

| Mode | Date | What it does | Limitation relevant to Dynodoc |
|---|---|---|---|
| OpenAI ChatGPT **study mode** | 29 July 2025 [S] | Custom system instructions developed with educators and experts from more than 40 institutions: Socratic questions, hints, self-reflection prompts, scaffolded responses, level checks. | Per EdWeek (July 2025) [P]: "the student can also just switch off study mode ... There are currently no guardrails against that, according to OpenAI." Leon Furze (30 July 2025) [P] found it answered directly on some models. It is a system prompt with no teacher lock. OpenAI's own page blocked automated access. |
| Anthropic Claude **Learning mode** | Launched April 2025 for Claude for Education; extended to all users and to Claude Code ("Explanatory" and "Learning" styles) around 13–14 August 2025 [S] | Socratic guidance instead of immediate answers. In Claude Code, it pauses to have the user write parts of the code. | A user-selectable style; no teacher enforcement. https://www.engadget.com/ai/anthropic-brings-claudes-learning-mode-to-regular-users-and-devs-170018471.html |
| Google Gemini **Guided Learning** | 6 August 2025 [S] | Built on LearnLM. Step-by-step breakdown, visuals and quizzes; "not just the answer". | User-selectable. In Google Classroom, teachers see who *opened* or *started a conversation with* an assigned Gem or notebook, not the contents [P]. Only Workspace admins can export conversations via Vault [S]. https://blog.google/products-and-platforms/products/education/guided-learning/ |
| Microsoft 365 Copilot **Study and Learn agent** | Blog of 13 May 2026 [P] | "The learner does the thinking"; built on adaptive scaffolding, productive struggle, active learning and transfer. For students aged 13+. | Optimised for US English at launch; I did not find Bengali listed. Assignment of the agent by teachers was "coming" [S]. |

Inference: all four are *student-selectable prompts on top of a general model*. None gives teachers an enforceable mode plus a decision-level record. That gap is exactly what Dynodoc's exam modes plus logging would fill, provided the modes are enforced server-side.

### 1.10 What the evidence supports for a guardrailed, process-visible assistant

| Design choice | Evidence | Strength |
|---|---|---|
| Default the assistant to hints, explanations and critique, not finished text for assessed parts | Bastani (GPT Tutor); Lehmann (explanations vs solutions); Liu 2026 (hints vs solutions); Contractor & Reyes (augmentation vs automation) | Moderate: several RCTs agree in direction |
| Require an own attempt before AI help on a section ("unlock after first draft") | Qin et al. (timing); productive-failure meta-analysis; Kosmyna session 4 (weak) | Moderate for ideation and conceptual learning; indirect for essay writing |
| Limit volume rather than ban (e.g., prompt budgets) | Christenson pilot (n = 24) | Weak: pilot only |
| Make AI's contribution and the student's decisions visible, because students misjudge their own learning | Bastani self-perception finding; Fan (performance is not learning) | Moderate |
| Don't assume less effort means less learning | Lira et al. "Coach not crutch" | Moderate: a counterweight to over-restriction |
| Teach and grade verification and integration, not generation | Lee et al. (effort shifts to verification and stewardship); Anthropic Fluency Index (fact-checking at 8.7% is rare) | Moderate (descriptive) |

---

## 2. Assessment frameworks for the AI era

### 2.1 The AI Assessment Scale (AIAS): Perkins, Furze, Roe & MacVaugh

**Original version** (arXiv 2312.07086, v1 12 December 2023; published in the Journal of University Teaching and Learning Practice 21(6), 2024) [P]. Level names and wording, as reproduced in Figure 2 of the 2025 paper [P]:

1. **NO AI**: "The assessment is completed entirely without AI assistance ... AI must not be used at any point during the assessment."
2. **AI-ASSISTED IDEA GENERATION AND STRUCTURING**: "AI can be used ... for brainstorming, creating structures, and generating ideas ... No AI content is allowed in the final submission."
3. **AI-ASSISTED EDITING**: "AI can be used to make improvements to the clarity or quality of student created work ... but no new content can be created using AI. AI can be used, but your original work with no AI content must be provided in an appendix."
4. **AI TASK COMPLETION, HUMAN EVALUATION**: "AI is used to complete certain elements of the task, with students providing discussion or commentary on the AI-generated content ... Any AI created content must be cited."
5. **FULL AI**: "AI should be used as a 'co-pilot' ... You may use AI throughout your assessment ... and do not have to specify which content is AI generated."

**Revised version.** Announced by Furze on 28 August 2024 [P]; preprint "The AI Assessment Scale Revisited" (arXiv 2412.09029, 12 December 2024) [P]; final paper Perkins, Roe & Furze, "Reimagining the AIAS" (JUTLP 22(7), online 27 September 2025, CC BY-ND 4.0; https://doi.org/10.53761/rrm4y757) [P]. The current website wording (v2.1) at https://aiassessmentscale.com/ [P]:

1. **No AI**: "This task is completed in a controlled environment designed to exclude AI. Knowledge, understanding, and skills are demonstrated and assessed independently."
2. **AI Planning**: "This task focuses on planning activities such as topic exploration, outlining, and initial research. AI may be used to support this process, and the quality of planning and idea development is assessed whether or not AI was used."
3. **AI Collaboration**: "AI may be used to help complete this task, including idea generation, drafting, feedback, and refinement. It is designed so that AI alone will not reach the required standard. Assessment covers both the work itself and how AI outputs are evaluated, modified, and integrated."
4. **Full AI**: "There is an expectation of AI involvement in this task. The goal cannot be reached by AI or by a person working alone in the time available. Assessment focuses on the critical thinking and subject knowledge shown in directing AI."
5. **AI Exploration**: "This task is designed for creative AI use to solve problems, generate novel insights, or develop innovative solutions in the discipline. Approaches may be co-designed by students and instructors."

(In the body of the 2025 paper, Levels 2 and 3 are also headed "AI-Assisted Planning" and "AI-Assisted Task Completion" [P].)

**Points from the 2025 paper that matter for Dynodoc [P]:**

- **Level 1 needs a controlled environment.** Level 1 "should occur in supervised settings where the absence of AI can be assured, rather than relying on honour systems or detection tools for take-home work".
- **The Level 3 appendix of "original work" was removed** because "current detection methods cannot reliably verify such submissions".
- **The authors directly critique the verification pitch.** "Although technological solutions exist that purport to show how students develop their work, such as document tracking or version control systems, they are fallible and can be circumvented by determined students. More importantly, the emphasis on 'proving' original authorship can create inequitable conditions..." This is the most important sentence in this brief for positioning.
- **Process evidence should be designed into the task.** Tasks should make planning "visible and assessable, for example, through prompt rationales, planning notes". Evaluation can "focus on specified elements such as judgement, verification processes, and collaborative decision-making, supported by brief evidence like prompt rationales, checking notes, or short peer or oral reviews". A Level 3 task "might ask students to ... submit a short prompt log or reflection".
- **Labels without redesign are ignored.** "Simply labelling an unchanged task 'Level 3' ... is a discursive change that students can ignore" (drawing on Corbin, Dawson & Liu 2025).
- **Detection is rejected for summative use**, and "security theatre" is criticised.

**Licence [P].** The website says the AIAS diagrams and designated assets are "licensed under CC BY-NC-SA 4.0" and are "free to use on a non-commercial basis"; it describes the AIAS as owned by Learning Innovation Practice Ltd. Inference: Dynodoc can *cite* the AIAS and let institutions choose a level, but reproducing the level texts or graphics inside a paid product needs permission.

### 2.2 University of Sydney "two-lane" approach

- **Lane 1, "secure" [P].** Assessment *of* learning, in-person and supervised: vivas, oral exams, in-class tasks.
- **Lane 2, "open" [P].** Assessment *for* and *as* learning, where AI use is scaffolded. "It is already not possible to restrict or detect AI use in assessments which are not secured."
- **Lane 2 focuses on the discipline [P].** Lane 2 is "primarily about learning the 'stuff, skills, and soul'" of the discipline, not AI proficiency for its own sake (Bridgeman & Liu FAQ, 2 July 2024). https://educational-innovation.sydney.edu.au/teaching@sydney/frequently-asked-questions-about-the-two-lane-approach-to-assessment-in-the-age-of-ai/
- **Policy dates [P].** Announced 27 November 2024. From Semester 1 2025, AI use is allowed by default in open assessments; full two-lane operation followed from Semester 2 2025. https://www.sydney.edu.au/news-opinion/news/2024/11/27/university-of-sydney-ai-assessment-policy.html
- **Relationship to the AIAS [P].** The 2025 AIAS paper maps AIAS Level 1 to Lane 1 and Levels 2–5 to Lane 2, and calls the two frameworks complementary (the AIAS is about design; the two lanes are about security).

### 2.3 TEQSA (Australia's regulator)

- **"Assessment reform for the age of artificial intelligence"** (November 2023; Lodge, Howard, Bearman, Dawson & associates; 18 experts) [S].
  - **Two principles:**
    1. "Assessment and learning experiences equip students to participate ethically and actively in a society where AI is ubiquitous."
    2. "Forming trustworthy judgements about student learning in a time of AI requires multiple, inclusive and contextualised approaches to assessment."
  - **Five propositions:**
    1. Appropriate, authentic engagement with AI.
    2. A systemic approach to program assessment aligned with disciplines or qualifications.
    3. An emphasis on the process of learning.
    4. Opportunities for students to work appropriately with each other and AI.
    5. Security at meaningful points across a program to inform decisions about progression and completion.
  - The PDF timed out for me; this wording comes from search summaries of the document. https://www.teqsa.gov.au/sites/default/files/2023-09/assessment-reform-age-artificial-intelligence-discussion-paper.pdf
- **"Enacting assessment reform in a time of artificial intelligence"** (24 September 2025) [S]. It describes three pathways: program-wide reform, assuring learning in every unit, and hybrids. Commentary summarises it as "AI detection tools can't guarantee integrity". https://www.teqsa.gov.au/guides-resources/resources/corporate-publications/enacting-assessment-reform-time-artificial-intelligence

### 2.4 JCQ "AI Use in Assessments" (UK exam boards)

- **Latest version: 30 April 2025 [P]**, the version on JCQ's knowledge hub as of today. The PDF is https://www.jcq.org.uk/wp-content/uploads/sites/2/2025/09/AI-Use-in-Assessments_Apr25_FINAL.pdf. I found no later revision [U].
- **Key rules [P]:**
  - Work must be the student's own. AI-generated content reproduced in the work must be identified, and it will not earn credit towards the criteria.
  - Students must keep "a copy of the question(s) and computer-generated content ... in a non-editable format (such as a screenshot)" and explain how it was used.
  - Detection tools are only "part of a holistic approach".
  - Centres must ensure "sufficient portions of work" are done "in class under direct supervision".
  - Teachers must not use AI as the sole means of marking [S].
- **Inference.** In UK-style coursework (and any Bangladesh board that adopts similar rules), Dynodoc's tamper-evident prompt/output record could *replace screenshots* as the acknowledgement artefact. That is a concrete, defensible use case.

### 2.5 UNESCO

- **"Guidance for generative AI in education and research"** (7 September 2023) [P]. It calls for data-protection standards, human agency, inclusion, and cultural and linguistic diversity. It proposes an **age limit of 13** for AI tools in the classroom, and notes that fewer than 10% of institutions had policies at the time. https://www.unesco.org/en/articles/unesco-governments-must-quickly-regulate-generative-ai-schools
- **"AI competency framework for students"** (September 2024) [S]. 12 competencies across four dimensions (human-centred mindset, ethics of AI, AI techniques and applications, AI system design), at three levels (Understand, Apply, Create). https://www.unesco.org/en/articles/ai-competency-framework-students

### 2.6 OECD and European Commission

- **AI Literacy Framework for Primary and Secondary Education** (draft May 2025; final "Empowering Learners for the Age of AI", June 2026) [S]. Four domains: engage with AI, create with AI, manage AI, shape AI. It feeds the PISA 2029 innovative domain. https://ailiteracyframework.org/ ; https://www.oecd.org/en/publications/empowering-learners-for-the-age-of-ai_65cd27d4-en.html
- **PISA 2029 Media & AI Literacy (MAIL).** The first draft framework was published in January 2026, and results are expected in December 2031 [S]. It assesses 15-year-olds' competence in engaging "effectively, ethically and responsibly" with media and AI. https://www.oecd.org/en/about/projects/pisa-2029-media-and-artificial-intelligence-literacy.html
- Inference: these frameworks describe *what* AI literacy is; they are not rubrics for grading an individual essay's AI process.

### 2.7 Interactive oral assessment (IOA) and viva

- **Integrity and engagement.** Sotiriadou et al. (Studies in Higher Education 45(11), 2020) found IOAs promoted integrity, engagement and employability skills [S]. https://www.tandfonline.com/doi/full/10.1080/03075079.2019.1582015
- **Design quality decides validity.** Nallaya et al.'s systematic review (Issues in Educational Research 34(2), 2024; 17 included studies) found oral assessment's validity, reliability and capacity to reduce integrity breaches "were dependent on whether it has been designed, scaffolded, and implemented well". The reported concerns were inter-rater reliability, student anxiety, and equity for students speaking English as an additional language [P]. http://www.iier.org.au/iier34/nallaya.pdf
- **Workload (modelled, not observed).** A conceptual paper estimated about 17 hours of direct assessment per 100 students at 10 minutes each, or 25–35 hours with preparation and moderation (Nikolić & Basta Nikolić, Frontiers in AI, 16 July 2026) [P]. https://www.frontiersin.org/journals/artificial-intelligence/articles/10.3389/frai.2026.1841682/full
- Inference: *sampled*, short orals that ask about 1–2 specific logged decisions keep the integrity benefit at a fraction of the cost.

### 2.8 Process-based and portfolio assessment of writing

- **Structural vs discursive change.** Corbin, Dawson & Liu, "Talk is cheap" (Assessment & Evaluation in Higher Education 50(7):1087–1097, 2025) [S] distinguish *discursive* changes (rules students can ignore) from *structural* ones (task mechanics), and warn of an "enforcement illusion". https://doi.org/10.1080/02602938.2025.2503964
- **Evaluative judgement.** Bearman, Tai, Dawson, Boud & Ajjawi (Assessment & Evaluation in Higher Education 49(6):893–905, 2024) [S] propose developing students' evaluative judgement of AI *outputs* and AI *processes*. https://www.tandfonline.com/doi/full/10.1080/02602938.2024.2335321
- **Systematic review.** Abdellatif et al., "Reimagining writing assessment for the AI era" (Frontiers in Psychology, 26 May 2026; 19 studies) [P] recommends draft documentation, "prompt engineering logs" and reflection, and warns against detection-focused surveillance. https://pmc.ncbi.nlm.nih.gov/articles/PMC13246662/

### 2.9 Rubrics and instruments for human–AI collaboration or "prompting competence"

- **4D AI Fluency Framework** (Dakan & Feller, with Anthropic) [P]: Delegation, Description, Discernment, Diligence. Licensed **CC BY-NC-SA 4.0**. https://aifluencyframework.org/ . Anthropic's 11 observable behaviours (section 1.8) operationalise part of it from logs.
- **Framing–Judging–Steering (CoRe-3)** (Apartsin & Aperstein, arXiv 2606.05983, June 2026) [P]. It separates specifying the task, judging AI output, and steering it. It is assessed on a platform that presents *flawed AI outputs*; validity evidence so far comes from simulated learners, with human-rater studies planned.
- **Knoth et al.** (Computers & Education: AI, 2024) [S]. Prompt-engineering skill predicted LLM output quality, and aspects of AI literacy predicted prompting quality. https://www.sciencedirect.com/science/article/pii/S2666920X24000262
- **"Use Me Wisely"** (Ognibene et al., arXiv 2503.02532; accepted in Educational Technology & Society) [P]. LLM-based detectors of prompt-quality features in learner prompts; GPT-4 was robust, earlier models inconsistent.
- **MAILS** (Carolus et al., 2023) [S]. A validated *self-report* questionnaire (34 items), not a performance rubric. https://arxiv.org/abs/2302.09319
- **Gap.** I found **no published, validated rubric** that grades a student's AI-collaboration *process* from interaction logs for summative purposes. The draft rubric in section 8 should be treated as a hypothesis to validate, and validation is itself a research partnership opportunity.

---

## 3. Research that analyses student–AI interaction logs

### 3.1 Chat and interaction logs

| Study | Data | Method | Finding |
|---|---|---|---|
| Nguyen, Hong, Dang & Huang (Studies in Higher Education 49(5), 2024) [P] | 626 recorded activities from 10 doctoral students writing with a GenAI tool | Content analysis, hidden Markov models, sequence clustering, process mining | Iterative, highly interactive use went with better writing; using GenAI as a supplementary source with linear writing went with lower performance. https://www.tandfonline.com/doi/full/10.1080/03075079.2024.2323593 |
| Bastani et al. (PNAS 2025) [P] | Tutor chat logs | Clustering of messages | Answer-seeking dominated in GPT Base; help-seeking and own attempts grew in GPT Tutor (see 1.2). |
| Yang, Fan, Tang, Raković, Li, Gašević & Chen (arXiv 2508.10310, August 2025) [P] | Trace data from GenAI-assisted writing | Hidden Markov models of "hidden tactics" underlying self-regulated-learning strategies | Three learner groups; different strategies led to different task outcomes. |
| Jin et al., "RelianceScope" (arXiv 2602.16251, 2026) [P] | Chat and code-edit logs from 79 students | Nine reliance patterns (help-seeking × response-use) | LLMs can reliably detect reliance patterns against an annotated benchmark. Common struggles were articulating knowledge gaps and adapting AI responses. |
| Li, Li, Xia, Tang, Chen, Lin, Gašević, Gašević & Fan (arXiv 2608.28619, July 2026) [P] | 1,030 GenAI virtual-patient dialogues from 210 medical students | Layered coding into "teacher-interpretable process evidence" | High-rated consultations were distinguished not by volume but by *connections*: summarising and organising moves led to verification. No reliability statistics reported. |
| Contractor & Reyes (2026) [P] | RCT logs | Classified use as augmentation vs automation | Augmentation predicted delayed gains (see 1.6). |
| Lehmann et al. (2024) [P] | Prompts | Explanation vs solution requests | Explanations built understanding; solutions reduced it. |
| Turnitin Clarity vendor data (24 February 2026) [P] | Student prompts in Clarity | Vendor categorisation | 29% of prompts asked for "review, judgment, or other general feedback", such as "Is this good?". Only 36% of those were judged "effective". 94% of students wrote their own prompts rather than using suggestions. Not peer-reviewed. |
| Anthropic AI Fluency Index (February 2026) [P] | 9,830 conversations | Binary coding of 11 behaviours | Iteration co-occurs with other fluency behaviours; verification is rare (see 1.8). |

**Summary.** Two findings replicate across these studies:

- The *type* of request matters: explanation, critique or help versus a finished answer.
- *Iteration with evaluation* matters.

These are exactly the signals a decision-level log can capture. Samples are small or vendor-run, however, and effects are associative.

### 3.2 Keystroke logging and writing-process features

- **Tools.** Inputlog (University of Antwerp, since 2003) logs keystrokes, mouse movements and speech input. See Leijten & Van Waes, "Keystroke Logging in Writing Research", Written Communication 30:358–392 (2013) [S]. https://eric.ed.gov/?id=EJ1014302 . ScriptLog (Lund University) is the other classic research logger; I did not open a primary source for it [U].
- **What predicts quality.** Zhang & Deane (ETS RR-15-27, 2015) [P] found four process factors (fluency, local editing, phrasal/chunk editing, planning and deliberation). Only fluency and planning/deliberation related to quality, and "the 4 factors altogether accounted for limited variance in human scores". Deane & Zhang (RR-15-26) report "moderate to strong prediction" using stable features [P]. https://eric.ed.gov/?id=EJ1109306
- **A near-null result.** Conijn, Cook, van Zaanen & Van Waes (International Journal of AI in Education 32(4), 2022) [P] studied 126 L2 learners with 54 keystroke features. The relationship with quality was "rather limited": no regression model beat the baseline, and the best classifier reached an AUC of 0.57. https://eric.ed.gov/?id=EJ1353533
- **Implication.** Typing rhythm, pauses and bursts are weak and population-sensitive signals, especially for second-language writers. Inference: they should not drive grades, and they should not be used for authorship verification (see section 7).

### 3.3 Goodhart-style gaming risks

- **Campbell's law** (1979): "The more any quantitative social indicator is used for social decision-making, the more subject it will be to corruption pressures..." [S]. https://www.educationnext.org/education-reforms-obey-campbells-law/
- **Students game tutors.** "Gaming the system" was documented in cognitive-tutor classrooms through help abuse and systematic trial and error (Baker, Corbett, Koedinger & Wagner, CHI 2004) [S]. https://dl.acm.org/doi/10.1145/985692.985741
- **Concrete circumvention of process tools (all [P] unless marked):**
  - *Autotypers.* The Chrome Web Store extension "Undetectable AI – Human Auto Typer" (10,000 users, updated 24 October 2024) lists "Bypassing writing reports and extensions like GPTzero, Originality.ai, and Draftback". Undetectable.ai's December 2023 announcement said it was "tested on bypassing draftback, origin, and originality". The Firefox add-on "HumanTyper" (updated 12 March 2026) lists "Avoiding detection of copy-pasted text". The Firefox add-on "Duey.ai Auto Typer" (updated 13 September 2026) says it "writes it keystroke by keystroke, so your document has a full Version History for review".
  - *Retyping.* Students can retype AI text; TechLearning (April 2026) and Tom Daccord both note this for Grammarly Authorship.
  - *Laundering through intermediate apps or rewriters.* Copying via an intermediate app changes Authorship's recorded source [S]. Grammarly's "Humanize" relabelled pasted AI text as "Typed by a Human" (Plagiarism Today, 6 November 2025).
- **Self-declaration is also gamed.** Corbin et al. report widespread non-completion of AI declarations; one figure quoted (up to 74%) comes from a secondary summary [U].
- **Inference for Dynodoc.** Any metric that is *cheap to fake* (typing time, keystroke naturalness, number of prompts, number of branches) will be faked once it affects grades. Signals that are *expensive to fake* are:
  - the quality of stated reasons for accepting or rejecting AI proposals;
  - consistency between the log and a short oral explanation;
  - the ability to improve on flawed AI output.

---

## 4. Existing products: what they capture and show teachers

| Product | Dates | What it captures | What teachers see | AI assistant and restrictions | Language | Limitations |
|---|---|---|---|---|---|---|
| **Turnitin Clarity** | Announced 4 March 2025 (SXSW EDU); generally available 15 July 2025; paid add-on to Feedback Studio [P] | A composition space inside the LMS: typing, pastes, draft history, AI chat [P] | "Revision timelines, pasted vs. typed text, and a summary of AI chat history" (press release, 15 July 2025) [P]; video playback [P]; similarity, AI-writing indicator and submission metadata in one view [P]; bulleted summary of AI use with a link to the full chat [S] | Optional; the instructor can disable it. Guardrails so it does not write for students [S]. Since June 2026: response complexity (Foundational, Standard, Advanced), permitted uses (brainstorming, outlining, proofreading, structure) and a preview tool (Campus Technology, 1 June 2026) [P] | At GA the assistant was "trained and tested to support interactions in English only" [P]. Since March 2026 the AI chat supports English plus 19 languages; **Bangla is not among them**; grammar and citation checks are English only [P] | Writing must happen inside Clarity. Privacy pushback likely; students can use AI elsewhere and retype (Plagiarism Today, 6 March 2025) [P]. Turnitin itself warns of "information overload" [P] |
| **Grammarly Authorship** | Launched August 2024 [S]; Word support, replay and analytics added 22 April 2025 [P] | Source of each text insertion: typed, pasted from a known or unknown source, generated by AI (Grammarly, ChatGPT, Claude, Gemini, Copilot), modified with Grammarly AI, edited by traditional suggestions [P] | A colour-coded report with percentages; replay [P] | Grammarly's own AI features are tracked; no teacher-configurable assistant | English-centric [U] | Needs clipboard access; can't track private windows or desktop sources in Google Docs; Word history unreliable past 20 pages; "may not capture 100% of cases" [P]. Voice typing mislabelled as pasted (Anna Mills, 14 November 2024) [P]. Humanize laundering; Grammarly said it would change the display (Plagiarism Today, 6 November 2025) [P]. Retyping evades it [P] |
| **GPTZero Writing Report / Origin / "Authorship"** | Ongoing; current page viewed 30 September 2026 [P] | Google Docs revision history via a Chrome extension; its own editor | Writing replay; "Typing Analysis" of "how natural the typing looks"; paste timeline; AI-detection flags; multi-author percentages; an "Investigate" chat grounded in the replay [P/S] | None | "Strongest on English prose" [P] | Vendor disclaimer: "Results should not be used to punish or as the final verdict" [P]. Explicitly targeted by autotypers [P] |
| **Draftback** (James Somers) | Long-standing Chrome extension [S] | Google Docs revision history | Character-level replay [S] | None | Any | Not an AI tool; defeated by autotypers [P] |
| **Revision History** (extension) | Current [P] | Google Docs and Slides history | Replay; edit sessions; time spent; typed vs pasted percentages; "unusual writing patterns" [P] | None | Any | Says it is "a starting point for conversations with students, not ... explicit evidence" [P]. Free up to 150 documents a month; Plus US$32 a year [P] |
| **Brisk "Inspect Writing"** | Current [P] | Google Docs history | Replay; what was added, deleted or pasted; response to feedback [P] | Brisk has student-facing AI, but I found no documentation of teacher visibility of those chats on this page [P] | Any | "Doesn't produce an accuracy score"; framed as a conversation starter [P] |
| **Cursive** (Cursive Technology) | Commercial launch 1 January 2025 [S] | Keystroke events in Moodle (TinyMCE), Canvas, D2L and Blackboard; "typing biometrics" for authorship and identity verification [S] | Process record and replay [S] | None | Any | Behavioural biometrics raise privacy and fairness issues (see 7.4) |
| **PowerNotes** (Composer and Insight) | Composer February 2024 [S]; current page [P] | Research notes, sources, outline, in-platform AI conversations | An "Insight score", described as a "correlation score that reflects genuine engagement"; an activity log with "every source, highlight, annotation, outline entry, and AI conversation", "usable in integrity proceedings" [P] | ChatGPT integrated; faculty can review AI use [S] | English [U] | The score is a proxy and gameable (Inference) |
| **Khan Academy Writing Coach** (Khanmigo) | July 2024 [S] | Guided stages: understanding the assignment, outlining, drafting, feedback [S] | "Detailed insights into their writing process"; teachers can "review their interactions" [S/P] | Designed not to write for students [S] | Free for **US** teachers and students only [P] | Not available in Bangladesh on this basis [P] |
| **MagicSchool** (Student Rooms / MagicStudent) | Current [P] | Student AI tool sessions inside teacher-configured rooms | "Student interactions are stored securely to support teacher visibility"; real-time insights; moderation alerts [P] | The teacher chooses which tools are enabled [S] | English-centric [U] | Framed around safety monitoring; privacy critics compare it to surveillance [S] |
| **Microsoft 365 Copilot Chat / Study and Learn** | Copilot Chat for ages 13+ (2025); Study and Learn blog 13 May 2026 [P/S] | Tenant-level AI chat | I found no teacher view of chats; admins govern access [S] | Study-mode-style agent [P] | Launched in US English, expanding [P] | Off by default for K-12 students aged 13–17 [S] |
| **Google Gemini in Classroom** | 2025–2026 [S] | Assigned Gems and notebooks | Who opened or started a conversation, not its content [P]; admins can use Vault [S] | Guided Learning mode [S] | Many | No teacher-level transcript review [S] |

**"Any tool that grades or reports prompts."** What I found:

- vendor aggregate analytics (Turnitin's prompt-effectiveness categorisation [P]);
- engagement scores (PowerNotes Insight [P]);
- research prototypes that score prompts or judgement (Ognibene et al. [P]; CoReasoningLab [P]).

I did **not** find a mainstream product that scores an individual student's prompting and AI-decision quality as a rubric criterion in a summative grade [U: absence of evidence, not evidence of absence].

Inference on Dynodoc's differentiation: competitors mostly record *where text came from* (typed or pasted) and sometimes the raw chat. Dynodoc's native primitives could record *what the student decided about each AI proposal, on which stable paragraph, with what reason, and in which draft*. Those primitives are a hash-chained event log, stable paragraph IDs, personal drafts, change requests and "Who changed this?" (see docs/GIT-STYLE-WORKFLOW.md). That is closer to what the AIAS Level 3 wording asks to assess ("how AI outputs are evaluated, modified, and integrated").

---

## 5. AI-detector reliability and bias

- **Liang, Yuksekgonul, Mao, Wu & Zou, "GPT detectors are biased against non-native English writers"** (Patterns 4(7):100779, July 2023) [P]:
  - Seven widely used detectors; 91 TOEFL essays by non-native writers; 88 US eighth-grade essays.
  - The average false-positive rate on TOEFL essays was **61.3%**, with near-perfect accuracy on the native essays.
  - All seven detectors flagged 19.8% of TOEFL essays; at least one flagged 97.8%.
  - Enriching vocabulary with ChatGPT cut the false-positive rate from 61.3% to 11.6%, and simple prompting let AI text evade detection.
  - https://pmc.ncbi.nlm.nih.gov/articles/PMC10382961/
- **Turnitin's own statements.**
  - At its April 2023 launch Turnitin claimed "less than 1 percent" false positives. By 1 June 2023 it acknowledged a sentence-level false-positive rate of about 4%, higher false positives below 20% AI (now marked with an asterisk), and that 54% of false-positive sentences sit next to AI text (Inside Higher Ed, 1 June 2023) [P]. https://www.insidehighered.com/news/quick-takes/2023/06/01/turnitins-ai-detector-higher-expected-false-positives
  - Turnitin states a document-level false-positive rate of under 1% for documents with more than 20% AI [S].
  - Turnitin published a 2023 blog claiming "no statistically significant bias against English Language Learners" [S; page blocked automated access].
- **Universities that turned it off:**
  - **Vanderbilt** (16 August 2023) [P]. At a 1% false-positive rate, its roughly 75,000 papers a year imply about 750 false flags. It also cited opacity and bias against non-native speakers.
  - **University of Waterloo** (from September 2025) [P]. Cited unreliability (a "100% AI" flag on human text in internal tests), bias against students whose first language is not English, and cost.
  - **Curtin University** (from 1 January 2026) [P]. Text-matching continues.
  - **Washington State University** cancelled its AI-detection contract (memo, spring 2026) [S].
  - Other reported disablers include UC Berkeley, Pittsburgh, Georgetown, Oregon State, Michigan State and Indiana [S; tracker sites, not individually verified].
- **Implications for ESL and Bangla-medium students.** Inference, grounded in Liang et al. and Nallaya et al.:
  - Students writing in English as a second or third language produce lower-perplexity text, which is exactly what detectors flag.
  - Bangla-language writing is poorly covered by English-trained detectors (GPTZero says it is "strongest on English prose" [P]).
  - Any Dynodoc feature that labels text "AI-like" would disproportionately harm the target users. The same fairness logic applies to typing-fluency metrics in L2 writing (Conijn et al. 2022 found them weak even within an L2 group).

---

## 6. Bangladesh context

### 6.1 National AI policy status

- **Earlier documents [P].** The National Strategy for AI (ICT Division, 2020/2021) names education as a priority sector (curriculum, teacher capacity, AI labs). A "National AI Policy 2024" draft followed (EdTech Hub, October 2025).
- **2026–2030 drafts [S/P].** Draft v1.1 was published about 14 January 2026 and Draft **v2.0 on 9 February 2026**, after public consultation closed on 8 February 2026. The draft includes a risk-based framework, prohibitions on mass surveillance and social scoring, and impact assessments for high-risk systems. https://ictd.gov.bd/pages/notices/national-ai-policy-bangladesh-2026-2030-draft-v11-6966e23b8c8a196928d558b4 (the official site aipolicy.gov.bd had an expired TLS certificate when I tried it).
- **Status now [P].** AIPolicyTracker (verified 26 September 2026) lists it as **"Under consultation"**, with the committee reviewing submissions (https://aipolicytracker.org/policies?jurisdiction=bangladesh). On 18 July 2026 the Posts, Telecommunications and IT Minister, Faqir Mahbub Anam, said Bangladesh "is formulating" a human-centric national AI policy, and mentioned expanding AI-focused education (Dhaka Tribune) [P]. **Conclusion: not adopted as of 30 September 2026.**
- **Universities and schools.** I found **no University Grants Commission or Ministry of Education guideline on generative AI** in assessment [U]. BRAC University's School of General Education uses an "Ethical AI Use Declaration" form requiring students to list AI tools and prompts [S; seen only on Scribd, not verified with BRAC]. EdTech Hub's Bangladesh scan (October 2025) found that teachers learn AI tools informally "without specific guidance from their respective institutions" [P].

### 6.2 Data protection: potentially decisive for school-age use

- **The Ordinance [S].** The Personal Data Protection Ordinance 2025 was approved by the Advisory Council on 9 October 2025 and issued in November 2025. Per the Daily Star's summary (22 October 2025) [P]:
  - a "child" is under 18;
  - processing a child's data needs parent or guardian consent;
  - "profiling, behavioural tracking, or targeted advertising directed at minors" is prohibited;
  - automated decision-making on children's data is restricted;
  - some sections take effect 18 months after gazetting.
  
  https://www.thedailystar.net/tech-startup/news/bangladeshs-personal-data-protection-ordinance-2025-key-takeaways-4015401
- **The Act [S].** Securiti reports that Parliament enacted it as the **Personal Data Protection Act, 2026** (Law 63 of 2026) in April 2026, retaining under-18 parental consent. https://securiti.ai/bangladesh-personal-data-protection-act-overview/ . I have not verified this against the gazette.
- **Inference.** Keystroke-level behavioural analytics or "typing biometrics" on SSC/HSC-age students look legally risky. Automated grading or flagging based on their behaviour data may also be restricted. An *educational record of decisions the student knowingly makes*, with parental consent and no automated decisions, is a safer design, but it needs Bangladeshi legal advice.

### 6.3 Exam system

- **Secondary.** The interim government abandoned the 2021 competency curriculum and reverted to the **National Curriculum 2012** textbooks and streams (science, humanities, business) [S]. SSC 2026 ran on a shortened syllabus, with changed question structures in Bangla, ICT and Finance [S]. HSC 2026 (May–June 2026) was held on the full syllabus with full marks [S]. https://www.thedailystar.net/news/bangladesh/education/news/2026-hsc-equivalent-exams-be-held-full-syllabus-marks-3968916
- **Inference.** SSC and HSC are supervised, handwritten, high-stakes board exams, which is effectively AIAS Level 1 / Lane 1. Dynodoc's natural fit is **coursework, assignments, school-based assessment, university coursework and theses**, not the board exams themselves.

### 6.4 Devices and connectivity

- **BBS ICT Access and Use Survey 2025–26, Q4 (April–June 2026)** [P], covering 61,632 households:
  - 58.2% of households have internet access; 75.1% have smartphone access; 99% have at least one mobile phone.
  - **9.1% of households have a computer.**
  - 59.2% of people aged 5+ used the internet; 11.8% used a computer.
  - https://www.thedailystar.net/news/technology/news/mobile-phone-use-reaches-896-bangladesh-bbs-4278021
- **Other constraints [P].** EdTech Hub (October 2025) reports rural teachers sharing smartphones, "70% of teachers" lacking basic digital skills (per one development partner's internal data, not independently verified), and gender gaps in digital ability.
- **Inference.** The student writing surface must work on a phone over intermittent mobile data. Offline work that is later uploaded must not be treated as suspicious. (Dynodoc's existing upload-as-change-request flow already handles offline edits; in education mode such text should be labelled "written outside Dynodoc".)

### 6.5 Bangla language support

- **Benchmarks.**
  - Frontier models score highly on Bengali Global-MMLU-Lite: Gemini 3.1 Pro Preview and Claude Opus 4.6 at about 92, GPT-5 at about 91 (Artificial Analysis, 2026) [P].
  - A perturbation benchmark (GSM-Plus-BN, arXiv 2607.13248, July 2026) still finds "a notable performance gap ... relative to their English benchmarks" in Bangla mathematical reasoning [P].
  - Informants in the EdTech Hub scan disagreed on Bangla quality; four raised accuracy and contextual concerns [P].
- **Education tools.** Turnitin Clarity's AI chat does not support Bangla [P]; Khanmigo Writing Coach is US-only [P]; Microsoft Study and Learn launched in US English [P].
- **Inference.** A Bangla- and Banglish-capable assistant with Bangla-validated prompt classification would be a genuine gap-filler. Code-mixed prompts (Bangla in Latin script) must be handled and not penalised.

### 6.6 Local ed-tech (all [P], from EdTech Hub, October 2025, unless marked)

- **10 Minute School:** the largest platform, with AI features.
- **Shikho:** "Shikho AI" doubt-solving in Bangla aligned to the national (NCTB) curriculum; "Think AI", a free Bangla AI-literacy course with Meta and LightCastle [S]; early talks with the Department of Secondary and Higher Education on teacher training.
- **Ostad:** AI question generation.
- **a2i:** MuktoPaath (an e-learning platform with more than 2 million learners); an AI-in-education teacher course planned in Bangla and dialects; "Noipunno", an AI assessment-synthesis tool whose development "stalled due to political changes".
- **Student research [S].**
  - Khulna University students, in a qualitative study (PLOS ONE, 2025), described ways to bypass plagiarism detection.
  - A knowledge/attitudes/practices survey of 386 students covered "AI-giarism".

---

## 7. Design implications for Dynodoc

The following are recommendations (Inference) grounded in the sources above.

### 7.1 Positioning

- **Say:** "A transparent record of how you worked with AI, which your teacher can grade for judgement." **Don't say:** "Proves authorship" or "detects cheating". The AIAS authors, Sydney, TEQSA and multiple universities have rejected those claims (sections 2 and 5).
- **Map the exam modes honestly:**

| Dynodoc mode | Closest AIAS level | Lane | Honest claim |
|---|---|---|---|
| **Off** | 1 No AI, *only if supervised* | Lane 1 only in a supervised room | "No AI inside Dynodoc." It cannot stop outside AI at home. |
| **Ask-only** (explain, question, critique; never produce or replace assessed text) | 2 AI Planning, or a constrained 3 | Lane 2 | The assistant can't write the work; the student could still use outside tools. |
| **Unlocked after first draft** (per section or per document) | 3 AI Collaboration | Lane 2 | Evidence-based guardrail: own attempt first (Qin et al.; productive failure). |
| **Open, prompts graded** | 4 Full AI (or 5 if co-designed) | Lane 2 | Grades "critical thinking ... shown in directing AI" (AIAS L4 wording). |

- **Enforce modes server-side.** A mode is a policy attached to the assignment, not a student toggle; unlike study mode, the student cannot switch it off. Record the policy snapshot in the hash chain.

### 7.2 What to log (decision-level, not keystroke-level)

Each item is an append-only event on the existing chain, keyed to stable paragraph IDs:

1. **Assistant session:** assignment ID, active mode, model and version, system-prompt version, language.
2. **Prompt:** full text (Bangla, English or mixed), the target (paragraph IDs or selection), and a student-chosen *intent tag* such as "explain", "critique my draft", "suggest structure", "check facts" or "rewrite". Offer the tags as optional buttons, since self-labelling doubles as metacognitive scaffolding.
3. **AI proposal:** text, type (explanation, suggestion, draft text), and target paragraph IDs.
4. **Student decision** on each proposal:
   - accept, modify or reject;
   - for modify, the resulting diff;
   - an *optional reason* (one line; required only for high-impact decisions in assessed modes, to limit burden).
5. **Span-level provenance** on the document itself: typed in Dynodoc; AI-proposed then accepted; AI-proposed then modified; pasted from inside Dynodoc; pasted or uploaded from outside (unknown origin).
   - Note: Dynodoc currently attributes merged changes to the merger while "the request remembers who wrote them" (docs/GIT-STYLE-WORKFLOW.md). Education mode needs provenance that survives merges and reverts at span level, so the AI origin of a sentence is not lost.
6. **Structure events:** creating, rebasing and discarding personal drafts; "Restore as new changes" (revert) with an optional reason; commit or change-request titles and "What did you change and why?" text.
7. **Checkpoints:** first-draft submission (which unlocks the assistant in mode 3), final submission, reflection statement.
8. **Coarse timestamps** (minute-level). **Do not** log keystroke timing, inter-key intervals, mouse movement or typing rhythm.

### 7.3 What to show teachers

A layered view. Most teachers should need only layer 1.

1. **Process card** (one screen per student):
   - mode and AIAS level;
   - an AI-touch map by paragraph (none, explained-only, suggestion-accepted, suggestion-modified, AI-originated);
   - the distribution of prompt intents, with 2–3 representative prompts auto-selected (for example the first, the most consequential, and a rejected one);
   - decision counts with *examples of reasons*;
   - the drafts and reverts timeline;
   - the student's reflection placed next to what the log shows.
2. **Rubric pre-fill:** for each criterion in section 8, links to the specific log excerpts that are evidence. The teacher chooses the level; the system never auto-grades (consistent with JCQ's "not the sole means of marking" [S] and the Bangladesh restrictions on automated decisions about children [S]).
3. **Drill-down:** full transcript and diffs on demand. Replay is optional and lower priority.
4. **Class view:** aggregate intents and common misconceptions, such as the share of "Is this good?" style prompts (Turnitin found 29% [P]). This supports *teaching* prompting, which workload-wise matters more than individual policing.

### 7.4 What never to infer (state these in the UI and documentation)

- **That pasted or uploaded text is AI-generated or plagiarised.** Offline work, voice typing and accessibility tools produce pastes; Grammarly mislabelled voice typing [P].
- **Authorship or identity from typing behaviour.** Keystroke features are weak predictors of quality (Zhang & Deane; Conijn), easy to fake with autotypers [P], biased against L2 writers, and possibly unlawful to profile for minors in Bangladesh [S].
- **Effort or honesty from time-on-task, number of sessions or edit counts.**
- **Prompting skill from the number, length or English fluency of prompts.** Bangla and code-mixed prompts are equally valid.
- **Learning from the AI share alone.** A high AI share can be appropriate at AIAS 4, and a low one can hide outside AI use.
- **Misconduct from any single automated signal.** Any flag leads to a conversation with the student, never a penalty.
- **Any "AI-likeness" score for text.** Given Liang et al., avoid it entirely.

### 7.5 Preventing gaming

- **Reward judgement, not volume.** No points for more prompts, branches or time (Campbell's law). The rubric should explicitly say volume is not evidence.
- **Sampled micro-vivas.** For summative tasks, randomly select about 10–20% of students (or everyone for high stakes) for a 2–3 minute conversation about 1–2 logged decisions: "Why did you reject this suggestion? Explain this paragraph." This links Lane 2 evidence to a Lane 1 check at low cost (sections 2.2 and 2.7).
- **Flawed-output tasks.** In mode 4, give students AI output with known errors to critique; this is harder to fake than logs (CoReasoningLab approach [P]). If the assistant itself is ever seeded with errors, disclose that to students in advance, to preserve trust.
- **In-class checkpoints.** A supervised first draft (mode 1 in class) followed by a mode 3 take-home revision gives a trustworthy baseline against which the AI-assisted delta is visible.
- **Tamper evidence.** The hash chain proves the record wasn't altered afterwards; it does not prove the student didn't use another device. Say both things plainly.
- **A no-penalty declaration channel** for outside AI use or uploads. Undeclared outside text becomes a conversation topic, not an automatic zero. This reduces the incentive to launder text through autotypers.

### 7.6 Psychological safety

- **No secret dashboards.** Students see exactly what the teacher sees, before submitting.
- **Formative first.** Practice assignments whose logs are not graded, so students learn that asking "naive" questions is safe. The Anthropic and Turnitin data show many students start with vague prompts; the goal is growth.
- **Grade trajectory,** not only the final state, where feasible.
- **Minimal required writing.** Reasons are optional except for a few consequential decisions, so logging doesn't become an extra essay.
- **Right to explain and to appeal;** the student can annotate any log item.
- **Retention limits** and deletion after the grading and appeal window. For under-18s: parental consent, no profiling, and no automated decisions (section 6.2).
- **Watch for chilling effects.** The AIAS paper notes that students using AI legitimately hide it when the discourse is adversarial [P]. Pilot surveys should measure whether logging reduces help-seeking.

### 7.7 Reducing teacher workload

- **Exception-based review:** the process card plus 2–3 auto-selected excerpts per student, and full logs only on demand.
- **Evidence-linked rubric pre-fill:** the system proposes evidence and the teacher chooses levels. Any LLM summarisation of logs must be labelled as a summary with links to the source events.
- **Sampling for orals** instead of universal vivas. Full orals cost about 25–35 hours per 100 students (modelled [P]).
- **Class-level analytics** to address common prompting weaknesses once, in class.
- **Reusable assignment templates** per mode and AIAS level, including Bangla-language templates.

### 7.8 Localisation and infrastructure

- **Mobile-first editor and assistant;** low-bandwidth mode; tolerate offline work.
- **Bangla, English and Banglish** in the UI, the assistant and the intent classifier, with the classifier validated on Bangla prompts before its outputs are shown to teachers.
- **Licensing.** Obtain permission before embedding AIAS or 4D wording or graphics (both CC BY-NC-SA [P]), or write original level descriptors that cite them.

---

## 8. Draft rubric: "AI-collaborative work (process)"

**Purpose.** Assess *how* the student worked with AI. Grade the quality of the finished product separately with the subject rubric; the AIAS 2025 paper and Sydney both say disciplinary learning remains primary.

**Use by AIAS level:**

- **L1 (No AI):** not used. The log only confirms mode compliance.
- **L2 (AI Planning):** criteria A, B and E, applied to planning artefacts.
- **L3 (AI Collaboration):** all five, suggested equal weight.
- **L4 (Full AI):** all five, with A and B weighted higher.
- **L5 (AI Exploration):** co-designed with students; start from A–E.

**Levels:** 1 Beginning, 2 Developing, 3 Proficient, 4 Advanced, plus **NE (not enough evidence)**, which triggers a conversation rather than a low score.

**General evidence rules (apply to all criteria):**

- Judge from **patterns across at least two sessions or several decisions**, never from a single event.
- **Counts, time, prompt length and English fluency are never evidence on their own.**
- Bangla, English and mixed prompts are equally valid.
- For summative use, confirm the level for sampled students with a 2–3 minute oral about specific logged decisions. If the oral contradicts the log, the oral wins and the teacher records why.

### A. Deciding when and how to ask (Delegation and framing)

*Logged evidence:* prompt text and intent tags; the target paragraph; whether a student-written version of that paragraph existed before the first AI proposal targeting it; the mode.

| Level | Descriptor | Evidence rule |
|---|---|---|
| 1 | For assessed parts, mostly asks for finished text; prompts lack task context ("write my conclusion"). | Most AI proposals on assessed paragraphs precede any student-written version, and intents are mostly "rewrite" or "write". |
| 2 | Some own attempts come first; prompts are generic ("Is this good?", "improve this"). | Mixed ordering; prompts rarely state a goal or criterion. |
| 3 | Usually drafts first, then asks targeted questions: goal, context and constraints are stated; asks for explanation or critique more than replacement. | Most assessed paragraphs have a student version before AI input; most prompts name a goal or criterion. |
| 4 | Strategic: consults AI on approach before executing, sets the interaction mode ("don't rewrite, question me"), targets specific weaknesses, and can justify when *not* to use AI. | Examples of approach-consultation and mode-setting prompts (as in Anthropic's observable behaviours), confirmed in the reflection or oral. |

### B. Judging AI output (Discernment)

*Logged evidence:* accept, modify and reject decisions with reasons; fact-checking notes or sources added; whether errors in AI proposals (teacher-seeded or naturally occurring) were caught.

| Level | Descriptor | Evidence rule |
|---|---|---|
| 1 | Accepts proposals wholesale, including incorrect or off-task ones; no verification. | One or more consequential errors accepted unchanged; no reasons recorded. |
| 2 | Some changes, mostly cosmetic; rejections without reasons; some errors get through. | Modifications are surface-level (wording); substantive errors partly caught. |
| 3 | Modifies or rejects with stated reasons; checks key facts and claims; catches most substantive errors. | Reasons recorded on consequential decisions; verification visible (sources added, or a note). |
| 4 | Interrogates the AI's reasoning, spots missing context or bias, verifies against sources, and improves on the AI's suggestion. | Explicit challenges to AI reasoning in prompts or reasons; the final text improves on the proposal. |

*Caution:* accepting a correct minor suggestion unchanged is not poor judgement. Judge only *consequential* decisions.

### C. Integrating and owning the work (Integration and voice)

*Logged evidence:* the per-paragraph AI-touch map; the diff between each AI proposal and the final text; the student's messages explaining changes; the oral.

| Level | Descriptor | Evidence rule |
|---|---|---|
| 1 | Core parts (claim, argument, analysis) are AI-originated with little transformation; the student cannot explain them. | Core paragraphs are marked AI-originated with minimal diff; the oral fails. |
| 2 | AI text lightly edited; voice inconsistent; partial explanations. | Small diffs on AI-originated paragraphs; the oral is partial. |
| 3 | AI contributions are reshaped to fit the student's own argument; the student can explain any part. | Substantial diffs, or AI input limited to explanation; the oral succeeds. |
| 4 | AI contributions are clearly subordinate to the student's thinking: extended, critiqued or recombined; explanations are precise. | The student adds reasoning beyond the proposals; the oral shows depth. |

*Caution:* the AI share is **context, not a score**. Thresholds depend on the level (L4 can legitimately be high).

### D. Iterating and managing the process (Steering)

*Logged evidence:* follow-up prompts that refine a proposal with specific feedback; personal drafts used to compare alternatives; reverts with reasons; change-request and commit messages.

| Level | Descriptor | Evidence rule |
|---|---|---|
| 1 | Single-shot use; no revision after AI input; no or empty messages. | No follow-ups; commit messages absent or boilerplate. |
| 2 | Some follow-ups; revisions mostly cosmetic; messages vague. | Follow-ups repeat the request rather than steer it. |
| 3 | Iterates with specific feedback toward a stated goal; uses drafts or reverts purposefully; messages say what changed and why. | At least one steered sequence that ends in an improvement; meaningful messages. |
| 4 | Deliberate experimentation: compares alternatives in drafts and chooses with a rationale; can change strategy when AI help isn't working. | A draft comparison or revert with an explicit rationale; strategy shifts visible in the log. |

*Caution:* more turns, drafts or reverts are **not** better in themselves.

### E. Transparency and reflection (Diligence)

*Logged evidence:* the reflection statement compared with the log; declarations of outside tools or uploads; compliance with the mode.

| Level | Descriptor | Evidence rule |
|---|---|---|
| 1 | No account, or one that contradicts the log; undeclared outside material. | The reflection conflicts with the log; there are outside uploads with no declaration. |
| 2 | Generic account ("I used AI to help with grammar"). | Consistent with the log but unspecific. |
| 3 | Accurate, specific account of where and how AI shaped the work. | Names sections and decisions that match the log. |
| 4 | Accurate, specific and evaluative: what AI did well or badly, its limits and biases, what they would do differently. | As for level 3, plus evaluative insight. |

*Rule:* **declared** outside AI use or uploads carry no penalty under this criterion. Undeclared outside text leads to a conversation, not an automatic misconduct finding (see section 5 on why automated inference is unsafe).

---

## 9. Sources

| # | Title | Author / publisher | Date | URL | Supports | Access |
|---|---|---|---|---|---|---|
| 1 | Your Brain on ChatGPT (arXiv 2506.08872) | Kosmyna et al., MIT Media Lab | v1 10 Jun 2025; v2 31 Dec 2025 | https://arxiv.org/abs/2506.08872 | Design, findings, preprint status | P |
| 2 | Project overview and FAQ | MIT Media Lab | viewed 30 Sep 2026 | https://www.media.mit.edu/projects/your-brain-on-chatgpt/overview/ | Limitations; not peer-reviewed; "don't say harm" | P |
| 3 | Comment on "Your Brain on ChatGPT" | Stanković et al. | 29 Dec 2025 | https://arxiv.org/abs/2601.00856 | Methodological critique | P |
| 4 | Generative AI without guardrails can harm learning | Bastani et al., PNAS 122(26) | 25 Jun 2025 | https://pmc.ncbi.nlm.nih.gov/articles/PMC12232635/ | +48/+127 practice, −17% exam, logs, self-perception | P |
| 5 | Correction for Bastani et al. | PNAS | 26 Aug 2025 | https://pmc.ncbi.nlm.nih.gov/articles/PMC12403119/ | Affiliation-only correction | P |
| 6 | Impact of GenAI on Critical Thinking | Lee et al., CHI 2025 / Microsoft Research | Apr 2025 | https://www.microsoft.com/en-us/research/publication/the-impact-of-generative-ai-on-critical-thinking-self-reported-reductions-in-cognitive-effort-and-confidence-effects-from-a-survey-of-knowledge-workers/ | 319 workers, confidence effects | P |
| 7 | AI Tools in Society | Gerlich, Societies 15(1):6 | Jan 2025 | https://papers.ssrn.com/sol3/papers.cfm?abstract_id=5082524 | Offloading correlations | S |
| 8 | Correction to Gerlich 2025 | MDPI Societies 15(9):252 | 2025 | https://doi.org/10.3390/soc15090252 | Existence of a correction (content unread) | U |
| 9 | Beware of metacognitive laziness | Fan et al., BJET 56(2) | 2025 | https://eric.ed.gov/?id=EJ1460793 | n = 117; essay gains without knowledge gains | P |
| 10 | AI Meets the Classroom | Lehmann, Cornelius & Sting | 2024 | https://arxiv.org/abs/2409.09047 | Substitute vs complement | P |
| 11 | Human Creativity in the Age of LLMs | Kumar et al., CHI 2025 | 25 Apr 2025 | https://arxiv.org/abs/2410.03703 | Harm to unassisted creativity | P |
| 12 | Timing Matters | Qin et al., CHI 2025 | Feb 2025 | https://arxiv.org/abs/2502.06197 | Delay the LLM until after own ideation | P |
| 13 | AI Assistance Reduces Persistence... | Liu, Christian, Dumbalska, Bakker & Dubey (arXiv 2604.04721) | 2026 | https://ai-project-website.github.io/AI-assistance-reduces-persistence/ | n = 1,222; solutions vs hints | P |
| 14 | Experimental Evidence on the Learning Impact of GenAI | Contractor & Reyes | 9 Jul 2026 | https://arxiv.org/abs/2607.08849 | Augmentation vs automation | P |
| 15 | Coach not crutch | Lira et al. | v4 18 Feb 2026 | https://arxiv.org/abs/2502.02880 | Counter-evidence | P |
| 16 | Effects of Varying LLM Access on Essay Writing | Christenson et al. | 29 May 2026 | https://arxiv.org/abs/2606.00250 | Limited access keeps ownership (pilot) | P |
| 17 | AI tutoring outperforms in-class active learning | Kestin et al., Scientific Reports | 2 Jun 2025 | https://www.nature.com/articles/s41598-025-97652-6 | A designed tutor helps | S |
| 18 | Does ChatGPT enhance student learning? | Deng et al., Computers & Education 227 | 2025 | https://www.sciencedirect.com/science/article/pii/S0360131524002380 | Meta-analysis g ≈ 0.71; caveats | S |
| 19 | ChatGPT in Education: An Effect in Search of a Cause | Weidlich et al., JCAL | 2025 | https://onlinelibrary.wiley.com/doi/10.1111/jcal.70105 | Audit of primary studies | S |
| 20 | Nature journal retracts oft-cited ChatGPT paper | GovTech | 6 May 2026 | https://www.govtech.com/education/nature-retracts-oft-cited-paper-on-positive-impact-of-chatgpt | Wang & Fan retracted 22 Apr 2026 | P |
| 21 | The Impact of ChatGPT on Students' Academic Achievement | Liu, Zuo & Lu, JCAL 41(4) | 2025 | https://onlinelibrary.wiley.com/doi/10.1111/jcal.70096 | g = 0.577 | S |
| 22 | ChatGPT's impact on student learning outcomes: a meta-analysis | HSS Communications | 2026 | https://www.nature.com/articles/s41599-026-07019-z | g = 0.670 | S |
| 23 | Learning outcomes with GenAI in the classroom | Microsoft Research | Oct 2025 | https://www.microsoft.com/en-us/research/publication/learning-outcomes-with-genai-in-the-classroom-a-review-of-empirical-evidence/ | Guardrails, struggle | S |
| 24 | When Problem Solving Followed by Instruction Works | Sinha & Kapur, Review of Educational Research | 2021 | https://journals.sagepub.com/doi/10.3102/00346543211019105 | Productive failure d = 0.36 | S |
| 25 | How university students use Claude | Anthropic | 8 Apr 2025 | https://www.anthropic.com/news/anthropic-education-report-how-university-students-use-claude | 4 patterns; 47% direct; Bloom's of Claude | P |
| 26 | The AI Fluency Index | Anthropic (Swanson, Bent, Ludwig, Dakan, Feller) | 23 Feb 2026 | https://academy.claude.com/tutorials/the-ai-fluency-index | 11 observable behaviours | P |
| 27 | What teachers should know about study mode | EdWeek | Jul 2025 | https://www.edweek.org/technology/what-teachers-should-know-about-chatgpts-new-study-mode-feature/2025/07 | Students can switch it off | P |
| 28 | First impressions of ChatGPT's study mode | Leon Furze | 30 Jul 2025 | https://leonfurze.com/2025/07/30/first-impressions-of-chatgpts-study-mode/ | Limitations | P |
| 29 | Introducing study mode | OpenAI | 29 Jul 2025 | https://openai.com/index/chatgpt-study-mode/ | Launch (blocked; via secondaries) | S |
| 30 | Claude learning mode for all users | Engadget | Aug 2025 | https://www.engadget.com/ai/anthropic-brings-claudes-learning-mode-to-regular-users-and-devs-170018471.html | Learning mode | S |
| 31 | Guided Learning in Gemini | Google | 6 Aug 2025 | https://blog.google/products-and-platforms/products/education/guided-learning/ | Guided Learning | S |
| 32 | Study and Learn: AI built for your student | Microsoft Education | 13 May 2026 | https://www.microsoft.com/en-us/education/blog/2026/05/study-and-learn-ai-built-for-your-student/ | Study and Learn agent | P |
| 33 | Create and assign Gemini Notebook & Gems | Google Classroom Help | viewed 30 Sep 2026 | https://support.google.com/edu/classroom/answer/16534159?hl=en | Teachers see opened/started only | P |
| 34 | The AI Assessment Scale (AIAS) | Perkins, Furze, Roe & MacVaugh, JUTLP 21(6) | Dec 2023 / 2024 | https://arxiv.org/abs/2312.07086 | Original scale | P |
| 35 | The AI Assessment Scale Revisited | Perkins, Roe, Furze et al. | 12 Dec 2024 | https://arxiv.org/abs/2412.09029 | Revised-scale preprint | P |
| 36 | Reimagining the AIAS | Perkins, Roe & Furze, JUTLP 22(7) | 27 Sep 2025 | https://doi.org/10.53761/rrm4y757 | Levels, two-lane mapping, version-control critique | P |
| 37 | AI Assessment Scale website (v2.1) | Learning Innovation Practice Ltd | viewed 30 Sep 2026 | https://aiassessmentscale.com/ | Exact level wording; CC BY-NC-SA | P |
| 38 | Updating the AI Assessment Scale | Leon Furze | 28 Aug 2024 | https://leonfurze.com/2024/08/28/updating-the-ai-assessment-scale/ | Revision announcement | P |
| 39 | Two-lane FAQ | Bridgeman & Liu, Univ. of Sydney | 2 Jul 2024 | https://educational-innovation.sydney.edu.au/teaching@sydney/frequently-asked-questions-about-the-two-lane-approach-to-assessment-in-the-age-of-ai/ | Lane definitions | P |
| 40 | University of Sydney AI assessment policy | Univ. of Sydney | 27 Nov 2024 | https://www.sydney.edu.au/news-opinion/news/2024/11/27/university-of-sydney-ai-assessment-policy.html | Default AI allowed from S1 2025 | P |
| 41 | Assessment reform for the age of AI | TEQSA (Lodge et al.) | Nov 2023 | https://www.teqsa.gov.au/sites/default/files/2023-09/assessment-reform-age-artificial-intelligence-discussion-paper.pdf | Principles and propositions | S |
| 42 | Enacting assessment reform in a time of AI | TEQSA | 24 Sep 2025 | https://www.teqsa.gov.au/guides-resources/resources/corporate-publications/enacting-assessment-reform-time-artificial-intelligence | Three pathways | S |
| 43 | AI Use in Assessments | JCQ | 30 Apr 2025 | https://www.jcq.org.uk/knowledge-hub/ai-use-in-assessments-your-role-in-protecting-the-integrity-of-qualifications/ | Rules, screenshots, supervision | P |
| 44 | UNESCO: governments must quickly regulate GenAI in schools | UNESCO | 7 Sep 2023 | https://www.unesco.org/en/articles/unesco-governments-must-quickly-regulate-generative-ai-schools | Guidance; age 13 | P |
| 45 | AI competency framework for students | UNESCO | Sep 2024 | https://www.unesco.org/en/articles/ai-competency-framework-students | Four dimensions, three levels | S |
| 46 | AI Literacy Framework (AILit) | EC and OECD | draft May 2025; final Jun 2026 | https://ailiteracyframework.org/ | Four domains | P/S |
| 47 | PISA 2029 Media and AI Literacy | OECD | draft Jan 2026 | https://www.oecd.org/en/about/projects/pisa-2029-media-and-artificial-intelligence-literacy.html | MAIL | S |
| 48 | Role of authentic assessment to preserve academic integrity | Sotiriadou et al., Studies in Higher Education 45(11) | 2020 | https://www.tandfonline.com/doi/full/10.1080/03075079.2019.1582015 | IOA benefits | S |
| 49 | Validity, reliability ... of oral assessments | Nallaya et al., IIER 34(2) | 2024 | http://www.iier.org.au/iier34/nallaya.pdf | Design dependence; EAL equity | P |
| 50 | Designing AI-resilient assessment | Nikolić & Basta Nikolić, Frontiers in AI | 16 Jul 2026 | https://www.frontiersin.org/journals/artificial-intelligence/articles/10.3389/frai.2026.1841682/full | Modelled oral workload | P |
| 51 | Talk is cheap | Corbin, Dawson & Liu, Assessment & Evaluation in Higher Education 50(7) | 2025 | https://doi.org/10.1080/02602938.2025.2503964 | Structural vs discursive change | S |
| 52 | Developing evaluative judgement for a time of GenAI | Bearman et al., Assessment & Evaluation in Higher Education 49(6) | 2024 | https://www.tandfonline.com/doi/full/10.1080/02602938.2024.2335321 | Evaluative judgement | S |
| 53 | Reimagining writing assessment for the AI era | Abdellatif et al., Frontiers in Psychology | 26 May 2026 | https://pmc.ncbi.nlm.nih.gov/articles/PMC13246662/ | Process logs, anti-surveillance | P |
| 54 | AI Fluency Framework | Dakan & Feller | 2025 | https://aifluencyframework.org/ | 4Ds; CC BY-NC-SA | P |
| 55 | Framing, Judging, Steering | Apartsin & Aperstein | 4 Jun 2026 | https://arxiv.org/abs/2606.05983 | Competency model, flawed outputs | P |
| 56 | AI literacy and prompt engineering strategies | Knoth et al., Computers & Education: AI | 2024 | https://www.sciencedirect.com/science/article/pii/S2666920X24000262 | Prompting skill | S |
| 57 | Use Me Wisely | Ognibene et al. | 4 Mar 2025 | https://arxiv.org/abs/2503.02532 | LLM scoring of prompts | P |
| 58 | MAILS | Carolus et al. | 2023 | https://arxiv.org/abs/2302.09319 | Self-report scale | S |
| 59 | Human-AI collaboration patterns in AI-assisted academic writing | Nguyen et al., Studies in Higher Education 49(5) | 2024 | https://www.tandfonline.com/doi/full/10.1080/03075079.2024.2323593 | Process mining of logs | P |
| 60 | Beyond SRL processes: hidden tactics | Yang et al. | 14 Aug 2025 | https://arxiv.org/abs/2508.10310 | HMM of trace data | P |
| 61 | RelianceScope | Jin et al. | Feb/Apr 2026 | https://arxiv.org/abs/2602.16251 | Reliance coding | P |
| 62 | From GenAI virtual patient dialogue logs to teacher-interpretable evidence | Li et al. | 29 Jul 2026 | https://arxiv.org/abs/2608.28619 | Teacher-interpretable log evidence | P |
| 63 | Keystroke Logging in Writing Research (Inputlog) | Leijten & Van Waes, Written Communication 30 | 2013 | https://eric.ed.gov/?id=EJ1014302 | Tool | S |
| 64 | Process features in writing | Zhang & Deane, ETS RR-15-27 | Dec 2015 | https://eric.ed.gov/?id=EJ1109306 | Limited variance; marginal increment | P |
| 65 | Exploring the feasibility of writing-process features | Deane & Zhang, ETS RR-15-26 | 2015 | https://eric.ed.gov/?id=EJ1109303 | Moderate prediction | P |
| 66 | Early prediction of writing quality using keystroke logging | Conijn et al., IJAIED 32(4) | Dec 2022 | https://eric.ed.gov/?id=EJ1353533 | AUC 0.57 | P |
| 67 | Off-task behavior in the cognitive tutor classroom | Baker et al., CHI 2004 | 25 Apr 2004 | https://dl.acm.org/doi/10.1145/985692.985741 | Gaming the system | S |
| 68 | Education reforms should obey Campbell's Law | Education Next | n.d. | https://www.educationnext.org/education-reforms-obey-campbells-law/ | Campbell's law | S |
| 69 | Turnitin announces Clarity | Plagiarism Today (J. Bailey) | 6 Mar 2025 | https://www.plagiarismtoday.com/2025/03/06/turnitin-announces-clarity-aim-to-make-student-writing-more-transparent/ | Features, critique | P |
| 70 | Turnitin Clarity: bringing transparency | Turnitin UK blog | 4 Mar 2025 | https://www.turnitin.co.uk/blog/turnitin-clarity-bringing-transparency-to-the-student-writing-process | Educator view | P |
| 71 | Turnitin Delivers Turnitin Clarity | Turnitin via PR Newswire | 15 Jul 2025 | https://www.prnewswire.com/news-releases/turnitin-delivers-turnitin-clarity-to-bring-transparency-and-responsible-ai-to-the-writing-process-supporting-academic-integrity-in-education-302504889.html | GA; English-only at launch | P |
| 72 | Clarity AI Assistant now supports 20 languages | Turnitin Educator Network | ~Mar/Apr 2026 | https://turnitin.forumbee.com/t/x2ygm51/turnitin-clarity-ai-assistant-now-officially-supports-20-languages | Language list; no Bangla | P |
| 73 | Turnitin data shows transparency about AI use benefits... | Turnitin via PR Newswire | 24 Feb 2026 | https://www.prnewswire.com/news-releases/turnitin-data-shows-transparency-about-ai-use-benefits-students-and-educators-302695254.html | 29% / 36% / 94% | P |
| 74 | Turnitin adds customizable AI assistance | Campus Technology | 1 Jun 2026 | https://campustechnology.com/articles/2026/06/01/turnitin-adds-customizable-ai-assistance-to-support-different-assignments-grade-levels.aspx | Complexity and use-case settings | P |
| 75 | About Grammarly Authorship | Grammarly Support | viewed 30 Sep 2026 | https://support.grammarly.com/hc/en-us/articles/29548735595405-About-Grammarly-Authorship | Categories, limits | P |
| 76 | Grammarly Authorship just got a major update | Grammarly | 22 Apr 2025 | https://www.grammarly.com/blog/academic-writing/grammarly-authorship-just-got-a-major-update/ | Word, replay, analytics | P |
| 77 | How Grammarly launders AI-generated content | Plagiarism Today | 6 Nov 2025 | https://www.plagiarismtoday.com/2025/11/06/how-grammarly-launders-ai-generated-content/ | Humanize → "Typed by a Human" | P |
| 78 | Trying out Grammarly Authorship Beta | Anna Mills | 14 Nov 2024 | https://annamills.substack.com/p/trying-out-grammarly-authorship-beta | Voice typing mislabelled | P |
| 79 | Grammarly Authorship: I tested... | TechLearning (E. Ofgang) | updated 14 Apr 2026 | https://www.techlearning.com/news/grammarly-authorship-i-tested-the-new-ai-and-plagiarism-tool | Retyping loophole | P |
| 80 | GPTZero Authorship | GPTZero | viewed 30 Sep 2026 | https://gptzero.me/authorship | Replay, typing analysis, disclaimer | P |
| 81 | Revision History | revisionhistory.com | viewed 30 Sep 2026 | https://www.revisionhistory.com/ | Features, disclaimer, price | P |
| 82 | Inspect Writing | Brisk Teaching | viewed 30 Sep 2026 | https://www.briskteaching.com/inspect-writing | Replay, no accuracy score | P |
| 83 | Cursive (Moodle plugin) | Cursive Technology | 2025 | https://moodle.org/plugins/tiny_cursive | Typing biometrics | S |
| 84 | PowerNotes AI | PowerNotes | viewed 30 Sep 2026 | https://www.powernotes.com/powernotes-ai | Insight score; AI activity log | P |
| 85 | Khan Academy Writing Coach | Khan Academy | viewed 30 Sep 2026 | https://www.khanmigo.ai/writingcoach | US-only; insights | P |
| 86 | MagicSchool for students | MagicSchool | viewed 30 Sep 2026 | https://www.magicschool.ai/magicstudent | Teacher visibility of interactions | P |
| 87 | Undetectable AI – Human Auto Typer | Chrome Web Store | updated 24 Oct 2024 | https://chromewebstore.google.com/detail/undetectable-ai-human-aut/jldjlnajbbkbmebhgfpcgghpbfpfakni?hl=en | Explicit bypass claim | P |
| 88 | New auto typer tool to simulate realistic typing | MarTech Series (Undetectable.ai) | 11 Dec 2023 | https://martechseries.com/predictive-ai/ai-platforms-machine-learning/new-auto-typer-tool-to-simulate-realistic-typing-in-google-docs-released/ | "Tested on bypassing draftback, origin..." | P |
| 89 | HumanTyper: Auto Typing | Firefox Add-ons | updated 12 Mar 2026 | https://addons.mozilla.org/en-US/firefox/addon/humantyper-auto-typing/ | "Avoiding detection of copy-pasted text" | P |
| 90 | Duey.ai Auto Typer | Firefox Add-ons | updated 13 Sep 2026 | https://addons.mozilla.org/en-US/firefox/addon/duey-ai-humanizer-auto-typer/ | "Full Version History for review" | P |
| 91 | GPT detectors are biased against non-native English writers | Liang et al., Patterns 4(7) | Jul 2023 | https://pmc.ncbi.nlm.nih.gov/articles/PMC10382961/ | 7 detectors; 61.3% FPR | P |
| 92 | Turnitin's AI detector: higher-than-expected false positives | Inside Higher Ed | 1 Jun 2023 | https://www.insidehighered.com/news/quick-takes/2023/06/01/turnitins-ai-detector-higher-expected-false-positives | ~4% sentence-level FPR; asterisk | P |
| 93 | Why we're disabling Turnitin's AI detector | Vanderbilt University | 16 Aug 2023 | https://www.vanderbilt.edu/brightspace/2023/08/16/guidance-on-ai-detection-and-why-were-disabling-turnitins-ai-detector/ | 750 false flags estimate; bias | P |
| 94 | Discontinuing AI detection in Turnitin | University of Waterloo | Sep 2025 | https://uwaterloo.ca/associate-vice-president-academic/discontinuing-use-ai-detection-functionality-turnitin | Reasons | P |
| 95 | Update on Turnitin AI-detection tool | Curtin University | effective 1 Jan 2026 | https://www.curtin.edu.au/news/oasis-news/update-on-turnitin-ai-detection-tool/ | Disabled | P |
| 96 | Cancellation of Turnitin AI Detection | Washington State University Provost | Feb 2026 | https://provost.wsu.edu/documents/2026/02/cancellation-of-turnitin-ai-detection-software_memo-to-instructors_provost-office_spring-2026.pdf/ | Cancelled | S |
| 97 | AI in Education Across Bangladesh (Helpdesk 217) | EdTech Hub (Honda, Ahmed & Ramesh Vasudevan) | Oct 2025 | https://docs.edtechhub.org/lib/G27DMNQM | Policy, actors, barriers | P |
| 98 | Bangladesh AI policy record | AIPolicyTracker | verified 26 Sep 2026 | https://aipolicytracker.org/policies?jurisdiction=bangladesh | Still "under consultation" | P |
| 99 | ICT minister: Bangladesh drafting human-centric national AI policy | Dhaka Tribune | 18 Jul 2026 | https://www.dhakatribune.com/bangladesh/415452/ict-minister-bangladesh-drafting-human-centric | Not yet adopted | P |
| 100 | National AI Policy Bangladesh 2026–2030 (Draft v1.1) | ICT Division | Jan 2026 | https://ictd.gov.bd/pages/notices/national-ai-policy-bangladesh-2026-2030-draft-v11-6966e23b8c8a196928d558b4 | Draft | S |
| 101 | PDPO 2025: key takeaways | The Daily Star | 22 Oct 2025 | https://www.thedailystar.net/tech-startup/news/bangladeshs-personal-data-protection-ordinance-2025-key-takeaways-4015401 | Minors: consent; no profiling or tracking | P |
| 102 | Bangladesh Personal Data Protection Act 2026 overview | Securiti | 2026 | https://securiti.ai/bangladesh-personal-data-protection-act-overview/ | Enacted April 2026 (unverified against gazette) | S |
| 103 | Mobile phone use reaches 89.6% (BBS) | The Daily Star | 2026 (Q4 FY26 data) | https://www.thedailystar.net/news/technology/news/mobile-phone-use-reaches-896-bangladesh-bbs-4278021 | Device and internet figures | P |
| 104 | 2026 HSC to be held on full syllabus | The Daily Star | 2025 | https://www.thedailystar.net/news/bangladesh/education/news/2026-hsc-equivalent-exams-be-held-full-syllabus-marks-3968916 | HSC 2026 | S |
| 105 | Bangla language AI model benchmark | Artificial Analysis | 2026 | https://artificialanalysis.ai/models/multilingual/bengali | Bengali Global-MMLU-Lite scores | P |
| 106 | GSM-Plus-BN | arXiv 2607.13248 | Jul 2026 | https://arxiv.org/abs/2607.13248 | Bangla reasoning gap | P |
| 107 | How university students in Bangladesh engage with ChatGPT | PLOS ONE | 2025 | https://journals.plos.org/plosone/article?id=10.1371%2Fjournal.pone.0333089 | Bypassing plagiarism checks | S |
| 108 | Hand in Hand: schools' embrace of AI and risks to students | Center for Democracy & Technology | Oct 2025 | https://cdt.org/insights/hand-in-hand-schools-embrace-of-ai-connected-to-increased-risks-to-students/ | Student-monitoring risks | S |

---

## 10. Claims from the owner's pasted chat

The first six rows are the claims named in the brief. The last four are closely related claims that circulate widely; I add them because they are likely to come up.

| Claim | Verdict | What the sources say |
|---|---|---|
| "Clarity's AI assistant has only been trained and tested for English." | **Outdated (true at launch, not now)** | The 15 July 2025 launch release said the assistant "has been trained and tested to support interactions in English only" [P]. Since about March 2026 its AI chat supports English plus 19 languages; Bangla is not among them; grammar and citation checks remain English-only [P]. Correct phrasing: "Clarity does not support Bangla." |
| "29% of student prompts across two months of Clarity use were requests for review or feedback." | **Verified (vendor data)** | Turnitin release, 24 February 2026: "Nearly a third (29%) of prompts reviewed across two months of Clarity use by students were asking for review, judgment, or other general feedback." Only 36% of those were "effective" [P]. Not peer-reviewed. |
| "An instructor edited a ChatGPT response and it was labelled AI generated" (Grammarly Authorship). | **Not verified** | I could not find this account. Grammarly's documentation does not say whether AI-pasted text keeps its label after manual edits [P]. Anna Mills's instructor test (14 November 2024) reported a *different* mislabel: voice typing shown as pasted [P]. Plausible but unconfirmed; ask the owner for the source. |
| Plagiarism Today reported Grammarly's "Humanize" feature affected Authorship reports. | **Verified** | 6 November 2025: after Humanize, pasted Gemini text showed as "75% Typed by a Human" in the headline, with fine print showing 0% typed and 75% "Rephrased with Grammarly's AI". Grammarly's parent (Superhuman) said it would change the display [P]. |
| Browser autotyper extensions (one Firefox, one Chrome) are built to evade version-history review tools by simulating human typing. | **Verified in substance** | Chrome: "Undetectable AI – Human Auto Typer" states "Bypassing writing reports and extensions like GPTzero, Originality.ai, and Draftback" [P]. Firefox: "HumanTyper" lists "Avoiding detection of copy-pasted text" [P], and "Duey.ai Auto Typer" markets "full Version History for review" [P]. I cannot tell which two extensions the chat meant. |
| Liang et al.: "seven detectors" and a "61.3% average false-positive rate on TOEFL essays". | **Verified** | Patterns 4(7), July 2023: seven detectors, 91 TOEFL essays, average false-positive rate 61.3% [P]. |
| (Related) "An MIT study proved ChatGPT damages the brain." | **Wrong / overstated** | Unreviewed preprint, n = 54, with a published critique. The authors ask people not to use "harm" or "damage" [P]. |
| (Related) "Students who used ChatGPT did 17% worse." | **Verified with context** | True for the GPT Base arm on the later unaided exam in one Turkish high-school maths RCT. The guardrailed GPT Tutor showed no significant loss [P]. |
| (Related) "Meta-analyses show ChatGPT has a large positive effect on learning (g ≈ 0.87)." | **Wrong to cite** | That meta-analysis (Wang & Fan 2025) was retracted on 22 April 2026 [P]. Other meta-analyses exist, but their primary studies are heavily criticised [S]. |
| (Related) "Turnitin's AI detector has under 1% false positives." | **Misleading** | This holds only at document level for documents with more than 20% AI, per Turnitin. The sentence-level rate is about 4%, and scores under 20% carry an asterisk [P/S]. It is independent of the bias finding for non-native writers [P]. |

---

## 11. Open questions

1. **Validity.** Do rubric levels assigned from Dynodoc logs (section 8) agree between teachers (inter-rater reliability) and predict learning, such as delayed unaided performance or oral explanation? This needs a pilot with a research partner, ideally with Bangladeshi universities and one school group. EdTech Hub, a2i and UNESCO Dhaka are active in this space [P].
2. **Chilling effect.** Does visible logging reduce help-seeking or honest use among Bangladeshi students? Measure it in the pilot.
3. **Legal.** Is decision-level logging of under-18s' AI use "profiling" or "behavioural tracking" under the Bangladesh data-protection law? What consent, retention and "no automated decision" rules apply? Get local counsel to confirm the Act text.
4. **Licensing.** Will the AIAS owners (Learning Innovation Practice Ltd) and Dakan & Feller grant commercial permission to embed their wording, or should Dynodoc write its own descriptors?
5. **Bangla intent classification.** Can prompt-intent tagging (explain, critique, rewrite...) be reliable for Bangla and Banglish prompts? What error rate is acceptable before teachers see it?
6. **Mode enforcement.** How strict should "ask-only" be? For example, should the assistant refuse to output sentences longer than N words for assessed paragraphs? How should edge cases like translation help for L2 writers be handled?
7. **Private drafts.** Should personal drafts be visible to teachers only at submission, or never? The trade-off is evidence against safety to experiment.
8. **Model and cost.** Which models give acceptable Bangla quality at a price schools can bear, with data kept appropriately (the draft AI policy and the data-protection law may constrain cross-border transfer)?
9. **Unverified items to chase:**
   - the Grammarly "instructor edited ChatGPT" anecdote;
   - the exact TEQSA 2023 wording (PDF timed out);
   - the content of the Gerlich correction;
   - whether JCQ issued a post-April 2025 revision;
   - BRAC University's official AI policy;
   - whether any Bangladeshi university or the UGC has issued AI assessment guidance since October 2025.
