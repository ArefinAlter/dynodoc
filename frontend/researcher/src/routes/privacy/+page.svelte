<script lang="ts">
  import LegalPage from '$lib/components/marketing/LegalPage.svelte';
  const sections = [
    { id: 'scope', title: 'Scope and responsibility' },
    { id: 'information', title: 'Information we handle' },
    { id: 'performance', title: 'Anonymous visits and performance' },
    { id: 'purposes', title: 'How information is used' },
    { id: 'collaboration', title: 'Who can see your work' },
    { id: 'providers', title: 'Service providers and integrations' },
    { id: 'cookies', title: 'Cookies and sign-in' },
    { id: 'history', title: 'History, retention and deletion' },
    { id: 'security', title: 'Security and storage' },
    { id: 'research', title: 'Research and sensitive data' },
    { id: 'rights', title: 'Your choices and rights' },
    { id: 'changes', title: 'Changes to this notice' },
    { id: 'contact', title: 'Contact and operator details' }
  ];
</script>

<LegalPage
  title="Privacy policy"
  description="How the Dynodoc research pilot handles account information, research materials, collaboration history and connected services."
  {sections}
>
  <section id="scope">
    <h2>Scope and responsibility</h2>
    <p>
      This draft covers the Dynodoc homepage and the associated hosted research workspace. It
      explains the behavior of the current pilot. A separately operated installation may have a
      different operator, hosting arrangement and privacy notice.
    </p>
    <p>
      “Dynodoc,” “we” and “our” refer to the operator of this hosted service, whose legal identity
      is still to be supplied below. We handle account and service-operation information to run the
      workspace. A research organization that uploads study materials generally determines why those
      materials are processed, who may access them and what participant information belongs in them.
    </p>
    <p>
      The precise controller, processor or equivalent roles depend on applicable law and the study
      arrangement. This notice is not a data processing agreement, a participant information sheet
      or an ethics approval. Those arrangements must be agreed before the pilot is used for
      identifiable participant data.
    </p>
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
              >Your email address, supplied display name, account identifier and sign-in records.
              These come from you or your selected sign-in provider.</td
            ><td>Identify you, authenticate your session and attribute contributions.</td></tr
          ><tr
            ><td>Research content</td><td
              >Uploaded documents and workbooks, original filenames and files, paragraphs, cells,
              formulas, questions, choices, notes, comments and suggestions.</td
            ><td>Provide editing, import, export, review and collaboration.</td></tr
          ><tr
            ><td>Version and collaboration records</td><td
              >Content changes, author identifiers, timestamps, saved versions, drafts, document
              membership, roles and import provenance.</td
            ><td>Show changes in context, manage access and preserve a working history.</td></tr
          ><tr
            ><td>Integration information</td><td
              >The server URL, project or form you select, the token supplied for a request, and the
              instrument or response data you import.</td
            ><td>Retrieve the requested material from your research platform.</td></tr
          ><tr
            ><td>Technical and support information</td><td
              >Server request information, which may include IP address, request path, time, browser
              information and error details; information you give an administrator when requesting
              help.</td
            ><td>Operate, troubleshoot and protect the service.</td></tr
          ></tbody
        >
      </table>
    </div>
    <p>
      An email address is required to use the authenticated workspace. Browsing the public homepage
      does not require an account. The interactive homepage examples use sample information and keep
      their edits in the page’s memory; they do not create research records or submit example
      responses.
    </p>
  </section>
  <section id="performance">
    <h2>Anonymous visits and performance</h2>
    <p>
      We measure page loads and browser performance (LCP, INP, CLS, FCP and TTFB) to find slow or
      unstable pages. Each page load gets a random identifier held only in page memory. Samples
      include the site, a broad page category, a metric and its numeric value. They do not contain
      your IP address, email, document identifier, full URL, query string, text you type or research
      content. These measurements use no tracking cookies or persistent identifiers and are retained
      for 30 days.
    </p>
    <p>
      The administrator dashboard shows page-load counts and aggregate performance. Counts do not
      identify unique visitors. Collection is skipped when your browser sends Do Not Track or Global
      Privacy Control. Server access logs are separate operational records and may still contain IP
      addresses as described above.
    </p>
  </section>
  <section id="purposes">
    <h2>How information is used</h2>
    <p>
      The pilot uses information to sign you in, save and retrieve work, display history and
      attribution, apply document permissions, process requested imports and exports, send sign-in
      emails, respond to support needs and investigate errors or misuse.
    </p>
    <p>
      The current application does not include advertising trackers, marketing profiling, sale of
      research content, AI model training or automated decisions about people with legal or
      similarly significant effects. It does not send document contents to an AI service.
    </p>
    <h3>Legal basis</h3>
    <p>
      Where a law requires a legal basis, the operator must identify it for each purpose before this
      draft becomes effective. Account and workspace operation may be necessary to provide the
      requested service; proportionate security and support processing may rely on legitimate
      interests where permitted; legal obligations may require particular records. Consent, where
      used, must be specific and withdrawable. These examples do not establish a legal basis for
      sensitive research data or replace the research organization’s assessment.
    </p>
  </section>
  <section id="collaboration">
    <h2>Who can see your work</h2>
    <p>
      Document access is controlled by direct membership and inherited organization, team and folder
      roles. The strongest applicable grant wins; a narrower grant does not cancel inherited access.
      Editors, reviewers and readers have different capabilities. People with permission to manage
      membership can give others access. A collaborator who is allowed to view or export content may
      retain a copy outside Dynodoc; removing access cannot recall copies they already obtained.
    </p>
    <p>
      Personal drafts are kept separate from the shared team version until you share changes. Saved
      versions and attribution may reveal earlier wording and contributor identities to people
      authorized to view the relevant document history.
    </p>
    <p>
      Owners and managers can publish a frozen read-only copy with a separate link. Anyone who
      obtains that link can read the published content without signing in. Comments, original
      uploads and version history remain private. Revoking a link or moving the document to Trash
      stops future access through the link, but cannot recall downloaded copies. Later edits are not
      included until a new copy is published.
    </p>
    <p>
      Authorized service administrators may access stored information when needed to operate or
      support the service. The pilot is not end-to-end encrypted. Access roles restrict application
      access; they do not prevent the infrastructure operator from administering the database.
    </p>
    <p>
      We may need to disclose information to comply with a binding legal requirement or address a
      substantiated security issue. Any adopted policy must specify the responsible operator and its
      process for assessing such requests.
    </p>
  </section>
  <section id="providers">
    <h2>Service providers and integrations</h2>
    <h3>Sign-in and hosting</h3>
    <ul>
      <li>
        <strong>Google, when enabled:</strong> sign-in requests identity, email and profile
        information. It does not request Google Drive, Docs or Sheets access. Google processes
        sign-in activity under its own
        <a href="https://policies.google.com/privacy">privacy policy</a>.
      </li>
      <li>
        <strong>Resend, when configured:</strong> receives the recipient email address and sign-in
        message needed to deliver a magic link. Its handling is described in the
        <a href="https://resend.com/legal/privacy-policy">Resend privacy policy</a>.
      </li>
      <li>
        <strong>Hosting providers:</strong> the workspace runs on the operator’s server. A homepage deployed
        on Vercel also involves Vercel in serving page requests. The final provider list, hosting locations
        and contractual arrangements remain to be confirmed by the operator.
      </li>
    </ul>
    <h3>Research platforms</h3>
    <p>
      When you choose an ODK Central or KoboToolbox connection, the server contacts the platform
      address you provide using your supplied token. The token is used for the requested connection
      and is not intentionally saved in the application database or browser local storage. Imported
      files or responses, their source information and the resulting workspace content are retained
      as research material.
    </p>
    <p>
      Imports are snapshots. The pilot does not schedule background synchronization or write changes
      back to the source platform. REDCap, SurveyCTO and SurveyMonkey also support the documented
      read-only API import paths using credentials supplied for the connection. CSPro compatibility
      is limited to exported CSV or Excel files.
    </p>
    <h3>Optional Google Forms connection</h3>
    <p>
      Google Forms requires a separate authorization after sign-in. With your permission, Dynodoc
      reads the form definition or responses for the form ID you choose. The connection requests
      forms.body.readonly and forms.responses.readonly, does not modify the Google form, and does
      not request access to all Drive files. A short-lived access token is encrypted in an HttpOnly
      cookie for up to one hour; no refresh token is requested. Disconnecting or signing out clears
      that connection. You can also revoke the grant in your Google Account permissions.
    </p>
    <p>
      Imported questions, answers, respondent identifiers supplied by Google and source metadata
      become workspace research content when you confirm import. They follow the document’s access,
      history and retention rules. Disconnecting does not erase imported copies. This data is used
      to provide the import and editing features you request; it is not sold, used for advertising
      or sent to a model-training service by this application.
    </p>
    <h3>External content</h3>
    <p>
      Homepage integration logos are served locally. If you insert an externally hosted image into a
      research document, a viewer’s browser may contact that image host and disclose ordinary
      request information to it. External links take you to services with their own policies.
    </p>
  </section>
  <section id="cookies">
    <h2>Cookies and sign-in</h2>
    <p>
      The application uses first-party cookies for authentication, the optional Forms connection and
      your performance-measurement preference. There are no advertising cookies or persistent
      visitor-tracking identifiers. The welcome-guide preference uses browser local storage. See the <a
        href="https://dynodoc.online/cookies">cookie policy and browser preference control</a
      >
      for the complete list and lifetimes.
    </p>
    <!-- Scrollable data tables need keyboard access on narrow screens. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" role="region" aria-label="Authentication cookies" tabindex="0">
      <table>
        <thead
          ><tr><th>Cookie or mechanism</th><th>Purpose</th><th>Configured lifetime</th></tr></thead
        ><tbody
          ><tr
            ><td>dynodoc_session</td><td
              >Encrypted session data used to keep you signed in. The cookie is HttpOnly, uses
              SameSite=Lax and is marked Secure on HTTPS.</td
            ><td>Up to 30 days, subject to session expiration, renewal or sign-out.</td></tr
          ><tr
            ><td>oauth_state and oauth_verifier</td><td
              >Protect the Google sign-in request and callback.</td
            ><td>Up to 10 minutes.</td></tr
          ><tr
            ><td>Email sign-in link</td><td
              >A short-lived verification link delivered to your inbox. It is not a tracking cookie.</td
            ><td
              >The link is valid for 15 minutes. Expiration does not imply immediate deletion of
              associated server records.</td
            ></tr
          ></tbody
        >
      </table>
    </div>
    <p>
      You can clear cookies through your browser, but authenticated features will require you to
      sign in again. Do not forward a sign-in link or share an active session. Your email provider
      and sign-in provider process their own service information independently.
    </p>
  </section>
  <section id="history">
    <h2>History, retention and deletion</h2>
    <div class="callout">
      <p>
        <strong>Removing text from the current version is not permanent erasure.</strong> Dynodoc preserves
        content history. Earlier wording, deleted cells or questions, attribution and original uploads
        can remain in historical records.
      </p>
    </div>
    <p>
      Archiving changes a document’s visibility. Trash blocks ordinary access and revokes public
      links while preserving recoverable content and history. Administrators can disable accounts;
      restoring an account requires fresh sign-in and does not restore old links or sessions. These
      actions are recorded in an append-only operations log and do not permanently erase data. The
      current pilot has no automatic expiry for research documents, original uploads or account
      records, and no self-service permanent-erasure feature.
    </p>
    <p>
      The current hosted backup schedule keeps a rolling set of daily database backups for 14 days.
      That backup rotation does not delete records from the live database, and restored backups may
      contain earlier copies. The operator must establish and publish retention rules for research
      projects, accounts, access logs, support records and any additional backups before wider use.
    </p>
    <p>
      A deletion or correction request must be assessed by the operator and, where appropriate, the
      responsible research organization. The assessment must account for the request, applicable
      rights, research obligations, restricted access, legal retention duties and the append-only
      history. The operator must not treat version history as a blanket exemption from deletion
      rights. This draft does not promise an automated or immediate erasure process that the pilot
      does not provide.
    </p>
  </section>
  <section id="security">
    <h2>Security and storage</h2>
    <p>
      The hosted pilot uses HTTPS for public traffic, authenticated server requests, document
      permissions, protected session cookies and a database isolated from public access. Content
      history includes integrity checks. Integrity checks help detect changes to recorded history;
      they do not prove that research content is correct or prevent every form of unauthorized
      access.
    </p>
    <p>
      No internet service can guarantee absolute security. This draft makes no claim of end-to-end
      encryption, a particular encryption-at-rest configuration, an external security certification
      or a regulated-data hosting standard. Local server backups are not a substitute for a
      confirmed offsite recovery arrangement.
    </p>
    <p>
      The operator must identify the hosting provider, processing countries, relevant subprocessors
      and any required international-transfer safeguards before the final policy is published. If a
      personal-data incident occurs, the operator and responsible organization must assess and meet
      applicable notification duties.
    </p>
  </section>
  <section id="research">
    <h2>Research and sensitive data</h2>
    <p>
      The workspace is intended for professional research collaboration. Study materials can contain
      information about participants who do not have Dynodoc accounts, including information
      supplied by research collaborators or imported from connected platforms.
    </p>
    <p>
      Research teams are responsible for their participant notices, consent or other lawful
      authority, ethics approvals, access decisions and retention requirements. Use the minimum
      information necessary. Avoid uploading identifiable, sensitive or regulated participant data
      until the institution and operator have agreed suitable safeguards and processing
      arrangements.
    </p>
    <p>
      The service is not directed at children. A study involving children requires its own lawful
      and ethical arrangements; the presence of research tools in this pilot does not authorize
      collecting children’s information.
    </p>
  </section>
  <section id="rights">
    <h2>Your choices and rights</h2>
    <p>
      You can review the content you are authorized to access, export supported files, update
      research content through the editor, manage access where your role permits, sign out and
      choose not to use optional connections.
    </p>
    <p>
      Depending on the law that applies, you may have rights to obtain information or a copy,
      correct inaccurate personal information, request erasure, restrict processing, receive
      portable data, withdraw consent or complain to an appropriate supervisory authority. These
      rights may have conditions or exceptions.
    </p>
    <p>
      <strong
        >You may also have a right to object to processing based on legitimate interests.</strong
      > If applicable, the operator must consider the objection and explain its response. Where consent
      is the basis, withdrawing it does not affect the lawfulness of processing before withdrawal.
    </p>
    <p>
      To exercise a right, contact the operator using the details to be completed below. A research
      participant may also contact the organization responsible for the study. For the current
      limited pilot, use the administrator who invited you as the interim contact. Do not send
      sensitive study records through public repository issues. Identity checks should be
      proportionate, and requests must be handled within applicable legal deadlines.
    </p>
  </section>
  <section id="changes">
    <h2>Changes to this notice</h2>
    <p>
      The final notice must carry an effective date and identify its operator. Material changes to
      purposes, providers or data handling should be communicated before they take effect when
      required. A revised notice does not itself authorize incompatible uses of previously collected
      information.
    </p>
    <p>
      For the proposed rules governing use of the service, see the <a href="/terms"
        >terms and conditions</a
      >.
    </p>
  </section>
  <section id="contact">
    <h2>Contact and operator details</h2>
    <dl class="contact-details">
      <dt>Legal operator</dt>
      <dd>Awaiting confirmation.</dd>
      <dt>Privacy contact email</dt>
      <dd>Awaiting confirmation.</dd>
      <dt>Mailing address</dt>
      <dd>Awaiting confirmation.</dd>
      <dt>Hosting jurisdictions</dt>
      <dd>Awaiting confirmation.</dd>
      <dt>Representative or DPO</dt>
      <dd>To be identified if required by applicable law.</dd>
    </dl>
    <p>
      These missing details are intentional markers in a draft. They are not a substitute for an
      identifiable operator and a functioning privacy-request channel.
    </p>
  </section>
</LegalPage>
