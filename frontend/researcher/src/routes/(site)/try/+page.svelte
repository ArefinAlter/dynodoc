<script lang="ts">
  import { env } from '$env/dynamic/public';
  import { Btn, Section, scrollable, spotlight } from '$lib/components/site';
  import { workspaceHref } from '$lib/components/site/nav';
  import { pageMeta } from '$lib/seo';
  import { ASSETS, RELEASE, SAMPLE_WORKSPACE_URL, size } from '$lib/site/release';

  const meta = pageMeta('/try');
  const title = meta?.title ?? 'Try the pre-alpha · Dynodoc';
  const description =
    meta?.description ??
    'Try Dynodoc 0.0.1a: a sample workspace of your own in the browser, or the desktop app for Windows and Linux.';
  const workspace = $derived(workspaceHref(env.PUBLIC_WORKSPACE_URL));
</script>

<svelte:head>
  <title>{title}</title>
  <meta name="description" content={description} />
  <meta property="og:title" content={title} />
  <meta property="og:description" content={description} />
  <meta name="twitter:title" content={title} />
  <meta name="twitter:description" content={description} />
</svelte:head>

<main id="main" use:scrollable>
  <header class="hero dotgrid dotgrid--fade spotlight" use:spotlight>
    <div class="container hero__copy">
      <span class="badge">Pre-alpha release · {RELEASE.tag}</span>
      <h1 class="h-1">Try the pre-alpha</h1>
      <p class="lede">
        Dynodoc 0.0.1a is the first public pre-alpha. It is early software: expect rough edges,
        missing pieces and changes. You can try it in your browser with a sample workspace of your
        own, sign in with your account, or install the desktop app for Windows or Linux.
      </p>
      <div class="actions">
        <Btn href={SAMPLE_WORKSPACE_URL} size="lg" arrow>Start a sample workspace</Btn>
        <Btn href="#download" variant="secondary" size="lg">Download the desktop app</Btn>
      </div>
    </div>
  </header>

  <Section
    id="browser"
    title="In your browser"
    lede="The hosted pre-alpha server makes you a workspace of your own, already filled with a sample organisation."
  >
    <div class="cols" data-reveal>
      <div class="card">
        <h3>A sample workspace</h3>
        <ul>
          <li>
            Three projects (a team handbook with its budget and review deck, a field guide and a
            grant application) with their history, drafts, change requests, reviews and issues.
          </li>
          <li>
            The colleagues in it are invented and use example.org addresses. Nothing is emailed.
          </li>
          <li>
            It is yours alone and is removed 24 hours after you start it. Do not put anything
            private or confidential in it.
          </li>
          <li>No account or address is asked for. Six workspaces per address per hour.</li>
        </ul>
        <Btn href={SAMPLE_WORKSPACE_URL} arrow>Start a sample workspace</Btn>
      </div>
      <div class="card">
        <h3>Your own account</h3>
        <p>
          Sign in with an emailed link or with Google and create your own projects. The hosted
          workspace is a test deployment with no paid plan; the
          <a href="/terms">terms</a> and <a href="/privacy">privacy policy</a> are drafts.
        </p>
        <Btn href={workspace} variant="secondary" arrow>Open the workspace</Btn>
      </div>
    </div>
  </Section>

  <Section
    id="download"
    title="Desktop app"
    lede="The desktop app opens on the client: change requests, history and drafts in the manner of GitHub Desktop. It connects to the hosted pre-alpha server by default, where it can start a sample workspace for you; Settings, Accounts lets you sign in or choose your own server. macOS is not built yet."
  >
    <div class="table-wrap" data-reveal>
      <table>
        <caption class="sr-only">Downloads of {RELEASE.name}</caption>
        <thead>
          <tr>
            <th scope="col">System</th>
            <th scope="col">File</th>
            <th scope="col">Size</th>
          </tr>
        </thead>
        <tbody>
          {#each ASSETS as asset (asset.file)}
            <tr>
              <th scope="row">{asset.platform}<span class="kind">{asset.kind}</span></th>
              <td><a href={asset.url}>{asset.file}</a><span class="note">{asset.note}</span></td>
              <td class="nowrap">{size(asset.bytes)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="links" data-reveal>
      <a href={RELEASE.sums}>Verify download (SHA-256)</a> ·
      <a href={RELEASE.page}>Release notes and all files</a>
    </p>
    <ul class="facts" data-reveal>
      <li>
        <strong>Unsigned.</strong> Neither build is signed yet. Windows SmartScreen will warn before
        it runs the installer; check the file against
        <a href={RELEASE.sums}>SHA256SUMS.txt</a> first.
      </li>
      <li>
        <strong>The bundled engine-cli</strong> creates this computer's signing key in the operating system's
        key store. The release also has engine-cli on its own for Windows and Linux.
      </li>
      <li>
        <strong>No auto-update, no telemetry.</strong> Newer releases appear on the
        <a href={RELEASE.page}>release page</a>, with notes on what works and what does not.
      </li>
    </ul>
  </Section>
</main>

<style>
  .hero {
    border-bottom: 1px solid var(--color-border);
  }
  .hero__copy {
    display: grid;
    gap: 22px;
    justify-items: start;
    padding-block: clamp(48px, 8vw, 104px) clamp(40px, 6vw, 80px);
  }
  .hero__copy .lede {
    max-width: 62ch;
  }
  .badge {
    display: inline-flex;
    padding: 2px 10px;
    border: 1px solid var(--color-border-strong);
    border-radius: 999px;
    color: var(--color-ink-soft);
    font-family: var(--site-font-mono);
    font-size: 0.75rem;
    letter-spacing: 0.04em;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
  .cols {
    display: grid;
    gap: 20px;
  }
  @media (min-width: 860px) {
    .cols {
      grid-template-columns: 1.4fr 1fr;
    }
  }
  .card {
    display: grid;
    gap: 14px;
    align-content: start;
    justify-items: start;
    padding: 24px;
    border: 1px solid var(--color-border);
    border-radius: var(--site-radius);
    background: var(--color-surface-raised);
  }
  .card h3 {
    margin: 0;
    font-size: 1.125rem;
  }
  .card ul {
    display: grid;
    gap: 8px;
    margin: 0;
    padding-left: 18px;
    color: var(--color-ink-soft);
  }
  .card p {
    margin: 0;
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
    min-width: 520px;
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
  .kind,
  .note {
    display: block;
    margin-top: 4px;
    color: var(--color-muted);
    font-size: 0.8125rem;
    font-weight: 400;
  }
  .nowrap {
    white-space: nowrap;
  }
  .links {
    margin: 16px 0 0;
  }
  .facts {
    display: grid;
    gap: 10px;
    margin: 20px 0 0;
    padding-left: 18px;
    color: var(--color-ink-soft);
  }
</style>
