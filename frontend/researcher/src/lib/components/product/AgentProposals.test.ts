import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import AgentProposals from './AgentProposals.svelte';

// Synthetic values only. Shapes mirror GET /projects/{id}/agent-proposals.
const PROJECT = '22222222-2222-4222-8222-222222222222';
const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
const item = (n: number, extra: Record<string, unknown> = {}) => ({
  id: `33333333-3333-4333-8333-${String(n).padStart(12, '0')}`,
  name: `Proposal ${n}`,
  state: 'open',
  revision: 2,
  base_seq: 5,
  head_seq: 5,
  behind: 0,
  stale_base: false,
  created_at: '2026-10-21T09:00:00Z',
  submitted_at: '2026-10-21T09:05:00Z',
  closed_at: null,
  document: { id: '44444444-4444-4444-8444-444444444444', title: 'Plan', kind: 'document' },
  agent: {
    id: '55555555-5555-4555-8555-555555555555',
    client_kind: 'claude',
    display_name: 'Claude for Olivia',
    principal: { id: '66666666-6666-4666-8666-666666666666', name: 'Olivia' },
    revoked: false
  },
  mine: false,
  edits: 3,
  ops: { proposed: 2, compiled: 3 },
  ...extra
});
const page = (items: unknown[], extra: Record<string, unknown> = {}) => ({
  items,
  more: false,
  offset: 0,
  state: 'active',
  budgets: null,
  ...extra
});

let target: HTMLElement;
let component: Record<string, unknown> | null = null;
afterEach(() => {
  if (component) unmount(component);
  component = null;
  target?.remove();
  vi.unstubAllGlobals();
});
async function settle() {
  for (let i = 0; i < 8; i++) await tick();
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}
async function render(
  respond: (path: string) => Response | undefined,
  props: Record<string, unknown> = {}
) {
  const paths: string[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL) => {
      const path = String(input).replace('/api/engine', '');
      paths.push(path);
      return respond(path) ?? json(page([]));
    })
  );
  target = document.createElement('div');
  document.body.appendChild(target);
  component = mount(AgentProposals as never, {
    target,
    props: { projectId: PROJECT, onReview: () => {}, ...props }
  }) as Record<string, unknown>;
  await settle();
  return { paths };
}
const flat = () => (target.textContent ?? '').replace(/\s+/g, ' ');
const button = (name: RegExp) =>
  [...target.querySelectorAll('button')].find((b) => name.test(b.textContent ?? '')) as
    | HTMLButtonElement
    | undefined;

describe('the inbox of proposals from AI agents', () => {
  it('asks for what is in progress first and lists each proposal with its agent', async () => {
    const { paths } = await render(() =>
      json(
        page([
          item(1),
          item(2, { state: 'draft', mine: true, submitted_at: null, name: 'Edit the intro' })
        ])
      )
    );
    expect(paths[0]).toBe(`/projects/${PROJECT}/agent-proposals?state=active&offset=0&limit=20`);
    const rows = target.querySelectorAll('article');
    expect(rows).toHaveLength(2);
    // Every row says who wrote it in words, not only in colour.
    for (const r of rows) expect(r.textContent).toContain('AI agent');
    expect(rows[0].textContent).toContain('Waiting for review');
    expect(rows[0].textContent).toContain('Claude for Olivia');
    expect(rows[0].textContent).toContain('in Claude');
    expect(rows[0].textContent).toContain('3 changes');
    expect(rows[1].textContent).toContain('Waiting for you to send');
    expect(rows[1].textContent).toContain('(your agent)');
    expect(button(/Review and send/)).toBeTruthy();
    expect(button(/Review changes/)).toBeTruthy();
    expect(target.querySelector('h2')?.textContent).toBe('Proposals from AI agents');
    expect(target.querySelector('select')?.getAttribute('aria-label')).toBeNull();
    expect(target.querySelector('label')?.textContent).toContain('Show');
  });

  it('shows a plain empty state and says what will appear', async () => {
    await render(() => json(page([])));
    expect(target.querySelectorAll('article')).toHaveLength(0);
    expect(flat()).toContain('No AI agent has a proposal waiting.');
    expect(flat()).toContain('appears here for a person to review');
  });

  it('warns that the file moved on, in words and with an icon', async () => {
    await render(() => json(page([item(1, { stale_base: true, behind: 2, head_seq: 7 })])));
    const note = target.querySelector('[role="note"]')!;
    expect(note.textContent).toContain('2 newer changes since the AI agent read it');
    expect(note.querySelector('svg')).toBeTruthy();
    await render(() => json(page([item(1)])));
    expect(target.querySelector('[role="note"]')).toBeNull();
  });

  it('shows what the agent wrote as text and never as markup', async () => {
    const hostile = '<img src=x onerror="window.pwned=1"> <b>bold</b>';
    await render(() =>
      json(
        page([
          item(1, {
            name: hostile,
            document: { id: 'd', title: '<script>window.pwned=2</script>', kind: 'document' },
            agent: {
              id: 'a',
              client_kind: 'claude',
              display_name: '<i>Claude</i> for Olivia',
              principal: { id: 'p', name: 'Olivia' },
              revoked: false
            }
          })
        ])
      )
    );
    expect(target.querySelector('article strong')?.textContent).toBe(hostile);
    expect(
      target.querySelectorAll('article img, article b, article i, article script')
    ).toHaveLength(0);
    expect((window as unknown as { pwned?: number }).pwned).toBeUndefined();
    expect(flat()).toContain('<i>Claude</i> for Olivia');
  });

  it('filters by state and starts again from the newest', async () => {
    const { paths } = await render((path) =>
      path.includes('state=merged')
        ? json(
            page([item(3, { state: 'merged', closed_at: '2026-10-21T10:00:00Z' })], {
              state: 'merged'
            })
          )
        : json(page([item(1)]))
    );
    const select = target.querySelector('select') as HTMLSelectElement;
    expect([...select.options].map((o) => o.textContent?.trim())).toEqual([
      'In progress',
      'Waiting for you to send',
      'Waiting for review',
      'Changes requested',
      'Merged',
      'Declined',
      'Withdrawn',
      'Everything'
    ]);
    select.value = 'merged';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();
    expect(paths.at(-1)).toContain('state=merged&offset=0');
    expect(target.querySelector('article')?.textContent).toContain('Merged');
    expect(target.querySelector('article')?.getAttribute('data-state')).toBe('merged');
  });

  it('pages with Newer and Older, only when there is somewhere to go', async () => {
    const { paths } = await render((path) =>
      path.includes('offset=20')
        ? json(page([item(2)], { offset: 20 }))
        : json(page([item(1)], { more: true }))
    );
    expect(button(/Newer/)?.disabled).toBe(true);
    button(/Older/)!.click();
    await settle();
    expect(paths.at(-1)).toContain('offset=20');
    expect(button(/Newer/)?.disabled).toBe(false);
    expect(button(/Older/)?.disabled).toBe(true);
  });

  it('says how much of the limits the viewer’s own agents have used', async () => {
    await render(() =>
      json(
        page([item(1)], {
          budgets: {
            write_mode: 'draft',
            proposals_today: { used: 2, limit: 20 },
            open_proposals: { used: 1, limit: 5 },
            ops_per_proposal: 500
          }
        })
      )
    );
    expect(target.querySelector('[data-testid="agent-budget"]')?.textContent).toContain(
      'started 2 of 20 proposals today and have 1 of 5 open'
    );
  });

  it('reports the in-progress count and opens a proposal for review', async () => {
    const onCount = vi.fn(),
      onReview = vi.fn();
    await render(() => json(page([item(1), item(2)], { more: true })), { onCount, onReview });
    expect(onCount).toHaveBeenCalledWith(2, true);
    button(/Review changes/)!.click();
    await settle();
    expect(onReview).toHaveBeenCalledOnce();
    expect(onReview.mock.calls[0][0].id).toBe(item(1).id);
    expect(onReview.mock.calls[0][0].document.id).toBe('44444444-4444-4444-8444-444444444444');
  });

  it('says what went wrong and lets the person try again', async () => {
    let failing = true;
    const { paths } = await render(() =>
      failing ? json({ error: 'x', code: 'forbidden' }, 403) : json(page([item(1)]))
    );
    expect(target.querySelector('[role="alert"]')?.textContent).toContain('Only members');
    failing = false;
    button(/Try again/)!.click();
    await settle();
    expect(paths).toHaveLength(2);
    expect(target.querySelector('[role="alert"]')).toBeNull();
    expect(target.querySelectorAll('article')).toHaveLength(1);
  });
});
