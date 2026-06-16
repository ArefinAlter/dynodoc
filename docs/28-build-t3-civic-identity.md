# 17 · T3 Verified Citizen Identity (civic-ID integration)

### Claude Code prompt to start this stage

> Implement the T3 tier: citizens who verify against a civic identity system, unlocking weighted vote in the official tally. Reference: §5.2 (the tier model), §5.3 (the civic-ID path). This stage is jurisdiction-specific; we implement for Bangladesh (NID via Election Commission), with the integration point abstracted so other jurisdictions can plug their own. In `crates/engine-api/src/auth/civic/`:
>
> 1. **Architecture**:
>    - A `CivicIdProvider` trait with one implementation per jurisdiction. Initial: `BangladeshNidProvider`.
>    - The provider exposes:
>      - `initiate_verification(identity_id) -> VerificationSession` — kicks off the verification flow, returning a session token + redirect URL or QR code.
>      - `complete_verification(session_token, code) -> Attestation` — completes the flow.
>      - `attestation_format()` — returns the cryptographic structure of attestations (for audit).
>    - Attestations are signed JWTs from the upstream civic-ID system; we never store the underlying ID, just the signed attestation hash and a flag that *some* valid ID was used.
>
> 2. **Bangladesh NID integration path**:
>    - The user submits NID number and date of birth via a secure form.
>    - Backend calls Election Commission API (`https://api.ecs.gov.bd/v1/verify/` — placeholder; real endpoint depends on agreement).
>    - The API responds with a signed attestation: `{ verified: true, citizenship: 'BD', is_adult: true, attestation_signature, issued_at }`.
>    - Store: `identity_credential.kind = 'civic_id_attest'`, `credential_hash = hash(attestation)`, `attestation = {signature, issuer, issued_at}`. Discard NID number after verification.
>    - Upgrade `identity.tier` to 3.
>
> 3. **Zero-knowledge variant** (preferred where supported):
>    - If the civic-ID issuer supports it, request a zero-knowledge proof: "this person has a valid NID, is over 18, is a citizen" without revealing the NID number itself.
>    - Bangladesh's NID system doesn't currently support this; the design accommodates it for when/if they add it.
>    - Other jurisdictions (Estonia, Singapore) already support ZK or selective-disclosure flows.
>
> 4. **What T3 unlocks**:
>    - Weighted vote in the *verifiable citizen tally* (§9.2). The closeout report can truthfully say "X verified citizens of Bangladesh agreed with this position."
>    - Ability to formally co-sign a position on behalf of an advocacy group (the group's T3-verified head signs; the platform records the signature).
>    - Optionally: a visible "verified citizen" badge on the user's display name (off by default; user toggles per consultation).
>
> 5. **What T3 does NOT change**:
>    - Read/write capabilities for comments and suggestions are the same as T2 (the differentiation is in counts, not capabilities, per §5.2 Subtlety A).
>    - The platform never shows the underlying NID number to anyone — moderators, sponsors, the public, or even the user (after verification, only an obscured "verified ending …4523" is shown for their own confirmation).
>    - T3 is never required for participation. Citizens at T2 participate fully.
>
> 6. **Privacy hardening**:
>    - The `identity_credential` table for `civic_id_attest` records is in a separate database (or separate Postgres schema with stricter access controls), per §5.2 Subtlety B.
>    - Only two backend service-accounts can read it: the auth service (to verify tier on token issuance) and the audit-export service (for lawful process). Application servers reading the public surface do not have access.
>    - The credential hash is one-way; cannot be reversed to recover the NID.
>    - Retention: the attestation expires after 24 months; renewal required. After expiry, the identity reverts to T2 unless re-verified.
>
> 7. **Optional alternative T3 paths** (per §5.3):
>    - **In-person verification at a civic site** — election commission offices, public libraries; a staff member uses a verification kiosk to issue the attestation. For citizens without smartphones.
>    - **Notarized declaration** — a small fee, a notary public confirms identity, uploads to the platform. Slower but offline-friendly.
>    - **Web-of-trust** (§5.3) — a citizen is attested by N already-verified citizens via video calls or in-person meetings. For diaspora or hard-to-reach populations.
>
> 8. **Audit and oversight**:
>    - Aggregate counts of T3 verifications per consultation, broken down by verification method, are part of the transparency report.
>    - No individual T3 verification is publicly identifiable.
>    - Lawful process for forensic access: the audit-export service can produce a sealed record (signed by the platform's key, hashed into the closeout report's Merkle root) without exposing the data to operational staff.
>
> Tests:
>    - Synthetic NID provider for tests; verify the full flow: T2 user initiates verification, completes, upgrades to T3.
>    - Attestation expiry: after 24 simulated months, tier reverts to T2.
>    - Privacy assertion: the NID number is not recoverable from any stored data (verified by replaying writes against a forensic tool).
>    - The verifiable count: 100 T3 votes on a statement produce a closeout count of exactly 100.

### Why this stage is sensitive and worth getting right

A civic-ID integration is a high-leverage primitive — it's what makes the platform's legitimacy claim ("X verified citizens supported this") cryptographically defensible. It is also a *political artifact*: a misdesigned T3 system can exclude communities (no NID), enable surveillance (storing too much), or be co-opted (the Election Commission becoming the gatekeeper of which citizens "count"). The architectural choices in §5.2 Subtleties A and B are what keeps this safe:

- T3 is optional, never gating. A citizen without an NID participates fully at T2.
- The verification is held by the identity layer alone, never exposed to the engine or other participants.
- Multiple T3 paths so any single gatekeeper failure does not exclude.

Build defensively. Talk to digital-rights organizations before launching.

### Watch out for

**Don't store the NID number.** Anywhere. After verification, hash and discard. If you later need to re-verify, prompt the user to re-enter it; don't keep it.

**Don't make T3 a path of social pressure.** If T3 verification becomes a status symbol or peer pressure, the platform has failed at inclusion. The UI should treat T3 as quietly available, not prominently featured.

**Bangladesh NID system quirks.** The NID database has known data-quality issues for marginalized groups (rural women, religious minorities, indigenous communities). A naive "verified = legitimate" framing inherits these biases. Surface the limitation honestly: the closeout report includes both the T3 verified count *and* the broader T1/T2/T3 total, so readers can judge.

**Sovereignty of the attestation pipeline.** If the verification flow routes through any third party (cloud provider, payment gateway, notification service), that third party sees citizens verifying. Use sovereign infrastructure end to end for this flow. No commercial cloud.

### Verification checklist

- [ ] End-to-end T2 → T3 flow with synthetic provider.
- [ ] Attestation expires and reverts tier correctly.
- [ ] NID number not recoverable from storage.
- [ ] In-person and notarized paths supported.
- [ ] Verifiable count query produces exact numbers from event log.
- [ ] Privacy review by an external party documented.
