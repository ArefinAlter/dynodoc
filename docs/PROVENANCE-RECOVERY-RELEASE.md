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
239 frontend unit tests passed in the full local run; an added observation-limit
boundary test also passes (240 total tests). Recovery tests cover frozen retries/late typing,
acknowledged requests/archives, invalid/inconsistent inputs, original-base
verification, existing-queue preservation and storage failures.
The 20,000-change upload limit applies to frozen bundles, while later observations
remain recoverable within the sidecar byte limit.

Two production-build browser tests exercise the real Word pane and Google Docs
sidebar/shared artifact with mocked host bridges. They cover restore, opt-in,
exact retry after transport failure, receipt-preserving exports, Word working-copy
changes and refusal to overwrite a saved Word queue. These are host-contract tests,
not acceptance inside Microsoft Word or Google Docs. The signed-in Projects
workflow now checks historical reads through the real gateway after merge.
Application implementation is pushed as
[`a286aee`](https://github.com/ArefinAlter/dynodoc/commit/a286aee276dff03f01c529485566115cc7024658),
with the later-observation correction in
[`05fc37c`](https://github.com/ArefinAlter/dynodoc/commit/05fc37cb09c86d97d8e370da7effa8f03a69da08).
[Application Rust CI](https://github.com/ArefinAlter/dynodoc/actions/runs/37424245115)
passed all 199 tests, formatting, Clippy and build. The corresponding engine API,
test and capability documentation are synchronized and pushed as
[`8e60c59`](https://github.com/ArefinAlter/dynodoc-engine/commit/8e60c59c30446fad1ad54fb839c7345ef3d20e14);
[independent engine CI](https://github.com/ArefinAlter/dynodoc-engine/actions/runs/37424321559)
passed all 199 tests, lint, build, documentation and OpenAPI consistency. This
correction changes client recovery only; the Rust revision remains identical.
[Final frontend/signed-in browser CI](https://github.com/ArefinAlter/dynodoc/actions/runs/37424678744)
passed lint/format, connector artifact consistency, production build, all 240
unit tests, 11 public browser tests and all 57 signed-in browser tests for
`05fc37c`. The public-only job intentionally skips 46 signed-in cases; the
authenticated job runs all 57 without skips. The previous authenticated run was
canceled by the client correction; this final run supplies the complete evidence.

The owned local preview server is stopped, the isolated PostgreSQL container and
test databases are removed, and the temporary build log is deleted. Test credentials
and accounts were synthetic; no production email was sent.

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
