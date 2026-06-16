# Dynodoc — Feasibility, Market, and Strategy Assessment

*Working title for the concept: a self‑hostable, AI‑native "Research/Study IDE" that takes a study description and produces a methodologically sound, ODK/Kobo‑compatible questionnaire, with collaborative review, mock‑data simulation, validity checks, and direct deployment to data collection.*

---

## 1. Executive summary

The idea sits in a real and surprisingly empty intersection of three established markets: (a) the ODK/Kobo/SurveyCTO field‑research data collection ecosystem, (b) the academic/clinical EDC ecosystem dominated by REDCap, and (c) the new wave of AI‑native survey and research tools. None of the incumbents in (a) and (b) have built an opinionated, agentic design environment, and none of the AI‑native tools in (c) target the field/development/health research workflow that *requires* offline‑capable, enumerator‑mediated collection in XLSForm or REDCap data dictionary formats. There is genuine product whitespace.

The core build is technically tractable. ODK Central exposes a fully public REST API; pyxform is a mature open‑source library that converts spreadsheet definitions into ODK XForm XML; XLSForm is itself a structured tabular spec, which LLMs handle well. KoboToolbox is open source, so self‑hosted deployments are practical. The AI cost picture has improved dramatically — a full questionnaire design session on Claude Sonnet 4.6 at current rates costs well under a dollar — and prompt caching plus batch processing cut that further. The hard parts are not the codegen; the hard parts are methodological validity, statistical simulation, and earning trust from a conservative buyer base.

Commercially, the path that makes sense is to start narrow — anchor on the ICT4D/global health research segment where Verne already has network and credibility — and expand outward into adjacent academic and M&E markets. Pricing should be tiered with a permanent free tier (to compete with Kobo's gravitational pull) and self‑hostable institutional plans (to compete on data sovereignty, which is increasingly a hard requirement).

The biggest risks are (1) AI hallucination in *methodological* advice, where errors are far more consequential than in marketing surveys, (2) the "free Kobo" anchoring problem, (3) and the mismatch between LMIC research budgets and SaaS economics. The biggest leverage points are (1) the data‑dictionary auto‑generation feature (huge time saver, easy to demo), (2) self‑hostability (a moat against pure‑SaaS competitors), and (3) the IDE framing itself, which signals seriousness to a research audience that distrusts marketing‑survey aesthetics.

Recommendation: build a focused MVP around the AI design + XLSForm export + collaborative review loop. Defer the simulation engine. Pilot with two or three friendly Bangladeshi/South Asian research institutions before broader launch.

---

## 2. Problem validation

The five pain points laid out in the brief are accurate and align with what is documented in the public literature and community discussion. A short audit:

**Pain point 1 — questionnaire validity.** Most field research teams are not staffed with psychometricians or survey methodologists. Validated instruments exist (PHQ‑9, GAD‑7, EORTC, WHODAS, MICS, DHS modules, etc.) but selecting, adapting, and combining them correctly is non‑trivial. Translation back‑translation, cultural adaptation, and item‑level review are routinely skipped under deadline pressure.

**Pain point 2 — three‑tier silo (Word → ODK/Kobo programmer → Stata/R).** This is the dominant workflow in development research and is the single biggest source of avoidable cost. SurveyCTO and Kobo both accept XLSForm uploads but neither has a Word‑to‑XLSForm bridge, so most teams either keep the Word version as the "real" version (and let it drift from the deployed form) or pay a programmer to translate. Anecdotally, a freelance ODK programmer in South Asia charges roughly USD 300–1,500 per study, and the loop time is days, not minutes.

**Pain point 3 — manual review.** Reviewing a 60–100 item questionnaire in Word with track changes is genuinely awful. Comments scatter, versions diverge, the "final" file accumulates filename suffixes (`_v3_FINAL_FINAL_reviewed_AB.docx`). There is no per‑item discussion thread, no resolution state, no audit trail.

**Pain point 4 — AI use is ad‑hoc.** Researchers already paste questionnaires into ChatGPT for review, and a January 2026 academic paper documents using ChatGPT to generate REDCap data dictionaries from PDFs of validated instruments. This is being done manually, in copy‑paste loops, with no integration to the rest of the workflow. The demand is observable; the integration is missing.

**Pain point 5 — no simulation.** Power calculations, factor‑structure checks, and item response theory pre‑testing are typically done in separate tools (G*Power, Stata, R packages like `lavaan` or `mirt`) after the questionnaire is "done." Late discovery of issues is common. Mock data generation, when it happens, is hand‑coded by a research assistant.

These pains compound. A team with stronger statistical expertise wastes less time per cycle, but the average team — especially in NGO M&E and LMIC research — is not that team. That asymmetry is exactly what an opinionated tool can flatten.

---

## 3. Market analysis

### 3.1 Sizing

There is no clean public TAM number for "field research questionnaire tooling," so the bottom‑up estimate has to be assembled.

**KoboToolbox** is the dominant free tool, used by over 32,000 nonprofit organizations worldwide, collecting 20 million surveys per month, including multiple United Nations agencies, the World Food Programme, International Rescue Committee, IFRC, Save the Children, and Médecins Sans Frontières. The user base is large but heavily non‑paying.

**SurveyCTO** is the paid leader in the same niche. Public pricing places its Basic plan at around USD 250/month (5,000 submissions) up to Advanced at USD 700/month, with custom enterprise pricing above. SurveyCTO does not publish customer counts, but it lists Stanford, Georgetown, IPA, and similar institutions as users.

**REDCap** runs on a consortium model with thousands of institutional installations, primarily in academic medical centres. It is free for non‑profits but paid via institutional support. The active install base is in the tens of thousands of researchers globally.

**Qualtrics, SurveyMonkey, Typeform, Jotform, Pollfish** dominate the much larger general/business survey market — billions of dollars combined — but they don't compete in offline‑capable field research. They are adjacent, not direct.

A defensible serviceable addressable market (SAM) sketch for a focused product:

- *Core field research segment*: research and M&E teams that today use Kobo or ODK or SurveyCTO. Plausible 100k–300k active users globally, with maybe 20k–40k institutionally‑purchased seats among NGOs, academic groups, and consulting firms.
- *Adjacent academic segment*: REDCap users who would value AI‑assisted data dictionary generation. Tens of thousands of researchers, but with a longer institutional sales cycle.
- *Aspirational expansion*: government statistics offices, large RCT operators (J‑PAL, IPA, IDinsight, 60 Decibels), public health surveillance programs.

If we assume a penetration target of even 1–2% of the core segment at average ARPU of USD 600/year (significantly below SurveyCTO's USD 2,400+/year floor), that's USD 1.2M–4M ARR within a focused niche before any expansion. This is meaningful for a Bangladesh‑based bootstrapped startup but small for a VC‑backed Western competitor — which is part of why this niche has been left open.

### 3.2 Customer segments and willingness to pay

| Segment | Pain intensity | Willingness to pay | Sales cycle |
|---|---|---|---|
| Independent academic researcher | Medium | Low (often grant‑constrained) | Short |
| University research group / lab | High | Medium (institutional license) | Medium |
| Research‑heavy NGO (BRAC, Pratham, IRC) | High | Medium–High | Medium |
| Global health institute (icddr,b, KEMRI, IHI) | Very High | Medium | Long (procurement) |
| Consulting firm doing M&E (IDinsight, 60 Decibels, Genesis Analytics) | Very High | High | Medium |
| UN agency / multilateral | High | High but procurement‑heavy | Very long |
| Government statistics office | High | Variable | Very long |
| Pharma / clinical research | Medium | Very High | Very long, regulatory |

The fastest money is in research‑heavy NGOs and M&E consulting firms — they have the pain, the budget, and short procurement. The deepest money is in university and clinical research, but those sales cycles can be 9–18 months.

### 3.3 Geographic considerations

Bangladesh is a *good* place to launch this. The ICT4D and field research density is high — icddr,b, BRAC, BIDS, BIGD, IPA Bangladesh, J‑PAL South Asia, and dozens of international NGO field offices all run continuous data collection. Verne's existing network into icddr,b alone is a meaningful first‑customer pipeline. Other strong launch geographies on the same logic: Kenya (KEMRI, APHRC, Nairobi tech ecosystem), India (IFMR, J‑PAL India, 3ie), South Africa (HSRC, SAMRC), and Uganda (IDI, MUII).

The Western academic market is reachable but slower; the right entry point there is partnerships rather than direct sales.

---

## 4. Competitive landscape

### 4.1 Direct competitors

There is, as of April 2026, no product that combines AI‑native questionnaire design, ODK/Kobo‑compatible export, collaborative review, and methodology validation in a single environment. The closest things are research prototypes:

- *REDCapQB* (an academic browser‑based questionnaire builder for REDCap, published 2024) — a WYSIWYG layer over REDCap. No AI.
- Manual ChatGPT + REDCap workflows documented in Medium posts and academic write‑ups — confirms demand, not a product.

### 4.2 Adjacent competitors (worth knowing in detail)

**SurveyCTO.** The dominant paid field‑research tool. Strong in offline reliability, audio audit, GPS fencing, enumerator quality control. Pricing starts at USD 198/month for the Basic plan. They have not built any AI design layer. They are the most likely to build a competitive feature if your product gains traction; expect a 12–24 month response window. Their moat is operational reliability and reputation, not design tooling.

**KoboToolbox.** Free, open‑source, GNU‑licensed, hosted by HHI/IFRC/UNHCR/OCHA. The "free Kobo" anchor is the dominant pricing pressure on this entire market. Limited product evolution velocity since they are donor‑funded. They will not build AI design tooling in any reasonable timeframe.

**ODK Central.** The open‑source server underneath everything. It exposes a public REST API; anything that can be done in the user interface can be done directly via the API, and forms can be uploaded as XLSForm directly. This is exactly the integration surface to build against.

**REDCap.** Clinical/academic standard. A secure web application for building and managing online surveys and databases, supporting compliance with 21 CFR Part 11, FISMA, HIPAA, and GDPR; specifically geared to support online and offline data capture for research studies and operations. The consortium model means thousands of institutional installs. AI integration is largely DIY at the moment.

**Outset.ai.** Well‑funded ($51M raised, Series B in late 2025) AI‑moderated interview platform. Different paradigm — Outset *replaces the human enumerator* with an AI conducting voice/text interviews. This works for online qualitative research at scale, but it does not solve field data collection in low‑bandwidth, low‑literacy, enumerator‑mediated contexts. There is no overlap in core use case, but Outset's existence is useful evidence: investors are paying attention to AI‑native research tooling.

**Qualtrics, SurveyMonkey, Typeform, Jotform.** Big general survey players, all now with AI question generation. They do not export XLSForm, do not work offline‑first, and their AI features are oriented around marketing/CX surveys rather than research methodology. Different market.

**FeedbackRobot, BlockSurvey, Pollfish, Fillout, Responsly, etc.** Smaller AI survey generators in the consumer/SMB space. Same exclusion as above.

### 4.3 Why hasn't this been built?

This is worth explicit attention because "obvious gap" usually has an unobvious explanation.

- *Domain knowledge gap.* Building a methodologically credible questionnaire IDE requires both LLM/agent engineering AND survey methodology AND psychometrics AND XLSForm specifics. Founders with all four are rare. Most AI survey startups have founders from B2B SaaS, not field research.
- *Market gravity to consumer/B2B SaaS.* The field research segment has lower ARPU than enterprise CX. AI‑native VCs chase the larger market.
- *Open‑source cultural defaults.* The ODK/Kobo community is heavily open‑source. A pure‑proprietary product faces resistance; this is a reason to design for hybrid open core / self‑host.
- *Outputs are an XML/Excel format, not a UI.* This is a strange thing to wrap a product around if you don't already understand why XLSForm is the universal standard.

Verne, by contrast, has direct experience with icddr,b's research workflow, AI/IT engineering capacity, and the LMIC context. The combination is rare; that's the actual moat.

---

## 5. Technical feasibility

### 5.1 Architecture sketch

The system breaks naturally into five components:

1. **Study Specification Layer** — natural language description of the study (research question, design, population, sample size, key constructs, validated instruments to include or adapt). This is the "prompt" but structured as a persistent project document, not a single message.
2. **Agent Orchestration Layer** — a small set of specialized agents:
   - *Methodologist agent*: maps study description to design choices (cross‑sectional vs longitudinal, cluster RCT, panel, etc.) and recommends validated instruments.
   - *Form‑builder agent*: emits XLSForm rows (question type, name, label, choices, constraints, relevance, calculations).
   - *Validity reviewer agent*: linguistic clarity, ordering, leading questions, double‑barrelled items, response set balance, scale reliability heuristics.
   - *Translator agent*: handles target‑language renditions and back‑translation prompts.
   - *Codebook/data dictionary agent*: emits Stata `.do`, R `.R`, and SPSS `.sps` import scripts plus a human‑readable codebook.
3. **Form Compilation Layer** — pyxform converts the XLSForm to ODK XForm XML; alternatively emits a REDCap CSV data dictionary for the REDCap path. Both compilers are open source.
4. **Collaboration Layer** — per‑item comments, suggestion mode, change resolution, branchable drafts, simple version history. Not real Git, but Git‑shaped.
5. **Deployment Layer** — direct push to ODK Central or KoboToolbox via their REST APIs; for REDCap, project XML import via REDCap API.

### 5.2 Why ODK Central is the right anchor

ODK Central is the right primary integration target rather than KoboToolbox specifically, because:

- ODK Central is the ODK server. It manages user accounts and permissions, forms and submissions, longitudinal data records, and allows data collection clients like ODK Collect to connect to it for form download and submission upload, with the goal of being straightforward to install, easy to use, and extensible via REST, OpenRosa, and OData programmatic APIs.
- XLSForm is a standard for designing forms in Excel; once your form has been designed, you can upload the XLSForm directly to your ODK Central server.
- KoboToolbox is built on top of ODK and accepts the same XLSForm format. So integrating with ODK XLSForm by default gives you Kobo compatibility for free.

### 5.3 The XLSForm → XForm pipeline

The codegen path is well-trodden. pyxform is a Python library that simplifies writing forms for ODK Collect and Enketo by converting spreadsheets that follow the XLSForm standard into ODK XForms. It has been actively maintained since 2018, supports Python 3.10–3.13, and is used by ODK Central itself. This means:

- The AI does not have to emit XForm XML directly (which is gnarly).
- The AI emits XLSForm rows (a flat tabular format that LLMs handle reliably).
- pyxform compiles those rows to spec‑correct XForm.
- Validation happens at compile time; bad output gets caught before reaching the user.

This is a major de‑risking. The hardest "exact format" part is solved by an existing library.

### 5.4 Where the AI will struggle

Honest accounting: there are things the AI will *not* do well at first.

- **Power calculations and sample size justification.** These need to be done by deterministic statistical functions (`pwr` in R, `power` commands in Stata), not by an LLM. The AI's role is to gather inputs and explain results, not compute.
- **Factor structure and reliability of new scales.** Cronbach's α, McDonald's ω, EFA/CFA fit indices, IRT parameter estimates — these need real data, which means simulation. The AI can run them once you have data; it cannot intuit them.
- **Cultural and linguistic appropriateness.** Especially for Bangla, Swahili, Hausa, low‑resource languages. The AI will produce *plausible* translations that may still need expert review.
- **Edge cases in skip logic for very long forms.** Complex relevance and calculation chains over 200+ items can produce subtle bugs. pyxform compilation catches structural errors but not semantic ones.

These are reasons for a "validity reviewer" loop that includes human approval gates, not reasons not to build.

### 5.5 Self-hostability

Self‑hosting is a meaningful differentiator and is achievable. The main components are all containerizable:

- ODK Central: Docker‑native, well‑documented self‑host path.
- pyxform: pip‑installable, no external services.
- The AI orchestration layer can run against (a) a hosted Anthropic/OpenAI/Google endpoint, (b) a self‑hosted open‑weights model (Llama 3.1 70B Instruct, Qwen 2.5 72B, DeepSeek‑V3, etc.), or (c) a hybrid where the methodology and codegen agents use the strong API model and the routine reformatting agents use a local Haiku‑class or open‑weights model. The hybrid path is the right default.

For institutions with data residency requirements (Bangladesh's PDPO, EU GDPR, Kenya's Data Protection Act, Indian DPDP), the self‑hosted option with open‑weights inference is a serious selling point. This connects naturally to ClearSight on the Verne portfolio side.

### 5.6 Build-cost estimate (engineering)

For a credible MVP suitable for paid pilots:

- Core agent orchestration + XLSForm emission: 2 senior engineers × 3 months
- Web UI (Next.js + collaborative editing): 1 frontend engineer × 4 months
- ODK Central / Kobo / REDCap integration: 1 engineer × 2 months
- Methodology curation (validated instrument library, prompt engineering, eval harness): 1 methodologist × 3 months part‑time
- DevOps, infra, observability: 1 engineer × 2 months part‑time

Roughly 10–12 person‑months for a minimum viable product. At Bangladeshi engineering rates including overhead, that is on the order of USD 35,000–60,000 in salary cost for the MVP. At Western rates this is USD 250,000+. The cost asymmetry favors building from Dhaka.

For a V1 that includes the simulation engine, mock data generation, and statistical pre‑testing, add another 6–9 person‑months.

---

## 6. Cost analysis (operating)

### 6.1 LLM cost per design session

Assume an average questionnaire design session involves:

- 20–40 LLM exchanges across the agents
- Average ~6,000 input tokens (study description + accumulated context + tool definitions) and ~2,500 output tokens per call
- Total per session: ~240k input tokens, ~100k output tokens

On Claude Sonnet 4.6 at $3.00 input / $15.00 output per million tokens, that is roughly USD 0.72 input + USD 1.50 output ≈ **USD 2.22 per design session** at full retail pricing.

With prompt caching providing 90% off repeat inputs and batch processing providing 50% off everything, and given that the system prompt + XLSForm spec + validated‑instrument knowledge base (~30–50k tokens) is the same every session and can be cached, the realistic cost is closer to **USD 0.50–0.80 per design session**.

For the routine agents (formatting, translation pass, codebook generation), Haiku 4.5 at $1/$5 per million tokens drops the cost further. A well‑architected hybrid setup (Sonnet for methodology and validation, Haiku for codebook and translation, Opus only for the hard edge cases) lands around **USD 0.30–0.60 per session**.

This is a comfortable unit economic. Even at a USD 19/month researcher tier, a user running 10 questionnaire designs a month costs USD 3–6 in LLM, leaving healthy margin.

### 6.2 Infrastructure costs

- Hosted ODK Central instance per organization (small): USD 20–50/month on a basic VPS.
- Postgres + Redis + object storage: USD 50–200/month for early scale.
- LLM API: variable; budget 15–25% of revenue.
- CDN, monitoring, errors: USD 50–150/month.

A managed multi‑tenant SaaS deployment serving the first 100–500 organizations runs roughly USD 1,500–4,000/month in fixed infra plus variable LLM. At USD 600 average ARPU/year per org, 100 orgs = USD 60K ARR vs ~USD 25–30K cost — viable.

### 6.3 Risks to the cost picture

- LLM provider price changes. Mitigation: hybrid open‑weights + commercial; abstraction layer over providers; aggressive caching.
- A user running unbounded simulation jobs. Mitigation: simulation as separately metered usage, like a build minutes model.
- Self‑hosted enterprise customers expecting on‑prem inference for sensitive data. Mitigation: bundle a tested open‑weights stack (Llama 3.1 70B served via vLLM on a single 80GB GPU is tractable) with a deployment guide. Quote those engagements separately.

---

## 7. User journey

### 7.1 The "first 30 minutes" experience

A research lead — say, a public health postdoc — arrives with a draft protocol. They:

1. Create a new project. Paste or upload the protocol document.
2. Talk through the design with the methodologist agent. The agent surfaces choices: cross‑sectional vs longitudinal, cluster vs individual sampling, validated instruments to include (PHQ‑9? K10? WHODAS?). It flags unstated assumptions ("you mentioned heat stress; do you want to include WBGT measurements or self‑reported thermal comfort?").
3. Approve the design choices. The form‑builder agent emits an initial XLSForm — say, 80 items with appropriate types, choice lists, relevance conditions, and constraints.
4. Preview the form in a browser (Enketo render of the compiled XForm). Click through it like a respondent would.
5. The validity reviewer agent runs a pass: "Item 23 is double‑barrelled; consider splitting"; "Items 41–48 have unbalanced response options"; "the proposed scale has only 3 reverse‑coded items, consider adding two more for acquiescence control."
6. Invite a colleague. They read the form, leave per‑item comments. The colleague can `@agent fix` to apply suggested edits, or resolve the comment manually.
7. Push the published version to ODK Central or Kobo with one click. Download the codebook + Stata/R import scripts.

Time: ~1 hour for a real questionnaire that might otherwise take 1–2 weeks of back‑and‑forth.

### 7.2 The collaborative review loop

Comments are first‑class objects. Each comment has a state (open, resolved, won't fix), an author, optionally a thread, and optionally an `@agent` directive. When a reviewer clicks "apply with AI," the agent reads the comment context (the item, surrounding items, the reviewer's intent) and proposes a diff. The lead approves or rejects. This is the part that meaningfully replaces the Word‑track‑changes ritual.

### 7.3 The simulation layer (Phase 2)

Mock data generation is technically straightforward — sample from item distributions, respect skip logic, produce N records. Where it gets interesting:

- *Distributional realism*: condition on plausible covariates (age × education × region) so the mock data is not just uniform random.
- *Pre‑test analysis*: run the proposed analysis pipeline against mock data and surface issues — multicollinearity, near‑empty cells in cross‑tabs, items with too little variance.
- *Reliability checks on validated instruments*: simulate underlying latent traits and verify that the planned scale recovers them.

This is the feature that makes the product feel like an "IDE" rather than a "form generator." It is also the feature with the highest hallucination risk and should ship after the basics.

---

## 8. Go-to-market strategy

### 8.1 Beachhead

Three customer profiles to land first, in priority order:

1. **A flagship Bangladeshi research institution** — icddr,b is the obvious target given Verne's existing relationships. A pilot inside one or two units (climate change & health, MNCH, infectious diseases) generates a credible reference customer. Free or heavily discounted in exchange for case study rights and design feedback.
2. **One South Asian impact-evaluation firm** — IPA Bangladesh, J‑PAL South Asia, IDinsight India, or Genesis Analytics Africa. These are heavy users of SurveyCTO/Kobo and have engineering‑literate teams who will be early product collaborators.
3. **One global health donor‑funded program** — REACH, JSI, FHI 360 country office, or similar. Slower to land but unlocks pipeline.

### 8.2 Channel strategy

- **Conferences and methodology workshops.** ICTD, GHTC, AERA, APHA, IHSN. Present at the methodology track, not the product track. The audience is researcher‑first.
- **Open‑source presence.** Maintain a public, properly engineered XLSForm validator or methodology checker as a free CLI tool that drives awareness. Some of the agent prompt templates can be open. The product is the orchestration + UI + collaboration; the components can be open without giving away the moat.
- **Academic partnerships.** Co‑authored methodology papers — "AI‑assisted instrument design: a case study from rural Bangladesh" — establish credibility much faster than ads.
- **Direct outbound to research project leads via grant databases.** USAID, FCDO, IDRC, BMGF — every funded grant has a named PI with email. This is a *very* targetable list; warm intros via mutual collaborators land at high rates.
- **Content for the technical buyer.** Long-form posts on prompt engineering for survey methodology, ODK API patterns, data dictionary automation. The audience is small but high‑intent.

### 8.3 Pricing tiers (proposal)

| Tier | Audience | Price | What's included |
|---|---|---|---|
| Community | Independent researchers, students | Free | 3 active projects, basic AI design, ODK/Kobo export, public sharing |
| Researcher | Solo academics, grad students | USD 19/month | Unlimited projects, full AI agents, private sharing |
| Team | Research labs, small NGO units | USD 99/month per workspace (≤10 users) | Above + collaboration, audit trail, custom instruments |
| Institution | Universities, large NGOs | USD 500–2,000/month | Above + SSO, branding, priority support, simulation |
| Self‑hosted / Sovereign | Govt, large research bodies, donor‑mandated | USD 8,000–25,000/year | On‑prem deployment, open‑weights inference, source‑available |

The free tier is non‑negotiable. KoboToolbox is free; competing without a free tier is suicide. The free tier limit is on projects and AI calls per month, not on features — research workflows benefit from the full feature set even at small scale.

The self‑hosted tier is where Bangladesh / South Asia / Africa institutional buyers will land because it dovetails with PDPO, NDGO, GDPR, Kenyan DPA, etc. This is also where Verne's Verne‑side integrator capabilities create natural cross‑sell into ClearSight.

### 8.4 Partnership strategy

- **ODK community.** Non‑adversarial partnership. The right framing is "we're a tool that produces compliant XLSForms; we make the ODK ecosystem more accessible." Active participation in ODK forum and conferences.
- **Open Data Kit / Get ODK Cloud**: explore reseller/integration arrangement.
- **REDCap consortium.** Longer term — the consortium is conservative and partnership requires institutional sponsorship.
- **Anthropic / OpenAI / Google**: research use cases tend to qualify for API credit programs. Apply early.
- **Cloud providers (AWS, GCP, Oracle)**: AWS in particular runs activate programs for African / South Asian startups. Useful for compute cost.

### 8.5 Unit economics target

For a team-tier customer at USD 99/month:

- LLM cost (estimated 80–120 sessions/month): USD 30–60
- Infra share: USD 5–10
- Gross margin: ~USD 30–60/month, ~30–60%
- Payback for a 1.5x sales-loaded customer: 2–4 months

For institutional and self‑hosted customers, gross margin is higher (60–80%) because LLM cost is either capped or borne by the customer.

---

## 9. Risks, pitfalls, and possible-impossible analysis

### 9.1 Risk matrix (top items)

| Risk | Likelihood | Severity | Mitigation |
|---|---|---|---|
| AI hallucinates methodologically incorrect advice | High | High | Mandatory human approval gates; conservative defaults; eval harness with known-good cases; surface confidence per recommendation |
| KoboToolbox-anchored buyers won't pay | Medium | High | Strong free tier; differentiated paid features focused on time savings the free tools can't match |
| Liability if simulated power calculation is wrong | Medium | High | Clear "advisory not authoritative" framing in product UI; rely on deterministic stats engines for actual computation |
| LLM provider API price spike or deprecation | Medium | Medium | Provider abstraction layer; open-weights fallback; quarterly cost review |
| Large incumbent (SurveyCTO, Qualtrics) ships AI design feature | Medium | High | Move fast; build moat in research methodology depth + self-hostability + LMIC focus |
| Conservative academic buyers reject AI-generated instruments | Medium | Medium | "AI-assisted, expert-validated" framing; instrument library uses validated, citable sources |
| Data residency/privacy concerns block adoption | High | Medium | Self-hosted tier from Day 1; published privacy and data flow documentation |
| Translation quality for low-resource languages | High | Medium | Human-in-the-loop translation review; partner with local linguists for key markets |
| Verne team bandwidth — core team is stretched across other priorities | High | High | Hire dedicated product owner; phase the build; resist scope creep |
| Cultural fit issues — research community distrusts SaaS UX | Medium | Medium | "Dynodoc" aesthetic, not "SaaS dashboard"; thoughtful defaults; methodology-first language |

### 9.2 Pitfall: the "just wraps ChatGPT" objection

Researchers will say "I can already do this with ChatGPT" — and they're partially right. The honest answer:

- ChatGPT will produce a Word document; this product produces a deployable XLSForm pushed to ODK Central with a generated codebook in three seconds.
- ChatGPT loses context across sessions; this product has persistent project state, version history, and per‑item review threads.
- ChatGPT cannot validate against the XLSForm specification; this product compiles via pyxform and surfaces real errors.
- ChatGPT cannot run a simulated dataset against your proposed analysis pipeline; this product (eventually) can.

If the demo doesn't make this difference visceral within five minutes, the product will lose the "wraps ChatGPT" comparison. This is a UX problem as much as a feature problem.

### 9.3 Pitfall: scope creep

The most dangerous failure mode is trying to ship the design IDE *and* the simulation engine *and* the data-collection layer *and* the analysis layer in V1. Resist. The order should be:

1. AI design + XLSForm export + ODK Central push + auto codebook (MVP)
2. Collaborative review with comments and AI‑applied edits (V0.2)
3. Translation and cultural adaptation pass (V0.3)
4. Mock data generation + simple pre‑test analysis (V1)
5. Full simulation, factor analysis, IRT diagnostics (V2)
6. Direct analysis layer (V3 — possibly never; let people use Stata/R)

### 9.4 Possible-impossible analysis (feature by feature)

| Feature | Verdict | Notes |
|---|---|---|
| Generate complete XLSForm from study description | Possible, achievable in 8–12 weeks | LLMs handle this format reliably; pyxform validates output |
| Direct push to ODK Central / KoboToolbox | Possible, straightforward | Public REST APIs; well-documented |
| Direct push to REDCap | Possible, moderate | Project XML format; REDCap API requires per-project tokens |
| Auto-generate Stata / R / SPSS codebook + import script | Possible, easy | Templated generation; this is one of the highest-leverage features |
| Per-item collaborative comments with AI-applied edits | Possible, ~2 months engineering | Standard collaborative editing patterns; AI edit application is the novel piece |
| Mock data generation respecting skip logic and constraints | Possible, ~1 month | Deterministic; AI helps with realistic distributions |
| Pre-test analysis on mock data | Possible, ~2 months | Wraps R/Python statistics; LLM interprets results |
| Validated-instrument library with proper citations and licensing | Possible but slow | Substantial curation effort; partnership with academic sources helps |
| Multilingual generation including back-translation | Possible, quality varies | Strong for major languages; weaker for Bangla / low-resource |
| Power calculation + sample size with methodologically rigorous defaults | Possible | Use deterministic R packages (`pwr`, `simr`); LLM only for input gathering and interpretation |
| Item-response-theory pre-testing | Possible, slow | Real psychometric work; needs domain expert in product team |
| Real-time questionnaire performance prediction (completion rate, abandonment) | Hard | Requires substantial training data; defer or partner |
| Replace ODK Collect mobile app entirely | Avoid | ODK Collect is the universal standard; build on top of it |
| Replace Stata/R in the analysis step | Impossible / inadvisable | Complementary, not substitute |
| Real-time fraud detection on enumerator submissions | Possible but separate product | This is where SurveyCTO is strong; a separate v2 module if you go this direction |

The aggressive read: ~80% of the feature surface in the brief is achievable in 6–9 months. The simulation/IRT layer is harder and should be staged.

---

## 10. What would change my mind on doing this

A few signals to watch for, that would weaken the case:

- SurveyCTO ships an AI design feature in the next 6 months. (Possible but their cadence suggests longer.)
- KoboToolbox announces an OpenAI/Anthropic-funded AI feature partnership. (Watch for it; would compress the differentiation window.)
- A well-funded YC-style AI survey startup pivots to ICT4D / global health. (Currently no signs.)
- LLM costs spike 2–3× without a corresponding revenue lift. (Trend is the opposite.)
- A major IRB or research ethics body issues guidance restricting AI use in instrument design. (Possible; would slow but not kill the market.)

Conversely, signals that would *strengthen* the case:

- A Bangladeshi or South Asian research org publicly commits to using AI in instrument design.
- A donor like BMGF, IDRC, or FCDO issues a digital research infrastructure call.
- A SurveyCTO price increase or KoboToolbox feature freeze.
- Continued publication of academic papers documenting ad-hoc ChatGPT-for-REDCap-or-Kobo workflows (already happening).

---

## 11. Recommendation

This concept is worth pursuing, with a tightly scoped MVP and a deliberately narrow first market.

**Build, in order:**

1. The AI design + XLSForm export + ODK Central push + auto-codebook pipeline. This is the value-dense MVP and could be in pilot in 12–16 weeks.
2. The collaborative review layer with AI-applied edits.
3. The self-hosted institutional deployment path, in parallel with #2 if there's hiring capacity.

**Sell, in order:**

1. icddr,b unit pilot (free, in exchange for case study and design feedback).
2. Two paid design partners from South Asian / African research firms or NGOs.
3. Productized self-serve launch with the Researcher and Team tiers, after the design partners are stable.

**Defer:**

1. Full simulation engine until V1.
2. Multi-language low-resource support beyond English + Bangla + one African language.
3. Direct analysis tooling.

**Watch:**

1. SurveyCTO and KoboToolbox feature roadmaps.
2. The pyxform library's evolution (changes affect compatibility).
3. Anthropic / OpenAI / Google pricing changes and open-weights model performance for the routine agents.

This sits naturally inside the Verne portfolio — it complements ClearSight (privacy/compliance) on the data side, leverages Verne's existing icddr,b relationships for the beachhead, and uses the same AI engineering skill the rest of the company is developing. It is the kind of product that is hard to build well but, once built, would be very hard for a Western consumer-survey startup to rebuild without the LMIC research network you have.

---

*Prepared for Verne. The numbers in this report are best-effort sketches based on publicly available information as of April 2026. They should be re-validated before commitments are made.*
