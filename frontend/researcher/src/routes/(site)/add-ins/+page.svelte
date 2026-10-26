<script lang="ts">
  import { Btn, Faq, Section, scrollable, spotlight } from '$lib/components/site';
  import { AddinMock } from '$lib/components/mock';
  import Dino from '$lib/art/Dino.svelte';
  import Ptero from '$lib/art/Ptero.svelte';
  import { pageMeta, scriptTag, type FaqEntry } from '$lib/seo';

  const DATA_DOC = '/privacy#addins';
  const meta = pageMeta('/add-ins');
  const title = meta?.title ?? 'Editors & add-ins · Dynodoc';
  const description =
    meta?.description ??
    'Add-ins for Word, Excel, PowerPoint, Google Docs, Sheets and Slides: what each captures, requirements and marketplace status.';

  type Host = 'word' | 'excel' | 'powerpoint' | 'docs' | 'sheets' | 'slides';
  type Card = {
    host: Host;
    name: string;
    covers: string;
    works: string;
    needs: string;
    not: string;
    marks: string;
  };

  const office: Card[] = [
    {
      host: 'word',
      name: 'Dynodoc for Word',
      covers:
        'Records edits to the text of existing body paragraphs, up to 2,000 paragraphs, while the pane is open.',
      works: 'Word on the web, and Word for Windows and Mac builds that support WordApi 1.3.',
      needs: 'WordApi 1.3. The pane checks this and says so in an unsupported build.',
      not: 'Formatting, new, deleted or moved paragraphs, tables, lists, images, footnotes, comments and headers or footers. Not Word for iPad, iPhone or Android.',
      marks:
        'Each covered paragraph is wrapped in a content control titled “Dynodoc paragraph” (tag dynodoc.block.<ID>). Removing or duplicating these stops syncing for that file.'
    },
    {
      host: 'excel',
      name: 'Dynodoc for Excel',
      covers:
        'Records edits to existing cell values, formula text and date-only values, up to 2,000 cells.',
      works: 'Excel on the web, and Excel for Windows and Mac builds that support ExcelApi 1.13.',
      needs: 'ExcelApi 1.13, which is also what lets it check for merged cells before binding.',
      not: 'Inserting, deleting or sorting rows or columns, formatting, merged cells, charts, pivot tables, timestamps and changing a cell type or date format. Not Excel for iPad, iPhone or Android.',
      marks:
        'One workbook-scoped named range per synced row, named dynodoc_<ID>. Deleting those names stops syncing for that file.'
    },
    {
      host: 'powerpoint',
      name: 'Dynodoc for PowerPoint',
      covers: 'Records edits to the text of existing slide shapes, up to 2,000 shapes.',
      works:
        'PowerPoint on the web, and PowerPoint for Windows and Mac on current Microsoft 365 builds or Office 2024.',
      needs:
        'PowerPointApi 1.4. PowerPoint 2021 and earlier do not have it, so they cannot use this add-in.',
      not: 'Inserting, deleting or reordering slides or shapes, grouped or table shapes, formatting, media, charts and animations. Not PowerPoint 2021 or earlier, and not the mobile apps.',
      marks: 'A tag named DYNODOC_ID on each synced slide and text shape.'
    }
  ];
  const google: Card[] = [
    {
      host: 'docs',
      name: 'Dynodoc for Google Docs',
      covers:
        'Records edits to the text of existing plain body paragraphs in the tab that was active when you connected, up to 2,000 paragraphs.',
      works:
        'Google Docs in a desktop browser, as an Editor add-on sidebar (Extensions → Dynodoc → Open Dynodoc).',
      needs:
        'A Google account. The add-on asks for access to the current document only, to show a sidebar, and to send requests to Dynodoc.',
      not: 'Formatting, new, deleted or moved paragraphs, tables, lists, images, footnotes, comments and other tabs. Not the Google Docs mobile apps.',
      marks:
        'One named range per paragraph, named dynodoc.block.<ID>. Named ranges are not shown in the normal Docs interface.'
    },
    {
      host: 'sheets',
      name: 'Dynodoc for Google Sheets',
      covers:
        'Records edits to existing cell values, formula text and date-only values, up to 2,000 cells, using the spreadsheet’s own time zone.',
      works: 'Google Sheets in a desktop browser, as an Editor add-on sidebar.',
      needs:
        'A Google account. The add-on asks for access to the current spreadsheet only, to show a sidebar, and to send requests to Dynodoc.',
      not: 'Inserting, deleting or sorting rows or columns, formatting, merged cells in synced rows, charts, timestamps and changing a cell type or date format. Not the Sheets mobile apps.',
      marks: 'One named range per synced row, named dynodoc_<ID>.'
    },
    {
      host: 'slides',
      name: 'Dynodoc for Google Slides',
      covers:
        'Records edits to the text of existing slide shapes, up to 2,000 shapes. Shape accessibility descriptions are preserved.',
      works: 'Google Slides in a desktop browser, as an Editor add-on sidebar.',
      needs:
        'A Google account. The add-on asks for access to the current presentation only, to show a sidebar, and to send requests to Dynodoc.',
      not: 'Inserting, deleting or reordering slides or shapes, grouped or table shapes, formatting, media and animations. Not the Slides mobile apps.',
      marks:
        'Nothing visible. Dynodoc keeps a private mapping from Google’s slide and shape IDs in your own Apps Script user storage.'
    }
  ];

  const steps = [
    {
      n: '1',
      title: 'Connect',
      text: 'Choose Sign in to Dynodoc in the pane, then open the browser link it shows. Check that the code matches, pick one file and approve, then choose I approved this connection in the pane. The editor never sees your password.'
    },
    {
      n: '2',
      title: 'Capture',
      text: 'Turn capture on. While the pane is open, Dynodoc compares snapshots and keeps edits to existing text in a private queue: in the Office pane’s browser storage on your device, or in your private Apps Script user properties for the Google editors. Nothing is sent yet.'
    },
    {
      n: '3',
      title: 'Review',
      text: 'Push the queue for review. It becomes a pull request in your project, where teammates compare before and after, approve or request changes, and merge.'
    },
    {
      n: '4',
      title: 'Apply',
      text: 'Preview the merged team changes in the pane, then apply them. If the file changed after the preview, Dynodoc stops instead of overwriting your work.'
    }
  ];

  const faq: FaqEntry[] = [
    {
      question: 'Do I need an add-in to use Dynodoc?',
      answer:
        'No. You can upload a file to a project, and push an edited copy back for review at any time. The add-ins add capture of existing text and values while the pane is open, and a preview-and-apply step for approved team changes.'
    },
    {
      question: 'Does it record every keystroke or work in the background?',
      answer:
        'No. It compares snapshots while the pane or sidebar is open and capture is switched on. Edits made while it is closed are not reported as observed changes. It is not a keystroke log and does not prove who typed a change.'
    },
    {
      question: 'Will it change my file?',
      answer:
        'It adds small identity markers (content controls, named ranges or tags) so paragraphs, rows and shapes keep their identity. It changes text only when you choose Apply on a preview of changes that were reviewed and merged in Dynodoc.'
    },
    {
      question: 'Is it in the Microsoft or Google marketplace?',
      answer:
        'Not yet. No listing exists, none has been submitted and none is approved. Until then you install a test copy yourself, for your own account.'
    },
    {
      question: 'What happens offline?',
      answer:
        'Loading the pane, signing in and sending changes need a connection. Captured changes wait in a private queue until you send them. Offline commits and background capture are not available.'
    }
  ];
  const faqScript = scriptTag({
    '@context': 'https://schema.org',
    '@type': 'FAQPage',
    mainEntity: faq.map((entry) => ({
      '@type': 'Question',
      name: entry.question,
      acceptedAnswer: { '@type': 'Answer', text: entry.answer }
    }))
  });

  const data: [string, string, string][] = [
    [
      'Where the code runs',
      'In Office’s embedded web view, loaded from app.dynodoc.online.',
      'In Google’s Apps Script runtime, with a sidebar page served by Google.'
    ],
    [
      'Where the capture queue lives',
      'In the browser database of the Office web view, on your device.',
      'In your private Apps Script user properties, never shared with collaborators.'
    ],
    [
      'Where the connection key lives',
      'In the pane’s memory only. Reopening the pane needs sign-in again.',
      'In your private user properties for that file, until it is replaced.'
    ],
    [
      'What leaves your device',
      'Changed text and metadata, to app.dynodoc.online over HTTPS, when you push. Connecting and previewing send only a request that carries your file-scoped key.',
      'The same, sent by the Apps Script runtime to the same address only.'
    ],
    [
      'Other network calls',
      'Loads Microsoft’s Office.js library from appsforoffice.microsoft.com.',
      'None by the add-on. Google’s own services run as the host, and Google may log script errors because exception logging is switched on in the manifest.'
    ],
    [
      'Written into your file',
      'Identity markers, and text only when you choose Apply.',
      'Identity markers, and text only when you choose Apply.'
    ]
  ];
</script>

<svelte:head>
  <title>{title}</title>
  <meta name="description" content={description} />
  <meta property="og:title" content={title} />
  <meta property="og:description" content={description} />
  <meta name="twitter:title" content={title} />
  <meta name="twitter:description" content={description} />
  <!-- Static, first-party FAQ markup; no visitor or document value is used. -->
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  {@html faqScript}
</svelte:head>

{#snippet hostCard(card: Card)}
  <article class="host" data-reveal>
    <div class="host__mock">
      <AddinMock host={card.host} />
      <p class="small caption">
        Illustration with sample content, not a screenshot of a real file.
      </p>
    </div>
    <div class="host__body">
      <h3 class="h-2">{card.name}</h3>
      <p class="status small">
        <span class="dot" aria-hidden="true"></span>Test install today · no marketplace listing
      </p>
      <p class="covers">{card.covers}</p>
      <dl>
        <div>
          <dt>Designed for</dt>
          <dd>{card.works}</dd>
        </div>
        <div>
          <dt>Needs</dt>
          <dd>{card.needs}</dd>
        </div>
        <div>
          <dt>Not covered</dt>
          <dd>{card.not}</dd>
        </div>
        <div>
          <dt>Written to your file</dt>
          <dd>{card.marks}</dd>
        </div>
      </dl>
    </div>
  </article>
{/snippet}

<main id="main" use:scrollable>
  <header class="hero dotgrid dotgrid--fade spotlight" use:spotlight>
    <div class="container hero__grid">
      <div class="hero__copy">
        <h1 class="h-1">Editors and add-ins</h1>
        <p class="lede">
          Connect Word, Excel, PowerPoint, Google Docs, Sheets and Slides to a Dynodoc project.
          Edits to existing text are recorded while the pane is open, sent for review, and approved
          team changes come back only when you apply them.
        </p>
        <ul class="chips" aria-label="Current status">
          <li><span class="badge badge--accent">Test install today</span></li>
          <li><span class="badge">No marketplace listing</span></li>
          <li><span class="badge">Installed-host acceptance pending</span></li>
        </ul>
        <div class="actions">
          <Btn href="#install" arrow>Install a test copy</Btn>
          <Btn href="#how-it-works" variant="secondary">How it works</Btn>
        </div>
      </div>
      <div class="hero__art" aria-hidden="true">
        <Dino name="ptero" art={Ptero} />
      </div>
    </div>
  </header>

  <Section id="how-it-works" title="How the add-ins work">
    <ol class="steps">
      {#each steps as step (step.n)}
        <li class="card step" data-reveal>
          <span class="num">Step {step.n}</span>
          <h3 class="h-3">{step.title}</h3>
          <p>{step.text}</p>
        </li>
      {/each}
    </ol>
  </Section>

  <Section
    id="microsoft"
    title="Word, Excel and PowerPoint"
    lede="Task-pane add-ins. After installing, open the Home tab and choose the Dynodoc button. If a file holds something an add-in does not cover, such as a table or list in Word, a merged cell in Excel, or a grouped, table or other unsupported shape in PowerPoint, or if paragraphs, rows or slides were inserted or deleted, the pane stops with a message and does not capture. Use Push an edited file for those changes."
    tone="muted"
  >
    <div class="hosts">
      {#each office as card (card.host)}{@render hostCard(card)}{/each}
    </div>
  </Section>

  <Section
    id="google"
    title="Google Docs, Sheets and Slides"
    lede="Editor add-ons that open in a sidebar from the Extensions menu. Each editor is its own add-on. If a file holds a table, list or image in Docs, a merged cell in Sheets, or a grouped, table or other unsupported shape in Slides, or if paragraphs, rows or slides were inserted or deleted, the sidebar stops with a message and does not capture. Use Push an edited file for those changes."
  >
    <div class="hosts">
      {#each google as card (card.host)}{@render hostCard(card)}{/each}
    </div>
  </Section>

  <Section
    id="install"
    title="Installing a test copy"
    lede="There is no store listing yet, so each add-in is installed directly. Both routes need only your own Office or Google sign-in. Use a disposable copy of a small file in a Dynodoc project."
    tone="muted"
  >
    <div class="installs">
      <div class="card card--raised" data-reveal>
        <h3 class="h-3">Word, Excel and PowerPoint</h3>
        <ol class="how">
          <li>
            Download the add-in file for your app from the <a href="/connectors"
              >add-ins download page</a
            >.
          </li>
          <li>
            Add it to Office. On Windows, put the file in a shared folder, add that folder as a
            trusted add-in catalog, restart Office, then choose Home, Add-ins, Advanced, Shared
            Folder and pick Dynodoc. On Mac, copy the file into the app’s wef add-in folder and
            restart the app. In Office on the web, upload it in the add-ins dialog.
          </li>
          <li>Open a saved file and choose Home, then Dynodoc, to open the pane.</li>
          <li>
            Choose Sign in to Dynodoc, follow the link to approve one file in the browser, choose I
            approved this connection, then turn capture on.
          </li>
        </ol>
      </div>
      <div class="card card--raised" data-reveal>
        <h3 class="h-3">Google Docs, Sheets and Slides</h3>
        <ol class="how">
          <li>Download the package for your editor from the add-ins download page and unzip it.</li>
          <li>
            Create a standalone Apps Script project named Dynodoc. Add each .gs and .html file with
            the same name (not INSTALL.md or ACCEPTANCE.md), and replace the project’s manifest
            (turn on showing the manifest in Project Settings first).
          </li>
          <li>
            Choose Deploy, Test deployments, Editor add-on, pick a test file and run it. Google may
            ask you to allow access to the current file and requests to Dynodoc.
          </li>
          <li>
            Open Extensions, then Dynodoc, then Open Dynodoc, and sign in to Dynodoc from the
            sidebar.
          </li>
        </ol>
      </div>
    </div>
    <p class="small note">
      A connection reaches one file, expires seven days after you approve it and can be revoked
      under Sync &amp; connections. If browser sign-in is not available, a file-scoped key from the
      same screen can be pasted under Advanced.
    </p>
    <div class="actions" data-reveal>
      <Btn href="/connectors" arrow>Download the add-in files and install guide</Btn>
    </div>
  </Section>

  <Section
    id="data"
    title="What is stored and what is sent"
    lede="The add-in reads only what its adapter covers, only while the pane is open. It does not read other files, other tabs, your contacts, mail, calendar or Drive listing."
  >
    <div class="table-wrap" data-reveal>
      <table>
        <caption class="sr-only">What the Office and Google add-ins store and send</caption>
        <thead>
          <tr>
            <th scope="col">Question</th>
            <th scope="col">Word, Excel, PowerPoint</th>
            <th scope="col">Docs, Sheets, Slides</th>
          </tr>
        </thead>
        <tbody>
          {#each data as [question, officeText, googleText] (question)}
            <tr><th scope="row">{question}</th><td>{officeText}</td><td>{googleText}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
    <div class="never" data-reveal>
      <h3 class="h-3">What it does not do</h3>
      <ul>
        <li>No analytics, advertising, fingerprinting or error-reporting service.</li>
        <li>No background capture: nothing is recorded while the pane or sidebar is closed.</li>
        <li>
          No Microsoft Graph, Google Drive, Gmail or Calendar access. Approval uses your Dynodoc
          account in the browser, not Microsoft or Google single sign-on.
        </li>
        <li>No account-wide token is given to the editor, only a file-scoped key.</li>
        <li>It records observations at snapshot times. It does not prove who typed a change.</li>
      </ul>
    </div>
    <div class="actions" data-reveal>
      <Btn href="/privacy" variant="secondary" arrow>Privacy policy</Btn>
      <Btn href={DATA_DOC} variant="ghost" external arrow>Add-in data handling in detail</Btn>
    </div>
  </Section>

  <Section id="marketplace-status" title="Marketplace status" tone="muted">
    <div class="status-box card card--raised" data-reveal>
      <p class="status-box__lead">
        No Microsoft Marketplace or Google Workspace Marketplace listing exists for these add-ins.
        None has been submitted and none is approved. Publication is paused, with no date.
      </p>
      <ul>
        <li>
          Public Office installation goes through Microsoft Partner Center and Marketplace
          certification. Public Google installation needs a Google Cloud project, OAuth
          configuration and a Workspace Marketplace listing, with a separate verification step. Both
          need a publisher account that has not been set up yet.
        </li>
        <li>
          The add-ins have automated protocol and storage tests. The installed-host acceptance run
          has not been done for any of the six editors yet. Treat a test install as a preview, and
          keep your original files.
        </li>
        <li>No dates or approval outcomes are promised.</li>
      </ul>
    </div>
  </Section>

  <Section id="faq" title="Common questions" layout="split">
    <div data-reveal><Faq items={faq} exclusive /></div>
  </Section>

  <Section
    id="try"
    tone="ink"
    title="Testing with a disposable file"
    lede="A small copy of a file in a project is enough to install a test copy and send one edit through review."
  >
    <div class="actions" data-reveal>
      <Btn href="/connectors" arrow>Get the add-in files</Btn>
      <Btn href="/help" variant="secondary">Read the guide</Btn>
      <Btn href="/support" variant="ghost">Get help</Btn>
    </div>
  </Section>
</main>

<style>
  .hero {
    border-bottom: 1px solid var(--color-border);
  }
  .hero__grid {
    display: grid;
    gap: clamp(28px, 5vw, 64px);
    align-items: center;
    padding-block: clamp(48px, 8vw, 104px) clamp(40px, 6vw, 80px);
  }
  .hero__copy {
    display: grid;
    gap: 22px;
    justify-items: start;
  }
  .hero__copy h1 {
    max-width: 16ch;
  }
  .hero__art {
    max-width: 520px;
    color: var(--color-ink-soft);
    --art-accent: var(--color-accent);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
  .note,
  .actions {
    margin-top: 28px;
  }
  .hero .actions {
    margin-top: 0;
  }

  .steps {
    display: grid;
    gap: 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .step {
    display: grid;
    gap: 12px;
    align-content: start;
  }
  .step p {
    margin: 0;
    color: var(--color-ink-soft);
  }

  .hosts {
    display: grid;
    gap: clamp(28px, 4vw, 48px);
  }
  .host {
    display: grid;
    gap: 24px;
    align-items: start;
    padding-top: clamp(24px, 3vw, 40px);
    border-top: 1px solid var(--color-border);
  }
  .host:first-child {
    padding-top: 0;
    border-top: 0;
  }
  .host__mock {
    min-width: 0;
  }
  .caption {
    margin: 10px 0 0;
  }
  .host__body {
    display: grid;
    gap: 12px;
    min-width: 0;
  }
  .host__body h3 {
    max-width: 22ch;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0;
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-accent);
  }
  .covers {
    margin: 0;
    color: var(--color-ink);
    font-size: 1.0625rem;
    line-height: 1.55;
  }
  dl {
    display: grid;
    gap: 0;
    margin: 6px 0 0;
    border-top: 1px solid var(--color-border);
  }
  dl > div {
    display: grid;
    gap: 4px;
    padding-block: 12px;
    border-bottom: 1px solid var(--color-border);
  }
  dt {
    color: var(--color-muted);
    font-family: var(--site-font-mono);
    font-size: 0.75rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  dd {
    margin: 0;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
    line-height: 1.55;
  }

  .installs {
    display: grid;
    gap: 16px;
  }
  .how {
    display: grid;
    gap: 12px;
    margin: 16px 0 0;
    padding-left: 1.3em;
    color: var(--color-ink-soft);
  }
  .how li::marker {
    color: var(--color-muted);
    font-family: var(--site-font-mono);
  }
  .note {
    max-width: 68ch;
  }

  .table-wrap {
    overflow: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--site-radius);
    background: var(--color-surface-raised);
  }
  table {
    width: 100%;
    min-width: 640px;
    border-collapse: collapse;
    font-size: 0.9375rem;
    line-height: 1.55;
  }
  th,
  td {
    padding: 14px 16px;
    border-bottom: 1px solid var(--color-border);
    text-align: left;
    vertical-align: top;
  }
  thead th {
    background: var(--color-surface-muted);
    color: var(--color-ink);
    font-family: var(--site-font-mono);
    font-size: 0.75rem;
    font-weight: 500;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  tbody th {
    color: var(--color-ink-strong);
    font-weight: 560;
  }
  td {
    color: var(--color-ink-soft);
  }
  tbody tr:last-child > * {
    border-bottom: 0;
  }
  .never {
    margin-top: 32px;
  }
  .never ul {
    display: grid;
    gap: 8px;
    margin: 14px 0 0;
    padding-left: 1.2em;
    max-width: 72ch;
    color: var(--color-ink-soft);
  }

  .status-box {
    display: grid;
    gap: 16px;
    max-width: 880px;
  }
  .status-box__lead {
    margin: 0;
    color: var(--color-ink-strong);
    font-size: 1.1875rem;
    font-weight: 520;
    line-height: 1.45;
  }
  .status-box ul {
    display: grid;
    gap: 10px;
    margin: 0;
    padding-left: 1.2em;
    color: var(--color-ink-soft);
  }

  @media (min-width: 640px) {
    .steps {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  @media (min-width: 900px) {
    .hero__grid {
      grid-template-columns: minmax(0, 7fr) minmax(0, 5fr);
    }
    .installs {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  @media (min-width: 1000px) {
    .steps {
      grid-template-columns: repeat(4, minmax(0, 1fr));
    }
    .host {
      grid-template-columns: minmax(0, 7fr) minmax(0, 5fr);
      gap: clamp(28px, 4vw, 56px);
    }
  }
</style>
