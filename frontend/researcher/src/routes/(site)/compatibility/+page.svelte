<script lang="ts">
  import { Btn, Eyebrow, Section, scrollable, spotlight } from '$lib/components/site';
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
    { level: 'yes', text: 'Works for the content named beside it.' },
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
      note: 'RTF is the next planned adapter. Anything not listed here is not supported.'
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
      note: 'An add-in connection expires after approval and can be revoked sooner.'
    }
  ];

  const platforms: { name: string; level: Level; note: string }[] = [
    {
      name: 'Chromium-based browsers (Chrome, Edge)',
      level: 'yes',
      note: 'Automated and live checks so far have run in Chromium.'
    },
    {
      name: 'Firefox and Safari',
      level: 'partial',
      note: 'Targeted at their two most recent major versions. They are not yet part of the automated checks.'
    },
    {
      name: 'Phones and tablets',
      level: 'partial',
      note: 'Public pages adapt to phone widths. The workspace and editors are desktop-first and not tested on phones.'
    },
    {
      name: 'Accessibility',
      level: 'partial',
      note: 'Labelled controls, keyboard navigation and document-language support. Public pages are checked with automated WCAG A and AA rules, which does not establish full conformance. Manual screen-reader testing is not complete.'
    }
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
        <Eyebrow label="COMPATIBILITY · UPDATED 12 OCTOBER 2026" />
        <h1 class="h-1">What works where, and <span class="hl">what does not yet</span>.</h1>
        <p class="lede">
          Dynodoc records and reviews supported changes in Word, Excel, PowerPoint and the three
          Google editors. This page lists what is covered today, the minimum host versions, the file
          formats and the hard limits. It is not a claim of full Office parity.
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
    eyebrow="EDITORS"
    title="What each editor add-in does today."
    lede="Add-ins capture edits to existing text and values while the pane or sidebar is open. Everything else goes through an edited-file push, which keeps the same review and merge rules."
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
    eyebrow="MINIMUM HOST VERSIONS"
    title="What your editor needs."
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
            <th scope="col">Runs in</th>
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
    eyebrow="FILE FORMATS"
    title="Files you can bring into a project."
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

  <Section id="limits" eyebrow="LIMITS" title="The numbers to know." tone="muted">
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

  <Section id="platforms" eyebrow="BROWSERS AND ACCESSIBILITY" title="Where the web app runs.">
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
  </Section>

  <Section
    id="web-editors"
    eyebrow="WEB EDITORS"
    title="The built-in editors."
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
    eyebrow="BEFORE YOU RELY ON IT"
    title="Check a representative file."
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
