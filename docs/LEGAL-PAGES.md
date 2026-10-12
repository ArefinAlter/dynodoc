# Legal and trust pages

12 October 2026. These pages are product drafts, not legal advice, and are not
represented as adopted agreements. Each one keeps the visible banner and the
"Draft revised 12 October 2026 · Not yet effective" line (rendered by
`LegalPage.svelte`; the e2e test in `e2e/smoke.spec.ts` asserts the exact text, so
change both together). Nothing records acceptance: the account form links to the
pages for information and has no agreement checkbox.

## Pages

| Route | Purpose | Source |
| --- | --- | --- |
| `/privacy` | What is handled, who sees it, history and signing keys, editor add-ins, retention and erasure | `src/routes/(site)/privacy` |
| `/terms` | Proposed terms for the service and the add-ins | `src/routes/(site)/terms` |
| `/cookies` | Every cookie and local-storage item, plus the measurement opt-out control | `src/routes/(site)/cookies` |
| `/security` | Plain-language security overview, what is live vs in source, limits, reporting | `src/routes/(site)/security` |
| `/subprocessors` | Providers actually configured, what each receives, what is not used | `src/routes/(site)/subprocessors` |
| `/acceptable-use` | Standalone rules; `terms#acceptable-use` summarises and links to it | `src/routes/(site)/acceptable-use` |
| `/accessibility` | WCAG 2.2 AA goal, what is tested, limitations, feedback | `src/routes/(site)/accessibility` |
| `/dpa` | Structure of data processing terms; not an agreement | `src/routes/(site)/dpa` |
| `/licenses` | AGPL application, MIT engine, font licenses, direct dependencies | `src/routes/(site)/licenses` |
| `/.well-known/security.txt` | RFC 9116 contact file (static) | `static/.well-known/security.txt` |

All are indexable and listed in `INDEXABLE_PAGES` (`src/lib/seo/site.ts`), so they
appear in `sitemap.xml` and `llms.txt` and are served without `noindex`. They are
served on both the public host and the workspace host. `LegalPage.svelte` also lists
every legal document in a "Related documents" strip; add a new page there and in the
footer (`nav.ts`) when one is created.

## Placeholders the owner must fill

The owner has not decided any of these and none is invented. Every one reads
"Awaiting confirmation" (or "Not set") on the pages:

- Legal operator (entity name), mailing address, governing law and forum, effective date.
- Public privacy/legal contact email, abuse contact, accessibility contact, a dedicated
  security email (security.txt currently uses GitHub private vulnerability reporting
  only; see below).
- Hosting provider name and storage location, domain registrar/DNS provider.
- Representative or DPO, if required.
- Web server log retention and administrative-log retention; legal bases per purpose.
- Data-processing items listed under "Open items" on `/dpa` (instruction mechanism,
  subprocessor-change notice, incident and assistance timelines, audit right).

## Facts the pages rely on (re-check when behaviour changes)

- Release state: production is application `858801c`, schema 29. Account signing keys
  (schema 33), shared project history and signed branch receipts (schema 36) and storage
  budgets (schema 34) are in source only; `/security#status` and the privacy callout say
  so. Update the status table on every rollout.
- Add-in data handling comes from [`listings/addin-data-handling.md`](listings/addin-data-handling.md);
  keep `/privacy#addins` and `/cookies#addins` in step with it.
- Cookies and storage come from `src/lib/server/session.ts`, the auth and Google
  integration routes, `src/lib/theme.svelte.ts`, `src/lib/auth-next.ts`,
  `src/lib/workspace/recovery.ts` and `src/lib/telemetry.ts`.
- Retention: 14-day same-server backups (`infra/demo/backup.sh`), 30-day measurements,
  30-day sessions, 15-minute sign-in links, 7-day connection keys. Off-site backup is
  owner-deferred.
- Erasure semantics follow [decision 005](decisions/005-product-operations-and-recovery.md).
- Security claims follow [SECURITY-HARDENING](SECURITY-HARDENING.md),
  [BROWSER-AND-WORKLOAD-SECURITY](BROWSER-AND-WORKLOAD-SECURITY.md) and
  [DATABASE-PRIVILEGES](DATABASE-PRIVILEGES.md); non-claims (no audit, certification,
  witness, end-to-end encryption or off-site backup) must stay.
- The optional source imports (survey/form platforms and Google Forms) still exist in the
  workspace, so they are disclosed neutrally in `/privacy` and `/cookies`. Delete those
  lines if the connectors are removed.

## Vulnerability reporting

`static/.well-known/security.txt` follows RFC 9116 with `Contact`, `Expires`,
`Preferred-Languages`, `Canonical` and `Policy`. `Contact` is GitHub private
vulnerability reporting for the **public** `ArefinAlter/dynodoc-engine` repository
(enabled; confirmed 12 October 2026). The application repository `ArefinAlter/dynodoc`
is private, so its advisory URL is not reachable by outsiders and must not be used until
that repository is public. `Expires` is under one year: renew it before 11 October 2027.
adapter-node serves `/.well-known/` through sirv (the directory is explicitly allowed);
nginx only special-cases `/.well-known/acme-challenge/`; the Vercel deployment serves it
as a static file.

## Drafting references

Checked 19 September 2026 and unchanged:

- [ICO: information in a privacy notice](https://ico.org.uk/for-organisations/uk-gdpr-guidance-and-resources/individual-rights/the-right-to-be-informed/what-privacy-information-should-we-provide/),
  a disclosure-completeness reference, not a finding that UK law governs the service.
- [Google privacy policy](https://policies.google.com/privacy),
  [Resend privacy policy](https://resend.com/legal/privacy-policy) and the
  [Vercel privacy notice](https://vercel.com/legal/privacy-policy), linked directly;
  Dynodoc makes no commitments on their behalf.

The text is original and describes current behaviour. To finalize, the operator also
needs applicable processing arrangements, retention periods and an operational
rights-request channel, then a legal review for its jurisdiction.

## Approved live-data erasure policy (21 September 2026)

The owner approved decision 005: administrator-confirmed whole-document erasure
removes live content/history/files/drafts/public copies, retaining a minimal audit
receipt. Account erasure removes the profile/access/private drafts and preserves
shared contributions under a deleted-user identity. Existing backups expire under
the current retention schedule. Off-site backup is deferred by the owner. The
operator identity/address/jurisdiction remain missing; the public legal pages stay
marked as drafts. This is a record of product behavior and owner direction, not
legal approval of the policy.
