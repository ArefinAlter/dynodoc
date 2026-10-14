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

1. **A mistake is now an incident, not a rebuild.** Schema 41 adds server-side
   rotation and recorded recovery (see [REF-AUTHORITY-ROTATION.md](REF-AUTHORITY-ROTATION.md)).
   A planned rotation is dual-signed and followed by clients; a lost or leaked key is
   a revocation marker plus a recorded break that every client must re-pin explicitly.
   This does not make a leak harmless: a leaked key can mint receipts until clients
   learn of the marker, and there are no independent witnesses yet.
2. **Nobody needs shared branches today.** The hosted pilot holds two documents. Shared
   publication is used only by the operator CLI, and there is no web authoring path
   (a browser holds no signing key by design).
3. **The writer path has not met production data.** CI and the restricted-runtime
   tests exercise it, but it has not been rehearsed with the live runtime role, the
   key mount and a production copy. The rotation and recovery commands also need that
   rehearsal (schema 41 migration on a production copy, rotation, loss and
   compromise drills).
4. **Key custody is not established.** Safe custody needs an offline escrow copy held
   by a named person, which is a decision for Verne Systems, not a default. Rotation
   makes custody changeable; it does not replace deciding who holds the keys.

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
- [x] Server-side rotation and a documented compromise procedure exist in source
      (schema 41). Remaining: rehearse them on a production copy and keep the
      operator runbook with the offline key copies.
- [ ] Rehearse a planned rotation and a recorded recovery with a second client journal.
- [ ] Install with the operator login only: `engine-cli install-ref-authority PUBLIC_HEX`,
      then recreate only the API container with the override file.
- [ ] Update `/security` and the roadmap page from "not enabled" to enabled, and add the
      authority fingerprint to the release record.

## Related work that lowers the risk

- Server-side authority rotation with a dual-signed record and a recovery procedure
  (done in source; pending rehearsal).
- Independent witnesses for signed heads (so a rewrite is visible without trusting the
  authority).
- Per-project device-key grants and a separate review scope, so a stolen key file is not
  a single factor for account-wide approval (see PROJECT-SCOPED-DEVICE-SESSIONS.md).
