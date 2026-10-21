<script lang="ts">
  /**
   * The mark of work written by an AI agent rather than a person: the same icon and words as
   * the badge on the signing keys screen, so it reads the same everywhere. The words are
   * always there (the icon is decoration, and colour is not the signal).
   */
  import { Bot } from '@lucide/svelte';
  let { title = '' }: { title?: string } = $props();
</script>

<span class="agent-badge" {title}><Bot size={14} aria-hidden="true" />AI agent</span>

<style>
  .agent-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 0 7px;
    border: 1px solid var(--color-accent-border);
    border-radius: 999px;
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
    font-size: 12px;
    font-weight: 500;
    line-height: 20px;
    white-space: nowrap;
    vertical-align: middle;
  }
</style>
