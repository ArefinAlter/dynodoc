<script lang="ts">
  import { workspaceRequest } from '$lib/workspace/api';
  let {
    projectId,
    onMoved,
    open = $bindable(false)
  }: { projectId: string; onMoved: () => Promise<void>; open?: boolean } = $props();
  type FileChoice = { id: string; title: string; space_id: string | null; can_manage: boolean };
  let files = $state<FileChoice[]>([]),
    selected = $state(''),
    busy = $state(false),
    error = $state(''),
    offset = $state(0),
    more = $state(false);
  async function load(next = 0) {
    busy = true;
    error = '';
    try {
      const result = await workspaceRequest<{ items: FileChoice[]; more: boolean }>(
        `/workspace/index?offset=${next}`
      );
      files = result.items.filter((f) => f.can_manage && f.space_id !== projectId);
      offset = next;
      more = result.more;
      selected = '';
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not load your files';
    } finally {
      busy = false;
    }
  }
  async function move() {
    if (!selected || busy) return;
    busy = true;
    error = '';
    try {
      await workspaceRequest(`/documents/${selected}/location`, { space_id: projectId });
      await onMoved();
      await load(offset);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not move the file';
    } finally {
      busy = false;
    }
  }
</script>

<details
  id="move-project-file"
  class="repo-box move"
  bind:open
  ontoggle={(e) => {
    if (e.currentTarget.open) void load();
  }}
>
  <summary>Move an existing file into this project</summary>
  {#if error}<p class="repo-alert" role="alert">{error}</p>{/if}
  <form
    class="repo-form"
    onsubmit={(e) => {
      e.preventDefault();
      void move();
    }}
  >
    <label
      >Choose a file<select bind:value={selected} disabled={busy} required>
        <option value="">Select a file you manage</option>
        {#each files as file}<option value={file.id}>{file.title}</option>{/each}
      </select></label
    >
    {#if !busy && !files.length}<p class="repo-muted">No files you can move on this page.</p>{/if}
    <div class="repo-actions">
      <button class="repo-primary" disabled={busy || !selected}>Move file</button>
      <button
        type="button"
        disabled={busy || offset === 0}
        onclick={() => load(Math.max(0, offset - 100))}>Previous files</button
      >
      <button type="button" disabled={busy || !more} onclick={() => load(offset + 100)}
        >More files</button
      >
    </div>
  </form>
  <p class="repo-muted repo-small">
    The file will inherit this project's access and review rules. Existing direct access remains.
  </p>
</details>
