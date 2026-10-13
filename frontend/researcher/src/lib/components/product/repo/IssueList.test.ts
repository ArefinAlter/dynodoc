import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import IssueList from './IssueList.svelte';

// Synthetic values only. Shapes mirror docs/PROJECT-ISSUES-API.md.
const PROJECT = '22222222-2222-4222-8222-222222222222';
const MAYA = { id: 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa', name: 'Maya Chen' };
const LABEL = {
  id: 'cccccccc-cccc-4ccc-8ccc-cccccccccccc',
  name: 'Needs evidence',
  color: 'd93f0b',
  description: ''
};
const issue = (number: number, extra: Record<string, unknown> = {}) => ({
  id: `00000000-0000-4000-8000-${String(number).padStart(12, '0')}`,
  number,
  title: `Issue number ${number}`,
  state: 'open',
  author: MAYA,
  assignee: null,
  labels: [LABEL],
  comment_count: 0,
  created_at: '2026-10-12T09:00:00Z',
  updated_at: '2026-10-12T09:00:00Z',
  closed_at: null,
  ...extra
});
const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

let target: HTMLElement;
let component: Record<string, unknown> | null = null;
afterEach(() => {
  if (component) unmount(component);
  component = null;
  target?.remove();
  vi.unstubAllGlobals();
});
async function render(routes: (url: URL) => Response, props: Record<string, unknown> = {}) {
  const fetchMock = vi.fn(async (input: RequestInfo | URL) =>
    routes(new URL(String(input), 'http://localhost'))
  );
  vi.stubGlobal('fetch', fetchMock);
  target = document.createElement('div');
  document.body.appendChild(target);
  component = mount(IssueList as never, {
    target,
    props: { projectId: PROJECT, role: 'editor', members: [], ...props }
  }) as Record<string, unknown>;
  flushSync();
  for (let i = 0; i < 10; i++) await tick();
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
  return fetchMock;
}
const labels = () => json({ items: [{ ...LABEL, open_issue_count: 2 }], can_manage: false });

describe('IssueList', () => {
  it('lists open issues with labels and totals, and reports the open count', async () => {
    const onCounts = vi.fn();
    const fetchMock = await render(
      (url) =>
        url.pathname.endsWith('/labels')
          ? labels()
          : json({
              items: [issue(2), issue(1, { comment_count: 3 })],
              next_before: null,
              counts: { open: 2, closed: 5 }
            }),
      { onCounts }
    );
    const urls = fetchMock.mock.calls.map((call) => String(call[0]));
    expect(urls.some((u) => u.includes(`/projects/${PROJECT}/issues?state=open&limit=30`))).toBe(
      true
    );
    expect(target.querySelectorAll('.iss-row')).toHaveLength(2);
    expect(target.textContent).toContain('Issue number 2');
    expect(target.textContent).toContain('Needs evidence');
    expect(target.textContent).toContain('#2 opened');
    expect(target.querySelector('a.iss-link')?.getAttribute('href')).toBe(
      `/projects/${PROJECT}?tab=issues&issue=2`
    );
    expect(onCounts).toHaveBeenCalledWith({ open: 2, closed: 5 });
    expect(target.querySelector('[role=alert],[role=status]')).toBeNull();
    const toggles = [...target.querySelectorAll('.repo-seg button')];
    expect(toggles.map((b) => b.getAttribute('aria-pressed'))).toEqual(['true', 'false']);
    expect(
      [...target.querySelectorAll('button')].some((b) => b.textContent?.includes('New issue'))
    ).toBe(true);
  });

  it('asks for the next page with the number the server gave', async () => {
    let calls = 0;
    const fetchMock = await render((url) => {
      if (url.pathname.endsWith('/labels')) return labels();
      calls++;
      return calls === 1
        ? json({ items: [issue(5), issue(4)], next_before: 4, counts: { open: 4, closed: 0 } })
        : json({ items: [issue(3), issue(2)], next_before: null, counts: { open: 4, closed: 0 } });
    });
    const more = [...target.querySelectorAll('button')].find((b) => b.textContent === 'Show more');
    expect(more).toBeTruthy();
    more?.click();
    for (let i = 0; i < 10; i++) await tick();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
    const urls = fetchMock.mock.calls.map((call) => String(call[0]));
    expect(urls.some((u) => u.includes('before=4'))).toBe(true);
    expect(target.querySelectorAll('.iss-row')).toHaveLength(4);
    expect([...target.querySelectorAll('button')].some((b) => b.textContent === 'Show more')).toBe(
      false
    );
  });

  it('keeps viewers read-only and says so in the empty state', async () => {
    await render(
      (url) =>
        url.pathname.endsWith('/labels')
          ? labels()
          : json({ items: [], next_before: null, counts: { open: 0, closed: 0 } }),
      { role: 'viewer' }
    );
    expect(target.textContent).toContain('No issues yet');
    expect(target.textContent?.replace(/\s+/g, ' ')).toContain(
      'can read issues but cannot open them'
    );
    expect(
      [...target.querySelectorAll('button')].some((b) => b.textContent?.includes('New issue'))
    ).toBe(false);
  });

  it('shows a calm note, not an error, when the server has no issue routes', async () => {
    await render(() => json({ error: 'not found' }, 404));
    expect(target.textContent).toContain('Issues are not available on this server');
    expect(target.querySelector('[role=alert],[role=status]')).toBeNull();
    expect(target.querySelector('.iss-row')).toBeNull();
  });

  it('turns a failed read into a short message with a retry', async () => {
    await render((url) =>
      url.pathname.endsWith('/labels')
        ? labels()
        : json({ error: 'select * from secrets', details: { reason: 'x' } }, 429)
    );
    expect(target.textContent).toContain('Too many requests');
    expect(target.textContent).not.toContain('secrets');
    expect([...target.querySelectorAll('button')].some((b) => b.textContent === 'Try again')).toBe(
      true
    );
  });
});
