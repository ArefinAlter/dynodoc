<script lang="ts">
  let { data } = $props();
  const forLabel = (file: { group: string; host: string; variant: string }) => {
    const names: Record<string, string> = {
      word: 'Word',
      excel: 'Excel',
      powerpoint: 'PowerPoint',
      'google-docs': 'Google Docs',
      'google-sheets': 'Google Sheets',
      'google-slides': 'Google Slides'
    };
    if (file.group === 'checksums') return 'All the files above';
    const app = names[file.host] ?? file.host;
    return file.variant === 'catalog' ? `${app} (catalog-compatible, no ribbon button)` : app;
  };
  const officeFiles = $derived(data.files.filter((file) => file.group === 'office'));
  const otherFiles = $derived(data.files.filter((file) => file.group !== 'office'));
</script>

<svelte:head>
  <title>Add-in files · {data.brand}</title>
  <meta name="robots" content="noindex" />
</svelte:head>

<main>
  <a href="/projects">Back to projects</a>
  <h1>Add-in files for {data.brand}</h1>
  <p>
    These files were made for <code>{data.origin}</code>. Each one names this address and no other.
    The downloads are open to anyone who has the link, so you can send a link to the people who need
    one; this page, with the instructions, is for administrators.
  </p>
  <p class="status">
    <strong>Not yet tried in an installed Office or Google editor.</strong> The files are checked by automated
    tests only. Before you roll them out, try each one on a disposable file and record the result in the
    acceptance worksheet (extensions/ACCEPTANCE.md in the source release).
  </p>

  {#if data.problem}
    <p class="problem" role="alert">
      The add-in files cannot be made yet. {data.problem}
    </p>
  {:else}
    <h2 id="office-files">Office files</h2>
    <p>
      Version <code>{data.manifestVersion}</code> follows the version of this program, so installing
      a newer {data.brand} gives newer manifests that Office will use to replace a cached copy. The add-in
      ids stay the same for as long as the address stays the same. The support link in the manifests is
      <code>{data.supportUrl}</code>.
    </p>
    <table aria-labelledby="office-files">
      <thead>
        <tr><th>File</th><th>For</th><th>Add-in id</th><th>SHA-256 (start)</th></tr>
      </thead>
      <tbody>
        {#each officeFiles as file (file.name)}
          <tr>
            <td><a href={`/connectors/addins/${file.name}`} download>{file.name}</a></td>
            <td>{forLabel(file)}</td>
            <td><code>{file.id}</code></td>
            <td><code>{file.sha256.slice(0, 12)}</code></td>
          </tr>
        {/each}
      </tbody>
    </table>

    <h2 id="other-files">Google packages and checksums</h2>
    {#if data.google.available}
      <table aria-labelledby="other-files">
        <thead><tr><th>File</th><th>For</th><th>Size</th><th>SHA-256 (start)</th></tr></thead>
        <tbody>
          {#each otherFiles as file (file.name)}
            <tr>
              <td><a href={`/connectors/addins/${file.name}`} download>{file.name}</a></td>
              <td>{forLabel(file)}</td>
              <td>{Math.round(file.size / 1024)} KB</td>
              <td><code>{file.sha256.slice(0, 12)}</code></td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p>
        No Google packages were made. {data.google.reason}
      </p>
      <p>
        <a href="/connectors/addins/SHA256SUMS" download>SHA256SUMS</a> lists the checksum of every
        file above. Check a download with <code>sha256sum -c SHA256SUMS</code>.
      </p>
    {/if}
  {/if}

  <h2>Install on Office</h2>
  <p>
    Checked against Microsoft's documentation on 14 October 2026; their rules change, so read their
    current page before you rely on a row.
  </p>
  <table aria-label="Ways to install the Office add-ins">
    <thead>
      <tr><th>Way to install</th><th>Use</th><th>What it needs</th><th>What to know</th></tr>
    </thead>
    <tbody>
      <tr>
        <td>Microsoft 365 admin center, Integrated apps, upload a custom app</td>
        <td>The standard manifests</td>
        <td>A Microsoft 365 tenant with Entra ID, and an administrator role</td>
        <td>
          It can take up to 24 hours to appear. People in nested groups are not assigned. A later
          manifest that asks for more permission needs the administrator to agree again. The pane
          address must be reachable from the people's computers, not from Microsoft. This is not for
          organisations without a Microsoft 365 tenant.
        </td>
      </tr>
      <tr>
        <td>Centralized Deployment</td>
        <td>The standard manifests</td>
        <td>The same as above</td>
        <td>
          Microsoft documents it as the way where the Integrated apps page is not available, such as
          government clouds.
        </td>
      </tr>
      <tr>
        <td>SharePoint app catalog with a trusted add-in catalog (group policy)</td>
        <td>The catalog-compatible manifests</td>
        <td>An app catalog site, and the Trusted Catalogs group policy or per-user setting</td>
        <td>
          These catalogs do not support the ribbon part of a manifest, so there is no {data.brand} button.
          People open the pane from Home, Add-ins, More Settings, My Organization. Windows only, not Mac.
        </td>
      </tr>
      <tr>
        <td>Network shared folder</td>
        <td>The standard manifests</td>
        <td>A shared folder and its path in the Trust Center of each computer</td>
        <td>
          Microsoft documents it for development and testing and does not support it for production
          add-ins. Windows only. Use it at your own risk.
        </td>
      </tr>
      <tr>
        <td>One person adds it themselves</td>
        <td>The standard manifest</td>
        <td>Office on the web (upload my add-in), or the wef folder on a Mac</td>
        <td>Per person. It cannot be managed or removed centrally.</td>
      </tr>
      <tr>
        <td>Microsoft AppSource</td>
        <td>None</td>
        <td>-</td>
        <td>
          Not available for an installation like this one: a store listing fixes one address, and
          this address is yours.
        </td>
      </tr>
    </tbody>
  </table>
  <p>
    Pages load over HTTPS, so the certificate of <code>{data.origin}</code> must be one that each computer
    trusts: the system store for desktop Office, the browser's store for Office on the web. If your own
    certificate authority issued it, install that authority's certificate on the computers by the usual
    means (group policy or device management). A self-signed certificate does not work.
  </p>
  <p>
    Word needs WordApi 1.3, Excel needs ExcelApi 1.13 and PowerPoint needs PowerPointApi 1.4.
    PowerPoint 2021 does not have it; use a current Microsoft 365 build or Office 2024.
  </p>

  <h2>Where Office.js comes from</h2>
  {#if data.officeJsLocal}
    <p>
      This installation serves Office.js from its own server, from the folder set in
      <code>DYNODOC_OFFICE_JS_DIR</code>. The panes do not contact Microsoft's servers for it. The
      small command page (<code>/connectors/commands.html</code>) still asks Microsoft's address for
      Office.js; it defines no ribbon command, and an installation where that request is blocked has
      not been tried.
    </p>
  {:else}
    <p>
      Office loads Office.js for each pane from Microsoft's servers (appsforoffice.microsoft.com).
      On a network that blocks that address, the panes cannot start. To avoid that, put a copy of
      the Office.js files in a folder on this server and set <code>DYNODOC_OFFICE_JS_DIR</code> to
      it.
      {data.brand} does not ship Office.js, because the terms for passing it on have not been reviewed;
      the deployment guide says how to get and stage a copy.
    </p>
  {/if}

  <h2>What does not work without internet access, or on a private network</h2>
  <ul>
    <li>
      <strong>The Google Docs, Sheets and Slides add-ons.</strong> Google's servers make their requests
      to this installation, so the address must be reachable from the public internet, and the editors
      themselves need Google. Google keeps the add-on's saved queue and file key in its own storage, outside
      this installation.
    </li>
    <li><strong>Office on the web.</strong> It runs in Microsoft's cloud.</li>
    <li>
      <strong>Installing from Microsoft AppSource</strong>, or deploying from the Microsoft 365
      admin center. Both go through a Microsoft cloud tenant. Desktop Office still works through a
      SharePoint catalog, group policy or a shared folder.
    </li>
    <li>
      <strong>Office.js from Microsoft's servers.</strong> With no route to them, use a local copy as
      described above.
    </li>
  </ul>
</main>

<style>
  main {
    max-width: 62rem;
    margin: 3rem auto;
    padding: 1.5rem;
    color: var(--color-ink);
    line-height: 1.55;
  }
  h2 {
    margin-top: 2rem;
  }
  a {
    color: var(--color-accent);
  }
  li {
    margin: 0.6rem 0;
  }
  code {
    overflow-wrap: anywhere;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.95rem;
  }
  th,
  td {
    padding: 0.5rem 0.65rem;
    border: 1px solid var(--color-border);
    text-align: left;
    vertical-align: top;
  }
  th {
    background: var(--color-surface);
  }
  .status,
  .problem {
    padding: 0.75rem 1rem;
    border: 1px solid var(--color-border);
    border-left: 4px solid var(--color-accent);
    border-radius: 6px;
    background: var(--color-surface);
  }
  .problem {
    border-left-color: var(--color-danger, #b3261e);
  }
  @media (max-width: 40rem) {
    main {
      margin: 1rem auto;
      padding: 1rem;
    }
    table {
      display: block;
      overflow-x: auto;
    }
  }
</style>
