<script lang="ts">
  import { Btn, Section, scrollable, spotlight } from '$lib/components/site';
  import Dino from '$lib/art/Dino.svelte';
  import Fossil from '$lib/art/Fossil.svelte';
  import { pageMeta } from '$lib/seo';

  const meta = pageMeta('/compatibility');
  const title = meta?.title ?? 'Compatibility · Dynodoc';
  const description =
    meta?.description ??
    'What Dynodoc captures, reviews and applies in each editor today, with minimum host versions, file formats and limits.';

  type Level = 'yes' | 'partial' | 'not-yet';
  const LABEL: Record<Level, string> = { yes: 'Yes', partial: 'Partial', 'not-yet': 'Not yet' };
  type Cell = { level: Level; note: string };

  const legend: { level: Level; text: string }[] = [
    {
      level: 'yes',
      text: 'Implemented and covered by automated tests for the content named beside it. The last column says whether it has been run in an installed editor.'
    },
    { level: 'partial', text: 'Works with the restrictions stated.' },
    { level: 'not-yet', text: 'Not available. Use an edited-file push or the original file.' }
  ];

  // Facts: extensions/INSTALL.md, docs/listings/*.md and the add-in adapters.
  const matrix: {
    name: string;
    kind: string;
    cells: [Cell, Cell, Cell, Cell, Cell];
  }[] = [
    {
      name: 'Word',
      kind: 'Paragraph text',
      cells: [
        { level: 'partial', note: 'Text of existing body paragraphs, up to 2,000.' },
        { level: 'yes', note: 'Push for review while the pane is open.' },
        { level: 'partial', note: 'Text only. Preview first; stops if the file changed.' },
        { level: 'not-yet', note: 'Formatting, new, deleted or moved paragraphs, tables, images.' },
        { level: 'not-yet', note: 'Installed-host acceptance run pending.' }
      ]
    },
    {
      name: 'Excel',
      kind: 'Cells',
      cells: [
        {
          level: 'partial',
          note: 'Existing values, formula text and date-only values, up to 2,000 cells.'
        },
        { level: 'yes', note: 'Push for review while the pane is open.' },
        { level: 'partial', note: 'Values only. Preview first; stops if the file changed.' },
        {
          level: 'not-yet',
          note: 'Inserting, deleting or sorting rows, formatting, merged cells, charts.'
        },
        { level: 'not-yet', note: 'Installed-host acceptance run pending.' }
      ]
    },
    {
      name: 'PowerPoint',
      kind: 'Slide shapes',
      cells: [
        { level: 'partial', note: 'Text of existing shapes, up to 2,000.' },
        { level: 'yes', note: 'Push for review while the pane is open.' },
        { level: 'partial', note: 'Text only. Preview first; stops if the file changed.' },
        {
          level: 'not-yet',
          note: 'New, deleted or reordered slides and shapes, grouped or table shapes, media.'
        },
        { level: 'not-yet', note: 'Installed-host acceptance run pending.' }
      ]
    },
    {
      name: 'Google Docs',
      kind: 'Paragraph text',
      cells: [
        { level: 'partial', note: 'Existing plain paragraphs in the connected tab, up to 2,000.' },
        { level: 'yes', note: 'Push for review while the sidebar is open.' },
        { level: 'partial', note: 'Text only. Preview first; stops if the file changed.' },
        {
          level: 'not-yet',
          note: 'Formatting, new or moved paragraphs, tables, lists, images, other tabs.'
        },
        { level: 'not-yet', note: 'Installed-host acceptance run pending.' }
      ]
    },
    {
      name: 'Google Sheets',
      kind: 'Cells',
      cells: [
        {
          level: 'partial',
          note: 'Existing values, formula text and date-only values, up to 2,000 cells.'
        },
        { level: 'yes', note: 'Push for review while the sidebar is open.' },
        { level: 'partial', note: 'Values only. Preview first; stops if the file changed.' },
        {
          level: 'not-yet',
          note: 'Inserting, deleting or sorting rows, formatting, merged cells, charts.'
        },
        { level: 'not-yet', note: 'Installed-host acceptance run pending.' }
      ]
    },
    {
      name: 'Google Slides',
      kind: 'Slide shapes',
      cells: [
        { level: 'partial', note: 'Text of existing shapes, up to 2,000.' },
        { level: 'yes', note: 'Push for review while the sidebar is open.' },
        { level: 'partial', note: 'Text only. Preview first; stops if the file changed.' },
        {
          level: 'not-yet',
          note: 'New, deleted or reordered slides and shapes, grouped or table shapes, media.'
        },
        { level: 'not-yet', note: 'Installed-host acceptance run pending.' }
      ]
    }
  ];
  const columns = [
    'Capture edits',
    'Send for review',
    'Preview and apply',
    'Formatting and structure',
    'Real-host acceptance'
  ];

  const versions: { name: string; need: string; runs: string; not: string }[] = [
    {
      name: 'Word',
      need: 'WordApi 1.3',
      runs: 'Word on the web. Word for Windows and Mac builds that support WordApi 1.3.',
      not: 'iPad, iPhone and Android. Builds without WordApi 1.3.'
    },
    {
      name: 'Excel',
      need: 'ExcelApi 1.13',
      runs: 'Excel on the web. Excel for Windows and Mac builds that support ExcelApi 1.13.',
      not: 'iPad, iPhone and Android. Builds without ExcelApi 1.13.'
    },
    {
      name: 'PowerPoint',
      need: 'PowerPointApi 1.4',
      runs: 'PowerPoint on the web. Windows and Mac on current Microsoft 365 builds or Office 2024.',
      not: 'PowerPoint 2021 and earlier, which lack PowerPointApi 1.4. iPad, iPhone and Android.'
    },
    {
      name: 'Google Docs, Sheets, Slides',
      need: 'Editor add-on sidebar',
      runs: 'A desktop browser, from Extensions → Dynodoc → Open Dynodoc.',
      not: 'The Docs, Sheets and Slides mobile apps, which do not offer Editor add-on sidebars.'
    }
  ];

  const formats: { name: string; level: Level; note: string }[] = [
    {
      name: 'DOCX',
      level: 'yes',
      note: 'Import, review, merge and export of text, common run formatting, lists, tables with spans and supported images. Floating layout, mail merge and native equations are not reproduced. The original is retained.'
    },
    {
      name: 'XLSX',
      level: 'yes',
      note: 'Values, formulas (a documented subset), common formats, notes, merged ranges, widths, named ranges and supported charts. Native pivot caches, macros and external data are not reproduced.'
    },
    {
      name: 'PPTX',
      level: 'partial',
      note: 'Slide order, text with basic formatting, placeholders, pictures, notes and hidden slides. Tables, charts, SmartArt, animations and transitions are not reproduced.'
    },
    {
      name: 'CSV',
      level: 'yes',
      note: 'Imported as a one-sheet workbook, up to 3,000 rows and 100 columns.'
    },
    {
      name: 'DOC (legacy Word)',
      level: 'partial',
      note: 'Converted to DOCX by a server-side converter before import.'
    },
    {
      name: 'Text and Markdown',
      level: 'partial',
      note: 'Imported as plain paragraphs. Markdown syntax is not interpreted.'
    },
    {
      name: 'LaTeX and Overleaf ZIP',
      level: 'partial',
      note: 'Imported as editable source blocks and exported as .tex or a project ZIP. There is no server-side TeX execution.'
    },
    {
      name: 'RTF and other formats',
      level: 'not-yet',
      note: 'RTF is the next planned adapter. Other file formats are not supported, apart from the optional form and survey imports described in the privacy policy.'
    }
  ];

  const limits: { value: string; label: string; note: string }[] = [
    {
      value: '3 MB',
      label: 'Original file size',
      note: '3,000,000 bytes per file while Dynodoc is in testing.'
    },
    {
      value: '2,000',
      label: 'Add-in coverage per file',
      note: 'Paragraphs, cells or shapes projected by an add-in.'
    },
    {
      value: '3,000 × 100',
      label: 'Workbook import',
      note: 'Rows per workbook and columns per sheet.'
    },
    {
      value: '5,000',
      label: 'Operations per save',
      note: 'Use smaller ranges for large formatting or fill actions.'
    },
    {
      value: '100 KB',
      label: 'Google add-on queue',
      note: 'Per file, within about 500 KB Google keeps per user and script.'
    },
    {
      value: '7 days',
      label: 'Connection lifetime',
      note: 'An add-in connection expires seven days after approval and can be revoked sooner.'
    }
  ];

  const platforms: { name: string; level: Level; note: string }[] = [
    {
      name: 'Chrome and Edge',
      level: 'yes',
      note: 'Chrome on Windows, macOS, Linux and Android; Edge on Windows and macOS, which has the same engine. Chrome on Android is the phone target. The whole browser test suite runs on Chromium 148; Edge, macOS and Android were not run separately.'
    },
    {
      name: 'Firefox',
      level: 'partial',
      note: 'Windows, macOS and Linux. The whole suite has been run on Firefox 150; three tests failed once in a long run and pass when run alone. Firefox on a computer cannot install the web app.'
    },
    {
      name: 'Safari',
      level: 'partial',
      note: 'macOS, iOS and iPadOS. Safari on iPhone and iPad is the Apple phone and tablet target, and every browser on iOS uses the same WebKit engine. The suite has been run on a WebKit 26.4 build, about the engine of Safari 26.4, and not on Safari itself. A test of several people editing one document fails on WebKit in every run and is unexplained, so treat that as unverified there.'
    },
    {
      name: 'Phones and tablets',
      level: 'partial',
      note: 'Public pages adapt to phone widths, and the workspace has small-screen layouts since 23 October. Phone-sized screens are emulated in the tests; no real phone or tablet has been tried. The limits are listed below.'
    },
    {
      name: 'Accessibility',
      level: 'partial',
      note: 'Labelled controls, keyboard navigation and document-language support. Public pages are checked with automated WCAG A and AA rules, which does not establish full conformance. Manual screen-reader testing is not complete.'
    }
  ];

  // Facts: docs/BROWSER-SUPPORT.md, run on 21 October 2026.
  const runs: { name: string; engine: string; covers: string }[] = [
    { name: 'Chromium', engine: 'Chrome for Testing 148', covers: 'The whole suite, every time.' },
    { name: 'Firefox', engine: 'Firefox 150', covers: 'The whole suite, run locally.' },
    {
      name: 'WebKit',
      engine: 'The Playwright WebKit build, about the engine of Safari 26.4',
      covers: 'The whole suite, run locally. Not Safari itself.'
    },
    {
      name: 'Pixel (emulated)',
      engine:
        'A Pixel 7 profile: 412 × 839 screen, touch, an Android 14 Chrome user agent, on a Chromium engine',
      covers: 'The public, workspace and projects tests.'
    },
    {
      name: 'iPhone (emulated)',
      engine:
        'An iPhone 15 profile: 393 × 659 screen, touch, an iOS 17.5 Safari user agent, on WebKit',
      covers: 'The public, workspace and projects tests.'
    }
  ];

  const notRun = [
    'Any real phone or tablet, and Safari itself on macOS or iOS.',
    'Safari’s zoom into a focused field (the tests check that fields are at least 16 px, the rule that prevents it), its collapsing address bar, the notch, and the on-screen keyboard covering the page.',
    'Real touch behaviour such as long-press menus, momentum scrolling, pinch zoom and edge-swipe back.',
    'Installing the app through an operating system’s own flow, and how it behaves once installed.',
    'How fast it runs on a phone’s processor and memory.'
  ];

  const smallScreens = [
    'Documents open in continuous view, which reflows to the screen. A printed page cannot be shown at a readable size on a phone. “Print layout” in the status bar still shows the page as it prints, and you pan to read it.',
    'Comments, version history and sharing take the whole screen.',
    'Spreadsheets can be read, scrolled, selected, and a cell’s value changed in the formula bar. Typing straight into cells is cramped, and the sheet says once per visit that spreadsheets are easier to edit on a larger screen. A double tap edits a cell in place.',
    'A presentation opens and shows its slides. Editing slides on a phone was not exercised, and moving shapes by dragging is awkward on a small screen.',
    'The toolbar rows, tab bar and status line take most of a phone’s height. On the iPhone 15 profile the sheet grid shows about two rows until the notice is dismissed.',
    'Tablets under about 700 px wide behave as phones; wider ones as a desktop with touch. Landscape phones leave little room for the document, and padding for a notch could not be checked without a device.',
    'WebKit’s tracking prevention can delete script-written storage, such as the editor’s local recovery copies, for a site not visited for seven days. WebKit documents this; it was not tested here.'
  ];

  const install: { where: string; how: string }[] = [
    {
      where: 'Chrome or Edge on Windows, macOS or Linux',
      how: 'Use the install icon at the right of the address bar, the browser menu (“Install Dynodoc”), or the Install app button on your profile page.'
    },
    {
      where: 'Chrome on Android',
      how: 'The Install app button on your profile page, or the menu: “Install app” or “Add to Home screen”.'
    },
    {
      where: 'Safari on iPhone or iPad',
      how: 'Share, then Add to Home Screen, then Add. On iOS 16.4 and later other browsers can do this too.'
    },
    { where: 'Safari 17 or later on macOS', how: 'File, then Add to Dock.' },
    { where: 'Firefox on Android', how: 'Menu, then Install.' },
    {
      where: 'Firefox on a computer',
      how: 'Not possible. Use another browser, or keep a pinned tab.'
    }
  ];

  const offlineDoes = [
    'If a page cannot load because the network is missing or the server cannot be reached, it shows a small “You are offline” page with a Try again button instead of the browser’s error page. A reply from the server, including an error, is shown as sent.',
    'It keeps the site’s public script, style and font files after they are first used, so repeat visits start faster.'
  ];

  const web = [
    {
      name: 'Documents',
      text: 'Headings, fonts, lists, links, tables, comments, text revisions, footnotes, tables of contents and A4, Letter or Legal pages. This is not Word’s layout engine: floating layouts, mail merge, citations and native equations remain incomplete.'
    },
    {
      name: 'Spreadsheets',
      text: 'Multiple sheets, a documented formula subset, sorting and filtering, merged ranges, frozen panes, number formats, validation, charts and pivot summaries. Native pivot caches, macros, external data and full function parity are not available.'
    },
    {
      name: 'Presentations',
      text: 'Slide rail, text and shape editing, layouts, themes, speaker notes, hidden slides and PPTX import and export. Tables, charts, SmartArt, animations and transitions are not reproduced.'
    }
  ];
</script>

<svelte:head>
  <title>{title}</title>
  <meta name="description" content={description} />
  <meta property="og:title" content={title} />
  <meta property="og:description" content={description} />
  <meta name="twitter:title" content={title} />
  <meta name="twitter:description" content={description} />
</svelte:head>

{#snippet chip(level: Level)}
  <span class="chip chip--{level}"
    ><span class="chip__mark" aria-hidden="true"
      >{level === 'yes' ? '✓' : level === 'partial' ? '◐' : '×'}</span
    >{LABEL[level]}</span
  >
{/snippet}

<main id="main" use:scrollable>
  <header class="hero dotgrid dotgrid--fade spotlight" use:spotlight>
    <div class="container hero__grid">
      <div class="hero__copy">
        <h1 class="h-1">Compatibility</h1>
        <p class="lede">
          Dynodoc records and reviews supported changes in Word, Excel, PowerPoint and the three
          Google editors. This page lists what is covered today, the minimum host versions, the file
          formats and the hard limits. It is not a claim of full Office parity. Updated 26 October
          2026.
        </p>
        <ul class="legend" aria-label="How to read the chips">
          {#each legend as item (item.level)}
            <li>{@render chip(item.level)}<span class="small">{item.text}</span></li>
          {/each}
        </ul>
      </div>
      <div class="hero__art" aria-hidden="true">
        <Dino name="fossil" art={Fossil} />
      </div>
    </div>
  </header>

  <Section
    id="editors"
    title="Editor add-in coverage"
    lede="Add-ins capture edits to existing text and values while the pane or sidebar is open. Everything else goes through an edited-file push, which keeps the same review and merge rules. If a file holds something an add-in does not cover, such as a table or list in Word and Docs, a merged cell in Excel and Sheets, or a grouped, table or other unsupported shape in PowerPoint and Slides, or if paragraphs, rows or slides were inserted or deleted, the pane or sidebar stops with a message and does not capture."
  >
    <div class="table-wrap matrix" data-reveal>
      <table>
        <caption class="sr-only">Editor add-in coverage</caption>
        <thead>
          <tr>
            <th scope="col">Editor</th>
            {#each columns as column (column)}<th scope="col">{column}</th>{/each}
          </tr>
        </thead>
        <tbody>
          {#each matrix as row (row.name)}
            <tr>
              <th scope="row">{row.name}<span class="kind">{row.kind}</span></th>
              {#each row.cells as cell, i (i)}
                <td>{@render chip(cell.level)}<span class="note">{cell.note}</span></td>
              {/each}
            </tr>
          {/each}
        </tbody>
      </table>
    </div>

    <ul class="cards-m" data-reveal aria-label="Editor add-in coverage by editor">
      {#each matrix as row (row.name)}
        <li class="card">
          <h3 class="h-3">{row.name}</h3>
          <dl>
            {#each row.cells as cell, i (i)}
              <div>
                <dt>{columns[i]}</dt>
                <dd>{@render chip(cell.level)}<span class="note">{cell.note}</span></dd>
              </div>
            {/each}
          </dl>
        </li>
      {/each}
    </ul>
  </Section>

  <Section
    id="hosts"
    title="Minimum host versions"
    lede="The pane checks the requirement set at runtime and says so in an unsupported host. An installed program or a valid manifest does not prove the add-in works."
    tone="muted"
  >
    <div class="table-wrap" data-reveal>
      <table>
        <caption class="sr-only">Minimum host versions</caption>
        <thead>
          <tr>
            <th scope="col">Editor</th>
            <th scope="col">Requires</th>
            <th scope="col">Designed for</th>
            <th scope="col">Not supported</th>
          </tr>
        </thead>
        <tbody>
          {#each versions as row (row.name)}
            <tr>
              <th scope="row">{row.name}</th>
              <td class="mono need">{row.need}</td>
              <td>{row.runs}</td>
              <td>{row.not}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="small after">
      In an unsupported editor you can still upload a file to a project and push an edited copy for
      review. <a class="link-sweep" href="/add-ins">See the add-ins</a>
    </p>
  </Section>

  <Section
    id="formats"
    title="File formats"
    lede="Dynodoc keeps the original upload and builds a reviewable copy with stable block identities. Check the conversion report before relying on a complex file."
  >
    <div class="table-wrap" data-reveal>
      <table>
        <caption class="sr-only">File formats</caption>
        <thead>
          <tr>
            <th scope="col">Format</th>
            <th scope="col">Support</th>
            <th scope="col">What happens</th>
          </tr>
        </thead>
        <tbody>
          {#each formats as row (row.name)}
            <tr>
              <th scope="row">{row.name}</th>
              <td>{@render chip(row.level)}</td>
              <td>{row.note}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  </Section>

  <Section id="limits" title="Limits" tone="muted">
    <ul class="limits" data-reveal>
      {#each limits as limit (limit.label)}
        <li class="card card--raised">
          <span class="num num--lg">{limit.value}</span>
          <span class="h-3">{limit.label}</span>
          <span class="small">{limit.note}</span>
        </li>
      {/each}
    </ul>
  </Section>

  <Section
    id="platforms"
    title="Browsers and accessibility"
    lede="The commitment is the last two major versions of Chrome, Edge, Firefox and Safari, and Chrome on Android. Older versions may work; nothing here promises it."
  >
    <div class="table-wrap" data-reveal>
      <table>
        <caption class="sr-only">Browsers and accessibility</caption>
        <thead>
          <tr>
            <th scope="col">Area</th>
            <th scope="col">Support</th>
            <th scope="col">Notes</th>
          </tr>
        </thead>
        <tbody>
          {#each platforms as row (row.name)}
            <tr>
              <th scope="row">{row.name}</th>
              <td>{@render chip(row.level)}</td>
              <td
                >{row.note}
                {#if row.name === 'Accessibility'}<a class="link-sweep" href="/accessibility"
                    >Read the accessibility statement</a
                  >{/if}</td
              >
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="small after">
      The features the code relies on set a practical floor of Chrome and Edge 111, Firefox 121 and
      Safari or iOS 16.4. Those versions are read from the features used. They are not tested.
    </p>
  </Section>

  <Section
    id="tested"
    title="Browsers used by the tests"
    lede="Run on 21 October 2026, on a Windows 11 computer, against a local copy of the whole stack. Emulation sets a screen size, touch input and a user agent on a desktop browser engine. It is not a phone."
    tone="muted"
  >
    <div class="table-wrap" data-reveal>
      <table>
        <caption class="sr-only">Browsers and devices used by the tests</caption>
        <thead>
          <tr>
            <th scope="col">Run</th>
            <th scope="col">Engine and device</th>
            <th scope="col">What it covers</th>
          </tr>
        </thead>
        <tbody>
          {#each runs as row (row.name)}
            <tr><th scope="row">{row.name}</th><td>{row.engine}</td><td>{row.covers}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
    <h3 class="h-3 sub">What the tests do not cover</h3>
    <ul class="bullets" data-reveal>
      {#each notRun as item (item)}<li>{item}</li>{/each}
    </ul>
    <p class="small after">
      On WebKit two tests fail: the several-people test mentioned above, and one page-layout test
      that misses a boundary by 1/16 of a pixel, with no visible effect. The Firefox failures looked
      like timing under load.
    </p>
  </Section>

  <Section
    id="small-screens"
    title="Small-screen limits"
    lede="These apply to screens under about 700 px wide and come from the same test run."
  >
    <ul class="bullets" data-reveal>
      {#each smallScreens as item (item)}<li>{item}</li>{/each}
    </ul>
  </Section>

  <Section
    id="app"
    title="Installing the workspace as an app"
    lede="The workspace can be installed as a web app. It opens at the workspace and has Workspace and Projects shortcuts. Your profile page has an Install app section that shows what applies to the browser you are using."
    tone="muted"
  >
    <div class="table-wrap" data-reveal>
      <table>
        <caption class="sr-only">How to install the web app, by browser</caption>
        <thead>
          <tr><th scope="col">Where</th><th scope="col">How</th></tr>
        </thead>
        <tbody>
          {#each install as row (row.where)}
            <tr><th scope="row">{row.where}</th><td>{row.how}</td></tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="small after">
      These steps follow each browser’s own flow. Installing on a real operating system has not been
      tried by the project.
    </p>
    <h3 class="h-3 sub">What offline does and does not do</h3>
    <p class="after">The service worker that makes the app installable does two things only.</p>
    <ul class="bullets" data-reveal>
      {#each offlineDoes as item (item)}<li>{item}</li>{/each}
    </ul>
    <p class="after">
      It does not make documents available offline, queue edits for later, store anything from your
      account on the device, or change how sign-in cookies work. An editor holding unsaved work
      keeps it on screen while offline, but saving needs the server.
    </p>
    <p class="small after">
      A new version installs in the background and waits. The page then says that a new version is
      ready, and nothing reloads until you press Update now.
    </p>
  </Section>

  <Section
    id="web-editors"
    title="Web editors"
    lede="The web editors stay available for quick edits and for documents that began there. Matching Word, Excel or PowerPoint is not their goal."
    tone="muted"
  >
    <ul class="web" data-reveal>
      {#each web as item (item.name)}
        <li class="card card--raised">
          <h3 class="h-3">{item.name}</h3>
          <p>{item.text}</p>
        </li>
      {/each}
    </ul>
  </Section>

  <Section
    id="check"
    title="Checking a representative file"
    lede="Add a copy to a project, review the conversion report, change a paragraph or a formula, and compare the result with your original before you depend on a complex workflow."
  >
    <div class="actions" data-reveal>
      <Btn href="/help" arrow>Learn the workflow</Btn>
      <Btn href="/roadmap" variant="secondary">See the roadmap</Btn>
      <Btn href="/support" variant="ghost">Ask for help</Btn>
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
    max-width: 18ch;
  }
  .hero__art {
    max-width: 440px;
    color: var(--color-ink-soft);
    --art-accent: var(--color-accent);
  }
  .legend {
    display: grid;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .legend li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 12px;
  }

  .chip {
    display: inline-flex;
    flex: none;
    align-items: center;
    gap: 6px;
    padding: 1px 9px;
    border: 1px solid var(--color-border-strong);
    border-radius: 999px;
    background: transparent;
    color: var(--color-ink-soft);
    font-family: var(--site-font-mono);
    font-size: 0.6875rem;
    font-weight: 500;
    letter-spacing: 0.06em;
    line-height: 1.7;
    text-transform: uppercase;
    white-space: nowrap;
  }
  .chip__mark {
    font-size: 0.8125rem;
    line-height: 1;
  }
  .chip--yes {
    border-color: var(--color-accent-border);
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
  }
  .chip--partial {
    border-color: var(--color-warn-border);
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
  }
  .chip--not-yet {
    border-style: dashed;
    color: var(--color-ink-soft);
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
  .kind {
    display: block;
    margin-top: 2px;
    color: var(--color-muted);
    font-size: 0.8125rem;
    font-weight: 400;
  }
  .note {
    display: block;
    margin-top: 8px;
  }
  .need {
    white-space: nowrap;
  }
  .matrix table {
    min-width: 980px;
  }
  .cards-m {
    display: none;
    gap: 14px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .cards-m dl {
    display: grid;
    margin: 14px 0 0;
    border-top: 1px solid var(--color-border);
  }
  .cards-m dl > div {
    display: grid;
    gap: 6px;
    padding-block: 12px;
    border-bottom: 1px solid var(--color-border);
  }
  .cards-m dl > div:last-child {
    border-bottom: 0;
  }
  .cards-m dt {
    color: var(--color-muted);
    font-family: var(--site-font-mono);
    font-size: 0.75rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .cards-m dd {
    display: grid;
    gap: 6px;
    justify-items: start;
    margin: 0;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
  }
  .cards-m .note {
    margin-top: 0;
  }

  .after {
    max-width: 68ch;
    margin: 20px 0 0;
  }
  .sub {
    margin: 32px 0 0;
  }
  .bullets {
    display: grid;
    gap: 10px;
    margin: 16px 0 0;
    padding: 0 0 0 1.1em;
    max-width: 72ch;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
    line-height: 1.55;
  }
  .bullets li::marker {
    color: var(--color-muted);
  }
  .link-sweep {
    color: var(--color-accent-strong);
  }
  td .link-sweep {
    display: inline-block;
    margin-top: 8px;
  }

  .limits,
  .web {
    display: grid;
    gap: 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .limits li {
    display: grid;
    gap: 8px;
    align-content: start;
  }
  .web p {
    margin: 10px 0 0;
    color: var(--color-ink-soft);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  @media (max-width: 859px) {
    .matrix {
      display: none;
    }
    .cards-m {
      display: grid;
    }
  }
  @media (min-width: 640px) {
    .limits,
    .web {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  @media (min-width: 900px) {
    .hero__grid {
      grid-template-columns: minmax(0, 7fr) minmax(0, 5fr);
    }
    .limits,
    .web {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
  }
</style>
