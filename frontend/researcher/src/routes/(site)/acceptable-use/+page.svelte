<script lang="ts">
  import LegalPage from '$lib/components/marketing/LegalPage.svelte';
  const sections = [
    { id: 'scope', title: 'What this covers' },
    { id: 'rules', title: 'What is not allowed' },
    { id: 'provenance', title: 'Keeping records honest' },
    { id: 'volume', title: 'Volume and automation' },
    { id: 'testing', title: 'Security testing and reports' },
    { id: 'enforcement', title: 'What we may do' },
    { id: 'contact', title: 'Questions and reports' }
  ];
</script>

<LegalPage
  title="Acceptable use"
  description="The rules for using Dynodoc: what is not allowed, how to keep provenance records honest, limits on volume and automation, and how to test security in good faith."
  {sections}
>
  <section id="scope">
    <h2>What this covers</h2>
    <p>
      This page sets out proposed rules for everyone who uses the Dynodoc website, workspace, shared
      links, API and editor add-ins. It forms part of the draft
      <a href="/terms#acceptable-use">terms and conditions</a> and is not yet effective. Use the service
      lawfully and within the authority you have over the files you handle.
    </p>
  </section>
  <section id="rules">
    <h2>What is not allowed</h2>
    <p>Do not:</p>
    <ul>
      <li>
        Upload, share or publish material in violation of privacy, confidentiality,
        intellectual-property rights or applicable law, or material you have no authority to share.
      </li>
      <li>
        Try to reach another person’s private files, drafts, credentials, sessions or connection
        keys, or the servers behind the service, without permission.
      </li>
      <li>
        Introduce malicious files, disrupt the service, probe it for weaknesses outside the testing
        rules below or circumvent access controls, project rules, approvals or limits.
      </li>
      <li>
        Use an import or connection to make the service contact systems you are not authorized to
        use, or to probe unrelated private or internal systems.
      </li>
      <li>
        Use invitations, notifications or shared links to send unsolicited messages, or to host
        unrelated abusive traffic.
      </li>
      <li>Knowingly interfere with other people’s work or with their access.</li>
    </ul>
  </section>
  <section id="provenance">
    <h2>Keeping records honest</h2>
    <p>
      Dynodoc’s value is an accurate record of who changed what. Do not impersonate another person,
      misstate your authority over a file, falsify authorship or use an add-in, connection key,
      signing key or the API to create or submit changes you have no right to make. Do not share
      sessions, connection keys or signing keys, and do not use one on a file you are not authorized
      to use.
    </p>
    <p>
      Do not try to alter, hide or forge history, receipts or audit records, or to pass off a
      document you exported or edited elsewhere as an unmodified Dynodoc record.
    </p>
  </section>
  <section id="volume">
    <h2>Volume and automation</h2>
    <p>
      The service has request, upload and storage limits to protect everyone’s use; the current file
      upload limit is 3 MB. Automated clients may use the documented API at reasonable volumes. Do
      not create accounts or requests to evade a limit, run load or denial-of-service tests, or use
      the service as general-purpose file hosting.
    </p>
  </section>
  <section id="testing">
    <h2>Security testing and reports</h2>
    <p>
      Good-faith security research is welcome within these bounds: test only against your own
      account and data, use synthetic content, stop as soon as you see another person’s information
      and report it. Do not publish credentials, private documents or working exploit details.
      Report what you find privately through the
      <a href="/security#report">vulnerability reporting channel</a>. No legal safe harbour is
      offered by this draft; the operator must decide that with its final terms.
    </p>
  </section>
  <section id="enforcement">
    <h2>What we may do</h2>
    <p>
      If these rules are broken, the operator may remove or disable a shared link, limit a feature,
      suspend an account or disable a connection. Any action should be proportionate and, where
      lawful and practical, explained with a way to seek review. Because Dynodoc keeps an
      append-only history, removing content from view does not by itself erase it; see the
      <a href="/privacy#retention">privacy policy</a>. Ending use is covered in the
      <a href="/terms#ending">terms</a>.
    </p>
  </section>
  <section id="contact">
    <h2>Questions and reports</h2>
    <dl class="contact-details">
      <dt>Report abuse</dt>
      <dd>Awaiting confirmation.</dd>
      <dt>Security vulnerabilities</dt>
      <dd><a href="/security#report">See the security overview</a></dd>
      <dt>Legal operator</dt>
      <dd>Awaiting confirmation.</dd>
    </dl>
    <p>
      Until an abuse contact is published, contact the administrator who invited you or use the
      sharing controls in the product to remove access.
    </p>
  </section>
</LegalPage>
