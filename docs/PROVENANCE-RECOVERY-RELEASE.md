# External-editor provenance recovery

6 October 2026. Follow-up to [Projects/provenance](PROJECTS-PROVENANCE-RELEASE.md),
under [decision 010](decisions/010-external-editor-provenance-and-projects.md).
Word and Google Docs remain equal priorities; existing web editors remain supported.

Both development connectors now export a versioned, credential-free provenance
sidecar and restore it into a fresh matching working copy/tab. Recovery preserves
observations, client/change/bundle IDs, frozen submission intent, receipts and
archives. It cannot overwrite an existing queue, replace paragraph text, change
the canonical head or silently switch the originating editor. Capture remains
opt-in after restoration. Use the same Dynodoc account and a valid scoped key.

Both checkpoint endpoints accept `through_seq` to retrieve exact historical
canonical state/hash under current access checks. Recovery verifies this original
base before host binding, even if the team version has advanced. The dedicated
gateway forwards only the validated revision selector. No migration, event
variant or existing bundle format changes. Current review statuses remain current
even when checkpoint content is historical.

Word now queries current file properties before capture/binding, remote calls and
pull writes. Save As/rename stops the connection while leaving its queue exportable.
All existing identities are validated before any recovery tag is inserted. A
production browser test also caught a dropped-parameter issue in the generic
request arrow helper; using a normal generic function restores correct requests.
This was missed by static checking and the earlier host-contract unit tests.

## Verification

Local formatting/Clippy and generated OpenAPI checks pass. All four
Projects/provenance integration tests pass in both application and standalone
engine, including the new historical-read test: exact previous hash/text, initial
state, out-of-range rejection, current access, cross-file scope and revocation.
Frontend formatting/lint, zero Svelte diagnostics and production build pass. All
239 frontend unit tests pass. Recovery tests cover frozen retries/late typing,
acknowledged requests/archives, invalid/inconsistent inputs, original-base
verification, existing-queue preservation and storage failures.

Two production-build browser tests exercise the real Word pane and Google Docs
sidebar/shared artifact with mocked host bridges. They cover restore, opt-in,
exact retry after transport failure, receipt-preserving exports, Word working-copy
changes and refusal to overwrite a saved Word queue. These are host-contract tests,
not acceptance inside Microsoft Word or Google Docs. The signed-in Projects
workflow now checks historical reads through the real gateway after merge.
Committed CI evidence follows once verified.

## Remaining boundaries and priorities

Recovery requires matching plain paragraph structure/text and the original editor.
Previous unwrapped ledger exports remain evidence; the import control accepts the
new `dynodoc.provenance-sidecar` format only. Sidecars are limited to 16 MiB and
Google retains its 100 KB ledger cap. This is explicit copy/device portability;
there is no background capture, automatic sidecar following or structural sync.
Stored bundles still distinguish authenticated uploader facts from claimed local
observations; restoring an export cannot establish human/AI authorship.

Next: actual Word Desktop/Web and Google Docs cross-host acceptance, then stable
structural/formatting adapters and durable background capture, secure pairing and
both marketplace publications. Project Activity/Compare and multi-file releases
follow. See [installation/recovery](../extensions/README.md) and
[service contract](PROVENANCE-SYNC.md). No VPS deployment or marketplace publication
was performed for this increment.
