<script lang="ts">
  import { untrack } from 'svelte';
  import type { EventPayload } from '$lib/api/types';
  import { workspaceRequest } from '$lib/workspace/api';
  import { download } from '$lib/workspace/files';

  let {
    documentId,
    head,
    initialRevision,
    canCreate,
    onCreated
  }: {
    documentId: string;
    head: number;
    initialRevision: number;
    canCreate: boolean;
    onCreated: (id: string) => void;
  } = $props();
  interface Preview {
    through_seq: number;
    base_seq: number;
    ops: EventPayload[];
    blocks: { id: string; label: string }[];
    can_propose: boolean;
    blocked_reason: string | null;
  }
  let revision = $state(untrack(() => initialRevision));
  let preview = $state<Preview | null>(null);
  let selected = $state<string[]>([]);
  let busy = $state(false);
  let error = $state('');
  let requestId = $state('');
  let visibleBlocks = $state(200);
  const endpoint = $derived(`/documents/${documentId}/history-recovery`);
  const blocks = $derived.by(() => {
    const groups = new Map<string, EventPayload[]>();
    const labels = new Map(preview?.blocks.map((block) => [block.id, block.label]) || []);
    for (const op of preview?.ops || []) {
      if ('node_id' in op && op.node_id) {
        const group = groups.get(op.node_id) || [];
        group.push(op);
        groups.set(op.node_id, group);
      }
    }
    return [...groups].map(([id, ops]) => ({
      id,
      ops,
      label: labels.get(id)
    }));
  });
  async function run(action: () => Promise<void>) {
    busy = true;
    error = '';
    try {
      await action();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Recovery could not be completed.';
    } finally {
      busy = false;
    }
  }
  async function inspect() {
    preview = null;
    selected = [];
    visibleBlocks = 200;
    await run(async () => {
      preview = await workspaceRequest<Preview>(endpoint, { through_seq: revision });
      requestId = crypto.randomUUID();
    });
  }
  async function create() {
    if (!preview || !canCreate || !selected.length) return;
    const plan = preview;
    await run(async () => {
      const receipt = await workspaceRequest<{ id: string }>(endpoint, {
        action: 'create',
        through_seq: plan.through_seq,
        base_seq: plan.base_seq,
        node_ids: selected,
        request_id: requestId
      });
      onCreated(receipt.id);
    });
  }
  async function exportCheckpoint() {
    await run(async () => {
      const checkpoint = await workspaceRequest<unknown>(
        `/documents/${documentId}/provenance?through_seq=${revision}`
      );
      download(
        JSON.stringify(checkpoint, null, 2),
        `dynodoc-${documentId}-revision-${revision}.json`,
        'application/json'
      );
    });
  }
  function describe(op: EventPayload) {
    if (op.type === 'NodeRestored') return 'Bring back this removed block';
    if (op.type === 'NodeDeleted') return 'Remove this later block';
    if (op.type === 'FieldEdited')
      return `${op.field}: ${typeof op.value === 'string' ? op.value : JSON.stringify(op.value)}`;
    if (op.type === 'RichTextPatched') return 'Recover earlier text and formatting';
    if (op.type === 'NodeMoved') return 'Recover the earlier position';
    return op.type;
  }
</script>

<section class="history-recovery" aria-label="Recover earlier content">
  <h3>Recover earlier content</h3>
  <p>
    Open a retained revision, choose the blocks you need, and create a private draft for review. The
    team version changes only when an authorized person merges it.
  </p>
  <form
    onsubmit={(event) => {
      event.preventDefault();
      void inspect();
    }}
  >
    <label
      >Earlier revision
      <input
        type="number"
        min="0"
        max={head}
        step="1"
        required
        bind:value={revision}
        disabled={busy}
        oninput={() => {
          preview = null;
          selected = [];
        }}
      />
    </label>
    <div class="actions">
      <button disabled={busy}>Preview recovery</button>
      <button
        type="button"
        disabled={busy || !Number.isSafeInteger(revision) || revision < 0 || revision > head}
        onclick={() => void exportCheckpoint()}>Download checkpoint JSON</button
      >
    </div>
  </form>
  {#if error}<p role="alert">{error}</p>{/if}
  {#if preview}
    <p>
      Content from revision {preview.through_seq}, compared with team revision {preview.base_seq}.
    </p>
    {#if preview.blocked_reason}<p>{preview.blocked_reason}</p>{/if}
    {#if blocks.length}
      <p>Select whole blocks. Related parents or children may need to be selected together.</p>
      <div class="actions">
        <button
          disabled={busy}
          onclick={() => {
            selected = blocks.map((block) => block.id);
            requestId = crypto.randomUUID();
          }}>Select all blocks</button
        >
        <button
          disabled={busy}
          onclick={() => {
            selected = [];
            requestId = crypto.randomUUID();
          }}>Clear selection</button
        >
      </div>
      <div class="blocks">
        {#each blocks.slice(0, visibleBlocks) as block (block.id)}
          <label class="block">
            <input
              type="checkbox"
              value={block.id}
              bind:group={selected}
              disabled={busy}
              onchange={() => {
                requestId = crypto.randomUUID();
              }}
            />
            <span
              >{block.label || `Block ${block.id}`}
              {#each block.ops as op}<small>{describe(op).slice(0, 240)}</small>{/each}
            </span>
          </label>
        {/each}
      </div>
      {#if blocks.length > visibleBlocks}
        <button
          disabled={busy}
          onclick={() => {
            visibleBlocks += 200;
          }}>Show more blocks</button
        >
      {/if}
      <p>
        {selected.length} of {blocks.length} blocks selected. Recover up to 5,000 blocks at a time.
      </p>
      {#if preview.can_propose}
        <button
          disabled={busy ||
            !canCreate ||
            !selected.length ||
            selected.length > 5000 ||
            preview.base_seq !== head}
          onclick={() => void create()}>Create recovery draft</button
        >
        {#if !canCreate}<p>
            Save your changes and open the team version before creating a recovery draft.
          </p>{/if}
        {#if preview.base_seq !== head}<p>
            The team changed. Preview again before creating a draft.
          </p>{/if}
      {:else}<p>
          Your access lets you inspect and download history. A contributor can propose recovery.
        </p>{/if}
    {:else}<p>The content already matches this revision.</p>{/if}
  {/if}
</section>

<style>
  .history-recovery {
    border-bottom: 1px solid var(--border);
    padding-bottom: 1rem;
    margin-bottom: 1rem;
  }
  h3 {
    font-size: 1rem;
  }
  p,
  small {
    font-size: 0.85rem;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin: 0.6rem 0;
  }
  .blocks {
    max-height: 20rem;
    overflow: auto;
    margin-bottom: 0.75rem;
  }
  .block {
    display: flex;
    align-items: start;
    gap: 0.5rem;
    padding: 0.5rem 0;
  }
  .block input {
    width: auto;
    margin-top: 0.2rem;
  }
  .block span {
    overflow-wrap: anywhere;
    font-size: 0.8rem;
  }
  small {
    display: block;
  }
  [role='alert'] {
    color: var(--danger, #b42318);
  }
</style>
