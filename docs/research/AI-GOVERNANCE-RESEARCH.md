# Dynodoc: Legal, Privacy and Ethics Research Brief

> Research input for the [agent provenance](../proposals/AGENT-PROVENANCE-PROPOSAL.md) and [AI assessment](../proposals/AI-ASSESSMENT-PROPOSAL.md) proposals, compiled by a research agent on 30 September 2026. **Not legal advice**: items marked "Lawyer to confirm" and the questions in section 10 need qualified counsel.
## AI provenance ledger, agentic editing, and AI-use assessment in education

Prepared: 30 September 2026. Jurisdictions: EU, UK, US (federal and selected states), Bangladesh.

---

## Disclaimer

This is a research brief, **not legal advice**. It summarises public sources as found on 30 September 2026. Several instruments are new (EU AI Omnibus, in force 27 July 2026; Bangladesh Personal Data Protection Act 2026; UK DUAA commencement in 2026). Others are still drafts (Commission high-risk guidelines, Bangladesh National AI Policy, EU GDPR "Digital Omnibus"). Items marked "Lawyer to confirm" need review by qualified counsel in the relevant jurisdiction before anyone relies on them. Bangladesh statutes are authoritative in Bengali (PDP Act 2026, s.45(2)). The English renderings below are this analyst's translations.

**Verification markers used throughout**

| Marker | Meaning |
|---|---|
| **[P]** | Checked against the primary text (official site, or a faithful mirror of statute text) fetched during this research |
| **[S]** | Confirmed only through reputable secondary sources (law-firm notes, press). The primary text was not fetched or was unavailable |
| **[K]** | Well-established statutory text stated from the analyst's knowledge. Not re-fetched in this session |
| **[U]** | Unverified, conflicting or incomplete. Treat with caution |

---

## Executive summary

1. **EU AI Act, Annex III point 3 (education)** lists four high-risk uses: admission, evaluating learning outcomes, assessing the level of education, and monitoring prohibited behaviour during tests [P]. The **AI Omnibus, Regulation (EU) 2026/1744**, is adopted and in force since 27 July 2026. It moves the Annex III application date from 2 Aug 2026 to **2 Dec 2027** [S]. Article 50 transparency still applied from **2 Aug 2026**. A grace period to 2 Dec 2026 covers machine-readable marking, and only for systems already on the market [S].
2. **A deterministic provenance ledger is probably not an "AI system" at all.** Rule-based recording of human, AI, paste and import origin falls outside the Commission's definition guidelines [P]. An **AI summariser that evaluates a student's process**, or **any automated scoring of prompts**, is likely to be Annex III 3(b) high-risk. The Article 6(3) exemptions are lost wherever the system *profiles* students. Draft Commission guidance (May 2026) reportedly treats point-3 systems as often involving profiling [S/U].
3. **Emotion recognition in education is prohibited now** (since 2 Feb 2025). The Commission's guidelines state that inferring emotions from **keystrokes (way of typing)** is covered by the ban. Inferring them from written text is not [P via mirror]. Dynodoc must never infer stress, engagement or anxiety from typing rhythm.
4. **Keystroke timing** is a recognised behavioural biometric (ICO; AI Act Recital 15; Bangladesh PDP Act s.2(14)). Under the GDPR it becomes *biometric data* once technically processed so that it can identify or verify a person. It becomes *special-category data* when used for that purpose [P]. The owner's claim that it is "close to biometric" **understates the risk**.
5. **The GDPR and UK GDPR require a DPIA** for the education product. It meets at least three WP248 criteria: evaluation or scoring, vulnerable data subjects, and systematic monitoring. **Automated scores that teachers rely on can count as Art. 22 decisions** (CJEU *SCHUFA*, C‑634/21) [S]. UK Articles 22A–22D replaced Art. 22 on 5 Feb 2026 [S].
6. **UK assessment rules align with Dynodoc's "attest, don't detect" model.** JCQ requires students to keep prompts and outputs in a *non-editable* form and to explain how they used AI [P]. JCQ's 2026‑27 regulations forbid AI as the "sole or primary" marker [P]. Ofqual prohibits AI as sole marker [P]. The DfE's Jan 2026 product safety standards *expect* logging of prompts and responses and tracking of "cognitive offloading" (paste, accepting autocomplete, "complete this for me") [P].
7. **US.** Under FERPA, Dynodoc would act as a "school official" under the school's direct control [P]. The COPPA amendments had a compliance date of 22 Apr 2026. The FTC did **not** codify the school-authorisation exception but will keep applying its guidance [S]. **California AB 1159 (Ch. 182, Stats. 2026, signed 10 Sep 2026)** bars operators from using student information to train or develop AI [P status; S content].
8. **Bangladesh: the owner's claim "PDP Act 2026" is correct.** The **Personal Data Protection Act, 2026 (Act No. 63 of 2026, dated 10 Apr 2026)** repealed the 2025 Ordinance and the Feb 2026 Amendment Ordinance [P]. A child is anyone under 18, who needs parent or guardian consent. Biometric data (including behavioural data) is sensitive. The Act includes erasure "propagation" that reaches backups. Cross-border transfer is allowed on consent, contract or consented education purposes, to destinations with prescribed "suitable technology". The enacted text contains **no explicit data-localisation mandate** [P]. The complaint and fine sections commence later (not before about May 2027) [P].
9. **Tamper-evidence helps as evidence but does not establish authorship.** Hash chains show that a record was not altered after it was written. External anchoring (RFC 3161 timestamps, eIDAS time stamps or ledgers, periodic checkpoints held by the school) makes the log more credible. Misconduct processes need the student to **see all the evidence**, a **balance-of-probabilities** standard and an appeal (JCQ 5.33 and 6.20; OIA) [P]. A report must say clearly what the system *could not* observe.
10. **Biggest structural risks.** First, the append-only log conflicts with erasure and rectification rights (GDPR Art. 17; Bangladesh s.14(6)). Second, a "grade the process" AI feature pulls the product into high-risk status. Third, fairness for students using assistive technology, input-method editors (e.g. Bengali IMEs) or dictation, whose input can look like "paste".

---

## 1. EU AI Act (Regulation (EU) 2024/1689)

### 1.1 Annex III point 3: exact text [P]

Annex III opens: "High-risk AI systems pursuant to Article 6(2) are the AI systems listed in any of the following areas:". Point 3, *Education and vocational training*:

- (a) "AI systems intended to be used to determine access or admission or to assign natural persons to educational and vocational training institutions at all levels";
- (b) "AI systems intended to be used to evaluate learning outcomes, including when those outcomes are used to steer the learning process of natural persons in educational and vocational training institutions at all levels";
- (c) "AI systems intended to be used for the purpose of assessing the appropriate level of education that an individual will receive or will be able to access, in the context of or within educational and vocational training institutions at all levels";
- (d) "AI systems intended to be used for monitoring and detecting prohibited behaviour of students during tests in the context of or within educational and vocational training institutions at all levels".

Source: text via artificialintelligenceact.eu mirror. The official OJ is at eur-lex.europa.eu/eli/reg/2024/1689/oj.

**Commission AI Act Service Desk examples (non-binding) [P]**

In scope of 3(b):
- AI grading and feedback on assignments that "count towards a final evaluation";
- a "personalised learning assistant" that generates end-of-unit reports with grades and strengths and weaknesses.

Out of scope:
- non-formal language apps that give instant corrections "but which does not lead to the attainment of a credential";
- pronunciation feedback tools;
- neurodiverse learning companions.

Filtered out under Art. 6(3):
- a grade calculator (narrow procedural task).

Under 3(d):
- in scope: proctoring "by using a combination of facial recognition, **keystroke analysis**, and screen monitoring", and real-time ML behaviour analysis for cheating;
- out of scope: plagiarism checks of homework against a database ("not monitoring during tests").

### 1.2 Article 6(3) derogations and the profiling rule [P]

An Annex III system is not high-risk "where it does not pose a significant risk of harm", if it is intended to do one of the following:
- (a) "perform a narrow procedural task";
- (b) "improve the result of a previously completed human activity";
- (c) "detect decision-making patterns or deviations from prior decision-making patterns and is not meant to replace or influence the previously completed human assessment, without proper human review";
- (d) "perform a preparatory task to an assessment".

Profiling exception: "an AI system referred to in Annex III shall always be considered to be high-risk where the AI system performs profiling of natural persons." Profiling has the GDPR Art. 4(4) meaning: automated evaluation of personal aspects such as "performance", "reliability" and "behaviour" [P].

Art. 6(4): a provider relying on the derogation "shall document its assessment before" placing the system on the market. It is subject to registration under Art. 49(2) and must hand the documentation to authorities on request [P]. Reports say the Omnibus kept this registration duty but simplified the documentation [S].

**Draft Commission guidelines on high-risk classification** (published 19 May 2026; consultation closed 23 Jul 2026; final version expected by end-2026) [S]:
- The filter "is available only where the AI system does not materially influence the outcome" and "cannot be relied upon at all" where the system profiles [S, Osborne Clarke].
- One summary quotes draft ¶215. Point‑3 systems "will often perform profiling ... They should always be classified as high-risk, even if the system fulfils the conditions of the filter mechanism" [S, Modulos].
- Another summary says the guidelines do *not* presume profiling in education [S, Dastra].

**[U] Conflicting. Lawyer to confirm against the draft text.**

### 1.3 Article 5(1)(f): emotion recognition in education [P]

Prohibited: "the use of AI systems to infer emotions of a natural person in the areas of workplace and education institutions, except where the use ... is intended ... for medical or safety reasons". It has applied since 2 Feb 2025.

"Emotion recognition system" means inference "on the basis of their biometric data" (Art. 3(39)). "Biometric data" in the AI Act does **not** require unique identification (Art. 3(34)) [P].

The Commission's *Guidelines on prohibited AI practices* (4 Feb 2025) give two contrasting examples:
- "An AI system inferring emotions from written text (content/sentiment analyses) ... is not based on biometric data and therefore does not fall within the scope of the prohibition."
- "An AI system inferring emotions from keystroke (way of typing), facial expressions, body postures or movements is based on biometric data and falls within the scope of the prohibition."

[P, quoted in Iren/Noldus/Brouwer analysis of the guidelines. The Commission's PDF was not fetched directly.]

**Dynodoc implication:** no inference of attention, anxiety, frustration or engagement from typing rhythm, pauses or cursor movement. This applies in schools and, for the agentic product, in workplaces too.

### 1.4 Article 50: transparency [P]

- **50(1), providers.** Systems "intended to interact directly with natural persons" must be designed so people "are informed that they are interacting with an AI system", unless that is obvious.
- **50(2), providers.** Providers of AI systems "generating synthetic audio, image, video or text content, shall ensure that the outputs ... are marked in a machine-readable format and detectable as artificially generated or manipulated". This does not apply where systems "perform an assistive function for standard editing or do not substantially alter the input data".
- **50(4), deployers.** Deployers must disclose AI-generated or manipulated text "published with the purpose of informing the public on matters of public interest". This does not apply where the content "has undergone a process of human review or editorial control and where a natural or legal person holds editorial responsibility".
- **50(5).** Information must be clear, at first interaction or exposure, and accessible.

**Commission Guidelines on Art. 50** (final, 20 Jul 2026) [P date; S content]:
- Obligations attach to *AI systems*. Application-layer providers using third-party models are therefore providers [S, Paul Weiss].
- "Standard editing" covers grammar, spelling, minor stylistic polishing and, now, translation.
- AI summaries and substantive rewrites still require marking [S].
- For text, "metadata and provenance fields are the practical path" [S].
- The editorial exception needs genuine responsibility. Routine or pro-forma sign-off is not enough [S].

**Code of Practice on Transparency of AI-generated Content** (final 10 Jun 2026; assessed as adequate by the Commission) [P date; S detail]:
- a layered approach (signed metadata plus imperceptible watermarking);
- simplified requirements for free-form text;
- reportedly no watermarking needed for text under about 200 tokens.

Lawyer to confirm against the Code's text.

**Who is obliged?**
- Dynodoc is the *provider* of its in-editor assistant (it places it on the market under its own name: Art. 3(3) [P]). It must mark substantive AI outputs in machine-readable form. Its span-level origin marks, carried into exports as signed metadata, are a natural way to comply.
- Students are not "deployers" when writing coursework: personal non-professional use is excluded by Art. 3(4) [P], and coursework is not public-interest publication. Their disclosure duty comes from **assessment rules** (JCQ), not Art. 50.
- In the agentic product, customers publishing public-interest text are deployers under 50(4). Dynodoc's review and approval records can help *evidence* the human-review exception, but only if review is substantive.

### 1.5 Dates of application (status 30 Sep 2026)

| Obligation | Original date | After AI Omnibus (Reg. (EU) 2026/1744) |
|---|---|---|
| Art. 4 AI literacy; Art. 5 prohibitions | 2 Feb 2025 | Unchanged. Art. 4 reworded to "support and facilitate" literacy [S] |
| GPAI obligations; penalties chapter | 2 Aug 2025 | Unchanged [S] |
| Art. 50 transparency | 2 Aug 2026 | Unchanged. **50(2) marking grace to 2 Dec 2026 only for systems placed on market before 2 Aug 2026** [S] |
| Annex III high-risk (incl. education) | 2 Aug 2026 | **2 Dec 2027** [S; mirror of amended Art. 113 [P]] |
| Annex I (product-embedded) high-risk | 2 Aug 2027 | **2 Aug 2028** [S] |
| New prohibitions (non-consensual intimate imagery / CSAM generation) | none | 2 Dec 2026 [S] |
| High-risk systems used by public authorities (legacy) | 2 Aug 2030 (Art. 111) | 2 Aug 2030; legacy rule reportedly applied at type/model level [S]. **Lawyer to confirm** |

**Legislative history** [S]. Commission proposal 19 Nov 2025. EP approval 16 Jun 2026; Council adoption 29 Jun 2026; signed 8 Jul 2026. Published in the OJ 24 Jul 2026 and in force 27 Jul 2026.

The GDPR/ePrivacy half of the "Digital Omnibus" is **not** adopted. It was still at first reading in September 2026, with no Council mandate [S].

### 1.6 Would Dynodoc's components fall inside Annex III? (analysis)

| Component | AI system (Art. 3(1))? | Annex III point 3? | Assessment |
|---|---|---|---|
| (i) Deterministic provenance ledger (rule-based origin marks, hash chain) | **Probably not.** The Commission's definition guidelines exclude systems "based on the rules defined solely by natural persons to automatically execute operations" and "basic data processing" [P] | No | Outside the high-risk regime. GDPR still applies in full. Caveat: if any origin mark is *inferred* by an ML classifier (e.g. "AI-modified" detected statistically), that component becomes an AI system |
| (ii) AI summariser surfacing decision points for the grading teacher | Yes | Arguably 3(b): it is intended for use in evaluating learning outcomes | **Defensible** under 6(3)(a)/(d) only if it *extracts and orders* observed events (e.g. "prompt 4 → AI text accepted into para 2 → edited") without judging quality, competence or integrity, and the teacher decides. **Not defensible** if it characterises the student ("over-reliant", "shallow engagement"). That is evaluating personal aspects, i.e. profiling, so it is always high-risk. Must document under Art. 6(4) and register under Art. 49(2). **Lawyer to confirm** |
| (iii) Automated scoring of prompts or AI-use quality | Yes | **Yes, 3(b)**; also profiling | High-risk from 2 Dec 2027 if placed on the EU market. Also engages GDPR Art. 22 (see 2.3) and JCQ/Ofqual limits on AI marking |
| In-editor assistant (tutor) | Yes | Not by default (Service Desk: tutors that do not evaluate outcomes are out of scope) | Art. 50(1) and 50(2) apply. Keep it out of grading |
| Exam mode that *blocks* the assistant | No, if deterministic | No | Blocking is prevention, not "monitoring and detecting" |
| Exam mode that *flags suspicious behaviour* (ML on paste, typing, timing) | Yes | **Yes, 3(d)** | Avoid, or accept high-risk status. Emotion inference from typing is prohibited |

"Intended purpose" includes marketing and sales statements (Art. 3(12)) [K]. A deployer that repurposes a tool for a high-risk use becomes its provider (Art. 25(1)(c)) [K]. So marketing copy such as "AI grades how students use AI" can itself trigger classification.

### 1.7 Provider vs deployer obligations if a component is high-risk [K unless marked]

**Dynodoc as provider:**
- risk management (Art. 9) and data governance (Art. 10);
- technical documentation (Art. 11);
- automatic logging (Art. 12);
- instructions for use (Art. 13);
- human-oversight design, including guarding against **"automation bias"** (Art. 14(4)(b));
- accuracy and robustness (Art. 15);
- quality management system (Art. 17);
- internal-control conformity assessment for Annex III (Art. 43 / Annex VI), EU declaration, CE marking and registration (Arts. 47–49);
- post-market monitoring and incident reporting (Arts. 72–73);
- as a provider established outside the EU (Bangladesh), an **EU authorised representative** (Art. 22).

The AI Act reaches third-country providers whose output is used in the EU (Art. 2(1)(c)).

**School or university as deployer (Art. 26)** [P]:
- use per instructions;
- assign competent human oversight;
- ensure input data is relevant;
- monitor, and suspend if risks arise;
- keep logs "at least six months";
- inform students that they are subject to the system.

Public bodies and private entities providing public services must also run a fundamental-rights impact assessment (Art. 27) [P]. Affected persons can ask the deployer for "clear and meaningful explanations of the role of the AI system" (Art. 86) [P].

**Penalties** [K]: prohibited practices up to EUR 35m or 7% of turnover; high-risk and Art. 50 breaches up to EUR 15m or 3% [S for Art. 50].

---

## 2. GDPR, UK GDPR and children

### 2.1 Lawful bases for schools and universities [K]

- **Public schools and universities** normally rely on Art. 6(1)(e), public task, or (c), legal obligation. Legitimate interests (6(1)(f)) is unavailable to public authorities performing their tasks (Art. 6(1), final sentence).
- **Consent** is weak in education because of the imbalance of power (Recital 43). It cannot be "freely given" if refusing affects grades.
- **Private institutions** may use contract or legitimate interests.

The DfE standards note the ICO's current view that "legitimate interest is likely to be the relevant lawful basis for ... generative AI products used by children" [P, DfE standards].

**Dynodoc's role.** Dynodoc is normally a **processor** (Art. 28) for the school. It becomes a **controller** for any own purposes (product improvement, model training, analytics). The Danish DPA held that 53 municipalities unlawfully shared pupils' data with Google for Google's own development purposes (Art. 6(1)(e) lacked a basis) [P, Datatilsynet, Jan 2024].

### 2.2 Article 8 (children's consent) [K]

Art. 8 applies only where **consent** is the basis *and* an information society service is offered **directly to a child**. The default age is 16; Member States may lower it to 13 (UK: 13). Where the school contracts Dynodoc and relies on public task, Art. 8 is not engaged. Art. 8 does apply to any direct-to-consumer or student sign-up route.

### 2.3 Article 22 (automated decisions) [K / S]

Art. 22(1) bars decisions "based solely on automated processing, including profiling" with legal or similarly significant effects, subject to limited exceptions and safeguards. Those safeguards include the right to human intervention, to express one's view and to contest (22(3)). Recital 71: such measures "should not concern a child" [K].

In **CJEU C‑634/21 *SCHUFA* (7 Dec 2023)**, an automated score was held to be an Art. 22 decision where a third party "draws strongly" on it [S]. By analogy, a prompt score or "AI-reliance index" that teachers routinely adopt could be caught even with a nominal human in the loop.

**UK:** DUAA 2025 s.80 replaced Art. 22 with **Arts. 22A–22D from 5 Feb 2026** (SI 2026/82) [S]. The new model permits such decisions with safeguards: information, human review, contest. It is stricter for special-category data [S].

### 2.4 DPIA (Art. 35) [K / S]

The WP248 rev.01 (endorsed by the EDPB) criteria include:
- evaluation or scoring;
- automated decisions with significant effect;
- systematic monitoring;
- sensitive data;
- large scale;
- **vulnerable data subjects (children)**;
- innovative technology.

"In most cases" two criteria mean a DPIA is required [S]. The education product meets at least three, so **a DPIA is required**. The DfE standards also expect a DPIA "during the full life cycle of the tool" [P]. So do the ICO Children's Code (standard 2) and Bangladesh's SOP power on impact assessment (PDP Act s.27(2)(ঞ)) [P].

### 2.5 Minimisation, storage limitation, and the append-only problem [K / S]

- Art. 5(1)(c) and (e) require collecting only what the stated purpose needs and keeping it no longer than necessary.
- An **append-only hash chain conflicts with erasure (Art. 17) and rectification (Art. 16)**.
- **EDPB Guidelines 02/2025 on blockchain** (draft Apr 2025; reported finalised July 2026):
  - avoid storing personal data on-chain;
  - keep it off-chain;
  - make erasure effective, e.g. by deleting off-chain data or destroying keys;
  - "technical impossibility cannot be invoked to justify non-compliance" [S; draft PDF on edpb.europa.eu].
- Bangladesh goes further. Erasure and rectification must propagate to "mirror, cache, backup, DR, test environment, migration dataset" copies (PDP Act s.14(6)) [P].

**Design consequence.** The chain should commit to *salted hashes or ciphertext*, with content stored separately under per-student or per-document keys. Deleting the key then "crypto-shreds" the content while the chain still verifies. The same approach is needed for Dynodoc's engine invariant "events are append-only". **Lawyer to confirm** that this satisfies Art. 17 in the relevant Member States.

### 2.6 Is keystroke timing or typing rhythm biometric data?

- **GDPR Art. 4(14)** [P]: biometric data is personal data "resulting from specific technical processing relating to the physical, physiological or behavioural characteristics ... which allow or confirm the unique identification". Art. 9(1) makes it special-category only when processed "for the purpose of uniquely identifying a natural person" [K].
  - Raw keydown timestamps stored for audit are **personal data**.
  - Once processed into a typing-rhythm profile able to identify or verify a person, they are **biometric data**.
  - Used to check "is this the same student?", they are **special-category** data under Art. 9, requiring an Art. 9(2) condition (often explicit consent, problematic in schools).
- **ICO** [P]: behavioural characteristics include "how they type". "Keystroke recognition" is listed as a behavioural biometric technique. Such data is special category "only ... if you use it to uniquely identify someone". The guidance is under review following DUAA.
- **AI Act** [P]: Recital 15 lists "keystrokes characteristics" among biometric features. Biometric data needs no identification purpose (Art. 3(34)). Emotion inference from keystrokes is prohibited in education (1.3).
- **Bangladesh PDP Act s.2(14)** [P]: biometric data covers measurements of "physical, physiological or behavioural characteristics" capable of identifying or confirming identity. It is **sensitive data** (s.2(21)(খ)) needing specific consent (s.7(ক)).
- **US** [K/U]: Illinois BIPA's enumerated "biometric identifiers" do not list keystrokes. The COPPA 2025 rule added biometric identifiers to "personal information" with an open "such as" list. **Lawyer to confirm** the exact COPPA text.

**Conclusion.** Record *events* (what text entered, from where, when, at coarse granularity). Do **not** record or analyse inter-key timing. This avoids biometric status in every jurisdiction reviewed.

### 2.7 Regulator guidance on edtech and proctoring

| Authority | Instrument / decision | Key point |
|---|---|---|
| ICO (UK) | *The Children's code and edtech*, 30 May 2023 [P] | The code applies when an edtech provider influences purposes, e.g. research beyond the core service, marketing, or "your own commercial purposes, which includes product development". It is excluded only if the provider acts "solely on the instruction of the school" for its public tasks and is not direct-to-consumer. Controller/processor status is determined by facts, "not ... by your designation ... in a contract" |
| ICO | *Edtech examined* statement, 24 Jun 2026 [P] | 28 providers audited. Findings: controller/processor confusion, weak contracts, weak minimisation and storage limitation, DPIA gaps. The ICO is discussing a new **edtech code** with DfE |
| CNIL (France) | Délibération 2023‑058 (8 Jun 2023) on remote exam proctoring [S] | Offer an in-person alternative. No biometric template databases. No facial recognition during the exam. **No automatic behavioural analysis** (false-positive risk) |
| Garante (Italy) | Bocconi / Respondus, decision 21 Sep 2021 [S] | EUR 200,000 fine. Biometric processing without a legal basis, inadequate transparency. Profiling and US transfers banned |
| Datatilsynet (Denmark) | Chromebook / Google Workspace, 30 Jan 2024 [P page] | No legal basis for sharing pupils' data for the vendor's own development purposes |
| EDPB | No edtech-specific guidelines found. Children's-data work ongoing; age-assurance statement Feb 2026 [U] | Apply WP248, the consent guidelines, and the blockchain guidelines |

### 2.8 Right of access to one's own record [K / S]

- GDPR Art. 15 gives students (or parents, per national law) access to their data.
- **CJEU C‑434/16 *Nowak* (20 Dec 2017)**: a candidate's answers *and the examiner's comments* are the candidate's personal data [S].
- Prompts, AI outputs, origin marks and teacher annotations are therefore likely accessible to the student.
- **UK nuance:** DPA 2018 Sch. 2 para. 25 exempts "information recorded by candidates during an exam" from access and sets special timing for exam marks [K]. Whether in-exam prompts fall within this exemption: **Lawyer to confirm**.
- Bangladesh s.11(3) requires a copy in "concise and intelligible format", including "the logic and significance of automated decisions" [P].

### 2.9 UK Age Appropriate Design Code (Children's Code)

The code applies to information society services "likely to be accessed by children". For edtech, see the ICO criteria in 2.7. From 5 Feb 2026, DUAA s.81 adds a statutory duty for such services to consider "children's higher protection matters" in data protection by design (Art. 25 UK GDPR) [S].

Relevant standards [K]:
- 2: DPIA;
- 7: high-privacy defaults;
- 8: data minimisation;
- 11: children told when they are monitored; DfE confirms "section 11 refers to monitoring" [P];
- 12: profiling off by default.

### 2.10 International transfers (VPS and a Bangladesh-based team) [K]

Hosting EU or UK students' data outside the EU/UK, or remote access by staff in Bangladesh (no adequacy decision), is a **restricted transfer**. It needs SCCs or the IDTA/UK Addendum plus a transfer impact assessment (GDPR Arts. 44–46). Choose EU or UK hosting for EU or UK schools, and minimise support access from outside. The DfE standards require disclosure of "where data will be processed, and whether there are appropriate safeguards in place if this is outside the UK or EU" [P].

---

## 3. Education-specific rules on AI in assessment

### 3.1 JCQ (UK general qualifications)

- ***AI Use in Assessments*** (30 Apr 2025) [P]:
  - Students must acknowledge AI use, naming the tool and date.
  - They "must retain a copy of the question(s) and computer-generated content for reference and authentication purposes, in a **non-editable format** (such as a screenshot) and provide a brief explanation of how it has been used".
  - Teachers "must only accept work ... which is the students' own".
  - Detection tools "may be used" but "should form part of a holistic approach". Their accuracy varies with "an individual's English language competency", and they give "lower scores for AI-generated content which has been subsequently amended".
  - Before the student signs the declaration, the centre handles concerns internally. After signing, the case is reported to the awarding body.
- ***General Regulations for Approved Centres 2026‑27*** (effective 1 Sep 2026), para 5.3(b) [P]: "teaching staff do not use AI tools as the sole or primary means of marking candidates' work. Teachers must review all the work in its entirety and determine the mark ... using their own professional judgement regardless of the outcomes of an AI tool."
- ***Suspected Malpractice: Policies and Procedures 2025‑26*** (republished 19 May 2026) [P]:
  - para 5.33: the accused must "be made aware of all evidence ... which supports the allegation" and have time and "an opportunity to submit a written statement", then an appeal;
  - para 6.20: the committee decides "on the balance of probabilities ... (i.e. that it is more likely than not)";
  - a child's parent or carer is kept informed.

**Fit with Dynodoc.** A tamper-evident in-editor log is a stronger "non-editable" record than a screenshot. Offering it as the student's own evidence pack is aligned with JCQ. Using it as a machine judgment is not.

### 3.2 Ofqual [P]

Ofqual's *Approach to regulating the use of AI in the qualifications sector* (updated 16 Jul 2026 per GOV.UK):
- "AI may not be used as a sole marker in any assessments that are part of regulated qualifications";
- marking must be "accountable, capable of being explained and, where necessary, challenged".

A Jan 2026 paper sets principles for AI in marking high-stakes assessments [S].

### 3.3 DfE (England)

- **Policy paper *Generative AI in education*** (last updated 12 Aug 2025 per GOV.UK) [P]: "It is recommended that personal data is not used in generative AI tools". Warns that free tools may train on inputs. Professional judgement remains responsible.
- ***Generative AI: product safety standards*** (first published 22 Jan 2025; updated **19 Jan 2026**, when cognitive, emotional/social, mental-health and manipulation standards were added) [P]:
  - **Monitoring:** "must maintain robust activity logging ... recording input prompts and responses". Alert the Designated Safeguarding Lead. Report to teachers "the rate of requests for cognitive offloading", "the level of personal and emotional engagement ... without directly disclosing the content", and "duration of usage".
  - **Cognitive development:** products should "not provide final answers, full solutions, or complete worked examples by default". They should use "progressive disclosure" and "create friction, or require teacher approval" before switching to full-solution modes. They should "track and report when learners offload thinking", e.g. "pasting text into an answer box", "accepting an auto-complete suggestion that fills most or all of the answer", or "a 'complete this for me' ... option".
  - **Privacy:** DPIA across the life cycle. No commercial use "including further model training and fine-tuning, without confirmation of appropriate lawful basis". Comply with the Children's Code.
  - **IP:** learner inputs should not be "collected, stored, shared for any commercial purposes ... product improvement, and product development" without the copyright owner's permission, which comes from a parent or guardian for under‑18s.
  - **Governance:** risk assessment and a formal complaints mechanism.

**Tension to manage.** The DfE *expects* logging and engagement monitoring. The ICO and ethics considerations require transparency to the child and minimisation. The EU prohibits biometric emotion inference. Dynodoc should implement DfE "offloading" metrics as **deterministic event counts** shown to the student as well as the teacher. It should not implement them as inferred states.

### 3.4 US FERPA [P / K]

- Records "directly related to a student" and maintained by the school or a party acting for it are education records [K].
- A vendor may receive them as a **"school official"** if it (1) performs an institutional service, (2) is "under the direct control of the agency or institution with respect to the use and maintenance of education records", and (3) complies with the re-disclosure limits (34 CFR 99.31(a)(1)(i)(B)) [P].
- Parents, or eligible students (18+ or postsecondary), may inspect and review records; the school must respond within 45 days [K].
- ED's rulemaking to amend FERPA (RIN 1875‑AA15) is pending. A reported April 2026 NPRM is **[U]**.

### 3.5 COPPA (2025 amendments) [S]

- Final rule published 22 Apr 2025, effective 23 Jun 2025, **compliance by 22 Apr 2026**.
- New requirements:
  - a written data-retention policy and no indefinite retention;
  - a written information-security programme;
  - separate verifiable parental consent for third-party disclosures;
  - biometric identifiers added to "personal information".
- The FTC **did not** codify the school-authorisation exception, citing pending FERPA changes. It will continue to follow its FAQ guidance: schools may consent for purely educational use, with no commercial use.

### 3.6 State student-privacy laws

- **California SOPIPA/KOPIPA** (Bus. & Prof. Code §22584) [P]. Operators with actual knowledge of K‑12 use may not:
  - engage in targeted advertising;
  - "amass a profile about a pupil" except for K‑12 school purposes;
  - sell pupil information.
  They must also maintain reasonable security and delete on school or parent request.
- **California AB 1159** [P status; S content]: approved 10 Sep 2026 (Ch. 182, Stats. 2026).
  - Prohibits using student information "to train a generative AI system or to develop an AI system".
  - Adds higher-education coverage (HESIPA, operative 1 Jul 2027) and a private right of action.
  - The general operative date is to be confirmed; non-urgency statutes usually take effect 1 Jan 2027.
- **Others** [S]:
  - New York Education Law §2‑d (parents' bill of rights; security) [K];
  - Illinois SOPPA [K];
  - Oklahoma SB 1734 (district AI policies; reportedly bars AI as "primary basis for grading") [S];
  - Colorado SB 26‑189 (repeals and re-enacts the Colorado AI Act; effective 1 Jan 2027; education is a "consequential decision") [S, lawyer to confirm].

### 3.7 US Department of Education AI guidance

- 2023: OET report *AI and the Future of Teaching and Learning* [K].
- July 2024: *Designing for Education with AI: An Essential Guide for Developers* [K]. OET was closed in 2025, so the current status of these pages is **[U]**.
- **OCR, *Avoiding the Discriminatory Use of AI* (20 Nov 2024)** [S]: includes an example of a school relying on an AI-detection checker over an English learner's objections, as a potential Title VI issue. Current status under the present administration: **[U]**.
- **EO 14277, *Advancing AI Education for American Youth*** (23 Apr 2025) [S].
- **ED Dear Colleague Letter (22 Jul 2025)** on using grant funds for AI [S]. Its principles: educator-led, ethical, accessible, "transparent and explainable", and FERPA-compliant. There is also a proposed supplemental priority on AI literacy.

### 3.8 International soft law

- **UNESCO** *Guidance for generative AI in education and research* (Sep 2023): recommends a minimum age of 13 for classroom use [S].
- **European Commission** *Guidelines on the ethical use of AI and data in teaching and learning for educators*: updated 2026 edition; listed release dates vary between May and June 2026 [S].
- **UNICEF**: *Policy guidance on AI for children 2.0* (2021) and *Guidance on AI and Children 3.0* (2025) [S].

---

## 4. Bangladesh

### 4.1 Personal Data Protection Act, 2026 (ব্যক্তিগত উপাত্ত সুরক্ষা আইন, ২০২৬) [P, official Bengali text]

**Enactment path**
- Personal Data Protection Ordinance 2025 (Ord. No. 61), promulgated 6 Nov 2025 [P via s.44].
- Amendment Ordinance 2026 (Ord. No. 23), 5 Feb 2026 [P via s.44].
- Parliament passed the **Personal Data Protection Bill 2026 on 9 Apr 2026** among 31 ordinance-replacement bills [S, Financial Express]. It became **Act No. 63 of 2026, dated 10 Apr 2026** [P].
- s.44 repeals both ordinances and saves actions taken under them [P].
- An authoritative English text is to be gazetted; Bengali prevails (s.45) [P].
- CCIA (Apr 2026) reported that a March 2026 revision removed the localisation mandates and officer criminal liability. The enacted text is consistent with that [S + P].

**Key provisions (analyst's translation)**

| Topic | Section | Content |
|---|---|---|
| Commencement | s.1(3) | Deemed in force **6 Nov 2025**, except **s.23 (Chief Data Officer) and ss.31–35 (complaints, fines, compensation)**. These commence on a date the Government notifies *after 18 months from promulgation*, i.e. not before about May 2027. **Lawyer to confirm** |
| Territorial scope | s.1(2) | Citizens and residents; processing in Bangladesh; and processing **outside Bangladesh** connected with providing goods or services to, or **monitoring or profiling**, data subjects in Bangladesh |
| Profiling | s.2(13) | Automated evaluation, analysis or prediction of performance, preferences, interests, reliability, **behaviour, habits**, location |
| Biometric data | s.2(14) | Data from measurement or technical processing of physical, physiological **or behavioural** characteristics able to identify or confirm identity (DNA, fingerprints, face, iris, voiceprint, gait, etc.) |
| Child | s.2(19) | Under **18**, or another age the Government prescribes |
| Sensitive data | s.2(21) | Includes genetic and **biometric** data, health, beliefs, ethnicity, real-time location |
| Lawful basis | s.5 | Consent must be voluntary, specific, clear, revocable and informed (purpose, retention, transfer, withdrawal). Alternatives: contract with the data subject, legal claims, vital interests, employment and social-security law, manifestly public data. **No general "public task" or balancing test.** Burden of proving consent is on the fiduciary |
| Sensitive processing | s.7 | Specific consent, or listed exceptions |
| Children | s.9 | Collection or processing only with **parent, legal guardian or authorised decision-maker consent**, in the child's interests. Consent lasts until age 18 |
| Rights | ss.10–14 | Access and portability, including a copy with "the logic and significance of automated decisions" (s.11(3)). Rectification, with notice to all within 30 days (s.12). Withdrawal of consent including to **automated decisions** (s.13(1)(গ)). Erasure (s.13(2)). No automated processing without informing the data subject (s.13(6)). **System-wide propagation** of rectification and erasure, including to backups and DR copies (s.14(6)) |
| Security | s.17 | Includes pseudonymisation, encryption and resilience testing |
| Retention | ss.18–19 | No retention beyond the prescribed period. A records register for **at least 5 years** (scope ambiguous) |
| Breach | s.20 | Notify the Authority if a breach is likely to cause significant harm (form and time by regulation) |
| Exemptions | s.24 | Consent exemptions include public interest, research and statistics, and a clause on "archival ... education" (ambiguous drafting: "সংরক্ষণাগার সম্পর্কিত (Archival) শিক্ষা"). **Lawyer to confirm** whether schools can rely on it |
| Transfer | s.29 | Government may classify data (public, internal, confidential, restricted) per a Schedule, which was not retrievable. Transfer abroad is allowed with (a) consent, (b) contract, or (c) consent for purposes incl. **education**. Only to places with "suitable technology and equipment" prescribed by regulation. Bulk transfers of sensitive identifiers (NID, biometrics, DNA, criminal records) must be notified to the Authority |
| Regulator | s.2(4) | National Data Management Authority under the National Data Management Act 2026 (Act No. 80 of 2026, 10 Apr 2026) [P] |
| Sanctions | ss.32–36 | Administrative fines up to BDT 25 lakh (50 lakh for significant fiduciaries); compensation; fines on implicated company officers (s.36); appeal to the ICT Act tribunal |

**Owner's claim "Bangladesh's PDP Act 2026": verified.** The data-localisation and criminal-penalty descriptions in the Oct 2025 press coverage of the *draft ordinance* do not match the enacted Act.

### 4.2 Cyber Security Act, 2026 (Act No. 81 of 2026) [P]

- Repeals and re-enacts the Cyber Security Ordinance 2025 (Ord. No. 25). Deemed effective **21 May 2025**.
- s.17 creates offences for unlawful access to critical information infrastructure. It expressly names "software developer or AI tools user" and data generated "by an artificial intelligence agent". This is relevant to the MCP agent API: keep audit trails of agent credentials and scopes.
- s.39 protects and binds service providers that disclose information for investigations; disclosures are reported half-yearly to the National Cyber Security Council.
- s.43 makes forensic evidence admissible notwithstanding the Evidence Act 1872.
- A **Cyber Security (Amendment) Bill 2026** passed on 30 Jun 2026 (platform content-removal timelines) [S, BSS].

### 4.3 National AI Policy

- ***Bangladesh National AI Policy 2026–2030*, Draft v2 (9 Feb 2026)**, ICT Division [P]. Relevant provisions:
  - AI companies must not collect data of children under 16 without guardian consent;
  - "profiling, and behavioural tracking of children shall be prohibited";
  - a ban on "using children's data for training AI models";
  - a rights framework for decisions in "education" with rights "to understand, contest, and seek human review";
  - an education section: modernise academic integrity, "limit over-reliance on AI tools", protect "student data, examinations, and academic records".
- **Status:** in July 2026 the ICT Minister said the policy was still being formulated [S, Dhaka Tribune / UNB]. No adoption found **[U]**.
- **Relevance.** If the child-profiling ban is adopted as written, grading students' AI-use behaviour could be challenged. The design should present the log as the **student's own evidence**, not as behavioural tracking.

### 4.4 Education-sector AI guidance

- No Ministry of Education or NCTB instrument on AI in assessment was found **[U]**.
- On 23 Apr 2026 the press reported that the UGC is developing a national policy on AI in higher education; its chair has suggested "AI disclosure statements" [S, Naya Diganta; not verified on ugc.gov.bd].

### 4.5 Hosting student data on a VPS: practical implications

- **Bangladeshi students:**
  - under‑18s need **guardian consent** unless an exemption clearly applies;
  - keystroke-derived biometrics would be **sensitive** data;
  - offshore hosting must meet s.29(3)–(4), where regulations are pending;
  - erasure propagation to backups (s.14(6)) needs crypto-shredding.
- **EU/UK students on a server in or accessed from Bangladesh:** a GDPR/UK GDPR restricted transfer (2.10).
- The Authority can issue binding directions (s.28) and the Government can issue directions (s.38). Consider this in the DPIA and the transfer impact assessment.

---

## 5. Ethics and fairness

**Critiques of surveillance in education**
- Philosophical analyses of proctoring identify harms to privacy, autonomy, fairness and trust (Coghlan, Miller and Paterson, *Philosophy & Technology* 34 (2021) 1581–1606) [S].
- In *Ogletree v. Cleveland State University* (N.D. Ohio, Aug 2022), a pre-exam room scan was held an unreasonable search under the Fourth Amendment [S].
- The Netherlands Institute for Human Rights (Oct 2023, *Pocornie v VU*, oordeel 2023‑111) found no discrimination on the facts but criticised the complaint handling. The case illustrates how hard algorithmic bias is to prove [S].
- CNIL rejects automatic behavioural analysis in proctoring [S].

**Psychological safety when prompts are graded (analysis; no study found).** Grading prompts risks the following:
- Chilling help-seeking. Students may avoid asking "naive" questions.
- Performative prompting.
- Penalising exploratory failure.

Mitigations:
- grade against a published rubric focused on decisions, not prompt "quality";
- keep a private scratch space;
- let students annotate their log;
- make exploratory use explicitly non-penalised in formative work.

The DfE standards' anti-manipulation and emotional-development standards support these mitigations [P].

**Consent and transparency for minors.**
- Children must be told, in age-appropriate language, that they are monitored (Children's Code std 11 via DfE [P]) and that they are talking to AI (AI Act Art. 50(1), (5) [P]).
- Guardian consent is required in Bangladesh (s.9) [P].
- UNESCO suggests age 13+ for classroom GenAI [S].

**Fairness across language and access.**
- Seven GPT detectors misclassified over 61% of TOEFL essays by non-native writers as AI-generated (Liang et al., *Patterns*, 14 Jul 2023) [S]. JCQ notes that detector accuracy varies with "English language competency" [P]. Dynodoc's no-statistical-detection stance is well founded.
- Deterministic origin marks can still be unfair:
  - **input-method editors** (Bengali, Chinese, etc.) commit text in bursts;
  - **dictation, word prediction and screen-reader workflows** used by disabled students may look like "pasted" or "imported".
- Such input must be recognised or declarable as legitimate. See Equality Act 2010 reasonable adjustments and JCQ access arrangements [K], and US Section 504/ADA and Title VI [K].
- Unequal access to paid AI tools outside the system is another problem. A log that shows only in-system use can make students with better outside tools look "cleaner".

**Right to contest.** GDPR Art. 22(3) [K]; UK Art. 22C [S]; AI Act Art. 86 [P]; JCQ 5.33 [P]; Bangladesh draft AI Policy [P]; PDP Act s.12(3) (mark data as disputed) [P].

**Academic integrity bodies.**
- ICAI *Fundamental Values*, 3rd ed. (2021): honesty, trust, fairness, respect, responsibility, courage [S].
- QAA *Reconsidering assessment for the ChatGPT era* (2023) favours assessment redesign over detection [S].
- OIA *Good Practice Framework* [P]: the student "must be told in writing ... which academic offence(s) ... and why" and "given any available supporting evidence" (para 50). Reasons must be "sufficiently detailed" (para 69). "Analysing the reports produced by detection software requires academic judgment" (para 48).

**Harm-reducing design commitments (recommended):**
1. no keystroke biometrics;
2. a private scratch space;
3. student access to their full log;
4. human-only grading;
5. purpose limitation;
6. retention limits;
7. no model training on student data;
8. no emotion or engagement inference;
9. deterministic, documented origin rules;
10. an explicit "limits of observation" statement on every report;
11. a student annotation and dispute channel;
12. exam mode that *blocks* rather than *watches*;
13. visible indicators when logging is on.

---

## 6. Evidence and tamper-evidence

**Hash chains** are *tamper-evident*, not tamper-proof. The operator that controls the database could rewrite an entire chain. Credibility rises when chain heads are **anchored outside Dynodoc's control**:
- RFC 3161 time-stamp tokens;
- periodic signed checkpoints exported to the school;
- witnessing by a third party.

**RFC 3161** (Aug 2001; updated by RFC 5816) [P]: a TSA "supports assertions of proof that a datum existed before a particular time". It time-stamps a hash only and does not examine the data. This is privacy-friendly.

**EU eIDAS**
- Reg. 910/2014 Art. 41 [K]: a non-qualified electronic time stamp "shall not be denied legal effect and admissibility as evidence ... solely" because it is electronic or not qualified. A **qualified** time stamp "shall enjoy the presumption of the accuracy of the date and the time ... and the integrity of the data".
- eIDAS 2 (Reg. 2024/1183) Art. 45k: electronic ledgers cannot be denied legal effect for being electronic. Data in **qualified electronic ledgers** "enjoy the presumption of their unique and accurate sequential chronological ordering and of their integrity" (Art. 45l requirements) [S via mirror]. Using a qualified TSA for chain-head anchoring is a low-cost upgrade.

**Transparency logs** (Sigstore **Rekor**; Certificate Transparency RFC 9162):
- Rekor is an append-only Merkle log with inclusion proofs and signed checkpoints [P].
- It is **public**, so never log personal data or unsalted hashes of student content.
- Hashes can still be personal data (EDPB blockchain guidelines [S]).
- Publish only aggregate Merkle roots.

**C2PA.** Spec 2.3 (Dec 2025) added manifests embedded in unstructured text via Unicode variation selectors. Spec 2.4 (Apr 2026) added HTML and structured text [S]. This is useful for Art. 50(2) machine-readable marking of exported text. Invisible characters can disrupt screen readers and copy-paste, so test accessibility.

**Courts**
- US FRE 902(13) and (14) (since 1 Dec 2017) allow self-authentication of records generated by an accurate electronic process, and of data copied and verified by hash, with a qualified certification [P].
- In England and Wales, the common-law presumption that computers operate correctly is under review after the Post Office Horizon scandal (MoJ call for evidence, 21 Jan 2025) [S].
- Bangladesh Cyber Security Act 2026 s.43 admits forensic evidence [P]. The Evidence Act 1872's digital-evidence amendments were not checked **[U]**.

**Misconduct procedures generally require** [P, JCQ, OIA]:
- a written allegation;
- disclosure of all supporting evidence;
- time and an opportunity to respond;
- representation;
- the balance-of-probabilities standard, with the burden on the institution;
- reasoned decisions;
- proportionate sanctions;
- an appeal.

**What a fair, usable provenance report must contain**
1. Scope statement: only activity inside Dynodoc, in named documents and sessions. It does **not** show whether outside tools (phones, other devices) were used.
2. Definitions of each origin mark and the exact rule that assigns it, with known failure modes (IME, dictation, accessibility tools, offline sync, clipboard from the student's own notes).
3. A complete, chronological event list available to the student, not just a summary.
4. Gaps and anomalies: offline periods, clock source, sync conflicts, software version.
5. An integrity section: chain verification result, anchor timestamps, and how an independent party can re-verify.
6. The student's own annotations and explanation (JCQ's "brief explanation").
7. No inference of intent, emotion or "likelihood of AI"; no statistical detector outputs.
8. Who accessed the record and when.
9. The human decision-maker's reasons, recorded separately from the system output.
10. The appeal route and retention period.

---

## 7. Compliance design checklist for Dynodoc

| # | Commitment | Primary source(s) |
|---|---|---|
| 1 | Keep origin marking **rule-based and documented**. No ML "AI-likelihood" classifier | Commission AI-system definition guidelines [P]; JCQ on detector limits [P]; Liang 2023 [S] |
| 2 | **Never** collect inter-key timing or typing rhythm. Coarse event timestamps only | GDPR Art. 4(14), 9 [P/K]; ICO biometric guidance [P]; PDP Act s.2(14), s.7 [P] |
| 3 | **No emotion, attention or engagement inference** from typing, cursor or webcam, in schools or workplaces | AI Act Art. 5(1)(f) [P]; Commission prohibited-practices guidelines [P via mirror] |
| 4 | No automated scoring of prompts or AI use. If a summariser exists, limit it to **extracting and ordering events**. No evaluative labels. Document an Art. 6(4) assessment and register | AI Act Annex III 3(b), Art. 6(3)–(4), 49(2) [P]; draft high-risk guidelines [S/U] |
| 5 | The teacher decides and writes the reasons. The UI counters automation bias (no default grades, no red flags) | JCQ Gen Regs 5.3(b) [P]; Ofqual [P]; AI Act Art. 14(4)(b) [K]; GDPR Art. 22 / *SCHUFA* [S] |
| 6 | **Exam mode blocks** the assistant (deterministic) and never "watches" behaviour | AI Act Annex III 3(d) [P]; CNIL 2023‑058 [S] |
| 7 | Tell students the assistant is AI. Show a visible "recording on" indicator in age-appropriate language | AI Act Art. 50(1), (5) [P]; Children's Code std 11 via DfE [P] |
| 8 | Mark substantive AI outputs in exports with machine-readable, signed provenance metadata (C2PA text or equivalent). Exempt grammar, spelling and translation edits | AI Act Art. 50(2) [P]; Art. 50 Guidelines, Code of Practice [S] |
| 9 | Give students **full self-access** and export of their log, and a way to annotate or dispute entries | GDPR Art. 15, *Nowak* [K/S]; PDP Act ss.11–12 [P]; FERPA inspect/review [K] |
| 10 | **Private scratch space.** Chain stores hashes only. Content is revealed only by the student's choice or under a documented misconduct process | Data minimisation GDPR Art. 5(1)(c) [K]; Children's Code std 8 [K] |
| 11 | **Purpose limitation**: no model training, product analytics or marketing on student content | DfE standards (privacy, IP) [P]; Datatilsynet 2024 [P]; SOPIPA [P]; AB 1159 [P/S]; ICO edtech [P] |
| 12 | **Crypto-shredding architecture** (salted hashes on-chain, keyed content off-chain) so erasure and rectification propagate, including to backups | GDPR Arts. 16–17 [K]; EDPB Guidelines 02/2025 [S]; PDP Act s.14(6) [P] |
| 13 | Written **retention schedule** tied to the assessment and appeal window, then delete or aggregate | GDPR Art. 5(1)(e) [K]; COPPA 2025 retention policy [S]; PDP Act s.18 [P]; JCQ retention rules [K] |
| 14 | **DPIA** before launch, reviewed each release. Child user testing | GDPR Art. 35, WP248 [K/S]; DfE standards [P]; Children's Code std 2 [K] |
| 15 | Processor contracts (Art. 28 / FERPA "direct control" / COPPA school authorisation) that forbid own-purpose use | GDPR Art. 28 [K]; 34 CFR 99.31 [P]; FTC COPPA FAQ [S] |
| 16 | Guardian-consent flow for Bangladesh under‑18s. Parental-consent route for any direct-to-consumer under‑13 (US) or under Art. 8 ages (EU/UK) | PDP Act s.9 [P]; COPPA [S]; GDPR Art. 8 [K] |
| 17 | Host EU/UK school data in the EU/UK. SCCs/IDTA plus TIA for any access from Bangladesh | GDPR Ch. V [K]; DfE standards [P] |
| 18 | Anchor chain heads with **RFC 3161 / eIDAS-qualified** time stamps. Export signed checkpoints to the school | RFC 3161 [P]; eIDAS Art. 41, 45k–45l [K/S] |
| 19 | Never write personal data (or unsalted per-student hashes) to public transparency logs | Rekor docs [P]; EDPB blockchain guidelines [S] |
| 20 | Every report includes a **"limits of observation"** statement and the rule definitions | JCQ 5.33 [P]; OIA paras 48–50, 69 [P] |
| 21 | Treat IME, dictation and assistive technology input as legitimate. Provide declarations and adjustments | Equality Act 2010 [K]; JCQ access arrangements [K]; OCR Nov 2024 [S] |
| 22 | Safeguarding-alert and complaints channel (DSL contact at setup) | DfE standards (monitoring, governance) [P] |
| 23 | Implement DfE "offloading" metrics as **event counts visible to the student**, not inferred traits | DfE standards (cognitive development) [P]; PDP draft AI Policy on child profiling [P] |
| 24 | If any component is high-risk and sold into the EU, appoint an **EU authorised representative** and plan conformity by 2 Dec 2027 | AI Act Arts. 2(1)(c), 22, 43 [K]; Reg. 2026/1744 [S] |
| 25 | AI-literacy materials for teachers and students (provider and deployer duty) | AI Act Art. 4 [K; S on Omnibus wording] |
| 26 | Agentic product: record agent identity, scope and prompts. Limit personal data in prompts. Inform employees about per-edit attribution | Cyber Security Act 2026 s.17 [P]; GDPR Arts. 5, 13 [K] |
| 27 | Keep a Bangladesh legal register and track PDP regulations/SOPs (transfer "suitable technology", retention periods, breach timelines) and the commencement notice for ss.31–35 | PDP Act ss.1(3), 18, 20, 27, 29 [P] |
| 28 | Marketing and intended-purpose copy reviewed by counsel. Avoid "AI grades AI use" | AI Act Art. 3(12), 25(1)(c) [K] |

---

## 8. Jurisdiction matrix

| Obligation | EU | UK | US | Bangladesh |
|---|---|---|---|---|
| AI classification of an education evaluation tool | AI Act Annex III 3(b)/(d) high-risk from **2 Dec 2027** [S] | No AI statute. Ofqual: no AI sole marker [P]; JCQ: not sole/primary marker [P] | No federal AI statute. State laws (e.g. Colorado SB 26‑189 from 1 Jan 2027) [S] | None in force. Draft AI Policy (risk-based; child-profiling ban) [P draft] |
| Emotion inference | Prohibited in education since 2 Feb 2025 [P] | DfE standards: avoid emotion recognition [S]; ICO fairness | No federal ban | Not addressed [U] |
| AI content transparency | Art. 50 since 2 Aug 2026 [P/S] | None statutory. JCQ acknowledgement duty [P] | None federal | Draft policy only |
| Lawful basis (schools) | Public task / legal obligation; consent weak [K] | Same; ICO view: legitimate interests for GenAI products [P via DfE] | FERPA school-official exception [P]; COPPA school authorisation (guidance) [S] | **Consent-centric**; guardian consent under 18 [P] |
| Child age thresholds | Art. 8: 13–16 by state [K] | 13 [K] | COPPA under 13 [K] | Under 18 [P] |
| DPIA / impact assessment | Mandatory here (Art. 35) [K] | Mandatory; Children's Code [K]; DfE [P] | Not federal; state contracts vary | SOP to be prescribed (s.27(2)(ঞ)) [P] |
| Automated decisions | Art. 22; *SCHUFA* [S] | Arts. 22A–22D from 5 Feb 2026 [S] | FERPA silent; state ADMT laws emerging [S] | s.13(1)(গ), s.13(6), s.11(3) [P] |
| Keystroke biometrics | Biometric if identifying; Art. 9 if used to identify; AI Act biometric regardless [P] | Same; ICO lists keystroke recognition [P] | BIPA list excludes keystrokes [K]; COPPA open list [U] | Behavioural biometric = sensitive [P] |
| Access to own record | Art. 15; *Nowak* [S] | Same; exam-script exemption (Sch. 2 para 25) [K] | FERPA inspect/review, 45 days [K] | s.11 incl. logic of automated decisions [P] |
| Erasure vs append-only | Art. 17; EDPB 02/2025 [S] | Art. 17 UK GDPR [K] | COPPA deletion; SOPIPA deletion on request [P] | s.13(2), **s.14(6) incl. backups** [P] |
| Retention | Storage limitation [K] | Same | COPPA written retention policy [S] | s.18 prescribed periods; s.19 5‑year register [P] |
| Cross-border / localisation | Ch. V transfers [K] | IDTA/Addendum [K] | None federal | s.29(3)–(4) conditions; no explicit localisation in the Act [P] |
| Breach notification | 72 h (Art. 33) [K] | 72 h [K] | State laws; COPPA security programme [S] | s.20, time by regulation [P] |
| Use of student data to train AI | Needs its own lawful basis; Datatilsynet [P] | DfE: not without permission [P] | CA AB 1159 prohibits [P/S] | Draft AI Policy would prohibit [P draft] |
| Misconduct evidence standard | National/institutional | JCQ: balance of probabilities; see all evidence [P]; OIA [P] | Due process at public institutions [K] | No specific rule found [U] |

---

## 9. Claims from the owner's pasted chat

| Claim | Verdict | Evidence and nuance |
|---|---|---|
| "Annex III treats AI systems that evaluate learning outcomes or monitor prohibited behaviour during tests as high-risk" | **Verified** | Annex III 3(b) and 3(d) [P]. Nuances: (1) the Art. 6(3) derogations can take a system out unless it profiles; (2) application is now **2 Dec 2027**, not 2 Aug 2026 (Reg. 2026/1744) [S]; (3) only *AI systems* are covered, and a deterministic ledger likely is not one |
| "Bangladesh's PDP Act 2026" | **Verified** | ব্যক্তিগত উপাত্ত সুরক্ষা আইন, ২০২৬, Act No. 63 of 2026, dated 10 Apr 2026, passed 9 Apr 2026. It repeals PDPO 2025 and the 2026 Amendment Ordinance [P]. Not all sections are in force (ss.23, 31–35 deferred) [P] |
| "Typing rhythm is close to biometric" | **Verified, but understated** | Keystroke dynamics *is* a behavioural biometric (ICO [P]; AI Act Recital 15 [P]; PDP Act s.2(14) [P]). Under the GDPR it is biometric data when technically processed to identify or verify, and special category when used for identification. Under the AI Act, emotion inference from keystrokes in education is **prohibited** [P] |
| (Implicit) "The Digital Omnibus delays high-risk obligations" | **Verified (AI part only)** | Reg. 2026/1744 in force 27 Jul 2026 [S]. The GDPR part is still pending [S] |
| (Implicit) "AI Act requires AI-written text to be labelled" | **Partly wrong** | Providers must *machine-mark* outputs (50(2)), except standard editing. Deployers must disclose only for **public-interest publications**, with an editorial-responsibility exception (50(4)) [P]. Student coursework disclosure comes from assessment rules (JCQ), not Art. 50 |

---

## 10. Open questions for a lawyer

1. **AI Act classification.** Does an event-extracting summariser (no evaluative labels) qualify under Art. 6(3)(a) or (d)? Does it "profile" under GDPR Art. 4(4)? What does draft ¶215 of the high-risk guidelines actually say, and did the final guidelines change it?
2. Is a rule-based origin classifier (paste, import, AI insertion detected by API source) outside the Art. 3(1) "AI system" definition?
3. How does Art. 111 (as amended) treat a system first placed on the EU market between now and 2 Dec 2027? Should Dynodoc plan conformity regardless?
4. Does Dynodoc's EU authorised-representative obligation arise if only the in-editor assistant (Art. 50) is offered, and not a high-risk system?
5. Are signed provenance metadata plus C2PA text manifests sufficient for Art. 50(2) under the final Code of Practice for text? Does the "short text" carve-out apply to inline completions?
6. **GDPR Art. 22.** At what level of teacher reliance does any system-generated summary become a "decision" (*SCHUFA*)? What UI safeguards suffice?
7. Does the crypto-shredding design satisfy Art. 17 (EU), UK GDPR, and Bangladesh s.14(6) "system-wide propagation"? Must backups be re-encrypted or destroyed?
8. **UK DPA 2018 Sch. 2 para 25.** Are in-exam prompts "information recorded by candidates during an exam" and so exempt from subject access?
9. Is Dynodoc a processor or a controller for the education product in each jurisdiction, and does the ICO Children's Code apply?
10. **Bangladesh.**
    - (a) Can schools or universities rely on s.24(1)(ঝ) ("archival ... education") or another exemption instead of guardian consent?
    - (b) What are the Schedule classification criteria under s.29, and has any "suitable technology" regulation been issued?
    - (c) Commencement date of ss.31–35.
    - (d) Scope of the s.19 five-year register versus minimisation.
    - (e) Does s.1(2)(গ) catch a foreign-hosted VPS serving Bangladeshi students?
11. Would the draft Bangladesh AI Policy's "behavioural tracking of children" ban, if adopted, cover a student-visible provenance log used for grading?
12. **US.** Are Dynodoc logs "education records"? What does California AB 1159's definition of "covered information" include (prompts, AI outputs, derived metrics), and when does it take effect? Do Colorado SB 26‑189 and Oklahoma SB 1734 reach Dynodoc as a vendor?
13. What is the exact COPPA 2025 definition of "biometric identifiers", and does it capture keystroke dynamics?
14. **Evidence.** Would anchoring to an eIDAS-qualified TSA, or operating a qualified electronic ledger, give Dynodoc reports a legal presumption useful in EU misconduct appeals? What certification would US FRE 902(13) require?
15. Liability allocation. Should contracts disclaim use of reports as the sole basis for sanctions and require institutional human review?
16. Employee monitoring in the agentic product: per-edit attribution of staff in the EU/UK (consultation duties, works councils), and Bangladesh labour law.

---

## 11. Sources

| Title | Issuer | Date | URL | Status |
|---|---|---|---|---|
| Regulation (EU) 2024/1689 (AI Act), Annex III, Arts. 3, 5, 6, 26, 27, 50, 86, 113; Recital 15 | EU (OJ); text via FLI mirror | 13 Jun 2024 (OJ 12 Jul 2024) | https://eur-lex.europa.eu/eli/reg/2024/1689/oj ; https://artificialintelligenceact.eu/annex/3/ | [P] mirror |
| Regulation (EU) 2026/1744 (AI Omnibus) | EU | OJ 24 Jul 2026; in force 27 Jul 2026 | https://eur-lex.europa.eu/eli/reg/2026/1744/oj/eng | [S] (EUR-Lex not retrievable) |
| Digital Omnibus on AI enters into force | K&L Gates / Cyber Law Watch | 31 Jul 2026 | https://www.cyberlawwatch.com/2026/07/31/eu-digital-omnibus-on-ai-enters-into-force/ | [S] |
| The Digital Omnibus on AI enters into force today | Lewis Silkin | 27 Jul 2026 | https://www.lewissilkin.com/insights/2026/07/27/the-digital-omnibus-on-ai-enters-into-force-today-102nedo | [S] |
| Digital AI Omnibus delays key deadlines | Cooley | 2026 | https://cdp.cooley.com/digital-ai-omnibus-delays-key-deadlines-introduces-new-rules/ | [S] |
| Education and vocational training (Annex III examples) | EC AI Act Service Desk | accessed 30 Sep 2026 | https://ai-act-service-desk.ec.europa.eu/en/education-and-vocational-training | [P] |
| Draft Commission guidelines on classification of high-risk AI systems | European Commission | 19 May 2026 | https://digital-strategy.ec.europa.eu/en/library/draft-commission-guidelines-classification-high-risk-ai-systems | [P] landing; content [S] |
| EU AI Act Annex III draft guidelines: what changed | Modulos | 2026 | https://www.modulos.ai/blog/eu-ai-act-annex-iii-draft-guidelines-what-changed/ | [S] |
| Draft AI Act guidelines on high-risk classification | Osborne Clarke | 2026 | https://www.osborneclarke.com/insights/european-commission-publishes-draft-ai-act-guidelines-high-risk-classification-and | [S] |
| Guidelines on transparency obligations (Art. 50) | European Commission | 20 Jul 2026 | https://digital-strategy.ec.europa.eu/en/library/guidelines-transparency-obligations-providers-and-deployers-ai-systems | [P] landing |
| Code of Practice on Transparency of AI-generated Content | European Commission | 10 Jun 2026 | https://digital-strategy.ec.europa.eu/en/policies/code-practice-ai-generated-content | [P] landing |
| EU finalises transparency rules for AI-generated content | Paul Weiss | Jul 2026 | https://www.paulweiss.com/insights/client-memos/eu-finalises-transparency-rules-for-ai-generated-content | [S] |
| Commission guidelines on prohibited AI practices (analysis incl. verbatim extracts) | EC; Iren, Noldus, Brouwer | 4 Feb 2025 / 5 Feb 2025 | https://www.aigl.blog/content/files/2025/04/AI-Act---Guidelines-on-Prohibited-Artificial-Intelligence--AI--Practices--An-Analysis-for-the-Emotion-Recognition-Field.pdf | [P] extracts |
| Commission guidelines on the definition of an AI system | European Commission | Feb 2025 | https://ai-act-service-desk.ec.europa.eu/sites/default/files/2025-08/commission_guidelines_on_the_definition_of_an_artificial_intelligence_system_established_by_regulation_eu_20241689_ai_actenglish_nf2skcqfrtjdfggjavcodopcwz4_112455.PDF | [S] |
| Regulation (EU) 2016/679 (GDPR), Art. 4 | EU; text via gdpr-info.eu | 27 Apr 2016 | https://eur-lex.europa.eu/eli/reg/2016/679/oj ; https://gdpr-info.eu/art-4-gdpr/ | [P] mirror |
| Guidelines on DPIA (WP248 rev.01) | Art. 29 WP / EDPB | 4 Oct 2017 | https://ec.europa.eu/newsroom/article29/items/611236 | [K/S] |
| Guidelines 02/2025 on blockchain (draft) | EDPB | Apr 2025 (final reported Jul 2026) | https://www.edpb.europa.eu/system/files/2025-04/edpb_guidelines_202502_blockchain_en.pdf | [S] |
| EDPB finalises guidelines on GDPR and blockchain | Slaughter and May | 21 Jul 2026 | https://thelens.slaughterandmay.com/post/102ndb2/edpb-finalises-guidelines-on-gdpr-and-blockchain | [S] |
| Case C‑634/21 *SCHUFA* | CJEU | 7 Dec 2023 | https://curia.europa.eu/juris/liste.jsf?num=C-634/21 | [S] |
| Case C‑434/16 *Nowak* | CJEU | 20 Dec 2017 | https://curia.europa.eu/juris/liste.jsf?num=C-434/16 | [S] |
| Biometric data guidance: biometric recognition | ICO | Feb 2024 (under review) | https://ico.org.uk/for-organisations/uk-gdpr-guidance-and-resources/lawful-basis/biometric-data-guidance-biometric-recognition/ | [P] |
| The Children's code and edtech | ICO | 30 May 2023 | https://ico.org.uk/for-organisations/uk-gdpr-guidance-and-resources/childrens-information/childrens-code-guidance-and-resources/the-children-s-code-and-education-technologies-edtech/ | [P] |
| ICO statement on *Edtech examined* report | ICO | 24 Jun 2026 | https://ico.org.uk/about-the-ico/media-centre/news-and-blogs/2026/06/ico-statement-on-edtech-examined-report/ | [P] |
| Data (Use and Access) Act 2025; Commencement No. 6 Regs (SI 2026/82) | UK Parliament / legislation.gov.uk | RA 19 Jun 2025; SI in force 5 Feb 2026 | https://www.legislation.gov.uk/ukpga/2025/18 ; https://www.legislation.gov.uk/uksi/2026/82/regulation/2/made | [S] |
| Délibération n° 2023‑058 (remote exam proctoring) | CNIL | 8 Jun 2023 | https://www.legifrance.gouv.fr/jorf/id/JORFTEXT000048038547 | [S] |
| Bocconi / Respondus fine | Garante (via enforcement tracker) | 21 Sep 2021 | https://www.enforcementtracker.com/ETid-876 | [S] |
| Datatilsynet giver påbud i Chromebook-sag | Datatilsynet (DK) | 30 Jan 2024 | https://www.datatilsynet.dk/afgoerelser/afgoerelser/2024/jan/datatilsynet-giver-paabud-i-chromebook-sag | [P] landing |
| AI Use in Assessments | JCQ | 30 Apr 2025 | https://www.jcq.org.uk/wp-content/uploads/sites/2/2025/09/AI-Use-in-Assessments_Apr25_FINAL.pdf | [P] |
| General Regulations for Approved Centres 2026‑27 | JCQ | eff. 1 Sep 2026 | https://www.jcq.org.uk/wp-content/uploads/sites/2/2026/08/Gen_regs_approved_centres_26-27_FINAL.pdf | [P] |
| Suspected Malpractice: Policies and Procedures 2025‑26 (rev. 1) | JCQ | 1 Sep 2025; republished 19 May 2026 | https://www.jcq.org.uk/knowledge-hub/suspected-malpractice-policies-and-procedures/ | [P] (copy via centre mirror) |
| Ofqual's approach to regulating the use of AI | Ofqual | updated 16 Jul 2026 | https://www.gov.uk/government/publications/ofquals-approach-to-regulating-the-use-of-artificial-intelligence-in-the-qualifications-sector | [P] |
| Generative AI in education (policy paper) | DfE | updated 12 Aug 2025 | https://www.gov.uk/government/publications/generative-artificial-intelligence-in-education | [P] |
| Generative AI: product safety standards | DfE | 22 Jan 2025; updated 19 Jan 2026 | https://www.gov.uk/government/publications/generative-ai-product-safety-expectations | [P] (archived PDF text) |
| 34 CFR 99.31 | US ED (via Cornell LII) | current | https://www.law.cornell.edu/cfr/text/34/99.31 | [P] mirror |
| Children's Online Privacy Protection Rule (final amendments) | FTC / Federal Register | 22 Apr 2025 | https://www.federalregister.gov/documents/2025/04/22/2025-05904/childrens-online-privacy-protection-rule | [S] |
| Cal. Bus. & Prof. Code §22584 (SOPIPA) | California Legislature | current | https://leginfo.legislature.ca.gov/faces/codes_displaySection.xhtml?lawCode=BPC&sectionNum=22584 | [P] |
| AB 1159 bill status | California Legislature | approved 10 Sep 2026 | https://leginfo.legislature.ca.gov/faces/billStatusClient.xhtml?bill_id=202520260AB1159 | [P] status |
| Legislative tracker: 2026 state AI in education bills | FutureEd | 13 Jul 2026 | https://www.future-ed.org/legislative-tracker-2026-state-ai-in-education-bills/ | [S] |
| Dear Colleague Letter on AI and federal grant funds | US Dept. of Education | 22 Jul 2025 | https://www.ed.gov/media/document/opepd-ai-dear-colleague-letter-7222025-110427.pdf | [S] |
| Avoiding the Discriminatory Use of AI | US ED OCR | 20 Nov 2024 | https://www.ed.gov/media/document/avoiding-discriminatory-use-of-artificial-intelligence-112274.pdf | [S] |
| Executive Order 14277 | The White House | 23 Apr 2025 | https://www.govinfo.gov/app/details/DCPD-202500511/ | [S] |
| ব্যক্তিগত উপাত্ত সুরক্ষা আইন, ২০২৬ (Act No. 63 of 2026) | Govt. of Bangladesh, Legislative Division | 10 Apr 2026 | http://bdlaws.minlaw.gov.bd/act-details-1692.html | [P] |
| জাতীয় উপাত্ত ব্যবস্থাপনা আইন, ২০২৬ (Act No. 80 of 2026) | Govt. of Bangladesh | 10 Apr 2026 | http://bdlaws.minlaw.gov.bd/act-details-1709.html | [P] |
| সাইবার সুরক্ষা আইন, ২০২৬ (Act No. 81 of 2026) | Govt. of Bangladesh | 10 Apr 2026 | http://bdlaws.minlaw.gov.bd/act-details-1710.html | [P] |
| Parliament passes 31 bills related to IG-era ordinances | The Financial Express (BD) | 9 Apr 2026 | https://thefinancialexpress.com.bd/home/parliament-passes-31-bills-related-to-ig-era-ordinances | [S] |
| 91 bills passed to convert Yunus-era ordinances | The Business Standard | 10 Apr 2026 | https://www.tbsnews.net/bangladesh/91-bills-passed-convert-yunus-era-ordinances-laws-1408216 | [S] |
| CCIA Views on Bangladesh's PDPO | CCIA | Apr 2026 | https://ccianet.org/wp-content/uploads/2026/04/CCIA-Views-on-Bangladeshs-Personal-Data-Protection-Ordinance.pdf | [S] |
| Cyber Security Amendment Bill 2026 passed in JS | BSS | 30 Jun 2026 | https://www.bssnews.net/js-session/401310 | [S] |
| Bangladesh National AI Policy 2026–2030, Draft v2 | ICT Division | 9 Feb 2026 | https://aipolicy.gov.bd/docs/national-ai-policy-bangladesh-2026-2030-draft-v2.0.pdf | [P] draft |
| ICT minister: Bangladesh drafting human-centric AI policy | Dhaka Tribune | Jul 2026 | https://www.dhakatribune.com/bangladesh/415452/ict-minister-bangladesh-drafting-human-centric | [S] (snippet) |
| UGC higher-education AI policy report | Daily Naya Diganta | 23 Apr 2026 | https://dailynayadiganta.com/bangladesh/education-campus/q6B7hFjJmGRn | [S] |
| GPT detectors are biased against non-native English writers | Liang et al., *Patterns* | 14 Jul 2023 | https://www.cell.com/patterns/fulltext/S2666-3899(23)00130-7 | [S] |
| Good Proctor or "Big Brother"? | Coghlan, Miller, Paterson, *Philos. & Tech.* | 2021 | https://link.springer.com/article/10.1007/s13347-021-00476-1 | [S] |
| *Ogletree v. Cleveland State University* | N.D. Ohio | 22 Aug 2022 | https://caselaw.findlaw.com/court/us-dis-crt-n-d-ohi-eas-div/2109381.html | [S] |
| Oordeel 2023‑111 (VU / Proctorio) | College voor de Rechten van de Mens | 17 Oct 2023 | https://oordelen.mensenrechten.nl/oordeel/2023-111 | [S] |
| Fundamental Values of Academic Integrity, 3rd ed. | ICAI | 2021 | https://academicintegrity.org/aws/ICAI/asset_manager/get_file/911282?ver=1 | [S] |
| Reconsidering assessment for the ChatGPT era | QAA | 2023 | https://www.qaa.ac.uk/docs/qaa/members/reconsidering-assessment-for-the-chat-gpt-era.pdf | [S] |
| Good Practice Framework: academic disciplinary procedures | OIAHE | current | https://www.oiahe.org.uk/resources-and-publications/good-practice-framework/disciplinary-procedures/part-a-academic-disciplinary-procedures/ | [P] |
| Guidance for generative AI in education and research | UNESCO | Sep 2023 | https://www.unesco.org/en/articles/guidance-generative-ai-education-and-research | [S] |
| Guidance on AI and Children 3.0 | UNICEF Innocenti | 2025 | https://www.unicef.org/innocenti/media/11991/file/UNICEF-Innocenti-Guidance-on-AI-and-Children-3-2025.pdf | [S] |
| Guidelines on the ethical use of AI and data in teaching and learning (2026 ed.) | European Commission (DG EAC) | 2026 | https://op.europa.eu/en/publication-detail/-/publication/f692aa0b-17a7-11f1-8870-01aa75ed71a1/ | [S] |
| RFC 3161 Time-Stamp Protocol | IETF | Aug 2001 | https://www.rfc-editor.org/rfc/rfc3161 | [P] |
| Rekor overview | Sigstore | current | https://docs.sigstore.dev/logging/overview/ | [P] |
| C2PA Technical Specification 2.4 | C2PA | Apr 2026 | https://spec.c2pa.org/specifications/specifications/2.4/specs/C2PA_Specification.html | [S] |
| Regulation (EU) 910/2014 (eIDAS) Art. 41; Regulation (EU) 2024/1183 Arts. 45k–45l | EU | 2014 / 2024 | https://eur-lex.europa.eu/eli/reg/2014/910/oj ; https://www.european-digital-identity-regulation.com/Article_45_(Regulation_EU_2024_1183).html | [K]/[S] |
| Federal Rules of Evidence 902(13)–(14) | US Courts (via Cornell LII) | eff. 1 Dec 2017 | https://www.law.cornell.edu/rules/fre/rule_902 | [P] mirror |
| Use of evidence generated by software in criminal proceedings (call for evidence) | UK Ministry of Justice | 21 Jan 2025 | https://assets.publishing.service.gov.uk/media/67892f5a93d4eae3088bd324/use-evidence-generated-software-criminal-proceedings.pdf | [S] |
| Digital Omnibus (GDPR part), legislative train | European Parliament | 2026 | https://www.europarl.europa.eu/legislative-train/theme-a-new-plan-for-europe-s-sustainable-prosperity-and-competitiveness/file-digital-package | [S] |

*Items marked [K] are standard statutory provisions. The analyst could not re-fetch them in this session, so counsel should check them against the official consolidated text.*
