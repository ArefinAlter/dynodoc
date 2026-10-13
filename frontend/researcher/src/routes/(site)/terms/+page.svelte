<script lang="ts">
  import LegalPage from '$lib/components/marketing/LegalPage.svelte';
  import OperatorDetails from '$lib/components/marketing/OperatorDetails.svelte';
  import { OPEN_SOURCE_STATEMENT, OPERATOR } from '$lib/site/operator';
  const sections = [
    { id: 'agreement', title: 'About these terms' },
    { id: 'accounts', title: 'Accounts and access' },
    { id: 'service', title: 'The Dynodoc service' },
    { id: 'content', title: 'Your content and ownership' },
    { id: 'sensitive', title: 'Sensitive and regulated information' },
    { id: 'collaboration', title: 'Collaboration, history and review' },
    { id: 'addins', title: 'Editor add-ins' },
    { id: 'imports', title: 'Imports and third-party services' },
    { id: 'acceptable-use', title: 'Acceptable use' },
    { id: 'availability', title: 'Availability and changes' },
    { id: 'ending', title: 'Suspension and ending use' },
    { id: 'responsibility', title: 'Warranties and responsibility' },
    { id: 'disputes', title: 'Law and disputes' },
    { id: 'general', title: 'General provisions' },
    { id: 'contact', title: 'Operator and contact details' }
  ];
</script>

<LegalPage
  title="Terms & conditions"
  description="Proposed terms for using Dynodoc, including accounts, projects and history, editor add-ins, content ownership, responsible use and ending use."
  {sections}
>
  <section id="agreement">
    <h2>About these terms</h2>
    <p>
      These draft terms describe the proposed relationship between the operator of the hosted
      Dynodoc service, {OPERATOR.legalName} (trading as {OPERATOR.tradingAs}), and the people
      authorized to use it. The governing law and forum are set out under
      <a href="#disputes">Law and disputes</a>. This page is not an executed agreement or an
      assertion that the operator has entered into a contract with you.
    </p>
    <p>
      “Dynodoc,” “we” and “our” below refer to that operator. “You” means the individual using the
      service and, where properly authorized, the organization on whose behalf they act.
    </p>
    <p>
      These terms concern the hosted service, its website and its editor add-ins. The open-source
      licence of the engine, API and command-line tools (see <a href="/licenses">licenses</a>) and
      any separately agreed institutional, <a href="/dpa">data-processing</a> or service agreement are
      distinct. A signed agreement may take precedence where it expressly says so.
    </p>
  </section>
  <section id="accounts">
    <h2>Accounts and access</h2>
    <p>
      Use an email address and identity that you are authorized to use, and provide accurate
      information needed for account operation. If you act for an organization, you must have the
      authority to use the service and upload its materials.
    </p>
    <p>
      Protect access to your email account, sign-in links, sessions, editor connection keys and any
      signing keys. Do not share an account as a substitute for inviting collaborators individually.
      Tell the operator promptly if you suspect unauthorized access, using the email address under
      <a href="#contact">Operator and contact details</a>.
    </p>
    <p>
      The service is intended for adults and authorized collaborators who can enter a binding
      agreement under applicable law. It is not directed at children.
    </p>
    <p>
      Access can depend on an invitation, a project or file role and the availability of the
      configured sign-in provider. Being able to sign in does not grant access to every project or
      file.
    </p>
  </section>
  <section id="service">
    <h2>The Dynodoc service</h2>
    <p>
      Dynodoc is a document provenance, review and synchronization service. It lets a team capture
      changes made in supported editors, gather them in projects, review and merge them and keep a
      verifiable history. It includes projects with files, personal drafts, change requests with
      review rules, named versions, history recovery, audit records, add-ins for Word, Excel,
      PowerPoint, Google Docs, Sheets and Slides, basic web editors and read-only shared links.
      Feature availability depends on the current release and deployment configuration; some
      capabilities described in the documentation are not yet live (see
      <a href="/security#status">what is live</a>).
    </p>
    <p>
      The service is under active development. It does not promise full feature or file-format
      parity with Microsoft Office or Google Workspace, and it does not currently provide
      character-by-character real-time co-editing. Dynodoc records the changes it observes or
      receives; it does not prove who typed a change and is not a legally certified audit trail.
    </p>
    <p>
      The website contains illustrations that use sample information. They do not create an account
      or save a real document.
    </p>
    <p>
      There is no payment collection flow in the current service. If paid services are introduced,
      pricing and any billing terms must be disclosed and agreed before a charge is authorized.
      These draft terms do not authorize automatic charges.
    </p>
  </section>
  <section id="content">
    <h2>Your content and ownership</h2>
    <p>
      You and the relevant rights holders retain ownership of the files, text, data and other
      content you submit. Using Dynodoc does not transfer that ownership to the operator.
    </p>
    <p>
      The proposed service permission is limited to hosting, storing, reproducing, transmitting,
      converting, displaying, backing up and otherwise processing your content as necessary to
      provide the features you request, apply your sharing settings, maintain the service and meet
      applicable legal obligations. It is not a permission to sell your content or use it to train
      AI models.
    </p>
    <p>
      You must have the rights and authority needed to upload, process and share the content,
      including permission for copyrighted material and an appropriate basis for personal
      information. Do not grant another person access that exceeds your own authority.
    </p>
    <p>
      {OPEN_SOURCE_STATEMENT} The <a href="/licenses">licenses page</a> has the details. A software license
      does not make your private content public or apply an open-source license to it. Third-party software
      and trademarks remain subject to their own licenses and rights.
    </p>
  </section>
  <section id="sensitive">
    <h2>Sensitive and regulated information</h2>
    <p>
      Documents can contain personal, confidential or regulated information. Use the minimum
      necessary, and do not upload special-category, health, financial, children’s or other
      regulated information unless you have the authority to do so and the institution and operator
      have agreed suitable safeguards and processing arrangements.
    </p>
    <p>
      No certification, regulated-data hosting arrangement or institutional approval is implied by
      these terms or by the <a href="/security">security overview</a>. Dynodoc is a collaboration
      and record-keeping tool; it must not be the only system for a decision whose failure could
      cause serious harm.
    </p>
  </section>
  <section id="collaboration">
    <h2>Collaboration, history and review</h2>
    <p>
      Project and file roles determine what collaborators can do. People authorized to manage access
      are responsible for assigning appropriate roles and reviewing access when a team changes.
      Project rules can set a minimum level of protection and required approvals for every file in
      the project; a file rule cannot weaken them. Collaborators remain responsible for copies they
      download, export or share outside the service.
    </p>
    <p>
      Personal drafts let you work separately from the team version. Sharing selected changes
      proposes them through the available review and conflict-resolution flow. You are responsible
      for reviewing the proposed result, particularly when two people have changed the same
      material.
    </p>
    <div class="callout">
      <p>
        <strong>History preserves earlier content.</strong> Deleting a paragraph, changing a cell or archiving
        a file does not remove it from previous versions, original uploads or backups. Restoring or reverting
        adds a new change rather than rewriting the past. Only an administrator’s erasure of a whole document
        removes its history; there is no redaction of a single past change.
      </p>
    </div>
    <p>
      Authorship, timestamps and content changes may be shown in history. These records support
      review. They are not a guarantee of correctness, a complete record of every administrative
      action or a legally certified audit trail, and they cannot by themselves detect a rewrite by
      someone who controls the whole database.
    </p>
    <p>
      Retention and personal-information requests are handled under the
      <a href="/privacy#retention">privacy policy</a> and applicable law. These terms do not remove statutory
      rights to correction or erasure where they apply.
    </p>
  </section>
  <section id="addins">
    <h2>Editor add-ins</h2>
    <p>
      The add-ins for Word, Excel, PowerPoint, Google Docs, Sheets and Slides capture changes in the
      file you have open and send them to Dynodoc when you choose. They need a Dynodoc account and
      your approval of a connection to one file; the connection key expires after seven days and you
      can revoke it under Sync &amp; connections. You are responsible for keeping the device, the
      editor session and any local queue secure, and for confirming that you may send the file’s
      content to Dynodoc.
    </p>
    <p>
      Text in your file changes only when you choose to apply a preview. Review it first: a
      conversion can omit unsupported structures, and the add-ins record observations rather than
      proof of authorship.
    </p>
    <p>
      The add-ins are not yet published in the Microsoft AppSource or the Google Workspace
      Marketplace and installed-editor acceptance testing is not complete, so treat them as
      pre-release. Where a marketplace distributes them, its terms also apply. Dynodoc is not
      affiliated with, sponsored by or endorsed by Microsoft or Google.
    </p>
  </section>
  <section id="imports">
    <h2>Imports and third-party services</h2>
    <p>
      Imports and exports may change formatting, omit unsupported features or convert formulas and
      structures differently from the source application. Review warnings and verify the result. The
      original upload is retained for reference, but retaining an original does not guarantee
      perfect conversion. Uploads are limited in size; the current limit is 3 MB per file.
    </p>
    <p>
      Optional source imports retrieve material at your request using credentials you supply. You
      must be authorized to use the server, project and data. Imports create snapshots; there is no
      promise of automatic synchronization, complete migration or writeback to a source platform.
    </p>
    <p>
      Third-party services, sign-in providers and external content have their own terms and
      availability. Product names and logos identify compatibility and belong to their respective
      owners. They do not imply a partnership, sponsorship or endorsement of Dynodoc.
    </p>
  </section>
  <section id="acceptable-use">
    <h2>Acceptable use</h2>
    <p>
      Use the service lawfully and within your authorization. In short, do not upload or share
      material you have no right to, access other people’s private content, credentials or sessions
      without permission, introduce malicious files, disrupt the service or circumvent its limits,
      impersonate others or falsify authorship, or use the service to send unsolicited messages.
    </p>
    <p>
      The complete rules, including how to test security in good faith, are on the
      <a href="/acceptable-use">acceptable use page</a>, which forms part of these draft terms.
    </p>
  </section>
  <section id="availability">
    <h2>Availability and changes</h2>
    <p>
      The service may be unavailable during maintenance, changes, provider outages or unexpected
      failures. No service-level agreement, uninterrupted availability, guaranteed recovery time or
      fixed support response time is offered by this draft.
    </p>
    <p>
      Keep independent copies of important source material and validated exports. A hosted backup
      process, which today keeps 14 days of backups on the same server, does not replace your own
      retention and recovery arrangements.
    </p>
    <p>
      The operator may improve, change or retire features. Material changes affecting access to
      existing work should be communicated with reasonable notice where practicable, and a
      reasonable opportunity to export should be provided when the service is discontinued, unless a
      legal or urgent security constraint prevents it. Changes should take effect prospectively and
      cannot remove non-waivable rights.
    </p>
  </section>
  <section id="ending">
    <h2>Suspension and ending use</h2>
    <p>
      You may stop using the service and ask the operator for help with account closure. Signing
      out, archiving a file and closing an account are different actions, and none is a promise of
      immediate permanent deletion of all history; see what can and cannot be erased in the
      <a href="/privacy#retention">privacy policy</a>.
    </p>
    <p>
      The proposed grounds for restricting access are a substantiated breach of these terms, a
      serious security or operational risk, a legal requirement or the end of the service. Any
      restriction should be proportionate. Where lawful and practical, the operator should explain
      the reason and provide a way to seek review.
    </p>
    <p>
      When your access ends, other authorized collaborators may still have access to shared projects
      under the relevant arrangement. Export, retention, deletion and any surviving permissions must
      be handled consistently with applicable law, the organization’s authority and the privacy
      policy.
    </p>
  </section>
  <section id="responsibility">
    <h2>Warranties and responsibility</h2>
    <p>
      To the extent permitted by applicable law, the service is provided on an “as available” basis
      without a promise that every feature will be error-free, suitable for a particular purpose or
      continuously accessible. You must verify outputs, conversions, calculations and merged changes
      before relying on them.
    </p>
    <p>
      Nothing in these draft terms excludes liability that cannot lawfully be excluded, waives
      mandatory consumer or data-protection rights, excuses fraud or deliberate misconduct, or
      transfers the operator’s own legal duties to users.
    </p>
    <p>
      No financial liability cap, blanket indemnity or jurisdiction-specific exclusion is included
      here.
    </p>
  </section>
  <section id="disputes">
    <h2>Law and disputes</h2>
    <p>
      The operator’s registered office is in Dhaka, Bangladesh. These terms are governed by
      {OPERATOR.governingLaw}, and disputes are to be brought before {OPERATOR.forum}. This does not
      limit mandatory consumer or data-protection rights that apply where you live. No mandatory
      arbitration or class-action waiver is proposed.
    </p>
    <p>
      The proposed first step for a service dispute is to contact the operator with enough detail to
      investigate it. This does not prevent a person from contacting an appropriate regulator,
      exercising statutory rights or seeking remedies available under applicable law.
    </p>
  </section>
  <section id="general">
    <h2>General provisions</h2>
    <p>
      These terms, any applicable signed agreement and the privacy policy should describe the entire
      relevant arrangement without overriding mandatory legal protections. If a provision is
      unenforceable, the remaining provisions should continue only to the extent permitted by
      applicable law. Failure to enforce a provision on one occasion does not automatically waive it
      for another.
    </p>
    <p>
      The operator must preserve a dated copy of adopted terms and communicate material revisions
      appropriately. The date at the top of this page is a drafting date, not a record of your
      acceptance.
    </p>
  </section>
  <section id="contact">
    <h2>Operator and contact details</h2>
    <OperatorDetails emailLabel="Public legal email">
      <dt>Governing law</dt>
      <dd>Laws of Bangladesh</dd>
      <dt>Forum</dt>
      <dd>Courts of Dhaka</dd>
    </OperatorDetails>
    <p>
      Write to the email address above with questions about these terms. To report a security
      vulnerability, see <a href="/security#report">the security overview</a>.
    </p>
  </section>
</LegalPage>
