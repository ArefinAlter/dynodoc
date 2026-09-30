<script lang="ts">
  import { wordDiff } from '$lib/workspace/file-push';
  let { before, after }: { before: string; after: string } = $props();
  // Very long blocks are summarised so a review screen stays responsive.
  const parts = $derived(
    before.length + after.length > 20000
      ? [
          { kind: 'removed' as const, text: before.slice(0, 2000) + '…' },
          { kind: 'added' as const, text: after.slice(0, 2000) + '…' }
        ]
      : wordDiff(before, after)
  );
</script>

<p class="diff" dir="auto">
  {#each parts as part}{#if part.kind === 'added'}<ins>{part.text}</ins
      >{:else if part.kind === 'removed'}<del>{part.text}</del>{:else}<span>{part.text}</span
      >{/if}{/each}
</p>

<style>
  .diff {
    margin: 0;
    font-size: 13px;
    line-height: 1.75;
    color: var(--color-ink);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  ins {
    background: var(--color-diff-add-bg);
    color: var(--color-diff-add-ink);
    text-decoration: none;
    border-bottom: 1px solid color-mix(in srgb, var(--color-success) 45%, var(--color-diff-add-bg));
  }
  del {
    background: var(--color-diff-del-bg);
    color: var(--color-diff-del-ink);
    text-decoration: line-through;
  }
</style>
