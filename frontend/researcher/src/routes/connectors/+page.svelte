<script lang="ts">
  import { page } from '$app/state';
  // The hosted service serves the files built for its own address; a self-hosted installation
  // serves files made for its own, under /connectors/addins (docs/SELF-HOSTED-EDITION.md 7.2).
  const base = $derived(
    page.data.site?.mode === 'selfhosted' ? '/connectors/addins/' : '/connectors/packages/'
  );
  const office = [
    {
      host: 'word',
      name: 'Word',
      syncs: 'Edits to existing paragraph text.',
      needs: 'Word with WordApi 1.3 (desktop or on the web).'
    },
    {
      host: 'excel',
      name: 'Excel',
      syncs: 'Edits to existing cell values, dates and formulas.',
      needs: 'Excel with ExcelApi 1.13 (desktop or on the web).'
    },
    {
      host: 'powerpoint',
      name: 'PowerPoint',
      syncs: 'Edits to existing text in slide shapes.',
      needs: 'PowerPoint with PowerPointApi 1.4: current Microsoft 365 or Office 2024, not 2021.'
    }
  ];
</script>

<svelte:head
  ><title>Dynodoc add-ins · Dynodoc</title><meta name="robots" content="noindex" /></svelte:head
>
<main>
  <a href="/projects">Dynodoc Projects</a>
  <h1>Add Dynodoc to your editor</h1>
  <p>
    Keep working in Word, Excel, PowerPoint, Google Docs, Google Sheets or Google Slides. The add-in
    records edits to existing text and cell values while its pane is open, lets you send them for
    review in a Dynodoc project, and brings approved team changes back into your file.
  </p>
  <p class="status">
    Marketplace listings are not live yet. Until they are, install an add-in yourself with the files
    below. Each installs for your own account and can be removed at any time.
  </p>

  <h2>Microsoft Word, Excel and PowerPoint</h2>
  <p>
    Download the add-in file for your app and add it to Office from a shared folder (Windows), the
    add-in folder (Mac) or the add-ins dialog (Office on the web). Then choose Home → Dynodoc.
  </p>
  <ul class="cards">
    {#each office as app (app.host)}<li>
        <h3>{app.name}</h3>
        <p>{app.syncs}</p>
        <p class="needs">{app.needs}</p>
        <a href={`${base}${app.host}.xml`} download>Download the {app.name} add-in</a>
      </li>{/each}
  </ul>
  <ul>
    <li>
      <a
        href="https://learn.microsoft.com/en-us/office/dev/add-ins/testing/create-a-network-shared-folder-catalog-for-task-pane-and-content-add-ins"
        target="_blank"
        rel="noreferrer">Windows desktop installation</a
      > — add the file through a trusted shared-folder catalog.
    </li>
    <li>
      <a
        href="https://learn.microsoft.com/en-us/office/dev/add-ins/testing/sideload-an-office-add-in-on-mac"
        target="_blank"
        rel="noreferrer">Mac desktop installation</a
      > — copy the file into the application's add-in folder.
    </li>
    <li>
      <a
        href="https://learn.microsoft.com/en-us/office/dev/add-ins/testing/sideload-office-add-ins-for-testing"
        target="_blank"
        rel="noreferrer">Office on the web installation</a
      > — upload the file in the add-ins dialog.
    </li>
  </ul>

  <h2>Google Docs, Sheets and Slides</h2>
  <p>
    Each Google add-in is a small Apps Script project. Download the package, unzip it, add its files
    to a new Apps Script project and run an Editor add-on test deployment for your file. Each
    package includes its own installation instructions.
  </p>
  <ul class="cards">
    {#each [['docs', 'Docs', 'Edits to existing paragraph text in the current tab.'], ['sheets', 'Sheets', 'Edits to existing cell values and formulas.'], ['slides', 'Slides', 'Edits to existing text in slide shapes.']] as [host, name, syncs] (host)}<li
      >
        <h3>Google {name}</h3>
        <p>{syncs}</p>
        <a href={`${base}google-${host}.zip`} download>Download the {name} package</a>
      </li>{/each}
  </ul>
  <p>
    <a
      href="https://developers.google.com/workspace/add-ons/how-tos/testing-editor-addons"
      target="_blank"
      rel="noreferrer">Google's test-deployment instructions</a
    >
  </p>

  <h2>Connect a file</h2>
  <ol>
    <li>Open the add-in in your editor and choose <strong>Sign in to Dynodoc</strong>.</li>
    <li>
      In the browser window that opens, sign in, check that the code matches the one in your editor,
      pick the Dynodoc file and approve.
    </li>
    <li>Return to the editor and choose <strong>I approved this connection</strong>.</li>
    <li>Turn on capture when you are ready. Nothing is recorded before you do.</li>
  </ol>
  <p>
    A connection reaches one file, expires after seven days and can be revoked under Sync &amp;
    connections. No account password or token is given to the editor.
  </p>

  <h2>What to expect</h2>
  <ul>
    <li>
      The add-ins cover existing paragraph text, cell values, formulas and dates, and slide shape
      text. Formatting, new or moved content and other structural edits go through file push in
      Projects.
    </li>
    <li>
      Changes are recorded only while the add-in pane is open. There is no background capture.
    </li>
    <li>
      The pane loads from Dynodoc, so opening it and syncing need an internet connection. Recorded
      changes wait in a local queue until you send them.
    </li>
    <li>The original file you import into a project must be 3 MB or smaller.</li>
  </ul>
</main>

<style>
  main {
    max-width: 48rem;
    margin: 3rem auto;
    padding: 1.5rem;
    color: var(--color-ink);
    line-height: 1.55;
  }
  h2 {
    margin-top: 2rem;
  }
  li {
    margin: 0.6rem 0;
  }
  a {
    color: var(--color-accent);
  }
  .status {
    padding: 0.75rem 1rem;
    border: 1px solid var(--color-border);
    border-left: 4px solid var(--color-accent);
    border-radius: 6px;
    background: var(--color-surface);
  }
  .cards {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(13rem, 1fr));
    gap: 0.75rem;
    padding: 0;
    list-style: none;
  }
  .cards li {
    margin: 0;
    padding: 0.9rem 1rem;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-surface);
  }
  .cards h3 {
    margin: 0 0 0.35rem;
    font-size: 1.05rem;
  }
  .cards p {
    margin: 0.25rem 0;
  }
  .needs {
    color: var(--color-muted);
    font-size: 0.92rem;
  }
</style>
