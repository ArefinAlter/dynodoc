import { describe, expect, it } from 'vitest';

import type { EventPayload, EventRow } from './api/types';
import { applyPayload, emptyState, fold } from './materializer';

/** Build a minimal event row carrying `payload` at `seq`. */
function event(seq: number, payload: EventPayload): EventRow {
  return {
    id: `01TEST${String(seq).padStart(8, '0')}`,
    document_id: '00000000-0000-0000-0000-000000000000',
    seq,
    type: payload.type,
    actor_id: '00000000-0000-0000-0000-000000000000',
    target_node_id: null,
    payload,
    content_hash: [],
    prev_chain_hash: [],
    chain_hash: [],
    created_at: '2026-06-17T00:00:00Z'
  };
}

describe('materializer reducer (mirrors engine-core/src/materializer.rs)', () => {
  it('NodeCreated inserts a node with object current_fields', () => {
    const s = emptyState();
    applyPayload(s, {
      type: 'NodeCreated',
      node_id: 'n1',
      node_type: 'item',
      parent_id: null,
      pos: 'a0',
      fields: { label: 'Q1' },
      var_name: 'q_1'
    });
    expect(s.nodes['n1']).toEqual({
      id: 'n1',
      parent_id: null,
      type: 'item',
      pos: 'a0',
      current_fields: { label: 'Q1' },
      var_name: 'q_1',
      deleted: false
    });
  });

  it('NodeCreated wraps non-object fields under "value" (object_or_value)', () => {
    const s = emptyState();
    applyPayload(s, {
      type: 'NodeCreated',
      node_id: 'n1',
      node_type: 'item',
      pos: 'a0',
      fields: 'plain string'
    });
    expect(s.nodes['n1'].current_fields).toEqual({ value: 'plain string' });
  });

  it('NodeCreated with absent fields yields { value: null }', () => {
    const s = emptyState();
    applyPayload(s, { type: 'NodeCreated', node_id: 'n1', node_type: 'section', pos: 'a0' });
    expect(s.nodes['n1'].current_fields).toEqual({ value: null });
    expect(s.nodes['n1'].var_name).toBeNull();
  });

  it('FieldEdited sets current_fields[field]', () => {
    const s = fold(emptyState(), [
      event(1, { type: 'NodeCreated', node_id: 'n1', node_type: 'item', pos: 'a0', fields: {} }),
      event(2, { type: 'FieldEdited', node_id: 'n1', field: 'label', value: 'Hello' })
    ]);
    expect(s.nodes['n1'].current_fields).toEqual({ label: 'Hello' });
  });

  it('FieldEdited on a missing node is a no-op', () => {
    const s = emptyState();
    expect(applyPayload(s, { type: 'FieldEdited', node_id: 'ghost', field: 'x', value: 1 })).toBe(
      false
    );
    expect(s.nodes['ghost']).toBeUndefined();
  });

  it('NodeMoved updates parent_id and pos', () => {
    const s = fold(emptyState(), [
      event(1, { type: 'NodeCreated', node_id: 'n1', node_type: 'item', pos: 'a0' }),
      event(2, { type: 'NodeMoved', node_id: 'n1', new_parent_id: 'sec1', new_pos: 'b5' })
    ]);
    expect(s.nodes['n1'].parent_id).toBe('sec1');
    expect(s.nodes['n1'].pos).toBe('b5');
  });

  it('NodeDeleted tombstones the node (no row removal)', () => {
    const s = fold(emptyState(), [
      event(1, { type: 'NodeCreated', node_id: 'n1', node_type: 'item', pos: 'a0' }),
      event(2, { type: 'NodeDeleted', node_id: 'n1' })
    ]);
    expect(s.nodes['n1'].deleted).toBe(true);
  });

  it('ChoiceAdded inserts a choice node parented to the item, clearing removed', () => {
    const s = fold(emptyState(), [
      event(1, { type: 'NodeCreated', node_id: 'item1', node_type: 'item', pos: 'a0' }),
      event(2, { type: 'ChoiceRemoved', node_id: 'item1', choice_id: 'c1' }),
      event(3, { type: 'ChoiceAdded', node_id: 'item1', choice_id: 'c1', fields: { label: 'Yes' } })
    ]);
    expect(s.nodes['c1']).toMatchObject({
      id: 'c1',
      parent_id: 'item1',
      type: 'choice',
      pos: 'c1',
      current_fields: { label: 'Yes' }
    });
    expect(s.removed_choices).not.toContain('c1');
  });

  it('ChoiceAdded does not overwrite an existing choice (or_insert)', () => {
    const s = fold(emptyState(), [
      event(1, { type: 'ChoiceAdded', node_id: 'item1', choice_id: 'c1', fields: { label: 'A' } }),
      event(2, { type: 'ChoiceAdded', node_id: 'item1', choice_id: 'c1', fields: { label: 'B' } })
    ]);
    expect(s.nodes['c1'].current_fields).toEqual({ label: 'A' });
  });

  it('ChoiceRemoved tombstones into removed_choices (deduped)', () => {
    const s = fold(emptyState(), [
      event(1, { type: 'ChoiceRemoved', node_id: 'item1', choice_id: 'c1' }),
      event(2, { type: 'ChoiceRemoved', node_id: 'item1', choice_id: 'c1' })
    ]);
    expect(s.removed_choices).toEqual(['c1']);
  });

  it('CommentAdded appends a comment', () => {
    const s = emptyState();
    applyPayload(s, { type: 'CommentAdded', node_id: 'n1', body: 'looks good' });
    applyPayload(s, { type: 'CommentAdded', body: 'doc-level note' });
    expect(s.comments).toEqual([
      { node_id: 'n1', body: 'looks good' },
      { node_id: null, body: 'doc-level note' }
    ]);
  });

  it('SuggestionProposed/Accepted update the suggestions map', () => {
    const s = fold(emptyState(), [
      event(1, {
        type: 'SuggestionProposed',
        suggestion_id: 's1',
        target_node_id: 'n1',
        detail: { type: 'FieldEdited', node_id: 'n1', field: 'label', value: 'New' }
      }),
      event(2, { type: 'SuggestionAccepted', suggestion_id: 's1' })
    ]);
    expect(s.suggestions['s1']).toMatchObject({
      target_node_id: 'n1',
      accepted: true,
      rejected_reason: null
    });
  });

  it('SuggestionRejected records the reason and clears accepted', () => {
    const s = fold(emptyState(), [
      event(1, { type: 'SuggestionProposed', suggestion_id: 's1', detail: {} }),
      event(2, { type: 'SuggestionRejected', suggestion_id: 's1', reason: 'out of scope' })
    ]);
    expect(s.suggestions['s1']).toMatchObject({
      accepted: false,
      rejected_reason: 'out of scope'
    });
  });

  it('SuggestionAccepted on a missing suggestion is a no-op', () => {
    const s = emptyState();
    expect(applyPayload(s, { type: 'SuggestionAccepted', suggestion_id: 'ghost' })).toBe(false);
  });

  it('Deployed is a no-op for the materialized tree', () => {
    const s = emptyState();
    expect(applyPayload(s, { type: 'Deployed', snapshot_id: 'snap1' })).toBe(false);
    expect(s.nodes).toEqual({});
  });

  it('folds a full sequence into the expected state', () => {
    const s = fold(emptyState(), [
      event(1, {
        type: 'NodeCreated',
        node_id: 'sec1',
        node_type: 'section',
        pos: 'a0',
        fields: { title: 'Intro' }
      }),
      event(2, {
        type: 'NodeCreated',
        node_id: 'q1',
        node_type: 'item',
        parent_id: 'sec1',
        pos: 'a1',
        fields: { label: 'How are you?' },
        var_name: 'mood'
      }),
      event(3, { type: 'FieldEdited', node_id: 'q1', field: 'hint', value: 'Be honest' }),
      event(4, { type: 'ChoiceAdded', node_id: 'q1', choice_id: 'c1', fields: { label: 'Good' } })
    ]);
    expect(Object.keys(s.nodes).sort()).toEqual(['c1', 'q1', 'sec1']);
    expect(s.nodes['q1'].current_fields).toEqual({ label: 'How are you?', hint: 'Be honest' });
    expect(s.nodes['c1'].parent_id).toBe('q1');
  });

  it('fold does not mutate the base state', () => {
    const base = emptyState();
    fold(base, [event(1, { type: 'NodeCreated', node_id: 'n1', node_type: 'item', pos: 'a0' })]);
    expect(base.nodes).toEqual({});
  });
});
