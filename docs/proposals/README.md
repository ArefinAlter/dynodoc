# Proposals and research for future releases

Reviewed 6 October 2026: nothing in this folder is implemented. Projects phase 1
is the next build; each proposal records the outstanding choices. Production was
reset on 30 September (accounts kept — [operations record](../operations/2026-09-30-document-reset.md)).
That is a dated observation: recheck current data and preserve it in new migrations.

| Document | What it is | Status |
| --- | --- | --- |
| [PROJECTS-WORKSPACE-HANDOFF.md](PROJECTS-WORKSPACE-HANDOFF.md) | GitHub-style Projects: files, change requests, drafts, compare, activity, settings, watch — pages, data model, API, tests, phases | Approved direction; build next |
| [AGENT-PROVENANCE-PROPOSAL.md](AGENT-PROVENANCE-PROPOSAL.md) | AI agents edit documents through an MCP server; sessions recorded; character-level origin (human / AI / pasted / imported) that can never drift towards "human"; tamper-evident sealing; C2PA/PROV exports | Proposal |
| [AI-ASSESSMENT-PROPOSAL.md](AI-ASSESSMENT-PROPOSAL.md) | Students use AI inside Dynodoc; teachers grade *how* (decisions, judgement) with evidence-linked rubrics; assignment modes; privacy and legal requirements; Bangladesh fit; pilot | Proposal |
| [AI-COLLABORATION-RUBRIC.md](AI-COLLABORATION-RUBRIC.md) | Five-criterion rubric with evidence rules tied to what the record contains | Draft for pilot |

Research behind them (sourced, dated 30 September 2026; secondary and unverified items
are labelled):

| Research | Covers |
| --- | --- |
| [../research/AGENTIC-DOCUMENTS-RESEARCH.md](../research/AGENTIC-DOCUMENTS-RESEARCH.md) | Atlas (verified against its code), Entire, Git AI, Agent Trace, Cursor Blame, Copilot/Claude Code logs; MCP 2026-07-28, OpenTelemetry GenAI, PROV-O, C2PA 2.4, IPTC, SynthID; Word/Docs/Notion/ChatGPT/Claude editing; WikiWho, Peritext, Yjs; origin propagation rules |
| [../research/AI-IN-EDUCATION-RESEARCH.md](../research/AI-IN-EDUCATION-RESEARCH.md) | Learning-science evidence, AI Assessment Scale, two-lane assessment, JCQ/TEQSA/UNESCO, interaction-log research, competing products, detector bias, Bangladesh context, draft rubric |
| [../research/AI-GOVERNANCE-RESEARCH.md](../research/AI-GOVERNANCE-RESEARCH.md) | EU AI Act (Annex III, Art. 5, 6, 50; Omnibus dates), GDPR and children, UK/US education rules, Bangladesh PDP Act 2026 and Cyber Security Act 2026, ethics, evidence and tamper-evidence, compliance checklist, questions for a lawyer (not legal advice) |

Suggested order: Projects → agent-provenance prototype (origin projection, sessions,
no model needed) → MCP server → assessment pilot. The in-editor assistant and any LLM
summaries need a decision record amending [decision 000](../decisions/000-initial.md).
