import { describe, expect, it } from 'vitest';

import type { DocumentDetail, DocumentState, MaterializedNode } from './api/types';
import { toEditorDocument } from './adapter';

function node(
  over: Partial<MaterializedNode> & { id: string; type: MaterializedNode['type'] }
): MaterializedNode {
  return {
    parent_id: null,
    pos: 'a0',
    current_fields: {},
    var_name: null,
    deleted: false,
    ...over
  };
}

function detail(state: DocumentState, over: Partial<DocumentDetail> = {}): DocumentDetail {
  return {
    id: 'doc-1',
    title: 'Test Document',
    languages: ['en'],
    status: 'draft',
    settings: {},
    deployed_snapshot_id: null,
    created_by: '11111111-2222-3333-4444-555555555555',
    created_at: '2026-06-17T00:00:00Z',
    updated_at: '2026-06-17T00:00:00Z',
    state,
    latest_seq: 0,
    ...over
  };
}

describe('adapter: DocumentState → EditorDocument', () => {
  it('folds sections, items, and choices in pos order', () => {
    const state: DocumentState = {
      nodes: {
        sec1: node({
          id: 'sec1',
          type: 'section',
          pos: 'a0',
          current_fields: { title: 'Screening' }
        }),
        q2: node({
          id: 'q2',
          type: 'item',
          parent_id: 'sec1',
          pos: 'b1',
          current_fields: { label: 'Second', type: 'integer' }
        }),
        q1: node({
          id: 'q1',
          type: 'item',
          parent_id: 'sec1',
          pos: 'b0',
          current_fields: { label: 'First', hint: 'be honest', relevance: '${x} > 0' },
          var_name: 'first'
        }),
        c1: node({
          id: 'c1',
          type: 'choice',
          parent_id: 'q1',
          pos: 'c0',
          current_fields: { label: 'Yes' }
        }),
        c0: node({
          id: 'c0',
          type: 'choice',
          parent_id: 'q1',
          pos: 'c1',
          current_fields: { label: 'No' }
        })
      },
      comments: [{ node_id: 'q1', body: 'reword' }],
      suggestions: {
        s1: { target_node_id: 'q1', detail: {}, accepted: false, rejected_reason: null }
      },
      removed_choices: []
    };

    const doc = toEditorDocument(detail(state));
    expect(doc.title).toBe('Test Document');
    expect(doc.sections).toHaveLength(1);

    const section = doc.sections[0];
    expect(section.title).toBe('Screening');
    expect(section.number).toBe('1');
    expect(section.items.map((i) => i.id)).toEqual(['q1', 'q2']); // pos order

    const item1 = section.items[0];
    expect(item1.number).toBe('1.1');
    expect(item1.text).toBe('First');
    expect(item1.hint).toBe('be honest');
    expect(item1.relevance).toBe('${x} > 0');
    expect(item1.varName).toBe('first');
    expect(item1.commentsCount).toBe(1);
    expect(item1.suggestionsCount).toBe(1);
    // choices sorted by pos: c1 (c0) before c0 (c1)
    expect(item1.choices?.map((c) => c.id)).toEqual(['c1', 'c0']);
    expect(item1.choices?.map((c) => c.label)).toEqual(['Yes', 'No']);

    const item2 = section.items[1];
    expect(item2.type).toBe('integer');
    expect(item2.choices).toBeUndefined();
  });

  it('drops deleted nodes and removed choices', () => {
    const state: DocumentState = {
      nodes: {
        sec1: node({ id: 'sec1', type: 'section', current_fields: { title: 'S' } }),
        q1: node({ id: 'q1', type: 'item', parent_id: 'sec1', current_fields: { label: 'Q' } }),
        qdel: node({ id: 'qdel', type: 'item', parent_id: 'sec1', deleted: true }),
        c1: node({ id: 'c1', type: 'choice', parent_id: 'q1', current_fields: { label: 'keep' } }),
        c2: node({ id: 'c2', type: 'choice', parent_id: 'q1', current_fields: { label: 'gone' } })
      },
      comments: [],
      suggestions: {},
      removed_choices: ['c2']
    };

    const doc = toEditorDocument(detail(state));
    expect(doc.sections[0].items.map((i) => i.id)).toEqual(['q1']);
    expect(doc.sections[0].items[0].choices?.map((c) => c.id)).toEqual(['c1']);
  });

  it('gathers orphan items under a synthetic Ungrouped section', () => {
    const state: DocumentState = {
      nodes: {
        loose: node({
          id: 'loose',
          type: 'item',
          parent_id: null,
          current_fields: { label: 'Orphan' }
        })
      },
      comments: [],
      suggestions: {},
      removed_choices: []
    };

    const doc = toEditorDocument(detail(state));
    expect(doc.sections).toHaveLength(1);
    expect(doc.sections[0].title).toBe('Ungrouped');
    expect(doc.sections[0].items[0].text).toBe('Orphan');
  });

  it('only counts pending suggestions in the gutter', () => {
    const state: DocumentState = {
      nodes: {
        sec1: node({ id: 'sec1', type: 'section', current_fields: { title: 'S' } }),
        q1: node({ id: 'q1', type: 'item', parent_id: 'sec1', current_fields: { label: 'Q' } })
      },
      comments: [],
      suggestions: {
        accepted: { target_node_id: 'q1', detail: {}, accepted: true, rejected_reason: null },
        rejected: { target_node_id: 'q1', detail: {}, accepted: false, rejected_reason: 'no' },
        pending: { target_node_id: 'q1', detail: {}, accepted: false, rejected_reason: null }
      },
      removed_choices: []
    };

    const doc = toEditorDocument(detail(state));
    expect(doc.sections[0].items[0].suggestionsCount).toBe(1);
  });

  it('falls back to en languages and capitalizes status', () => {
    const doc = toEditorDocument(
      detail(
        { nodes: {}, comments: [], suggestions: {}, removed_choices: [] },
        { languages: [], status: 'deployed' }
      )
    );
    expect(doc.languages).toEqual(['en']);
    expect(doc.status).toBe('Deployed');
  });
});
