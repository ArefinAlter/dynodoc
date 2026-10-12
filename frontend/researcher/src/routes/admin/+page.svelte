<script lang="ts">
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import AdminRecords from '$lib/components/product/AdminRecords.svelte';
  import AdminNav, { type NavGroup } from '$lib/components/admin/AdminNav.svelte';
  import RelativeTime from '$lib/components/admin/RelativeTime.svelte';
  import TableScroll from '$lib/components/admin/TableScroll.svelte';
  import { formatBytes, serviceLabel, serviceOk, serviceState } from '$lib/components/admin/health';
  import '$lib/components/admin/admin.css';
  import { clearSession } from '$lib/stores/session.svelte';
  import { invalidateAll, goto } from '$app/navigation';
  import {
    CircleAlert,
    CircleCheck,
    Database,
    ExternalLink,
    FolderGit2,
    Gauge,
    HardDrive,
    LayoutDashboard,
    LogOut,
    RefreshCw,
    ScrollText,
    Server,
    ShieldCheck,
    TriangleAlert,
    Trash2,
    Users,
    FileText
  } from '@lucide/svelte';
  import type { PageData } from './$types';
  let { data }: { data: PageData } = $props();
  let busy = $state(false);
  let signOutError = $state('');
  let records = $state<ReturnType<typeof AdminRecords>>();
  let browsing = $state('documents');
  const counts = $derived(data.overview.counts);
  const services = $derived(data.host?.services ?? []);
  const tableBytes = $derived(
    Math.max(1, ...data.overview.tables.map((t: { bytes: number }) => Number(t.bytes)))
  );
  const running = $derived(services.filter((s) => s.running).length);
  const cards = $derived([
    { label: 'Accounts', value: counts.accounts, action: 'Browse accounts', kind: 'users' },
    { label: 'Documents', value: counts.documents, action: 'Browse documents', kind: 'documents' },
    // Only shown when the engine reports project totals.
    ...(counts.projects != null
      ? [{ label: 'Projects', value: counts.projects, action: 'Browse projects', kind: 'projects' }]
      : []),
    { label: 'Recorded changes', value: counts.events },
    { label: 'Saved versions', value: counts.versions }
  ] as { label: string; value: number; action?: string; kind?: string }[]);
  const nav = $derived<NavGroup[]>([
    {
      items: [
        {
          label: 'Overview',
          href: '#overview',
          section: 'overview',
          icon: LayoutDashboard,
          count: data.attention.length || null
        }
      ]
    },
    {
      items: [
        {
          label: 'People',
          href: '#records',
          section: 'records',
          icon: Users,
          count: counts.accounts,
          kinds: ['users']
        },
        {
          label: 'Projects & spaces',
          href: '#records',
          section: 'records',
          icon: FolderGit2,
          count: counts.projects ?? null,
          kinds: ['projects', 'spaces']
        },
        {
          label: 'Content',
          href: '#records',
          section: 'records',
          icon: FileText,
          count: counts.documents,
          kinds: ['documents']
        },
        {
          label: 'Audit log',
          href: '#records',
          section: 'records',
          icon: ScrollText,
          kinds: ['audit', 'erasures']
        }
      ]
    },
    {
      items: [
        { label: 'Storage & recovery', href: '#storage', section: 'storage', icon: HardDrive },
        { label: 'Services & health', href: '#services', section: 'services', icon: Server },
        { label: 'Traffic & performance', href: '#traffic', section: 'traffic', icon: Gauge },
        {
          label: 'Database',
          href: '#database',
          section: 'database',
          icon: Database,
          count: data.overview.tables.length
        }
      ]
    },
    {
      items: [
        {
          label: 'Danger zone',
          href: '#danger',
          section: 'danger',
          icon: TriangleAlert,
          tone: 'danger'
        }
      ]
    }
  ]);
  async function refresh() {
    busy = true;
    try {
      await invalidateAll();
    } finally {
      busy = false;
    }
  }
  /** Used by the navigation and the danger zone: choose what the records table lists. */
  function showRecords(kind: string) {
    records?.show(kind);
  }
  function jumpToRecords(kind: string) {
    showRecords(kind);
    document.getElementById('records-heading')?.focus();
  }
  async function signOut() {
    signOutError = '';
    try {
      await clearSession();
      await goto('/login');
    } catch {
      signOutError = 'Sign-out could not be completed. Please try again.';
    }
  }
</script>

<svelte:head
  ><title>Administration · Dynodoc</title><meta
    name="robots"
    content="noindex,nofollow"
  /></svelte:head
>
<div class="admin">
  <a class="skip" href="#admin-main">Skip to content</a>
  <header class="topbar">
    <a class="brand" href="https://dynodoc.online"
      ><img class="logo-day" src="/brand/logo.svg?v=2" width="128" height="28" alt="Dynodoc" /><img
        class="logo-night"
        src="/brand/logo-dark.svg?v=2"
        width="128"
        height="28"
        alt="Dynodoc"
      /></a
    ><span class="divider" aria-hidden="true">/</span><span class="app-name">Administration</span
    ><span class="pill" class:pill-accent={data.environment === 'Local'}>{data.environment}</span>
    <div class="top-actions">
      <a class="btn btn-sm btn-ghost" href="https://app.dynodoc.online/projects"
        >Workspace <ExternalLink size={14} aria-hidden="true" /></a
      ><ThemeToggle /><button type="button" class="btn btn-sm" onclick={signOut}
        ><LogOut size={14} aria-hidden="true" /> Sign out</button
      >
    </div>
  </header>
  {#if signOutError}<p class="flash flash-error sign-out-error" role="alert">
      <CircleAlert size={16} aria-hidden="true" />{signOutError}
    </p>{/if}
  <div class="shell">
    <AdminNav groups={nav} activeKind={browsing} onpick={showRecords} />
    <main id="admin-main">
      <div class="page-head">
        <div>
          <span class="pill pill-accent"
            ><ShieldCheck size={13} aria-hidden="true" /> Administrator access</span
          >
          <h1>Your workspace, under control.</h1>
          <p class="muted">
            Signed in as {data.overview.administrator}. Checked <RelativeTime
              value={data.overview.checked_at}
            />.
          </p>
        </div>
        <button type="button" class="btn" disabled={busy} onclick={refresh}
          ><RefreshCw size={16} class={busy ? 'spin' : ''} aria-hidden="true" />{busy
            ? 'Refreshing…'
            : 'Refresh'}</button
        >
      </div>

      <section id="overview" class="block" aria-labelledby="overview-heading">
        <h2 id="overview-heading" class="section-title">Overview</h2>
        <ul class="cards">
          {#each cards as card}
            <li class="card">
              <span class="card-label">{card.label}</span>
              <strong>{Number(card.value).toLocaleString('en')}</strong>
              {#if card.kind}{@const kind = card.kind}<button
                  type="button"
                  class="link"
                  onclick={() => jumpToRecords(kind)}>{card.action}</button
                >{/if}
            </li>
          {/each}
        </ul>
        <div class="split">
          <div class="box">
            <div class="box-header">
              <h3 class="box-title">Needs attention</h3>
              <span class="counter">{data.attention.length}</span>
            </div>
            {#if data.attention.length}
              <ul class="rows">
                {#each data.attention as item (item.id)}
                  <li class="box-row attention">
                    {#if item.level === 'danger'}<CircleAlert
                        class="level-danger"
                        size={18}
                        aria-hidden="true"
                      />{:else}<TriangleAlert
                        class="level-warning"
                        size={18}
                        aria-hidden="true"
                      />{/if}
                    <div>
                      <strong>{item.title}</strong>
                      <p class="muted">{item.detail}</p>
                    </div>
                    <a class="btn btn-sm" href={item.href}
                      >Review<span class="sr-only"> {item.title}</span></a
                    >
                  </li>
                {/each}
              </ul>
            {:else}
              <div class="box-row attention">
                <CircleCheck class="level-ok" size={18} aria-hidden="true" />
                <div>
                  <strong>Nothing needs attention</strong>
                  <p class="muted">
                    Checked the host report, services, disk space, backups and credential presence.
                    This does not test provider sign-in or a backup restore.
                  </p>
                </div>
              </div>
            {/if}
          </div>
          <div class="box">
            <div class="box-header"><h3 class="box-title">At a glance</h3></div>
            <dl class="kv">
              <div>
                <dt>Services running</dt>
                <dd>{data.host ? `${running} of ${services.length}` : 'Unavailable'}</dd>
              </div>
              <div>
                <dt>Latest local backup</dt>
                <dd><RelativeTime value={data.host?.backup?.created_at} /></dd>
              </div>
              <div>
                <dt>Server disk free</dt>
                <dd>{data.host ? formatBytes(data.host.disk_free_bytes) : 'Unavailable'}</dd>
              </div>
              <div>
                <dt>Database size</dt>
                <dd>{formatBytes(counts.database_bytes)}</dd>
              </div>
            </dl>
          </div>
        </div>
      </section>

      <div class="block"><AdminRecords bind:this={records} onkind={(k) => (browsing = k)} /></div>

      <section id="storage" class="block" aria-labelledby="storage-heading">
        <h2 id="storage-heading" class="section-title">Storage & recovery</h2>
        <div class="flash flash-warn">
          <TriangleAlert size={16} aria-hidden="true" />
          <p>
            Files are stored in PostgreSQL. There are no object-storage buckets in this deployment.
            Local backups share the VPS; off-server recovery is not configured.
          </p>
        </div>
        <div class="split">
          <div class="box">
            <div class="box-header"><h3 class="box-title">Database</h3></div>
            <dl class="kv">
              <div>
                <dt>PostgreSQL database</dt>
                <dd>{formatBytes(counts.database_bytes)}</dd>
              </div>
              <div>
                <dt>Original files</dt>
                <dd>
                  {Number(counts.uploads).toLocaleString('en')} · {formatBytes(counts.upload_bytes)}
                </dd>
              </div>
              <div>
                <dt>Active database connections</dt>
                <dd>{counts.connections}</dd>
              </div>
              <div>
                <dt>Applied migrations</dt>
                <dd>{counts.migrations}</dd>
              </div>
            </dl>
          </div>
          <div class="box">
            <div class="box-header"><h3 class="box-title">Server and backups</h3></div>
            <dl class="kv">
              <div>
                <dt>VPS disk free</dt>
                <dd>{data.host ? formatBytes(data.host.disk_free_bytes) : 'Unavailable'}</dd>
              </div>
              <div>
                <dt>Latest local backup</dt>
                <dd><RelativeTime value={data.host?.backup?.created_at} /></dd>
              </div>
              <div>
                <dt>Backup checksum</dt>
                <dd>
                  {#if data.host?.backup?.checksum_ok}<span class="pill pill-success">Verified</span
                    >{:else}<span class="pill pill-attention">Not verified</span>{/if}
                </dd>
              </div>
            </dl>
          </div>
        </div>
      </section>

      <section id="services" class="block" aria-labelledby="services-heading">
        <h2 id="services-heading" class="section-title">Services & health</h2>
        {#if data.stale}<div class="flash flash-warn">
            <TriangleAlert size={16} aria-hidden="true" />
            <p>
              Host monitoring is unavailable or older than three minutes. Refresh after checking the
              collector.
            </p>
          </div>{/if}
        <div class="split">
          <div class="box">
            <div class="box-header">
              <h3 class="box-title">Host services</h3>
              <span class="counter">{services.length}</span>
            </div>
            {#if services.length}
              <TableScroll label="Host services table">
                <table>
                  <thead
                    ><tr
                      ><th scope="col">Service</th><th scope="col">State</th><th scope="col"
                        >Health</th
                      ><th scope="col">Started</th></tr
                    ></thead
                  >
                  <tbody>
                    {#each services as service (service.name)}
                      <tr>
                        <td title={service.name}>{serviceLabel(service.name)}</td>
                        <td
                          ><span
                            class="pill"
                            class:pill-success={serviceOk(service)}
                            class:pill-danger={!serviceOk(service)}>{serviceState(service)}</span
                          ></td
                        >
                        <td>{service.health || '—'}</td>
                        <td><RelativeTime value={service.started_at} fallback="—" /></td>
                      </tr>
                    {/each}
                  </tbody>
                </table>
              </TableScroll>
            {:else}
              <p class="empty">No host report is available.</p>
            {/if}
          </div>
          <div class="box">
            <div class="box-header"><h3 class="box-title">Provider credentials</h3></div>
            <dl class="kv">
              <div>
                <dt>Google credentials</dt>
                <dd>
                  <span
                    class="pill"
                    class:pill-success={data.providers.google}
                    class:pill-attention={!data.providers.google}
                    >{data.providers.google ? 'Configured' : 'Missing'}</span
                  >
                </dd>
              </div>
              <div>
                <dt>Email credentials</dt>
                <dd>
                  <span
                    class="pill"
                    class:pill-success={data.providers.email}
                    class:pill-attention={!data.providers.email}
                    >{data.providers.email ? 'Configured' : 'Missing'}</span
                  >
                </dd>
              </div>
            </dl>
            <p class="note">Credential presence does not verify a successful provider sign-in.</p>
          </div>
        </div>
      </section>

      <section id="traffic" class="block" aria-labelledby="traffic-heading">
        <h2 id="traffic-heading" class="section-title">
          Traffic & performance <span class="pill">Last 30 days</span>
        </h2>
        <p class="muted lead">
          Anonymous page loads, not unique people. No IP addresses, account IDs, document IDs,
          queries or document content are collected. Browser-reported samples may exclude blocked or
          unsupported clients.
        </p>
        <div class="split">
          <div class="box">
            <div class="box-header"><h3 class="box-title">Page loads</h3></div>
            {#if !data.overview.traffic.length}<p class="empty">No visits recorded yet.</p>{:else}
              <TableScroll label="Page loads table">
                <table>
                  <thead
                    ><tr><th scope="col">Site / page</th><th scope="col" class="num">Loads</th></tr
                    ></thead
                  >
                  <tbody>
                    {#each data.overview.traffic as row}<tr
                        ><td>{row.host} / {row.page}</td><td class="num">{row.visits}</td></tr
                      >{/each}
                  </tbody>
                </table>
              </TableScroll>
            {/if}
          </div>
          <div class="box">
            <div class="box-header"><h3 class="box-title">75th percentile</h3></div>
            {#if !data.overview.vitals.length}<p class="empty">
                Performance samples will appear after real browser visits.
              </p>{:else}
              <TableScroll label="Web vitals table">
                <table>
                  <thead
                    ><tr
                      ><th scope="col">Site / metric</th><th scope="col" class="num">p75</th><th
                        scope="col"
                        class="num">Samples</th
                      ></tr
                    ></thead
                  >
                  <tbody>
                    {#each data.overview.vitals as row}<tr
                        ><td>{row.host} / {row.metric}</td><td class="num"
                          >{Number(row.p75).toFixed(row.metric === 'CLS' ? 3 : 0)}{row.metric ===
                          'CLS'
                            ? ''
                            : ' ms'}</td
                        ><td class="num">{row.samples}</td></tr
                      >{/each}
                  </tbody>
                </table>
              </TableScroll>
            {/if}
          </div>
        </div>
      </section>

      <section id="database" class="block" aria-labelledby="database-heading">
        <h2 id="database-heading" class="section-title">Database</h2>
        <p class="muted lead">
          Storage and approximate row counts from PostgreSQL statistics. Document content is not
          exposed in this dashboard.
        </p>
        <div class="box">
          <TableScroll label="Database tables">
            <table>
              <thead
                ><tr
                  ><th scope="col">Table</th><th scope="col" class="num">Estimated rows</th><th
                    scope="col"
                    class="num">Total storage</th
                  ><th scope="col">Relative size</th></tr
                ></thead
              >
              <tbody>
                {#each data.overview.tables as table (table.name)}<tr
                    ><td>{table.name}</td><td class="num">{table.estimated_rows}</td><td class="num"
                      >{formatBytes(table.bytes)}</td
                    ><td class="share"
                      ><span
                        class="bar"
                        aria-hidden="true"
                        style:width="{Math.max(2, (Number(table.bytes) / tableBytes) * 100)}%"
                      ></span></td
                    ></tr
                  >{/each}
              </tbody>
            </table>
          </TableScroll>
        </div>
      </section>

      <section id="danger" class="block" aria-labelledby="danger-heading">
        <h2 id="danger-heading" class="section-title danger-title">Danger zone</h2>
        <div class="danger-box">
          <div class="danger-row">
            <div>
              <h3>Move to Trash or disable</h3>
              <p class="muted">
                Reversible. Trash keeps content and history and revokes public links. Disabling an
                account blocks its sessions and published links; restoring requires a fresh sign-in.
              </p>
            </div>
            <div class="danger-actions">
              <button
                type="button"
                class="btn btn-danger"
                onclick={() => jumpToRecords('documents')}>Find documents</button
              ><button type="button" class="btn btn-danger" onclick={() => jumpToRecords('users')}
                >Find accounts</button
              >
            </div>
          </div>
          <div class="danger-row">
            <div>
              <h3>Transfer ownership</h3>
              <p class="muted">
                Give an active account ownership of a document or a workspace. Existing
                collaborators keep their access.
              </p>
            </div>
            <div class="danger-actions">
              <button type="button" class="btn btn-danger" onclick={() => jumpToRecords('spaces')}
                >Review workspaces</button
              >
            </div>
          </div>
          <div class="danger-row">
            <div>
              <h3>Permanently delete</h3>
              <p class="muted">
                Removes a document, with its history, files, versions, drafts and public copies, or
                an account profile and credentials, from the live database. This cannot be undone. A
                minimal deletion receipt remains, and existing backups may retain the data until
                they expire.
              </p>
            </div>
            <div class="danger-actions">
              <button
                type="button"
                class="btn btn-danger-solid"
                onclick={() => jumpToRecords('documents')}
                ><Trash2 size={15} aria-hidden="true" /> Choose documents to delete</button
              >
            </div>
          </div>
        </div>
      </section>
    </main>
  </div>
  <footer class="foot">
    <a href="https://app.dynodoc.online/source">Download source code</a>
  </footer>
</div>

<style>
  .skip {
    position: absolute;
    left: 8px;
    top: -48px;
    z-index: 20;
    padding: 8px 12px;
    border-radius: 6px;
    background: var(--a-canvas);
    border: 1px solid var(--a-accent);
  }
  .skip:focus {
    top: 8px;
  }
  .topbar {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 56px;
    padding: 8px 32px;
    background: var(--a-subtle);
    border-bottom: 1px solid var(--a-border);
  }
  .brand {
    display: inline-flex;
  }
  .brand img {
    height: 28px;
    width: auto;
  }
  .divider {
    color: var(--a-border);
    font-size: 20px;
  }
  .app-name {
    font-weight: 600;
  }
  .top-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .sign-out-error {
    margin: 16px 32px 0;
  }
  .shell {
    display: grid;
    grid-template-columns: 256px minmax(0, 1fr);
    gap: 32px;
    max-width: 1344px;
    margin: 0 auto;
    padding: 24px 32px 96px;
  }
  main {
    min-width: 0;
  }
  .page-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 24px;
  }
  .page-head h1 {
    margin: 8px 0 4px;
    font-size: 28px;
    font-weight: 600;
    line-height: 1.25;
  }
  .block {
    margin-bottom: 40px;
    scroll-margin-top: 16px;
  }
  .block > .section-title {
    padding-bottom: 8px;
    margin-bottom: 16px;
    border-bottom: 1px solid var(--a-border-muted);
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .block > .section-title .pill {
    margin-left: auto;
    font-size: 12px;
    font-weight: 500;
  }
  .lead {
    margin: -4px 0 16px;
    max-width: 760px;
  }
  .block > .flash {
    margin-bottom: 16px;
  }
  .cards {
    list-style: none;
    margin: 0 0 16px;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
    gap: 16px;
  }
  .card {
    display: grid;
    align-content: start;
    gap: 4px;
    padding: 16px;
    border: 1px solid var(--a-border);
    border-radius: 6px;
    background: var(--a-canvas);
    transition:
      border-color 0.15s ease,
      transform 0.15s ease;
  }
  .card:hover {
    border-color: var(--a-muted);
    transform: translateY(-1px);
  }
  .card-label {
    color: var(--a-muted);
    font-size: 12px;
    font-weight: 600;
  }
  .card strong {
    font-size: 28px;
    font-weight: 600;
    line-height: 1.2;
    font-variant-numeric: tabular-nums;
  }
  .link {
    justify-self: start;
    padding: 2px 0;
    border: 0;
    background: none;
    color: var(--a-accent);
    font: inherit;
    cursor: pointer;
  }
  .link:hover {
    text-decoration: underline;
  }
  .split {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 16px;
    align-items: start;
  }
  .block > .cards + .split {
    grid-template-columns: minmax(0, 3fr) minmax(0, 2fr);
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .attention {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 12px;
    align-items: start;
  }
  .attention p {
    margin-top: 2px;
  }
  .attention :global(.level-danger) {
    color: var(--a-danger);
    margin-top: 1px;
  }
  .attention :global(.level-warning) {
    color: var(--a-attention);
    margin-top: 1px;
  }
  .attention :global(.level-ok) {
    color: var(--a-success);
    margin-top: 1px;
  }
  .empty,
  .note {
    padding: 12px 16px;
    color: var(--a-muted);
  }
  .note {
    border-top: 1px solid var(--a-border-muted);
    font-size: 12px;
  }
  .share {
    width: 22%;
    min-width: 96px;
  }
  .bar {
    display: block;
    height: 6px;
    border-radius: 3px;
    background: var(--a-accent);
    opacity: 0.55;
  }
  .danger-title {
    color: var(--a-danger);
  }
  .danger-box {
    border: 1px solid var(--a-danger);
    border-radius: 6px;
    overflow: hidden;
  }
  .danger-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    padding: 16px;
    border-top: 1px solid var(--a-danger-border);
  }
  .danger-row:first-child {
    border-top: 0;
  }
  .danger-row h3 {
    margin: 0 0 4px;
    font-size: 14px;
    font-weight: 600;
  }
  .danger-row p {
    max-width: 640px;
  }
  .danger-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    justify-content: flex-end;
  }
  .foot {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 12px 24px;
    max-width: 1344px;
    margin: 0 auto;
    padding: 16px 32px 32px;
    border-top: 1px solid var(--a-border-muted);
    font-size: 12px;
  }
  :global(.admin .spin) {
    animation: admin-spin 0.9s linear infinite;
  }
  @keyframes admin-spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    :global(.admin .spin) {
      animation: none;
    }
    .card:hover {
      transform: none;
    }
  }
  /* Wordmark with light lettering for night mode (printing keeps the day logo). */
  .brand .logo-night {
    display: none;
  }
  @media screen {
    :global([data-theme='dark']) .brand .logo-day {
      display: none;
    }
    :global([data-theme='dark']) .brand .logo-night {
      display: block;
    }
  }
  @media (max-width: 860px) {
    .shell {
      grid-template-columns: minmax(0, 1fr);
      gap: 16px;
      padding: 16px 16px 96px;
    }
    .topbar {
      padding: 8px 16px;
      flex-wrap: wrap;
    }
    .divider {
      display: none;
    }
    .split,
    .block > .cards + .split {
      grid-template-columns: minmax(0, 1fr);
    }
    .sign-out-error {
      margin: 16px 16px 0;
    }
    .foot {
      padding: 16px 16px 32px;
    }
    .danger-row {
      flex-direction: column;
      align-items: flex-start;
    }
    .danger-actions {
      justify-content: flex-start;
    }
  }
</style>
