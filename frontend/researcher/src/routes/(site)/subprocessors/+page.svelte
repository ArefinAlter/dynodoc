<script lang="ts">
  import LegalPage from '$lib/components/marketing/LegalPage.svelte';
  import { EMAIL_HREF, OPERATOR, UNCONFIRMED } from '$lib/site/operator';
  const sections = [
    { id: 'about', title: 'About this list' },
    { id: 'list', title: 'Providers that handle data' },
    { id: 'platforms', title: 'Editor platforms' },
    { id: 'infrastructure', title: 'Infrastructure that sees no content' },
    { id: 'unused', title: 'What is not used' },
    { id: 'changes', title: 'Changes and contact' }
  ];
</script>

<LegalPage
  title="Subprocessors"
  description="The outside providers Dynodoc uses to host, deliver and sign in to the service, what each receives, and what is not used."
  {sections}
>
  <section id="about">
    <h2>About this list</h2>
    <p>
      This draft names the providers that the Dynodoc code and deployment configuration actually
      use. It was checked against the repository on 12 October 2026 and is not an executed
      agreement. A provider is listed only if it is configured; hosting details that the operator
      has not yet confirmed are marked as such rather than guessed.
    </p>
    <p>
      “Subprocessor” here means a provider that handles information on the operator’s behalf to run
      the service. Providers you choose yourself, such as your sign-in provider, are listed
      separately. For how information is used and kept, see the
      <a href="/privacy">privacy policy</a>; for the draft processing structure, see the
      <a href="/dpa">data processing terms</a>.
    </p>
  </section>
  <section id="list">
    <h2>Providers that handle data</h2>
    <!-- Scrollable data tables need keyboard access on narrow screens. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" role="region" aria-label="Providers that handle data" tabindex="0">
      <table>
        <thead
          ><tr
            ><th>Provider</th><th>Role and purpose</th><th>Information involved</th><th>Location</th
            ></tr
          ></thead
        ><tbody
          ><tr
            ><td>Hosting provider of the workspace server (name awaiting confirmation)</td><td
              >Runs the workspace, API, database, document converter and backups on a rented virtual
              server.</td
            ><td
              >All service data: account records, document content and history, original uploads,
              audit records, logs and backups.</td
            ><td>{UNCONFIRMED}</td></tr
          ><tr
            ><td>Vercel Inc.</td><td>Serves the public website at dynodoc.online.</td><td
              >IP address, requested address and browser details for public-page requests. The
              website deployment is configured with only the public workspace address and has no
              database.</td
            ><td>Per Vercel.</td></tr
          ><tr
            ><td><a href="https://resend.com/legal/privacy-policy">Resend</a></td><td
              >Delivers transactional email: sign-in links and invitations. Used when configured.</td
            ><td
              >Recipient address, the sender’s name and reply-to address for invitations, and the
              message, which contains a link and a file or project name.</td
            ><td>Per Resend.</td></tr
          ><tr
            ><td><a href="https://policies.google.com/privacy">Google</a> (your choice)</td><td
              >Optional sign-in with Google, limited to identity, email and profile; and the
              optional read-only Google Forms import. Not a Drive, Docs or Sheets integration.</td
            ><td
              >Your Google account identifier, name and email address are passed to Dynodoc; for a
              Forms import, requests for the form you select.</td
            ><td>Per Google.</td></tr
          ></tbody
        >
      </table>
    </div>
  </section>
  <section id="platforms">
    <h2>Editor platforms</h2>
    <p>
      The add-ins run inside products from Microsoft and Google. These are the host platforms you
      choose to use, not Dynodoc subprocessors, and they apply their own terms and privacy
      practices.
    </p>
    <ul>
      <li>
        <strong>Microsoft:</strong> Word, Excel and PowerPoint panes load Microsoft’s Office.js library
        from appsforoffice.microsoft.com and run in Office’s embedded web view. The page security policy
        permits only Microsoft’s Office.js hosts in addition to Dynodoc’s own origin.
      </li>
      <li>
        <strong>Google:</strong> Docs, Sheets and Slides add-ons run on Google Apps Script. Their stored
        queue and key sit in Apps Script user properties, requests leave only to the Dynodoc connector
        address, and Google may log execution errors because exception logging is enabled.
      </li>
    </ul>
    <p>
      What each add-in reads, stores and sends is described under
      <a href="/privacy#addins">editor add-ins</a> in the privacy policy.
    </p>
  </section>
  <section id="infrastructure">
    <h2>Infrastructure that sees no content</h2>
    <ul>
      <li>
        <strong>Let’s Encrypt:</strong> issues the TLS certificates for the workspace and administration
        hostnames. It receives domain names, not user information.
      </li>
      <li>
        <strong>Domain registrar and DNS:</strong> provider awaiting confirmation. It holds the domain
        records, not user information.
      </li>
      <li>
        <strong>GitHub:</strong> hosts the source code and runs the automated tests, which use synthetic
        data only. It receives no customer content.
      </li>
    </ul>
  </section>
  <section id="unused">
    <h2>What is not used</h2>
    <ul>
      <li>Analytics, advertising networks, tag managers or social widgets.</li>
      <li>
        Third-party fonts, scripts, stylesheets or content delivery networks on the website; fonts,
        icons and artwork are served from Dynodoc itself.
      </li>
      <li>AI or language-model services; document content is not sent to one.</li>
      <li>
        A third-party document conversion service; the converter runs on the same private network as
        the application.
      </li>
      <li>Third-party error reporting or session replay.</li>
      <li>
        Separate object storage or a cache service for documents; documents, history and uploads are
        stored in the PostgreSQL database.
      </li>
    </ul>
  </section>
  <section id="changes">
    <h2>Changes and contact</h2>
    <p>
      Adding or replacing a provider that handles content changes this list and, before the service
      is adopted for customer data, must be announced in a way the final terms define. Until then
      this page is the single record. A document can also link to an image hosted elsewhere; that
      host is chosen by the author, not by Dynodoc.
    </p>
    <dl class="contact-details">
      <dt>Legal operator</dt>
      <dd>{OPERATOR.legalName}, trading as {OPERATOR.tradingAs}</dd>
      <dt>Contact for questions</dt>
      <dd>
        <a href={EMAIL_HREF}>{OPERATOR.email}</a>. See also the
        <a href="/privacy#contact">privacy policy</a>.
      </dd>
      <dt>Hosting provider</dt>
      <dd>{UNCONFIRMED}</dd>
    </dl>
  </section>
</LegalPage>

<style>
  .table-scroll table {
    min-width: 760px;
  }
</style>
