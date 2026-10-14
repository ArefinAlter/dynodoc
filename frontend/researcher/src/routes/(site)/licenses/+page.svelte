<script lang="ts">
  import LegalPage from '$lib/components/marketing/LegalPage.svelte';
  import { ENGINE_REPO_URL, OPEN_SOURCE_STATEMENT, OPERATOR } from '$lib/site/operator';
  const sections = [
    { id: 'engine', title: 'Engine, API and command-line tools' },
    { id: 'software', title: 'Workspace and website' },
    { id: 'fonts', title: 'Fonts' },
    { id: 'dependencies', title: 'Dependencies' },
    { id: 'art', title: 'Artwork, icons and marks' },
    { id: 'content', title: 'Your content' }
  ];
  const fonts = [
    {
      name: 'Public Sans',
      author: 'U.S. Web Design System',
      use: 'Fallback text face and the editors',
      file: 'OFL.txt'
    },
    {
      name: 'Geist',
      author: 'Vercel with Basement Studio',
      use: 'Website text and headings',
      file: 'geist-OFL.txt'
    },
    {
      name: 'Geist Mono',
      author: 'Vercel with Basement Studio',
      use: 'Website labels and code',
      file: 'geist-mono-OFL.txt'
    },
    {
      name: 'Noto Sans Bengali',
      author: 'The Noto Project Authors',
      use: 'Editor script fallback',
      file: 'notosansbengali-OFL.txt'
    },
    {
      name: 'Noto Naskh Arabic',
      author: 'The Noto Project Authors',
      use: 'Editor script fallback',
      file: 'notonaskharabic-OFL.txt'
    },
    {
      name: 'Noto Sans Devanagari',
      author: 'The Noto Project Authors',
      use: 'Editor script fallback',
      file: 'notosansdevanagari-OFL.txt'
    },
    {
      name: 'Noto Sans Hebrew',
      author: 'The Noto Project Authors',
      use: 'Editor script fallback',
      file: 'notosanshebrew-OFL.txt'
    }
  ];
</script>

<LegalPage
  title="Licenses"
  description="The licenses that apply to Dynodoc: the open-source engine, API and command-line tools, the workspace and website, bundled fonts, direct dependencies and artwork."
  {sections}
>
  <section id="engine">
    <h2>Engine, API and command-line tools</h2>
    <p>
      The event-log, versioning and merge engine, its HTTP API and the command-line tools are
      published in a public repository under the MIT license, copyright 2026 Verne Ltd. Source and
      license text: <a href={ENGINE_REPO_URL}>dynodoc-engine on GitHub</a>.
    </p>
    <p>{OPEN_SOURCE_STATEMENT}</p>
  </section>
  <section id="software">
    <h2>Workspace and website</h2>
    <p>
      The Dynodoc workspace, which is the web application and its operator tools, and this website
      are &copy; 2026 {OPERATOR.legalName}. They are proprietary, developed in a private repository
      and not published as open source; no open-source license is offered for them, and no source
      archive of them is available.
    </p>
    <p>
      A software license is separate from the <a href="/terms">terms of using the hosted service</a
      >. Using the hosted service is governed by those terms, not by a software license, and none of
      the licenses on this page applies to your documents or to Dynodoc’s name and artwork.
    </p>
  </section>
  <section id="fonts">
    <h2>Fonts</h2>
    <p>
      All fonts are self-hosted, so reading Dynodoc makes no request to a font service. Each is
      licensed under the SIL Open Font License 1.1; the full text, with the copyright notices, is
      linked for each.
    </p>
    <!-- Scrollable data tables need keyboard access on narrow screens. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" role="region" aria-label="Fonts and their licenses" tabindex="0">
      <table>
        <thead><tr><th>Font</th><th>Designed by</th><th>Used for</th><th>License</th></tr></thead
        ><tbody>
          {#each fonts as font (font.file)}
            <tr
              ><td>{font.name}</td><td>{font.author}</td><td>{font.use}</td><td
                ><a
                  href={`/fonts/${font.file}`}
                  data-sveltekit-reload
                  aria-label={`OFL 1.1 license text for ${font.name}`}>OFL 1.1</a
                ></td
              ></tr
            >
          {/each}
        </tbody>
      </table>
    </div>
  </section>
  <section id="dependencies">
    <h2>Dependencies</h2>
    <p>
      These are the direct runtime dependencies as of 14 October 2026, with licenses taken from the
      lockfiles and installed package metadata. Each keeps its own license, and so do the transitive
      packages beneath them, which are not listed here. The authoritative record is the lockfile; in
      the engine repository, <code>cargo metadata --locked</code> prints the complete Rust set. Build
      and test tools are not shipped and are not listed.
    </p>
    <p>
      Spreadsheet formulas are calculated by Dynodoc’s own formula engine, which is part of the
      workspace and covered by its proprietary license. No third-party calculation library is
      shipped. On this date none of the packages the web application ships, direct or transitive, is
      available only under a GPL-family license; <code>jszip</code> is offered under the MIT license or
      the GPL, and the MIT license applies here.
    </p>
    <h3>Workspace and website (npm)</h3>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" role="region" aria-label="Web dependencies" tabindex="0">
      <table>
        <thead><tr><th>License</th><th>Packages</th></tr></thead><tbody
          ><tr
            ><td>MIT</td><td
              >The 14 TipTap editor packages (<code>@tiptap/*</code>), <code>@xmldom/xmldom</code>,
              <code>docx</code>, <code>exceljs</code>, <code>fflate</code>, <code>jose</code>,
              <code>katex</code>, <code>openapi-fetch</code>, <code>openid-client</code>,
              <code>pagedjs</code>, <code>papaparse</code>, <code>pptxgenjs</code>,
              <code>ulid</code>; Svelte and SvelteKit, whose runtime is compiled into the app</td
            ></tr
          ><tr><td>ISC</td><td><code>@lucide/svelte</code> (icons)</td></tr><tr
            ><td>BSD-2-Clause</td><td><code>mammoth</code></td></tr
          ><tr><td>Apache-2.0</td><td><code>web-vitals</code></td></tr><tr
            ><td>MIT or GPL-3.0-or-later</td><td><code>jszip</code></td></tr
          ></tbody
        >
      </table>
    </div>
    <h3>Engine, API and command-line tools (Rust)</h3>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="table-scroll" role="region" aria-label="Rust dependencies" tabindex="0">
      <table>
        <thead><tr><th>License</th><th>Crates</th></tr></thead><tbody
          ><tr
            ><td>MIT</td><td
              ><code>tokio</code>, <code>tokio-stream</code>, <code>axum</code>, <code>tower</code>,
              <code>tower-http</code>, <code>ulid</code>, <code>pasetors</code>,
              <code>tracing</code>, <code>tracing-subscriber</code></td
            ></tr
          ><tr
            ><td>MIT or Apache-2.0</td><td
              ><code>futures</code>, <code>sqlx</code>, <code>serde</code>,
              <code>serde_json</code>, <code>base64</code>, <code>utoipa</code>,
              <code>chrono</code>, <code>uuid</code>, <code>hex</code>, <code>thiserror</code>,
              <code>anyhow</code>, <code>clap</code>, <code>reqwest</code></td
            ></tr
          ><tr><td>Apache-2.0</td><td><code>similar</code></td></tr><tr
            ><td>Apache-2.0 and ISC</td><td><code>ring</code></td></tr
          ></tbody
        >
      </table>
    </div>
  </section>
  <section id="art">
    <h2>Artwork, icons and marks</h2>
    <p>
      The pencil-and-halftone dinosaur plates on this site are original works of the Dynodoc
      project, drawn as silhouettes and rendered by a script. Interface icons come from Lucide
      (ISC). Names and logos of other products, such as Microsoft Word or Google Docs, identify
      compatibility only; they belong to their owners and imply no partnership or endorsement.
    </p>
  </section>
  <section id="content">
    <h2>Your content</h2>
    <p>
      None of these licenses applies to the documents you put into Dynodoc. You keep your rights in
      them; what Dynodoc may do with them is limited by the
      <a href="/terms#content">terms</a> and the <a href="/privacy">privacy policy</a>.
    </p>
  </section>
</LegalPage>

<style>
  .table-scroll table {
    min-width: 600px;
  }
</style>
