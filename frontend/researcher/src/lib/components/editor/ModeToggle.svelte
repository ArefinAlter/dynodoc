<script lang="ts">
  import type { EditorMode } from '$lib/types/editor';

  interface Props {
    mode: EditorMode;
    onChange: (mode: EditorMode) => void;
  }

  let { mode, onChange }: Props = $props();

  const modes: EditorMode[] = ['document', 'outline', 'spec', 'preview'];
</script>

<div class="toggle" aria-label="Editor mode">
  {#each modes as item}
    <button type="button" class:active={item === mode} onclick={() => onChange(item)}>
      {item[0].toUpperCase() + item.slice(1)}
    </button>
  {/each}
</div>

<style>
  .toggle {
    display: flex;
    gap: 0.2rem;
    padding: 0.2rem;
    border: 1px solid var(--color-border);
    border-radius: 0.6rem;
    background: var(--color-surface-muted);
    font-family: var(--font-ui);
  }

  button {
    border: 0;
    border-radius: 0.45rem;
    padding: 0.45rem 0.85rem;
    background: transparent;
    color: var(--color-muted);
    font: inherit;
  }

  button.active {
    background: var(--color-ink);
    color: var(--color-surface);
    font-weight: 600;
  }
</style>
