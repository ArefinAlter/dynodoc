<script lang="ts">
  import LegalPage from '$lib/components/marketing/LegalPage.svelte';
  import { EMAIL_HREF, OPERATOR } from '$lib/site/operator';
  const sections = [
    { id: 'status', title: 'Status of this document' },
    { id: 'roles', title: 'Parties and roles' },
    { id: 'subject', title: 'Subject matter, duration and purpose' },
    { id: 'categories', title: 'Information and people' },
    { id: 'instructions', title: 'Processing instructions' },
    { id: 'security', title: 'Security measures' },
    { id: 'subprocessors', title: 'Subprocessors' },
    { id: 'assistance', title: 'Assistance, incidents and requests' },
    { id: 'return', title: 'Deletion and return' },
    { id: 'audit', title: 'Audit and records' },
    { id: 'transfers', title: 'Location and transfers' },
    { id: 'open', title: 'Open items' }
  ];
</script>

<LegalPage
  title="Data processing terms (draft)"
  description="The structure that data processing terms for Dynodoc would follow: roles, subject matter, categories, security, subprocessors, deletion and audit. A draft, not an executed agreement."
  {sections}
>
  <section id="status">
    <h2>Status of this document</h2>
    <p>
      This page sets out the structure that data processing terms for Dynodoc would follow. It is a
      draft, not an executed agreement; it is not offered for signature, and nobody becomes bound by
      it by using the service. It describes how the service behaves today, as already stated in the
      <a href="/privacy">privacy policy</a>, the <a href="/terms">terms</a> and the
      <a href="/security">security overview</a>, and it leaves anything that needs a legal decision
      as an open item. It adds no commitment of its own.
    </p>
  </section>
  <section id="roles">
    <h2>Parties and roles</h2>
    <ul>
      <li>
        <strong>Customer:</strong> the organization or person that decides what is uploaded to its projects,
        who is a member and how long content is kept. For that content the customer is generally the controller,
        or a processor acting for its own clients.
      </li>
      <li>
        <strong>Operator:</strong>
        {OPERATOR.legalName} (trading as {OPERATOR.tradingAs}), the legal operator of the hosted
        service. For project content it would act as a processor on the customer’s instructions. For
        account and service-operation records, security logs and anonymous performance measurements
        it decides how and why information is processed.
      </li>
    </ul>
    <p>
      The roles actually applicable depend on the law that governs the arrangement. A person who
      uses the service in their own capacity is covered by the privacy policy directly. The
      operator’s registered address and registration details are under
      <a href="/privacy#contact">contact and operator details</a> in the privacy policy; questions
      and notices about these draft terms go to <a href={EMAIL_HREF}>{OPERATOR.email}</a>.
    </p>
  </section>
  <section id="subject">
    <h2>Subject matter, duration and purpose</h2>
    <p>
      <strong>Subject matter:</strong> hosting, storing, converting, displaying, backing up and
      otherwise processing documents and project history so that the customer can capture, review,
      merge, recover and verify changes. <strong>Purpose:</strong> providing the features the
      customer requests, applying its sharing settings, maintaining and securing the service and
      meeting legal obligations. Content is not sold, used for advertising or used to train AI
      models. <strong>Duration:</strong> while the account or project exists, followed by the
      retention and erasure described in the
      <a href="/privacy#retention">privacy policy</a>.
    </p>
  </section>
  <section id="categories">
    <h2>Information and people</h2>
    <!-- Scrollable data tables need keyboard access on narrow screens. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" role="region" aria-label="Categories of information" tabindex="0">
      <table>
        <thead><tr><th>Category</th><th>Examples</th></tr></thead><tbody
          ><tr
            ><td>Document content</td><td
              >Text, cells, slides, formatting, comments, suggestions and original uploads.</td
            ></tr
          ><tr
            ><td>Project structure</td><td>Project, folder and file names and member roles.</td></tr
          ><tr
            ><td>Account identifiers</td><td>Email address, display name and account identifier.</td
            ></tr
          ><tr
            ><td>History and provenance</td><td
              >Authors, timestamps, hashes, signed commits, branch receipts and drafts.</td
            ></tr
          ><tr
            ><td>Audit and connection records</td><td
              >Administrative actions, erasure receipts and editor connection grants.</td
            ></tr
          ></tbody
        >
      </table>
    </div>
    <p>
      The people concerned are the customer’s members and invited collaborators and anyone named or
      described in a document. Dynodoc does not decide what documents contain. Special categories of
      information are not intended content; the customer is responsible for what it uploads, as the
      terms say under
      <a href="/terms#sensitive">sensitive and regulated information</a>.
    </p>
  </section>
  <section id="instructions">
    <h2>Processing instructions</h2>
    <p>
      The service processes project content only to provide the features the customer uses, as
      configured through roles, project rules, shared links and erasure requests. The final
      agreement must say how further written instructions are given, and what happens when an
      instruction would be unlawful.
    </p>
  </section>
  <section id="security">
    <h2>Security measures</h2>
    <p>
      The measures that exist are described, with their limits, in the
      <a href="/security">security overview</a>. In summary: a hash-chained append-only history,
      checked snapshots, a restricted database login, revocable sessions, scoped and expiring editor
      connection keys, HTTPS with a strict content policy, request limits, hardened containers,
      validated backups and audited whole-document erasure. The same page lists what is not claimed,
      including independent audit, certification and off-site backups.
    </p>
  </section>
  <section id="subprocessors">
    <h2>Subprocessors</h2>
    <p>
      The current providers are listed on the <a href="/subprocessors">subprocessors page</a>. The
      final agreement must say how a change to that list is announced and whether the customer may
      object.
    </p>
  </section>
  <section id="assistance">
    <h2>Assistance, incidents and requests</h2>
    <p>
      A customer can export supported files and see its own members’ access. Administrators can
      erase a whole document or account as described in the privacy policy; there is no self-service
      erasure today. If a personal-data incident occurs, the operator and the customer must assess
      it and meet applicable notification duties. The final agreement must set the timelines and the
      form of assistance with data-subject requests and impact assessments; this draft invents none.
    </p>
  </section>
  <section id="return">
    <h2>Deletion and return</h2>
    <ul>
      <li>
        <strong>Return:</strong> while access lasts, a customer can download supported files and retained
        revisions under their permissions. If the service is discontinued, the terms propose a reasonable
        opportunity to export.
      </li>
      <li>
        <strong>Deletion:</strong> an administrator can erase a whole document or an account. A minimal
        receipt, a deleted-account identity and public signing-key bindings stay so that remaining history
        verifies. Individual past changes cannot be redacted from a retained history.
      </li>
      <li>
        <strong>Backups:</strong> database backups are kept for a rolling 14 days on the same server and
        expire on that schedule.
      </li>
    </ul>
  </section>
  <section id="audit">
    <h2>Audit and records</h2>
    <p>
      The service keeps an operations log and erasure receipts, and a customer with access to a
      document can ask the service to verify the integrity of its history. The engine, API and
      command-line tools that implement the history are open source under the MIT licence so that
      their behaviour can be inspected; the workspace and website are not published as open source.
      See <a href="/licenses">licenses</a>. No independent audit or certification exists. The final
      agreement must state any audit right, how it is exercised and who bears its cost.
    </p>
  </section>
  <section id="transfers">
    <h2>Location and transfers</h2>
    <p>
      The hosting provider and the locations where information is stored are awaiting confirmation.
      Safeguards for any transfer outside the customer’s jurisdiction must be identified before
      these terms are completed.
    </p>
  </section>
  <section id="open">
    <h2>Open items</h2>
    <ul>
      <li>
        Counsel review of the proposed governing law and forum (the laws of Bangladesh and the
        courts of Dhaka).
      </li>
      <li>Hosting provider, storage locations and transfer safeguards.</li>
      <li>How instructions are given and how an unlawful instruction is handled.</li>
      <li>How subprocessor changes are announced and objected to.</li>
      <li>Incident-notification and assistance timelines.</li>
      <li>Audit rights and cost.</li>
      <li>Retention for administrative logs and web server logs.</li>
      <li>Liability allocation, reviewed with the final terms.</li>
    </ul>
  </section>
</LegalPage>

<style>
  .table-scroll table {
    min-width: 520px;
  }
</style>
