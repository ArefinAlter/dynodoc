//! The read path and the operation compiler of the Dynodoc AI-agent connector, as pure
//! functions.
//!
//! This is the `engine-agent` crate of `docs/AI-AGENT-CONNECTORS.md` section 3.1, the
//! "refs, hashes, DCT writer, outline, read, find, diff, escaping" task of its section 11.2
//! and the "op compiler" task of its section 11.3. It implements the read tools and the
//! compile step of `propose` of `docs/AI-AGENT-PROTOCOL-SPEC.md` (`dynodoc.agent/1`) over
//! [`engine_core::materializer::DocumentState`]: no HTTP, no database, no MCP, no I/O, no
//! clock, no randomness (new node ids come from a caller-supplied [`compile::IdSource`]).
//! Given the same state and the same arguments every function returns the same bytes. Maps
//! are ordered, so iteration order never shows in the output.
//!
//! # What is here
//!
//! | Spec | Module | |
//! |---|---|---|
//! | 5.2 revision tokens | [`rev`] | [`rev::RevToken`] `"<rev>:<chain8>"` ETag form |
//! | 5.3 refs, sub-addresses | [`refs`] | [`refs::Ref`], [`refs::RefIndex`], [`refs::Address`], [`refs::SlideSel`] |
//! | 5.4 hashes | [`hash`] | [`hash::Hash`], [`hash::ShortHash`], block, row, range, shape, notes and outline hashes |
//! | 5.5 sheet addressing | [`a1`] | [`a1::CellRef`], [`a1::A1Range`] |
//! | 6.2 header, 6.1 grammar | [`dct`] | [`dct::HeaderBuilder`], [`dct::parse_header`], [`dct::split_response`] |
//! | 6.5 escaping | [`escape`] | [`escape::escape`], [`escape::escape_line`], [`escape::unescape`] |
//! | 6.3 to 6.9 text | [`text`], [`sheet`], [`slides`] | block text, column profile, slide text |
//! | 6.10, 11 budgets | [`budget`], [`paging`] | byte budget, cursors, paged responses |
//! | 7.2 `files` | [`files`] | [`files::files`] over caller-supplied metadata |
//! | 7.3 `outline` | [`outline`] | [`outline::outline`] |
//! | 7.4 `read` | [`read`] | [`read::read`] |
//! | 7.5 `find` | [`find`] | [`find::find`] |
//! | 7.6 `diff` | [`diff`] | [`diff::diff`] |
//! | 8.2 position keys | [`pos`] | [`pos::between`], the Rust port of the editor's function, and [`pos::key_between`] |
//! | 7.7, 8 operation grammar | [`grammar`] | [`grammar::parse_propose`], [`grammar::parse_ops`], [`grammar::Op`] |
//! | 8.2 to 8.5, 9 compile | [`compile`] | [`compile::compile`], [`compile::CompileInput`], [`compile::Compiled`] |
//! | 7.7, 8.7 dry run | [`preview`] | [`preview::preview`] |
//! | 10 errors | [`error`] | [`error::AgentError`] (26 codes), [`error::CompileError`] (an error and the op that caused it) |
//! | 12.3 JSON mirror | [`response`] | [`response::Response::json_mirror`] |
//! | 17 conformance | [`parse`], [`corpus`], [`fixtures`] | strict body parsers, the golden call list, the fixture corpus |
//!
//! Every tool returns a [`response::Response`]: the DCT `text` for the model, the `meta`
//! of `_meta["dynodoc/v1"]` (`rev`, `chain`, `file`, `cursor`, `budget`, `ids`) for
//! programmatic clients, and a typed `data`. The caller supplies the state at the wanted
//! revision, a [`model::FileMeta`] (document id, kind, `rev`, hex chain hash; the state does
//! not carry them) and, for `files`, the already authorised [`files::FileEntry`] list.
//!
//! Of `propose` only the compile step is here: parse the ops, turn them into the engine
//! operations `edit_draft` would store, check them against the state and the role, and
//! render the preview of a dry run. Out of scope, and not here: drafts, receipts and the
//! idempotency ledger, agent identities, `submit`, `status`, the state cache, agent routes
//! and authorisation, migrations, the MCP tool, the application.
//!
//! # The compiler in one paragraph
//!
//! `grammar::parse_ops` makes typed [`grammar::Op`]s from the JSON of `propose.ops` or fails
//! with the field named. `compile::compile` takes them with a [`compile::CompileInput`] (the
//! state the ops apply to, the team's current state for staleness, the role, `strict`, the
//! blocks to withhold) and an [`compile::IdSource`], resolves refs with the same
//! [`refs::RefIndex`] the read tools use, and runs every produced engine op through
//! `governance::authorize`, the `redundant_label` skip and `richtext::compact_event`, then
//! `materializer::apply_payload` on a working copy, so the next op sees the last one and the
//! list that comes out applies as it is. Text edits rebuild the container from its runs and
//! let `richtext::make_patch` derive and check the patch. The call is atomic: the first op
//! that fails fails the call, and the error names the op. `preview::preview` writes the
//! `dd/1 propose` response from the compiled call, bounded by `max_bytes`.
//!
//! # Verification
//!
//! * `tests/golden/*.dct`: the exact output of every tool call over the fixture corpus
//!   (a document, a workbook and a deck built with real engine operations), regenerated with
//!   `UPDATE_GOLDEN=1`. Each transcript is also parsed with the strict parsers of [`parse`].
//! * `tests/hostile.rs`: for the committed injection corpus (`tests/corpus/injection.json`)
//!   and for generated hostile text (fixed seeds), every tool's output parses back to exactly
//!   the structure that was written, and the text comes back as the sanitised original.
//! * `tests/props.rs`, `tests/conformance.rs`: round trips for refs, addresses, hashes,
//!   revision tokens, cursors and A1 ranges; minimal-suffix laws; the error table; paging and
//!   exact byte-budget boundaries; shared vectors under `tests/vectors/`; a check that the
//!   source holds no clock, randomness, unordered map or I/O.
//! * `examples/byte_benchmark.rs`: DCT size against the JSON mirror.
//! * `tests/position.rs` with `tests/vectors/between.json`: 10,000 pairs and five long chains
//!   of position keys, produced by running the real `between()` of
//!   `frontend/researcher/src/lib/workspace/changes.ts` (`tests/vectors/between-gen.mjs`
//!   transpiles it with the installed TypeScript compiler and runs it; `--check` compares a
//!   fresh run with the committed file). The Rust function reproduces every one; order and
//!   length laws are property-tested, including the 200-position boundary.
//! * `tests/compile.rs`: every op against the fixture corpus, each rejection of spec 10 the
//!   compiler can raise, atomicity, staleness, roles, redaction, id handling, the engine op cap.
//!   Every compiled list is replayed with `authorize` and `apply_payload` and must give the
//!   state the compiler reports, and must round-trip through `workspace_merge::diff` without
//!   an op on a node the compile did not touch.
//! * `tests/compile_differential.rs`: fixed-seed random edit scripts (60 document scripts of 12
//!   calls, 40 sheet scripts, `replace_all` against its per-block expansion, `move`) checked
//!   against a reference model of styled characters and grid cells, plus alignment-optimality
//!   of the styles `set_text` keeps. `inline.rs` has the same kind of tests for the text model.
//! * `tests/compile_hostile.rs`: huge counts, deep and malformed refs, the injection corpus and
//!   generated hostile text through every op that carries text (stored as data or refused, and
//!   every later `read`, `outline` and preview parses back), the vocabulary of engine ops, the
//!   things an op must not reach, an untouched input state after a failed call, and a property
//!   test over random, partly corrupted ops (no panic; what compiles applies).
//!   `DIFFERENTIAL_SCALE=25` and `PROPTEST_CASES` run the random tests longer.
//! * `tests/preview.rs`: the preview header and body, the dry-run form, and the byte bound at
//!   every budget from 500 to 64,000.
//!
//! # Decisions and deviations from the specification
//!
//! Where the spec says "proposed" the detail was settled here; where the spec contradicts
//! itself or the code, the code and the stricter rule won. Each item is a deviation or a
//! choice a reader of the spec could not have predicted.
//!
//! **Grammar and header (6.1, 6.2, 6.5)**
//!
//! 1. The header `key` allows a leading `~` (`~tok`), which the spec's own grammar forbids.
//!    A value is written bare only from `[A-Za-z0-9_.:/!+~@%,-]`; everything else is quoted,
//!    so no value can hold a space, `=` or `"` that a reader would take for structure.
//! 2. The header never holds document text (6.5). The spec examples that do conflict with
//!    that rule: `sheet="Budget"` moves to a first body line `sheet "Budget"`, and a range
//!    read echoes the sheet by ref (`range=R91W!A1:E4`, not `Budget!A1:E4`). `q=` echoes the
//!    caller's own query, always quoted.
//! 3. `~tok` is `ceil(bytes / 4)` of the whole response including the `~tok` pair itself,
//!    found by iteration. It is an estimate and is never used as data.
//! 4. `more` is absent when a response is complete (6.2 and 6.10), not `more=-` (7.2 example).
//!    A cursor is `c_<rev hex>.<position hex>[.<aux hex>]`: not secret, not signed, no
//!    authority; every call re-authorises. `diff` carries the `from` revision as `aux`.
//!
//! **Escaping (6.5, AI-AGENT-SECURITY P1 to P3)**
//!
//! 5. Backslash, LF, TAB, `⟦` and `⟧` are escaped everywhere, so escaped text holds no LF
//!    and no TAB (the spec escapes TAB only in sheet cells). A line that starts a text line
//!    gets a leading `\` when its first character is `@` (always), or `#`, `=`, `-` alone or
//!    before a space. The spec text reads "line starting with `@`, `#`, `=` or `-` followed by
//!    space"; `@` is unconditional because AC12's `@AB12 P #000000` has no space. Slide text
//!    also guards `.`; `diff detail=text` also guards `~`, `+`, `>`, `-`.
//! 6. Dropped: every control character (Unicode `Cc`, which includes DEL and the C1 range;
//!    the spec says U+0000..U+001F) except LF and TAB, counted in `warn=ctrl`; and the
//!    characters of AI-AGENT-SECURITY P3 that hide or reorder text (U+200B, U+2060..U+2064,
//!    U+FEFF, U+202A..U+202E, U+2066..U+2069, U+E0000..U+E007F), counted in `warn=hidden`.
//!    Kept on purpose: U+200C and U+200D (Bengali, Persian and emoji sequences need them) and
//!    the directional marks U+200E, U+200F, U+061C. U+0085, U+2028 and U+2029 become `\n`.
//!    So `unescape(escape(x)) == sanitize(x)`, not `x`.
//! 7. Quoted values escape `"` too; `fmt=md` text also escapes `* _ ` ~ [ ]`; inline diffs
//!    escape `[ ] { }` so only the server writes `[-`, `-]`, `{+`, `+}`. Markdown link
//!    targets are written only for `http`, `https`, `mailto` and relative URLs, percent-encoded.
//!
//! **Blocks, refs and hashes (5, 6.3, 6.4, 6.6)**
//!
//! 8. A block is a live child of the root `form` whose `content` is an object (the set
//!    `/structure` lists), in `(pos, id)` order. Nodes whose id is not 4 to 26 ASCII
//!    alphanumerics have no ref and are not listed; real ULIDs always qualify.
//! 9. Types: `UL<n>`/`OL<n>` count every item at every depth (the table of 6.3 was followed;
//!    its examples show `LI3`, which no real content produces); `TBL<r>x<c>` takes `c` from
//!    the widest row, a merged cell appears once; `LI` only for a top-level `listItem`;
//!    anything unknown is `X` with `rich=1`. `m`, `lnk`, `cm` count text runs. `hardBreak`
//!    reads as `\n` although `plain_text` drops it. Words and `find` use plain text, in which
//!    an atom contributes what `plain_text` gives (math: its LaTeX).
//! 10. `ref.N` counts list items depth-first, the order of the rendered lines. `ref.rRcC`
//!     counts physical cells. A sub-address read returns the text of the item or the whole
//!     cell (all its paragraphs, joined by `\n`) with the hash of the whole block. A tail
//!     spelled like a cell (`R1C1`) parses as one and is reinterpreted by node type. A slide
//!     label is lowercase `s` plus digits; a ref that happens to look like one is tried as a
//!     ref when the position does not exist.
//! 11. `oh` is the fingerprint of the JSON string `"REF:h6,REF:h6,...;n=<count>"` over the
//!     selection before paging, whatever the depth; the spec says SHA-256 over the
//!     concatenation, and this keeps one hash primitive (`richtext::fingerprint`). Row, range,
//!     shape and notes hashes are fingerprints of `{"cell_0":..}`, the 2-D array,
//!     `{"kind","role","text"}` and `{"notes"}`; see `tests/vectors/hashes.json`.
//!
//! **Tools**
//!
//! 12. `outline` depth 0 appends `[<n>b <w>w]` for the blocks and words after the heading up
//!     to the next heading of the same or higher level; the header gains `depth=`. For a
//!     spreadsheet depth 1 answers with the profile of one sheet (the 6.7 example) instead of
//!     appending it to the sheet list (7.3). The profile header gains `h=` (sheet hash);
//!     `header=` is the frozen row count, `guess` or `0`; `cols` is the highest non-empty
//!     column plus one, not the stored `columns` width; `pattern` and `top` values are quoted;
//!     `distinct` is omitted for numeric and formula columns; `exceptions` lists at most 10
//!     rows, then `+N`. All statistics are over stored strings (`calc=none`).
//! 13. `read` of a range clamps to the used cells and applies the 2,000-cell limit to that
//!     size. `fmt=md` cell format annotations (6.7) are not implemented. Slide shapes list
//!     their text as physical lines; the notes line is written only when notes are not empty.
//!     A read never returns half a block; a block that alone exceeds `max_bytes` is
//!     `limit_exceeded field=max_bytes`, not an empty page with a cursor that never advances.
//! 14. `find` reports one hit per match, document order, the hits of one block in a row (that
//!     order is the later `nth`). `total` stops at 10,000 scanned matches (`total=10000+`).
//!     Hits in sheets read `REF!C14` with the sheet ref, in lists `ref.N`, in tables
//!     `ref.rRcC`. `max_bytes` is accepted although the 7.5 schema omits it.
//! 15. `diff` takes its candidates from `workspace_merge::diff`, then compares nodes by
//!     hash. An entry and its detail lines form one unit (the spec reads as all summaries
//!     first). `>` marks a moved block. Inline diffs keep 3 words of context. Refs come from
//!     the union of both states so they resolve in each. Spreadsheet rows and slides/shapes
//!     use the same line grammar as an extension (geometry, style and sheet renames are not
//!     reported).
//! 16. `files` orders by title then id and filters by project UUID, title substring and
//!     kind. Redaction is the caller's: a set of node ids, `@ref X redacted`, `warn=redacted`;
//!     the call fails only when every selected block is withheld.
//! 17. `AgentError` carries typed payloads only (refs, hashes, counts, field names), never
//!     free text, with one fixed hint per code. `limit_exceeded` is 413 when `field` is a byte
//!     size (`max_bytes`, `body`, `text`), else 400.
//!
//! **The operation compiler (7.7, 8, 9)**
//!
//! 18. *Signature.* `compile(&CompileInput, &[Op], &mut dyn IdSource) -> Result<Compiled,
//!     CompileError>`. [`error::CompileError`] is an `AgentError` plus the index and label of
//!     the op, because 7.7 wants the error to name the op; it converts to and from
//!     `AgentError`. The input holds one state (the proposal's current state, or the state at
//!     `rev` for a new proposal) that refs resolve against and ops apply to, an optional
//!     `head` (the team's current state) for the `base` comparison, the role and `strict`.
//!     A ref made by an earlier op of the same call is `$n`, numbered in creation order across
//!     all inserts (the spec says "the nth insert", which is the same until an insert makes
//!     more than one block).
//! 19. *What comes out is what `edit_draft` stores.* Every engine op is authorised with the
//!     given role, a `label` edit equal to the current label is dropped and a whole-`content`
//!     edit becomes a `RichTextPatched` when that is smaller. So a short paragraph edit is
//!     `FieldEdited content` plus `FieldEdited label` (the digests make a patch larger than the
//!     paragraph) and a long one is one patch: spec 8.2 says "one `RichTextPatched` (or
//!     `FieldEdited` when not smaller)". `set_type` is one `props` edit at path `[]` when that is
//!     smaller than the whole field, else a whole-field edit. A call may produce 10,000 engine ops; `edit_draft`
//!     takes 5,000 per request, so a caller that stores more splits the list.
//! 20. *`base` and staleness.* A `base` of 6 to 64 hex characters is accepted (the spec says
//!     12; `read` shows 6). It is compared with the team's hash (`head`, else the input
//!     state) of the block, of the row range of `set_cells` and `clear` (the range hash), of the
//!     shape or of the slide notes. A difference is recorded in `Compiled::stale` and, with
//!     `strict`, fails the call as `stale_base` (or `conflict` when the target is gone from
//!     the team's state). `base` on `insert`, `replace_all` or a block made in the same call is
//!     `bad_request field=base`: nothing earlier exists to guard.
//! 21. *`replace` and `replace_all` are literal and case-sensitive by default.* `args.case`
//!     defaults to `true` here and means "match case" as in `find`, where the default is
//!     insensitive; a destructive edit should not change `policy` when asked for `Policy`, and
//!     `expect` catches a count that differs from `find`. `args.word` is whole words.
//!     `nth` counts the matches of one block in document order across its containers (list
//!     items, cells) like `find` reports them. An `expect` equal to the count of several
//!     matches without `nth` or `all` replaces all of them. Zero matches is `no_match`
//!     (`match_count` when `expect` says otherwise), so a second `replace_all` is an error,
//!     not a silent success. More than 25 matching blocks need `expect` (8.6), more than 5,000 are
//!     `limit_exceeded`. Withheld blocks are left out of the scope and the count, since the
//!     agent cannot have read them. A match never spans an inline object, and U+FFFC (which
//!     stands for one while matching) cannot be searched for. `find` is at most 2,000
//!     characters. The replacement takes the style of the first replaced character; the rule
//!     of 8.3 for `set_text` (the preceding retained character) would let a replaced word at the
//!     start of a bold or linked run lose the mark. A span replaced by identical text is left
//!     as it was.
//! 22. *`set_text`.* Plain only: `fmt=md` is `bad_request field=fmt` (an inline-markdown parser
//!     is the inverse of the `read fmt=md` renderer and is not written). A line feed becomes a
//!     `hardBreak` in paragraphs and headings (that is how `read` shows one) and a character in
//!     code blocks. A block with inline objects keeps them when the new text carries each
//!     `⟦...⟧` placeholder of `read` in order (spec 6.11), else `rich_block`. Alignment is the
//!     engine's own (Myers, one replacement past 4,000 changed characters), not the 20 ms
//!     deadline of the spec text. Sub-addresses `ref.N` and `ref.rRcC` edit the first
//!     paragraph of a list item or table cell that has exactly one.
//! 23. *Agent text is refused, not repaired.* `text`, `with`, `find`, cell values, new block
//!     text, `why` and the proposal `name` are normalised (CRLF, CR, NEL, LS, PS become LF) and
//!     then refused with `bad_request` when they hold a control character or a hidden or
//!     bidirectional character of 6.5: an agent may not write what a reviewer cannot see. (The
//!     security document says such proposals are "flagged"; refusing is the stricter reading.)
//!     U+200C and U+200D stay allowed. `id` is limited to `A-Za-z0-9_.:-` so it can be echoed in
//!     an error line; `why` is at most 280 characters.
//! 24. *Position keys.* [`pos::between`] returns, for every input, what the TypeScript function
//!     returns, except that a result with a lone surrogate (two stored keys with units on both
//!     sides of the surrogate block) has U+FFFD in its place. The original has two quirks
//!     that the compiler does not inherit: a key past the 200-position window is `left + "P"`,
//!     which can sort at or above `right`, and which grows by a character per call at the end
//!     of the list. [`pos::key_between`] refuses any key that is not strictly between its
//!     neighbours in `(pos, id)` order or is longer than 200 characters, as `limit_exceeded
//!     field=pos`. In practice that is a thousand inserts at one spot.
//! 25. *Sheets.* `set_cells` writes only changed cells; rows that do not exist are created below
//!     the last one up to the last row that holds a value (rows before it that hold none are
//!     blank), carrying their cells in the `NodeCreated` (one event per row, not per cell:
//!     the spec's "one `FieldEdited` per changed cell" is kept for existing rows). The sheet's
//!     `columns` grows to the widest written column. A formula must have balanced brackets
//!     outside quotes and is otherwise stored as written. At most 2,000 cells, 3,000 rows, 100
//!     columns and 100,000 scalars in all. `at` is `A1` or `rowRef:Col`. `clear` empties
//!     existing cells and keeps formats. `sheet_protected` gives `protected`.
//! 26. *Slides.* `set_shape` (text shapes only) writes `text`, the shape `label` and, for a
//!     title, the slide `label`, as the editor's `editShapeOps` does; `set_notes` writes
//!     `notes`.
//! 27. *Not compiled (`unsupported_op`)*: `fill` (needs the formula engine), `add_slide` and
//!     `insert into` a slide (need the layout geometry of `presentation.ts` in Rust),
//!     `delete_slide` and `delete sheet` (they remove a container and its children), `macro`
//!     and the v1.1 macros. Everything the 7.7 schema lists beyond Phase 1 is in that list.
//! 28. *Identity.* [`compile::DerivedIds`] makes ULIDs from a caller-supplied millisecond
//!     timestamp and a seed (the `request_id`), so a dry run and the real call, or a retry,
//!     name new blocks alike. A source that returns something that is not a ULID, or an id in
//!     use, fails the call as `internal`.
//! 29. *Preview.* The text is the `diff` of the state before and after the compiled ops with a
//!     `propose` header (`applied`, `would_apply`, `expanded`, `stale`, `stale_refs`, `state`,
//!     `revision`), cut at whole entries to fit `max_bytes` (default 8,000, 500 to 64,000).
//!     No new body line exists; per-op results and stale targets are in the typed data.
//!
//! # Not verified
//!
//! No TypeScript implementation reads the vectors yet (the TypeScript side produced the
//! position vectors and nothing consumes the hash and ref vectors). No model has been run
//! against this output, no real tokeniser was used (the benchmark estimates), and nothing here
//! was measured under load. The fixtures are built with real operations but not through the
//! event log, so the `chain` is a stand-in; the checkpoint-plus-tail reader is not exercised.
//! The compiler was run against `authorize`, `compact_event` and `apply_payload` as
//! `edit_draft` calls them, not against `edit_draft` itself or a database; no draft was stored
//! and no change request merged from its output. The token argument for compact ops (spec 8,
//! the token-efficiency document) is not measured here. The `unsupported_op` list of decision
//! 27 and `fmt=md` are not implemented.
#![forbid(unsafe_code)]

pub mod a1;
pub mod budget;
pub mod compile;
pub mod corpus;
pub mod dct;
pub mod diff;
pub mod doc;
pub mod error;
pub mod escape;
pub mod files;
pub mod find;
pub mod fixtures;
pub mod grammar;
pub mod hash;
pub(crate) mod inline;
pub mod model;
pub mod outline;
pub mod paging;
pub mod parse;
pub mod pos;
pub mod preview;
pub mod read;
pub mod refs;
pub mod response;
pub mod rev;
pub mod sheet;
pub mod slides;
pub mod text;
