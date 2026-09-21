import { describe, expect, it } from 'vitest';
import fixtures from './richtext-fixtures.json';
import { applyRichTextPatch, describeRichTextPatch, type RichTextPatch } from './richtext';
import { applyPayload, emptyState } from '$lib/materializer';

describe('canonical rich-text event replay', () => {
  it('accepts reactive document proxies without mutating the base', () => {
    const fixture = fixtures[0];
    const base = new Proxy(fixture.before, {});
    expect(applyRichTextPatch(base, fixture.patch as RichTextPatch)).toEqual(fixture.after);
    expect(fixture.before.content[0]).toEqual({ type: 'text', text: 'বাংলা 👩🏽‍🔬 e\u0301 العربية' });
  });
  for (const [i, fixture] of fixtures.entries()) {
    it(`matches the Rust replay fixture ${i + 1}`, () => {
      expect(applyRichTextPatch(fixture.before, fixture.patch as RichTextPatch)).toEqual(
        fixture.after
      );
    });
  }
  it('updates the derived label with wording and formatting', () => {
    const fixture = fixtures[0];
    const state = emptyState();
    applyPayload(state, {
      type: 'NodeCreated',
      node_id: 'p',
      node_type: 'item',
      pos: 'a',
      fields: { content: fixture.before, label: 'Old' }
    });
    applyPayload(state, {
      type: 'RichTextPatched',
      node_id: 'p',
      patch: fixture.patch as RichTextPatch
    });
    expect(state.nodes.p.current_fields.label).toBe('বাংলা 👩🏽‍🔬 revised e\u0301 العربية');
    expect(describeRichTextPatch(fixture.patch as RichTextPatch)).toBe(
      'Wording and formatting updated'
    );
  });
  it('rejects malformed changes instead of partially altering the base', () => {
    const fixture = fixtures[0];
    const patch = structuredClone(fixture.patch) as RichTextPatch;
    patch.edits = [
      { kind: 'text', path: [], ops: [{ kind: 'retain', count: Number.MAX_SAFE_INTEGER }] }
    ];
    expect(() => applyRichTextPatch(fixture.before, patch)).toThrow('out of bounds');
    patch.edits = [{ kind: 'text', path: [], ops: [] }];
    expect(() => applyRichTextPatch(fixture.before, patch)).toThrow('Incomplete');
    patch.edits = [{ kind: 'text', path: [400], ops: [] }];
    expect(() => applyRichTextPatch(fixture.before, patch)).toThrow('Missing');
  });
});
