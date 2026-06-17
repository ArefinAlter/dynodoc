/**
 * Map an API {@link ConflictView} into the scaffold's {@link ConflictCard} view model so
 * the existing four-card {@link ConflictModal} renders real data.
 *
 * Cards, in order: the "Original" (ancestor) card, then one card per competing proposal
 * (labelled by proposer / suggestion id). Selecting a proposal card resolves by accepting
 * that suggestion; the modal additionally offers a "Compose new" custom-value field that
 * resolves with `custom_value`. The ancestor card is informational (selecting it is a
 * no-op — keeping the current value needs no resolution).
 */
import type { ConflictOption, ConflictView } from './api/types';
import type { ConflictCard, ConflictOption as CardOption } from './types/editor';

/** Sentinel option id for the read-only "Original" (ancestor) card. */
export const ANCESTOR_OPTION_ID = '__ancestor__';

/** Render any field value as short display text. */
export function displayValue(value: unknown): string {
  if (value === null || value === undefined) return '(empty)';
  if (typeof value === 'string') return value;
  return JSON.stringify(value);
}

function shortActor(option: ConflictOption): string {
  if (option.actor) return `Proposal · ${option.actor.slice(0, 8)}`;
  return `Proposal · ${option.suggestion_id.slice(0, 8)}`;
}

/**
 * Build a {@link ConflictCard} from a conflict. `remaining` is the total number of
 * unresolved conflicts (shown in the modal overline). The option ids are the
 * `suggestion_id`s (so "Apply selection" can resolve by accepting), plus the ancestor
 * sentinel for the read-only Original card.
 */
export function toConflictCard(conflict: ConflictView, remaining: number): ConflictCard {
  const options: CardOption[] = [
    {
      id: ANCESTOR_OPTION_ID,
      label: 'Original',
      text: displayValue(conflict.ancestor)
    },
    ...conflict.options.map((o) => ({
      id: o.suggestion_id,
      label: shortActor(o),
      text: displayValue(o.value)
    }))
  ];
  return {
    id: conflict.conflict_id,
    itemId: conflict.node_id,
    itemLabel: `${conflict.node_id.slice(0, 8)} · ${conflict.field}`,
    remaining,
    options
  };
}
