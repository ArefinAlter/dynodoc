# Legal and trust pages

13 October 2026. These pages are product drafts, not legal advice, and are not
represented as adopted agreements. Each one keeps the visible banner and the
"Draft revised <date> · Not yet effective" line (rendered by `LegalPage.svelte`; the
e2e test in `e2e/smoke.spec.ts` asserts the exact text per page, so change both
together). The date is the page's `lastmod` in `src/lib/seo/site.ts`, so the visible
line, the sitemap and the structured data agree; bump it only when the page's content
changes. Two pages are statements of fact, not proposed terms, and are rendered with
the `statement` prop (23 October 2026): Security and Licenses carry "Updated <date>"
and no draft banner. Nothing records acceptance: the account form links to the
pages for information and has no agreement checkbox. The owner has not adopted the
policies, so the pages carry no effective date.

## Policy: undecided facts are omitted

Owner instruction, 13 October 2026: "For the legal pages we don't say the other
information now." A fact the owner has not decided is **left out of the public
pages**, not shown as a placeholder. No "Awaiting confirmation", "Not set", "to be
confirmed" or "pending counsel review" text, no empty table row, no "open items"
section, and no list of what is missing. When a decision is made, add the fact (and
only then) in one place, the operator module or the page that states it. The "draft,
not yet adopted" banner and the "Not yet effective" line stay: that is the status of
the documents, not a missing fact.

## Operator facts (owner, 13 October 2026)

One module holds them: `frontend/researcher/src/lib/site/operator.ts`
(`OPERATOR`, `ENGINE_REPO_URL`, `OPEN_SOURCE_STATEMENT`). Pages and
structured data import from it; the shared `OperatorDetails.svelte` renders the
operator block. Do not retype a value in a page.

| Fact | Value |
| --- | --- |
| Legal entity / trading name | Verne Ltd. |
| Registered address | 8/A, Sher-e-Bangla Road, Zighatola, Dhaka-1209, Bangladesh |
| Company registration | C-201411-1 |
| Trade licence | TRAD/DSCC/035498/2024 |
| Email (privacy, legal, support, abuse, accessibility, security, DPA notices) | info@vernesystems.com |
| Phone | +880 1870 034384 |
| Operator website | vernesystems.com |
| Governing law / forum | Laws of Bangladesh / courts of Dhaka, stated as a plain term of the draft |

The owner's tax identification number and D&B D-U-N-S number are deliberately **not**
published anywhere.

## Licensing and openness (owner decision)

The application repository stays private. Only the engine, HTTP API and command-line
tools are open source (MIT, <https://github.com/ArefinAlter/dynodoc-engine>). The
workspace and the website are not published as open source and no licence is asserted
for them (the pages say they are (c) Verne Ltd. and "not published as open
source"). The one sentence every public page uses is `OPEN_SOURCE_STATEMENT`: "The
engine, API and command-line tools are open source under the MIT licence. The workspace
and website are not published as open source." Do not use "AGPL", "open source" for the
application, "source archive" or "Download the source" on a public page. `/source` is a
302 redirect to the engine repository and serves no archive.

Copyright: the engine repository is MIT with copyright held by Verne Ltd.; the
application repository is proprietary (decision 013). `/licenses` says so and lists the
third-party components under their own licences.

Resolved licensing point (decision 013, resolution of 14 October 2026): the workspace
bundle no longer ships `hyperformula`, which its publisher licensed as GPL-3.0-only or
under a separate commercial licence. Formulas are calculated by the in-house engine in
`src/lib/workspace/formula/`; see `FORMULA-ENGINE-REPLACEMENT.md`. `/licenses` no longer
lists the library and states that no third-party calculation library is shipped.

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
| `/licenses` | MIT engine, API and CLI; workspace and website not open source; font licenses, direct dependencies | `src/routes/(site)/licenses` |
| `/.well-known/security.txt` | RFC 9116 contact file (static) | `static/.well-known/security.txt` |

All are indexable and listed in `INDEXABLE_PAGES` (`src/lib/seo/site.ts`), so they
appear in `sitemap.xml` and `llms.txt` and are served without `noindex`. They are
served on both the public host and the workspace host. `LegalPage.svelte` also lists
every legal document in a "Related documents" strip; add a new page there and in the
footer (`nav.ts`) when one is created.

## Facts the owner has not decided (omitted from the public pages)

The operator identity, address, registration numbers, contact email, phone and the
governing law and forum are published (see "Operator facts"). The following are not
decided, are none of them invented, and are **not mentioned** on any public page:

- Effective date (the policies are not adopted).
- Hosting provider name and storage location, domain registrar/DNS provider.
- Representative or DPO, if required.
- Web server log retention and administrative-log retention; legal bases per purpose.
- Counsel review of the governing law and forum.
- Data-processing items: instruction mechanism, subprocessor-change notice, incident
  and assistance timelines, audit right, transfer safeguards, liability allocation.

Where a page would have said so, the sentence, row or section is simply absent (for
example `/dpa` has no "Location and transfers" or "Open items" section, and
`/subprocessors` has no location column). The hosting provider is still listed as a
role on `/subprocessors` and `/privacy` because it holds all service data; only its
name and location are omitted.

## Facts the pages rely on (re-check when behaviour changes)

- Release state: production runs application `49ada1e` on schema 38 (12 October 2026).
  Account signing keys and storage budgets are live; shared project history and signed
  branch receipts are deployed with writers off and no receipt-signing key, shown as
  "Live, not enabled" in `/security#status`, and the privacy callout says the same.
  Update the status table on every rollout.
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
- The optional source imports (form platforms and Google Forms) still exist in the
  workspace, so they are disclosed neutrally in `/privacy` and `/cookies`. Delete those
  lines if the connectors are removed.

## Vulnerability reporting

`static/.well-known/security.txt` follows RFC 9116 with two `Contact` lines
(`mailto:info@vernesystems.com` and GitHub private vulnerability reporting for the
**public** `ArefinAlter/dynodoc-engine` repository, enabled and confirmed 12 October
2026), `Expires`, `Preferred-Languages`, `Canonical` and `Policy`
(`https://dynodoc.online/security#report`). The application repository
`ArefinAlter/dynodoc` is private, so its advisory URL is not reachable by outsiders and
must not be used. `Expires` is under one year: renew it before 11 October 2027.
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
operator identity and address are published as of 13 October 2026, but the policies
are not adopted and have no effective date; the public legal pages stay marked as
drafts. This is a record of product behavior and owner direction, not legal approval
of the policy.
