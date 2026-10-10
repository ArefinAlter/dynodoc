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
    Sideload the matching manifest, then choose Home → Dynodoc → Open provenance. Save your working
    file first.
  </p>
  <ul>
    {#each ['word', 'excel', 'powerpoint'] as host}<li>
        <a href={`/connectors/packages/${host}.xml`} download
          >{host === 'word' ? 'Word' : host === 'excel' ? 'Excel' : 'PowerPoint'} manifest</a
        >
      </li>{/each}
  </ul>
  <p>
    <a
      href="https://learn.microsoft.com/en-us/office/dev/add-ins/testing/sideload-office-add-ins-for-testing"
      target="_blank"
      rel="noreferrer">Microsoft installation instructions</a
    >
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
