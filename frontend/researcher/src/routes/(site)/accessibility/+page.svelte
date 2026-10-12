<script lang="ts">
  import LegalPage from '$lib/components/marketing/LegalPage.svelte';
  const sections = [
    { id: 'commitment', title: 'Our goal' },
    { id: 'practice', title: 'What we do' },
    { id: 'testing', title: 'How it is tested' },
    { id: 'limitations', title: 'Known limitations' },
    { id: 'feedback', title: 'Feedback and requests' }
  ];
</script>

<LegalPage
  title="Accessibility statement"
  description="Dynodoc's accessibility goal (WCAG 2.2 level AA), what is done and tested, the known limitations, and how to tell us about a barrier."
  {sections}
>
  <section id="commitment">
    <h2>Our goal</h2>
    <p>
      Dynodoc aims for its website and workspace to meet the Web Content Accessibility Guidelines
      (WCAG) 2.2 at level AA. That is a goal, not a claim of conformance: the product has not had a
      full manual audit by an independent accessibility specialist, and this statement does not
      assert that every page or feature meets the standard.
    </p>
  </section>
  <section id="practice">
    <h2>What we do</h2>
    <ul>
      <li>
        Pages use one main landmark, one top-level heading in order, and a skip link to the content.
      </li>
      <li>
        The public site is built to be used with a keyboard, with a visible focus ring and targets
        of at least 24 pixels. Text colours are chosen for at least 4.5 to 1 contrast in the light
        and the dark theme.
      </li>
      <li>
        Layouts reflow down to a 390 pixel wide screen without horizontal scrolling. Text can be
        enlarged with the browser, and legal pages print cleanly.
      </li>
      <li>
        Content is in the page without JavaScript. Motion respects the reduced-motion setting, and
        moving content that lasts more than five seconds has a pause control.
      </li>
      <li>
        Colour is not the only signal: change lines carry plus and minus marks, and decorative
        artwork is hidden from assistive technology.
      </li>
    </ul>
  </section>
  <section id="testing">
    <h2>How it is tested</h2>
    <p>
      The automated test suite runs the axe-core engine against the public pages, including the
      sign-in page and the legal and trust pages, with the WCAG 2 A and AA, WCAG 2.1 A and AA and
      WCAG 2.2 AA rule sets plus best-practice rules, and requires zero violations. A set of
      signed-in workspace and editor views are checked the same way in the authenticated test stack.
    </p>
    <p>
      Automated tools find only part of the possible barriers. Testing with a screen reader, speech
      input and native input-method editors on real devices, and by people who use them, has not
      been completed and is still needed.
    </p>
  </section>
  <section id="limitations">
    <h2>Known limitations</h2>
    <ul>
      <li>
        <strong>Editors.</strong> The rich-text and spreadsheet editors are complex custom components.
        They have labelled controls and keyboard navigation but are not guaranteed to work with every
        screen reader, and they are not meant to match the accessibility of Word, Excel or Google Docs.
      </li>
      <li>
        <strong>Documents you create.</strong> Basic checks for heading order, image descriptions and
        table headers help authors, but exported files are not certified accessible.
      </li>
      <li>
        <strong>Add-ins.</strong> The Word, Excel, PowerPoint, Docs, Sheets and Slides add-ins run in
        panes and sidebars provided by Microsoft and Google, whose own accessibility behaviour applies.
        They have not been tested with assistive technology in installed editors.
      </li>
      <li>
        <strong>Comparison views.</strong> Change views are dense. Changes carry plus and minus marks
        as well as colour, but reading them with a screen reader can be slow.
      </li>
      <li>
        <strong>User content.</strong> Images and files that people add to documents may lack text alternatives.
      </li>
    </ul>
  </section>
  <section id="feedback">
    <h2>Feedback and requests</h2>
    <p>
      If you meet a barrier or need information in another form, please tell us what you were trying
      to do, the page address and the assistive technology you use. We will try to fix it or offer
      an alternative; no response time is promised by this draft.
    </p>
    <dl class="contact-details">
      <dt>Accessibility contact</dt>
      <dd>Awaiting confirmation.</dd>
      <dt>Meanwhile</dt>
      <dd>
        Use the <a href="/support">support page</a> or contact the administrator who invited you.
      </dd>
      <dt>Statement prepared</dt>
      <dd>12 October 2026</dd>
    </dl>
  </section>
</LegalPage>
