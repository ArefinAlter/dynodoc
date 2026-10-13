# Receipt authority: decision and enablement checklist

Decision date: 13 October 2026. Owner delegated the judgement; this record explains it.

## Decision

Production keeps the signed-receipt authority **not installed**. Shared project-branch
publication stays refused on the hosted service, and `DYNODOC_REF_SIGNING_KEY_FILE` is
not set. Everything else in schema 38 works without it: reading, project activity,
private drafts, change requests, device sessions for reads and staging, and the CLI's
local repository.

Reconsider when at least one of the conditions below changes, not on a calendar date.

## Why not now

1. **It cannot be undone.** The authority's public key is installed into the database
   once and is immutable. Clients pin that key when they first trust it. If the private
   key is lost, no further receipt can ever be issued by this deployment; if it leaks,
   an attacker could mint receipts that pinned clients accept.
2. **There is no server-side rotation or compromise recovery yet.** Clients can verify a
   dual-signed rotation record (see REF-RECEIPTS.md), but the service cannot yet install
   a successor authority, serve that record, or recover from a lost key. That is the
   missing piece that would turn a mistake into an incident instead of a rebuild.
3. **Nobody needs shared branches today.** The hosted pilot holds two documents. Shared
   publication is used only by the operator CLI, and there is no web authoring path
   (a browser holds no signing key by design).
4. **The writer path has not met production data.** CI and the restricted-runtime
   tests exercise it, but it has not been rehearsed with the live runtime role, the
   key mount and a production copy.
5. **Key custody is not established.** Safe custody needs an offline escrow copy held
   by a named person, which is a decision for Verne Systems, not a default.

## What enabling would require (checklist)

- [ ] A named custodian and two offline copies of the private key (not on the VPS
      backup path that holds database dumps).
- [ ] Generate the key on an owner machine: `openssl genpkey -algorithm ed25519 -outform DER`
      (see SHARED-PROJECT-HISTORY.md for the exact public-key extraction).
- [ ] A production-copy rehearsal with the key mounted read-only
      (`infra/demo/compose.schema38.override.yml`) and the restricted runtime login:
      publish, protected-main review, receipt verification from a second client journal.
- [ ] Decide the client distribution of the authority pin (document it in the
      onboarding for each project; a server-reported key must never be auto-trusted).
- [ ] Either implement server-side rotation and a documented compromise procedure, or
      accept in writing that a lost or leaked key means a new deployment authority and
      re-pinning every client.
- [ ] Install with the operator login only: `engine-cli install-ref-authority PUBLIC_HEX`,
      then recreate only the API container with the override file.
- [ ] Update `/security` and the roadmap page from "not enabled" to enabled, and add the
      authority fingerprint to the release record.

## Related work that lowers the risk

- Server-side authority rotation with a dual-signed record and a recovery procedure.
- Independent witnesses for signed heads (so a rewrite is visible without trusting the
  authority).
- Per-project device-key grants and a separate review scope, so a stolen key file is not
  a single factor for account-wide approval (see PROJECT-SCOPED-DEVICE-SESSIONS.md).
