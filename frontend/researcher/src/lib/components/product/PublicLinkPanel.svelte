<script lang="ts">
  import { onMount } from 'svelte';
  import { workspaceRequest as request } from '$lib/workspace/api';
  import type { Space } from '$lib/workspace/access';
  let { documentId, prepare }: { documentId: string; prepare: () => Promise<number> } = $props();
  let canManage = $state(false),
    spaceId = $state(''),
    spaces = $state<Space[]>([]),
    links = $state<
      { id: string; created_at: string; revoked_at: string | null; expires_at: string | null }[]
    >([]),
    url = $state(''),
    error = $state(''),
    busy = $state(false),
    days = $state('30');
  const prefix = $derived(`/documents/${documentId}`);
  async function refresh() {
    const a = await request<{ can_manage: boolean; space_id: string | null }>(`${prefix}/access`);
    canManage = a.can_manage;
    spaceId = a.space_id || '';
    if (canManage) {
      links = (await request<{ items: typeof links }>(`${prefix}/public-links`)).items;
      spaces = (await request<{ items: Space[] }>('/spaces')).items;
    }
  }
  async function run(work: () => Promise<void>) {
    error = '';
    busy = true;
    try {
      await work();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not update sharing';
    } finally {
      busy = false;
    }
  }
  onMount(() => {
    void run(refresh);
  });
  async function publish() {
    await run(async () => {
      const base_seq = await prepare();
      const p = await request<{ token: string }>(`${prefix}/public-links`, {
        base_seq,
        expires_days: days ? Number(days) : null
      });
      url = `${location.origin}/view/${p.token}`;
      await refresh();
    });
  }
</script>

{#if canManage}<section class="public-links">
    <h3>Publish a read-only copy</h3>
    <p>
      Anyone with the link can read this saved copy without an account. Comments, original uploads,
      and history stay private. Later edits require a new link. Accept or reject tracked text
      changes before publishing.
    </p>
    <label
      >Link expires<select bind:value={days}
        ><option value="7">In 7 days</option><option value="30">In 30 days</option><option
          value="90">In 90 days</option
        ><option value="">When I revoke it</option></select
      ></label
    ><button disabled={busy} onclick={() => void publish()}>Create public link</button
    >{#if url}<label
        >New public link<input
          readonly
          value={url}
          onclick={(e) => e.currentTarget.select()}
        /></label
      ><button
        onclick={() =>
          void run(async () => {
            await navigator.clipboard.writeText(url);
          })}>Copy public link</button
      >{/if}{#each links as link}<div class="link">
        <span
          >Published {new Date(link.created_at).toLocaleDateString()}<small
            >{link.revoked_at
              ? 'Revoked'
              : link.expires_at
                ? `Expires ${new Date(link.expires_at).toLocaleDateString()}`
                : 'No expiry'}</small
          ></span
        >{#if !link.revoked_at}<button
            disabled={busy}
            onclick={() =>
              void run(async () => {
                await request(`${prefix}/public-links/${link.id}/revoke`, {});
                url = '';
                await refresh();
              })}>Revoke</button
          >{/if}
      </div>{/each}
    <h3>Folder and inherited access</h3>
    <p>
      Moving a document adds the destination’s inherited access. Existing direct invitations stay in
      place.
    </p>
    <label
      >Workspace<select bind:value={spaceId}
        ><option value="">Personal documents</option
        >{#each spaces.filter((s) => ['owner', 'manager', 'editor'].includes(s.role)) as s}<option
            value={s.id}>{s.name} · {s.kind}</option
          >{/each}</select
      ></label
    ><button
      disabled={busy}
      onclick={() =>
        void run(async () => {
          await request(`${prefix}/location`, { space_id: spaceId || null });
          await refresh();
        })}>Move document</button
    >
  </section>{/if}
{#if error}<p role="alert">{error}</p>{/if}

<style>
  .public-links {
    border-top: 1px solid #d1dce8;
    margin-top: 22px;
    padding-top: 12px;
  }
  h3 {
    font-size: 16px;
  }
  p,
  small {
    font-size: 13px;
    line-height: 1.65;
    color: #456078;
  }
  label {
    display: grid;
    gap: 6px;
    margin: 12px 0;
    font-size: 13px;
  }
  select,
  input {
    min-width: 0;
    max-width: 100%;
    padding: 9px;
    font: inherit;
    background: white;
    color: #29455e;
    border: 1px solid #b4c5d5;
    border-radius: 5px;
  }
  button {
    padding: 8px 12px;
    background: #e7effa;
    color: #1c4d8d;
    border: 1px solid #bfd2ec;
    border-radius: 5px;
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .link {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    align-items: center;
    margin: 14px 0;
    font-size: 13px;
  }
  small {
    display: block;
  }
</style>
