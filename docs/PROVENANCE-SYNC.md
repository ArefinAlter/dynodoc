# Portable provenance sync, version 1

Implemented service endpoints and development clients; see [decision 010](decisions/010-external-editor-provenance-and-projects.md).
No production rollout or marketplace publication is implied by source availability.

## Service contract

| Request | Meaning |
|---|---|
| `GET /documents/{id}/provenance` | Member-only consistent full checkpoint: exact `through_seq`, hexadecimal `chain_hash`, stable-ID materialized state and the caller's recent provenance proposal statuses. |
| `POST /documents/{id}/provenance` | Push the envelope below as a private draft or submitted change request. The same actor and normalized envelope with the same document/bundle ID returns its original receipt. Reuse with different data returns 409. |
| `GET /documents/{id}/provenance/bundles/{bundle}` | Preserved envelope, digest, authenticated uploader, reception timestamp and receipt. Unsubmitted bundles are visible only to the uploader, including against managers. |
| `GET/POST /documents/{id}/connectors` | List the caller's connections / create a Word or Google Docs seven-day key. Key material is returned once; only its SHA-256 hash is stored. Up to ten active connections per person/file. |
| `POST /documents/{id}/connectors/{grant}/revoke` | Revoke the caller's connection, including after loss of file membership. |
| `GET /connector/documents/{id}/checkpoint` | Read with a file-scoped connection key. |
| `POST /connector/documents/{id}/bundles` | Propose with that key; capture host must match its Word/Docs scope. |

Both checkpoint GET endpoints accept optional `through_seq` for an exact canonical
historical revision. Omitted means current head; zero returns the empty initial
state/zero hash. Negative or future revisions return 400. Current permissions and
credential checks apply to historical reads; proposal statuses describe their
current review state. The dedicated gateway forwards only this validated selector.

The web session gateway remains `/api/engine`. Editor keys use the separate
bearer-only `/api/connectors/documents/{id}/checkpoint|bundles` gateway. Neither
browser cookies nor a web service key are exchanged there. Google Apps Script
contacts the allowlisted workspace origin server-side; the Word pane is hosted on
the workspace origin. Arbitrary cross-origin browser CORS is not enabled.

```json
{
  "name": "Updated introduction",
  "note": "Observed in my local Word copy",
  "submit": true,
  "bundle": {
    "format": "dynodoc.change-bundle",
    "version": 1,
    "bundle_id": "d89c0fd6-c7ed-4cc0-9b37-671d69b55a85",
    "client_id": "fe257224-bcdd-4261-aa37-778f836fa8e2",
    "document_id": "7034d450-77cf-44f0-8eec-63f892643f8e",
    "base_seq": 0,
    "base_chain_hash": "0000000000000000000000000000000000000000000000000000000000000000",
    "capture": {
      "host": "word",
      "mode": "observed_snapshot",
      "document_name": "report.docx"
    },
    "changes": []
  }
}
```

This is a shape example: a real push needs 1–20,000 changes. Each change has a
UUID `id`, RFC3339 `observed_at` and `operation`, using the existing content-only
event vocabulary: NodeCreated, FieldEdited, RichTextPatched, NodeMoved,
NodeDeleted, NodeRestored. Canonical node IDs remain ULIDs. The body limit is
16 MiB. For a nonzero base, its chain hash must equal the stored event at that
sequence in this document; an old valid base is permitted for review/three-way
merge. Permission is checked even for retries. No partial draft survives an
invalid operation. Draft insertion and the immutable receipt share one transaction.

The digest is SHA-256 of JSON serialized from the typed envelope after conversion
to `serde_json::Value`, with sorted object keys. Unknown envelope/capture/change
fields are rejected; normal event decoding is unchanged. Parsed timestamps and
event fields are normalized by Rust serde. This is not a digest of the original
HTTP byte stream. Merge still appends ordinary events as the responsible merger;
the stored bundle separately preserves the authenticated uploader and claimed
local observations.

## Shared clients

`frontend/researcher/src/lib/provenance/connector.ts` exports the shared protocol
and recovery module into both the Word pane
and Google Docs `Protocol.html`; `pnpm build:connectors` regenerates that artifact,
and `pnpm check:connectors` checks drift. Client queues contain no credentials.
Word uses transactional IndexedDB; Google Docs uses user properties, Unicode-safe
chunks, manifest replacement and a generation check against concurrent sidebars.
Google's development ledger is capped at 100 KB; exceeding storage limits stops
capture and retains the previous queue. Export before a queue grows large.

First binding requires an exact text/order match to the explicitly selected
checkpoint. Once bound, identity tags/anchors are required; the client refuses to
guess identities after insertion, deletion, movement or duplication. A failed push
keeps the frozen bundle for retry/export. A receipt never advances the canonical
checkpoint. New capture can continue while review is pending; a new push waits for
that request to close. Pull checks its document/history, previews independent
changes, refuses overlaps and rechecks the host text before applying. Rebase
archives earlier local observations and retains remaining edits against the new
checkpoint. Host operations are not assumed to be atomic: on a partial write,
the checkpoint stays old, and the next capture detects the changed local text.

## Portable recovery sidecars

Both connectors export `dynodoc.provenance-sidecar`, version 1, containing their
host and complete credential-free ledger. Select it before connecting a fresh
working copy/tab. Use the original editor and Dynodoc account, original file ID
and a valid host-scoped key. Restore requires matching paragraph text/IDs and
the exact original checkpoint retrieved from the server, including its hash and
projected paragraph content. It preserves observations, client/change/bundle IDs,
frozen submission intent, review receipts and archives. It does not update the
canonical head or replace paragraph text. Capture remains opt-in after recovery.

An existing queue, including an empty connected ledger, cannot be replaced.
Storage generation checks still prevent another pane/sidebars overwriting it.
Malformed/inconsistent data is rejected before host binding; storage failures keep
the source export available for retry. Sidecars are at most 16 MiB and Google
storage retains its 100 KB ledger cap. Original unwrapped ledger exports remain
evidence; the import control accepts the new versioned sidecar format only.

Moving a queue between editor kinds would change frozen capture intent, so use
reviewed team checkpoints for Word/Docs exchange. Restore is explicit portability,
not background observation or automatic copy recognition. Filename/content
matching still requires the existing ambiguity/access checks.

Word queries current Office file properties before capture/binding, remote calls
and pull writes. A Save As/rename stops the active connection and keeps its queue
exportable; reopen the pane and explicitly restore into the saved copy. It uses
[Microsoft's current file-properties API](https://learn.microsoft.com/en-us/javascript/api/office/office.document#office-office-document-getfilepropertiesasync-member(1)).

## Next work in order

1. Real Word Desktop/Web and Google Docs acceptance in the same test project;
   fix host-specific anchor behaviour before promising a supported connector.
2. Stable structural block operations, richer formatting and transport/rebase
   recovery; import identity manifests from existing DOCX metadata where possible.
3. A local/background capture agent and offline sidecar interchange: capture must
   survive pane closure, file copies/renames and switching devices/editors.
4. Secure installation/pairing, signed distributions, operator/privacy details,
   Microsoft/Google verification and marketplace submission.
5. Project activity/compare, project-wide releases and further file-type adapters.

Existing filename/content copy detection remains useful for selecting a matching
file when history markers are absent. Its ambiguity/permissions rules continue to
apply; extension observation is distinct from a later imported-file comparison.
