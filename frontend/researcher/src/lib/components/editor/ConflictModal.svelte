<script lang="ts">
  import type { ConflictCard } from '$lib/types/editor';

  interface Props {
    conflict: ConflictCard;
    open: boolean;
    selectedOptionId: string;
    onClose: () => void;
    onSelect: (id: string) => void;
  }

  let { conflict, open, selectedOptionId, onClose, onSelect }: Props = $props();

  function handleOverlayKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onClose();
    }
  }

  function handleOverlayClick(event: MouseEvent) {
    if (event.target === event.currentTarget) {
      onClose();
    }
  }
</script>

{#if open}
  <div class="overlay" role="button" tabindex="0" aria-label="Close conflict dialog" onclick={handleOverlayClick} onkeydown={handleOverlayKeydown}>
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
      <div class="overline">{conflict.remaining} conflicts pending · {conflict.itemLabel}</div>
      <h2>Four-card conflict resolution</h2>
      <p>
        This UI is safe to build in the PoC because same-field conflicts are a core product
        promise, even before the final merge engine wiring is complete.
      </p>

      <div class="cards">
        {#each conflict.options as option}
          <button type="button" class:selected={selectedOptionId === option.id} class="card" onclick={() => onSelect(option.id)}>
            <div class="label">{option.label}</div>
            <div class="text">{option.text}</div>
          </button>
        {/each}
      </div>

      <div class="footer">
        <button type="button" class="secondary" onclick={onClose}>Close</button>
        <button type="button" disabled={!selectedOptionId}>Apply selection</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 1rem;
    background: rgba(21, 16, 11, 0.35);
    backdrop-filter: blur(6px);
    z-index: 40;
  }

  .modal {
    width: min(62rem, 100%);
    padding: 1.3rem;
    border-radius: 1.2rem;
    background: var(--color-surface);
    box-shadow: 0 24px 60px rgba(20, 13, 8, 0.2);
  }

  .overline,
  .label,
  .footer button {
    font-family: var(--font-ui);
  }

  .overline {
    color: var(--color-muted);
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.1em;
  }

  h2 {
    margin: 0.4rem 0 0.6rem;
    font-family: var(--font-display);
    font-size: 1.8rem;
    font-weight: 500;
  }

  .cards {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 0.9rem;
    margin: 1rem 0 1.25rem;
  }

  .card {
    border: 1px solid var(--color-border);
    border-radius: 1rem;
    padding: 1rem;
    background: var(--color-surface-muted);
    text-align: left;
    color: inherit;
  }

  .card.selected {
    border-color: var(--color-accent);
    background: var(--color-accent-bg);
  }

  .label {
    color: var(--color-accent);
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin-bottom: 0.45rem;
  }

  .footer {
    display: flex;
    justify-content: flex-end;
    gap: 0.7rem;
  }

  .footer button {
    border: 1px solid var(--color-border);
    border-radius: 0.7rem;
    padding: 0.7rem 0.95rem;
    background: var(--color-ink);
    color: var(--color-surface);
  }

  .footer button.secondary {
    background: transparent;
    color: var(--color-muted);
  }

  .footer button:disabled {
    opacity: 0.5;
  }

  @media (max-width: 800px) {
    .cards {
      grid-template-columns: 1fr;
    }
  }
</style>
