<script lang="ts">
  import ProductPage from '$lib/components/marketing/ProductPage.svelte';
  import { guide, versionVocabulary } from '$lib/workspace/guide';
</script>

<ProductPage
  title="Make yourself at home."
  description="A practical guide to editing, sharing and keeping a clear history of your research."
  eyebrow="Help & tutorials"
>
  <section>
    <p>
      The Help & tutorials button in the workspace, editor and admin screen replays the guided tour.
      Use this page whenever you need a longer explanation.
    </p>
    {#each guide as topic}<details>
        <summary>{topic.title}</summary>
        <p><strong>{topic.action}.</strong> {topic.body}</p>
      </details>{/each}
  </section>
  <section>
    <h2>Everyday editing</h2>
    <div class="cards">
      <article>
        <h3>Documents</h3>
        <p>
          Use the ribbon to format selected text. Right-click for comments, clipboard actions and
          table tools. Add a tab for an appendix or another part of the document. Insert equations
          with the ∑ button. Click an equation to change its LaTeX source.
        </p>
      </article>
      <article>
        <h3>Spreadsheets</h3>
        <p>
          Choose a cell, then type or edit its formula in the formula bar. Shift-select to work with
          a range. Right-click cells for row, column and comment actions; right-click sheet tabs to
          rename, duplicate or move sheets. A comment is attached to the stable row block.
        </p>
      </article>
      <article>
        <h3>Questionnaires</h3>
        <p>
          Choose an answer type, variable name and label. Add choices or create sections and
          repeats. Location & grouping moves questions into groups. Logic & validation controls
          expressions. Advanced XLSForm fields keep appearance settings, translations and dataset
          references.
        </p>
      </article>
      <article>
        <h3>Imports and exports</h3>
        <p>
          Review all conversion notes. Word and Excel features have different levels of support.
          Google Forms and research API imports create snapshots. Overleaf uses .tex and ZIP
          interchange. Keep the original when formatting or specialized behavior matters.
        </p>
      </article>
    </div>
  </section>
  <section>
    <h2>Version control in everyday language</h2>
    <p>
      You can use the workspace without learning Git commands. This mapping explains which ideas
      inspired the workflow. Dynodoc has a centralized block history; it is not a Git repository or
      an offline distributed editing system.
    </p>
    <div class="table-wrap">
      <table>
        <thead><tr><th>Version-control idea</th><th>In Dynodoc</th></tr></thead><tbody
          >{#each versionVocabulary as [term, label]}<tr><td>{term}</td><td>{label}</td></tr
            >{/each}</tbody
        >
      </table>
    </div>
  </section>
  <section>
    <h2>Access and recovery</h2>
    <p>
      An organization can contain teams; teams can contain folders. Inherited permissions and direct
      invitations are combined, with the strongest role applying. Only the owner or a workspace
      manager controls document sharing, publication and Trash. Removing direct access does not
      cancel an inherited grant.
    </p>
    <p>
      Trash preserves content, drafts, uploads and history. Restoring a document does not reactivate
      revoked public links. Administrators can disable accounts and restore them later; old sessions
      remain invalid. Permanent erasure requires a separately reviewed retention process.
    </p>
  </section>
  <section>
    <h2>Before fieldwork</h2>
    <p>
      The preview runs a documented subset of survey expressions. It is not an ODK Collect or Kobo
      runtime. Test repeats, media capture, device metadata, external datasets, translations and
      advanced logic in the actual destination platform, with the attachments you intend to use.
    </p>
    <p>
      <a href="https://docs.getodk.org/form-question-types/">ODK question types</a> ·
      <a href="https://developers.google.com/workspace/forms/api/guides/authorizing"
        >Google Forms authorization</a
      >
      ·
      <a href="https://docs.overleaf.com/managing-projects-and-files/downloading-a-project"
        >Overleaf project files</a
      >
    </p>
  </section>
  <a class="cta" href="https://app.dynodoc.online/workspace">Return to your workspace</a>
</ProductPage>
