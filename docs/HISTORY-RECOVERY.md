# Historical content recovery

10 October 2026. Owner clarification to the local/remote Git-style product vision.
Team members should be able to retrieve retained older states and propose recovery
according to current access and review rules, even fifty commits behind the head.
This increment uses the current centralized event revisions. Event sequences are
not yet portable commits or multi-file releases.

## Implemented boundary

- The existing authenticated `GET /documents/{id}/provenance?through_seq=N` exports
  the state at any retained event revision with its chain hash and stable IDs.
  The normal reader selects legacy/shared checkpoints and replays the suffix.
- `POST /documents/{id}/history-recovery` defaults to `action: preview`. It accepts
  `through_seq` and optional `node_ids`. Omission previews all content differences;
  a selection restores whole logical blocks, including fields and position, while
  leaving unselected content alone. A block absent at that revision is proposed
  for deletion. The preview returns operations, source hash, `base_seq`, short block
  labels, proposal capability and any structural validation error.
- `action: create` requires the preview's `base_seq` and a fresh UUID `request_id`.
  The server checks current membership under the normal document/project locks,
  rejects viewers and stale heads, reconstructs the old state itself and validates
  the selected operations. It creates a private draft based on the current team
  state, never writes canonical events and never submits or merges implicitly.
- Source metadata records `kind: history_recovery`, contract version 1, source
  `through_seq`/`chain_hash`, sorted selected IDs (or null for all) and the current
  starting sequence. The operational audit records the creator and source. This
  records server facts; it is not a device signature or an external timestamp.
  Ordinary client-authored change requests cannot use this reserved source kind.
- Repeating the same request ID/document/actor/source returns the existing draft,
  including after submission or merge. A changed request, different owner or ID
  collision conflicts. Current access is checked before replaying a receipt.
  Request and draft creation share one transaction. Erasure removes them through
  existing draft/audit cleanup; no additional storage table is introduced.
- The ordinary submission, approval and merge path applies. Owners/managers retain
  the existing explicit approval override; editors cannot bypass owner-only rules.
  Changes after draft creation use the existing three-way comparison and require
  resolution on overlaps. Canonical recovery appends ordinary content events with
  the responsible merger; the request/audit retains the proposer/source linkage.

The web file workspace's **Version history** panel offers an earlier revision,
checkpoint JSON download, block selection and recovery draft creation. Selection
starts empty. The draft opens for inspection and **Send for review**. Activity rows
offer **Recover before this edit**; named versions retain their existing workflow.
The review dialog identifies the recovery source revision. Existing source and
target content comparisons show the changes being proposed.

## Permission and resource contract

Viewers can inspect/download history, contributors and above can propose, and
current merge/approval rules decide canonical changes. A historical role, node ID
or object hash grants no access. Scoped native-editor keys retain their existing
checkpoint/bundle endpoints; this account-session recovery route grants them no
new capabilities. Public links do not expose recovery.

The request is bounded to 256 KiB, a selection to 5,000 unique IDs and the result to
20,000 operations. Preview and create perform normal governance validation on the
proposed content. Missing parents, live children of a proposed deletion, duplicate
names and protected cells can require a broader selection or a separate draft edit.
Dependencies are never silently included. Preview may show a blocked plan; creation
rejects it atomically. The UI renders 200 block groups initially and can show more.

This operation still materializes full current and historical states and runs the
existing full-state diff under a document lock. It does not improve asymptotic
time/space bounds. No claim of hundred-MB latency or service-scale acceptance follows.
Historical checkpoint downloads are full JSON, not a fidelity-certified native
DOCX/XLSX/PPTX export. Source title/kind and access metadata are current metadata;
the content state and chain anchor refer to the requested revision.

## Remaining work

- Range/commit cherry-pick, commit revert, field/run-level selection and branch-aware
  ancestry; do not describe whole-block state restoration as any of these.
- Indexed date/message/author/commit discovery and a dedicated Projects history
  view. Activity currently displays the latest 500 events, while the revision
  reader/recovery endpoint has no such history cutoff.
- Native historical file generation and checked application through all six hosts,
  retained opaque assets, multi-file named stages/releases and recovery UI there.
- Shared draft/version references, portable commit identities and signed/witnessed
  recovery provenance in the ordered engine plan. No canonical event/schema change
  or SQL migration is introduced by this increment.

Comments, permissions, ownership, approvals and operational audit records are not
rewound by content recovery. Explicitly erased history cannot be recovered. A
content hash proves integrity relative to the retained chain, not independent
authorship or protection from a privileged rewrite of the entire server history.

## Verification

Three real-router PostgreSQL regressions cover a paragraph deleted before 55
subsequent independent saves, reconstruction through a shared checkpoint, viewer
downloads, private proposal ownership, exact concurrent retries, source hash,
owner-only approval rules, append-only restoration, preservation of unrelated work,
revocation, stale/out-of-range/no-op requests, incomplete parent/child selections,
and conflicts with changes made after the draft was created.

A passing signed-in browser scenario exercises revision selection, checkpoint download,
selected recovery, private draft, submission and merge, and checks both restored
and retained later content plus chain verification. Local checks and source CI pass
229 Rust tests in each repository, 274 frontend unit tests, 16 public browser tests
and 63 signed-in workflows. The source-label guard's five-test review suite also
passed after the full local application suite. Publication and check links are in
[the handoff](ENGINE-EVOLUTION-HANDOFF.md).
