import { describe, expect, it } from 'vitest';
import { parseStarred, starredUnavailable } from './profile';

// Synthetic values only. Shape mirrors docs/DEVICE-KEY-GRANTS-API.md (GET /profiles/{id}/starred).
describe('parseStarred', () => {
  it('reads id, name and both counts, and the truncation flag', () => {
    const parsed = parseStarred({
      items: [
        { id: 'a', name: 'Board packs', star_count: 3, open_issue_count: 4 },
        { id: 'b', name: 'Launch plan', star_count: 1, open_issue_count: 0 }
      ],
      truncated: true
    });
    expect(parsed.truncated).toBe(true);
    expect(parsed.items).toEqual([
      { id: 'a', name: 'Board packs', starCount: 3, openIssueCount: 4 },
      { id: 'b', name: 'Launch plan', starCount: 1, openIssueCount: 0 }
    ]);
  });
  it('skips rows it cannot read and treats a bad body as empty', () => {
    expect(
      parseStarred({ items: [{ name: 'No id' }, { id: 'c', name: 'Ok', star_count: -2 }] }).items
    ).toEqual([{ id: 'c', name: 'Ok', starCount: 0, openIssueCount: 0 }]);
    expect(parseStarred(null)).toEqual({ items: [], truncated: false });
    expect(parseStarred({ items: 'nope' }).items).toEqual([]);
  });
});

describe('starredUnavailable', () => {
  it('hides the section only for a server without the route', () => {
    expect(starredUnavailable(Object.assign(new Error('x'), { status: 404 }))).toBe(true);
    expect(starredUnavailable(Object.assign(new Error('x'), { status: 501 }))).toBe(true);
    expect(starredUnavailable(Object.assign(new Error('x'), { status: 500 }))).toBe(false);
    expect(starredUnavailable(new TypeError('Failed to fetch'))).toBe(false);
  });
});
