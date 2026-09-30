# Agents that edit documents, with provenance — design proposal

Status: **proposal for a future release; nothing here is implemented.** Prepared
30 September 2026 at the owner's request after comparing Dynodoc with Atlas ("source
control for agents"). Evidence, sources and verification of the claims in the
owner's pasted chat: [research/AGENTIC-DOCUMENTS-RESEARCH.md](../research/AGENTIC-DOCUMENTS-RESEARCH.md).
Education use: [AI-ASSESSMENT-PROPOSAL.md](AI-ASSESSMENT-PROPOSAL.md). Legal notes:
[research/AI-GOVERNANCE-RESEARCH.md](../research/AI-GOVERNANCE-RESEARCH.md).

## 1. The idea

People increasingly produce documents with AI — chat assistants, office copilots, or
coding agents that generate Word/HTML through code. Afterwards nobody can say which
sentences a person wrote, which an AI wrote, what it was asked, and what the person
did with its suggestions.

Atlas (Apache-2.0 alpha, created May 2026) solves part of this for code: it records
agent sessions (prompts, messages, tool calls, reasoning, patches) and links a whole
session to a whole Git commit by *watching* refs move, re-linking through amends and
rebases only when the diff is byte-identical and "orphaning" otherwise. Entire, Git
AI, Agent Trace and Cursor Blame go further, to line level, but all key attribution
to commits and must recompute or guess after history rewrites, because Git stores
snapshots, not operations.

**Dynodoc stores operations.** Every edit is a semantic, attributed, hash-chained
event, and rich-text edits are exact per-character insert/delete/retain operations.
If agents edit *through* the engine, the origin of every character can be recorded
at the moment of the edit and computed exactly by replaying the log — no
reconciliation, no orphaning. As of September 2026 no mainstream editor (Word
Copilot Agent Mode, Google Docs Gemini, Notion agents, ChatGPT, Claude) keeps a
durable, span-level, tamper-evident record of AI origin that reviewers can see after
acceptance; Grammarly Authorship is the nearest precedent and was caught mislabelling
"humanized" AI text as typed by a person (fixed December 2025). This is a genuine gap.

## 2. Foundation already in Dynodoc

| Need | Existing mechanism |
| --- | --- |
| Tamper-evident history | Append-only events; `content_hash` covers type, target, canonical payload and actor; `chain_hash = SHA-256(content ‖ prev)`; `/documents/:id/verify` |
| Exact text operations | `RichTextPatched` v1: retain/insert/delete counted in Unicode scalars with interned text metadata; base-checked |
| Stable identity | ULID blocks that survive moves, merges, restores and file pushes |
| Human decisions on proposals | Suggestions, personal drafts, change requests, approvals, per-change comments, review rules, override audit |
| Actors | Every event has a hashed `actor_id` |
| Content matching | MinHash/LSH fingerprints (6.3 ms per 192k-word document, 0.02 ms per comparison) |

Missing: agents as actors, their sessions, span-level origin, and sealing of draft
history at merge (draft edits are append-only but only enter the chain when shared).

## 3. Principles

1. **Agents edit through the engine.** Documents generated elsewhere and imported are
   *unattested*, never "human".
2. **Mechanisms, not verdicts.** Labels describe how text arrived ("typed in Dynodoc",
   "inserted by assistant X in session S", "pasted from outside", "imported from a
   file"); no statistical AI detection anywhere in the product.
3. **Origin cannot drift towards "human".** Only a person typing *new* characters, or
   an explicit audited correction, makes text look more human. AI paraphrase
   ("humanize"), approval, merge, formatting, moves, restores, copy/paste and file
   round trips never do.
4. **Agents propose; people decide.** Agents act for a named person, write only to
   drafts, suggestions and change requests, and never merge. Accept, modify and reject
   are recorded human decisions under the document's review rules.
5. **Commit to what was recorded.** Session records are hashed and committed into the
   document chain at merge; external timestamps are optional.
6. **Minimise.** Operations and conversations, not keystroke timing, cursor paths or
   cameras. Redact secrets before storing transcripts.

## 4. Design

```mermaid
flowchart LR
  A[External agent\nClaude, Codex, Copilot…] -- MCP --> M[Dynodoc MCP server]
  B[In-editor assistant] --> API
  M --> API[engine-api]
  P[Person in the editor] --> API
  API --> D[(drafts / suggestions)]
  API --> E[(document event chain)]
  API --> S[(agent sessions\nturns, tool calls)]
  D -- merge seals drafts + session commitments --> E
  E --> O[origin projection\nper block, run-length]
  O --> R[reports, "who wrote this?"]
```

### 4.1 Agents and sessions

- `identity.kind` = `person` | `agent`; agent identities are per (principal, agent
  product) with `on_behalf_of`. Because `actor_id` is hashed, agent authorship becomes
  tamper-evident. UI: "Claude for Olivia".
- `agent_session` (append-only): ULID, document, principal, agent identity, OAuth
  client, *self-reported* model and tool version (stored as claims), `traceparent`,
  times, status.
- `agent_turn`, `agent_tool_call` (append-only, redacted before persistence): role,
  content, tool name, arguments, result digest. Transcripts compressed in PostgreSQL,
  capped like uploads. Vocabulary follows the OpenTelemetry GenAI conventions
  (`invoke_agent`, `execute_tool`, `gen_ai.conversation.id`), but the record lives in
  Dynodoc's append-only store, not in telemetry.
- Visibility: principal and document owner/managers by default; a person may read a
  transcript only if they can open every document it quotes.

### 4.2 Origin of text: a projection computed from operations

**RichTextPatch v2** adds an interned `origins` table next to `styles`, and an
optional `origin` index on `Insert` ops, plus optional `transformed_by_ai` ranges.
Omitted when empty, so canonical JSON — and every existing hash — is unchanged.

Per character, three axes:

| Axis | Values | Meaning |
| --- | --- | --- |
| `generated_by` | `human`, `ai`, `pasted`, `imported`, `unknown` | Who produced these characters; `ai` carries session and tool call |
| `transformed_by_ai` | flag | Human characters kept inside an AI rewrite (prevents keeping "the/and" as human after a rewrite) |
| `attestation` | `server`, `client`, `none` | How trustworthy the label is: agent wrote via API/MCP; browser reported typing/paste; imports and legacy text |

The **origin projection** (run-length intervals per block) is rebuilt from events,
cached in snapshots, permission-checked, and never overrides canonical history.
Display aggregates to words (a word is AI if any character is; "mixed" when origins
differ) and snaps to grapheme clusters so Bengali conjuncts are never split.

*Alternative considered and rejected:* storing origin as a mark inside the rich-text
content. It rides along with text automatically, but it splits formatting runs,
changes content hashes, must be stripped from every export, breaks the "unchanged"
comparison used when edited files are pushed back, needs the editor schema to keep an
unknown mark, and puts the label under the browser's control.

### 4.3 Propagation rules (server-enforced)

| Operation | Effect | Can it make text look more human? |
| --- | --- | --- |
| Person types | New characters `human`; neighbours unchanged (labels never expand) | — |
| Agent inserts/rewrites | New characters `ai` (+session, tool call, `server`); kept human characters inside the rewrite get `transformed_by_ai` | No |
| Delete | Text leaves the projection; origin kept in history for restore | No |
| Person replaces words inside AI text | Only retyped characters become `human`; the rest stays `ai`; words show "mixed" | Only retyped characters |
| AI paraphrase of AI text ("humanize") | Still `ai` | **No — laundering blocked** |
| Retyping identical text | Diff is `Retain`; stays `ai` | No |
| Formatting, block moves | Unchanged | No |
| Copy/cut and paste inside Dynodoc | Original origin restored when the text matches the document's own current or deleted text (or a server-signed clipboard slice) | No |
| Paste from outside | `pasted`, `client`; if it matches an earlier agent output (MinHash containment via `similarity`), `ai` with a "pasted" note | No |
| Restore a version | Original origins return | No |
| Merge / "keep team's" / "use theirs" | Characters keep the origin of the side they came from; the merger is recorded as responsible, not as author | No |
| Conflict text typed by a person | Typed characters `human` | Only typed characters |
| Approve, accept, merge | Adds review evidence; origin unchanged | No |
| Office round trip: unchanged aligned text | Keeps prior origin | No |
| Office round trip: changed text | `imported`, `none`; uploader responsible. Failed alignment "orphans" to `imported` (Atlas's rule: orphan, don't guess) | No |
| Non-generative spellcheck | `human` | — |
| LLM grammar/style fix | `ai` + `transformed_by_ai`; recorded as an "assistive edit" with edit distance | No |
| Administrative correction | Explicit compensating event with a reason, owner/admin only, in audit | Yes, only this way |

"Unknown" and "imported" are never displayed as human.

### 4.4 What is hashed (tamper evidence)

1. `actor_id` of every event (already) — agent identities make agent authorship
   tamper-evident.
2. Origin descriptors and `transformed_by_ai` ranges inside `RichTextPatched` payloads.
3. A new `ChangeRequestMerged` event: draft ID and base, **a Merkle root over the
   ordered draft edits** (sealing previously un-chained draft history), the agent
   sessions involved with **salted** transcript commitments (erasing transcript and
   salt leaves an unguessable hash, compatible with erasure receipts), a digest of the
   reviews and per-block decisions, the approvers counted under the rules, the override
   flag, and for imports the uploaded file's SHA-256.
4. Model identifiers are hashed but labelled "claimed".
5. **Erasure without breaking the chain (crypto-shredding):** transcripts and other
   personal content are stored encrypted with a per-session (or per-person) key and
   referenced from the chain only by salted hashes. Destroying the key and salt erases
   the content everywhere, including backups, as GDPR Art. 17 and Bangladesh's PDP Act
   2026 s.14(6) require, while the chain still verifies.
6. Time is not hashed today: add a payload timestamp and optional RFC 3161 anchoring of
   chain heads if "when" must stand up externally.

The chain proves the record was not altered; it does not prove a client's claim was
true — hence the attestation axis.

### 4.5 MCP server

The MCP specification changed on 2026-07-28: it is **stateless** (no
`Mcp-Session-Id`, no initialize handshake), sampling and roots are deprecated, and
server-to-client requests became multi-round-trip requests. Dynodoc therefore mints
its own handles. Support the 2025-11-25 handshake mode too while clients migrate.

| Tool | Effect |
| --- | --- |
| `begin_agent_session(document_id, purpose)` | Returns a server-minted session handle |
| `read_outline`, `read_nodes(ids, at?)` | Block IDs, text/JSON, base hashes (read-only) |
| `create_draft(session, name)` | Agent's working branch |
| `propose_text_edit(draft, node, base_hash, edits \| new_content, rationale)` | Base-checked v2 patch in the draft; idempotent per request; returns a diff preview |
| `insert_node`, `move_node`, `delete_node` | Draft only |
| `add_comment`, `suggest(node, detail)` | Discussion / `SuggestionProposed` |
| `submit_change_request(draft, note)` | Hands over to people |
| `ask_history(node, question)` | Deterministic retrieval of the events and turns behind a block |

There is **no tool that writes the team version**. Authorization: OAuth 2.1 with
Protected Resource Metadata (RFC 9728); scopes such as `documents:read`,
`drafts:write`, `comments:write`; the token identifies the person, the registered
client identifies the agent product. Existing document MCP servers are coarse (Notion:
find-and-replace; Google Docs: index-based batch updates, cannot create suggestions;
Word: create/read/comment only) — a block- and draft-aware server is differentiating.

### 4.6 Exports and interchange

- Optional **C2PA** manifests on DOCX/PDF exports (C2PA 2.3–2.4 support OOXML
  collection hashes, text regions such as "AI assistant edited content", and a
  `c2pa.ai-disclosure` assertion), mapping `ai` → IPTC `trainedAlgorithmicMedia`,
  mixed → `compositeWithTrainedAlgorithmicMedia`. Word discards manifests on edit, so a
  returning file can only prove it changed.
- A JSON-LD provenance manifest using PROV-O (entities: block revisions; activities:
  sessions, turns, merges; agents: people and software agents `actedOnBehalfOf`).
- Optional Agent Trace-style export (node ID + scalar ranges instead of line ranges).
- Text watermarks (e.g. SynthID) are applied by model providers and weaken under
  paraphrase and translation; Dynodoc may read a provider's signal, never rely on it.

### 4.7 Reports and "ask the history"

Surviving text by origin (per section, heat strip), sequence (timeline of sessions,
proposals and human edits), decisions (accepted / modified with edit distance /
rejected with reason), sessions (filterable by block). Every report states the capture
boundary. "Ask the history" is deterministic retrieval first; any LLM summary is
labelled and never used as evidence (and needs a new scope decision, see §7).

## 5. Atlas and peers → Dynodoc

| Concept | Code tools | Dynodoc |
| --- | --- | --- |
| Session record | Atlas SQLite; Entire refs; Claude Code JSONL | `agent_session`/`agent_turn`, append-only, committed at merge |
| Link work to history | Observe commits; patch-id re-linking; orphan on doubt | Written into the edit event; exact |
| Attribution granularity | Commit (Atlas, Entire trailer); line (Git AI, Agent Trace, Cursor) | Character, displayed by word |
| Surviving rewrites | Recompute notes; heuristics; orphan | Stable block IDs + operation replay; orphan only on Office round trip |
| Shared memory | Atlas semantic index | Document context resource (style guide, decisions, sources) |
| Chat with a checkpoint | Atlas | Ask the history, per block or version |

## 6. Other markets

Journals and funders requiring AI-use disclosure; legal and policy drafting (the owner's
government contacts); regulated documentation; newsrooms — EU AI Act Article 50(4)
exempts deployers from labelling AI text when it "has undergone a process of human
review or editorial control", and Dynodoc's approvals and review rules are exactly that
evidence (not legal advice).

## 7. Scope, risks, limits

- **Scope:** decision 000 excludes LLM features and ML services from the PoC.
  Recording external agents over MCP needs no model; an in-editor assistant and
  "ask the history" summaries need a new decision record.
- **Outside the capture boundary** (text generated elsewhere and typed or auto-typed
  in) looks like typing. Answer with product design and supervised settings, not
  detection.
- **Privacy and security:** transcripts can contain personal data and quotes of other
  documents; retention, redaction, access and crypto-shredded erasure are required.
  Keep an audit trail of agent credentials and scopes (Bangladesh Cyber Security Act
  2026 s.17 names AI-tool users and AI-agent data). For staff, per-edit attribution is
  employee monitoring in some jurisdictions (EU works councils) — see the governance
  research.
- **Regulation:** a rule-based origin ledger is probably not an "AI system" under the EU
  AI Act; an assistant Dynodoc runs itself makes Dynodoc a *provider* under Article 50
  (machine-readable marking of substantive AI output in exports, from 2 August 2026).
- **Performance:** origin tables are tens of bytes per patch; the projection is about
  100–300 KB for the 8,000-paragraph benchmark document (estimate); merge mapping is
  linear in the operations. Measure in the prototype.
- **Standards churn:** MCP, OpenTelemetry GenAI (still "Development"), Agent Trace
  (v0.1 RFC; repository unreachable in September 2026) — keep the internal model
  independent and export.

## 8. Phasing

1. **Origin prototype:** patch v2, projection, the rules above as unit and property
   tests, origin report; `unknown` for existing text. No AI needed to test (simulate
   agent writes via API with agent credentials).
2. **Agents over MCP:** agent identities, sessions, draft-only tools, OAuth, session
   viewer, `ChangeRequestMerged` sealing.
3. **Evidence:** C2PA/PROV exports, RFC 3161 anchoring, "ask the history".
4. **In-editor assistant** (after a scope decision) and the education pilot.

## 9. Decisions needed

1. Agents propose only by default (no Editor rights) — confirm.
2. Transcript visibility and retention (suggest: principal + owner/managers; delete
   with the document; 1 year otherwise).
3. Whether existing text stays `unknown` or owners may attest it through an audited event.
4. Word hints: keep `w:ins/@w:author` strings (e.g. Copilot) from pushed files as
   unverified hints?
5. Whether to obtain a C2PA signing certificate.
6. Scope amendment for an in-editor assistant and LLM summaries.
