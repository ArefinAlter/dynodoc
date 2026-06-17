<script lang="ts">
  import type { ConflictCard } from '$lib/types/editor';
  import { ANCESTOR_OPTION_ID } from '$lib/conflictCards';

  interface Props {
    conflict: ConflictCard;
    open: boolean;
    selectedOptionId: string;
    onClose: () => void;
    onSelect: (id: string) => void;
    /** Apply the author's decision: accept a proposal, or write a composed custom value. */
    onApply: (selection: { accept_suggestion_id?: string; custom_value?: string }) => void;
  }

  let { conflict, open, selectedOptionId, onClose, onSelect, onApply }: Props = $props();

  const COMPOSE_ID = '__compose__';

  let composeValue = $state('');
  let applying = $state(false);

  // "Apply" is enabled for any real proposal selection, or Compose with non-empty text.
  // The read-only Original card cannot be applied (keeping the value needs no resolution).
  const canApply = $derived(
    (selectedOptionId === COMPOSE_ID && composeValue.trim().length > 0) ||
      (selectedOptionId !== '' &&
        selectedOptionId !== COMPOSE_ID &&
        selectedOptionId !== ANCESTOR_OPTION_ID)
  );

  async function apply() {
    if (!canApply || applying) return;
    applying = true;
    try {
      if (selectedOptionId === COMPOSE_ID) {
        await onApply({ custom_value: composeValue });
      } else {
        await onApply({ accept_suggestion_id: selectedOptionId });
      }
      composeValue = '';
    } finally {
      applying = false;
    }
  }

  function handleOverlayKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
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
  <div
    class="overlay"
    role="button"
    tabindex="0"
    aria-label="Close conflict dialog"
    onclick={handleOverlayClick}
    onkeydown={handleOverlayKeydown}
  >
    <div class="modal" role="dialog" aria-modal="true" tabindex="-1">
      <div class="overline">{conflict.remaining} conflicts pending · {conflict.itemLabel}</div>
      <h2>Four-card conflict resolution</h2>
      <p>
        Two or more pending suggestions edit this field to different values. Choose the proposal to
        keep — the others are rejected — or compose a new value.
      </p>

      <div class="cards">
        {#each conflict.options as option}
          <button
            type="button"
            class:selected={selectedOptionId === option.id}
            class:readonly={option.id === ANCESTOR_OPTION_ID}
            class="card"
            onclick={() => onSelect(option.id)}
          >
            <div class="label">{option.label}</div>
            <div class="text">{option.text}</div>
          </button>
        {/each}

        <button
          type="button"
          class:selected={selectedOptionId === COMPOSE_ID}
          class="card"
          onclick={() => onSelect(COMPOSE_ID)}
        >
          <div class="label">Compose new</div>
          <input
            type="text"
            placeholder="Enter a custom value…"
            bind:value={composeValue}
            onfocus={() => onSelect(COMPOSE_ID)}
            onclick={(e) => e.stopPropagation()}
          />
        </button>
      </div>

      <div class="footer">
        <button type="button" class="secondary" onclick={onClose}>Close</button>
        <button type="button" disabled={!canApply || applying} onclick={apply}>
          Apply selection
        </button>
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

  .card.readonly {
    opacity: 0.85;
  }

  .label {
    color: var(--color-accent);
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    margin-bottom: 0.45rem;
  }

  .card input {
    width: 100%;
    margin-top: 0.2rem;
    padding: 0.45rem 0.55rem;
    border: 1px solid var(--color-border);
    border-radius: 0.5rem;
    font: inherit;
    background: var(--color-surface);
    color: inherit;
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
