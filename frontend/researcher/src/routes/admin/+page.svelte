<script lang="ts">
  import { clearSession } from '$lib/stores/session.svelte';
  import { invalidateAll, goto } from '$app/navigation';
  import {
    Activity,
    Database,
    HardDrive,
    ShieldCheck,
    RefreshCw,
    ExternalLink
  } from '@lucide/svelte';
  import type { PageData } from './$types';
  let { data }: { data: PageData } = $props();
  let busy = $state(false);
  const bytes = (n: number) =>
    new Intl.NumberFormat('en', { maximumFractionDigits: 1 }).format(n / 1024 / 1024) + ' MiB';
  const stale = $derived(!data.host || Date.now() - Date.parse(data.host.checked_at) > 180_000);
  async function refresh() {
    busy = true;
    try {
      await invalidateAll();
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head
  ><title>Operations · Dynodoc</title><meta name="robots" content="noindex,nofollow" /></svelte:head
>
<div class="ops">
  <header>
    <a href="https://dynodoc.online"
      ><img src="/brand/logo.svg" width="165" height="36" alt="Dynodoc" /></a
    ><span>Operations</span><a href="https://app.dynodoc.online/workspace"
      >Workspace <ExternalLink size={14} /></a
    >
  </header>
  <main>
    <div class="heading">
      <div>
        <p class="eyebrow"><ShieldCheck size={15} /> ADMINISTRATOR ACCESS</p>
        <h1>A clear view of your workspace.</h1>
        <p>
          Signed in as {data.overview.administrator}. Checked {new Date(
            data.overview.checked_at
          ).toLocaleString()}.
        </p>
      </div>
      <button disabled={busy} onclick={refresh}
        ><RefreshCw size={16} />{busy ? 'Refreshing…' : 'Refresh'}</button
      >
    </div>
    <section class="stats" aria-label="Workspace totals">
      {#each [['Accounts', data.overview.counts.accounts], ['Documents', data.overview.counts.documents], ['Recorded changes', data.overview.counts.events], ['Saved versions', data.overview.counts.versions]] as [label, value]}<article
        >
          <span>{label}</span><strong>{Number(value).toLocaleString()}</strong>
        </article>{/each}
    </section>
    <div class="columns">
      <section class="panel">
        <h2><Activity size={20} /> Services</h2>
        {#if stale}<p class="notice">
            Host monitoring is unavailable or older than three minutes. Refresh after checking the
            collector.
          </p>{/if}
        {#each data.host?.services || [] as service}<div class="line">
            <span>{service.name}</span><b
              class:healthy={service.running && service.health !== 'unhealthy'}
              >{service.health || (service.running ? 'Running' : 'Stopped')}</b
            >
          </div>{/each}
        <div class="line">
          <span>Google credentials</span><b>{data.providers.google ? 'Configured' : 'Missing'}</b>
        </div>
        <div class="line">
          <span>Email credentials</span><b>{data.providers.email ? 'Configured' : 'Missing'}</b>
        </div>
        <p class="detail">Credential presence does not verify a successful provider sign-in.</p>
      </section>
      <section class="panel">
        <h2><HardDrive size={20} /> Storage & recovery</h2>
        <dl>
          <dt>PostgreSQL database</dt>
          <dd>{bytes(data.overview.counts.database_bytes)}</dd>
          <dt>Original files</dt>
          <dd>{data.overview.counts.uploads} · {bytes(data.overview.counts.upload_bytes)}</dd>
          <dt>Active database connections</dt>
          <dd>{data.overview.counts.connections}</dd>
          <dt>Applied migrations</dt>
          <dd>{data.overview.counts.migrations}</dd>
          <dt>VPS disk free</dt>
          <dd>{data.host ? bytes(data.host.disk_free_bytes) : 'Unavailable'}</dd>
          <dt>Latest local backup</dt>
          <dd>
            {data.host?.backup?.created_at
              ? new Date(data.host.backup.created_at).toLocaleString()
              : 'Unavailable'}
          </dd>
          <dt>Backup checksum</dt>
          <dd>{data.host?.backup?.checksum_ok ? 'Verified' : 'Not verified'}</dd>
        </dl>
        <p class="detail">
          Files are stored in PostgreSQL. There are no object-storage buckets in this deployment.
          Local backups share the VPS; off-server recovery is not configured.
        </p>
      </section>
    </div>
    <section class="panel">
      <h2><Activity size={20} /> Visits & web vitals <span>Last 30 days</span></h2>
      <p class="detail">
        Anonymous page loads, not unique people. No IP addresses, account IDs, document IDs, queries
        or research content are collected. Browser-reported samples may exclude blocked or
        unsupported clients.
      </p>
      <div class="columns">
        <div>
          <h3>Page loads</h3>
          {#if !data.overview.traffic.length}<p>No visits recorded yet.</p>{:else}<table>
              <thead><tr><th>Site / page</th><th>Loads</th></tr></thead><tbody
                >{#each data.overview.traffic as row}<tr
                    ><td>{row.host} / {row.page}</td><td>{row.visits}</td></tr
                  >{/each}</tbody
              >
            </table>{/if}
        </div>
        <div>
          <h3>75th percentile</h3>
          {#if !data.overview.vitals.length}<p>
              Performance samples will appear after real browser visits.
            </p>{:else}<table>
              <thead><tr><th>Site / metric</th><th>p75</th><th>Samples</th></tr></thead><tbody
                >{#each data.overview.vitals as row}<tr
                    ><td>{row.host} / {row.metric}</td><td
                      >{Number(row.p75).toFixed(row.metric === 'CLS' ? 3 : 0)}{row.metric === 'CLS'
                        ? ''
                        : ' ms'}</td
                    ><td>{row.samples}</td></tr
                  >{/each}</tbody
              >
            </table>{/if}
        </div>
      </div>
    </section>
    <section class="panel">
      <h2><Database size={20} /> Database tables</h2>
      <p class="detail">
        Storage and approximate row counts from PostgreSQL statistics. Research records are not
        exposed in this dashboard.
      </p>
      <div class="scroll">
        <table>
          <thead><tr><th>Table</th><th>Estimated rows</th><th>Total storage</th></tr></thead><tbody
            >{#each data.overview.tables as table}<tr
                ><td>{table.name}</td><td>{table.estimated_rows}</td><td>{bytes(table.bytes)}</td
                ></tr
              >{/each}</tbody
          >
        </table>
      </div>
    </section>
    <footer class="detail">
      <a href="https://app.dynodoc.online/source">Download source code</a>
      <button
        onclick={async () => {
          await clearSession();
          await goto('/login');
        }}>Sign out</button
      >
    </footer>
  </main>
</div>

<style>
  footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 20px;
    margin-top: 24px;
  }
  footer a {
    color: #2059b0;
  }
  .ops {
    min-height: 100vh;
    background: #faf9f6;
    color: #20374f;
    font-family: var(--font-ui);
  }
  header {
    display: flex;
    align-items: center;
    gap: 22px;
    padding: 24px 5%;
    border-bottom: 1px solid #dce4e9;
  }
  header > a:last-child {
    margin-left: auto;
    display: flex;
    gap: 8px;
    color: #2059b0;
    text-decoration: none;
  }
  header > span {
    border-left: 1px solid #ccd7e0;
    padding-left: 22px;
  }
  main {
    max-width: 1280px;
    margin: auto;
    padding: 42px 30px;
  }
  .heading {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 24px;
    margin-bottom: 32px;
  }
  h1 {
    font-size: 34px;
    letter-spacing: -1px;
    margin: 12px 0;
  }
  .heading p,
  .detail {
    color: #55697b;
    font-size: 13px;
    line-height: 1.7;
  }
  .eyebrow {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 11px !important;
    letter-spacing: 1px;
  }
  button {
    display: flex;
    gap: 8px;
    align-items: center;
    background: #e4eefb;
    color: #184c96;
    padding: 11px 16px;
    border: 1px solid #c9dcf2;
    border-radius: 6px;
    cursor: pointer;
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 16px;
    margin-bottom: 24px;
  }
  .stats article,
  .panel {
    border: 1px solid #dce4e9;
    background: #fffefb;
    border-radius: 10px;
    padding: 24px;
  }
  .stats span {
    font-size: 13px;
    color: #55697b;
  }
  .stats strong {
    display: block;
    font-size: 34px;
    margin-top: 12px;
    font-weight: 600;
  }
  .columns {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 24px;
  }
  .panel {
    margin-bottom: 24px;
    min-width: 0;
  }
  h2 {
    display: flex;
    gap: 10px;
    align-items: center;
    font-size: 18px;
    margin: 0 0 20px;
  }
  h2 > span {
    font-size: 12px;
    font-weight: 400;
    margin-left: auto;
  }
  h3 {
    font-size: 14px;
  }
  .line {
    display: flex;
    justify-content: space-between;
    gap: 20px;
    padding: 12px 0;
    border-bottom: 1px solid #e7ecf0;
    font-size: 13px;
  }
  .line b {
    font-weight: 500;
  }
  .healthy {
    color: #217346;
  }
  dl {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 13px;
    font-size: 13px;
  }
  dd {
    margin: 0;
    text-align: right;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
    text-align: left;
  }
  th,
  td {
    padding: 12px;
    border-bottom: 1px solid #e7ecf0;
  }
  th {
    font-weight: 600;
    background: #edf4fc;
  }
  td:last-child,
  th:last-child {
    text-align: right;
  }
  .notice {
    background: #fff2cf;
    padding: 14px;
    line-height: 1.6;
    color: #71551a;
  }
  .scroll {
    overflow: auto;
  }
  @media (max-width: 750px) {
    main {
      padding: 24px 16px;
    }
    .columns {
      grid-template-columns: 1fr;
    }
    .stats {
      grid-template-columns: 1fr 1fr;
    }
    .heading {
      align-items: start;
      flex-direction: column;
    }
    h1 {
      font-size: 29px;
    }
    header {
      gap: 12px;
    }
    header > span {
      display: none;
    }
    .panel {
      padding: 18px;
    }
  }
</style>
