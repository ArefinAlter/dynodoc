<script lang="ts">
  import { FileText, FileSpreadsheet, Presentation, File } from '@lucide/svelte';
  import { formatOf } from '$lib/workspace/repo-ui';
  let { kind, size = 16 }: { kind: string; size?: number } = $props();
  const key = $derived(formatOf(kind).key);
</script>

<span class={`fmt ${key}`} aria-hidden="true">
  {#if key === 'word'}<FileText {size} />{:else if key === 'excel'}<FileSpreadsheet
      {size}
    />{:else if key === 'powerpoint'}<Presentation {size} />{:else}<File {size} />{/if}
</span>

<style>
  .fmt {
    display: inline-flex;
    flex: none;
    color: var(--rp-muted);
  }
  .word {
    color: var(--rp-format-word);
  }
  .excel {
    color: var(--rp-format-excel);
  }
  .powerpoint {
    color: var(--rp-format-powerpoint);
  }
</style>
