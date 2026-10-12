import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import ProjectBranches from './ProjectBranches.svelte';

const AUTHORITY = '11111111-1111-4111-8111-111111111111';
const PROJECT = '22222222-2222-4222-8222-222222222222';
const hex = (digit: string) => digit.repeat(64);
const receipt = (branch: string, commit: string, generation: number) => ({
  receipt: {
    version: 1,
    scope: { authority: AUTHORITY, project: PROJECT, branch },
    head: { commit, generation },
    previous: generation > 1 ? hex('9') : null,
    issued_at_ms: 1_760_270_400_000,
    issuer_key: hex('a')
  },
  signature: hex('b') + hex('c')
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
  component = mount(ProjectBranches as never, {
    target,
    props: { projectId: PROJECT, requiredApprovals: 1, ...props }
  }) as Record<string, unknown>;
  flushSync();
  for (let i = 0; i < 10; i++) await tick();
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
  return fetchMock;
}

describe('ProjectBranches', () => {
  it('explains shared history when nothing has been published, without dead links', async () => {
    const fetchMock = await render(() => json({ items: [], next: null, receipts: [] }));
    expect(String(fetchMock.mock.calls[0][0])).toContain(`/projects/${PROJECT}/history/refs`);
    expect(target.textContent).toContain('No shared history yet');
    expect(target.textContent).toContain('engine-cli sync');
    expect(target.querySelector('table')).toBeNull();
    expect(target.querySelector('a')).toBeNull();
    expect(target.querySelector('[role=alert],[role=status]')).toBeNull();
  });

  it('lists branches and shows receipt, commit and reviews for one branch, never as verified', async () => {
    const head = hex('1');
    const fetchMock = await render(
      (url) => {
        if (url.pathname.endsWith('/history/refs'))
          return json({
            items: [{ branch: 'main', head: { commit: head, generation: 4 } }],
            next: 'main',
            receipts: [receipt('main', head, 4)]
          });
        if (url.pathname.endsWith(`/history/commits/${head}`))
          return json({
            commit: {
              version: 1,
              project: PROJECT,
              parents: [hex('2')],
              files: { 'Q3 budget.xlsx': { document: AUTHORITY, state: hex('3') } },
              author_key: hex('d'),
              authored_at_ms: 1_760_270_000_000,
              message: 'Update the Q3 budget'
            },
            signature: hex('e') + hex('f')
          });
        if (url.pathname.endsWith('/history/reviews'))
          return json({
            items: [
              {
                id: 7,
                branch: 'main',
                expected_generation: 3,
                expected_commit: hex('2'),
                reviewer: 'a9f1c2d3-0000-4000-8000-000000000001',
                decision: 'approve',
                created_at: '2026-10-12T10:00:00+00:00'
              }
            ],
            next: 7
          });
        return json({ error: 'not found' }, 404);
      },
      {
        members: [
          {
            id: 'a9f1c2d3-0000-4000-8000-000000000001',
            name: 'Priya Nair',
            email: 'priya@example.test',
            role: 'approver'
          }
        ]
      }
    );
    const rows = target.querySelectorAll('tbody tr');
    expect(rows).toHaveLength(1);
    expect(rows[0].textContent).toContain('main');
    expect(rows[0].textContent).toContain('Default');
    expect(rows[0].textContent).toContain('Protected');
    expect(rows[0].textContent).toContain('Available');
    const toggle = rows[0].querySelector<HTMLButtonElement>('button[aria-expanded]')!;
    expect(toggle.getAttribute('aria-label')).toBe('Details for main');
    toggle.click();
    flushSync();
    for (let i = 0; i < 10; i++) await tick();
    await new Promise((resolve) => setTimeout(resolve, 0));
    flushSync();
    expect(toggle.getAttribute('aria-expanded')).toBe('true');
    const panel = target.querySelector('#branch-details')!;
    expect(panel.textContent).toContain('Latest signed receipt');
    expect(panel.textContent).toContain(hex('a'));
    expect(panel.textContent).toContain('Update the Q3 budget');
    expect(panel.textContent).toContain('Priya Nair approved');
    expect(panel.textContent).toContain('1 of 1 required approval');
    expect(panel.textContent).toMatch(/does not check the signature/);
    expect(panel.textContent).not.toMatch(/\bverified\b/i);
    const paths = fetchMock.mock.calls.map(
      (c) => new URL(String(c[0]), 'http://localhost').pathname
    );
    expect(paths.some((p) => p.endsWith(`/history/commits/${head}`))).toBe(true);
    expect(paths.some((p) => p.endsWith('/history/reviews'))).toBe(true);
    // Hide again: no further reads.
    toggle.click();
    flushSync();
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
  });

  it('says why a list cannot be shown instead of echoing the server', async () => {
    await render(() =>
      json(
        { error: 'forbidden', code: 'forbidden', details: { reason: 'secret internal text' } },
        403
      )
    );
    expect(target.textContent).toContain('do not have access');
    expect(target.textContent).not.toContain('secret internal text');
    expect(target.querySelector('[role=alert]')).toBeNull();
  });
});
