<script lang="ts">
  import LegalPage from '$lib/components/marketing/LegalPage.svelte';
  import Dino from '$lib/art/Dino.svelte';
  import Fossil from '$lib/art/Fossil.svelte';
  const sections = [
    { id: 'summary', title: 'Overview' },
    { id: 'status', title: 'What is live today' },
    { id: 'history', title: 'What the history guarantees' },
    { id: 'access', title: 'Access, sessions and keys' },
    { id: 'infrastructure', title: 'Hosting and network' },
    { id: 'limits', title: 'Limits and what we do not claim' },
    { id: 'report', title: 'Report a vulnerability' }
  ];
  const ADVISORY = 'https://github.com/ArefinAlter/dynodoc-engine/security/advisories/new';
</script>

<LegalPage
  title="Security"
  description="How Dynodoc protects the history of your documents and access to it, what is live today, what is only in source code so far, and how to report a vulnerability."
  {sections}
>
  <section id="summary">
    <h2>Overview</h2>
    <p>
      Dynodoc keeps a record of who changed what in a document, so most of its security work
      protects that record and the access to it. This page says what is in place, what exists only
      in source code so far and what is not claimed. It is written from the project’s own
      engineering documents and deployment configuration; where something has not been verified it
      says so.
    </p>
    <ul>
      <li>
        Changes go into an append-only, hash-chained log that the database refuses to edit, and the
        service’s own database login cannot rewrite or delete it.
      </li>
      <li>Sessions, editor connection keys and signing keys are scoped, expiring and revocable.</li>
      <li>
        Pages are served over HTTPS with a strict content security policy and no third-party
        scripts.
      </li>
      <li>
        There is no external audit, no certification and no independent witness of the history yet.
        The limits are listed below rather than left for you to find.
      </li>
    </ul>
  </section>
  <section id="status">
    <h2>What is live today</h2>
    <p>
      As of 12 October 2026 the hosted service runs the controls marked <strong>Live</strong>. Those
      marked <strong>In source</strong> are implemented and tested in the code but not yet deployed.
      Those marked <strong>Not built</strong> do not exist.
    </p>
    <!-- Scrollable data tables need keyboard access on narrow screens. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      class="table-scroll"
      role="region"
      aria-label="Security controls and their state"
      tabindex="0"
    >
      <table>
        <thead><tr><th>Control</th><th>What it does</th><th>State</th></tr></thead><tbody
          ><tr
            ><td>Hash-chained, append-only history</td><td
              >Each change event is linked to the previous one with SHA-256. Sequence gaps are
              checked, and the database blocks UPDATE and DELETE of events.</td
            ><td><strong>Live</strong></td></tr
          ><tr
            ><td>Checked snapshots</td><td
              >Snapshots carry immutable receipts. Reads replay the event chain and compare the full
              state, and fail closed on a missing or mismatched receipt.</td
            ><td><strong>Live</strong></td></tr
          ><tr
            ><td>Restricted database login</td><td
              >The API connects with a role that cannot alter history, disable triggers, change
              schemas or elevate itself, and refuses to start with an owner or superuser role.
              Migrations run separately, by the operator.</td
            ><td><strong>Live</strong></td></tr
          ><tr
            ><td>Revocable sessions</td><td
              >Signing out revokes the login on the server, not only the cookie.</td
            ><td><strong>Live</strong></td></tr
          ><tr
            ><td>Project rules and metadata audit</td><td
              >Project rules set a floor of protection and approvals for every file. Changes to
              titles, settings and archive state are recorded with before and after digests.</td
            ><td><strong>Live</strong></td></tr
          ><tr
            ><td>Browser policy</td><td
              >Nonce-based content security policy, HSTS over HTTPS, no MIME sniffing, strict
              referrer policy and restricted framing.</td
            ><td><strong>Live</strong></td></tr
          ><tr
            ><td>Request and workload limits</td><td
              >Bounded concurrent responses, per-identity read budgets, SQL timeouts, a 16 MiB
              request cap, 3 MB file uploads and proxy rate limits on sign-in.</td
            ><td><strong>Live</strong></td></tr
          ><tr
            ><td>Hardened containers</td><td
              >Web, API and converter run with read-only file systems, no Linux capabilities and
              memory, CPU and process limits. The database, API and converter are not publicly
              reachable.</td
            ><td><strong>Live</strong></td></tr
          ><tr
            ><td>Audited erasure</td><td
              >An administrator can erase a whole document, leaving a minimal receipt. No other
              operation removes canonical history.</td
            ><td><strong>Live</strong></td></tr
          ><tr
            ><td>Checked backups</td><td
              >PostgreSQL archives are validated before they are kept, for 14 days, on the same
              server, so they are not independent recovery.</td
            ><td><strong>Live</strong></td></tr
          ><tr
            ><td>Published source</td><td
              >The source archive of the deployed revision is downloadable from
              <a href="/source" data-sveltekit-reload>/source</a>.</td
            ><td><strong>Live</strong></td></tr
          ><tr
            ><td>Account signing keys</td><td
              >An Ed25519 public key is bound to your account by proving possession of the private
              key. Revocation is permanent.</td
            ><td><strong>In source</strong></td></tr
          ><tr
            ><td>Signed project history and branch receipts</td><td
              >Signed commits, protected main branch, reviews bound to an exact candidate, and
              signed receipts issued with each accepted branch update, verified against a pin kept
              on the client.</td
            ><td><strong>In source</strong></td></tr
          ><tr
            ><td>Durable storage budgets</td><td
              >Per-document and per-account limits on retained shared objects.</td
            ><td><strong>In source</strong></td></tr
          ><tr
            ><td>Independent witness of history</td><td
              >Signed heads kept by someone other than the service, to expose a rewrite.</td
            ><td><strong>Not built</strong></td></tr
          ><tr
            ><td>Off-site backups</td><td>A second copy that survives loss of the server.</td><td
              ><strong>Not built</strong></td
            ></tr
          ></tbody
        >
      </table>
    </div>
  </section>
  <section id="history">
    <h2>What the history guarantees</h2>
    <p>
      Each event stores a SHA-256 hash of its type, target, payload and actor, plus a chain hash
      that combines it with the previous event. Altering, removing or reordering an earlier event
      changes every later chain hash, which verification detects. Verification, available through
      the API and the operator tool, replays the chain from the first event and rebuilds each stored
      snapshot to compare it with the stored state, so a corrupt snapshot cannot hide behind a
      healthy later one.
    </p>
    <p>
      This is a consistency check, not a proof of truth. The first hash version does not cover the
      document identifier, event identifier, sequence number or creation time. Receipts on snapshots
      are internal attestations, not signatures. Someone who controls the whole database can disable
      triggers and rewrite both the history and its checks, and the hash chain alone cannot expose
      that until the history is committed to something outside the database. Signed and
      independently witnessed heads are planned and are not in place.
    </p>
    <p>
      Restoring, reverting or deleting content adds new events. The only removal is an
      administrator’s erasure of a whole document, which keeps a minimal receipt; see
      <a href="/privacy#retention">what can and cannot be erased</a>.
    </p>
  </section>
  <section id="access">
    <h2>Access, sessions and keys</h2>
    <ul>
      <li>
        <strong>Sign-in:</strong> an emailed link valid for 15 minutes, or Google sign-in limited to identity,
        email and profile. The session is an encrypted, HTTP-only cookie for up to 30 days whose login
        can be revoked on the server; an open event stream stops before its next protected event once
        the login is revoked.
      </li>
      <li>
        <strong>Permissions:</strong> roles are checked again on every read and write, and project rules
        set a minimum that file rules cannot weaken. A published link or a content hash never grants access
        to private history.
      </li>
      <li>
        <strong>Editor connection keys:</strong> 32 random bytes made in the add-in, of which Dynodoc
        stores only a hash. A key reaches one file and one editor, can read and propose but not merge
        or grant access, expires after seven days and can be revoked at once.
      </li>
      <li>
        <strong>Signing keys (in source):</strong> enrolment needs a five-minute, single-use challenge
        signed by the private key, which never leaves your device. Challenge requests and active keys
        per account are limited.
      </li>
    </ul>
  </section>
  <section id="infrastructure">
    <h2>Hosting and network</h2>
    <ul>
      <li>
        <strong>Transport:</strong> HTTPS only, with HTTP redirected and TLS 1.2 or 1.3 on the workspace.
        HSTS lasts one year and is not set for subdomains or preloaded.
      </li>
      <li>
        <strong>Scripts:</strong> only from Dynodoc’s own origin with a per-response nonce, no inline
        script or eval, and no third-party analytics, fonts or CDNs on the website. The three Office panes
        additionally allow Microsoft’s Office.js hosts and may be framed by Office.
      </li>
      <li>
        <strong>Limits:</strong> at most 64 ordinary and 256 streaming responses in flight; 120 expensive
        reads per identity per minute with two active at a time; a 15-second SQL statement timeout. Sign-in
        routes are limited to 60 requests per minute per address, with a short burst allowance.
      </li>
      <li>
        <strong>Logs:</strong> web server access logs record address, time, method, path (without the
        query string, so sign-in tokens and OAuth codes are not logged), status and size, and mask private
        link paths.
      </li>
      <li>
        <strong>Backups:</strong> validated PostgreSQL archives, kept 14 days on the same server. Restores
        are exercised into a fresh database, never over the live one.
      </li>
    </ul>
  </section>
  <section id="limits">
    <h2>Limits and what we do not claim</h2>
    <ul>
      <li>
        <strong>No independent assurance.</strong> Dynodoc has no security certification such as SOC 2
        or ISO 27001, no independent security audit or penetration test, and makes no claim of compliance
        with GDPR, HIPAA or another regulation.
      </li>
      <li>
        <strong>Not end-to-end encrypted.</strong> The service reconstructs and reviews versions on the
        server, so the operator can read content. A particular encryption-at-rest configuration is not
        claimed.
      </li>
      <li>
        <strong>The operator is trusted.</strong> The database owner and credential rotation are trusted
        operations. Startup checks cannot stop an operator from changing privileges later, and the permitted
        audited erasure path remains available to administrators.
      </li>
      <li>
        <strong>One server, no uptime promise.</strong> Backups share the server with the live system,
        so they are not independent recovery. There is no service-level agreement. Workload limits are
        per process, allow a burst at window boundaries and do not make search indexed or stop all CPU-heavy
        work.
      </li>
      <li>
        <strong>Add-ins are pre-release.</strong> They are not in a marketplace, and acceptance testing
        in installed Word, Excel, PowerPoint, Docs, Sheets and Slides is still to be done by the owner.
        They record observations at snapshot times and do not prove who typed a change.
      </li>
      <li>
        <strong>Signed history needs more.</strong> Even with signed receipts, a server holding its signing
        key can show different histories to different clients until independent witnesses exist.
      </li>
    </ul>
  </section>
  <section id="report">
    <h2>Report a vulnerability</h2>
    <p>
      Please report a suspected vulnerability privately and do not publish it. Use
      <a href={ADVISORY}>GitHub private vulnerability reporting</a> for the public Dynodoc engine repository;
      it is the only reporting channel with a confirmed owner today, and reports about any part of Dynodoc,
      including this website and the add-ins, are welcome there. A dedicated security email address is
      awaiting confirmation.
    </p>
    <dl class="contact-details">
      <dt>Reporting channel</dt>
      <dd><a href={ADVISORY}>GitHub private vulnerability reporting</a></dd>
      <dt>Security email</dt>
      <dd>Awaiting confirmation.</dd>
      <dt>Machine-readable</dt>
      <dd>
        <a href="/.well-known/security.txt" data-sveltekit-reload>/.well-known/security.txt</a>
      </dd>
    </dl>
    <p>
      Include the affected address or revision, the impact, a minimal reproduction that uses
      synthetic data and any suggested fix. Test only against your own account and data. Do not
      publish credentials, private documents or a working exploit, and do not run denial-of-service
      or volume tests. Areas of particular interest are authorization across drafts and public
      views, event integrity and replay, parser and resource limits, authentication, administrative
      erasure and SQL concurrency.
    </p>
    <p>
      This is an early project: fixes target the current main branch and no response time is
      guaranteed. No legal safe harbour is offered by this draft; the operator must decide that with
      its final terms. See also the <a href="/acceptable-use">acceptable use page</a>.
    </p>
    <div class="plate"><Dino name="fossil" art={Fossil} /></div>
  </section>
</LegalPage>

<style>
  .table-scroll table {
    min-width: 600px;
  }

  .table-scroll td:last-child {
    white-space: nowrap;
  }
  .plate {
    width: min(100%, 240px);
    margin-top: 2rem;
    color: var(--color-ink-soft);
    --art-accent: var(--color-accent);
  }
</style>
