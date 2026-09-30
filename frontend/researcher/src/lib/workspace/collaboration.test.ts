import { describe, expect, it } from 'vitest';
import type { EventPayload } from '$lib/api/types';
import {
  describe as describeNotification,
  percent,
  roleName,
  type NotificationItem
} from './collaboration';
import { blockTexts, isCopy, isSameName } from './upload-check';

const node = (fields: Record<string, unknown>, parent: string | null = 'root') =>
  ({
    type: 'NodeCreated',
    node_id: Math.random().toString(36).slice(2),
    node_type: 'item',
    parent_id: parent,
    pos: '0',
    fields
  }) as EventPayload;

describe('upload text for copy detection', () => {
  it('reads paragraphs, spreadsheet rows in column order and slide notes, not the title', () => {
    const texts = blockTexts([
      node({ label: 'Group report' }, null),
      node({ label: 'Response rates were highest in Sylhet.' }),
      node({ content: { type: 'paragraph', content: [{ type: 'text', text: 'From rich text' }] } }),
      node({ cell_10: 'x', cell_0: 'Tariq', cell_1: 12 }),
      node({ label: 'Agenda', notes: 'Say hello' })
    ]);
    expect(texts).toEqual([
      'Response rates were highest in Sylhet.',
      'From rich text',
      'Tariq | 12 | x',
      'Agenda',
      'Say hello'
    ]);
  });
  it('treats embedded ids and copy relations as copies, and name-only matches separately', () => {
    const match = (relation: string, title_match = false, embedded_id = false) =>
      ({
        embedded_id,
        comparison: { relation, title_match }
      }) as Parameters<typeof isCopy>[0];
    expect(isCopy(match('edited_copy'))).toBe(true);
    expect(isCopy(match('unrelated', false, true))).toBe(true);
    expect(isSameName(match('same_name', true))).toBe(true);
    expect(isSameName(match('copy', true))).toBe(false);
  });
});

describe('notification wording', () => {
  const base = {
    id: '1',
    created_at: new Date().toISOString(),
    read: false,
    document: { id: 'd', title: 'Group report', kind: 'document', role: 'owner' },
    actor: { id: 'a', name: 'Tariq', email: 'tariq@example.test' },
    source: null,
    match: null,
    request: null
  } as const;
  it('names the uploader, the file and the similarity of a detected copy', () => {
    const text = describeNotification({
      ...base,
      kind: 'copy_detected',
      payload: { relation: 'edited_copy', score: 0.873 },
      source: {
        id: 's',
        title: 'report',
        kind: 'document',
        role: null,
        filename: 'report - Tariq.docx'
      }
    } as NotificationItem);
    expect(text).toBe(
      'Tariq uploaded “report - Tariq.docx”, which is an edited copy of “Group report” (87% match).'
    );
  });
  it('describes reviews and access in plain words', () => {
    expect(
      describeNotification({
        ...base,
        kind: 'change_request_reviewed',
        payload: { verdict: 'changes_requested' },
        request: { id: 'r', name: 'Fix totals', status: 'open' }
      } as NotificationItem)
    ).toBe('Tariq requested changes to “Fix totals” in “Group report”.');
    expect(
      describeNotification({
        ...base,
        kind: 'access_granted',
        payload: { role: 'reviewer' }
      } as NotificationItem)
    ).toBe('Tariq gave you reviewer access to “Group report”.');
    expect(roleName('contributor')).toBe('Contributor');
    expect(percent(1.4)).toBe('100%');
  });
});
