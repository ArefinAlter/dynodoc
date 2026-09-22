<script lang="ts">
  import ProductPage from '$lib/components/marketing/ProductPage.svelte';
  const documents = [
    [
      'Writing & formatting',
      'Headings 1–6, font family and size, colors, highlighting, emphasis, lists, links, alignment, paragraph spacing, indentation and equations.'
    ],
    [
      'Pages',
      'A4, Letter and Legal; portrait or landscape; margins, plain-text headers/footers, page numbers and explicit page breaks. Editable pages reflow as you type, with a continuous-view option and a separate print/PDF preview.'
    ],
    [
      'Tables',
      'Insert, resize columns, add/remove rows and columns, header rows and merged cells. Complex floating tables and exact Word layout need review.'
    ],
    [
      'Review',
      'Track text insertions/deletions inside existing paragraphs and accept/reject them. Comments, personal drafts, suggestions and version history. Formatting revisions and structural edits use version history rather than Word-style redlines.'
    ],
    [
      'Languages & input',
      'Unicode text, automatic/LTR/RTL direction, document language and composition-aware input. Font availability and platform-specific IMEs still require testing with your workflow.'
    ],
    [
      'Fonts',
      'System font selection and shared WOFF2, WOFF, TTF or OTF uploads, up to 2 MB each and 16 families per document. Font names export to Word; font files are not embedded in DOCX or public copies.'
    ],
    [
      'Accessibility',
      'Keyboard ribbon navigation, labelled controls, document language, image descriptions and basic heading/table/link checks. Automated checks do not establish full screen-reader or WCAG conformance.'
    ],
    [
      'DOCX exchange',
      'Native style inheritance, common run formatting, paragraph direction, lists, table spans, supported embedded images, text revisions and imported comments. Original files and conversion notes are retained.'
    ],
    [
      'Advanced Word features',
      'Floating layouts, native equation round trips, footnote/endnote bodies, mail merge, citations, full section layout, tracked moves and formatting revisions remain incomplete.'
    ]
  ];
  const sheets = [
    [
      'Editing & calculation',
      'Multiple sheets, formula bar, keyboard ranges, sort/filter, find/replace, summaries and relative formula fill. The calculation engine implements a documented formula subset.'
    ],
    [
      'Layout & formatting',
      'Fonts, emphasis, colors, common number formats, borders, wrapping, merged ranges, frozen panes, widths/heights and hidden rows/columns.'
    ],
    [
      'Conditional formatting',
      'Numeric greater-than, less-than, equality and between rules, including calculated values, with XLSX import and export. Formula rules, color scales, data bars and icon sets remain incomplete.'
    ],
    [
      'Validation',
      'Inline lists, whole/decimal number and text-length constraints. Other imported validation expressions are retained for Excel but are not fully evaluated here.'
    ],
    [
      'XLSX exchange',
      'Values and formulas, numeric/date/text types, common formatting, notes, validation metadata, merged ranges, widths/heights and freeze settings. XLSX conversion runs in a cancellable browser worker.'
    ],
    [
      'Advanced Excel features',
      'Pivot tables, native chart authoring, VBA/macros, external data connections, advanced conditional formatting and complete Excel function parity remain incomplete.'
    ],
    [
      'Current limits',
      '10 MB upload, 3,000 rows per workbook import and 100 columns per sheet. Ordinary edits are limited to 5,000 operations per save; use smaller ranges for large formatting or fill operations.'
    ]
  ];
</script>

<ProductPage
  title="Know what your files can do here"
  description="Dynodoc puts version control at the center of document and spreadsheet work. This is the current compatibility guide, not a claim of complete Microsoft Office parity."
  eyebrow="Editor compatibility · Updated 22 September 2026"
>
  <section>
    <h2>Documents</h2>
    <div class="table-wrap">
      <table>
        <thead><tr><th scope="col">Area</th><th scope="col">Current support</th></tr></thead><tbody
          >{#each documents as [name, description]}<tr
              ><th scope="row">{name}</th><td>{description}</td></tr
            >{/each}</tbody
        >
      </table>
    </div>
  </section>
  <section>
    <h2>Spreadsheets</h2>
    <div class="table-wrap">
      <table>
        <thead><tr><th scope="col">Area</th><th scope="col">Current support</th></tr></thead><tbody
          >{#each sheets as [name, description]}<tr
              ><th scope="row">{name}</th><td>{description}</td></tr
            >{/each}</tbody
        >
      </table>
    </div>
  </section>
  <section>
    <h2>Version control is the common foundation</h2>
    <p>
      Personal drafts, named versions, selected sharing, comparison, overlap review and restoration
      work across documents and spreadsheets. History records both content and formatting. Restoring
      a version adds changes; it never erases earlier events.
    </p>
    <p>
      Collaboration currently uses a central server and explicit sharing. Offline replica
      synchronization, portable remote histories and character-level live cursors remain engineering
      work. Real 300–400-page files have not yet been certified for a fixed import-time guarantee.
    </p>
  </section>
  <section class="callout">
    <h2>Check a representative file</h2>
    <p>
      Import a copy, review the conversion report, edit a paragraph or a formula, and export it back
      to Word or Excel. Compare the result with your original before depending on a complex file
      workflow.
    </p>
    <a href="/roadmap">See the engineering priorities</a> · <a href="/help">Learn the workflow</a>
  </section>
</ProductPage>
