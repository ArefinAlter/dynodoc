<script lang="ts">
  import type { Snippet } from 'svelte';
  let {
    label,
    busy = false,
    children
  }: { label: string; busy?: boolean; children: Snippet } = $props();
</script>

<!-- A scrollable area must be reachable by keyboard so its overflow can be read (WCAG 2.1.1). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="table-scroll" role="region" aria-label={label} aria-busy={busy} tabindex="0">
  {@render children()}
</div>
