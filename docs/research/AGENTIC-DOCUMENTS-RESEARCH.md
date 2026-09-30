# Agentic rich-text documents with provenance — research brief for Dynodoc

> Research input for [the agent provenance proposal](../proposals/AGENT-PROVENANCE-PROPOSAL.md). Compiled by a research agent on 30 September 2026 from primary and secondary sources listed in section 7; labels [V]/[S]/[I]/[U] mark verified facts, secondary sources, inferences and unverified items. Not legal advice.

Prepared 30 September 2026. Scope: Atlas and similar tools, relevant standards, mainstream editors, span-level lineage algorithms, and what all of this means for Dynodoc's event model.

**How to read the evidence labels**

- **[V]**: I read this in a primary source: source code, a spec, official documentation or the paper itself. The date shown is the publication date, or the date I accessed an undated page.
- **[S]**: Secondary source only, such as press, a law firm or a search-engine snippet of an official page I could not fetch.
- **[I]**: My own inference or recommendation. It is not a documented fact.
- **[U]**: I could not verify this.

Source numbers such as [S12] refer to the source table in section 7.

---

## Executive summary

1. **Atlas records at the level of whole commits, not spans of text.** In source it is called "Source control for coding agents". It captures agent sessions (prompts, messages, tool calls, thinking, files touched, patches) in a local SQLite store and links a session to a git commit by watching refs move. It installs no hooks. Nothing records who wrote which line; Atlas's blame view is ordinary `git blame`. [V, S1–S5]
2. **The owner's three claims about Atlas are accurate, with one qualification.** "Observed rather than intercepted", "re-point through amend and rebase by patch-id reconciliation" and "orphans instead of guessing" are all README text and match the code. However, re-pointing works only when the rewritten commit's diff is byte-identical (`git patch-id --stable`). A content-changing amend, a squash, or a rebase whose conflicts were resolved differently all produce an orphan. [V, S1, S3, S5]
3. **Tools that attribute individual lines to AI now exist, and they share one weakness.** Entire (trailer plus git refs), Git AI (git notes, standard v3.0.0), Agent Trace (a vendor-neutral JSON RFC, v0.1.0, January 2026) and Cursor Blame all do this. All of them key line ranges to a commit or revision. None records the edit operations themselves, so history rewrites force re-computation or heuristics. [V, S8–S17]
4. **Dynodoc already records every edit as an operation, which is its advantage.** Each `RichTextPatched` event holds exact insert, delete and retain operations counted in Unicode scalars, carries its actor, and is hash-chained. Origin of every character can therefore be computed exactly by replaying the log, as WikiWho did for Wikipedia but without heuristics. Entire judged exactly this kind of position-aware tracking "Very High" complexity for git, because git has no operation log. [V, S9; I]
5. **Record origin in the event payload, not as editor marks inside the content.** The recommendation is an optional, interned `origins` table in a version-2 `RichTextPatch`, parallel to the existing `styles` table. It is then covered by the existing `content_hash` and is never exported to Word or stripped by clients. Marks inside the content can be forged or dropped by the browser, and they change run splitting and content hashes. [I, grounded in S60]
6. **The key rule: an AI label may never drift to "human".** It can be removed only by deleting the text, by a human typing new characters in its place (those characters only), or by an explicit, audited correction event. Approval, merge, formatting, moves, restores, internal copy, round-trips and AI paraphrase ("humanize") must not lower it. Grammarly Authorship showed the harm of getting this wrong: in November 2025, text rephrased by its "Humanize" feature was grouped under "Typed by a Human". Grammarly changed the label in December 2025. [V, S52, S53; I]
7. **Anything that returns from an offline Word, Excel or PowerPoint edit can only be labelled "imported, unattested".** Dynodoc's hidden `_dd` bookmarks and alignment by content preserve block identity. They cannot show whether a person or Copilot-in-Word produced the changed text. Unchanged spans that align exactly can keep their prior origin. [V, S60; I]
8. **MCP changed substantially on 2026-07-28 and is now stateless.** The `Mcp-Session-Id` header and protocol-level sessions are gone, Sampling and Roots are deprecated, and server-to-client requests are replaced by "multi round-trip requests" (MRTR). A Dynodoc MCP server must issue its own session and draft handles as ordinary tool arguments. Existing document MCP servers are coarse: Notion uses find-and-replace, Google Docs uses index-based `batchUpdate` with revision checks, and Word only creates, reads and comments. [V, S26–S32]
9. **The standards stack is usable but immature for text.** OpenTelemetry GenAI conventions are still "Development". PROV-O (2013) fits the ontology. C2PA 2.3 (December 2025) added embedding in unstructured text and 2.4 (April 2026) added structured text, HTML and a `c2pa.ai-disclosure` assertion. C2PA also defines textual "regions of interest", with a DOCX example labelled "AI assistant edited content". SynthID Text is robust only to light edits. OpenAI has not shipped text watermarking. [V, S33–S39]
10. **EU AI Act Article 50 applies from 2 August 2026, and provenance plus review records help deployers.** Providers must mark AI-generated text in machine-readable form; secondary sources say systems already on the market get until 2 December 2026. Deployers must disclose AI text published on matters of public interest, unless it has undergone human review or editorial control. Dynodoc's approvals and review rules are exactly that evidence. This is not legal advice. [V for the Article text, S40–S41; S for the Omnibus dates, S43; I for the product fit]

---

## 1. Atlas (pacifio/atlas, tryatlas.cc)

### 1.1 Identity and maturity

| Fact | Value | Source |
|---|---|---|
| Repository description | "Source control for agents. Use multiple coding agents, track their changes and query them in one place" | GitHub API, accessed 30 Sep 2026 [V, S6] |
| Created | 14 May 2026 | GitHub API [V, S6] |
| Stars / forks / open issues | 8,456 / 339 / 27 | GitHub API, 30 Sep 2026 [V, S6] |
| Commits on `main` / contributors | 1,208 / about 41 (including anonymous) | GitHub API pagination, 30 Sep 2026 [V, S6] |
| Latest releases | `alpha-0.3.4` "Atlas Teams Early" (27 Sep 2026); `alpha-0.3.0` "ACP + Timeline" (25 Aug 2026); `alpha-0.2.3` "Accounts" (24 Jul 2026) | GitHub releases [V, S6] |
| License | Apache-2.0 | README, API [V, S1, S6] |
| Stack | Rust + Tauri desktop app; CodeMirror editor; macOS 13+ and Windows 10+ supported; Linux untested | README [V, S1] |
| Telemetry | "Anonymous usage analytics are on by default" | README [V, S1] |

**Maturity assessment [I]:** a young, fast-moving alpha, about 4.5 months old with alpha tags only. Popular (Trendshift badge), but it offers no stable API and no interchange format.

### 1.2 What it records [V]

- **Sessions.** Every agent session goes to `.atlas/sessions.db`: "prompts, messages, tool calls, the files each one touched, and the patches it applied" [S1]. Messages carry a `mode` of `text`, `tool` or `thinking`, so reasoning is kept when the agent reports it separately [S2 `model.rs`].
- **Secret redaction before writing.** Secrets are redacted before persistence. Capture "fails closed" and flags the session if scrubbing does not finish [S1; `capture.rs`].
- **Content sketches of agent writes.** Each file write stores a "sketch": the bottom-k (up to 512) hashes of distinct non-blank lines. It is used later to test *containment*, meaning how much of the agent's output survived into a commit. The minimum containment for a linked file is `MIN_CONTAINMENT = 0.5` [S4].
- **Transcript import.** Existing Claude Code history can be backfilled [S1].

### 1.3 How sessions become "checkpoints" linked to commits [V, S2, S3]

- **Definition.** A *checkpoint* is "the slice of one Session whose work landed in one git commit", keyed by the pair (Session, commit). It carries no transcript of its own.
- **Observation, not interception.** "No git hooks: they mutate the user's repository, have to chain whatever else is installed, and — decisively — cannot see a commit made before they existed. Watching refs move can" (`lib.rs`). A watcher reacts to ref movement. When a project opens, Atlas walks from the last-seen commit cursor to HEAD, so commits made from a terminal, another editor, or while Atlas was closed are still found. If the cursor was garbage-collected or rewritten, Atlas re-scans the last 200 commits.
- **Asymmetric link rule.** If a touched file *existed* in the parent commit, the path alone links it. If the file is *new*, the committed content must contain the agent's output (containment on the sketch).
- **Touches are consumed.** Once a commit settles a touched path, that touch cannot nominate the session again. Atlas credits this "carry-forward rule" to "Entire's link engine".
- **Time bound.** A session never links to a commit created before the session started.
- **Merges.** The walk follows the first parent. A merge commit creates no checkpoints itself; the commits it brought in are evaluated individually.

### 1.4 Surviving history rewrites (`reconcile_rewrites`) [V, S3, S5]

- **Mechanism.** When a linked commit becomes unreachable, Atlas builds a map from `git patch-id --stable` to commits over the most recent 200 commits on all refs, then looks up the checkpoint's stored patch-id. Patch-id "hashes the **diff** rather than the commit".
  - One candidate: re-link. Several candidates: prefer the recorded branch, otherwise orphan. No candidate: orphan.
- **Squash.** "A squash collapses several patches into one that matches none of them; a differently-resolved conflict changes the diff. Both are honest orphans."
- **Safeguards.**
  - Reconciliation is deferred while a rebase is in progress.
  - A failed probe is counted as "failed", never as an orphan.
  - An orphaned checkpoint keeps its session link and re-links if its commit becomes reachable again, for example after a reverted force-push.
  - Five or more orphans in one pass produce a single "mass orphan" health note.
- **What the tests actually cover.** The test names include:
  - `an_amend_re_points_the_checkpoint_rather_than_orphaning_it`, which only changes the message
  - `a_rebase_re_points_affected_checkpoints…`
  - `squashing_orphans_the_affected_checkpoints_rather_than_mis_attaching_them`
  - `a_conflict_resolved_differently_orphans_rather_than_mis_matching`
  - `a_cherry_pick_collision_orphans_rather_than_picking_arbitrarily`

  No test covers an amend that *changes content*. By construction that changes the patch-id, so the result would be an orphan [I from code].

### 1.5 Storage, chat with a checkpoint, shared memory, agents [V, S1, S2]

- **Storage.** SQLite at `.atlas/sessions.db` in the project's gitignored `.atlas/`. Bodies over 64 KB spill to content-addressed files alongside it. Tables "mirror the eventual server shape so syncing is a copy". A `sync_state` column acts as the outbox for syncing to an Organisation.
  - The README also says "sessions are JSONL" and that the checkpoint record is the one SQLite exception. The code says all capture goes to SQLite. My reading is that "JSONL" refers to the agents' own transcript files [I].
- **Chat with a checkpoint.** "select a checkpoint and chat with it directly, and it answers from what actually happened in that session" [S1].
- **Shared memory.** An "On-device semantic index (local embeddings, HNSW search) that every agent reads from and writes to". It holds active plan, decisions, file changes, failures and architecture notes, and is injected every turn. A "session handoff" fact pack goes into the first message [S1].
- **Agents.**
  - Claude Code and Codex run as subprocesses over ACP (Zed's Agent Client Protocol).
  - Other ACP-registry agents also run this way (Cursor, OpenCode, Kilo Code); "QA on the long tail of registry agents is ongoing."
  - "Atlas Agent", the native agent, is a hard fork of OpenAI Codex. Vendored files are headed "Modified by Atlas from upstream OpenAI Codex (Apache-2.0)" [S1; `vendor/atlas-engine/…`].

### 1.6 Takeaways for Dynodoc [I]

- **Principles worth copying:**
  - Orphan rather than guess.
  - Containment rather than similarity when deciding whether an agent's work survived.
  - Consumed touches, so a session is not credited forever.
  - Redact before persistence.
  - A single visible capture-health signal.
- **Where Atlas does not fit Dynodoc:** Atlas's problem is *recovering* links that git rewrites break. Dynodoc never rewrites canonical history, and agents would write through the engine, so the link is exact at write time. Atlas-style heuristics are needed in Dynodoc only for the offline Office round trip.
- **Granularity:** Atlas answers "which session produced this commit", not "who wrote this sentence". A document tool needs the latter.

---

## 2. Comparable tools for agent provenance in code

| Tool | What it captures | How it links to commits | Granularity | Storage | Survives rewrites? | Maturity and license |
|---|---|---|---|---|---|---|
| **Entire CLI** (entireio/cli) | Transcript, prompts, files touched, tokens, subagent records [V, S8] | Git hooks plus agent hooks. Each commit gets an `Entire-Checkpoint: <id>` trailer (checkpoint IDs are ULIDs). Work in progress sits on a short-lived shadow branch [V, S8] | Commit-level; an `Entire-Attribution: 73% agent (146/200 lines)` trailer; `entire blame` / `entire why <file>:<line>` [V, S8, S9] | One git ref per checkpoint, `refs/entire/checkpoints/<shard>/<id>`. Older versions used a `entire/checkpoints/v1` branch [V, S8, S10] | The trailer travels with the commit message. `commit --amend -m` can drop it, but it is restored from session state in most cases [V, S10] | MIT; about 5,140 stars; created 2 Jan 2026; nightly v0.11.4 on 30 Sep 2026 [V]. Company launched with a $60M seed led by Felicis on 10–11 Feb 2026 [S, S12]. Checkpoint commits can be signed with GPG or SSH, best effort [V, S11] |
| **Git AI** (git-ai-project/git-ai) | Line ranges per session and trace; agent, model, human author; prompts stored outside git (Teams) [V, S13, S14] | Agents report exactly which lines they wrote ("does not use AI or heuristics to 'detect' AI code"). The README's FAQ says it uses no git hooks; agent hooks are installed [V, S13] | Line-level. Keys: `s_<14hex>::t_<14hex>` (AI session and trace), `h_<14hex>` (a known human seen by an IDE extension). Unkeyed lines are "untracked" [V, S14] | Git notes under `refs/notes/ai`: an attestation section, a `---` line, then JSON metadata (`schema_version: "authorship/3.0.0"`) [V, S14] | Yes, by rewriting notes after rebase, cherry-pick, squash, stash, reset or amend ("eventually consistent"). Squash: last session wins on overlapping lines. Human-modified AI lines "SHOULD be re-attributed to the human" [V, S13, S14] | Apache-2.0; about 2,800 stars; v1.7.5 on 9 Sep 2026 [V] |
| **Agent Trace** (agent-trace.dev) | A spec, not a tool: trace records with files, conversations, ranges, contributor (`human` / `ai` / `mixed` / `unknown`), `model_id`, optional `content_hash` [V, S15] | Storage-agnostic ("local files, git notes, a database, or anything else"). `vcs.revision` is a git SHA or a jj change ID [V, S15] | Line ranges "at the recorded revision"; range-level `content_hash` (for example `murmur3:…`) for moved code [V, S15] | Undefined. MIME type `application/vnd.agent-trace.record+json` [V, S15] | Explicitly open: "How should I handle rebases or merge commits? We expect to see different implementations…" [V, S15] | v0.1.0, "RFC", January 2026, CC BY 4.0. Proposed by Cursor with Amp, Amplitude, Cline, Cloudflare, Cognition, git-ai, Jules, OpenCode, Tapes and Vercel [V, S15, S16]. The linked repo `github.com/cursor/agent-trace` returned HTTP 404 on 30 Sep 2026 [V]. The schema's version pattern `^[0-9]+\.[0-9]+$` rejects its own example `"0.1.0"` [V, a spec inconsistency] |
| **Cursor Blame** | Tab, Agent (by model) and Human lines; conversation summaries on hover [V, S17] | Retrieves attribution data for committed code from Cursor's servers and caches it locally [V, S17] | Line-level | Cursor service | Not documented [U] | Enterprise plan; introduced in Cursor 2.4, reportedly 22 Jan 2026 [S] |
| **GitHub Copilot coding agent** | Session logs with the agent's reasoning and tools used [V, S19] | Commits authored by Copilot with the assigning human as co-author. Since 20 Mar 2026 an `Agent-Logs-Url` trailer links each commit to its session logs [V, S18] | Commit-level | GitHub | Trailer follows the commit message | Commercial |
| **Claude Code** | Local JSONL transcripts at `~/.claude/projects/<project>/<session-id>.jsonl`; the format is internal and "changes between versions" [V, S21] | `Co-Authored-By` commit trailer and a PR line, configurable via the `attribution` setting (`includeCoAuthoredBy` deprecated since v2.0.62) [V, S20] | Commit-level | Local files | Trailer only | Commercial |
| **Aider** | Chat history; commits | Appends "(aider)" to author and committer names by default; `--attribute-co-authored-by` adds a trailer [V, S22] | Commit-level | Git metadata | Trailer / author field | Apache-2.0 [U for current version] |
| **VS Code `git.addAICoAuthor`** | none | Adds `Co-authored-by: Copilot`. The default moved to `all` in 1.117, then `chatAndAgent` after a bug attributed non-Copilot completions, then back to `off` in 1.119 (5–6 May 2026) [V, S23, S24] | Commit-level | none | none | This episode shows how contentious and error-prone coarse trailers are [I] |
| **SpecStory** | Saves Cursor, Copilot and terminal-agent chats as Markdown in `.specstory/history/`; the CLI wraps Claude Code and reads its JSONL [S, S25] | None (file per session) | Session-level | Files in the repo | n/a | Commercial with free tiers [U] |

**Schemas that exist [V]:**
- Agent Trace trace record (JSON Schema 2020-12) [S15].
- Git AI Authorship Log v3.0.0 (text plus JSON inside git notes) [S14].
- Entire's commit trailers and per-checkpoint `metadata.json` trees [S8, S9].
- The OpenTelemetry GenAI span and attribute conventions, covered in section 3.2 [S33].

**No schema survives rewrites and refactors well [I].**
- *Trailers* (Entire, Copilot, Claude Code, Aider) survive rebase and amend because they sit in the message. Squash concatenates or drops them, and they are all-or-nothing.
- *Notes* (Git AI) need a daemon to re-map line ranges after each rewrite.
- *Patch-id* (Atlas) survives only identical diffs.
- *Content hashes* (Agent Trace) survive moves but not edits.
- The root cause in every case is that git stores snapshots, not operations. Entire's own design notes rejected "position-aware diff tracking" as "Very High" complexity and chose a per-file heuristic that assumes "users modify their own recent code before agent code" [V, S9].

---

## 3. Standards and building blocks

### 3.1 Model Context Protocol (MCP)

- **Current version.** **2026-07-28**, marked "Current" on the versioning page [V, S27]. The previous version was 2025-11-25.
- **Major changes in 2026-07-28** [V, S26]:
  1. Protocol-level sessions and the `Mcp-Session-Id` header are removed. "Servers that need cross-call state use explicit, server-minted handles passed as ordinary tool arguments" (SEP-2567).
  2. The protocol is stateless: there is no `initialize` handshake. Each request carries `io.modelcontextprotocol/protocolVersion` and client capabilities in `_meta`, and clients SHOULD send `clientInfo` on each request. `server/discover` is mandatory for servers (SEP-2575).
  3. **Multi Round-Trip Requests** replace server-initiated `sampling/createMessage`, `roots/list` and `elicitation/create`. The server returns `resultType: "input_required"` and the client retries with `inputResponses` (SEP-2322).
  4. **Tasks** moved to an official extension (`io.modelcontextprotocol/tasks`).
  5. `subscriptions/listen` replaces `resources/subscribe`.
  6. **Deprecated:** Roots, Sampling and Logging (SEP-2577), and Dynamic Client Registration in favour of Client ID Metadata Documents.
  7. OpenTelemetry `traceparent` / `tracestate` / `baggage` propagation in `_meta` is documented (SEP-414).
- **Features.** Servers offer Resources, Prompts and Tools; clients offer Elicitation [V, S26].
- **Tool definitions.** Tools have `inputSchema`, optional `outputSchema` and `structuredContent`, and annotations (`readOnlyHint`, `destructiveHint`, `idempotentHint`, `openWorldHint`). "clients MUST consider tool annotations to be untrusted unless they come from trusted servers" [V, S28].
- **Authorization.** OAuth 2.1 (draft-ietf-oauth-v2-1-13). Servers MUST implement Protected Resource Metadata (RFC 9728); Resource Indicators are required [V, S29].

**Existing document MCP servers:**

| Server | Operations exposed | Edit model | Source |
|---|---|---|---|
| Notion (hosted) | 25+ tools including `notion-fetch`, `notion-create-pages`, `notion-update-page`, `notion-move-pages`, `notion-create-comment`, plus Custom Agent session tools | `notion-update-page` applies batched search-and-replace. Each `old_str` must match exactly one location, and if any fails "the page is left unchanged" | [V, S30], accessed 30 Sep 2026 |
| Google Docs (`docsmcp.googleapis.com`, developer preview) | `read_doc` (JSON representation) and `update_doc` (batch update requests) | About 51 `batchUpdate` operations with index ranges: `insertText`, `deleteContentRange`, `replaceAllText`, `updateTextStyle`, comments, and `acceptSuggestion` / `rejectSuggestion` / `deleteSuggestion`. The list contains **no operation to create a suggestion** | [V, S31], page updated 2026-07-10 |
| Microsoft Word (Work IQ / Agent 365, preview) | `WordCreateNewDocument`, `WordGetDocumentContent` (plain text plus comments), `WordCreateNewComment`, `WordReplyToComment` | No in-place editing of existing text | [V, S32], ms.date 2026-08-03 |

**Sketch of a Dynodoc MCP server [I, grounded in S26–S29 and the repository]:**

- **Resources:**
  - `dynodoc://documents/{id}/outline` returns node IDs, kinds, content hashes and positions (as the existing `/structure` route does).
  - `dynodoc://documents/{id}/nodes/{node_id}?at={seq}` returns ProseMirror JSON plus its `before` hash.
- **Tools:**
  - `begin_agent_session(document_id, purpose)` returns a **server-minted** `session_handle` (ULID). This replaces the removed protocol session.
  - `create_draft(session_handle, name)` returns a `draft_id`.
  - `read_nodes(...)` is read-only.
  - `propose_text_edit(draft_id, node_id, base_hash, edits | new_content, rationale)` is idempotent per request ID and returns a diff preview. It maps to a base-checked `RichTextPatch`, the same optimistic-concurrency idea as Google's `requiredRevisionId`.
  - `insert_node`, `move_node`, `delete_node` operate on the draft only.
  - `add_comment`, `suggest(node_id, detail)` maps to `SuggestionProposed`.
  - `submit_change_request(draft_id, note)`.
  - There is **no** tool that writes the team version. Merging stays a human action under `document_policy`. Any consequential confirmation uses MRTR `input_required` (elicitation).
- **Authorization:** OAuth 2.1 scopes such as `documents:read`, `drafts:write` and `comments:write`. The token identifies the human principal. The registered OAuth client identifies the agent product. The per-request `clientInfo` and model name are self-reported and must be stored as *claims*, not facts.
- **Tracing:** store the incoming `traceparent` against the session so agent-side OTel traces can be joined to Dynodoc's record.

### 3.2 OpenTelemetry GenAI semantic conventions [V, S33]

- **Where they live.** The conventions moved to a separate repository, `open-telemetry/semantic-conventions-genai` (created 5 May 2026). Their status is **"Development"**, and every attribute in the tables carries the development badge.
- **Span operations.** `gen_ai.operation.name` values include `create_agent`, `invoke_agent`, `invoke_workflow` and `execute_tool`, alongside inference operations such as `chat`.
- **Correlation.** `gen_ai.conversation.id` correlates a session. Tool spans carry `gen_ai.tool.call.id` (Recommended) and `gen_ai.tool.call.arguments` / `gen_ai.tool.call.result` (Opt-In).
- **Content capture is opt-in.** Prompts and completions go in `gen_ai.input.messages`, `gen_ai.output.messages` and `gen_ai.system_instructions`, all Opt-In and flagged as likely to contain PII. "OpenTelemetry instrumentations SHOULD NOT capture them by default". The page also defines events (`gen_ai.client.inference.operation.details`) and an MCP page.
- **Relevance [I].** OTel is telemetry: sampled, mutable pipelines with no integrity guarantee. Reuse its vocabulary for Dynodoc's agent-session tables and link by trace ID, but keep the provenance record in Dynodoc's own append-only store.

### 3.3 W3C PROV-O [V, S34]

- **Status.** W3C Recommendation, 30 April 2013.
- **Core classes.** `prov:Entity`, `prov:Activity`, `prov:Agent`.
- **Core properties.** `wasGeneratedBy`, `wasDerivedFrom`, `wasAttributedTo`, `used`, `wasAssociatedWith`, `wasInformedBy`, `actedOnBehalfOf`, `startedAtTime` / `endedAtTime`.
- **Expanded properties.** `wasRevisionOf`, `wasQuotedFrom`, `hadPrimarySource`, `qualifiedAssociation` / `hadPlan`.

**Proposed mapping [I]:**

| PROV concept | Dynodoc equivalent |
|---|---|
| Entities | Block revisions (`node_id@seq`), spans, document versions, transcripts, uploaded files |
| Activities | Agent session, agent turn or tool call, merge, review |
| Agents | Human identity (Person) and agent (SoftwareAgent) with `actedOnBehalfOf` the human; model identifier recorded as a plan or used entity |
| Paste from a URL | `wasQuotedFrom` |
| Merge | Merger `wasAssociatedWith` the merge activity; span origin stays `wasAttributedTo` the agent |

HANDOFF already says PROV-O "informs the design vocabulary" [S60].

### 3.4 C2PA 2.x and text [V, S35]

The spec page lists versions 1.0 to 2.4. Version 2.4 is dated **April 2026** and 2.3 **December 2025**.

**Formats and text support:**
- **Documents.** Embedding into ZIP-based formats has been supported since 1.4 (November 2023). That covers OOXML (DOCX, XLSX, PPTX), ODF, EPUB and OpenXPS, using a *collection data hash* over every part. PDF is also supported.
- **Unstructured text (2.3).** A `C2PATextManifestWrapper` (magic `C2PATXT\0`) encodes the JUMBF manifest store as invisible Unicode variation selectors (U+FE00–FE0F, U+E0100–E01EF) with a data-hash hard binding. The spec says this "should only be used … where no other embedding method is feasible" and "remains under review".
- **Structured text and HTML (2.4).** Source code, YAML, Markdown, AsciiDoc and LaTeX use an ASCII-armour comment block that references or embeds the manifest.
- **Span-level expression.** Actions have a `changes: [region-map]` field. Since 1.4 there are "regions of interest for text-based formats (e.g., PDF, Office, EPUB)". The spec's own example is a DOCX `xpointer(/w:document/w:body/w:p/)` region with the description "AI assistant edited content".
- **AI signals.** `digitalSourceType` on actions and, since 2.4, on ingredients. A new `c2pa.ai-disclosure` assertion (2.4) carries `modelType`, `modelName`, `modelIdentifier` and a `humanOversightLevel` (`fully_autonomous`, `prompt_guided`, `human_validated`). A table maps these to IPTC source types.
- **JSON-LD.** 2.4 adds "crJSON", a JSON-LD serialisation. It is "a derived view … not independently verifiable or intended as an input format".
- **Soft bindings.** `c2pa.soft-binding` records a fingerprint or watermark (the `alg` field names the algorithm, with an optional region), so a manifest can be found again after the hard binding breaks.

**Limits [I]:**
- A hard binding is designed to break on any edit: C2PA proves "this file is as signed", not "this sentence stayed AI-written".
- Signing needs X.509 credentials on the C2PA trust list.
- Word and Excel will not maintain a manifest when a user edits and saves, so a manifest on an export will not describe the file that comes back.
- Variation-selector text is easily stripped by normalisers.

For Dynodoc, C2PA is an **export format** for provenance: sign a DOCX export with regions and actions derived from Dynodoc's origin projection. It is not the internal model.

### 3.5 IPTC Digital Source Type [V, S36]

The active terms are:
- `digitalCapture`, `computationalCapture`, `negativeFilm`, `positiveFilm`, `print`
- `humanEdits`, `compositeWithTrainedAlgorithmicMedia` ("Augmentation, correction or enhancement using a Generative AI model"), `algorithmicallyEnhanced`, `digitalCreation`, `dataDrivenMedia`
- `trainedAlgorithmicMedia` ("created algorithmically using an Artificial Intelligence model trained on captured content"), `algorithmicMedia`
- `screenCapture`, `virtualRecording`, `composite`, `compositeCapture`, `compositeSynthetic`

Retired terms are `minorHumanEdits`, `digitalArt` and `softwareImage`. The page shows it was last modified 23 Oct 2024.

The vocabulary is written for media. There are no text-specific terms, but C2PA 2.4 applies them to any asset [V, S35; I].

**Proposed mapping [I]:**

| Dynodoc origin | IPTC term |
|---|---|
| AI-generated | `trainedAlgorithmicMedia` |
| AI-modified human text, or a mixed span | `compositeWithTrainedAlgorithmicMedia` |
| Human-typed | `digitalCreation` |
| Human edits with non-generative tools | `humanEdits` |

### 3.6 Text watermarking

- **SynthID Text [V, S37, S38].** Described in Dathathri et al., *Nature* 634:818–823, published online 23 Oct 2024.
  - It modifies sampling only and needs no LLM for detection. A live experiment covered "nearly 20 million Gemini responses".
  - It is open-sourced in Hugging Face Transformers v4.46.0+ as a logits processor with a Bayesian detector that returns watermarked, not watermarked or uncertain.
  - Google's page (updated 2025-04-09): robust to "cropping pieces of text, modifying a few words, or mild paraphrasing". It is "less effective on factual responses", and confidence "can be greatly reduced when an AI-generated text is thoroughly rewritten, or translated". It is "not designed to directly stop motivated adversaries".
- **OpenAI.** On 4 Aug 2024 OpenAI updated "Understanding the source of what we see and hear online". According to the reporting, it said its text watermark was accurate against "localized tampering, such as paraphrasing" but "less robust against globalized tampering" (translation, rewording with another model, inserting and then deleting characters), and that it could "stigmatize use of AI as a useful writing tool for non-native English speakers". OpenAI chose not to release it. [S] The primary page returned HTTP 403; the wording comes from secondary reports and search snippets [S39].
- **Implication [I].** Watermarks are applied by the model provider at generation time. Dynodoc cannot add one to text it did not generate. At best it could run a provider's detector as a *signal*. Provenance recorded at write time is stronger evidence than detection after the fact. The stigma concern matters for Bengali and other non-native-English writers, so provenance should serve reviewers and not become a public "AI shame" badge unless policy requires it.

### 3.7 EU AI Act Article 50

- **Article 50(2) [V, S40].** "Providers of AI systems, including general-purpose AI systems, generating synthetic audio, image, video or text content, shall ensure that the outputs … are marked in a machine-readable format and detectable as artificially generated or manipulated". This does not apply "to the extent the AI systems perform an assistive function for standard editing or do not substantially alter the input data".
- **Article 50(4) [V, S40].** Deployers of a system that generates or manipulates text "published with the purpose of informing the public on matters of public interest shall disclose" this. The exception is "where the AI-generated content has undergone a process of human review or editorial control and where a natural or legal person holds editorial responsibility".
- **Application date.** 2 August 2026 [V, S41].
- **Code of Practice.** The final *Code of Practice on transparency of AI-generated content* (marking and labelling) was published **10 June 2026**. It is voluntary [V, S41]. Commentary says it favours "layered" solutions combining metadata, watermarking and provenance, because "no single marking technique" meets all four requirements. It also excludes ordinary editing (spellcheck, grammar, format conversion) from scope [S, S42].
- **Digital Omnibus.** Secondary sources report Regulation (EU) 2026/1744, published in the OJ on 24 Jul 2026 and in force on 27 Jul 2026. Systems placed on the market *before* 2 Aug 2026 reportedly get until **2 December 2026** to meet Article 50(2) marking [S, S43]. I could not open EUR-Lex to confirm the text [U].
- **Relevance to Dynodoc [I, not legal advice].**
  - If Dynodoc runs its own generative feature, it may be a *provider* of an AI system that generates text. It would then need machine-readable marking of outputs in exports, such as C2PA or an embedded provenance part, unless the assistive-editing exemption applies. That exemption requires recording what kind of AI edit occurred.
  - Customers who publish are *deployers*. Dynodoc's per-block approvals, required-approval rules and merge records are direct evidence of "human review or editorial control".

---

## 4. Agentic editing in mainstream document tools (2025–2026)

| Product | Agentic editing | How AI edits reach the document | Span-level AI provenance shown to reviewers? | Sources |
|---|---|---|---|---|
| **Microsoft Word, Copilot Agent Mode** | Announced 29 Sep 2025 ("vibe working", starting in the Frontier program) [V, S44]. Secondary sources say it became GA and the default around April 2026 [S] | Since about April 2026 (Frontier / Insiders Beta on Windows): "Copilot can now track changes at the word level". If Track Changes is on, Copilot's edits are tracked. Secondary sources say Copilot cannot turn Track Changes on or off or accept or reject changes [S, S45] | Only through Track Changes if the user turned it on. **Whether tracked revisions carry "Copilot" or the user as author is not documented** [U]. No persistent AI label after acceptance was found [U] | S44, S45 |
| **Google Docs, Gemini** | Co-editing with Gemini (9to5Google, 10 Mar 2026). Gemini acts on comment threads ("Smart Review", July 2026, per secondary reports) [S] | "Gemini's suggested edits are only visible to you until you approve them" (rollout 22 Apr to 6 May 2026) [V, S46] | No documentation found of AI labels in version history; the pages consulted do not describe attribution [U] | S46, S47 |
| **Notion agents** | Notion 3.0 (18 Sep 2025): "Anything you can do in Notion, your Agent can do (across dozens of pages at once)", with "over 20 minutes of multi-step actions" [V, S48] | Direct edits. Help pages mention undoable changes [S] | Not documented [U] | S48, S30 |
| **ChatGPT Canvas** | Canvas showed "Show changes" diffs and version arrows. It was reportedly removed from GPT-5.5 models on **28 May 2026** and replaced by in-chat "writing blocks" [S, S49] | Whole-document versions | No span provenance; the whole canvas is AI-mediated | S49 (primary help pages not fetched: 403) |
| **Claude artifacts** | Artifacts keep version history per publish, with restore [S, S50] | Whole-artifact versions | No span-level AI or human attribution documented [U] | S50 |
| **Grammarly Authorship** | Not an agent editor, but **the closest precedent for span origin labels** | Tracks text as it is written and colour-codes: human-typed, AI-generated, AI-modified, edited by Grammarly or spellcheck, pasted from a known or unknown source. Replay and report are available [V/S, S51] | **Yes.** In Nov 2025 it labelled "Humanize"-rephrased AI text as "Typed by a Human", which amounts to laundering. By 11 Dec 2025 that text was under "AI-Generated or Rephrased with AI". Text generated inside Google Docs or pasted from non-web apps still ends up "Unknown" [V, S52, S53] | S51–S53 |
| **Ink & Switch Patchwork** (research) | An AI bot edits "as another collaborator" on a branch that "you can choose to partially or completely merge". "The history timeline also shows which edits came from the bot" (19 Mar 2024) [V, S58] | Branch, then partial merge | Yes, attribution by actor in history | S58 |

**Conclusion [I].** As of September 2026 I found no mainstream editor that keeps a *durable, span-level, tamper-evident* AI-origin record that reviewers can see after acceptance. The two nearest precedents are:
- Track Changes or suggestions *before* acceptance (Word, Docs).
- Authorship-style colouring for integrity checks (Grammarly).

This is a genuine gap that Dynodoc's hash-chained operation log could fill.

---

## 5. Span-level authorship and lineage algorithms, and propagation rules

### 5.1 Prior art

- **WikiWho** (Flöck & Acosta, WWW 2014) [V, S54]. A graph of revisions, paragraphs, sentences and tokens, with paragraph and sentence nodes identified by MD5 hash. It traces reintroductions (reverts) and moves back to the *original* author. It reached "an average of 95% precision" on a gold standard of 240 Wikipedia tokens, beat the A3 algorithm by over 10 percentage points on average, and ran about ten times faster.
  - Lesson: when content is reintroduced, the original origin is restored.
  - Its heuristics exist only because Wikipedia stores snapshots; Dynodoc stores operations.
- **git blame and git diff** [V, S55]. Blame annotates each line with the last-modifying revision. It follows whole-file renames automatically. `-M` detects lines moved or copied within a file (default threshold 20 alphanumeric characters) and `-C` does the same across files (repeatable for broader search). `--ignore-rev` skips formatting-only commits. The diff algorithms are Myers (default), patience, histogram and anchored, and `--color-moved` highlights moved lines.
  - Lessons: formatting-only revisions should not take authorship ("ignore-rev"), and move detection needs thresholds.
- **Peritext** (Litt, Lim, Kleppmann, van Hardenberg; CSCW 2022, PACM HCI) [V, S56]. Marks anchor to positions "before" or "after" a character ID.
  - Bold-like marks *expand*: an inserted character "inherits the bold/non-bold status of the preceding character".
  - Links and comments *do not*: text inserted at their edges lands outside.
  - Comments may overlap. Exclusive marks resolve by last-writer-wins on operation IDs.
  - Lesson: an origin label must be a *non-expanding* attribute and, stronger still, never inherited at all. It comes from the operation that inserted the character.
- **Yjs v14 attribution** (release candidate `v14.0.0-rc.28`, 29 Sep 2026; 13.6.x still `latest`) [V, S57]. The `y-simple-attribution-server` README says: "A change may be attributed to a user/AI agent, a timestamp, and other properties"; it returns an `IdMap` of change ranges to attributes. Attribution derives from the blocks an update *creates*; pure deletions name no actor [S].
- **Automerge / Patchwork.** CRDT history with Peritext-style rich text (Ink & Switch dispatch, May 2024) [S]. Patchwork (section 4) staged AI edits on branches with partial merge [V, S58].
- **CoAuthor** (Lee, Liang & Yang, CHI 2022) [S, S59]. Keystroke-level logs of 63 writers and GPT-3 over 1,445 sessions, with replay. This shows that character-level human/AI provenance can be captured and replayed.
- **Entire's analysis** [V, S9]:
  - "Line Hash Tracking" fails on common lines, for example which `}` was removed.
  - "Position-Aware Diff Tracking" is accurate but has "Very High" complexity without an operation log.
  - Entire chose per-file pools with a "LIFO assumption".
- **Git AI rules** [V, S14]: squash means the "LAST session's attribution wins" on overlapping lines; human-modified AI lines are re-attributed to the human; conflict resolutions go to the human resolver.
  - For prose, re-attributing a whole line or sentence because one word changed is too coarse. It is exactly the downgrade Dynodoc should avoid [I].

### 5.2 Recommended model for Dynodoc [I]

Store provenance on three independent axes, per Unicode scalar, as run-length intervals per node. This matches the patch contract's unit.

1. **`generated_by`: who produced these characters.** Classes:
   - `human` (typed in the editor)
   - `ai` (produced by an AI operation)
   - `pasted` (external clipboard; optional source URL)
   - `imported` (Office push or upload)
   - `unknown` (text that predates the feature)

   Each carries a reference to the operation's actor, and for `ai` to the agent session and tool call.
2. **`transformed_by_ai`: a region flag.** Set on characters that an AI operation *touched* but kept, for example human words that survive inside an AI rewrite of a sentence.
   - This prevents *reverse laundering*: a character-level Myers diff of an AI rewrite keeps common words such as "the" and "and", which would otherwise remain "human".
   - Compute AI-operation diffs at **word granularity within the rewritten sentence span** and flag the whole hull.
3. **`attestation`: how much to trust the label.**
   - `server` (the agent wrote through Dynodoc's API or MCP with agent credentials)
   - `client` (the browser reported typing or pasting)
   - `none` (imports, legacy text)

**Display [I].** Aggregate to *words* (tokens), as WikiWho did:
- A word is AI if any of its characters is AI.
- It is "mixed" when origins differ.
- Snap boundaries to grapheme clusters so a Bengali conjunct joined with ZWJ is never split in the display.

**Review is a separate dimension.** Approval of a change request adds a *review attestation* (who reviewed, when, under which policy). It never changes `generated_by`.

**Mapping to Agent Trace contributors:** `human` → human, `ai` → ai, mixed runs → mixed, `pasted` / `imported` / `unknown` → unknown.

### 5.3 Propagation rules

"Downgrade" means making text look *more human* than it is. Moving from `ai` to `unknown` is **not** a downgrade to `human`, because "unknown" is never shown as human.

| # | Operation | Effect on origin | May it downgrade an AI label? |
|---|---|---|---|
| 1 | Human inserts (types) | New characters get `human`; neighbours are unchanged (no expansion either side) | n/a (new characters only) |
| 2 | Agent inserts (tool call, draft edit) | New characters get `ai` with session and tool call; attestation `server` | n/a |
| 3 | Delete (anyone) | Characters leave the projection. Their origin is kept in the tombstone history for restore | Removes text; does not relabel it |
| 4 | Human replaces AI text | Deleted AI characters are gone; the human's *new* characters are `human`; untouched AI characters in the sentence stay `ai` (the word shows "mixed") | **Yes, only for the characters actually retyped** |
| 5 | AI rewrites human text | New characters `ai`; surviving human characters inside the rewritten hull get `transformed_by_ai` | No (upgrade) |
| 6 | AI paraphrases AI text ("humanize", "rephrase") | `ai` throughout | **No: this is laundering and is blocked** |
| 7 | Formatting-only change (`Retain` with style, `Props`), by anyone | No change to `generated_by`. Optionally record formatting provenance separately | No |
| 8 | Block move (`NodeMoved`, stable ULID) | No change | No |
| 9 | Cut/copy and paste within Dynodoc, verified | Restore the original origins. Verify by exact match against the document's own current or tombstoned text, or a clipboard slice signed by the server | No |
| 10 | Paste from outside (unverified) | `pasted`, attestation `client`. **If the text matches an output of a Dynodoc agent session** (block hash or MinHash containment using the existing `similarity` module), set `ai` with a "pasted" note | No |
| 11 | Restore (`NodeRestored`, "restore as new changes") | Original origins return (WikiWho's reintroduction principle) | No |
| 12 | Three-way merge, "Use theirs" / "Keep the team's" | Characters keep the origins of the side they came from. The merger is recorded as *responsible*, not as origin | No |
| 13 | Manual conflict resolution typed by a human | Typed characters `human` (Git AI's rule) | Only for typed characters |
| 14 | Suggestion accepted, change request approved or merged | No change to origin; adds a review attestation | **No** |
| 15 | Office round trip, unchanged aligned text | Keeps prior origin (alignment by bookmark or content) | No |
| 16 | Office round trip, changed text | `imported`, attestation `none`, uploader recorded as responsible | No. `ai` becomes `imported`, which is never `human` |
| 17 | Non-generative spellcheck or autocorrect (client) | `human` (IPTC `humanEdits` analogue). The Code of Practice treats "spellchecking, grammar correction" as outside scope | n/a |
| 18 | LLM grammar or style fix | Inserted characters `ai`; hull `transformed_by_ai`. Also record an "assistive edit" kind and an edit-distance ratio, relevant to the Article 50(2) "standard editing" exemption | No |
| 19 | Administrative correction (for example a mis-labelled import) | Explicit compensating event with a reason, by owner or admin, visible in audit | **Yes, but only through this explicit, audited event** |
| 20 | Human retypes AI text with no net change | A diff produces `Retain`, so the characters stay `ai` | No. Retyping identical text cannot launder it |

**Rule of thumb [I]:** only *new human keystrokes* or *explicit audited correction* can make text look more human. Every AI operation, including one that only paraphrases, pushes the label towards `ai`. Every approval adds review evidence and does not change origin.

---

## 6. Design implications for Dynodoc

### 6.1 What the repository gives us today [V, S60]

- **Events.** 15 payload variants (`payload.rs`), including `RichTextPatched { node_id, patch }`, `SuggestionProposed` / `Accepted` / `Rejected`, `NodeMoved` and `NodeRestored`. Every event has `actor_id` (an identity UUID) and a gap-free per-document `seq`. Appends are serialized by a lock on the document row. There is no CRDT.
- **Hashing.** `content_hash` = SHA-256 over length-framed (event type, target node, canonical-JSON payload, actor UUID bytes); `chain_hash` = SHA-256(content ‖ previous). Timestamps and other columns are **not** hashed, and there is no external anchoring ("a privileged operator could replace the entire chain").
- **Rich-text patch v1.** `{version, before, after, styles, edits}`. Styles are interned per patch and "Preserve every JSON property, even marks unknown to this server". Counts are Unicode scalars. Guards: 100,000 scalars per container, 64 path levels, 10,000 edits or styles per patch, and a 20 ms Myers deadline. Uneconomical deltas fall back to a full-field event.
- **Drafts.** `workspace_draft` (with `base_seq`, `base_state`) and `draft_edit` rows (payload plus `actor_id`) are append-only. "Draft rows … receive the canonical chain guarantee only when shared."
- **Change requests and review.** `change_request_review` is append-only. `document_policy` holds `required_approvals`, `merge_roles` and `protect_team_version`. Overrides go to the operations audit. **"Merged events remain attributed to the person who merged."**
- **Office round trip.**
  - Word: hidden `_dd…` bookmarks carry block identity, with fallback alignment by content.
  - Excel and PowerPoint: `dynodoc:document=…;revision=…` in the core properties' subject, plus content alignment.
  - `document_upload` keeps the original file bytes (≤ 20 MB).
- **Similarity.** 128-value MinHash on word 3-shingles within blocks, 32 LSH bands, block hashes for blocks of three or more words, a 512-byte signature. Measured: 6.3 ms to fingerprint a 192,000-word document and 0.02 ms per comparison.

### 6.2 Mapping agentic concepts onto Dynodoc [I]

| Concept | Dynodoc construct | Change needed |
|---|---|---|
| Agent (Claude, Codex, a server-side model) | New `identity` rows of kind `agent`, one per (principal, agent product); `acted_on_behalf_of` the human | Migration adding `identity.kind` and `on_behalf_of`. Because `actor_id` is hashed, agent authorship becomes part of each event's hash |
| Agent session (Atlas "session") | New append-only `agent_session`: ULID, document, principal, agent identity, OAuth client, self-reported model and tool version, `traceparent`, times, status | Table plus immutability trigger, like `draft_edit` |
| Turns, tool calls, reasoning | Append-only `agent_turn` and `agent_tool_call` (redacted at write time), transcript stored as compressed `bytea` with a salted SHA-256 commitment | Tables. Redact before persistence (Atlas and Entire precedent) |
| Agent's working area ("branch") | A personal draft owned by the principal; `draft_edit.actor_id` = the agent identity | Mostly exists |
| Proposal to the team | Change request (`workspace_draft.submitted_at`, source, note) | Add `agent_session_ids` to `source` |
| Inline proposal | `SuggestionProposed` with the agent as actor | Exists |
| Accept / modify / reject per block | Accept: merge the block. Modify: a human edits the draft (human-actor `draft_edit`, so rules 4 and 13 apply). Reject: exclude the block. Store per-block decisions append-only | New `change_request_decision` table, or an extension of `change_request_review` with `node_id` plus decision |
| Anchoring the merge in the chain | New canonical event `ChangeRequestMerged` | See 6.4. Needs a migration widening the `event.type` CHECK, as 0016 and 0018 did |
| Origin of text | Version-2 `RichTextPatch` with interned `origins: [descriptor]`; `Insert { text, style, origin? }`; optional `Mark { range, transformed_by_ai }` | Engine and fixture change; bump the patch `version` |
| "Who wrote this?" | Origin projection per node (run-length), rebuilt from events and cached in snapshots | New projection. HANDOFF requires projections to be "rebuildable from checkpoints, permission-aware and unable to override canonical history" |
| "Chat with a checkpoint" | Answer "why is this paragraph like this?" from the transcripts linked to the spans' origin sessions | Needs an LLM. **Decision 000 marks LLM features out of PoC scope, so a new decision record is required first** |
| Shared memory across agents | A document-level "agent notes" resource (style guide, decisions) exposed over MCP, permission-scoped | Optional; plain append-only notes suffice |

**Why origin belongs in the event payload rather than in ProseMirror marks [I]:**
- **Integrity.** Payload fields are hashed by the existing `content_hash`.
- **Forgery.** The server derives or validates origin from the actor and channel. The browser cannot spoof `ai` → `human` by editing mark JSON.
- **Exports stay clean.** Word, Excel and PowerPoint files and public views stay free of provenance marks.
- **Formatting runs are unaffected.** Runs would otherwise split on every origin boundary; HANDOFF already warns that "equal styles do not make two passages or their authorship the same object".
- **Merges carry origin explicitly.** Today merged events are recomputed diffs attributed to the merger. With v2, the merge engine copies origin indices from the draft's projection into the merged patch's inserts. That fixes the current "merged events … attributed to the person who merged" loss for text origin, while keeping the merger as the event actor (responsibility).
- **Compatibility.** If the new fields are omitted when empty (`skip_serializing_if`), canonical JSON of existing events is unchanged, so historical hashes stay valid.

### 6.3 Where attestations come from [I]

| Channel | Actor | Attestation |
|---|---|---|
| MCP or API with agent credentials | Agent identity | `server` |
| Browser typing | Human | `client` |
| Browser paste | Human | `client` + `pasted`, with the fingerprint check against agent outputs (rule 10) |
| Browser AI features (if Dynodoc adds in-editor generation) | Route generation through the server so the output is fingerprinted and inserted as an agent operation | `server` |
| Office push or upload | Uploader | `none` for changed spans |
| Legacy text before the feature | n/a | `unknown`. Do not back-fill as human |

### 6.4 What must be hashed into the chain for tamper evidence [I]

1. **Actor identity per event.** Already hashed. With agent identities, agent authorship becomes tamper-evident.
2. **Origin descriptors and `transformed_by_ai` ranges** inside the `RichTextPatched` payload. Hashed automatically.
3. **`ChangeRequestMerged` payload:**
   - draft ID and `base_seq`
   - **a Merkle root (or hash chain) over the ordered `draft_edit` rows**: payload, actor and order. This retroactively seals the un-chained draft history.
   - agent session IDs with **salted** transcript commitments. Salt them so erasing the transcript and salt leaves an unguessable hash (GDPR-compatible with the existing erasure receipts).
   - the digest of the review and decision rows, with the approver IDs counted under `document_policy`
   - the override flag if rules were bypassed
   - for imports, the SHA-256 of the uploaded file in `document_upload`
4. **Self-reported model or tool identifiers** belong in session or merge payloads. They are hashed, which makes them tamper-evident, but they are *not* verified and should be labelled "claimed".
5. **Time.** It is not currently covered. If "when" matters for compliance, put a timestamp in the payload *and* anchor chain heads externally, for example with RFC 3161 timestamps, which C2PA also uses. HANDOFF lists external anchoring as not implemented.

**Caveat.** The chain proves the record was not altered after writing. It does **not** prove that a client's claim ("typed by a human") was true. That is why the attestation level is needed.

### 6.5 Offline Word, Excel and PowerPoint round trip [I, grounded in S60 and S35]

- **What comes back.** The push computes a diff against the starting revision.
  - *Aligned, unchanged* text produces `Retain`, so origin is preserved (rule 15).
  - *Changed or new* text is `imported`, attestation `none`, with the uploader as responsible actor (rule 16).
  - Dynodoc cannot tell whether a person, Copilot in Word, or a macro produced it.
- **Failed alignment.** When a block's bookmark is lost and content alignment falls below threshold (for example a "remove + add" of a heavily rewritten row or slide), the origin link **orphans** to `imported`. Do not guess, following Atlas.
- **Word tracked changes as a hint.** A `w:ins` or `w:del` author string, possibly "Copilot" (unverified), could be kept as an *unverified hint* in the import report, never as origin.
- **Export.** Optionally embed:
  - a provenance part (a custom XML part or `docProps` custom property referencing the Dynodoc revision), and
  - a signed **C2PA manifest** (OOXML collection hash; `c2pa.actions` with `digitalSourceType`; textual regions from the origin projection; `c2pa.ai-disclosure`).

  Expect Word to discard or invalidate it on save. A returning file whose manifest no longer validates proves only that it was modified after export.
- **Excel.** Origin should be per cell, since cells are replaced wholesale. **PowerPoint:** per text box or shape, with the same rules.

### 6.6 Performance and storage [I, grounded in S60]

- **Origin in patches.** One interned descriptor table per patch (tens of bytes) plus an optional small integer per insert: negligible next to `styles`.
- **Projection.** Run-length intervals per node, typically a handful per paragraph. For the 8,000-paragraph benchmark document, assuming two to three runs per paragraph, that is on the order of 20,000 runs, roughly 100–300 KB as compact JSON [I, estimate]. Cache it in snapshots, alongside the node Merkle commitment, so replay is bounded by the snapshot tail. Recomputation is linear in the number of operations; interval arithmetic adds little to the existing Myers step.
- **Merge.** Mapping origins through three-way merge is interval splicing along the merged patch: O(size of the ops).
- **Transcripts.** Coding-agent transcripts can be large (size not measured here [U]). Store them compressed in Postgres (`bytea`/TOAST, capped like `document_upload` at 20 MB). MinIO is out of PoC scope. Keep only digests in the chain.
- **Laundering detection.** Store one MinHash signature (512 bytes) plus block hashes per agent output block, reusing `engine_core::similarity`. A paste check is a band lookup plus about 0.02 ms per comparison, per the existing benchmark.
- **Concurrency.** No CRDT. Agents write to drafts, never the team version, so they do not contend with humans on the per-document lock. Rate-limit agent `draft_edit` appends per session. The patch guards (10,000 edits per patch) bound single agent turns.

### 6.7 Privacy and scope [I]

- **Scope.** Decision 000 and AGENTS.md exclude "LLM summary" and ML services from the PoC. Recording provenance for *external* agents via MCP does not require Dynodoc to call an LLM. Any Dynodoc-run generation, or "chat with a checkpoint", needs a new decision record.
- **Privacy.**
  - Transcripts may quote other documents the agent read. Restrict transcript access to people who can open every referenced document, or store only redacted excerpts.
  - Follow decision 009's pattern of storing identifiers and joining names at read time, so account erasure propagates.
  - Consider showing origin colouring to editors and reviewers by default rather than to public viewers. This reflects the stigma concern OpenAI raised, and the Article 50(4) review exception.

---

## 7. Sources

All accessed 30 Sep 2026 unless noted.

| # | Title | Publisher / author | Date | URL | Supports |
|---|---|---|---|---|---|
| S1 | Atlas README | pacifio (GitHub) | repo created 14 May 2026; read 30 Sep 2026 | https://github.com/pacifio/atlas | What Atlas records, the three claims, storage, agents, license, telemetry |
| S2 | `atlas-checkpoint/src/lib.rs`, `model.rs`, `capture.rs` | pacifio | main @ 30 Sep 2026 | https://github.com/pacifio/atlas/tree/main/crates/atlas-checkpoint/src | No hooks; SQLite plus 64 KB spill; redaction; thinking mode |
| S3 | `atlas-checkpoint/src/checkpoint.rs` | pacifio | main @ 30 Sep 2026 | https://github.com/pacifio/atlas/blob/main/crates/atlas-checkpoint/src/checkpoint.rs | Link rule, consumed touches, patch-id reconciliation, orphaning, 200-commit limit |
| S4 | `atlas-checkpoint/src/sketch.rs` | pacifio | main @ 30 Sep 2026 | https://github.com/pacifio/atlas/blob/main/crates/atlas-checkpoint/src/sketch.rs | Bottom-k line sketch, containment ≥ 0.5 |
| S5 | `atlas-checkpoint/tests/rewrites.rs` | pacifio | main @ 30 Sep 2026 | https://github.com/pacifio/atlas/blob/main/crates/atlas-checkpoint/tests/rewrites.rs | Tested amend, rebase, squash and cherry-pick behaviour |
| S6 | GitHub REST API: repo, releases, commits, contributors | GitHub | 30 Sep 2026 | https://api.github.com/repos/pacifio/atlas | Stars, forks, created date, releases, commit count |
| S7 | Atlas docs: Timeline | tryatlas.cc | accessed 30 Sep 2026 | https://docs.tryatlas.cc/docs/source-control/timeline | Link rule as documented for users |
| S8 | Entire CLI README | entireio | nightly 30 Sep 2026 | https://github.com/entireio/cli | Hooks, trailer, per-checkpoint refs, blame/why, supported agents |
| S9 | Entire `docs/architecture/attribution.md` | entireio | accessed 30 Sep 2026 | https://github.com/entireio/cli/blob/main/docs/architecture/attribution.md | Attribution trailer; rejected position-aware tracking; LIFO heuristic |
| S10 | Entire `docs/KNOWN_LIMITATIONS.md` | entireio | accessed 30 Sep 2026 | https://github.com/entireio/cli/blob/main/docs/KNOWN_LIMITATIONS.md | Amend `-m` trailer handling; legacy `entire/checkpoints/v1` |
| S11 | Entire `checkpoint-signing.md` | entireio | accessed 30 Sep 2026 | https://github.com/entireio/cli/blob/main/docs/architecture/checkpoint-signing.md | Best-effort GPG/SSH signing |
| S12 | "AI Code Transparency Platform Entire Unveiled By Ex-GitHub CEO With $60M Seed" | OpenSourceForU | 11 Feb 2026 | https://www.opensourceforu.com/2026/02/ai-code-transparency-platform-entire-unveiled-by-ex-github-ceo-with-60m-seed/ | Launch, funding, initial agents [S] |
| S13 | Git AI README | git-ai-project | v1.7.5 (9 Sep 2026) | https://github.com/git-ai-project/git-ai | Git notes, rewrite handling, no heuristics |
| S14 | Git AI Standard v3.0.0 | git-ai-project | accessed 30 Sep 2026 | https://github.com/git-ai-project/git-ai/blob/main/specs/git_ai_standard_v3.0.0.md | Authorship log schema; rebase, squash and amend rules |
| S15 | Agent Trace specification v0.1.0 (RFC) | Cursor and partners | January 2026 | https://agent-trace.dev/ | Trace record schema, contributor types, storage-agnostic, open rebase question |
| S16 | "Agent Trace: Capturing the Context Graph of Code" | Cognition | 29 Jan 2026 | https://cognition.com/blog/agent-trace | Multi-vendor support |
| S17 | Cursor Blame docs | Cursor | accessed 30 Sep 2026 (2.4 release reportedly 22 Jan 2026 [S]) | https://cursor.com/docs/integrations/cursor-blame | Tab/Agent/Human line attribution; Enterprise |
| S18 | "Trace any Copilot coding agent commit to its session logs" | GitHub Changelog | 20 Mar 2026 | https://github.blog/changelog/2026-03-20-trace-any-copilot-coding-agent-commit-to-its-session-logs/ | `Agent-Logs-Url` trailer; Copilot author with human co-author |
| S19 | About agent management | GitHub Docs | accessed 30 Sep 2026 | https://docs.github.com/en/copilot/concepts/agents/cloud-agent/agent-management | Session logs |
| S20 | Claude Code settings reference (`attribution`) | Anthropic | accessed 30 Sep 2026 | https://code.claude.com/docs/en/settings-reference | Commit trailer; `includeCoAuthoredBy` deprecated since v2.0.62 |
| S21 | Claude Code: Manage sessions | Anthropic | accessed 30 Sep 2026 | https://code.claude.com/docs/en/sessions | JSONL transcript location; internal format |
| S22 | Aider: Git integration | aider.chat | accessed 30 Sep 2026 | https://aider.chat/docs/git.html | "(aider)" suffix; co-authored-by option |
| S23 | "WTF: Microsoft forces 'Co-Authored-by Copilot' in commits" | heise online | 3 May 2026 | https://www.heise.de/en/news/WTF-Microsoft-forces-Co-Authored-by-Copilot-in-commits-11279554.html | `git.addAICoAuthor` controversy [S] |
| S24 | vscode issue #314311 | Microsoft (GitHub) | 5 May 2026 | https://github.com/microsoft/vscode/issues/314311 | Default reverted to `off` in 1.119 |
| S25 | SpecStory docs | SpecStory | accessed 30 Sep 2026 | https://docs.specstory.com/specstory/introduction | `.specstory/history` Markdown capture [S] |
| S26 | MCP specification 2026-07-28 and changelog | MCP project | 2026-07-28 | https://modelcontextprotocol.io/specification/2026-07-28/changelog | Stateless protocol, no sessions, MRTR, deprecations |
| S27 | MCP versioning | MCP project | accessed 30 Sep 2026 | https://modelcontextprotocol.io/specification/versioning | 2026-07-28 is "Current" |
| S28 | MCP tools page and `schema.ts` (2026-07-28) | MCP project | 2026-07-28 | https://modelcontextprotocol.io/specification/2026-07-28/server/tools | Tool annotations untrusted; output schema |
| S29 | MCP authorization (2026-07-28) | MCP project | 2026-07-28 | https://modelcontextprotocol.io/specification/2026-07-28/basic/authorization | OAuth 2.1, RFC 9728 |
| S30 | Notion MCP supported tools | Notion | accessed 30 Sep 2026 | https://developers.notion.com/docs/mcp-supported-tools | Tool list; find-and-replace edits |
| S31 | Google Docs MCP reference | Google | updated 2026-07-10 | https://developers.google.com/workspace/docs/api/reference/mcp | `read_doc` / `update_doc`; batch operations; no create-suggestion |
| S32 | Word reference (preview) | Microsoft Learn | 3 Aug 2026 | https://learn.microsoft.com/en-us/microsoft-agent-365/mcp-server-reference/word | Create, read and comment only |
| S33 | OpenTelemetry GenAI semantic conventions (repo docs) | OpenTelemetry | repo created 5 May 2026; read 30 Sep 2026 | https://github.com/open-telemetry/semantic-conventions-genai | "Development" status; agent and tool spans; opt-in content |
| S34 | PROV-O: The PROV Ontology | W3C | 30 Apr 2013 | https://www.w3.org/TR/prov-o/ | Classes and properties |
| S35 | C2PA Technical Specification 2.4 | C2PA | April 2026 (2.3: December 2025) | https://spec.c2pa.org/specifications/specifications/2.4/specs/C2PA_Specification.html | Text embedding, OOXML, regions, ai-disclosure, soft binding, crJSON |
| S36 | IPTC Digital Source Type vocabulary | IPTC | modified 23 Oct 2024 (per page) | https://cv.iptc.org/newscodes/digitalsourcetype/ | Terms and definitions |
| S37 | SynthID (Responsible GenAI Toolkit) | Google AI for Developers | updated 2025-04-09 | https://ai.google.dev/responsible/docs/safeguards/synthid | Robustness and limits; Transformers v4.46.0+ |
| S38 | "Scalable watermarking for identifying large language model outputs" | Dathathri et al., *Nature* 634:818–823 | 23 Oct 2024 | https://www.nature.com/articles/s41586-024-08025-4 (abstract via PubMed 39443777) | SynthID-Text; about 20M Gemini responses |
| S39 | "Understanding the source of what we see and hear online" (update) | OpenAI; reported by cdotrends and thurrott | 4 Aug 2024 | https://openai.com/index/understanding-the-source-of-what-we-see-and-hear-online/ | Text watermark decision [S; primary returned 403] |
| S40 | EU AI Act, Article 50 | artificialintelligenceact.eu (text of Reg. 2024/1689) | accessed 30 Sep 2026 | https://artificialintelligenceact.eu/article/50/ | Obligations and exceptions |
| S41 | Code of Practice on transparency of AI-generated content | European Commission | final 10 Jun 2026 | https://digital-strategy.ec.europa.eu/en/policies/code-practice-ai-generated-content | Final code; application from 2 Aug 2026 |
| S42 | "The EU's AI Transparency Code of Practice, Explained" | Tech Policy Press | 24 Jun 2026 | https://www.techpolicy.press/the-eus-ai-transparency-code-of-practice-explained/ | Layered marking; editing exclusions [S] |
| S43 | "Digital AI Omnibus Delays Key Deadlines…" | Cooley | 3 Aug 2026 | https://cdp.cooley.com/digital-ai-omnibus-delays-key-deadlines-introduces-new-rules/ | Reg. 2026/1744; 2 Dec 2026 grace [S] |
| S44 | "Vibe working: Introducing Agent Mode and Office Agent in Microsoft 365 Copilot" | Microsoft | 29 Sep 2025 | https://www.microsoft.com/en-us/copilot/blog/2025/09/29/vibe-working-introducing-agent-mode-and-office-agent-in-microsoft-365-copilot/ | Agent Mode in Word and Excel |
| S45 | "Microsoft Copilot in Word can now track changes and manage comments" | The Decoder | 15 Apr 2026 | https://the-decoder.com/microsoft-copilot-in-word-can-now-track-changes-and-manage-comments/ | Word-level tracked changes by Copilot [S] |
| S46 | "New Gemini capabilities in Google Docs…" | Google Workspace Updates | April 2026 | https://workspaceupdates.googleblog.com/2026/04/new-gemini-capabilities-in-google-docs-help-you-go-from-blank-page-to-brilliance.html | Private suggested edits until approved |
| S47 | "Google Docs upgrades now let you co-edit with Gemini" | 9to5Google | 10 Mar 2026 | https://9to5google.com/2026/03/10/google-docs-gemini-upgrade/ | Co-editing [S] |
| S48 | Notion 3.0 release | Notion | 18 Sep 2025 | https://www.notion.com/releases/2025-09-18 | Agents' scope |
| S49 | ChatGPT canvas help and model release notes | OpenAI (via search snippets) | canvas removal reported 28 May 2026 | https://help.openai.com/en/articles/9930697 | Canvas diffs and versions; removal [S] |
| S50 | "Claude Code now supports artifacts" and artifact help | Anthropic (via search) | accessed 30 Sep 2026 | https://claude.com/blog/artifacts-in-claude-code | Version history per publish [S] |
| S51 | About Grammarly Authorship | Grammarly | accessed 30 Sep 2026 | https://support.grammarly.com/hc/en-us/articles/29548735595405-About-Grammarly-Authorship | Origin categories [S via search summary] |
| S52 | "How Grammarly Launders AI-Generated Content" | Jonathan Bailey, Plagiarism Today | 6 Nov 2025 (updated 12 Nov 2025) | https://www.plagiarismtoday.com/2025/11/06/how-grammarly-launders-ai-generated-content/ | Humanize labelled as "typed" |
| S53 | "Grammarly Updates Authorship, Improves Labeling" | Jonathan Bailey, Plagiarism Today | 11 Dec 2025 | https://www.plagiarismtoday.com/2025/12/11/grammarly-updates-authorship-improves-labeling/ | New "AI-Generated or Rephrased with AI" category |
| S54 | "WikiWho: Precise and Efficient Attribution of Authorship of Revisioned Content" | Flöck & Acosta, WWW 2014 | April 2014 | https://archives.iw3c2.org/www2014/proceedings/proceedings/p843.pdf | 95% precision; reintroductions and moves |
| S55 | git-blame and git-diff documentation | git-scm.com | accessed 30 Sep 2026 | https://git-scm.com/docs/git-blame and https://git-scm.com/docs/git-diff | `-M` / `-C`, `--ignore-rev`, diff algorithms |
| S56 | "Peritext: A CRDT for Rich-Text Collaboration" | Litt, Lim, Kleppmann, van Hardenberg (Ink & Switch; CSCW 2022) | November 2022 | https://www.inkandswitch.com/peritext/ | Mark expansion and anchors |
| S57 | y-simple-attribution-server README; Yjs releases | Yjs (Kevin Jahns) | repo Nov 2025; `v14.0.0-rc.28` 29 Sep 2026 | https://github.com/yjs/y-simple-attribution-server | User or AI-agent attribution; v14 still RC |
| S58 | "07 · AI bots in version control" | Ink & Switch Patchwork notebook | 19 Mar 2024 | https://www.inkandswitch.com/patchwork/notebook/2024-version-control/07/ | Bot branches, partial merge, bot shown in history |
| S59 | "CoAuthor: Designing a Human-AI Collaborative Writing Dataset" | Lee, Liang, Yang (CHI 2022) | 2022 | https://arxiv.org/abs/2201.06796 | Keystroke-level human/AI logs [S via search summary] |
| S60 | Dynodoc repository (`docs/HANDOFF.md`, `docs/decisions/009-…md`, `engine-shared/src/payload.rs`, `richtext.rs`, `engine-core/src/log.rs`, `richtext.rs`, migrations 0002, 0004, 0012, 0020, 0021, `frontend/…/docx-import.ts`) | Dynodoc | main @ 35210d9, 30 Sep 2026 | this repository | Everything about Dynodoc's current model |

---

## 8. Claims from the owner's pasted chat

| Claim | Verdict | Evidence and nuance |
|---|---|---|
| Atlas is "Source control for agents. Use multiple coding agents, track their changes and query them in one place" | **Verified** | The GitHub repository description, verbatim [S6]; the README says "Source control for coding agents" [S1]. |
| "Commits are observed rather than intercepted" | **Verified** | README, verbatim [S1]. `lib.rs`: "No git hooks … Watching refs move can" [S2]. The mechanism is a ref watcher plus a walk from a stored cursor when the project opens, with a 200-commit re-scan if the cursor is lost [S3]. |
| "Links re-point through amend and rebase by patch-id reconciliation" | **Verified, with a qualification** | README, verbatim [S1]. Code uses `git patch-id --stable` over the most recent 200 commits on all refs [S3]. It re-points only when the rewritten diff is identical: a message-only amend or a conflict-free rebase. A content-changing amend or a differently resolved conflict orphans instead. Tests cover the message-only amend and the differently resolved conflict [S5]. |
| "When a squash makes a link ambiguous it orphans instead of guessing" | **Verified, with a nuance** | README: "When a squash makes the link genuinely ambiguous, it orphans instead of guessing" [S1]. In code, a squash yields a patch-id that matches none of the originals, so no candidate is found and the checkpoint orphans. True ambiguity (several commits with the same patch-id, for example a cherry-pick) also orphans unless the recorded branch disambiguates. Orphans keep their session link and re-link if the commit becomes reachable again [S3, S5]. |
| (Implied) Atlas records prompts, tool calls, reasoning and diffs | **Verified** | "prompts, messages, tool calls, the files each one touched, and the patches it applied"; `thinking` message mode [S1, S2]. |
| (Implied) Atlas attributes individual lines or spans to agents | **Not a feature, so the implication is wrong** | Linking is per (session, commit). An existing file links on path alone, so human edits in the same commit are covered by the same link. The editor's blame is ordinary git blame [S3; `blame-inline.ts`]. |

---

## 9. Open questions

1. **Scope.** Is agent provenance (recording external agents via MCP) acceptable inside the PoC? Or should Dynodoc-run generation, "chat with a checkpoint" and LLM-backed features wait for a decision record that amends decision 000?
2. **Identity model.** Should agent identities be per (principal, product) or per session? This affects the `actor_id` that is hashed. Who may create or revoke them?
3. **Display.** Should reviewers see origin at character, word or sentence granularity? Should public viewers ever see AI colouring? This involves privacy and the stigma concern.
4. **Transcripts.** What retention, redaction and visibility rules apply? Must the same access rules as the documents the agent read also cover the transcript?
5. **Legal.** Would Dynodoc be an Article 50(2) *provider* if it integrates a model, and does the "assistive function for standard editing" exemption cover planned features? The Omnibus grace-period text needs confirming on EUR-Lex [U].
6. **Word hints.** What author string does Word write for Copilot's tracked changes (`w:ins/@w:author`)? [U] If it is stable, imports could keep it as an unverified hint.
7. **External anchoring.** Should chain heads be timestamped or anchored externally (RFC 3161 or similar) so the provenance record resists a privileged operator?
8. **C2PA exports.** Is signed DOCX/PDF export with a C2PA manifest worth obtaining a signing certificate for, given Word discards manifests on edit?
9. **MCP compatibility.** Most clients in the field may still speak 2025-11-25 (with sessions). Should a Dynodoc server support both handshake and stateless modes during the transition?
10. **Interchange.** Should Dynodoc export Agent Trace-style records (adapted from line ranges to `node_id` plus scalar ranges) or Git AI notes for interoperability? Agent Trace's repository was unreachable (404) on 30 Sep 2026, so its governance status is unclear.
11. **Legacy content.** Should text created before the feature stay `unknown`, or could an owner attest it as human through an explicit, audited event?
