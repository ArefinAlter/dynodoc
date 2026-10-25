<script lang="ts">
  import LegalPage from '$lib/components/marketing/LegalPage.svelte';
  import OperatorDetails from '$lib/components/marketing/OperatorDetails.svelte';
  import { OPERATOR } from '$lib/site/operator';
  const sections = [
    { id: 'scope', title: 'Scope and responsibility' },
    { id: 'information', title: 'Information we handle' },
    { id: 'purposes', title: 'How information is used' },
    { id: 'visibility', title: 'Who can see your work' },
    { id: 'provenance', title: 'History, signing keys and receipts' },
    { id: 'addins', title: 'Editor add-ins' },
    { id: 'measurements', title: 'Anonymous visits and performance' },
    { id: 'providers', title: 'Service providers and integrations' },
    { id: 'cookies', title: 'Cookies and local storage' },
    { id: 'retention', title: 'Retention, erasure and what stays' },
    { id: 'security', title: 'Security' },
    { id: 'rights', title: 'Your choices and rights' },
    { id: 'changes', title: 'Changes to this policy' },
    { id: 'contact', title: 'Contact and operator details' }
  ];
</script>

<LegalPage
  title="Privacy policy"
  description="How Dynodoc handles account information, documents, project history, signing keys, audit records and editor add-in data, and what can and cannot be erased."
  {sections}
>
  <section id="scope">
    <h2>Scope and responsibility</h2>
    <p>
      This draft covers the Dynodoc website at dynodoc.online, the hosted workspace and
      administration sites (app.dynodoc.online and admin.dynodoc.online) and the Dynodoc editor
      add-ins for Microsoft Word, Excel and PowerPoint and for Google Docs, Sheets and Slides. It
      also covers the workspace as an installable web app. Dynodoc is a document provenance, review
      and synchronization service: it records who changed what in a document, helps a team review
      and merge changes, and keeps that history checkable. A separately operated installation of the
      open-source engine and API may have a different operator and a different notice.
    </p>
    <p>
      “Dynodoc,” “we” and “our” refer to the operator of this hosted service, {OPERATOR.legalName},
      of Dhaka, Bangladesh, whose details are under
      <a href="#contact">Contact and operator details</a>. For account and service-operation
      information we decide how and why it is processed. For the documents and project history a
      team places in Dynodoc, that team or its organization generally decides what is uploaded, who
      may see it and how long it should be kept; we process it to provide the service. The precise
      controller, processor or equivalent roles depend on applicable law and any signed agreement.
      The
      <a href="/dpa">draft data processing terms</a> show how those roles would be set out; they are not
      an executed agreement.
    </p>
    <p>The service is not directed at children.</p>
    <div class="callout">
      <p>
        <strong>Not everything described here is switched on yet.</strong> This policy also covers
        capabilities that exist in the software but are not yet enabled on the hosted service, such
        as shared project history and signed branch receipts. The editor add-ins are not yet
        published in a marketplace. The
        <a href="/security#status">security overview</a> lists what is live today.
      </p>
    </div>
  </section>
  <section id="information">
    <h2>Information we handle</h2>
    <!-- Scrollable data tables need keyboard access on narrow screens. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" role="region" aria-label="Categories of information" tabindex="0">
      <table>
        <thead
          ><tr><th>Category</th><th>Examples and source</th><th>Why it is needed</th></tr></thead
        ><tbody
          ><tr
            ><td>Account and identity</td><td
              >Your email address, display name, account identifier and sign-in records, including
              login sessions and their revocation. These come from you or from the sign-in provider
              you choose.</td
            ><td>Identify you, authenticate your session and attribute contributions.</td></tr
          ><tr
            ><td>Profile</td><td
              >An optional display name and short bio. A profile is visible to you and to people who
              share a project with you; it never includes your email address. Per-day counts of your
              edits are shown to others only for files in projects and folders you both belong to.</td
            ><td>Let collaborators recognise each other.</td></tr
          ><tr
            ><td>Documents and project content</td><td
              >Uploaded files and the original upload, text, cells, slides, formatting, comments,
              suggestions, titles, file, folder and project names, and member roles.</td
            ><td>Provide capture, editing, import, export, review and collaboration.</td></tr
          ><tr
            ><td>History and provenance records</td><td
              >Append-only change records with author identifiers and timestamps, personal drafts,
              change requests, reviews, named versions, verified snapshots, signed project commits
              and branch receipts. These contain document content.</td
            ><td
              >Show changes in context, apply review rules, verify integrity and allow recovery.</td
            ></tr
          ><tr
            ><td>Audit and operations records</td><td
              >Administrative actions such as forced sign-out, link revocation, role changes and
              erasure receipts, digests of metadata changes, and security-relevant events.</td
            ><td>Run, secure and account for the service.</td></tr
          ><tr
            ><td>Connection and signing keys</td><td
              >Editor connection grants (file, editor, account, a hash of the key, expiry and
              revocation), account signing public keys and enrolment challenges. Private keys and
              the connection keys themselves are not stored by Dynodoc.</td
            ><td>Authorize editors and check who signed a commit.</td></tr
          ><tr
            ><td>Editor add-in data</td><td
              >What an add-in reads from the open file, its local queue, the identity markers it
              writes and what it sends. See <a href="#addins">Editor add-ins</a>.</td
            ><td>Capture and deliver the changes you choose to push.</td></tr
          ><tr
            ><td>Technical and support information</td><td
              >Web server request logs (IP address, time, method, path without the query string,
              status and size; private link paths are masked), error details, and information you
              send when you ask for help.</td
            ><td>Operate, troubleshoot and protect the service.</td></tr
          ></tbody
        >
      </table>
    </div>
    <p>
      An email address is required to use the signed-in workspace. Browsing the public website does
      not require an account. When you invite someone, we handle their email address to give them
      access and, if you choose, to send an invitation. Please invite only people you are authorized
      to invite.
    </p>
  </section>
  <section id="purposes">
    <h2>How information is used</h2>
    <p>
      We use information to sign you in, store and return your work, show history and attribution,
      apply file and project permissions, process requested imports and exports, send sign-in and
      invitation emails, deliver in-app notifications, detect likely copies of documents (described
      below), respond to support needs, investigate errors and misuse, and keep the records the
      service needs in order to operate.
    </p>
    <p>
      The service has no advertising trackers, no marketing profiling, no sale of content, no use of
      your content to train AI models and no automated decisions about people with legal or
      similarly significant effects. Document content is not sent to an AI service.
    </p>
  </section>
  <section id="visibility">
    <h2>Who can see your work</h2>
    <p>
      Access is controlled by project, folder and file membership with the roles Owner, Manager,
      Editor, Reviewer, Contributor and Viewer. The strongest applicable grant wins; a narrower
      grant does not cancel inherited access. People with permission to manage membership can give
      others access. Project rules can require approvals before a change is merged and apply to
      every file in the project.
    </p>
    <p>
      Personal drafts stay private until you share them. Once changes are merged, earlier wording
      and contributor identities are visible to anyone authorized to see the file’s history. A
      collaborator who can view or export content may keep a copy outside Dynodoc; removing access
      cannot recall copies they already have.
    </p>
    <p>
      To recognise renamed or edited copies, Dynodoc fingerprints document text and compares it with
      other documents in the service. When a document you upload or create closely matches a
      document owned by someone else, that owner is told your name, email address, the file name and
      how similar the two are, even if you cannot open their document. They are not shown your
      content. Before uploading, you are only shown matches in documents you can already open.
    </p>
    <p>
      Owners and managers can publish a frozen read-only copy with a separate link. Anyone who has
      the link can read it without signing in. Comments, original uploads and version history stay
      private. Revoking a link or moving the file to Trash stops future access through the link, but
      cannot recall copies already downloaded. Later edits are not included until a new copy is
      published.
    </p>
    <p>
      Authorized administrators may access stored information when needed to operate or support the
      service, and the operator administers the database. Dynodoc is not end-to-end encrypted: it
      reconstructs and reviews versions on the server, so it holds document content in a form the
      service can read. Access roles restrict application access; they do not prevent the
      infrastructure operator from administering the database.
    </p>
    <p>
      We may need to disclose information to comply with a binding legal requirement or to address a
      substantiated security issue.
    </p>
  </section>
  <section id="provenance">
    <h2>History, signing keys and receipts</h2>
    <p>
      Dynodoc’s history is append-only. Each recorded change is linked to the one before it with a
      SHA-256 hash, so a later alteration of the record is detectable. Deleting a paragraph,
      restoring an older version or reverting a change adds new records; it does not remove the old
      ones.
    </p>
    <p>
      Signed project commits and branch receipts, where enabled, carry Ed25519 signatures. To sign,
      you enrol a public key with your account by proving that you hold the matching private key.
      The private key stays on your device and is never uploaded. The service keeps the public key
      with its enrolment and revocation times, and no email address, device label or secret.
      Revoking a key is permanent, and commits it already signed remain verifiable. A branch receipt
      records the project, branch, commit, a generation counter and a declared time, signed by the
      service’s authority key. Receipts hold identifiers and hashes, not document text.
    </p>
    <p>
      These records are pseudonymous, not anonymous: they identify an account by its internal
      identifier, and the history they point to can contain personal information that people wrote.
      They stay with the history, including after an account is erased, so that contributions remain
      verifiable. See <a href="#retention">Retention, erasure and what stays</a>.
    </p>
  </section>
  <section id="addins">
    <h2>Editor add-ins</h2>
    <p>
      The add-ins for Word, Excel and PowerPoint (task panes hosted at app.dynodoc.online) and for
      Docs, Sheets and Slides (Google Apps Script add-ons) capture changes in the file you have open
      and send them to Dynodoc when you choose. This section describes what the add-in code does.
    </p>
    <h3>What an add-in reads</h3>
    <p>
      Only the open file, and only while the pane or sidebar is open. Word and Docs: the text of
      body paragraphs (Docs: the active tab), up to 2,000 paragraphs; a table, list, image or
      embedded object stops the adapter with a message. Excel and Sheets: values, formula text,
      date-only values and number formats of cells in rows mapped to Dynodoc, up to 2,000 cells,
      plus sheet names and whether cells are merged. PowerPoint and Slides: the text of existing
      text shapes and the identity of slides and shapes, up to 2,000 shapes. The add-ins do not read
      other files or tabs, contacts, mail, calendar, Drive listings or your account profile, and
      they request no permission that could.
    </p>
    <h3>What stays on your device</h3>
    <ul>
      <li>
        <strong>Office:</strong> a capture queue in the browser database (IndexedDB) of the Office web
        view, keyed to the Dynodoc file and the saved file’s address. The connection key is held only
        in the pane’s memory and is gone when the pane closes.
      </li>
      <li>
        <strong>Google:</strong> the queue and the connection key are kept in Apps Script UserProperties,
        which are private to the Google user who installed the add-on and cannot be read by collaborators
        on the file. A file’s queue is capped at 100 KB.
      </li>
      <li>
        A queue holds the text of the file’s synced fields as last served by Dynodoc, the last
        observed text of covered items, the observed changes with timestamps and identifiers, any
        frozen upload awaiting delivery and the last receipt.
      </li>
      <li>
        There is no automatic expiry and no in-product delete button yet. Office queues remain until
        you clear the web view’s storage or remove the add-in. For Google, what happens to stored
        properties when you uninstall is Google’s platform behaviour, which we have not confirmed.
        <em>Export local provenance</em> gives you a JSON copy of what is held.
      </li>
    </ul>
    <h3>The connection key</h3>
    <p>
      The add-in creates a 32-byte random key. Only its SHA-256 hash appears in the approval address
      that opens in your browser; the key itself is sent only in an authorization header from the
      editor to Dynodoc. You approve while signed in, choose one file and confirm a code shown in
      both places. Dynodoc stores a grant (file, editor, account, key hash, expiry and revocation),
      not the key. The key can read and propose changes for that one file and editor only. It cannot
      merge, grant access or act as your account session, it expires seven days after approval and
      you can revoke it under Sync &amp; connections, which stops access immediately. No Dynodoc
      password, session cookie or account-wide token is ever given to an add-in.
    </p>
    <h3>What is sent to Dynodoc</h3>
    <p>
      Every request goes to <code>https://app.dynodoc.online/api/connectors/</code> over HTTPS; the Google
      manifests enforce that single address. The add-in checks the connection, fetches the file’s retained
      text and history position when you connect, preview or recover, and, only when you choose Push for
      review, sends a bundle: a bundle identifier, a random client identifier, the file identifier, the
      base revision and hash, the editor and capture mode, the Dynodoc file title, the change request
      name and note you typed and, for each change, the node identifier, field, new text or value and
      observed time. Nothing is sent while you only edit, nothing is sent in the background, and no device
      identifier or telemetry is sent.
    </p>
    <h3>What is written into your file</h3>
    <p>
      Identity markers only: Word content controls tagged <code>dynodoc.block.&lt;id&gt;</code>,
      named ranges in Excel, Sheets and Docs, and tags on PowerPoint slides and shapes. They travel
      with the file when it is shared or exported and contain identifiers, not text. Text in your
      file changes only when you choose <em>Apply preview</em>, after a compare-before-write check,
      and only for text that has been reviewed and merged in Dynodoc.
    </p>
    <h3>Third parties and what is not done</h3>
    <p>
      Office panes load Microsoft’s Office.js library from appsforoffice.microsoft.com and run
      inside Office, which has its own data practices. Google add-ons run on Google’s Apps Script
      platform, which may log execution errors because exception logging is enabled in the add-on
      manifests. The add-ins contain no analytics, advertising, fingerprinting or error-reporting
      service, set no cookies and capture nothing while the pane or sidebar is closed. They record
      what they observe at snapshot times; they do not prove who typed a change.
    </p>
  </section>
  <section id="sample-workspaces">
    <h2>Sample workspaces and the desktop app (pre-alpha)</h2>
    <p>
      Anyone can start a sample workspace from the <a href="/try">Try the pre-alpha</a> page without
      an account. It is a separate account with an invented address on
      <code>visitors.dynodoc.invalid</code>, filled with invented sample content. It is removed 24
      hours after it was started: you are taken out of its projects and its files are moved to the
      trash. To limit misuse, the address you start one from is counted for an hour, kept only as a
      shortened one-way hash. Please do not put personal or confidential information in a sample
      workspace.
    </p>
    <p>
      The desktop app keeps your preferences in a file on your computer. When it is signed in, a
      small service beside the workspace keeps, for your account, the changes captured but not yet
      committed, commits not yet pushed, commit messages and the agent threads you start in the app.
      They are removed with a sample workspace, and on request for any account.
    </p>
  </section>
  <section id="measurements">
    <h2>Anonymous visits and performance</h2>
    <p>
      We measure page loads and browser performance (LCP, INP, CLS, FCP and TTFB) to find slow or
      unstable pages. Each page load gets a random identifier held only in page memory. A sample
      contains the site, a broad page category such as home, editor or sign-in, a metric name and
      its numeric value. It does not contain your IP address, email address, document identifier,
      full address, query string or anything you type. The request omits cookies, no tracking cookie
      or persistent identifier is used, and samples are kept for 30 days.
    </p>
    <p>
      Counts are page loads, not unique people. Collection is skipped when your browser sends Do Not
      Track or Global Privacy Control, or when you switch it off on the
      <a href="/cookies#measurements">cookies page</a>. Web server logs are separate operational
      records and do contain IP addresses, as described above.
    </p>
  </section>
  <section id="providers">
    <h2>Service providers and integrations</h2>
    <ul>
      <li>
        <strong>Google, when enabled:</strong> sign-in requests your identity, email address and
        profile only. It does not request Google Drive, Docs or Sheets access. Google handles
        sign-in activity under its own
        <a href="https://policies.google.com/privacy">privacy policy</a>.
      </li>
      <li>
        <strong>Resend, when configured:</strong> receives the recipient address and the message
        needed to deliver a sign-in link or an invitation. Its handling is described in the
        <a href="https://resend.com/legal/privacy-policy">Resend privacy policy</a>.
      </li>
      <li>
        <strong>Vercel:</strong> serves the public website at dynodoc.online and so receives
        ordinary request information such as IP address and the page requested. See the
        <a href="https://vercel.com/legal/privacy-policy">Vercel privacy notice</a>. The signed-in
        workspace does not run on Vercel.
      </li>
      <li>
        <strong>Hosting provider:</strong> the workspace, API, database, document converter and backups
        run on a server the operator rents.
      </li>
      <li>
        <strong>Microsoft and Google as editor platforms:</strong> the add-ins run inside their products,
        as described under Editor add-ins.
      </li>
    </ul>
    <p>
      The <a href="/subprocessors">subprocessors page</a> lists these providers, what each receives and
      what is not used. Website fonts, icons and artwork are served from Dynodoc itself, and the document
      converter runs on the same private network rather than at a third party.
    </p>
    <h3>Optional source imports</h3>
    <p>
      The workspace can import a form definition or a set of responses from an external form or
      survey platform you point it at. The server contacts the address you give it using a
      credential you supply for that request; the credential is not intentionally saved in the
      application database or browser storage. A Google Forms import uses a separate, read-only
      Google authorization (<code>forms.body.readonly</code> and
      <code>forms.responses.readonly</code>) whose short-lived access token is kept encrypted in an
      HTTP-only cookie for up to one hour; no refresh token is requested, and signing out or
      disconnecting clears it. Imported material becomes workspace content under the file’s access,
      history and retention rules, and disconnecting does not erase imported copies.
    </p>
    <h3>External content</h3>
    <p>
      If a document contains an image hosted elsewhere, a viewer’s browser may contact that host and
      disclose ordinary request information to it. External links lead to services with their own
      policies.
    </p>
  </section>
  <section id="cookies">
    <h2>Cookies and local storage</h2>
    <p>
      Dynodoc sets first-party cookies for your sign-in session, the sign-in handshake, your light
      or dark preference and, if you change it, your performance-measurement choice, plus a
      short-lived cookie for the optional Google Forms import. There are no advertising cookies and
      no persistent visitor-tracking identifiers. The browser also stores a few preferences, the
      optional recovery copies you enable, and, through a service worker, an offline page and copies
      of the site’s public script, style and font files (never pages, documents or account data).
      The <a href="/cookies">cookies page</a> lists every item with its lifetime and lets you control
      the optional measurements.
    </p>
  </section>
  <section id="retention">
    <h2>Retention, erasure and what stays</h2>
    <div class="callout">
      <p>
        <strong>Removing text from the current version is not erasure.</strong> Dynodoc preserves history.
        Earlier wording, deleted cells or slides, attribution and original uploads can remain in historical
        records until the whole document is erased.
      </p>
    </div>
    <h3>Archive and Trash</h3>
    <p>
      Archiving changes a file’s visibility. Trash blocks ordinary access and revokes public links
      while preserving content and history so it can be recovered. These actions, like other
      administrative actions, are recorded in a separate operations log and erase nothing.
    </p>
    <h3>Erasing a whole document</h3>
    <p>
      An administrator can permanently erase a whole document that is already in Trash, after
      reviewing an impact preview, typing its title, choosing a reason category and acknowledging
      that backups persist. In one transaction this removes its files, content, change history,
      snapshots, named versions, personal drafts, public copies and cached request responses from
      the live database. A minimal receipt remains: the resource identifier and kind, the
      administrator’s identifier, the reason category, aggregate counts and the time. It holds no
      title, email address, file or content. Where shared project history is enabled, published
      history that depended on the document is removed with it, leaving only opaque receipt
      commitments and generation counters.
    </p>
    <h3>Erasing an account</h3>
    <p>
      Account erasure requires the account to be disabled first and its owned documents and
      workspaces to be transferred or removed. It removes sessions, pending sign-in links,
      memberships and personal drafts and turns the email address and display name into a
      deleted-account identity. The internal identifier remains so that shared contributions and
      their hashes stay verifiable. Signing keys are revoked; their public bindings stay with the
      historical authorship. Using the same email address again creates a new identity with no
      inherited access, and an erased identity cannot be restored.
    </p>
    <h3>What cannot be removed</h3>
    <ul>
      <li>
        <strong>One past change inside a retained history.</strong> There is no feature that redacts a
        single revision and keeps the rest of the chain intact; the only removal is erasure of the whole
        document.
      </li>
      <li>
        <strong>What you wrote in other people’s documents.</strong> Account erasure is not content redaction
        across other people’s documents.
      </li>
      <li>
        <strong>Copies outside Dynodoc:</strong> downloads, exports, published links already used, files
        in an editor and add-in queues on your own devices.
      </li>
      <li>
        <strong>Backups.</strong> Database backups are kept for a rolling 14 days on the same server as
        the live system and expire on that schedule; erasure does not reach into them. An off-site copy
        is not currently configured.
      </li>
      <li>
        <strong>Receipts and audit records:</strong> erasure receipts, signed branch receipt commitments
        and the operations log remain under their own retention.
      </li>
    </ul>
    <h3>How long things are kept</h3>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" role="region" aria-label="Retention periods" tabindex="0">
      <table>
        <thead><tr><th>Item</th><th>How long</th></tr></thead><tbody
          ><tr
            ><td>Login session cookie</td><td>Up to 30 days; ends at sign-out or revocation.</td
            ></tr
          ><tr><td>Email sign-in link</td><td>Valid for 15 minutes.</td></tr><tr
            ><td>Editor connection key</td><td
              >Expires seven days after approval; revocable at any time.</td
            ></tr
          ><tr><td>Signing-key enrolment challenge</td><td>Five minutes.</td></tr><tr
            ><td>Performance measurements</td><td>30 days.</td></tr
          ><tr><td>Database backups</td><td>Rolling 14 days.</td></tr><tr
            ><td>Browser recovery copies (your device)</td><td
              >Seven days, at most 30 copies of 10 MiB each, only if you enable them.</td
            ></tr
          ><tr
            ><td>Documents, history, drafts and audit records</td><td
              >Until erased through the processes above; there is no automatic expiry.</td
            ></tr
          ></tbody
        >
      </table>
    </div>
    <p>
      A deletion or correction request must be assessed by the operator and, where appropriate, the
      organization responsible for the project. The assessment must account for the request,
      applicable rights, legal retention duties and the append-only history; history is not a
      blanket exemption from deletion rights. There is no self-service or automatic erasure today,
      so requests go through the contact details below.
    </p>
  </section>
  <section id="security">
    <h2>Security</h2>
    <p>
      The hosted service uses HTTPS, signed-in sessions in protected cookies, document and project
      permissions, a restricted database login that cannot rewrite history, integrity checks over
      recorded history and limits on request volume. The <a href="/security">security overview</a>
      explains each of these with its limits. Integrity checks help detect changes to recorded history;
      they do not prove that content is correct or prevent every form of unauthorized access.
    </p>
    <p>
      No internet service can guarantee absolute security. This draft claims no end-to-end
      encryption, no particular encryption-at-rest configuration, no external security certification
      and no regulated-data hosting standard. If a personal-data incident occurs, the operator and
      the responsible organization must assess it and meet applicable notification duties.
    </p>
  </section>
  <section id="rights">
    <h2>Your choices and rights</h2>
    <p>
      You can review the content you are authorized to access, export supported files, manage access
      where your role permits, revoke editor connections, enrol or revoke signing keys, sign out,
      and choose not to use optional connections or measurements.
    </p>
    <p>
      Depending on the law that applies, you may have rights to obtain information or a copy,
      correct inaccurate personal information, request erasure, restrict processing, receive
      portable data, withdraw consent or complain to a supervisory authority. These rights can have
      conditions or exceptions, and the append-only history is one of the things the operator must
      weigh.
    </p>
    <p>
      <strong
        >You may also have a right to object to processing based on legitimate interests.</strong
      > If applicable, the operator must consider the objection and explain its response. Where consent
      is the basis, withdrawing it does not affect the lawfulness of processing before withdrawal.
    </p>
    <p>
      To exercise a right, write to the email address under
      <a href="#contact">Contact and operator details</a>. A person named in a document may also
      contact the organization responsible for that project. Do not send sensitive documents through
      public repository issues. Identity checks should be proportionate, and requests must be
      handled within applicable legal deadlines.
    </p>
  </section>
  <section id="changes">
    <h2>Changes to this policy</h2>
    <p>
      Material changes to purposes, providers or data handling should be communicated before they
      take effect when required, and a revised policy does not authorize incompatible uses of
      information already collected. For the rules proposed for using the service, see the
      <a href="/terms">terms and conditions</a>.
    </p>
  </section>
  <section id="contact">
    <h2>Contact and operator details</h2>
    <OperatorDetails emailLabel="Privacy and legal email" />
    <p>
      To report a security vulnerability, see <a href="/security#report">the security overview</a>.
    </p>
  </section>
</LegalPage>

<style>
  .table-scroll table {
    min-width: 600px;
  }
</style>
