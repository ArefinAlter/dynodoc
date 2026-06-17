import { describe, expect, it } from 'vitest';

import type { ConflictView } from './api/types';
import { ANCESTOR_OPTION_ID, displayValue, toConflictCard } from './conflictCards';

const conflict: ConflictView = {
  conflict_id: 'CID',
  node_id: '01NODEABCDEF',
  field: 'prompt',
  ancestor: 'Old prompt',
  options: [
    { suggestion_id: 'sug-foo', actor: '11111111-2222-3333', value: 'Foo' },
    { suggestion_id: 'sug-bar', value: 'Bar' }
  ]
};

describe('conflictCards: ConflictView → ConflictCard', () => {
  it('leads with the Original (ancestor) card, then one card per proposal', () => {
    const card = toConflictCard(conflict, 3);
    expect(card.id).toBe('CID');
    expect(card.remaining).toBe(3);
    expect(card.itemLabel).toContain('prompt');

    // First card is the read-only Original carrying the ancestor value.
    expect(card.options[0].id).toBe(ANCESTOR_OPTION_ID);
    expect(card.options[0].label).toBe('Original');
    expect(card.options[0].text).toBe('Old prompt');

    // Then one option per proposal, keyed by suggestion id.
    expect(card.options[1].id).toBe('sug-foo');
    expect(card.options[1].text).toBe('Foo');
    expect(card.options[2].id).toBe('sug-bar');
    expect(card.options[2].text).toBe('Bar');
    // The actor (when present) is surfaced in the label.
    expect(card.options[1].label).toContain('11111111');
  });

  it('renders empty and non-string values readably', () => {
    expect(displayValue(null)).toBe('(empty)');
    expect(displayValue(undefined)).toBe('(empty)');
    expect(displayValue('text')).toBe('text');
    expect(displayValue(42)).toBe('42');
    expect(displayValue({ a: 1 })).toBe('{"a":1}');
  });
});
