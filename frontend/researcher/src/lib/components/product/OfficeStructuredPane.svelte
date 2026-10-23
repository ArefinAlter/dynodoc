<script lang="ts">
  import { onMount } from 'svelte';
  import ConnectorSignIn from './ConnectorSignIn.svelte';
  import {
    CaptureSession,
    browserStore,
    connectorRequest,
    projectCheckpoint,
    sameBlocks,
    type Checkpoint,
    type Receipt,
    type Block
  } from '$lib/provenance/protocol';
  import { wordFileUrl, type OfficeDocument } from '$lib/provenance/word';
  import {
    ExcelAdapter,
    PowerPointAdapter,
    type ExcelRuntime,
    type PowerPointRuntime,
    type StructuredAdapter
  } from '$lib/provenance/office-structured';
  let { host }: { host: 'excel' | 'powerpoint' } = $props();
  const label = $derived(host === 'excel' ? 'Excel' : 'PowerPoint');
  const apiVersion = $derived(host === 'excel' ? '1.13' : '1.4');
  const scope = $derived(
    host === 'excel' ? 'existing cell values, dates and formulas' : 'existing text in slide shapes'
  );
  import {
    exportSidecar,
    parseSidecar,
    recoveryLedger,
    restoreCapture,
    SIDECAR_MAX_BYTES,
    type Sidecar
  } from '$lib/provenance/sidecar';
  import { downloadJson } from '$lib/workspace/projects';
  import { officeJsUrl } from '$lib/connectors/office-js';
  let origin = $state(''),
    documentId = $state(''),
    token = $state(''),
    name = $state(''),
    note = $state(''),
    enabled = $state(false),
    busy = $state(false),
    ready = $state(false),
    error = $state(''),
    notice = $state('');
  let session = $state<CaptureSession | null>(null),
    recovery = $state<Sidecar | null>(null),
    localFile = '',
    officeDocument: OfficeDocument | null = null,
    adapter: StructuredAdapter | null = null,
    preview = $state<{ checkpoint: Checkpoint; expected: Block[]; next: Block[] } | null>(null);
  onMount(() => {
    origin = location.origin;
    name = `${label} changes`;
    const script = document.createElement('script');
    script.src = officeJsUrl(document);
    script.onload = () => {
      const office = window as unknown as {
        Office: {
          onReady: (callback: (info: { host: string }) => void) => void;
          context: {
            document: OfficeDocument;
            requirements: { isSetSupported: (api: string, version: string) => boolean };
          };
        };
        Excel: ExcelRuntime;
        PowerPoint: PowerPointRuntime;
      };
      office.Office.onReady((info) => {
        if (
          info.host !== label ||
          !office.Office.context.requirements.isSetSupported(label + 'Api', apiVersion)
        ) {
          error = `Open this task pane in ${label} with ${label}Api ${apiVersion} support.`;
          return;
        }
        officeDocument = office.Office.context.document;
        adapter =
          host === 'excel'
            ? new ExcelAdapter(office.Excel, assertWorkingCopy)
            : new PowerPointAdapter(office.PowerPoint, assertWorkingCopy);
        ready = true;
      });
    };
    script.onerror = () => {
      error = 'Could not load the Microsoft editor runtime.';
    };
    document.head.append(script);
    const timer = setInterval(() => {
      if (enabled && session && !busy)
        void run(async () => {
          await session!.capture(await adapter!.read());
          notice = `${session!.ledger.changes.length} observed changes saved locally`;
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
  async function assertWorkingCopy() {
    if (localFile && officeDocument && (await wordFileUrl(officeDocument)) !== localFile)
      throw new Error(
        'The working copy changed. Export this queue, reopen the pane in the saved copy and restore its provenance before continuing.'
      );
  }
  async function request<T>(
    action: 'checkpoint' | 'bundles',
    body?: Parameters<typeof connectorRequest>[4],
    throughSeq?: number
  ) {
    await assertWorkingCopy();
    return connectorRequest<T>(origin, documentId, token, action, body, throughSeq);
  }
  async function chooseRecovery(event: Event) {
    recovery = null;
    const file = (event.currentTarget as HTMLInputElement).files?.[0];
    if (!file) return;
    if (file.size > SIDECAR_MAX_BYTES)
      throw new Error('Use a provenance recovery file up to 16 MiB.');
    recovery = parseSidecar(await file.text());
    notice =
      'Recovery selected. Connect to the original Dynodoc file using the same account. Existing content will not be replaced.';
  }
  async function connect() {
    if (!adapter || !officeDocument) return;
    localFile = await wordFileUrl(officeDocument);
    const checkpoint = await request<Checkpoint>('checkpoint'),
      fileKey = Array.from(
        new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(localFile)))
      )
        .map((b) => b.toString(16).padStart(2, '0'))
        .join(''),
      store = browserStore(`dynodoc.${host}.ledger.${origin}.${documentId}.${fileKey}`),
      saved = await store.load();
    if (recovery) {
      if (saved)
        throw new Error(
          'This working copy already has a saved queue. Export it and use a fresh copy; recovery cannot replace it.'
        );
      const verifiedBase = await request<Checkpoint>(
        'checkpoint',
        undefined,
        recovery.ledger.checkpoint.through_seq
      );
      const ledger = recoveryLedger(recovery, checkpoint, host, verifiedBase);
      adapter.configure(ledger.checkpoint);
      await adapter.bind(ledger.seen);
      session = await restoreCapture(
        store,
        recovery,
        checkpoint,
        await adapter.read(),
        host,
        verifiedBase
      );
      notice =
        'Provenance restored. Queued upload IDs and earlier observations are retained. Enable capture explicitly to continue.';
      recovery = null;
    } else if (saved) {
      adapter.configure(saved.checkpoint);
      session = await CaptureSession.connect(store, checkpoint, [], host);
      await session.capture(await adapter.read());
    } else {
      adapter.configure(checkpoint);
      const blocks = projectCheckpoint(checkpoint);
      await adapter.bind(blocks);
      session = await CaptureSession.connect(store, checkpoint, await adapter.read(), host);
    }
    ready = true;
    if (!notice.startsWith('Provenance restored'))
      notice = 'Connected. Enable capture to record changes while this pane is open.';
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
    preview = {
      checkpoint,
      expected,
      next: rows.map((r) => ({ id: r.id, field: r.field, text: r.text }))
    };
  }
  async function apply() {
    if (!preview || !adapter || !session) return;
    session.canApply(preview.checkpoint);
    await adapter.apply(preview.expected, preview.next);
    const actual = await adapter.read();
    if (!sameBlocks(actual, preview.next))
      throw new Error(
        'The editor did not apply the complete preview. The saved starting revision is unchanged; inspect the file and export its provenance.'
      );
    await session.acceptPull(preview.checkpoint, actual);
    preview = null;
    notice =
      'Team changes applied to the editor. Local observations remain in your exported ledger.';
  }
</script>

<svelte:head><title>Dynodoc for {label}</title><meta name="robots" content="noindex" /></svelte:head
>
<main>
  <h1>Dynodoc for {label}</h1>
  <p>
    Sign in to connect this file to a Dynodoc project. While this pane is open, Dynodoc records
    edits to {scope} so you can send them for review and bring approved team changes back. Save the file
    first so identities stay with it. Structural and formatting changes use file push.
  </p>
  {#if error}<p role="alert">{error}</p>{/if}{#if notice}<p role="status">{notice}</p>{/if}
  {#if !session}<ConnectorSignIn
      {host}
      {origin}
      disabled={busy || !ready}
      onConnected={async (value) => {
        documentId = value.documentId;
        token = value.token;
        await run(connect);
      }}
    />
    <label
      >Restore provenance from this editor (optional)<input
        type="file"
        accept=".json,application/json"
        disabled={busy}
        onchange={(event) => run(() => chooseRecovery(event))}
      /></label
    >
    <p>
      Use an exported recovery file in a saved copy with matching content. Existing queues are
      preserved.
    </p>
  {/if}
  <details class="advanced">
    <summary>Advanced: connect with a key</summary>
    <div class="advanced-body">
      <p>
        Only needed when browser sign-in is not available. Create a key under Sync &amp; connections
        in your Dynodoc project. A key works for one file and expires after seven days.
      </p>
      <label>Dynodoc address<input bind:value={origin} disabled={!!session} /></label><label
        >File ID<input bind:value={documentId} disabled={!!session} /></label
      ><label
        >File connection key<input type="password" bind:value={token} autocomplete="off" /></label
      >
      <button disabled={busy || !ready || !!session} onclick={() => run(connect)}
        >Connect this file</button
      >
    </div>
  </details>
  {#if session}
    <label><input type="checkbox" bind:checked={enabled} disabled={busy} /> Enable capture</label>
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
        {preview.next.filter((b, i) => b.text !== preview!.expected[i].text).length}
        {host === 'excel' ? 'cells' : 'text shapes'} will change in {label}. Local edits are
        retained.
      </p>
      <button disabled={busy} onclick={() => run(apply)}>Apply preview to {label}</button><button
        onclick={() => {
          preview = null;
        }}>Cancel</button
      >{/if}
    <button
      disabled={busy}
      onclick={() => downloadJson(exportSidecar(session!), `${documentId}.${host}-provenance.json`)}
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
  .advanced {
    margin: 16px 0;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-surface);
  }
  .advanced summary {
    cursor: pointer;
    padding: 10px 12px;
    font-weight: 600;
  }
  .advanced summary:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }
  .advanced-body {
    padding: 0 12px 12px;
  }
</style>
