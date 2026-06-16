# UI/UX Theory for an Agentic Research IDE

*A design philosophy for the questionnaire authoring environment, the collaborative review surface, and the agent integration model. Grounded in references from REDCap, Notion-class block editors, Google Docs, Figma branching, NN/G human factors work, and outliner theory. Aimed at field researchers, not developers.*

---

## 1. The problem, stated precisely

The product needs to do four things that are in tension:

1. Feel familiar to researchers who have spent a decade in Word and Google Docs.
2. Carry enough structured semantic information about each item that AI agents, validators, and exporters can act on it without ambiguity.
3. Let many people review and edit the same instrument over weeks or months without losing track of who changed what or why.
4. Stay quiet — a research instrument is a tool of thought, and the editor is a glass not a stage. Anything that interrupts the rhythm of reading a question, weighing it, and adjusting it is friction.

The Miller Lab post captures the core symptom of the current state. REDCap users hand-write `<div style="margin-left: 40px; background-color: lightyellow">` into field labels to express things like "this is a child of the field above," "this is a notification," "this is a calculated field." They are not doing this to be fancy. They are doing it because the tool gives them blocks but no *semantic* vocabulary for those blocks. Researchers want to encode the *role* of each item, and the tool only models the *type*. They've been hacking visual structure as a substitute for missing structural primitives.

This is the design opening. The right product gives researchers a vocabulary of semantic roles (parent, child, descriptive, calculated, transition, prompt, instrument-block) as first-class objects, and renders them with sensible visual defaults so the researcher doesn't have to write inline CSS to make the form readable.

So the first foundational claim is: **researchers do not actually want a Word document. They want what they have been trying to make Word into.**

---

## 2. Why three obvious paradigms fail

It's worth being explicit about why the first three things one might reach for are wrong.

### 2.1 The Word / Google Docs paradigm

Familiarity is its only real strength, and it's a strong one. But:

- The document has no item-level identity. "Question 7" is a position, not an entity. If a reviewer adds a question above it, "Question 7" is now Question 8, and any external reference (the Stata `do` file, the data dictionary) silently breaks.
- Comments attach to text spans, not to questions. Resolving a comment has no semantic effect on the question itself.
- AI agents have to re-parse the whole document on every interaction. There is no place to attach "this question is the third item in the validated PHQ-9 module."
- Track changes / Suggesting mode work, but conflicts are character-level and reviewers have to mentally diff prose to understand what changed.

Word and Docs survive in the research workflow not because they are good at this job, but because they are the lowest common denominator for collaborators who don't have shared tooling. They are the *format of last resort*, not the *workflow of first choice*.

### 2.2 The Notion / Coda / BlockNote paradigm

Block-based rich text editors solve the identity problem (every block has an ID, type, and properties) and unlock real collaboration via CRDTs like Yjs. But they fail on a different axis:

- Visual chrome per block — borders, drag handles, plus buttons, slash menus. The HN community noted this exact issue with the Notion paradigm: it's far too easy to accidentally add text to a block by pressing anything on the keyboard, or move things around with an accidental drag of the mouse. In a 100-item questionnaire, the cumulative chrome is exhausting.
- Pressing Enter creating a new block interrupts flow. Reviewers from the Word world experience this as "the cursor keeps doing the wrong thing."
- The block menu is generic. It exposes every possible block type, not the small set relevant to a questionnaire item. Researchers have to learn an unfamiliar grammar of "callouts," "toggles," "synced blocks" that doesn't map to how they think about questions.

Block editors are the right *backend*, the wrong *frontend*. The lesson: the data model should be block-based; the visual treatment should not advertise that fact.

### 2.3 The traditional form-builder paradigm (REDCap, Qualtrics, SurveyMonkey, Kobo Form Builder)

These are properly structured but:

- They show one item at a time, or one item per row in a table, with a thick boundary around each. The questionnaire as a whole is hard to read in flow. Reviewers can't see how three questions in a row sound when read consecutively, which is exactly what they need to evaluate ordering, redundancy, and rhythm.
- The designer mode is visibly *different* from the rendered mode. Researchers have to mentally translate "the designer view" into "what the respondent will actually see." The Miller Lab post documents this exact pain: REDCap fields appear as text without any indentation, on a gray background — visually flat — even though the logical structure is hierarchical.
- Heavy reliance on properties panels and modal dialogs. Every choice (response options, validation, branching logic) requires opening a sub-dialog. The interface is *deep* rather than *wide*.

Form builders treat the questionnaire as a *configuration problem*. Researchers experience it as a *writing problem* with structural constraints. The right tool inverts this priority.

---

## 3. The core theory: semantic blocks under a prose surface

The synthesis is straightforward to state and harder to execute.

> **Every item is a structured block in the data model. The visual treatment is by default prose-shaped. Structure surfaces on demand, not by default.**

Three implications fall out of this immediately.

**Implication 1 — Blocks are computational, not visual.** Every question has a stable ID, a type (`select_one`, `integer`, `geopoint`, etc.), a role (`parent_question`, `follow_up`, `descriptive`, `calculated`, `instrument_module_item`), a history, a comment thread, and a set of metadata. None of this needs to be visible in the default view. The block exists for the database, the AI agent, the diff engine, and the exporter. It does not need to declare itself to the human reading the document.

**Implication 2 — The default view reads like a methodology section.** A 60-item questionnaire renders as numbered items in a single column, with headings for sections, indentation for follow-ups, and italics for instructions. No card backgrounds. No drag handles. No plus buttons in the margin. Just the text, the numbers, and the structure. A researcher should be able to read three pages of this without their eye catching on a single piece of UI chrome.

**Implication 3 — Affordances appear contextually.** Hover a question and a thin gutter on the left shows a comment count and a small `⋮` menu. Click a question and a side inspector reveals its structured properties (type, validation, choices, relevance condition). Right-click for the agent menu. Press `Cmd+/` for the slash menu, scoped to questionnaire-relevant operations only. The UI is *quiet by default and rich on demand*.

This is the crucial reframing of the "blocky" complaint in the brief. The complaint is correct: rendering every item as a visible block is too noisy. The mistake is to conclude that blocks are the wrong abstraction. The right conclusion is that blocks should not announce themselves visually until the user wants them to.

The Smashing Magazine piece on `document.designMode` captures the spirit: the tool that makes web pages editable is the same tool that makes them readable. Edit and read are different *modes* over the same content, not different *tools*.

---

## 4. The researcher's mental model

A useful way to design this UI is to enumerate what researchers actually think about while authoring an instrument. There are roughly five layers, each of which the UI should respect:

| Layer | What the researcher is thinking | Where it surfaces in the UI |
|---|---|---|
| Construct | "I want to measure household food insecurity" | Module/section header; instrument-library reference |
| Item | "What's the right wording for the third item in HFIAS?" | The question text and label |
| Response | "Should this be a 4-point or 5-point scale?" | Inspector showing choice list |
| Logic | "If they say 'never,' skip the next two follow-ups" | Inline relevance pill on the follow-up items |
| Data | "How does this become a Stata variable?" | Codebook view; spec mode |

Most of the time researchers are operating in the top three layers and want the bottom two to be invisible. Logic and data become important at specific moments — usually when the AI surfaces an issue, or when the form is being prepared for export. The default view should privilege the top three layers; the bottom two should be one click or one keypress away.

This is also the reason to support multiple representations of the same canonical content (Section 6 develops this).

---

## 5. Eight design principles

Distilled from the references, the brief, and the analysis above:

**P1 — Semantic structure is first-class, but invisible by default.** Every item carries its role, type, and history; the visual treatment surfaces these only when relevant. The tool should never require a researcher to write HTML or CSS to express something that is structurally meaningful (the REDCap antipattern).

**P2 — The default view reads like a printed instrument.** Single column, generous typography, numbered items, indentation for hierarchy, no per-item chrome. If you can't print it and hand it to a methodologist for review, the design is wrong.

**P3 — Modes are explicit and discoverable.** Borrowing from NN/G's analysis of modes in user interfaces, the system should have a small number of clearly distinguishable states (Reading, Writing, Reviewing, Inspecting), with strong visual signals such as different backgrounds, emphasis on active-mode indicators, or changing the cursor for which mode is active. Mode slips are the most common source of editor frustration; the cure is bold visual differentiation. Google Docs' three-mode pencil dropdown (Editing / Suggesting / Viewing) is a good reference.

**P4 — Single source of truth, multiple isomorphic views.** The canonical form is the structured AST. Document, outline, spec, and preview are all *renderings* of it. Edits in any view are equally valid; the system roundtrips losslessly between them. This eliminates the form-builder problem of "designer view diverging from respondent view."

**P5 — Progressive disclosure of complexity.** Following NN/G's cognitive load principles, the editor should show only what's relevant to the current task and introduce additional fields as users progress. The Inspector panel should default to the three most-used properties and have an "Advanced" toggle for the rest. The slash menu should default to the eight most common operations. The agent panel should default to the two most common requests ("review for clarity," "convert this to ODK").

**P6 — Comments and suggestions are first-class, not bolted on.** Every research instrument is a negotiated artifact. The UI should treat reviewer feedback as a primary content type, with a dedicated review mode (P3) and a clear acceptance flow. The Google Docs suggesting model — propose, review, accept or reject, with a side margin trail — is the right reference, but adapted to operate at the *item level* rather than the *character level*.

**P7 — AI agents are participants, not panels.** The agent shouldn't live in a separate sidebar that researchers have to switch attention to. It should be addressable from any item (right-click → "Ask agent..."), from any comment (`@agent fix this`), and from a global command bar. Its outputs should appear *in line* as suggestions (visually identical to a human reviewer's suggestions), not as walls of chat text.

**P8 — Exit is one click.** At every moment, the researcher should be able to download a clean Word document, a CSV, a printable PDF, or an XLSForm without any negotiation with the tool. Lock-in is the death of trust in research tools. This is also a competitive moat: a tool that is easy to leave is a tool that is easy to commit to.

---

## 6. The four-mode editor

A practical synthesis: the editor exposes one canonical content with four primary modes. Each mode is optimized for a different task; switching is fast (keyboard shortcut, toggle in toolbar); the mode indicator is visible at all times.

### 6.1 Document mode (default)

- Single column. Generous line height. Numbered items in their natural reading order.
- Sections render as second-level headings. Modules from validated instruments (PHQ-9, GAD-7, WHODAS) render with a subtle sidebar in the gutter to indicate provenance.
- Follow-up questions render indented under their parent.
- Calculated fields render in a slightly muted color with a small calculator icon.
- Hover any item: thin gutter on the left appears with comment count and `⋮` menu.
- Click any item: thin highlight on the left edge of the row; Inspector panel slides in from the right.
- Reads like a research protocol appendix. This is the mode for writing, reading, and reviewing the *content*.

### 6.2 Outline mode

- Two-pane, intrinsic-tree style (drawing on the outliner literature: a hybrid between one-pane and two-pane outliners).
- Left pane: collapsible tree of sections → modules → items. Each item shows its label and type icon only.
- Right pane: the selected item's content, full Inspector visible.
- Use case: navigating a 200-item form, reorganizing sections, doing high-level structural review.

### 6.3 Spec mode

- The underlying XLSForm rendered in a familiar tabular view.
- Each row is editable. Changes round-trip back to Document mode immediately.
- This is the mode for power users who already know XLSForm syntax, and for the rare case where a researcher wants to copy-paste from a spreadsheet they've been keeping.
- Crucially: spec mode is *always available*, never the default. The presence of this mode reassures technical users that nothing is hidden; the absence by default reassures non-technical users that they don't have to learn it.

### 6.4 Preview mode

- The compiled XForm rendered via Enketo (or an equivalent), exactly as a respondent on a tablet would see it.
- Click-through behavior, including skip logic, validation, and constraints, all live.
- Use case: walking a colleague through what the form will feel like; sanity-checking branching logic; sharing with a stakeholder for non-editing review.

The mode toggle is a segmented control with four labeled options, always visible in the top toolbar. NN/G's research on modes is unambiguous on the requirement: clear visibility of the current active mode, with strong visual differentiation. The mode indicator should be impossible to miss.

A subtle but important rule: the Inspector panel and the comment thread are *not* modes. They are auxiliary surfaces that overlay any mode. A researcher in Document mode can have the Inspector open for the selected item without changing modes. Modes are about how the *content* is displayed; the Inspector is about a *single item's* properties.

---

## 7. Collaboration without Git: the draft-and-suggest model

The brief raises the right concern: Git is too developer-centric, but raw Google-Docs-style live editing is too undisciplined for research instruments where every change needs an audit trail and a rationale.

The synthesis is a two-tier collaboration model that uses developer-style mechanics under non-developer language.

### 7.1 The two tiers

**Tier 1 — Suggestions on the main draft.** The default behavior. Anyone with review access edits in *suggesting mode*, identical in spirit to Google Docs. Their changes appear inline with attribution, on a per-item basis. The PI or designated approver accepts or rejects, item by item or in batch. This handles 80–90% of all collaboration. No researcher is forced to learn the concept of a branch.

**Tier 2 — Alternative drafts.** When a reviewer wants to explore a substantially different version of the instrument — say, a shorter pilot version, a translated version, or a methodologically different approach — they create an *alternative draft*. This is a Figma branch in disguise. Branches in Figma are exploratory spaces that enable designers to safely try new ideas without making changes to the main file; rather than auto-saving to the main file, changes from branches are merged into the main file when they're ready. Same model, different vocabulary.

The vocabulary matters. Never use *branch*, *merge*, *pull request*, *fork*, *checkout*, *rebase*, *cherry-pick*. Use *draft*, *alternative version*, *propose changes*, *apply to main*, *compare*, *take from this version*. This isn't dishonesty; the underlying mechanics are the same. It's a translation layer for an audience whose mental model is research, not version control.

### 7.2 The user flow

A reviewer who wants to propose changes does this:

1. Opens the instrument in Review mode (an explicit mode toggle, like Google Docs).
2. Edits items normally. Each edit is silently captured as a *suggestion* attached to that item, with the reviewer's name, timestamp, and an optional rationale.
3. Adds a comment to any item with `Cmd+M`. Comments are threaded.
4. When done, clicks "Send for review." The PI is notified.

The PI's view:

1. Opens the instrument with a "23 suggestions on this draft" banner.
2. Each suggestion appears in line, with a green-on-strikethrough visual treatment matching Google Docs' suggesting mode.
3. For each suggestion: ✓ accept, ✕ reject, or 💬 reply. Bulk operations: "accept all from Reviewer X," "accept all in Module Y."
4. AI assistance: a "Resolve with AI" button on any comment that lets the methodologist agent propose how to address the underlying concern. The AI's suggested edit appears as another suggestion (visually identical to a human's), preserving the PI's authority to accept or reject.

This is significantly better than Word's track changes for one specific reason: every suggestion is attached to an *item*, not to a *character span*. A suggestion that says "rephrase this question" doesn't get orphaned when someone above it adds a paragraph. Items have stable identity; suggestions ride on that identity.

### 7.3 Alternative drafts (the "branch" path)

Used rarely — for translation work, parallel pilot versions, or methodologically different alternatives. The mechanic:

1. Click "Create alternative draft" from the version menu. Name it ("Bangla version," "Short pilot," "Stratified by gender").
2. The draft opens as a copy. Edit freely.
3. When ready, click "Compare to main" to see an item-by-item diff. The diff visualization is a two-column view: each item that differs shows side-by-side, with metadata about who changed it and when.
4. Click "Apply to main" to start a review. The owner of main reviews the diff, accepts or rejects on a per-item basis. This is a merge, but the user never sees that word.

Alternative drafts also receive updates from main. If main has changed since the draft was created, the system notifies and offers to "update this draft from main," at which point conflicts (items changed in both places) surface for resolution. See Section 8.

### 7.4 What the activity log looks like

Every state-changing action is logged: edits, suggestions, comment threads, accepts, rejects, mode switches. The log is presented as a human-readable timeline ("Aluk suggested rewording question 23"; "Verne accepted 14 suggestions in module 3"; "AI proposed clarification on question 7, rejected by Verne"), not a Git commit log. Filterable by author, by item, by date. Exportable as a methodology audit trail (this is a real value-add for clinical research and IRB compliance).

---

## 8. Conflict resolution that researchers can do

The hardest moment in any collaborative editor is when two people changed the same thing differently. Git-style three-way merges are unhelpful for non-developers because they require the user to think about three versions simultaneously (yours, theirs, common ancestor) and resolve at the character level.

Two design moves make this tractable:

### 8.1 Item-level granularity

The unit of conflict is the *item*, not the *character span*. Two reviewers cannot "both edit question 23" in a way that requires character-by-character merge logic. Instead, both reviewers' versions of question 23 are stored as alternative suggestions, and the resolver picks one or composes a third. This is structurally equivalent to picking a "theirs" or "ours" — but the visual presentation is "Reviewer A's version / Reviewer B's version / Original / Compose new" — four cards, each fully readable, with a single radio-button style choice.

This eliminates the visual complexity of `<<<<<<<` markers entirely. The cost is that the system can't auto-merge non-conflicting word-level changes in the same item; the resolver always sees the full item. For research instruments, where individual items are usually 1–3 sentences, this is the right tradeoff.

### 8.2 Conflict resolution as a guided flow

When merging an alternative draft back to main, the system surfaces conflicts as a step-by-step wizard:

1. "5 items changed in both versions. Let's resolve them one at a time."
2. Each conflict screen shows: Original (faded), Main draft's current version, Alternative draft's version, with full attribution and timestamp on each. A fourth option "Compose new" opens an editor.
3. Optional: "Ask AI to help merge these" — the methodologist agent reads both versions and proposes a synthesized one as a fourth option.
4. The resolver chooses; system records the decision and the rationale (optional comment).
5. After the last conflict, the system shows a summary "Apply 5 resolutions to main? (12 non-conflicting items will also be applied)" → confirm.

Compare this to a Git merge: instead of staring at conflict markers, the researcher sees four readable versions of the same question and picks one. The mental load is bounded by the small number of items, not by the size of the document.

### 8.3 The "many reviewers" case

Real research projects often have 4–8 reviewers commenting in parallel. The model still works:

- Each reviewer leaves suggestions on the same main draft.
- Multiple suggestions on the same item get stacked in the side margin (Reviewer A: "shorten this"; Reviewer B: "add a follow-up about frequency"; Reviewer C: "this is double-barrelled").
- The PI sees all suggestions on the item, can accept any subset, reject the rest, or compose a new version that synthesizes.
- The AI can be invoked to propose a synthesis: "Here's a version that addresses A, B, and C — would you like to use it?"

This is a massive workflow improvement over the Word-track-changes-back-and-forth that today's research teams suffer through.

---

## 9. Where the AI agents live in the UI

The brief frames this as agentic actions. The risk is that AI shows up as Yet Another Sidebar — a chat panel detached from the document. Following P7, the better approach is to make agents addressable from anywhere and to make their outputs look like any other reviewer's suggestions.

Three integration points:

### 9.1 Inline addressing

Right-click any item → "Ask agent..." → a small popover with three or four scoped operations:
- Review for clarity, leading language, double-barrelled phrasing
- Suggest validated instrument alternatives
- Translate to {language}
- Convert to ODK / Kobo equivalent
- Generate codebook entry

The agent's response appears as one or more *suggestions* on the item, indistinguishable in visual treatment from a human reviewer's. The researcher accepts or rejects in the same flow.

### 9.2 Comment-driven agent action

Any comment can be addressed to the agent: `@agent please make this gender-neutral` or `@agent suggest two follow-up items here`. The agent reads the comment plus the surrounding context (the item, the section, the overall study description) and replies — again as a suggestion on the item, not as a chat message in the comment thread.

This pattern unifies the comment system and the agent: agents *are* reviewers, with a different name and infinite availability.

### 9.3 Project-level orchestration

A small command bar (`Cmd+K`) exposes whole-project actions:
- "Review the entire instrument for methodology issues"
- "Check translation consistency across languages"
- "Generate the data dictionary"
- "Push to ODK Central"

These return *summary cards* listing items that need attention, each linking to the item in Document mode with a pre-populated suggestion. The researcher walks through them like a code review.

The methodologist agent should specifically *not* be a chat. Researchers don't want to converse with a method advisor; they want a method advisor to mark up their instrument. The chat metaphor is a distraction from the goal.

---

## 10. Specific resolutions for the open questions in the brief

The brief raised several specific tensions. Direct answers, with the theory above as backing:

**"How do we make this familiar to Word/Docs users?"**
Document mode is the default and reads like a Word document with better typography. The mode toggle is the single piece of new vocabulary they have to learn. Suggesting mode is identical to Google Docs Suggesting. They never have to encounter blocks or branches to do their work.

**"Block-per-item makes the UI overly blocky."**
True if blocks are rendered as visible cards. False if blocks are computational and the visual treatment is prose-shaped (Section 3, Implication 3). Block chrome appears on hover, not by default. A 60-item form looks like a 60-item methodology appendix, not a stack of cards.

**"How do we resolve multiple reviewer conflicts in a Git-like way without being developer-centric?"**
Item-level conflicts presented as 3–4 readable alternatives with clear attribution. No three-way text merge. Vocabulary from research review (drafts, alternatives, apply, compare), not from Git. AI-assisted synthesis as an additional option.

**"How do we balance the agent without making it a chatbox?"**
Agents are reviewers. They produce suggestions on items, addressable inline (right-click), via comments (`@agent`), and via the global command bar for whole-instrument operations. No persistent chat sidebar.

---

## 11. References this theory rests on

A short bibliography of what shaped this thinking, mapped to where each piece informs the design:

- *NN/G — Modes in User Interfaces (When They Help and When They Hurt Users):* the four-mode toggle (Section 6) and the rule that mode indicators must be visually unmissable.
- *NN/G — Few Guesses, More Success: 4 Principles to Reduce Cognitive Load in Forms:* the single-column reading layout, progressive disclosure, "one thing per page" patterns echoed in the Inspector.
- *Miller Lab — Optimizing Real World Data Collection: Field Formatting in REDCap:* the diagnostic that researchers want semantic visual structure (Section 1) and the failure mode of tools that force them to encode it in inline HTML/CSS.
- *Google Docs Suggesting mode:* the suggest/accept/reject pattern, the side-margin trail, and the explicit three-mode dropdown (Editing/Suggesting/Viewing).
- *Figma branching guides (Compound design system, LogRocket, Builder.io's "Git Branching for Designers"):* the mental model that branches are exploratory spaces, the importance of merging "little and often," and the translation problem when developer mechanics meet non-developer audiences.
- *Outliner literature (Wikipedia, Smashing Magazine on `document.designMode`):* the one-pane / two-pane / hybrid distinction informs the Document/Outline mode design; the principle that edit and view are modes over the same content.
- *Sweller's cognitive load theory and the working-memory ~7 limit:* the rule of thumb that any single screen should not require the user to hold more than a handful of distinct concepts in mind at once. The Inspector's "Advanced" toggle and the slash-menu's defaults follow this.
- *BlockNote / Tiptap / Yjs:* the right backend for the editor — block-based AST with CRDT-based real-time collaboration. The mistake to avoid is letting the backend's structure dictate the frontend's chrome.

---

## 12. What to build first (V1 UI scope)

To keep this from becoming a wishlist, here is a V1 scope that ships the theory in its smallest defensible form:

1. **Document mode and Outline mode only.** Defer Spec and Preview to V1.1 and V1.2. (Preview needs Enketo integration; Spec is a power-user feature and not on the critical path.)
2. **Inspector panel with a small set of properties:** type, label, choices, validation, relevance condition. Nothing more in V1.
3. **Suggesting mode with item-level granularity.** This is the single biggest workflow improvement over Word and is non-negotiable for V1.
4. **Per-item comments with `@agent` addressing.** Comments are first-class, threadable, resolvable.
5. **One agent integration point:** right-click → "Ask agent..." with a fixed set of four scoped operations. Defer the global command bar to V1.1.
6. **Activity log,** even if unfancy. This is the audit trail and it's needed from day one.
7. **Three exit paths:** download as Word (.docx), download as XLSForm (.xlsx), push to ODK Central. Anything beyond this is V1.1.
8. **Defer entirely:** alternative drafts and merge resolution. The single-draft + suggestions flow handles the 80% case. Add drafts when a real customer asks for them.

This is roughly what a senior frontend engineer plus a backend engineer can deliver in 12–16 weeks alongside the AI/agent work. It is ugly to read in a feature list, and it is *exactly* what a methodologist will love at first demo.

---

## 13. The one-paragraph version

Build a single-column, prose-shaped document editor whose blocks are computational rather than visual. Keep the chrome quiet by default and surface affordances on hover, click, and selection. Expose four isomorphic views — Document, Outline, Spec, Preview — over one canonical AST, with a clearly indicated mode toggle. Borrow Google Docs' Suggesting model for collaboration, but operate at the item level rather than the character span. Treat AI agents as reviewers whose outputs look identical to human suggestions, not as a separate chat surface. Use research vocabulary throughout (drafts, alternative versions, apply, compare) and never expose Git mechanics. The product's defining sensibility is that it disappears: a researcher spending an afternoon refining an instrument should feel like they were doing methodology, not configuring a tool.

---

*Drafted as a working theory; living document. Many of these decisions are reversible and most should be tested against real reviewer behavior in the first design partnership.*
