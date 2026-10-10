<svelte:head
  ><title>Editor add-ins · Dynodoc</title><meta name="robots" content="noindex" /></svelte:head
>
<main>
  <a href="/projects">Dynodoc Projects</a>
  <h1>Connect your editor</h1>
  <p>
    Capture changes in your usual editor, push them for review, and pull team changes. These are
    test installations; store publication and acceptance in every native host remain pending.
  </p>
  <h2>Microsoft Office</h2>
  <p>
    Use the matching manifest in a compatible desktop Word, Excel or PowerPoint, or in Office on the
    web. Choose Home → Dynodoc → Open provenance after installation. Save your working file first.
  </p>
  <ul>
    {#each ['word', 'excel', 'powerpoint'] as host}<li>
        <a href={`/connectors/packages/${host}.xml`} download
          >{host === 'word' ? 'Word' : host === 'excel' ? 'Excel' : 'PowerPoint'} manifest</a
        >
      </li>{/each}
  </ul>
  <ul>
    <li>
      <a
        href="https://learn.microsoft.com/en-us/office/dev/add-ins/testing/create-a-network-shared-folder-catalog-for-task-pane-and-content-add-ins"
        target="_blank"
        rel="noreferrer">Windows desktop installation</a
      > — add the XML through a trusted shared-folder catalog.
    </li>
    <li>
      <a
        href="https://learn.microsoft.com/en-us/office/dev/add-ins/testing/sideload-an-office-add-in-on-mac"
        target="_blank"
        rel="noreferrer">Mac desktop installation</a
      > — copy the XML into the application's test add-in folder.
    </li>
    <li>
      <a
        href="https://learn.microsoft.com/en-us/office/dev/add-ins/testing/sideload-office-add-ins-for-testing"
        target="_blank"
        rel="noreferrer">Office on the web installation</a
      > — upload the XML in the add-ins dialog.
    </li>
  </ul>
  <p>
    The PowerPoint connector requires PowerPointApi 1.4, available in supporting Microsoft 365
    builds and Office 2024. PowerPoint 2021 cannot run this connector; use edited-file push in
    Projects. Word requires WordApi 1.3 and Excel requires ExcelApi 1.13.
  </p>
  <h2>Google Workspace</h2>
  <p>
    Unzip a package, copy its files into an Apps Script project, and create an Editor add-on test
    deployment for a test file. Each package includes installation instructions.
  </p>
  <ul>
    {#each ['docs', 'sheets', 'slides'] as host}<li>
        <a href={`/connectors/packages/google-${host}.zip`} download
          >Google {host[0].toUpperCase() + host.slice(1)} package</a
        >
      </li>{/each}
  </ul>
  <p>
    <a
      href="https://developers.google.com/workspace/add-ons/how-tos/testing-editor-addons"
      target="_blank"
      rel="noreferrer">Google test installation instructions</a
    >
  </p>
  <h2>Sign in from the editor</h2>
  <p>
    Select “Sign in to Dynodoc”, open the browser link, compare the code, choose the matching file
    and approve. Return to the pane and select “I approved this connection”. Connections expire in
    seven days and can be revoked in Sync &amp; connections.
  </p>
  <p>
    The current adapters support existing paragraph text, cells/formulas/date-only values and slide
    shape text. Structural and formatting changes require file push. Capture runs while the pane is
    open; enable it explicitly.
  </p>
  <p>
    Desktop files can stay on your local disk. Loading the pane, signing in and syncing need an
    internet connection. Captured changes have a local queue; complete offline sync and background
    capture are still being developed. Test files are limited to 3 MB each.
  </p>
</main>

<style>
  main {
    max-width: 48rem;
    margin: 3rem auto;
    padding: 1.5rem;
  }
  li {
    margin: 0.6rem 0;
  }
  h2 {
    margin-top: 2rem;
  }
</style>
