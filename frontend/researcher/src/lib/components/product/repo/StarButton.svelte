<script lang="ts">
  import { Star } from '@lucide/svelte';
  import { isAbort, issueMessage, setStar } from '$lib/workspace/issues';
  import { plural } from '$lib/workspace/repo-ui';

  let {
    projectId,
    starred,
    count,
    onChange,
    onError
  }: {
    projectId: string;
    starred: boolean;
    count: number;
    /** The server's answer, never a guess. */
    onChange: (state: { starred: boolean; starCount: number }) => void;
    onError: (message: string) => void;
  } = $props();
  const descriptionId = $props.id();
  let busy = $state(false);

  async function toggle() {
    if (busy) return;
    busy = true;
    try {
      onChange(await setStar(projectId, !starred));
    } catch (error) {
      if (!isAbort(error)) onError(issueMessage(error, 'star'));
    } finally {
      busy = false;
    }
  }
</script>

<button
  type="button"
  class="repo-star"
  class:on={starred}
  aria-pressed={starred}
  aria-describedby={descriptionId}
  disabled={busy}
  onclick={() => void toggle()}
  ><Star size={15} fill={starred ? 'currentColor' : 'none'} aria-hidden="true" />{starred
    ? 'Starred'
    : 'Star'}<span class="repo-counter" aria-hidden="true">{count}</span></button
><span id={descriptionId} class="sr-only">{plural(count, 'star')}</span>
