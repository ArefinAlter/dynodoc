<script lang="ts">
  import { FALLBACK_COLOR, labelTextColor, normalizeColor } from '$lib/workspace/issues';
  let {
    name,
    color,
    description = ''
  }: { name: string; color: string; description?: string } = $props();
  const hex = $derived(normalizeColor(color) ?? FALLBACK_COLOR);
</script>

<span
  class="pill"
  style:background-color={`#${hex}`}
  style:color={labelTextColor(hex)}
  title={description || undefined}>{name}</span
>

<style>
  .pill {
    display: inline-block;
    max-width: 100%;
    padding: 0 8px;
    border-radius: 999px;
    box-shadow: inset 0 0 0 1px rgb(31 35 40 / 0.18);
    font-size: 12px;
    font-weight: 600;
    line-height: 20px;
    overflow: hidden;
    text-overflow: ellipsis;
    vertical-align: middle;
    white-space: nowrap;
  }
  :global(:root[data-theme='dark']) .pill {
    box-shadow: inset 0 0 0 1px rgb(240 246 252 / 0.22);
  }
</style>
