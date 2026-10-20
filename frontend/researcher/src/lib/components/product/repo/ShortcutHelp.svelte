<script lang="ts">
  import { X } from '@lucide/svelte';
  let { open = $bindable(false) }: { open?: boolean } = $props();
  let dialog = $state<HTMLDialogElement | null>(null);
  $effect(() => {
    if (!dialog) return;
    if (open && !dialog.open) dialog.showModal();
    else if (!open && dialog.open) dialog.close();
  });
  const rows: [string[], string][] = [
    [['/'], 'Search projects, files and actions'],
    [['Ctrl', 'K'], 'Open the command palette'],
    [['t'], 'Go to a file in this project'],
    [['g', 'p'], 'Go to your projects'],
    [['g', 'e'], 'Go to the web editors'],
    [['g', 'c'], 'Go to connect editors'],
    [['?'], 'Show this list'],
    [['Esc'], 'Close a menu or dialog']
  ];
</script>

<dialog
  bind:this={dialog}
  class="help"
  aria-labelledby="shortcut-title"
  onclose={() => (open = false)}
  onclick={(e) => {
    if (e.target === dialog) open = false;
  }}
>
  {#if open}
    <header>
      <h2 id="shortcut-title">Keyboard shortcuts</h2>
      <button
        type="button"
        class="close"
        aria-label="Close shortcuts"
        onclick={() => (open = false)}><X size={16} aria-hidden="true" /></button
      >
    </header>
    <p class="note">Shortcuts do not run while you are typing in a field.</p>
    <dl>
      {#each rows as [keys, label]}
        <div>
          <dt>
            {#each keys as k, i}{#if i}<span class="then">then</span>{/if}<kbd>{k}</kbd>{/each}
          </dt>
          <dd>{label}</dd>
        </div>
      {/each}
    </dl>
  {/if}
</dialog>

<style>
  .help {
    width: min(460px, calc(100vw - 24px));
    margin: 14vh auto auto;
    margin: 14dvh auto auto;
    padding: 0 20px 18px;
    border: 1px solid var(--rp-border);
    border-radius: 12px;
    background: var(--rp-overlay);
    color: var(--rp-fg);
    box-shadow: var(--rp-shadow);
  }
  .help::backdrop {
    background: rgb(1 4 9 / 0.5);
  }
  .help[open] {
    animation: rise 0.14s ease-out;
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(-6px) scale(0.985);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .help[open] {
      animation: none;
    }
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 0 4px;
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  .close {
    display: inline-grid;
    place-items: center;
    width: 32px;
    height: 32px;
    border: 0;
    border-radius: 6px;
    background: transparent;
    color: var(--rp-muted);
    cursor: pointer;
  }
  .close:hover {
    background: var(--rp-btn-hover);
    color: var(--rp-fg);
  }
  .note {
    margin: 0 0 10px;
    color: var(--rp-muted);
    font-size: 13px;
  }
  dl {
    margin: 0;
  }
  dl div {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 8px 0;
    border-top: 1px solid var(--rp-border-muted);
  }
  dt {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: none;
  }
  dd {
    margin: 0;
    text-align: right;
  }
  .then {
    color: var(--rp-muted);
    font-size: 12px;
  }
  kbd {
    min-width: 22px;
    padding: 1px 6px;
    border: 1px solid var(--rp-border);
    border-bottom-width: 2px;
    border-radius: 5px;
    background: var(--rp-subtle);
    font-family: var(--rp-mono);
    font-size: 12px;
    text-align: center;
  }
</style>
