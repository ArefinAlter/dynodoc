<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';
  import { workspaceRequest, documentKind } from '$lib/workspace/api';
  import type { Document } from '$lib/api/types';
  import { HOST_LABELS } from '$lib/provenance/pairing';
  import type { Host } from '$lib/provenance/protocol';
  const host = $derived(page.url.searchParams.get('host') as Host);
  const challenge = $derived(page.url.searchParams.get('challenge') || '');
  const expires = $derived(Number(page.url.searchParams.get('expires')));
  const valid = $derived(
    Object.hasOwn(HOST_LABELS, host) &&
      /^[0-9a-f]{64}$/.test(challenge) &&
      Number.isSafeInteger(expires)
  );
  const kind = $derived(
    host === 'word' || host === 'google-docs'
      ? 'document'
      : host === 'excel' || host === 'google-sheets'
        ? 'spreadsheet'
        : 'presentation'
  );
  let files = $state<Document[]>([]),
    selected = $state(''),
    error = $state(''),
    busy = $state(false),
    approved = $state(false),
    cursor = $state<string | null>(null);
  async function load() {
    busy = true;
    try {
      const data = await workspaceRequest<{ items: Document[]; next_cursor: string | null }>(
        `/documents?limit=100${cursor ? `&cursor=${encodeURIComponent(cursor)}` : ''}`
      );
      files = [...files, ...data.items];
      cursor = data.next_cursor;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    if (valid) void load();
  });
  async function approve() {
    busy = true;
    error = '';
    try {
      await workspaceRequest(`/documents/${selected}/connectors`, {
        host,
        key_challenge: challenge,
        expires_at: expires
      });
      approved = true;
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not approve';
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head
  ><title>Connect your editor · Dynodoc</title><meta name="robots" content="noindex" /></svelte:head
>
<main>
  <a href="/projects">Dynodoc Projects</a>
  <h1>Connect your editor</h1>
  {#if !valid}<p role="alert">Open a fresh sign-in link from your Dynodoc editor add-in.</p>
  {:else if approved}<p role="status">
      Connection approved. Return to your editor and choose “I approved this connection”.
    </p>
  {:else}
    <p>
      Connect <strong>{HOST_LABELS[host]}</strong> to one file. Only approve if you started this in
      your editor and its code is <strong>{challenge.slice(0, 12)}</strong>.
    </p>
    <p>
      This connection can read this file and its retained history, and propose changes for review
      for seven days. It cannot merge changes or manage access. You can revoke it in the file's Sync
      &amp; connections panel.
    </p>
    <label
      >File to connect<select bind:value={selected} disabled={busy}
        ><option value="">Choose a file</option
        >{#each files.filter((f) => documentKind(f) === kind) as file (file.id)}<option
            value={file.id}>{file.title}</option
          >{/each}</select
      ></label
    >
    {#if cursor}<button disabled={busy} onclick={() => void load()}>Load more files</button>{/if}
    <button disabled={!selected || busy} onclick={() => void approve()}>Approve connection</button>
    <a href="/projects">Cancel</a>
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
</main>

<style>
  main {
    max-width: 42rem;
    margin: 3rem auto;
    padding: 1.5rem;
  }
  label {
    display: block;
    margin: 1rem 0;
  }
  select {
    display: block;
    width: 100%;
  }
  button {
    margin-right: 1rem;
  }
</style>
