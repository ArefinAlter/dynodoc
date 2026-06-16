# 06 · Authentication and Identity Tiers (T1–T2)

### Claude Code prompt to start this stage

> Build the identity layer that issues credentials and assigns capability tiers. T3 (civic-ID verified) is deferred to stage 17; this stage covers T0 (anonymous read), T1 (email/SMS), T2 (T1 + phone/behavioral). Reference: `dynodoc_public_consultation_design.md` §5. In `crates/engine-api/src/auth/`:
>
> 1. **Token format**: use **Paseto v4 local** tokens (not JWT). Paseto v4 is symmetric AEAD, no algorithm-confusion attacks, no `none` algorithm, no key-type bait. Use the `pasetors` crate. Token payload: `{ identity_id, tier, expires_at, issued_at, kid }`. 24-hour expiry, refresh via a separate long-lived refresh token in an HttpOnly cookie.
> 2. **Registration endpoints**:
>    - `POST /auth/register/email` — accepts email + display_name. Creates an `identity` row at T0 (until verified). Sends a verification email (6-digit code, 10-min expiry, max 3 attempts). On verification, upgrades the identity to T1 and adds an `identity_credential` row.
>    - `POST /auth/register/phone` — accepts phone (E.164 format). Sends an SMS code via a configurable provider (Twilio, MessageBird, BTS for Bangladesh telcos). On verification, adds a phone credential. If the identity already has a verified email, upgrades to T2.
> 3. **Login endpoints**:
>    - `POST /auth/login/magic-link` — passwordless: accepts email, sends a one-time magic link, exchanges it for a Paseto token.
>    - `POST /auth/login/oauth/{provider}` — optional, for Google/Microsoft civic-account integration in jurisdictions that want it. Use `oauth2` crate.
> 4. **Rate limits per identity** (the §5.5 + §11.5 limits):
>    - 5 registration attempts per IP per hour.
>    - 1 magic-link request per email per minute, 5 per hour.
>    - 20 comments per identity per day at T2, 5 suggestions per day per node.
>    - 100 votes per identity per day at T1+.
>    - Store rate-limit counters in Redis with TTL'd keys; fall back to Postgres `rate_window` table if Redis is down.
> 5. **First-time gentling** (the §5.5 anti-spam mechanism):
>    - A T2 identity's first 3 contributions are flagged `shadowed_for_first_30min`.
>    - These contributions are recorded as events normally (audit-attributable) but are visible *only to the author* until 30 minutes after submission, then released publicly.
>    - This forces a Sybil attacker to wait per account, not parallel-spray.
> 6. **Behavioral signal collection**:
>    - On every authenticated request, record (in Redis with sliding-window aggregation): identity_id, IP CIDR /24, user-agent fingerprint hash, action type, timestamp.
>    - Flag suspicious patterns: 10+ accounts registered from the same /24 in 1 hour → `sybil_signal` row with severity 3.
>    - Voting correlation > 0.95 between two accounts over 50+ votes → `sybil_signal` row with severity 4. (Computed in a periodic background job, not on the hot path.)
> 7. **Middleware**: Axum middleware that extracts the Paseto token from `Authorization: Bearer ...`, validates it, attaches an `AuthContext { identity_id, tier }` to the request. Endpoints declare their minimum tier via a tower layer.
>
> Tests:
>    - End-to-end registration → email verify → upgrade to T1 → phone verify → upgrade to T2.
>    - Rate limits trigger correctly (101st vote in a day is rejected).
>    - First-time gentling: 3 fresh-account submissions are shadowed and released after 30 minutes (use mocked time in test).

### Why Paseto, not JWT

JWT's history is littered with critical CVEs: algorithm confusion (RS256 → HS256 with public key as secret), `alg: none` bypass, embedded JWKS endpoints that get hijacked. Paseto v4 fixes the design: there is one algorithm per version, the version is in the token type, and the local/public split is unambiguous. The migration cost from JWT to Paseto is trivial; the security gain is real. Use `pasetors` crate (pure Rust, audited).

### The shadowing logic in detail

Shadowing must be implemented at the read layer, not the write layer:

- The event is written normally with the same `chain_hash` etc.
- A side state `shadowed_until: Map<EventId, Timestamp>` (kept in Redis with TTL) holds the release time.
- The read API filters out events that are shadowed AND `requesting_identity != event.author`.
- After the release time, the event becomes visible to all.

This way the audit log is honest (the event exists, it's chained, an auditor can see it) and the public surface is gentled.

### Watch out for

**SMS costs and delivery in Bangladesh.** Local providers (Robi, Grameenphone, Banglalink) have stricter delivery, sometimes opaque rate limits. Pick a provider that supports BD numbers reliably and budget for ~$0.01–0.05 per SMS. At T2 onboarding rate of, say, 500 verified per day during a national consultation, that's $5–25/day. Negligible.

**Email deliverability.** Use a reputable transactional email provider (Postmark, AWS SES) with proper SPF/DKIM/DMARC. Magic links going to spam is the most common cause of user drop-off.

**Don't store phone numbers in plain.** Hash + salt for the `identity_credential.credential_hash` column (per the schema). Last 4 digits stored separately if you need to display "verified ending in 9821."

**Refresh tokens are dangerous.** A long-lived refresh token is a high-value target. Bind it to user-agent + IP /24 fingerprint; rotate on every use; revoke on credential change.

### Verification checklist

- [ ] End-to-end registration flow works for email + phone.
- [ ] Token signed with Paseto v4 local, validates correctly, expires correctly.
- [ ] Rate limits enforce as specified.
- [ ] First-time gentling shadows the first 3 posts.
- [ ] Suspicious-pattern detection writes `sybil_signal` rows correctly.
- [ ] Tier upgrade from T0 → T1 → T2 works idempotently.
