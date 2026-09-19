<script lang="ts">
  import LegalPage from '$lib/components/marketing/LegalPage.svelte';
  const sections = [
    { id: 'agreement', title: 'About these terms' },
    { id: 'accounts', title: 'Accounts and access' },
    { id: 'service', title: 'The research pilot' },
    { id: 'content', title: 'Your content and ownership' },
    { id: 'collaboration', title: 'Collaboration and history' },
    { id: 'research', title: 'Research responsibilities' },
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
  description="Proposed terms for using the Dynodoc research pilot, including accounts, collaboration, content ownership and responsible use."
  {sections}
>
  <section id="agreement">
    <h2>About these terms</h2>
    <p>
      These draft terms describe the proposed relationship between the operator of the hosted
      Dynodoc service and people authorized to use it. The legal operator and governing jurisdiction
      have not yet been confirmed. This page is not an executed agreement or an assertion that an
      unidentified operator has entered into a contract with you.
    </p>
    <p>
      Before these terms become effective, the operator must complete its identity and contact
      details, confirm the applicable law, review the provisions and communicate the final terms
      through an appropriate acceptance process. “Dynodoc,” “we” and “our” below refer to that
      operator. “You” means the individual using the service and, where properly authorized, the
      organization on whose behalf they act.
    </p>
    <p>
      These terms concern the hosted service. The open-source software license and any separately
      agreed institutional, data-processing or service agreement are distinct. A signed agreement
      may take precedence where it expressly says so.
    </p>
  </section>
  <section id="accounts">
    <h2>Accounts and access</h2>
    <p>
      Use an email address and identity that you are authorized to use. Provide accurate information
      needed for account operation. If you act for an organization, you must have the authority to
      use the service and upload its materials.
    </p>
    <p>
      Protect access to your email account, sign-in links, authentication sessions and integration
      tokens. Do not share an account as a substitute for inviting collaborators individually. Tell
      the operator promptly if you suspect unauthorized access, using the contact channel to be
      established below.
    </p>
    <p>
      The pilot is intended for adult researchers and authorized collaborators who can enter a
      binding agreement under applicable law. Participation by someone who cannot do so requires an
      appropriate institutional or representative arrangement; the service is not directed at
      children.
    </p>
    <p>
      Access may depend on an invitation, a document role and the availability of the configured
      authentication provider. Being able to sign in does not grant access to every research
      document.
    </p>
  </section>
  <section id="service">
    <h2>The research pilot</h2>
    <p>
      Dynodoc provides a web workspace for research documents, spreadsheets and questionnaires, with
      imports, exports, personal drafts, selected-change sharing, review and content history.
      Feature availability depends on the current release and deployment configuration.
    </p>
    <p>
      This is a pilot for a small research team. It does not promise full feature or file-format
      parity with Microsoft Word, Excel, Google Docs or Google Sheets. It does not currently provide
      real-time character-by-character co-editing, all advanced spreadsheet functions or a complete
      survey execution platform.
    </p>
    <p>
      The homepage contains interactive illustrations that use sample information. They demonstrate
      a workflow and do not create an account, save a real document or submit research responses.
    </p>
    <p>
      There is no payment collection flow in the current pilot. If paid services are introduced,
      pricing and any billing terms must be disclosed and agreed before a charge is authorized.
      These draft terms do not authorize automatic charges.
    </p>
  </section>
  <section id="content">
    <h2>Your content and ownership</h2>
    <p>
      You and the relevant rights holders retain ownership of the research materials, documents,
      data and other content you submit. Using Dynodoc does not transfer that ownership to the
      operator.
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
      Dynodoc’s application code is available under the GNU Affero General Public License, version 3
      or later, as specified in the repository. That software license does not make your private
      research content public or apply an open-source license to it. Third-party software and
      trademarks remain subject to their own licenses and rights.
    </p>
  </section>
  <section id="collaboration">
    <h2>Collaboration and history</h2>
    <p>
      Document membership and roles determine what collaborators can do. People authorized to manage
      access are responsible for assigning appropriate roles and reviewing access when a project or
      team changes. Collaborators remain responsible for copies they download, export or share
      outside the service.
    </p>
    <p>
      Personal drafts let you work separately from the team version. Sharing selected changes
      updates the shared work through the available review and conflict-resolution flow. You are
      responsible for reviewing the proposed result, particularly when two people have changed the
      same material.
    </p>
    <div class="callout">
      <p>
        <strong>Version history preserves earlier content.</strong> Deleting a paragraph, changing a cell
        or archiving a document does not necessarily remove it from previous versions, original uploads
        or backups.
      </p>
    </div>
    <p>
      Authorship, timestamps and content changes may be shown in history. Restoring earlier content
      creates a new change rather than rewriting the past. These records support review; they are
      not a guarantee of research validity, a comprehensive record of every administrative action or
      a legally certified audit trail.
    </p>
    <p>
      Retention and personal-information requests must be handled under the <a href="/privacy"
        >privacy policy</a
      > and applicable law. These terms do not remove statutory rights to correction or erasure where
      they apply.
    </p>
  </section>
  <section id="research">
    <h2>Research responsibilities</h2>
    <p>
      You and your organization are responsible for the study design, research quality, ethics
      review, participant notices, consent or other lawful basis, disclosure limits, authorship
      decisions and any regulatory or professional requirements that apply to your work.
    </p>
    <p>
      Do not assume that a questionnaire, imported field, formula or preserved logic expression has
      been validated simply because it is displayed by Dynodoc. The pilot preserves some survey
      metadata without executing or validating all relevance rules, constraints or calculations.
    </p>
    <p>
      Evaluate the pilot using sample or appropriately de-identified materials before relying on it
      for a study. Identifiable, sensitive or regulated data requires an agreed assessment of
      hosting, access, retention, transfers and institutional requirements. No certification,
      regulated-data hosting arrangement or institutional approval is implied by these terms.
    </p>
    <p>
      The service is a collaboration tool. It does not provide clinical, legal, ethical or
      methodological approval, and it must not be used as the sole system for a decision whose
      failure could cause serious harm.
    </p>
  </section>
  <section id="imports">
    <h2>Imports and third-party services</h2>
    <p>
      Imports and exports may change formatting, omit unsupported features or convert formulas and
      structures differently from the source application. Review warnings and verify the result. The
      original upload is retained for reference, but retaining an original does not guarantee
      perfect conversion.
    </p>
    <p>
      ODK Central and KoboToolbox connections retrieve material at your request using credentials
      you supply. You must be authorized to use the server, project and data. Current imports create
      snapshots; there is no promise of automatic synchronization, complete migration or writeback
      to a source platform.
    </p>
    <p>
      Compatibility with other named research tools is limited to the file workflows described in
      the interface. Native applications, platform-specific scripts, advanced survey behavior and
      unsupported file features are not included merely because a product is listed.
    </p>
    <p>
      Third-party services, identity providers and external content have their own terms and
      availability. Product names and logos identify compatibility and belong to their respective
      owners. They do not imply a partnership, sponsorship or endorsement of Dynodoc.
    </p>
  </section>
  <section id="acceptable-use">
    <h2>Acceptable use</h2>
    <p>Use the service lawfully and within your authorization. In particular, do not:</p>
    <ul>
      <li>
        Upload or share material in violation of privacy, confidentiality, intellectual-property
        rights or applicable law.
      </li>
      <li>
        Attempt to access another person’s private documents, credentials, sessions or server
        resources without permission.
      </li>
      <li>
        Introduce malicious files, deliberately disrupt the service, circumvent access controls or
        use connectors to probe unrelated private systems.
      </li>
      <li>
        Impersonate another person, misrepresent your authority over research materials or falsify
        your identity to conceal unauthorized contributions.
      </li>
      <li>
        Use the service to send unsolicited messages, host unrelated abusive traffic or knowingly
        interfere with other users’ work.
      </li>
    </ul>
    <p>
      Good-faith security reports should go to the operator’s private contact channel once
      established. Do not publish credentials, participant data or exploit details in public
      repository issues.
    </p>
  </section>
  <section id="availability">
    <h2>Availability and changes</h2>
    <p>
      The pilot may be unavailable during maintenance, changes, provider outages or unexpected
      failures. No service-level agreement, uninterrupted availability, guaranteed recovery time or
      fixed support response time is offered by this draft.
    </p>
    <p>
      Keep independent copies of important source material and validated exports. A hosted backup
      process does not replace your organization’s research retention and recovery arrangements.
    </p>
    <p>
      The operator may improve, change or retire features. Material changes affecting access to
      existing work should be communicated with reasonable notice where practicable, and a
      reasonable opportunity to export should be provided when the service is discontinued, unless a
      legal or urgent security constraint prevents it.
    </p>
    <p>
      Any final terms must explain how material contractual changes are notified and accepted where
      required. Changes should take effect prospectively and cannot remove non-waivable rights.
    </p>
  </section>
  <section id="ending">
    <h2>Suspension and ending use</h2>
    <p>
      You may stop using the service and request assistance with account closure through the
      operator. Sign-out, document archiving and account closure are different actions. None should
      be understood as a promise of immediate permanent deletion of all history.
    </p>
    <p>
      The proposed grounds for restricting access are a substantiated breach of these terms, a
      serious security or operational risk, a legal requirement or the end of the pilot. Any
      restriction should be proportionate. Where lawful and practical, the operator should explain
      the reason and provide a way to seek review.
    </p>
    <p>
      When access ends, previously authorized collaborators may still have access to shared research
      materials under the relevant project arrangement. Export, retention, deletion and any
      surviving permissions must be handled consistently with applicable law, the organization’s
      authority and the final privacy notice.
    </p>
  </section>
  <section id="responsibility">
    <h2>Warranties and responsibility</h2>
    <p>
      To the extent permitted by applicable law, the proposed pilot is provided on an “as available”
      basis without a promise that every feature will be error-free, suitable for a particular study
      or continuously accessible. You must verify outputs, conversions, calculations and shared
      revisions before relying on them.
    </p>
    <p>
      Nothing in these draft terms excludes liability that cannot lawfully be excluded, waives
      mandatory consumer or data-protection rights, excuses fraud or deliberate misconduct, or
      transfers the operator’s own legal duties to users.
    </p>
    <p>
      No financial liability cap, blanket indemnity or jurisdiction-specific exclusion is invented
      here. Any such provision must be reviewed in light of the confirmed operator, service
      arrangement and applicable law before final terms are adopted. Until then, these drafts must
      not be represented as a complete allocation of legal risk.
    </p>
  </section>
  <section id="disputes">
    <h2>Law and disputes</h2>
    <p>
      The governing law, competent forum and operator’s legal address remain to be confirmed. No
      mandatory arbitration, class-action waiver or exclusive court is designated by this draft.
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
      The final terms, any applicable signed agreement and the privacy notice should describe the
      entire relevant arrangement without overriding mandatory legal protections. If a provision is
      unenforceable, the remaining provisions should continue only to the extent permitted by
      applicable law.
    </p>
    <p>
      Failure to enforce a provision on one occasion does not automatically waive it for another
      occasion. Software licenses and rights in research content remain independent of account
      access to this hosted pilot.
    </p>
    <p>
      The operator must preserve a dated copy of adopted terms and communicate material revisions
      appropriately. The date at the top of this page is a drafting date, not a record of your
      acceptance.
    </p>
  </section>
  <section id="contact">
    <h2>Operator and contact details</h2>
    <dl class="contact-details">
      <dt>Legal operator</dt>
      <dd>Awaiting confirmation.</dd>
      <dt>Public legal email</dt>
      <dd>Awaiting confirmation.</dd>
      <dt>Mailing address</dt>
      <dd>Awaiting confirmation.</dd>
      <dt>Governing law</dt>
      <dd>Awaiting confirmation.</dd>
      <dt>Effective date</dt>
      <dd>Not set; this is a draft.</dd>
    </dl>
    <p>
      During the limited pilot, contact the administrator who invited you for operational questions.
      Before public adoption, these details must be completed and the terms reviewed for the
      operator’s jurisdiction and actual service arrangements.
    </p>
  </section>
</LegalPage>
