# Dynodoc self-hosted edition: identity and access

> **Status: design, not implemented.** 14 October 2026. This document specifies how a
> self-hosted installation authenticates people and machines, provisions and removes
> accounts, and maps an organisation's groups to Dynodoc roles. Items tagged MISSING
> or PARTIAL are proposals. Nothing here changes code.
>
> Read first: the [master design](SELF-HOSTED-EDITION.md). Companions:
> [deployment guide](SELF-HOSTED-DEPLOYMENT-GUIDE.md),
> [licensing and distribution](SELF-HOSTED-LICENSING-AND-DISTRIBUTION.md).

Contents

1. Purpose, goals, non-goals, personas, success metrics
2. Current state versus target
3. Architecture and trust boundaries
4. Identity model changes
5. OpenID Connect
6. SAML 2.0
7. SCIM 2.0
8. LDAP option
9. SMTP and email flows
10. Group-to-role mapping
11. Administrator roles and service accounts
12. Session and token model changes
13. Academic federation
14. Offboarding and account lifecycle
15. Security analysis
16. Operations
17. Delivery plan
18. Risks and open questions
19. Appendices

---

## 1. Purpose, goals, non-goals, personas, success metrics

### 1.1 Purpose

The hosted service authenticates with an email link or Google and grants access by
manual invitation. That works for a pilot of nine identities. A university department
or an enterprise needs the opposite: people arrive through the organisation's identity
provider, access follows group membership, and a leaver loses access without anyone
remembering to remove them.

### 1.2 Goals

| ID | Goal |
|---|---|
| IG1 | Sign-in through the customer's OpenID Connect or SAML 2.0 provider, with no dependency on any provider operated by Verne or Google. |
| IG2 | Account lifecycle driven by the customer's directory through SCIM 2.0: create, update, deactivate. |
| IG3 | Roles driven by groups through explicit, reviewable mapping rules, without weakening the existing role ladder or project policy floors. |
| IG4 | A stable identity key that does not depend on a mutable email address. |
| IG5 | Immediate and complete revocation: web sessions, project device sessions, device keys and connector grants. |
| IG6 | Machine identities for automation that do not need personal credentials. |
| IG7 | A deployment without outbound mail: operator-issued links and SSO only. |
| IG8 | Support for academic federations (SAML, eduGAIN, Shibboleth) without special code paths. |

### 1.3 Non-goals

- Dynodoc does not become an identity provider and does not store passwords.
- No multi-factor authentication implementation of its own. MFA is the IdP's job; Dynodoc
  can require evidence of it (`acr`/`amr`) and re-authentication.
- No fine-grained attribute-based access control. The existing role ladder, spaces,
  project floors and review rules remain the authorisation model.
- No automatic erasure of data on deprovisioning. Deactivation disables; erasure stays an
  explicit audited administrator action.
- No group-nested-hierarchy synchronisation beyond what the mapping rules express.
- LDAP is an option, not the recommended path (section 8).

### 1.4 Personas

| Persona | Needs |
|---|---|
| Identity administrator (university IAM, enterprise IT) | Connect an IdP in an afternoon, map groups, see what will change before it changes, deprovision reliably |
| Department or lab manager | Create a project and have the right people appear without per-person invitations |
| Instructor / research PI | Course and project spaces populated from enrolment groups; guests from other institutions |
| Security officer | No password handling, enforced MFA evidence, session limits, complete audit, a kill switch |
| Auditor | Read-only view of who has what and who changed it |
| Automation owner | A credential for a CI job or a scheduled sync that is not a person's account |
| End user | Sign in once through the usual page; no new password |

### 1.5 Success metrics (targets to validate)

| Metric | Target |
|---|---|
| SSO configuration from IdP metadata to first mapped sign-in | 8 working hours or less (validation V7) |
| Deactivation at the IdP to loss of all access | 60 seconds or less after SCIM delivery; at most one token lifetime for claim-only deployments without SCIM (documented) |
| Account takeover tests (email reassignment, unverified email, issuer confusion) | All fail closed |
| Mapping preview accuracy | The previewed change set equals the applied change set in 100% of test runs |
| Authentication-related support tickets per 100 users per month after month three | 5 or fewer |

---

## 2. Current state versus target

| # | Capability | Tag | Evidence and limitation |
|---|---|---|---|
| 1 | Identity keyed by lower-cased email | EXISTS | `identity_email_lower_idx` (`0004_identity.sql`); `upsert_identity` (`auth/store.rs`). No provider subject is stored. |
| 2 | Email magic link | EXISTS | `/auth/magic-link` (service key), `/auth/verify`; 15-minute, single-use, hash stored; 5 requests per 15 minutes per email hash (`auth_request_window`). Delivery through Resend only. |
| 3 | Provider sign-in exchange | EXISTS | `/auth/exchange` (service key) takes `{email, display_name}` and mints tokens. Web verifies Google `email_verified`. |
| 4 | Google OIDC | EXISTS | `routes/auth/[...action]/+server.ts`: authorization code, PKCE S256, state in an HttpOnly cookie, no `nonce`, userinfo fetch. |
| 5 | Generic OIDC (multiple providers, issuer per provider, claims mapping) | MISSING | `openid-client` and `jose` are installed; discovery URL is fixed to Google. |
| 6 | SAML 2.0 | MISSING | |
| 7 | SCIM 2.0 | MISSING | |
| 8 | LDAP | MISSING | |
| 9 | Provider groups to roles | MISSING | |
| 10 | Revocable logins; per-login lineage; account generation | EXISTS | `refresh_token.session_id`, `identity.session_generation` (schemas 15, 27); `ensure_bound_session` per request. |
| 11 | Administrator control: disable, restore, revoke sessions (also revokes device keys), erase | EXISTS | `admin.rs`, `admin_controls.rs`; typed confirmation. |
| 12 | Administrator identification | PARTIAL | `ADMIN_EMAILS` exact email match; no auditor role. |
| 13 | Device keys, project device sessions, grants, connector grants | EXISTS | Schemas 23, 33, 38, 40. |
| 14 | Non-human identities | MISSING | `identity.email` is mandatory and unique. |
| 15 | Session policy configuration | PARTIAL | `TOKEN_TTL_SECONDS` only; refresh lifetime constant 30 days; no idle or absolute cap; web cookie 30 days. |
| 16 | Per-session list and revoke | MISSING | Logout revokes by lineage; no listing. |
| 17 | IdP logout propagation | MISSING | |
| 18 | Open-redirect-safe return path | EXISTS | `safeNext` allowlist of workspace paths (`lib/auth-next.ts`). |
| 19 | Email sign-in can be disabled | MISSING | Offered whenever Resend is configured (or `DEV_AUTH`). |
| 20 | Offboarding workflow with ownership report | PARTIAL | Admin erase reports blockers (owned documents, owned workspaces); no deactivation report. |
| 21 | Live streams stop promptly after revocation | PARTIAL | Checked only when the next event is delivered; idle streams keep heartbeating ([SECURITY-HARDENING.md](SECURITY-HARDENING.md)). |
| 22 | Role ladder, spaces, project floors | EXISTS | `access.rs` (viewer, contributor, reviewer, editor, manager, owner), `access_space` kinds organization, team, folder, project. |
| 23 | Sign-in of the native apps from the system browser | EXISTS | One-time hand-over code with PKCE ([decision 017](decisions/017-native-app-sign-in-handover.md)): web pages `/app-signin` and `/app-signin/finish`, web routes `POST /auth/app-signin` and `/auth/app-signin/redeem`, API routes `POST /auth/app-handover` and `/auth/app-handover/redeem` (both need the service key; issuing also needs the person's own bearer token), table `app_handover_code` (`0046_app_handover.sql`): code stored as SHA-256, 60 seconds, once, at most three unused per account, audit `auth.app_handover.*`. Works at any configured origin; with email sign-in off and no Google the page says no method is turned on. Nothing new to configure. |

---

## 3. Architecture and trust boundaries

### 3.1 Placement of components

Authentication protocols live in the web tier. The API continues to trust the web tier
only through the service key and never speaks to an IdP. This keeps the MIT engine free
of SAML and OIDC client code and keeps the API's attack surface unchanged.

```
 Browser ---- OIDC/SAML redirects ----> IdP (customer)
    |                                      |
    | cookies                              | back-channel logout, SCIM push
    v                                      v
 +--------------------- web tier (proprietary) ------------------------+
 |  /auth/oidc/{id}/*   /auth/saml/{id}/*   /scim/v2/*   /auth/email  |
 |  claim validation, mapping evaluation, SCIM facade, mail transport |
 +------------------------+-------------------------------------------+
                          | service key (HMAC), internal routes
                          v
 +--------------------- API (MIT) -------------------------------------+
 |  /auth/exchange  /auth/refresh  /auth/logout                        |
 |  /internal/identities  /internal/memberships  (proposed, generic)  |
 |  identity, identity_external, login_session, space_member, audit   |
 +------------------------+--------------------------------------------+
                          v
                      PostgreSQL
```

| Component | Responsibility | Why there |
|---|---|---|
| Web tier | Protocol endpoints, assertion and token validation, claims mapping, SCIM HTTP facade, mail | Already holds provider secrets, `openid-client`, `jose`, cookies |
| API internal routes (proposed) | Create/link/disable identities, set memberships with a source tag, write audit rows, revoke sessions, mint sessions | Data and invariants live here; one place enforces "disable revokes everything" |
| IdP, directory | Authentication, MFA, group membership, SCIM client | Customer |

Where code that implements the enterprise protocols lives is an open question with a
recommended default (master Q2): proprietary web tier plus generic MIT internal routes.

### 3.2 Trust boundaries

| Boundary | Trust decision | Control |
|---|---|---|
| IdP to web | The IdP asserts who the user is and which groups they have | Signature validation, `iss`, `aud`, `exp`, nonce/InResponseTo, replay cache, algorithm allowlist |
| Web to API | The web tier has authenticated the user | `x-dynodoc-service-key` on issuance and internal routes; constant-time comparison |
| SCIM client to web | The IdP is entitled to provision | Bearer token (hashed), TLS, source allowlist, rate limits |
| Mapping rules | Group names mean what the rule says | Admin-authored rules, ceilings, dry run, audit |
| User device to API | Device key possession | Existing device-session protocol |

---

## 4. Identity model changes

### 4.1 Identity key

Today the person is `lower(email)`. The edition keeps `identity.id` (UUID) as the
internal key and adds the external key `(issuer, subject)`.

```sql
-- proposed migration 0043_external_identity.sql (illustrative number; 0041 is taken)
create table identity_external (
  id             uuid primary key default gen_random_uuid(),
  identity_id    uuid not null references identity(id),
  provider       text not null,                 -- configured provider id
  issuer         text not null,                 -- OIDC iss, SAML IdP entityID, 'scim:<tenant>'
  subject        text not null,                 -- OIDC sub, SAML NameID / persistent id, SCIM externalId
  email_at_link  text,
  linked_at      timestamptz not null default now(),
  last_login_at  timestamptz,
  unique (issuer, subject)
);
create index identity_external_identity_idx on identity_external(identity_id);

alter table identity add column kind text not null default 'person'
  check (kind in ('person','service'));
```

The runtime login gets SELECT, INSERT and UPDATE (the "insert-update" class used for
`device_key_access`), no DELETE. `check_runtime` in `engine-core/src/database.rs` must be
extended; this is the same pattern as schemas 38 to 40.

### 4.2 Linking policy

| Situation | Behaviour |
|---|---|
| `(issuer, subject)` already linked | Sign in as the linked identity. The email in the assertion is informational. |
| Not linked; assertion email matches an existing identity; provider setting `link_by_email: verified` and the assertion carries a verified email (or the provider is marked `trust_email: true` and has `allowed_domains`) | Link, write an audit row, notify the person's address (optional), continue |
| Not linked; email matches; linking not allowed | Refuse with a message that names the administrator contact; create nothing |
| Not linked; no match; provisioning `jit` and the email domain is in `allowed_domains` | Create person identity and link |
| Not linked; no match; provisioning `invite_only` or `scim_only` | Refuse |
| Linked identity is disabled | Refuse; generic message (same as today's `TokenInvalid`) |

Email is never used to decide authority once a link exists. This is the central defence
against the class of takeovers where an IdP lets a tenant administrator set an
arbitrary, unverified `email` claim (publicly reported for Microsoft Entra ID multi-tenant
applications under the name "nOAuth", 2023; **unverified; check the reference at build
time**). A multi-tenant issuer such as the Entra `common` endpoint must never be
configured with `link_by_email`.

### 4.3 Email changes at the IdP

Email addresses are the sharing key in the UI (`POST /documents/{id}/members` takes an
email). When a linked person's email changes at the IdP:

1. If the new address is unused, update `identity.email` and write an audit row with
   old and new values.
2. If another identity already uses it, do not change anything, flag the account for an
   administrator, and keep signing the person in by `(issuer, subject)`.
3. Invitations sent to the old address that were never accepted do not follow the person.

### 4.4 Display name and profile

`display_name` is updated from the IdP only until the person saves their own profile
(`profile_revision > 0`), matching today's `upsert_identity` rule. Bio is never taken from
the IdP.

### 4.5 Internal API surface (proposed, MIT)

All require the service key. They are generic directory-sync primitives with no protocol
knowledge.

| Route | Purpose |
|---|---|
| `POST /auth/exchange` (extended) | Body adds `provider`, `issuer`, `subject`, `email_verified`, `acr`, `amr`, `auth_time`, `idp_session`; returns tokens and `{created, linked}`; the old two-field body keeps working |
| `POST /internal/identities` | Provision (SCIM create) with optional external link |
| `PATCH /internal/identities/{id}` | Update display name, email, `active` (false disables, revokes sessions and device keys, sets a reason) |
| `PUT /internal/memberships` | Reconcile source-tagged memberships for an identity (mapping output) with a dry-run flag |
| `POST /internal/sessions/revoke` | By identity, by `(provider, idp_session)` or by provider (back-channel logout, incident) |
| `GET /internal/identities` | Lookup by external id or email (SCIM filter support) |

---

## 5. OpenID Connect

### 5.1 Flow

Authorization code with PKCE (S256), `state`, `nonce`, confidential client. The existing
Google route already implements the first two; `nonce` and ID-token claim validation are
added. Implicit and hybrid flows are not supported.

```
 /auth/oidc/{provider}?next=...        -> store state, nonce, verifier, next (sealed cookie, 10 min)
                                          redirect to authorization_endpoint
 /auth/oidc/{provider}/callback        -> exchange code; validate id_token (sig, iss, aud, exp,
                                          nonce, azp, auth_time if max_age); fetch userinfo only if
                                          needed; map claims; POST /auth/exchange; set session cookie
```

### 5.2 Provider definition

Provider configuration is a file (declarative, reviewable, secrets by reference). A
database-managed variant with an administrator screen is later work (IQ1).

```yaml
# oidc-providers.yaml (proposed)
providers:
  - id: uni
    display_name: "University sign-in"
    issuer: https://idp.example.edu/realms/main        # discovery at /.well-known/openid-configuration
    client_id: dynodoc
    client_secret_file: /run/secrets/oidc_uni_secret
    client_auth: client_secret_basic                   # or private_key_jwt (client_key_file)
    scopes: [openid, email, profile, groups]
    pkce: S256
    claims:
      subject: sub
      email: email
      email_verified: email_verified
      name: name
      groups: groups                                   # path, supports dotted paths
    trust_email: false
    allowed_domains: [example.edu]
    link_by_email: verified
    provisioning: jit                                  # jit | invite_only | scim_only
    require:
      acr: [https://refeds.org/profile/mfa]            # optional; fail if absent
      max_age_seconds: 43200
    logout:
      rp_initiated: true
      backchannel: true
    ca_file: /etc/dynodoc/ca/idp-ca.pem                # for private CAs (also NODE_EXTRA_CA_CERTS)
    mapping: uni-mappings                              # refers to section 10
```

### 5.3 Validation rules

| Check | Rule |
|---|---|
| Issuer | Exact match with the configured issuer; discovery document `issuer` must equal it; multiple providers never share an issuer (mix-up defence) |
| Algorithms | Allowlist `RS256`, `PS256`, `ES256`, `EdDSA`; reject `none` and HMAC with the client secret |
| Audience and authorised party | `aud` contains `client_id`; if several audiences, `azp` equals `client_id` |
| Time | `exp`, `iat` with skew of 60 seconds (configurable, bounded by 5 minutes) |
| Nonce and state | Bound to the sealed cookie; single use |
| Email | Required for linking or creation; `email_verified === true` unless `trust_email` |
| Groups | Read from the ID token; if an overage indicator is present, see 5.5 |
| `auth_time` / `acr` | Enforced when configured |
| Return path | `safeNext` allowlist unchanged |
| JWKS | Cached with key id rotation; refresh on unknown `kid` at most once per minute |

### 5.4 Discovery in restricted networks

Discovery needs the web tier to reach the IdP; in air-gapped sites the IdP is internal.
Where discovery is unavailable or blocked, the provider file may carry the endpoints
and a JWKS file explicitly. Private CAs: `ca_file` per provider, `NODE_EXTRA_CA_CERTS`
for the process.

### 5.5 Group claims and token size

| IdP | Behaviour | Consequence |
|---|---|---|
| Keycloak, Authentik, most standards-based providers | Group claim via a mapper, size limited only by the token | Direct mapping works |
| Microsoft Entra ID | If the user is in more than 200 groups (JWT) or 150 (SAML), the groups claim is omitted and an overage indicator is sent; the client is expected to call Microsoft Graph (limits per Microsoft documentation, checked 14 October 2026) | An intranet-only Dynodoc cannot call Graph. Use SCIM for group membership, or application roles, or filter the groups claim to groups assigned to the application. Treat a missing groups claim with an overage indicator as "unknown", never as "no groups". |
| Google Workspace | No groups claim in the ID token | Use SCIM through a directory sync tool, or map by domain only |
| ADFS | Group claims through claim rules | Works; keep the list small |

### 5.6 Logout

- **RP-initiated.** Sign-out revokes the Dynodoc login and, if `rp_initiated`, redirects
  to `end_session_endpoint` with `id_token_hint` (stored encrypted in `login_session`,
  section 12) and `post_logout_redirect_uri`.
- **Back-channel** ([OpenID Connect Back-Channel Logout 1.0](https://openid.net/specs/openid-connect-backchannel-1_0.html)).
  `POST /auth/oidc/{provider}/backchannel-logout` with a logout token; validate
  signature, `iss`, `aud`, `events`, absence of `nonce`, `jti` replay; revoke
  `login_session` rows matching `sid` or `sub`. RPs must implement their own session
  termination, which `login_session` provides.

### 5.7 Presets

| Preset | Purpose |
|---|---|
| `google` | Today's behaviour, with the fixed issuer `https://accounts.google.com`, `email_verified` required, `allowed_domains` optional (Workspace) |
| `keycloak`, `entra-single-tenant`, `okta`, `authentik` | Claim names and recommended scopes; tenant-specific issuers only |

---

## 6. SAML 2.0

### 6.1 Position

SAML is required by many universities (Shibboleth, federations) and by enterprises with
AD FS. It is the highest-risk component in this design because of XML signature
handling. Policy: **never implement XML signature processing in Dynodoc code**; use a
maintained library, keep it behind a narrow interface and treat its upgrades as security
releases. Rollout order (master Q12): OIDC natively first; SAML through a supported
SAML-to-OIDC bridge (Keycloak, Authentik, SimpleSAMLphp) as a documented configuration;
native SAML service provider after the matrix is stable.

### 6.2 Endpoints and bindings

| Endpoint | Purpose |
|---|---|
| `GET /auth/saml/{id}/metadata` | SP metadata: entity ID, ACS URL, signing and encryption certificates, NameID formats, required attributes |
| `GET /auth/saml/{id}/login` | SP-initiated AuthnRequest, HTTP-Redirect (optionally signed); stores request id, relay state, `next` |
| `POST /auth/saml/{id}/acs` | Assertion Consumer Service, HTTP-POST binding |
| `GET|POST /auth/saml/{id}/slo` | Single logout (optional) |

IdP-initiated sign-in (an unsolicited Response) is **off** by default; it removes the
`InResponseTo` binding and widens replay risk. It can be enabled per provider where a
federation requires it.

### 6.3 Provider definition

```yaml
# saml-providers.yaml (proposed)
providers:
  - id: campus
    display_name: "Campus single sign-on"
    sp:
      entity_id: https://docs.example.edu/auth/saml/campus/metadata
      signing_key_file: /run/secrets/saml_sp_signing.key       # optional: sign AuthnRequests
      signing_cert_file: /etc/dynodoc/saml/sp-signing.crt
      encryption_key_file: /run/secrets/saml_sp_enc.key        # to receive encrypted assertions
      encryption_cert_file: /etc/dynodoc/saml/sp-enc.crt
    idp:
      metadata_file: /etc/dynodoc/saml/campus-idp-metadata.xml  # or metadata_url / mdq_url
      metadata_signing_cert_file: /etc/dynodoc/saml/federation.crt  # verify signed metadata
    require_signed_assertions: true        # assertion itself, even if the Response is signed
    require_signed_response: false
    allowed_signature_algorithms: [rsa-sha256, rsa-sha384, rsa-sha512, ecdsa-sha256]
    clock_skew_seconds: 60
    name_id:
      format: urn:oasis:names:tc:SAML:2.0:nameid-format:persistent
    attributes:
      subject: urn:oasis:names:tc:SAML:2.0:nameid-format:persistent   # NameID, or an attribute id
      email: urn:oid:0.9.2342.19200300.100.1.3                        # mail
      name: urn:oid:2.16.840.1.113730.3.1.241                         # displayName
      groups: urn:oid:1.3.6.1.4.1.5923.1.5.1.1                        # isMemberOf
    force_authn: false
    allow_idp_initiated: false
    trust_email: false
    allowed_domains: [example.edu]
    link_by_email: never
    provisioning: jit
```

### 6.4 Validation rules (all mandatory)

| Check | Rule |
|---|---|
| Signature | A trusted IdP key (from verified metadata) signs the assertion, or the Response when `require_signed_assertions` is false. Enveloping or detached signatures are rejected. The signed element must be the one the library hands to the application (defence against signature wrapping). |
| Algorithms | Allowlist above; SHA-1 rejected unless an explicit legacy flag is set and recorded in audit |
| Issuer | Equals the configured IdP entity ID |
| Audience restriction | Contains the SP entity ID |
| Recipient / Destination | ACS URL for this provider |
| Time | `NotBefore`, `NotOnOrAfter` with skew; `SubjectConfirmationData NotOnOrAfter` |
| InResponseTo | Matches an outstanding request id (single use); absent only for allowed IdP-initiated |
| Replay | Assertion `ID` recorded in a shared table until `NotOnOrAfter` plus skew; a second presentation is refused |
| XML hygiene | DTD and external entities disabled; document size cap; no XSLT; schema validation of the envelope |
| Encryption | Encrypted assertions decrypted with the SP key; failures are errors, never fall back |
| Attributes | Missing subject refuses; email processed under 4.2 |
| Relay state | Opaque id into the sealed cookie, not a URL |

```sql
-- proposed (same migration family)
create unlogged table saml_assertion_seen (
  provider   text not null,
  assertion_id text not null,
  expires_at timestamptz not null,
  primary key (provider, assertion_id)
);
```

### 6.5 Library selection

Candidates in the Node ecosystem include `@node-saml/node-saml` and `samlify`; in Rust,
`samael` (which depends on libxmlsec1). The choice criteria, not a decision: active
maintenance and advisories history, signature-wrapping resistance tests, algorithm
allowlisting, encrypted assertion support, XML parser hardening, licence compatible with
a proprietary product, and a dependency footprint acceptable for the offline bundle.
**Maintenance status of each candidate is unverified; check at build time.** A
security review of the chosen library is part of P1-12.

### 6.6 Federation metadata

Single IdP: pin the IdP's metadata file and certificate; refresh on a schedule only from
the pinned location and verify the metadata signature if signed. For federations
(section 13) use a metadata query protocol endpoint or a pre-filtered file; never load an
entire federation aggregate into memory in the request path.

---

## 7. SCIM 2.0

### 7.1 Scope

SCIM ([RFC 7643](https://datatracker.ietf.org/doc/html/rfc7643) schema,
[RFC 7644](https://datatracker.ietf.org/doc/html/rfc7644) protocol) is the lifecycle
channel: it creates, updates and deactivates accounts and delivers group membership. It
is optional; OIDC or SAML sign-in works without it (JIT provisioning, claim-based
roles). Without SCIM, leavers are removed only when their sessions expire and sign-in
is refused at the IdP, which is why Profile C requires it.

### 7.2 Endpoints

Mounted by the web tier at `/scim/v2` when `SCIM_ENABLED=true`, translating to internal
API routes (4.5).

| Endpoint | Operations | Notes |
|---|---|---|
| `/ServiceProviderConfig` | GET | Advertises: PATCH supported, filter supported (`maxResults` 100), bulk not supported, sort not supported, ETag supported, change password not supported, authentication schemes (OAuth bearer token) |
| `/ResourceTypes`, `/Schemas` | GET | Core `User` and `Group`; no custom extension in v1 other than an optional enterprise-user subset |
| `/Users` | GET (filter, startIndex, count), POST, GET by id, PUT, PATCH, DELETE | `userName` equals the lower-cased email |
| `/Groups` | GET, POST, GET by id, PUT, PATCH, DELETE | Members reference user ids; group changes trigger mapping evaluation |
| `/Bulk`, `/Me`, `/.search` | Not implemented in v1 | Return 501 with a SCIM error |

### 7.3 Attribute mapping

| SCIM attribute | Dynodoc | Notes |
|---|---|---|
| `id` | `identity.id` | Server-assigned UUID |
| `externalId` | `identity_external.subject` for issuer `scim:<provider>` | Stored; also used as the link target for SSO when `link_by: externalId` |
| `userName` | `identity.email` (lower-cased) | Must be an email address; conflicts return 409 `uniqueness` |
| `name.formatted`, `displayName` | `display_name` | Subject to 4.4 |
| `emails[type eq "work" or primary]` | `identity.email` if `userName` is not an email | |
| `active` | `disabled_at` | `false` disables, revokes sessions, keys and grants; `true` restores access (new sign-in needed) |
| `groups` (read-only on User) | Derived from group membership | |
| Group `displayName`, `externalId`, `members` | Input to the mapping engine | Groups are not stored as access objects unless a rule maps them |

### 7.4 Semantics

| Operation | Behaviour |
|---|---|
| POST /Users | Create or link (by `externalId`, then verified email under the provider's link policy); 201 with `meta.version` ETag |
| PUT /Users/{id} | Replace mutable attributes; does not touch role assignments |
| PATCH /Users/{id} | `add`, `replace`, `remove` operations; atomic; supports path filters on `emails` and boolean `active` expressed as boolean or string (some clients send the string form: test and accept both) |
| DELETE /Users/{id} | Same as `active=false` plus a `scim_deleted_at` marker; **never** erases; returns 204 |
| Group PATCH add/remove members | Re-evaluates mapping for affected identities; returns after the transaction commits |
| Idempotency | Repeating an identical create returns the existing resource (200/409 per RFC) |
| Concurrency | `If-Match` on PUT/PATCH; mismatch returns 412 |
| Errors | `urn:ietf:params:scim:api:messages:2.0:Error` with `scimType` |
| Rate limiting | 429 with `Retry-After`; per token window in PostgreSQL (shared) |

Microsoft Entra ID provisioning needs specific behaviours (filter on `userName eq`,
PATCH operation shapes, soft delete through `active`); Okta and Keycloak plugins
vary. The interoperability matrix (P1-10) records each. For networks where the IdP
cannot reach the endpoint directly, Microsoft documents an on-premises provisioning
agent that bridges Entra ID to SCIM endpoints inside a private network (Microsoft
Learn, checked 14 October 2026).

### 7.5 Authentication and protection

- Bearer tokens: 256-bit random, shown once, stored as SHA-256 hash, with a label,
  creator, creation and last-use time, optional expiry, and the single scope `scim`.
  Created and revoked by administrators; every creation is audited. A SCIM token cannot
  authenticate any other route.
- Optional mutual TLS or source IP allowlist (Profile C).
- **Circuit breaker.** If deactivations exceed a configured fraction of active
  identities (default 10%, minimum 20 accounts) within an hour, further deactivations
  are queued and an administrator must confirm. This limits damage from a faulty
  provisioning job.
- Bounds: request body 1 MiB, filter expression length 1,024, `count` at most 100,
  group member list at most 5,000 per request.
- All SCIM-originated changes carry `source: scim:<provider>` in audit rows.

---

## 8. LDAP option

### 8.1 Position

Direct LDAP authentication means the Dynodoc web tier receives the user's password. That
is the opposite of the design goal in 1.3 and enlarges the credential-theft surface of
the web tier. The recommended path for LDAP and Active Directory estates is an
identity provider in front of the directory (Keycloak, AD FS, Shibboleth IdP, Entra
ID) speaking OIDC or SAML. Direct LDAP is built only when at least two design partners
cannot use that path (open question IQ7, master Q15). Estimated at 3.0 PW (P1-11,
excluded from the plan total).

### 8.2 Design if built

| Aspect | Specification |
|---|---|
| Transport | LDAPS (636) or StartTLS; certificate verification mandatory; private CA file |
| Authentication | Search-then-bind: a service account searches `(&(objectClass=person)(mail=%s))` or `(sAMAccountName=%s)` under `base_dn`, then binds as the found DN with the supplied password. Password never logged or stored. |
| Brute force | Per-account and per-source windows in PostgreSQL (reuse the `auth_request_window` pattern); generic failure message; the directory's own lockout policy applies and can be triggered by an attacker, which is documented |
| Attributes | `mail`, `displayName`, `uid` or `objectGUID` (as the stable subject), `memberOf` or a group search |
| Nested groups | Optional server-side expansion (Active Directory `LDAP_MATCHING_RULE_IN_CHAIN`, OID 1.2.840.113556.1.4.1941); bounded depth |
| Paging | RFC 2696 simple paged results for group sync |
| Sync | Optional scheduled reconciliation (every N minutes) feeding the same internal routes as SCIM; deactivation when an account disappears or is flagged disabled |
| Failure | Directory unreachable: sign-in fails; existing sessions continue; sync pauses with an alert |

### 8.3 Risks specific to LDAP

Password exposure to the web tier; credential stuffing against the directory through
Dynodoc; weak transport misconfiguration; group nesting explosion. Each is a reason to
prefer the bridge.

---

## 9. SMTP and email flows

### 9.1 What email is used for today

Exactly two message types, both sent by the web tier through the Resend HTTP API:
the sign-in link (`routes/auth/[...action]/+server.ts`) and the invitation
(`lib/server/invitations.ts`, called from `routes/api/invite`). In-application
notifications are not emailed. Sending is synchronous with a 15-second timeout; failure
returns a generic 503.

### 9.2 Target transport

| Setting | Meaning |
|---|---|
| `MAIL_TRANSPORT=smtp\|resend\|none` | `none` removes email features and hides the email sign-in option |
| `SMTP_URL` | `smtp://host:587` (STARTTLS) or `smtps://host:465`; credentials in the URL or a file; `SMTP_REQUIRE_TLS` default true |
| `SMTP_FROM` | Envelope and header sender; must be accepted by the relay |
| `SMTP_CA_FILE` | Private CA for relay certificates |
| Authentication | AUTH PLAIN or LOGIN over TLS; unauthenticated relay by source address (common on intranets); OAuth2 token authentication (XOAUTH2) for hosted relays is a candidate extension, not v1 |
| Library | A maintained Node SMTP client (Nodemailer is the usual candidate; maintenance status **unverified, check at build time**) |

DKIM, SPF and DMARC are the relay's concern; Dynodoc does not sign mail. The previous
guide's note remains true: a From address does not create an inbound mailbox.

### 9.3 Behaviour

- Link host is `ORIGIN`; a link for another origin is never generated.
- Existing limits stay: five link requests per 15 minutes per email hash;
  30 invitations per inviter per hour and one per recipient and item per ten minutes
  (shared across replicas through the rate table, master 6.3).
- Templates are plain text and HTML, localisable, with the organisation name from
  configuration; the footer states why the recipient received the message.
- Delivery failures are surfaced in the administrator view; a bounded outbox with retry
  is a Phase 2 candidate so a relay outage does not become a sign-in outage for email
  users (SSO users are unaffected).

### 9.4 Without outbound mail

| Need | Mechanism |
|---|---|
| First administrator | `dynodoctl admin sign-in-link EMAIL`: calls `/auth/magic-link` with the service key and prints the one-time link (15 minutes). Same security properties as an emailed link; the printed link is a credential and is handled as one |
| Break-glass access during an IdP outage | A configured `break_glass` list of emails that may use operator-issued links even when email sign-in is otherwise off; every use is audited; links are never reusable |
| Invitations | The product shows the link to copy; no email is sent (`email_status: not_configured` exists today) |

### 9.5 Email sign-in and SSO

An emailed link bypasses the IdP's MFA and conditional access. Therefore:

- `AUTH_EMAIL_LINK=off` disables it completely (Profile C default).
- With SSO enforced for a domain (`sso_required_domains`), email links are refused for
  addresses in those domains.
- Guests outside the organisation may keep email links through a separate allowlist or
  an external IdP.

---

## 10. Group-to-role mapping

### 10.1 What can be targeted

| Target | Stored as | Notes |
|---|---|---|
| Instance role | proposed `identity_instance_role(identity_id, role, source)` | `admin`, `auditor` (section 11) |
| Space membership | `space_member(space_id, identity_id, role)` with role in owner, manager, editor, reviewer, viewer | Spaces are `access_space` rows of kind organization, team, folder or project; inheritance by ancestry already exists (`inherited_space_role`) |
| Project membership | Same table; projects are spaces of kind `project` | Project policy floors still apply to what a role can do |
| Document access | `document_access` | Not mapped; direct sharing stays manual |

The product role names (viewer, contributor, reviewer, editor, manager, owner;
[`access.rs`](../crates/engine-api/src/access.rs)) are the only roles a rule can grant.
Note that `space_member.role` uses the five-value set owner, manager, editor, reviewer,
viewer; "contributor" is a document-level role stored as `reviewer` in
`document_access`. Mapping to a space therefore uses the space set.

### 10.2 Rule format

```yaml
# mappings.yaml (proposed)
mappings:
  - id: uni-mappings
    ceiling: manager                     # no rule may grant above this; owner needs explicit allow_owner
    on_removal: remove                   # remove | downgrade_to_viewer | keep_until_review
    rules:
      - name: Biology lab staff
        match: {provider: uni, group: "cn=bio-staff,ou=groups,dc=example,dc=edu"}
        grant: {space: {name: "Biology Lab", kind: team}, role: editor}
      - name: Biology lab students
        match: {provider: uni, group: "cn=bio-students,ou=groups,dc=example,dc=edu"}
        grant: {space: {name: "Biology Lab", kind: team}, role: viewer}
      - name: Course spaces
        match: {provider: uni, group_regex: '^course-(?P<code>[A-Z]{3}[0-9]{3})-(?P<kind>staff|students)$'}
        grant:
          space: {name: "Course {code}", kind: team, parent: {name: "Teaching", kind: organization}, create_if_missing: true}
          role_from: {staff: editor, students: viewer}
      - name: Instance administrators
        match: {provider: uni, group: "dynodoc-admins"}
        grant: {instance_role: admin}
```

### 10.3 Evaluation

1. Inputs: group names from the sign-in assertion, or group membership from SCIM, or the
   directory sync. An identity's group set always comes from one authoritative source per
   provider (`group_source: claims | scim | ldap`) to avoid flapping.
2. Output: the desired set of `(target, role)` for the identity.
3. Reconcile against **source-tagged** rows only. Memberships created manually keep
   `source = 'manual'` and are never removed or lowered by mapping. IdP-created rows are
   tagged `source = 'idp:<provider>'`, plus the rule name for audit.
4. Where manual and mapped grants overlap, the higher role applies at read time (the
   existing rule: the strongest direct or inherited grant wins).
5. Safety rails: never remove the last direct owner of a space (existing protection);
   never grant above `ceiling`; refuse to create a space whose name collides with a
   manually created space of another kind; cap changes per run (default 500) and require
   confirmation for larger ones.
6. Every change writes an audit row with source, rule, before and after.

### 10.4 Preview, freeze and drift

| Feature | Description |
|---|---|
| Dry run | `PUT /internal/memberships?dry_run=true` returns the exact change set; the administrator screen shows it per rule |
| Freeze | A switch that stops reconciliation but keeps sign-in working (incident response for a bad rule) |
| Reconciliation | Nightly full comparison of desired and actual for SCIM and LDAP sources; discrepancies reported, optionally corrected |
| Stale grants | Mapped memberships carry `last_confirmed_at`; those not confirmed within a configured number of days are reported |

### 10.5 What groups cannot do

Groups cannot grant access to a single document, cannot bypass project floors or
review rules, cannot raise a role above the ceiling, and cannot create accounts unless
`provisioning` allows it. An IdP administrator who can edit a mapped group can
therefore change access within the mapped scope: that is the intended trust
relationship and is stated in the threat model.

---

## 11. Administrator roles and service accounts

### 11.1 Instance roles

| Role | Capabilities | Notes |
|---|---|---|
| `admin` | Everything in today's `/admin`: overview, records, disable and restore accounts, trash and restore documents, erase with impact preview and typed confirmation, session revocation, SCIM tokens, provider configuration (view) | Replaces `ADMIN_EMAILS` after first run |
| `auditor` | Read-only: overview, records, audit, mapping preview, session list | Cannot change anything |
| `operator` (not an application role) | Holds the database owner credential and the host | Outside the application; separation of duties is organisational |

Bootstrap: `ADMIN_EMAILS` continues to work to create the first `admin`; after at least
one role-based admin exists, the variable is ignored unless `ADMIN_EMAILS_BOOTSTRAP=always`
is set. Erasure may optionally require a second administrator's approval
("four-eyes", Profile C default off, setting `ERASURE_TWO_PERSON=on`). Administrators
cannot disable themselves or another configured administrator (existing rule).

### 11.2 Service accounts

Requirements: automation (`engine-cli sync` in a pipeline, scheduled exports, a
validation job) must not use a person's credentials.

| Aspect | Specification |
|---|---|
| Identity | `identity.kind = 'service'`; address `svc-<name>@service.invalid` (reserved TLD; the unique email index is satisfied); never receives email links; cannot sign in through a browser (`/auth/magic-link`, `/auth/exchange` refuse `kind = 'service'`) |
| Owner | A named person recorded at creation; ownership transfer is an administrator action; the report of service accounts per owner is part of offboarding |
| Credentials | Device keys only. An administrator enrols a key on behalf of the account using the same signed one-use challenge as for people (the key holder signs; the administrator session authorises). No password, no personal access token, no long-lived bearer |
| Authority | Per-project device-key grants with explicit scopes (`project:read`, `history:read`, `history:write`, `history:review`); `granted` mode only (no `all`); role in each project must be granted through normal membership |
| Limits | Grants do not include `history:review` unless explicitly requested; optional expiry on every grant (new column) so that unattended credentials lapse |
| Audit | All actions carry the service account id; session issuance audit rows already exist |
| Rotation | New key, add grants, revoke the old key |

Personal access tokens are deliberately not proposed: the product's design avoids
long-lived bearer credentials, and device keys with per-project grants already provide
scoped, revocable, non-replayable authority ([DEVICE-KEY-GRANTS-API.md](DEVICE-KEY-GRANTS-API.md)).

---

## 12. Session and token model changes

### 12.1 Today

| Element | Value | Source |
|---|---|---|
| Access token | PASETO v4.local, `TOKEN_TTL_SECONDS` (code default 86,400; compose 3,600), bound to `session_id` and account generation | `auth/token.rs`, `main.rs` |
| Refresh token | 32 random bytes, single use, 30 days, rotates each use, hash stored | `auth/mod.rs`, `store.rs` |
| Web cookie | `dynodoc_session`, JWE A256GCM, HttpOnly, SameSite=Lax, Secure on HTTPS, 30 days; holds both tokens | `lib/server/session.ts` |
| Checks per request | Token decrypts; identity active; generation equal; an unrotated, unrevoked, unexpired refresh row exists for the session | `ensure_bound_session` |
| Project device session | One hour, one project, subset of four scopes, no refresh secret | [PROJECT-SCOPED-DEVICE-SESSIONS.md](PROJECT-SCOPED-DEVICE-SESSIONS.md) |
| Connector grant | Seven days, file-scoped, hash stored | [CONNECTOR-PAIRING-AND-INSTALLATION.md](CONNECTOR-PAIRING-AND-INSTALLATION.md) |

### 12.2 Changes

| Change | Detail | Task |
|---|---|---|
| Configurable lifetimes | `TOKEN_TTL_SECONDS`, `REFRESH_TTL_SECONDS`, `SESSION_IDLE_TIMEOUT_SECONDS`, `SESSION_ABSOLUTE_TIMEOUT_SECONDS`; the refresh row stores `session_started_at` so the absolute cap survives rotation | P1-6 |
| Session table | `login_session` (below) written at sign-in; updated on refresh; the web gets a "sessions" view per person and for administrators | P1-6 |
| IdP coupling | `provider`, `idp_session` (OIDC `sid`, SAML SessionIndex) stored for back-channel/SLO | P1-6 |
| Re-authentication | Sensitive actions (erase, key-grant changes to `all`, SCIM token creation, provider changes) require `auth_time` within a configured window; otherwise the web redirects to the IdP with `max_age`/`prompt=login` | P1-6 |
| Minimal cookie | Keep tokens only; never put group lists or claims in the cookie (header size trap) | P1-6 |
| Refresh reuse interval | Idempotent successor within a short window (master 6.4) | P2-3 |
| Key ring | `PASETO_LOCAL_KEYS` with key ids in the token footer so rotation does not end all sessions | P2-10 |
| Cookie name | `__Host-` prefix on HTTPS where path and domain rules allow | P1-6 |

```sql
-- proposed
create table login_session (
  session_id     uuid primary key,              -- same value as refresh_token.session_id
  identity_id    uuid not null references identity(id),
  provider       text not null,                 -- 'email' | configured provider id
  idp_session    text,                          -- OIDC sid / SAML SessionIndex
  id_token_hint  bytea,                         -- encrypted with a key derived from the service key
  auth_time      timestamptz,
  acr            text,
  started_at     timestamptz not null default now(),
  last_refresh_at timestamptz,
  user_agent_hash bytea,                        -- optional; hash only
  revoked_at     timestamptz
);
create index login_session_identity_idx on login_session(identity_id) where revoked_at is null;
create index login_session_idp_idx on login_session(provider, idp_session) where idp_session is not null;
```

### 12.3 Recommended defaults

| Setting | A | B | C |
|---|---|---|---|
| Access token TTL | 3,600 s | 3,600 s | 900 s |
| Refresh lifetime | 30 days | 14 days | 12 hours to 7 days (policy) |
| Idle timeout | none | 8 hours | 30 to 60 minutes |
| Absolute session cap | none | 30 days | 12 hours |
| Re-auth window for sensitive actions | none | 15 minutes | 5 minutes |
| Email link | on | on, SSO preferred | off |
| Refresh reuse interval | 0 | 15 s | 15 s |

---

## 13. Academic federation

### 13.1 Context

Universities and research institutes authenticate through national or regional
federations of SAML identity providers, interconnected internationally through eduGAIN,
with Shibboleth and SimpleSAMLphp as common software. Institutional services rarely
hold passwords. A lab at one university collaborating with partners at others needs
all those people to sign in to one installation hosted by one of them.

### 13.2 What a service must provide

| Item | Design |
|---|---|
| Registration | Register the SP's metadata with the host institution's federation operator; the federation signs metadata and republishes it |
| Entity categories | Request the Research and Scholarship category (a REFEDS entity category under which IdPs release a minimal attribute set to qualifying services) and publish a Sirtfi contact (REFEDS framework for coordinated security incident response). Both described at the REFEDS wiki; see references. |
| Attributes | Request `eduPersonPrincipalName` or `eduPersonUniqueId` as the subject, `mail`, `displayName` (or `givenName` and `sn`), `eduPersonScopedAffiliation` (OIDs: ePPN 1.3.6.1.4.1.5923.1.1.1.6, scoped affiliation 1.3.6.1.4.1.5923.1.1.1.9, uniqueId 1.3.6.1.4.1.5923.1.1.1.13; **verify against the REFEDS R&S specification at build time**) |
| Identifier | Prefer a non-reassignable identifier (`eduPersonUniqueId` or the SAML subject-id attribute) as `subject`. `eduPersonPrincipalName` can be reassigned to a different person over time; never use email or ePPN alone for long-term linking. Scope the subject by IdP entity ID (the `issuer` column). |
| Mail availability | Many IdPs release `mail` only to services in R&S or after consent; treat a missing email as "cannot link by email, cannot be invited by email until supplied" and allow the person to add one that is then verified by an emailed link or an administrator |
| Multiple IdPs in one installation | Several providers configured; `(issuer, subject)` keeps the same person from different institutions distinct; the same human with two IdPs is two identities unless an administrator merges (not built; flagged) |
| Discovery | A discovery service ("where are you from") is needed when many IdPs are enabled; use the federation's discovery service or a configured list per email domain (home-realm discovery). Do not enumerate all IdPs on the login page. |
| Metadata | Signed aggregate metadata is large (thousands of entities); load only the IdPs of interest, through a metadata query endpoint or a filtered, signature-verified file |
| Logout | SAML single logout is unreliable in federations; rely on short sessions and the idle and absolute caps in 12.3 |
| Attribute minimisation and notice | Show what is received; keep the privacy notice linked from the login page (customer-provided) |

### 13.3 Shibboleth Service Provider as an alternative front end

Some campuses require the Shibboleth SP software. Two integration modes:

| Mode | Description | Verdict |
|---|---|---|
| SAML-to-OIDC bridge | The institution runs Keycloak/Authentik/SimpleSAMLphp as an SP toward the federation and as an OIDC provider toward Dynodoc | Recommended first path; no new code in Dynodoc |
| Trusted-header authentication | The Shibboleth SP module on the reverse proxy authenticates and passes attributes as headers to Dynodoc | Not planned: any path that bypasses the proxy becomes full impersonation; if ever built, only on a dedicated listener reachable from the proxy and with explicit header allowlist (IQ8) |

### 13.4 Academic account lifecycle

| Event | Handling |
|---|---|
| Semester start | SCIM or claim-based mapping creates course spaces and memberships from enrolment groups |
| Semester end | Course groups disappear: `on_removal` policy removes viewer or contributor memberships; optional `archive` action keeps the space read-only for staff |
| Graduation or departure | SCIM `active=false` or the account stops authenticating; sessions end by caps; authorship stays in history |
| External collaborators | Guest identities from partner IdPs through federation, or email links for individuals, restricted to named projects |
| Research data retention | Funder and institutional retention rules apply to documents; Dynodoc retains history by design; retention schedules are a customer policy |
| Student records | Education-record considerations in the [master design, 10.4](SELF-HOSTED-EDITION.md) |

---

## 14. Offboarding and account lifecycle

### 14.1 States

```
 invited ---> active <-----> disabled ---> erased
                ^               |             (irreversible, audited, blockers apply)
                +---- restored -+
```

Disabled accounts cannot sign in, hold no valid sessions and have no valid keys or
grants; their authorship remains. Erased accounts keep pseudonymous authorship in history
(existing behaviour of the administrator erase flow).

### 14.2 What deactivation does, in one transaction

| Artefact | Effect | Existing or new |
|---|---|---|
| `identity.disabled_at` | Set | Existing |
| Session generation | Incremented; every access token fails the per-request check | Existing |
| Refresh lineage | All revoked | Existing |
| Device signing keys | Revoked permanently (re-enrolment needs a new key) | Existing (admin "revoke sessions" revokes keys) |
| Project device sessions | Fail on next request (key revoked) | Existing |
| Connector grants (seven-day, file-scoped) | Revoked | New in the sweep (grants are checked against account generation today; add an explicit sweep and audit) |
| SCIM tokens or service accounts owned by the person | Flagged; not revoked automatically (continuity), reported to administrators | New |
| Pending invitations and magic links | Invalidated (links check generation) | Existing |
| Live SSE streams | Closed within 60 seconds by the periodic re-check | New (master 6.2) |
| IdP-sourced memberships | Removed or downgraded by the mapping `on_removal` policy | New |
| Documents and spaces owned | Listed in an ownership report; transfer is manual with a "transfer all to X" action | New (report, action) |
| Public view links the person created | Remain until expiry or revocation; listed in the report | Report new |
| Add-in ledgers on the person's devices | Cannot be reached remotely; the queue may hold unsent content; pairing keys are memory-only for Office and per-user for Google | Documented limit |
| Profile (name, bio) | Visible to project members while the account is disabled; cleared on erase | Existing |

### 14.3 Timeliness

| Path | Time to loss of access |
|---|---|
| SCIM `active=false` | Seconds (all checks are per request); streams within 60 seconds |
| Administrator disable | Same |
| Claim-based only (no SCIM) | Until the access token expires (3,600 s by default, 900 s for Profile C) or the next refresh, at which point the IdP session or account state is not rechecked unless the refresh path performs an introspection or userinfo call (proposed option `recheck_idp_on_refresh`) |
| IdP back-channel logout | On receipt |

### 14.4 Reports

- **Ownership report.** Documents and spaces owned by the person, shared drafts,
  outstanding review requests, service accounts owned, SCIM tokens created.
- **Access review.** Per space: members by role with source (manual or IdP), last
  sign-in, last activity; export CSV for periodic review.
- **Orphan finder.** Spaces with no active owner.

---

## 15. Security analysis

### 15.1 Assets and actors

Assets: identity records and links; session and device credentials; group mappings and
the access they confer; provider secrets and SCIM tokens; audit trail. Actors: external
attacker; malicious user; compromised IdP or tenant admin; compromised SCIM client;
malicious Dynodoc administrator; curious operator; compromised endpoint.

### 15.2 Threats and controls

| Threat | Class | Controls | Residual |
|---|---|---|---|
| Account takeover through reassigned or unverified email | Spoofing | `(issuer, subject)` primary; email link only under policy; tenant-specific issuers; no linking on multi-tenant issuers | IdP that lies about `sub` |
| Mix-up between providers | Spoofing | Per-provider callback path, state bound to provider, issuer checked | Misconfigured duplicate issuers |
| Forged or replayed SAML assertion | Spoofing | Signature, audience, destination, time, InResponseTo, replay cache, algorithm allowlist, hardened XML | Library flaw |
| Signature wrapping / XXE | Tampering | Vetted library, DTD off, size caps, tests | Library flaw |
| Open redirect after login | Tampering | `safeNext` allowlist (exists); relay state is an opaque id | None known |
| CSRF on callbacks and logout | Tampering | State/nonce in sealed cookies; origin checks on POST (exist) | |
| Email link bypasses MFA | Elevation | `AUTH_EMAIL_LINK=off`, `sso_required_domains` | Break-glass list |
| IdP admin edits a mapped group | Elevation | Ceiling, scope limited to named spaces, audit, preview, freeze | Trust in the IdP |
| Mapping rule mistake grants broad access | Elevation | Dry run, per-run change cap, ceilings, review by auditor | Human error |
| SCIM token stolen | Elevation | Hash at rest, scope, mTLS or IP allowlist, circuit breaker, audit, rotation | Mass provisioning of accounts until detected (new accounts have no access without mapping) |
| Mass deprovision by faulty IdP job | Denial | Circuit breaker, restore action | Delay of legitimate removals |
| Admin abuses erasure | Repudiation / DoS | Typed confirmation, impact preview, blockers, audit; optional two-person approval | Insider with database owner credential |
| Session theft | Spoofing | HttpOnly, SameSite, short TTL, rotation, revocation, idle and absolute caps | XSS (CSP mitigates) |
| IdP outage | Denial | Break-glass links, existing sessions, mail fallback | Prolonged outage |
| Service account key theft | Spoofing | Per-project grants, no `all` mode, expiry, revocation | Unencrypted key file |
| Enumeration of accounts | Information disclosure | Uniform responses on link request and challenge; SCIM only with token | Timing differences |
| Over-collection of attributes | Privacy | Request minimal attributes; do not store raw assertions; log claims names, not values | |

### 15.3 What is explicitly not claimed

No claim that SAML or OIDC support makes an installation compliant with any framework;
no claim of MFA (the IdP provides it); no claim that group mapping is free of
administrator error; no independent penetration test has been performed (P1-12 plans
one).

---

## 16. Operations

### 16.1 Configuration summary

| File or variable | Content |
|---|---|
| `OIDC_PROVIDERS_FILE`, `SAML_PROVIDERS_FILE` | Provider definitions (5.2, 6.3); secrets by file reference |
| `MAPPINGS_FILE` | Rules (10.2) |
| `SCIM_ENABLED`, `SCIM_ALLOWED_SOURCES` | SCIM facade and optional CIDR allowlist |
| `AUTH_EMAIL_LINK`, `SSO_REQUIRED_DOMAINS`, `BREAK_GLASS_EMAILS` | Email policy |
| `SESSION_*`, `REFRESH_*`, `TOKEN_TTL_SECONDS` | Session policy (12.3) |

Changes to provider files are validated at start and by `dynodoctl config check`
(fetches discovery or metadata in the test mode; never in the request path).

### 16.2 Monitoring

Metrics: sign-in results by provider and reason, SCIM requests by method and status,
mapping reconciliations and change counts, circuit breaker state, live sessions, mail
results. Alerts: IdP discovery failing, JWKS refresh failing, SAML metadata expiry within
14 days, SCIM token unused for the expected interval, spike in refused sign-ins.

### 16.3 Failure modes

| Failure | Effect | Handling |
|---|---|---|
| IdP unreachable | SSO sign-in fails; sessions continue until refresh | Break-glass; communicate; cached JWKS continue to validate tokens already issued |
| IdP signing key rollover | Validation fails until JWKS/metadata refresh | Refresh on unknown `kid`; metadata refresh schedule; alert |
| Clock skew | Assertions and tokens rejected | NTP; configurable skew within bounds |
| Mapping misconfiguration | Wrong access | Freeze, preview, revert through audit before and after |
| SCIM duplicate users | 409 conflicts | Link by `externalId`, documented merge procedure |
| Relay outage | No email links | SSO unaffected; outbox (candidate) |
| Group overage (Entra) | Missing groups | Treat as unknown; use SCIM |

---

## 17. Delivery plan

Phase 1 of the [master plan](SELF-HOSTED-EDITION.md), section 13.3, with the task
detail for this document. Estimates share the basis in master 13.1 and exclude external
penetration-test fees, IdP licence costs for test tenants and legal review.

| ID | Task | PW | Conf. | Depends on | Acceptance |
|---|---|---|---|---|---|
| P1-1 | Generic OIDC RP, presets, logout hooks | 3.0 | H | P0-2 | Keycloak and Google presets pass; `nonce` and `azp` tests; wrong issuer/audience fail |
| P1-2 | `identity_external`, `kind`, link policy, `/auth/exchange` extension, internal routes skeleton, runtime allowlist update | 2.5 | M | none | Linking and takeover tests; `check-runtime` passes with new tables |
| P1-3 | Instance roles, auditor, bootstrap | 2.0 | M | P1-2 | Auditor cannot mutate; bootstrap works; `ADMIN_EMAILS` retired when roles exist |
| P1-4 | Mapping engine, source-tagged memberships, preview, freeze | 3.0 | M | P1-1, P1-2 | Property tests: idempotence, ceilings, last-owner protection; preview equals apply |
| P1-5 | SCIM server | 5.0 | M | P1-2, P1-4 | Conformance run; Entra, Okta and Keycloak interoperability cases; circuit breaker; filter fuzzing |
| P1-6 | Session policy, `login_session`, list/revoke, back-channel logout, re-auth | 3.0 | M | P1-1 | Idle and absolute caps enforced across rotation; back-channel ends sessions; step-up works |
| P1-7 | Offboarding sweep and reports; periodic stream re-check | 2.0 | M | P1-6 | Deactivation ends web, device, connector access within 60 s in an integration test |
| P1-8 | Native SAML SP | 4.5 | L | P1-2 | Negative suite (wrapping, replay, wrong audience, stale, unsigned) all refused; two IdP products pass |
| P1-9 | Service accounts | 2.0 | M | P1-3 | Cannot sign in; key enrolment on behalf; grants without `all` |
| P1-10 | Interoperability matrix | 3.0 | L | P1-1, P1-8 | Matrix published: Keycloak, Entra ID, Okta, Google Workspace, Shibboleth IdP, AD FS |
| P1-11 | LDAP (conditional) | 3.0 | M | P1-4 | Not in total |
| P1-12 | Security review and fixes | 2.5 | M | P1-1 to P1-9 | No open high findings |
| | **Total without P1-11** | **32.5** | | | |

**Excluded from these estimates:** external penetration-test fees, test-tenant and IdP
licence costs, legal review of the privacy notice, customer onboarding effort, and
translation of login and mail templates.

**Test strategy.** Containerised IdPs in CI (Keycloak, a SAML IdP image such as
SimpleSAMLphp or a Shibboleth IdP); recorded runs against Entra and Okta developer
tenants; property tests on mapping and linking; a SCIM client test suite; browser tests
for sign-in, logout and step-up; failure injection (IdP down, clock skew, JWKS rotation,
duplicate assertion).

**Rollout and migration.** Additive migrations (numbers allocated at build time, after 0041). Existing people
keep working: on first SSO sign-in the link is created under the provider's policy.
Hosted production gains the generic OIDC without behaviour change (Google preset).
`ADMIN_EMAILS` remains valid until an administrator role exists.

---

## 18. Risks and open questions

### 18.1 Risks

| ID | Risk | Mitigation |
|---|---|---|
| IR1 | SAML library vulnerability or misuse | Vetted library, hardened parser, tests, external review, bridge as the default for small teams |
| IR2 | Mapping errors cause mass over-grant or lock-out | Dry run, ceilings, caps, freeze, auditor review |
| IR3 | IdP-specific SCIM quirks consume schedule | Matrix early; restrict v1 to documented IdPs |
| IR4 | Email-keyed model leaks into the UI (sharing by email) and confuses linked identities | Show linked provider on member lists; email-change policy |
| IR5 | Customers expect directory-level groups as first-class objects | Document that groups are rules, not objects |
| IR6 | MFA assumptions | State that MFA is the IdP's; support `acr` enforcement |
| IR7 | Identity merge demand (same person, two IdPs) | Not built; manual procedure; revisit |

### 18.2 Open questions

| # | Question | Recommended default | Decider |
|---|---|---|---|
| IQ1 | Provider configuration as files or database-managed | Files in v1; admin screen is read-only; revisit after design partners | Engineering lead |
| IQ2 | Host SCIM in the web tier or the API | Web tier facade over generic internal API routes | Engineering lead and owner (master Q2) |
| IQ3 | Allow IdP-initiated SAML | Off by default, per-provider switch | Engineering lead |
| IQ4 | Default provisioning mode | `jit` only with `allowed_domains`; otherwise `invite_only` | Owner |
| IQ5 | Enforce SSO per domain by default | Off; recommended on for Profile C | Owner |
| IQ6 | Mapping ceiling default | `manager`; owner only with explicit allow | Owner |
| IQ7 | Build direct LDAP | No; bridge first (master Q15) | Owner |
| IQ8 | Trusted-header authentication for Shibboleth SP | Not built | Engineering lead |
| IQ9 | Identity merge | Not built | Owner |
| IQ10 | Two-person approval for erasure | Setting, off by default | Owner |
| IQ11 | Recheck IdP on refresh | Option off by default | Engineering lead |

---

## 19. Appendices

### Appendix A: glossary

| Term | Meaning |
|---|---|
| ACS | SAML Assertion Consumer Service, the SP endpoint that receives assertions |
| `acr`, `amr` | OIDC authentication context class and methods references |
| eduGAIN | International interfederation service connecting national research and education identity federations |
| IdP, SP | Identity provider, service provider |
| MDQ | SAML Metadata Query protocol |
| R&S | REFEDS Research and Scholarship entity category |
| SCIM | System for Cross-domain Identity Management |
| Sirtfi | Security Incident Response Trust Framework for Federated Identity |
| SLO | SAML Single Logout |

### Appendix B: sign-in decision flow

```
 assertion/ID token validated
        |
   linked by (issuer, subject)? --yes--> identity disabled? --yes--> refuse
        |no                                   |no
        |                                     +--> mint session; update last_login; reconcile mapping
   email present and allowed to link? --no--> provisioning allows create? --no--> refuse
        |yes                                         |yes
   existing identity with that email?                +--> create identity, link, mapping, mint session
        |yes                         |no
        +--> link, audit ------------+--> provisioning allows create? ...
```

### Appendix C: references

| Topic | Reference | Status |
|---|---|---|
| OpenID Connect Back-Channel Logout 1.0 | https://openid.net/specs/openid-connect-backchannel-1_0.html | Verified 14 Oct 2026 |
| OpenID Connect RP-Initiated Logout 1.0 | https://openid.net/specs/openid-connect-rpinitiated-1_0.html | Verified |
| OpenID Connect Core 1.0 | https://openid.net/specs/openid-connect-core-1_0.html | Standard; not re-fetched |
| SCIM 2.0 protocol | https://datatracker.ietf.org/doc/html/rfc7644 | Verified |
| SCIM 2.0 core schema | https://datatracker.ietf.org/doc/html/rfc7643 | Standard; not re-fetched |
| SAML 2.0 | OASIS SAML V2.0 Core, Bindings, Profiles | Standard; not re-fetched |
| Entra ID group claim limits (200 JWT, 150 SAML, overage) | Microsoft Learn, SAML token claims reference and directory service limits | Verified via search |
| Entra on-premises SCIM provisioning agent | https://learn.microsoft.com/en-us/entra/identity/app-provisioning/on-premises-scim-provisioning | Verified via search |
| REFEDS Research and Scholarship | https://wiki.refeds.org/display/ENT/Research+and+Scholarship | Verified via search (existence and purpose); attribute bundle unverified |
| Sirtfi | https://refeds.org/sirtfi | Unverified; check at build time |
| nOAuth (Entra email claim abuse) | Public disclosure by Descope, 2023 | Unverified; check at build time |
| LDAP paging, matching rule in chain | RFC 2696; Microsoft documentation of 1.2.840.113556.1.4.1941 | Unverified |
| SAML and OIDC library candidates | Project repositories | Unverified |
