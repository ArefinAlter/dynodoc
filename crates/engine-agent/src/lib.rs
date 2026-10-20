//! The read path of the Dynodoc AI-agent connector, as pure functions.
//!
//! This is the `engine-agent` crate of `docs/AI-AGENT-CONNECTORS.md` section 3.1 and the
//! "refs, hashes, DCT writer, outline, read, find, diff, escaping" task of its section 11.2.
//! It implements the read-only half of `docs/AI-AGENT-PROTOCOL-SPEC.md` (`dynodoc.agent/1`)
//! over [`engine_core::materializer::DocumentState`]: no HTTP, no database, no MCP, no I/O,
//! no clock, no randomness. Given the same state and the same arguments every function
//! returns the same bytes. Maps are ordered, so iteration order never shows in the output.
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
//! | 10 errors | [`error`] | [`error::AgentError`] (26 codes) |
//! | 12.3 JSON mirror | [`response`] | [`response::Response::json_mirror`] |
//! | 17 conformance | [`parse`], [`corpus`], [`fixtures`] | strict body parsers, the golden call list, the fixture corpus |
//!
//! Every tool returns a [`response::Response`]: the DCT `text` for the model, the `meta`
//! of `_meta["dynodoc/v1"]` (`rev`, `chain`, `file`, `cursor`, `budget`, `ids`) for
//! programmatic clients, and a typed `data`. The caller supplies the state at the wanted
//! revision, a [`model::FileMeta`] (document id, kind, `rev`, hex chain hash; the state does
//! not carry them) and, for `files`, the already authorised [`files::FileEntry`] list.
//!
//! Out of scope, and not here: `propose`, `submit`, `status`, the operation compiler, the
//! state cache, agent routes and authorisation, migrations, the MCP server.
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
//! # Not verified
//!
//! No TypeScript implementation reads the vectors yet. No model has been run against this
//! output, no real tokeniser was used (the benchmark estimates), and nothing here was
//! measured under load. The fixtures are built with real operations but not through the event
//! log, so the `chain` is a stand-in; the checkpoint-plus-tail reader is not exercised.
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
pub mod read;
pub mod refs;
pub mod response;
pub mod rev;
pub mod sheet;
pub mod slides;
pub mod text;
