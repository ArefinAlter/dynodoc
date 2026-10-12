import { describe, expect, it } from 'vitest';
import { formatBreakdown, formatOf, groupByDay, hashHex, relativeTime, shortHash } from './repo-ui';
import { buildHeatGrid, daysFromActivity, kindLabel, mapPool } from './activity';

const NOW = Date.parse('2026-10-12T12:00:00');

describe('relativeTime', () => {
  it('uses plain units', () => {
    expect(relativeTime('2026-10-12T11:59:50', NOW)).toBe('just now');
    expect(relativeTime('2026-10-12T11:55:00', NOW)).toBe('5 minutes ago');
    expect(relativeTime('2026-10-12T09:00:00', NOW)).toBe('3 hours ago');
    expect(relativeTime('2026-10-11T12:00:00', NOW)).toBe('yesterday');
    expect(relativeTime('2026-10-09T12:00:00', NOW)).toBe('3 days ago');
    expect(relativeTime('2026-07-12T12:00:00', NOW)).toBe('3 months ago');
    expect(relativeTime('2024-10-12T12:00:00', NOW)).toBe('2 years ago');
    expect(relativeTime('not a date', NOW)).toBe('');
  });
});

describe('groupByDay', () => {
  it('keeps order and labels recent days', () => {
    const now = new Date('2026-10-12T12:00:00');
    const groups = groupByDay(
      [
        { at: '2026-10-12T10:00:00' },
        { at: '2026-10-12T08:00:00' },
        { at: '2026-10-11T23:00:00' },
        { at: '2026-09-01T09:00:00' }
      ],
      (x) => x.at,
      now
    );
    expect(groups.map((g) => [g.label, g.items.length])).toEqual([
      ['Today', 2],
      ['Yesterday', 1],
      [expect.stringContaining('2026'), 1]
    ]);
  });
});

describe('formats and hashes', () => {
  it('maps kinds and shares', () => {
    expect(formatOf('document').key).toBe('word');
    expect(formatOf('questionnaire').key).toBe('other');
    const shares = formatBreakdown([
      { kind: 'document' },
      { kind: 'document' },
      { kind: 'spreadsheet' },
      { kind: 'questionnaire' }
    ]);
    expect(shares.map((s) => [s.format.key, s.count])).toEqual([
      ['word', 2],
      ['excel', 1],
      ['other', 1]
    ]);
    expect(shares[0].percent).toBe(50);
  });
  it('hex encodes byte hashes', () => {
    expect(hashHex([0, 15, 255])).toBe('000fff');
    expect(shortHash('abcdef0123456789')).toBe('abcdef0');
    expect(hashHex(null)).toBe('');
  });
});

describe('activity helpers', () => {
  it('labels events and unknown kinds', () => {
    expect(kindLabel('FieldEdited')).toBe('Edited content');
    expect(kindLabel('change_request_merged')).toBe('Merged a pull request');
    expect(kindLabel('document.moved')).toBe('Document moved');
  });
  it('builds a Sunday-first year grid with levels', () => {
    const grid = buildHeatGrid(
      2025,
      [
        { date: '2025-03-04', count: 2 },
        { date: '2025-03-05', count: 8 }
      ],
      new Date('2026-01-01T00:00:00')
    );
    expect(grid.weeks.every((w) => w.length === 7)).toBe(true);
    expect(grid.weeks.length).toBeGreaterThanOrEqual(52);
    expect(grid.total).toBe(10);
    const cells = grid.weeks.flat();
    expect(cells.find((c) => c.date === '2025-03-05')?.level).toBe(4);
    expect(cells.find((c) => c.date === '2025-03-04')?.level).toBe(1);
    expect(grid.months[0].label).toBeTruthy();
  });
  it('counts only the named actor in the chosen year', () => {
    const days = daysFromActivity(
      [
        { at: '2026-02-03T10:00:00', actor_name: 'Maya Chen' },
        { at: '2026-02-03T11:00:00', actor_name: 'Maya Chen' },
        { at: '2026-02-03T12:00:00', actor_name: 'Sam Okoye' },
        { at: '2025-02-03T12:00:00', actor_name: 'Maya Chen' }
      ].map((x) => ({
        ...x,
        kind: '',
        document_id: null,
        document_title: null,
        summary: null,
        seq: null
      })),
      2026,
      'Maya Chen'
    );
    expect(days).toEqual([{ date: '2026-02-03', count: 2 }]);
  });
  it('limits concurrency and tolerates failures', async () => {
    let active = 0;
    let peak = 0;
    const out = await mapPool([1, 2, 3, 4, 5], 2, async (n) => {
      active++;
      peak = Math.max(peak, active);
      await new Promise((r) => setTimeout(r, 5));
      active--;
      if (n === 3) throw new Error('boom');
      return n * 2;
    });
    expect(out).toEqual([2, 4, null, 8, 10]);
    expect(peak).toBeLessThanOrEqual(2);
  });
});
