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
    color: #34495c;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  ins {
    background: #dcf3e3;
    color: #1d5a33;
    text-decoration: none;
    border-bottom: 1px solid #83c29a;
  }
  del {
    background: #fbe3e1;
    color: #8a2f25;
    text-decoration: line-through;
  }
</style>
