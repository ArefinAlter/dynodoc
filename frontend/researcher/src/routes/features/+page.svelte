<script lang="ts">
  import ProductPage from '$lib/components/marketing/ProductPage.svelte';
  import ResearchJourney from '$lib/components/marketing/ResearchJourney.svelte';
</script>

<ProductPage
  title="The workspace behind the research"
  description="Write the protocol, refine the instrument, review the data, and keep the decisions that connect them."
>
  <section>
    <h2>Follow a revision from draft to shared research</h2>
    <ResearchJourney />
  </section>
  <section class="cards">
    <article>
      <span class="badge">Documents</span>
      <h2>Write with room to revise.</h2>
      <p>
        Use a familiar formatting ribbon, document tabs, headings, lists, tables, links, images,
        checklists and LaTeX equations. Add comments where the discussion belongs. Export Word or a
        LaTeX project for Overleaf.
      </p>
    </article>
    <article>
      <span class="badge">Spreadsheets</span>
      <h2>Keep data work connected.</h2>
      <p>
        Work with multiple sheets, formulas, formatting, filtering and cell ranges. Insert or remove
        rows and columns with formula references adjusted. Rename, duplicate and reorder sheets from
        the tab controls.
      </p>
    </article>
    <article>
      <span class="badge">Questionnaires</span>
      <h2>Let the instrument evolve.</h2>
      <p>
        Author single and multiple choice, ranking, dates, numeric scales, media and location
        definitions. Organize nested groups and repeats. Preserve translations, relevance,
        validation and advanced XLSForm columns.
      </p>
    </article>
    <article>
      <span class="badge">Review</span>
      <h2>Give each proposal a clear path.</h2>
      <p>
        Explore personal drafts, select changes to share, compare versions and review competing
        edits. Stable block identities connect a paragraph or question to its recorded changes.
      </p>
    </article>
  </section>
  <section>
    <h2>Access that follows your team</h2>
    <p>
      Organize research into organizations, teams and folders. Owners and managers control sharing.
      Editors, reviewers and viewers get appropriate access. Publish a frozen read-only copy when
      you want to share outside the team, and revoke it later. Recover documents from Trash without
      rewriting their history.
    </p>
  </section>
  <section>
    <h2>Bring the tools you already use</h2>
    <div class="table-wrap">
      <table>
        <thead
          ><tr><th>Tool or format</th><th>Available connection</th><th>What to check</th></tr
          ></thead
        ><tbody
          ><tr
            ><td>ODK Central / KoboToolbox</td><td
              >Read questionnaires and response snapshots using your API credentials; import/export
              XLSForm.</td
            ><td>Validate exported forms and attach datasets/media in the collection platform.</td
            ></tr
          ><tr
            ><td>Google Forms</td><td
              >Separate read-only OAuth connection for form definitions and responses.</td
            ><td
              >Imports create independent copies. Grid questions flatten; branching, media and quiz
              grading need review.</td
            ></tr
          ><tr
            ><td>REDCap</td><td>Dictionary and record import; data-dictionary export.</td><td
              >REDCap branching logic and action tags need destination testing.</td
            ></tr
          ><tr
            ><td>SurveyCTO / SurveyMonkey</td><td
              >Read response snapshots through supported API endpoints.</td
            ><td>Account plan and API permissions determine availability. No writeback.</td></tr
          ><tr
            ><td>CSPro</td><td>Bring exported CSV or Excel datasets into the workspace.</td><td
              >Native CSPro applications and dictionaries are not executed.</td
            ></tr
          ><tr
            ><td>Word / Excel</td><td>DOCX, DOC, XLSX and CSV import; DOCX, XLSX and CSV export.</td
            ><td
              >Complex layouts, macros, charts and workbook-specific features may not transfer.
              Original files are retained.</td
            ></tr
          ><tr
            ><td>LaTeX / Overleaf</td><td
              >Equation editing, .tex import/export, source ZIP import and Overleaf project ZIP
              export.</td
            ><td
              >Project interchange; no live Overleaf synchronization or server-side TeX compilation.</td
            ></tr
          ></tbody
        >
      </table>
    </div>
  </section>
  <section class="callout">
    <h2>Know what is available today</h2>
    <p>
      Dynodoc supports deliberate, versioned collaboration. Simultaneous character-level cursors,
      complete Office feature parity, pivot-table design, native chart creation and full vendor
      survey runtimes are still outside the current editor. <a href="/roadmap"
        >See the planned research tools</a
      >
      and <a href="/help">learn the current workflow</a>.
    </p>
  </section>
  <a class="cta" href="https://app.dynodoc.online/workspace">Open your workspace</a>
</ProductPage>
