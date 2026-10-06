<script lang="ts">
  import { onMount } from 'svelte';
  import {
    CaptureSession,
    browserStore,
    connectorRequest,
    projectCheckpoint,
    type Checkpoint,
    type Receipt,
    type Block
  } from '$lib/provenance/protocol';
  import { WordAdapter, type WordRuntime } from '$lib/provenance/word';
  import { downloadJson } from '$lib/workspace/projects';
  let origin = $state(''),
    documentId = $state(''),
    token = $state(''),
    name = $state('Word changes'),
    note = $state(''),
    enabled = $state(false),
    busy = $state(false),
    ready = $state(false),
    error = $state(''),
    notice = $state('');
  let session = $state<CaptureSession | null>(null),
    localFile = '',
    adapter: WordAdapter | null = null,
    preview = $state<{ checkpoint: Checkpoint; expected: Block[]; next: Block[] } | null>(null);
  onMount(() => {
    origin = location.origin;
    const script = document.createElement('script');
    script.src = 'https://appsforoffice.microsoft.com/lib/1/hosted/office.js';
    script.onload = () => {
      const office = window as unknown as {
        Office: {
          onReady: (callback: (info: { host: string }) => void) => void;
          context: {
            document: { url: string };
            requirements: { isSetSupported: (api: string, version: string) => boolean };
          };
        };
        Word: WordRuntime;
      };
      office.Office.onReady((info) => {
        if (
          info.host !== 'Word' ||
          !office.Office.context.requirements.isSetSupported('WordApi', '1.3')
        ) {
          error = 'Open this task pane in Word with WordApi 1.3 support.';
          return;
        }
        adapter = new WordAdapter(office.Word);
        localFile = office.Office.context.document.url;
        if (!localFile) {
          error = 'Save this Word file before connecting so its working copy has a durable queue.';
          return;
        }
        ready = true;
      });
    };
    script.onerror = () => {
      error = 'Could not load the Microsoft Word runtime.';
    };
    document.head.append(script);
    const timer = setInterval(() => {
      if (enabled && session && !busy)
        void run(async () => {
          await session!.capture(await adapter!.read());
          notice = `${session!.ledger.changes.length} observed paragraph changes saved locally`;
        });
    }, 2000);
    return () => {
      clearInterval(timer);
      script.remove();
    };
  });
  async function run(action: () => Promise<void>) {
    if (busy) return;
    busy = true;
    error = '';
    try {
      await action();
    } catch (e) {
      enabled = false;
      error = e instanceof Error ? e.message : 'The operation failed';
    } finally {
      busy = false;
    }
  }
  const request = <T,>(
    action: 'checkpoint' | 'bundles',
    body?: Parameters<typeof connectorRequest>[4]
  ) => connectorRequest<T>(origin, documentId, token, action, body);
  async function connect() {
    if (!adapter) return;
    const checkpoint = await request<Checkpoint>('checkpoint'),
      fileKey = Array.from(
        new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(localFile)))
      )
        .map((b) => b.toString(16).padStart(2, '0'))
        .join(''),
      store = browserStore(`dynodoc.word.ledger.${origin}.${documentId}.${fileKey}`),
      saved = await store.load();
    if (saved) {
      session = await CaptureSession.connect(store, checkpoint, [], 'word');
      await session.capture(await adapter.read());
    } else {
      const blocks = projectCheckpoint(checkpoint);
      await adapter.bind(blocks);
      session = await CaptureSession.connect(store, checkpoint, await adapter.read(), 'word');
    }
    ready = true;
    notice = 'Connected. Enable capture to record paragraph text changes while this pane is open.';
  }
  async function send() {
    if (!session || !adapter) return;
    await session.capture(await adapter.read());
    const push = await session.prepare(name, note, true);
    const receipt = await request<Receipt>('bundles', push);
    await session.acknowledge(receipt);
    notice = 'Changes sent for review. You can keep editing; pull after the request is merged.';
  }
  async function plan() {
    if (!session || !adapter) return;
    await session.capture(await adapter.read());
    const checkpoint = await request<Checkpoint>('checkpoint');
    session.canApply(checkpoint);
    const expected = await adapter.read(),
      rows = session.plan(checkpoint, expected);
    if (rows.some((r) => r.conflict))
      throw new Error(
        'Local and team edits overlap. Export your captured changes and resolve them through a change request before pulling.'
      );
    preview = { checkpoint, expected, next: rows.map((r) => ({ id: r.id, text: r.text })) };
  }
  async function apply() {
    if (!preview || !adapter || !session) return;
    session.canApply(preview.checkpoint);
    await adapter.apply(preview.expected, preview.next);
    await session.acceptPull(preview.checkpoint, await adapter.read());
    preview = null;
    notice = 'Team changes applied to Word. Local observations remain in your exported ledger.';
  }
</script>

<svelte:head
  ><title>Word provenance · Dynodoc</title><meta name="robots" content="noindex" /></svelte:head
>
<main>
  <h1>Dynodoc for Word</h1>
  <p>
    Development connector: existing paragraph text, with stable block identities. Save the DOCX to
    retain its identities. Capture runs only while this pane is open.
  </p>
  {#if error}<p role="alert">{error}</p>{/if}{#if notice}<p role="status">{notice}</p>{/if}
  <label>Dynodoc address<input bind:value={origin} disabled={!!session} /></label><label
    >File ID<input bind:value={documentId} disabled={!!session} /></label
  ><label>File connection key<input type="password" bind:value={token} autocomplete="off" /></label>
  <button disabled={busy || !ready || !!session} onclick={() => run(connect)}
    >Connect this file</button
  >
  {#if session}
    <label
      ><input type="checkbox" bind:checked={enabled} disabled={busy} /> Enable paragraph capture</label
    >
    <p>
      Capture stores checkpoint observations on this device. It does not prove who typed each change
      or record every keystroke. Text replacements may inherit the host's formatting; formatting
      itself is not synchronised.
    </p>
    <button
      disabled={busy}
      onclick={() =>
        run(async () => {
          await session!.capture(await adapter!.read());
          notice = 'Changes captured locally';
        })}>Capture now</button
    >
    <label>Change request name<input bind:value={name} maxlength="200" /></label><label
      >Message<textarea bind:value={note} maxlength="4000"></textarea></label
    ><button disabled={busy} onclick={() => run(send)}>Push changes for review</button><button
      disabled={busy}
      onclick={() => run(plan)}>Preview team changes</button
    >
    {#if preview}<p>
        {preview.next.filter((b, i) => b.text !== preview!.expected[i].text).length} paragraphs will change
        in Word. Local edits are retained.
      </p>
      <button disabled={busy} onclick={() => run(apply)}>Apply preview to Word</button><button
        onclick={() => {
          preview = null;
        }}>Cancel</button
      >{/if}
    <button
      disabled={busy}
      onclick={() => downloadJson(session!.export(), `${documentId}.word-ledger.json`)}
      >Export local provenance</button
    >
    {#if session.ledger.pending}<button
        disabled={busy}
        onclick={() => downloadJson(session!.ledger.pending, `${documentId}.changes.json`)}
        >Export queued upload</button
      >{/if}
  {/if}
  <p><a href="/projects" target="_blank" rel="noreferrer">Open Projects</a></p>
</main>

<style>
  main {
    padding: 18px;
    color: var(--color-ink);
    max-width: 600px;
    margin: auto;
  }
  label {
    display: grid;
    gap: 5px;
    margin: 12px 0;
  }
  input,
  textarea,
  button {
    font: inherit;
    padding: 8px;
    border: 1px solid var(--color-border);
    border-radius: 5px;
    background: var(--color-surface);
    color: inherit;
  }
  button {
    cursor: pointer;
    margin: 4px;
  }
  p {
    line-height: 1.5;
  }
  a {
    color: var(--workspace-accent);
  }
</style>
