# 009 — Copy detection, the role ladder, review rules, notifications and night mode

30 September 2026. The owner asked for file recognition that holds up when files are
renamed ("bulletproof"), a notification for the file's owner when a matching file
is uploaded, clearly defined owner/editor/reviewer roles for testing, fast
similarity detection based on the project's research, conflicts when a file with
the same name but different content is pushed, day and night mode across the
website, workspace, spreadsheets and slides, and permanent deletion from the admin
panel; then a push to `main` and deployment to Vercel and the VPS. This record states
what was built, why, and where its limits are. The plain-language guide is
[GIT-STYLE-WORKFLOW.md](../GIT-STYLE-WORKFLOW.md); release evidence is in
[COPIES-AND-ROLES-RELEASE.md](../COPIES-AND-ROLES-RELEASE.md).

## Copy detection

`engine_core::similarity` fingerprints content in three layers, cheapest first:

1. **Exact content** — an order-insensitive hash of every block's normalised text.
2. **Block containment** — 64-bit hashes of *substantive* blocks (≥ 3 words):
   paragraphs, spreadsheet rows (cells joined in column order), slide text boxes and
   notes. Shared blocks measure how much of one file appears verbatim in another,
   which catches excerpts and files with added material.
3. **Wording** — a 128-value MinHash of word 3-shingles within blocks (Broder 1997),
   computed with one-permutation hashing and optimal densification (Li, Owen & Zhang
   2012; Shrivastava 2017): one hash per shingle instead of 128, same estimator.
   32 locality-sensitive bands of 4 rows (Indyk & Motwani 1998; Leskovec, Rajaraman
   & Ullman, *Mining of Massive Datasets*, ch. 3) turn candidate search into indexed
   equality lookups: a pair becomes a candidate with probability 1 − (1 − J⁴)³²,
   0.87 at Jaccard 0.5 and > 0.99 from 0.6.

Shingles never cross blocks, so reordering paragraphs, rows or slides does not change
a fingerprint. Normalisation folds case, drops punctuation (including the Bengali
danda) and ignores zero-width joiners so conjuncts compare equal. Fingerprints are
versioned (`VERSION`); stored values stay comparable across processes. A comparison
yields `identical`, `copy`, `edited_copy`, `excerpt`, `extended`, `overlap`,
`same_name` or `unrelated`, with a score = max(Jaccard, coverage either way).
Coverage uses |A∩B| = J(|A|+|B|)/(1+J) and the shared-block share.

SimHash (Charikar 2002; Manku et al. 2007) was considered: it is compact but
estimates cosine similarity of term weights, is weaker for short documents and for
containment, and its Hamming-ball lookups need multiple permuted tables. MinHash +
LSH gives Jaccard directly and the block layer covers containment. ML embeddings
remain out of scope (decision 000) and are not needed for copy detection.

**Measured** (release build, Windows x86-64, `cargo run --release -p engine-core
--example similarity_bench`, [raw result](../benchmarks/similarity-2026-09-30.json)):
fingerprinting a synthetic 8,000-paragraph, 192,000-word document takes a median
6.3 ms; comparing two stored fingerprints takes 0.02 ms. A renamed copy with 10% of
paragraphs rewritten and 5% added scores 0.90 (`edited_copy`) and shares 9 of 32
bands; an unrelated document of the same size scores 0. Storage per document is a
512-byte signature, 32 band keys and 8 bytes per substantive block. Database lookup
and upload times are not included.

**Where it runs.** A background task in the API (`copies::spawn`) fingerprints any
document whose content or title changed — new documents first, edited ones at most
every 15 s — outside the edit path. A first import wakes it immediately. After
indexing, candidates come from band keys, shared block hashes, the exact hash and the
normalised title (`title_key` drops extensions, "(v12)", "copy of", "final" etc.).
Every qualifying pair is recorded once in `document_match` (unique on the unordered
pair). The **newer** document is treated as the copy: indexing order is not used,
because an original can still be being typed when the copy is uploaded. The owner of
the original is notified once per pass (`copy_detected`), with the uploader's
identity, the uploaded file name and the similarity — never the content. A file
uploaded under the same name with different content produces `same_name_detected`,
but only when the uploader already collaborates on that document (ordinary names are
reused by strangers). "Make a copy" by someone with access is recorded as `linked`
without a notice; the client stores `copied_from` for this.

**Before uploading**, `POST /similarity/check` compares the file's text (extracted
in the browser exactly as the server extracts blocks) with documents the caller can
open, so Home → Open offers **Upload as changes** for renamed copies, and warns about
same-name/different-content files. Documents the caller cannot open are never
returned; the upload dialog explains that owners of closely matching documents are
told who uploaded which file.

**Owner actions** (`POST /copies/{id}`): *invite* grants Contributor access (never
lowers an existing role) and sends `copy_invitation`; the uploader's **Send my copy
as changes** exports their current copy in the right format and opens the push
dialog on the original. *link* and *dismiss* close the notice. `GET
/documents/{id}/copies` lists copies of a document (editors and above).

## Same name, different content

When a file is pushed, the server compares the resulting draft with its starting
revision (content similarity and the share of that revision's blocks kept). Below
0.3 similarity with less than half the blocks kept — or sharing nothing with the
current team version when the starting point was nearly empty — the request is
marked `unrelated`. Such requests merge with `workspace_merge::merge_strict`: every
change to a block that already exists in the team version becomes a `$block`
conflict needing **Keep the team's** or **Use theirs** (as Git asks for both sides of
an add/add conflict); new blocks merge as usual. The client never decides this. When
no saved revision matches a pushed file at all, the dialog starts it from the current
team version so replaced paragraphs surface as choices.

## Roles and review rules

Engine capability (`governance::Role`: author/reviewer/auditor) is unchanged. A
product ladder sits above it (`crate::access`, SQL `document_member_role`):

| Product role | Stored as | Engine role |
| --- | --- | --- |
| Owner | `document.created_by` | author |
| Manager | workspace owner/manager | author |
| Editor | `author` / workspace editor | author |
| Reviewer (new) | `approver` | reviewer |
| Contributor | `reviewer` ("comment & suggest") | reviewer |
| Viewer | `auditor` | auditor |

Stored names are kept for compatibility; migration `0021` widens the check
constraints and adds `approver` to workspace roles. `access_role_rank` gives
`approver` the same rank as `reviewer`, so numeric thresholds elsewhere are
unchanged; `member_role_order` orders the finer ladder.

`document_policy` holds the review rules: `protect_team_version` (only owner and
managers change the team version directly — enforced on batches, single ops,
suggestion acceptance, conflict resolution, draft sharing and restores; comments and
suggestions stay open), `required_approvals` (0–5) and `merge_roles`
(`editors` | `owners`). `change_request_review` is append-only (approve, request
changes, comment, optionally on one block). Approvals count from people who still
hold Reviewer or above, never from the author, and only while the author has not
changed the request since (`workspace_draft.content_changed_at`, set on draft edits,
pulls and rebases). A current "changes requested" blocks merging when approvals are
required. Owners and managers may merge anyway (`override_rules`), recorded in the
operation audit (`rules_overridden`). Ownership moves with `POST
/documents/{id}/owner`; the previous owner stays an editor.

## Notifications

`notification` rows hold identifiers and scores only; names, titles and file names
are joined when the inbox is read, so renames show immediately and erasing an
account or document removes its details from other inboxes (document rows cascade;
account erasure deletes the account's own inbox). Kinds: `copy_detected`,
`same_name_detected`, `copy_invitation`, `change_request_submitted` (owner and
direct editors/reviewers), `change_request_reviewed`, `change_request_merged`,
`change_request_declined`, `access_granted`, `ownership_transferred`. People are
never notified of their own actions. Delivery is in-app (a bell with an unread count,
refreshed each minute and on focus); e-mail delivery through the web server's
Resend integration is future work.

## Excel and PowerPoint push

`structured-push.ts` aligns an uploaded workbook or deck with the revision it started
from, like Word push: sheets by name then order, rows by exact content (Myers /
longest increasing subsequence for moves) then by similarity inside each gap; slides
by content and text similarity, shapes by text and geometry. Canonicalisation makes
an untouched download produce zero changes. Downloads record `dynodoc:document=…;
revision=…` in the core properties' subject (the one property both exceljs and
pptxgenjs write) and use the `Title (vN).ext` name. Limits: matching is by content,
so heavily rewritten rows or slides appear as remove + add; a row that is both moved
and edited does too.

## Day and night mode

All interface colours are tokens in `app.css`, redefined under
`:root[data-theme='dark']` inside `@media screen`, so printing always uses day
colours. A pre-paint script in `app.html` applies the saved preference (cookie on
`.dynodoc.online`, shared by the website, workspace and admin hosts, or local
storage), `$lib/theme.svelte.ts` switches it, and `ThemeToggle` cycles day → night →
system. Authored content is content: text colours, fills, slide themes, chart series
and exports are unchanged. In night mode, near-black authored text shows in the page
ink and text on authored fills gets readable ink through editor decorations
(documents) and `cellColors` (spreadsheets); **View → White pages** keeps pages
light. Slides keep their authored design, as PowerPoint does.

## Administrator deletion

`/admin/erase` now accepts a document outside Trash: it is moved to Trash (public
links revoked) and erased in the same transaction under the existing
receipt-and-trigger guard. `/admin/erase-batch` erases up to 100 documents, each in
its own audited transaction with its own receipt, after a typed `DELETE N
DOCUMENTS`. The admin list opens on Documents with an All/Active/In Trash filter,
per-row **Delete** and multi-select.

## Not claimed

- Detection covers content inside Dynodoc; copies that never reach Dynodoc cannot be
  detected. Very short files (fewer than 12 distinct three-word sequences) are not
  compared. A document into which a newer document's text is pasted is treated as
  the original, because only creation times are compared.
- The owner learns who uploaded a matching file and its name; this is disclosed to
  uploaders in the upload dialog and should be reflected in the privacy notice.
- Notifications are in-app only.
- Merged events remain attributed to the person who merged.
