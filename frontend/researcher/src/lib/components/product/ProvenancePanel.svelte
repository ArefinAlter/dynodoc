<script lang="ts">
  import { onMount } from 'svelte';
  import { workspaceRequest } from '$lib/workspace/api';
  import { downloadJson } from '$lib/workspace/projects';
  let {
    documentId,
    title,
    canPropose,
    onChanged
  }: { documentId: string; title: string; canPropose: boolean; onChanged: () => void } = $props();
  interface Grant {
    id: string;
    host: string;
    expires_at: string;
    revoked_at: string | null;
  }
  let grants = $state<Grant[]>([]),
    token = $state(''),
    host = $state('word'),
    error = $state(''),
    notice = $state(''),
    busy = $state(false);
  async function refresh() {
    grants = (await workspaceRequest<{ items: Grant[] }>(`/documents/${documentId}/connectors`))
      .items;
  }
  onMount(() => {
    void refresh().catch((e) => {
      error = String(e);
    });
  });
  async function run(action: () => Promise<void>) {
    if (busy) return;
    busy = true;
    error = '';
    notice = '';
    try {
      await action();
    } catch (e) {
      error = e instanceof Error ? e.message : 'The request failed';
    } finally {
      busy = false;
    }
  }
  async function connect() {
    token = '';
    const result = await workspaceRequest<{ token: string }>(
      `/documents/${documentId}/connectors`,
      { host }
    );
    token = result.token;
    await refresh();
  }
  async function upload(file?: File) {
    if (!file) return;
    if (file.size > 16 * 1024 * 1024) throw new Error('Use a change bundle smaller than 16 MB.');
    const input = JSON.parse(await file.text());
    const receipt = await workspaceRequest<{ id: string; bundle_id: string; submitted: boolean }>(
      `/documents/${documentId}/provenance`,
      input
    );
    notice = `${receipt.submitted ? 'Change request' : 'Private draft'} saved. Uploading this exact bundle again returns the same receipt.`;
    downloadJson(receipt, `${receipt.bundle_id}.receipt.json`);
    onChanged();
  }
</script>

<section>
  <h2>Sync {title}</h2>
  <p>
    Keep editing in Microsoft Word, Excel or PowerPoint, or Google Docs, Sheets or Slides. Changes
    arrive as a draft or a review request; merging records them in the team history.
  </p>
  {#if error}<p role="alert">{error}</p>{/if}{#if notice}<p role="status">{notice}</p>{/if}
  <button
    disabled={busy}
    onclick={() =>
      run(async () =>
        downloadJson(
          await workspaceRequest(`/documents/${documentId}/provenance`),
          `${documentId}.checkpoint.json`
        )
      )}>Download latest checkpoint</button
  >
  {#if canPropose}<label
      >Upload a portable change bundle<input
        type="file"
        accept=".json,application/json"
        disabled={busy}
        onchange={(e) => {
          const file = e.currentTarget.files?.[0];
          e.currentTarget.value = '';
          void run(() => upload(file));
        }}
      /></label
    >
    <h3>Connect an editor</h3>
    <p>
      Development connectors exchange existing paragraph text, spreadsheet cells/formulas and slide
      shape text. Installation and real-host acceptance are still required.
    </p>
    <label
      >Editor<select bind:value={host}
        ><option value="word">Microsoft Word</option><option value="google-docs">Google Docs</option
        ><option value="excel">Microsoft Excel</option><option value="google-sheets"
          >Google Sheets</option
        ><option value="powerpoint">Microsoft PowerPoint</option><option value="google-slides"
          >Google Slides</option
        ></select
      ></label
    >
    <button disabled={busy} onclick={() => run(connect)}>Create connection key</button>
    {#if token}<label
        >Copy this key into your connector<textarea readonly value={token}></textarea></label
      >
      <p>
        This key permits reading and proposing changes to this file for seven days. Keep it private.
        It is shown only once.
      </p>
      <button
        onclick={() => {
          token = '';
        }}>Hide key</button
      >{/if}
    <p>
      <a href="/connectors/word">Word task pane</a> ·
      <a href="/connectors/excel">Excel task pane</a> ·
      <a href="/connectors/powerpoint">PowerPoint task pane</a> ·
      <a href="https://github.com/ArefinAlter/dynodoc/tree/main/extensions"
        >Connector installation instructions</a
      >
    </p>
  {/if}
  <h3>Your connections</h3>
  {#if !grants.length}<p>No connections created.</p>{/if}
  {#each grants as g}<p>
      {g.host} · expires {new Date(g.expires_at).toLocaleString()}
      {#if g.revoked_at}· revoked{:else}<button
          disabled={busy}
          onclick={() =>
            run(async () => {
              await workspaceRequest(`/documents/${documentId}/connectors/${g.id}/revoke`, {});
              token = '';
              await refresh();
            })}>Revoke</button
        >{/if}
    </p>{/each}
</section>

<style>
  section {
    padding: 20px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    margin: 20px 0;
  }
  label {
    display: grid;
    gap: 8px;
    margin: 12px 0;
  }
  input,
  textarea,
  select,
  button {
    font: inherit;
    padding: 8px;
    background: var(--color-surface);
    color: inherit;
    border: 1px solid var(--color-border);
    border-radius: 5px;
  }
  button {
    cursor: pointer;
    margin: 4px;
  }
  textarea {
    min-height: 60px;
    overflow-wrap: anywhere;
  }
  a {
    color: var(--workspace-accent);
  }
</style>
