# Marketplace submission checklist (owner-only steps)

Everything that could be prepared inside the repository is prepared; what is left needs
an account, a legal identity, a real device or a human decision. **No listing exists, none
has been submitted and nothing has been accepted.** Do not describe it otherwise anywhere
(site, listing, support replies) until a store says so.

Per-host copy, scopes and reviewer scripts: [word](word.md), [excel](excel.md),
[powerpoint](powerpoint.md), [google-docs](google-docs.md), [google-sheets](google-sheets.md),
[google-slides](google-slides.md). Data handling for the privacy policy:
[addin-data-handling.md](addin-data-handling.md). Background and distribution options:
[../ADDIN-PUBLICATION-AND-ACCEPTANCE.md](../ADDIN-PUBLICATION-AND-ACCEPTANCE.md).

Store rules change. Sizes and limits below come from the vendors' public documentation as
read on 12 October 2026 (Microsoft manifest icon sizes, name/summary/description limits;
Google listing assets and field limits). Anything marked "commonly" or "plan for" is not
confirmed. Re-check in the live dashboard before you submit; if the dashboard disagrees,
the dashboard wins.

## 0. Decide first

- [ ] Publisher identity: Verne Systems (trading as Verne), 8/A, Sher-e-Bangla Road,
      Zighatola, Dhaka-1209, Bangladesh, as supplied by the owner on 13 October 2026. Both
      stores show or verify these; confirm they match the registration documents. The
      public legal pages name the operator but are still unadopted drafts.
- [ ] A **monitored support mailbox** (info@vernesystems.com; the owner must confirm it is
      monitored) and a public support page at
      `https://dynodoc.online/support` (the Office manifests already point there; it must
      load without sign-in and say how to get help). The page is built by the site work,
      not by the add-ins: confirm it is live before any submission.
- [ ] Final `/privacy` and `/terms` reviewed against
      [addin-data-handling.md](addin-data-handling.md). Both stores require the links.
- [ ] Visibility: public, or organization-only first (Microsoft 365 admin Integrated apps;
      a private Google Workspace Marketplace listing). Organization-only skips public
      review but still needs the manifest/script, a deployment and admin action.
- [ ] Naming: accept `Dynodoc for Word/Excel/PowerPoint` and `Dynodoc for Docs/Sheets/Slides`,
      or choose fallbacks (see each host file). The Google name must match the OAuth
      consent screen name and must not contain "Google".

## 1. Real-host acceptance (blocks every submission)

- [ ] Run [extensions/ACCEPTANCE.md](../../extensions/ACCEPTANCE.md) on every host and
      platform you intend to list: Word and Excel on Windows, Mac and web; PowerPoint on a
      build with PowerPointApi 1.4 (not Office 2021); Docs, Sheets and Slides in a desktop
      browser. Two test identities, synthetic files, evidence kept without secrets.
- [ ] The result for a host is **pass** or the host is left out of the listing's
      supported-platform text. A manifest that validates is not a pass.
- [ ] `pnpm validate:manifests` (online, uses `office-addin-manifest`) passes for all
      three XML manifests, and Partner Center's own validation passes at upload. It
      reported "The manifest is valid" for all three on 12 October 2026; re-run it after any
      manifest edit.
- [ ] Decide how to handle platforms the validator lists as eligible but nobody has run
      (Word and Excel on iPad, older Mac builds). Microsoft's output says certification
      tests the add-in on every platform the requirement set allows, so a platform you
      have not exercised is a certification risk, not a footnote.

## 2. Microsoft: Word, Excel, PowerPoint (Partner Center)

Three separate offers, one manifest each.

- [ ] Partner Center account for the publisher, enrolled in the commercial marketplace
      program, with the business/publisher verification Microsoft asks for (this can take
      days and is the owner's legal entity, not the repository's).
- [ ] Create an offer for the Office add-in type (the dashboard's current name for it may
      differ) and **reserve the name** to equal the manifest `DisplayName` exactly.
- [ ] Offer setup / properties: category, legal terms (standard contract or your own),
      industries and markets. Pricing is a decision for the owner; the add-in has no
      in-app purchases.
- [ ] Marketplace listing: summary (100), description (4,000), search keywords, support
      URL, privacy policy URL, terms URL, language English (United States).
- [ ] Images: **300 x 300 PNG store logo**
      (`extensions/marketplace/assets/office-store-logo-300.png`) and screenshots
      (1366 x 768 PNG is the commonly required size, up to five; see section 5).
- [ ] Technical configuration: upload the matching file from `extensions/<host>/manifest.xml`.
      The manifest's `IconUrl` (32 px) and `HighResolutionIconUrl` (64 px) are required for
      AppSource and are served from `https://app.dynodoc.online/connectors/assets/`.
- [ ] Notes for certification: the reviewer steps and the reviewer account from the host
      file. Dynodoc email sign-in mails a link, so provision a mailbox the reviewer can
      read (or an account that uses Google sign-in) and put the details only in the
      private notes.
- [ ] Submit, answer certification feedback, publish when approved. Record the offer id,
      the manifest `Version` that passed and the date.

## 3. Google: Docs, Sheets, Slides (Cloud + Marketplace SDK)

Three standalone Apps Script projects, one Editor add-on listing each (a container-bound
script cannot be published). Use one standard Cloud project per add-on so each has its
own consent screen name, scopes and Marketplace configuration.

- [ ] Google account/organization that will own the Cloud projects; a Google Cloud
      billing account is not required for these APIs but the Cloud console must be
      accessible to the owner.
- [ ] Create the Apps Script project from `extensions/google-<host>/` (the files in the
      downloadable package); in Project settings, switch it to the **standard Cloud
      project** you created and show `appsscript.json`.
- [ ] Domain: verify `dynodoc.online` (and the app origin if the console asks) in Google
      Search Console with DNS; add it as an authorized domain.
- [ ] OAuth consent screen: app name (= listing name), user support email, **logo**
      (square; use `extensions/marketplace/assets/google-icon-128.png`, re-export at the exact
      size the console requires), app home page `https://dynodoc.online`, privacy policy
      and terms links, developer contact email.
- [ ] Scopes on the consent screen exactly match the host's manifest (see the host file):
      one `*.currentonly` scope, `script.container.ui`, `script.external_request`. Do not add
      more.
- [ ] OAuth verification: submit for brand verification and for any sensitive scope the
      console flags (plan for `script.external_request`). Expect to provide a scope
      justification (the host file has one) and a screen recording of the full flow. This
      is a separate review from the Marketplace listing and can take time.
- [ ] Enable the **Google Workspace Marketplace SDK** API on the Cloud project.
- [ ] Create a **versioned deployment** of the script (Deploy > New deployment > Add-on)
      and keep its **deployment id** and version number; the Marketplace SDK needs a
      versioned deployment, not the test/head deployment.
- [ ] Marketplace SDK > App Configuration: visibility (public or private), installation
      settings, app integration = Editor add-on with the deployment id, and the same OAuth
      scopes as the manifest.
- [ ] Marketplace SDK > Store Listing, exact asset sizes:
      application icon **128 x 128** and **32 x 32** PNG (`google-icon-128.png`,
      `google-icon-32.png`; a 96 x 96 and 48 x 48 pair is needed only if the listing
      includes a web app), **card banner 220 x 140** (`google-card-banner-220x140.png`;
      `-440x280` is the 2x version), screenshots **1280 x 800** recommended (640 x 400 and
      2560 x 1600 accepted, at least one, square corners, full bleed).
      Text: name (50), short description (200), detailed description (16,000), category,
      terms of service, privacy policy and support links (all required).
- [ ] Submit for Marketplace review; answer reviewer questions with the host file's steps
      and a copy link to the Google-native sample.

## 4. After a listing exists: what to re-run on every version bump

A change to a manifest, `Code.gs`, `Common.gs`, `Sidebar.html`, `Protocol.html`,
`appsscript.json`, a pane route or an icon is a new submission, not a hot fix.

1. `pnpm build:connectors && pnpm check:connectors` (regenerates Google copies, the
   Protocol bundle and the downloadable packages).
2. `pnpm exec vitest run src/lib/connectors` and the existing
   `src/lib/provenance/google-*.test.ts` (offline invariants, storage behaviour).
3. If images changed: `pnpm build:marketplace-assets`, commit the PNGs.
4. Raise the Office manifest `<Version>` (four numeric parts). Keep `<Id>` forever.
   Never change `ProviderName` or `DisplayName` casually: they must match the offer.
5. **Deploy the web build first.** Manifests load the live pane, icons and commands from
   `https://app.dynodoc.online`; then
   `python infra/demo/check-connectors.py https://app.dynodoc.online` must pass.
6. `pnpm validate:manifests`; re-upload the manifest in Partner Center. For Google,
   create a new versioned deployment and point the Marketplace SDK at it.
7. Re-run the affected ACCEPTANCE.md cases on real hosts and record the `Version`.
8. Compatibility: panes update the moment the web build deploys, while an installed
   Google script stays on its published version until Google updates it. Keep the
   `/api/connectors/*` protocol backward compatible with every version still published.

## 5. Screenshots (owner step)

Store screenshots must be **real captures from installed hosts**; nothing in this
repository fakes them. The shot-list is in each host file and the process is in
[extensions/marketplace/README.md](../../extensions/marketplace/README.md). Use
synthetic users and files (Maya Chen, Sam Okoye, Priya Nair; Board update, Q3 budget,
Launch plan).

## 6. Things that must not be claimed

- That a listing exists, is approved, or is "available in the Microsoft/Google store".
- Installed-host acceptance, any specific supported version or platform that has not
  passed ACCEPTANCE.md, user counts, uptime commitments, security certifications.
- Background capture, offline commits, formatting or structural sync, or proof of who
  typed a change.
- Microsoft single sign-on or Google sign-in as the identity used by the add-in.
- That the privacy policy or terms are adopted or effective; they are drafts until the owner adopts them.

## 7. Known gaps to be aware of before submitting

- Real-host acceptance has not been run for any of the six add-ins.
- Adopting the privacy policy and terms, and confirming the support mailbox is monitored, are owner items (the operator details are published).
- Dynodoc sign-in for reviewers (email link) needs a reviewer mailbox or Google sign-in.
- Google stores each user's queue in about 500 KB of script properties shared by all
  files; a full store stops capture with a message. There is no in-add-on clear or
  disconnect action, and no automatic expiry of local queues.
- Several error messages come from the shared provenance library
  (`src/lib/provenance/protocol.ts`, `word.ts`) and still say "development connector";
  they are not in the listing copy but a reviewer can trigger them with oversized files.
- PowerPoint on Office 2021 and earlier is unsupported; the listing must say so.
