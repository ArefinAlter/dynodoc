import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import ChangeRequestReview from './ChangeRequestReview.svelte';

// Synthetic values only. Shapes mirror GET /documents/{id}/change-requests/{id}.
const DOC = '44444444-4444-4444-8444-444444444444';
const REQ = '33333333-3333-4333-8333-333333333333';
const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
const AGENT = {
  id: '55555555-5555-4555-8555-555555555555',
  client_kind: 'claude',
  display_name: 'Claude for Olivia',
  principal: { id: '66666666-6666-4666-8666-666666666666', name: 'Olivia' },
  revoked: false
};
function detail(
  request: Record<string, unknown> = {},
  extra: Record<string, unknown> = {},
  review: Record<string, unknown> = {}
) {
  return {
    request: {
      id: REQ,
      name: 'Proposal from Claude for Olivia',
      note: '',
      source: { kind: 'agent' },
      base_seq: 3,
      created_at: '2026-10-21T09:00:00Z',
      submitted_at: '2026-10-21T09:05:00Z',
      merged_at: null,
      review_outcome: null,
      reviewed_at: null,
      review_note: '',
      revision: 2,
      author: { id: AGENT.id, name: 'Claude for Olivia', email: null },
      reviewer: null,
      edits: 3,
      status: 'open',
      mine: false,
      agent: AGENT,
      ...request
    },
    team_seq: 5,
    revision: 2,
    ops: [],
    conflicts: [],
    own: [],
    nodes: {},
    behind: 0,
    review: {
      reviews: [],
      rules: { protect_team_version: false, required_approvals: 1, merge_roles: 'editors' },
      approvals: 0,
      approved_by: [],
      changes_requested_by: [],
      required_approvals: 1,
      satisfied: false,
      can_merge: true,
      can_override: false,
      can_approve: true,
      ...review
    },
    agent_statement: {
      calls: 1,
      ops_in: 2,
      ops_engine: 3,
      stale: 0,
      reasons: [{ call: 1, i: 0, op: 'set_text', id: 'a1', why: 'Clearer wording.' }],
      more: false,
      trust: 'untrusted_text'
    },
    ...extra
  };
}

let target: HTMLElement;
let component: Record<string, unknown> | null = null;
afterEach(() => {
  if (component) unmount(component);
  component = null;
  target?.remove();
  vi.unstubAllGlobals();
});
async function settle() {
  for (let i = 0; i < 10; i++) await tick();
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}
interface Call {
  method: string;
  path: string;
  body: Record<string, unknown> | undefined;
}
async function render(
  respond: (call: Call, calls: Call[]) => Response | undefined,
  props: Record<string, unknown> = {}
) {
  const calls: Call[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const call: Call = {
        method: init?.method ?? 'GET',
        path: String(input).replace('/api/engine', ''),
        body: init?.body ? JSON.parse(String(init.body)) : undefined
      };
      calls.push(call);
      return respond(call, calls) ?? json({}, 404);
    })
  );
  const handlers = { onClose: vi.fn(), onChanged: vi.fn(), onOpenDraft: vi.fn() };
  target = document.createElement('div');
  document.body.appendChild(target);
  component = mount(ChangeRequestReview as never, {
    target,
    props: {
      documentId: DOC,
      requestId: REQ,
      team: { nodes: {} },
      teamSeq: 5,
      canMerge: true,
      canComment: true,
      ...handlers,
      ...props
    }
  }) as Record<string, unknown>;
  await settle();
  return { calls, ...handlers };
}
const button = (name: RegExp) =>
  [...target.querySelectorAll('button')].find((b) => name.test((b.textContent ?? '').trim())) as
    | HTMLButtonElement
    | undefined;
const click = async (el: HTMLElement | undefined) => {
  expect(el).toBeTruthy();
  el!.click();
  await settle();
};
const flat = () => (target.textContent ?? '').replace(/\s+/g, ' ');
const posts = (calls: Call[]) => calls.filter((c) => c.method === 'POST');
const REVIEWS = `/documents/${DOC}/change-requests/${REQ}/reviews`;
const DETAIL = `/documents/${DOC}/change-requests/${REQ}`;

describe('reviewing a request an AI agent wrote', () => {
  it('says who proposed it and in which client, and labels the agent’s own words', async () => {
    await render((call) => (call.path === DETAIL ? json(detail()) : undefined));
    expect(target.querySelector('header .agent-badge')?.textContent).toContain('AI agent');
    const block = target.querySelector('section.agent')!;
    const said = (block.textContent ?? '').replace(/\s+/g, ' ');
    expect(said).toContain('Proposed by an AI agent');
    expect(said).toContain('Claude for Olivia');
    expect(said).toContain('proposed this through Claude, on behalf of Olivia');
    expect(said).toContain('nothing changes in the file until a person merges it');
    expect(said).toContain('shown exactly as written');
    expect(said).toContain('Clearer wording.');
    expect(block.querySelector('h3')?.id).toBe('agent-title');
  });

  it('shows the agent’s reasons and notes as text, never as markup', async () => {
    const hostile = '<img src=x onerror="window.pwned=1"><a href="https://evil.example">click</a>';
    await render((call) =>
      call.path === DETAIL
        ? json(
            detail(
              { note: hostile, name: hostile },
              {
                agent_statement: {
                  calls: 1,
                  ops_in: 1,
                  ops_engine: 1,
                  stale: 2,
                  reasons: [{ call: 1, i: 0, op: 'set_text', id: null, why: hostile }],
                  more: true,
                  trust: 'untrusted_text'
                }
              }
            )
          )
        : undefined
    );
    expect(target.querySelector('.why-text')?.textContent).toBe(hostile);
    expect(target.querySelector('blockquote')?.textContent).toBe(hostile);
    expect(target.querySelector('h2')?.textContent).toBe(hostile);
    expect(target.querySelectorAll('img, a[href*="evil"]')).toHaveLength(0);
    expect((window as unknown as { pwned?: number }).pwned).toBeUndefined();
    expect(flat()).toContain(
      '2 of the agent’s edits changed text that someone else had already changed'
    );
    expect(flat()).toContain('The agent gave more reasons than are shown here.');
  });

  it('approves the version on screen: the revision goes with the click', async () => {
    const { calls, onChanged } = await render((call) => {
      if (call.path === DETAIL) return json(detail());
      if (call.path === REVIEWS) return json({ id: 'r', revision: 2, review: detail().review });
      return undefined;
    });
    await click(button(/^Approve$/));
    const sent = posts(calls).find((c) => c.path === REVIEWS)!;
    expect(sent.body).toEqual({ verdict: 'approved', note: '', node_id: null, revision: 2 });
    expect(onChanged).toHaveBeenCalledOnce();
    expect(flat()).toContain('Approved.');
  });

  it('when the request moved while it was read: says so calmly, shows the new version, keeps the text and records nothing', async () => {
    let reads = 0;
    const { calls, onChanged } = await render((call) => {
      if (call.path === DETAIL) {
        reads += 1;
        // The second read is the new version.
        return json(reads === 1 ? detail() : detail({ revision: 3 }, { revision: 3, behind: 0 }));
      }
      if (call.path === REVIEWS && call.body?.revision === 2)
        return json(
          {
            error: 'conflict: This change request changed while you were reviewing it.',
            code: 'request_changed',
            details: { current_revision: 3, revision: 2, reason: 'This change request changed' }
          },
          409
        );
      if (call.path === REVIEWS) return json({ id: 'r', revision: 3, review: detail().review });
      return undefined;
    });
    const box = target.querySelector(
      'textarea[aria-label="Review comment"]'
    ) as HTMLTextAreaElement;
    box.value = 'Looks right to me.';
    box.dispatchEvent(new Event('input', { bubbles: true }));
    await click(button(/^Approve$/));

    const moved = target.querySelector('[data-testid="request-moved"]')!;
    expect(moved.textContent).toContain(
      'This request changed while you were reading it. Nothing was recorded. Read the new version, then decide.'
    );
    expect(moved.getAttribute('role')).toBe('status');
    expect(document.activeElement).toBe(moved);
    // Not an error: no alert, no red text, nothing was reported as changed.
    expect(target.querySelector('[role="alert"]')).toBeNull();
    expect(onChanged).not.toHaveBeenCalled();
    expect(reads).toBe(2);
    expect(
      (target.querySelector('textarea[aria-label="Review comment"]') as HTMLTextAreaElement).value
    ).toBe('Looks right to me.');
    // The next click is for the new version.
    await click(button(/^Approve$/));
    const last = posts(calls)
      .filter((c) => c.path === REVIEWS)
      .at(-1)!;
    expect(last.body).toMatchObject({
      verdict: 'approved',
      note: 'Looks right to me.',
      revision: 3
    });
    expect(target.querySelector('[data-testid="request-moved"]')).toBeNull();
  });

  it('can be dismissed, and an ordinary request gets the same calm message', async () => {
    let reads = 0;
    await render((call) => {
      if (call.path === DETAIL) {
        reads += 1;
        const plain = {
          agent: undefined,
          author: { id: 'u', name: 'Pat', email: 'pat@example.test' }
        };
        return json({ ...detail(plain, { revision: 2 }), agent_statement: undefined });
      }
      if (call.path === REVIEWS)
        return json({ error: 'x', code: 'request_changed', details: { current_revision: 3 } }, 409);
      return undefined;
    });
    expect(target.querySelector('section.agent')).toBeNull();
    expect(target.querySelector('.agent-badge')).toBeNull();
    await click(button(/^Approve$/));
    expect(target.querySelector('[data-testid="request-moved"]')).toBeTruthy();
    expect(reads).toBe(2);
    await click(button(/Dismiss this message/));
    expect(target.querySelector('[data-testid="request-moved"]')).toBeNull();
  });

  it('does not treat another refusal as the request having moved', async () => {
    await render((call) => {
      if (call.path === DETAIL) return json(detail());
      if (call.path === REVIEWS)
        return json({ error: 'You cannot', code: 'forbidden', details: { reason: 'No.' } }, 403);
      return undefined;
    });
    await click(button(/^Approve$/));
    expect(target.querySelector('[data-testid="request-moved"]')).toBeNull();
    expect(target.querySelector('[role="alert"]')?.textContent).toContain('You cannot');
  });
});

describe('adopting an AI agent’s draft', () => {
  const draft = (extra: Record<string, unknown> = {}) =>
    detail({ submitted_at: null, mine: true, ...extra }, {}, { can_approve: false });

  it('offers to adopt it and nothing that would merge, decline or approve it', async () => {
    await render((call) => (call.path === DETAIL ? json(draft()) : undefined));
    expect(target.querySelector('.status')?.textContent).toBe('draft');
    expect(button(/Adopt and send for review/)).toBeTruthy();
    expect(button(/Merge/)).toBeUndefined();
    expect(button(/Decline/)).toBeUndefined();
    expect(button(/^Approve$/)).toBeUndefined();
    expect(target.querySelector('textarea[aria-label="Review comment"]')).toBeNull();
    expect(flat()).toContain('This is a draft only you can see');
  });

  it('sends the revision it was reading and tells the person it stays the agent’s work', async () => {
    const { calls, onChanged } = await render((call) => {
      if (call.path === DETAIL) return json(draft());
      if (call.path === `/documents/${DOC}/drafts/${REQ}/submit`) return json({ status: 'open' });
      return undefined;
    });
    const note = target.querySelector(
      'textarea[aria-label="Note for the reviewers"]'
    ) as HTMLTextAreaElement;
    note.value = 'Please look at the intro.';
    note.dispatchEvent(new Event('input', { bubbles: true }));
    await click(button(/Adopt and send for review/));
    const sent = posts(calls).find((c) => c.path.endsWith('/submit'))!;
    expect(sent.body).toEqual({ note: 'Please look at the intro.', revision: 2 });
    expect(onChanged).toHaveBeenCalledOnce();
    expect(flat()).toContain('It stays marked as the AI agent’s work');
  });

  it('when the agent added to the draft meanwhile: sends nothing more, shows the new draft, says so', async () => {
    let reads = 0;
    const { calls, onChanged } = await render((call) => {
      if (call.path === DETAIL) {
        reads += 1;
        return json(reads === 1 ? draft() : draft({ revision: 3 }));
      }
      if (call.path.endsWith('/submit'))
        return json({ error: 'x', code: 'request_changed', details: { current_revision: 3 } }, 409);
      return undefined;
    });
    await click(button(/Adopt and send for review/));
    const moved = target.querySelector('[data-testid="request-moved"]')!;
    expect(moved.textContent).toContain(
      'The AI agent added to this draft while you were reading it. Nothing was sent.'
    );
    expect(onChanged).not.toHaveBeenCalled();
    expect(posts(calls).filter((c) => c.path.endsWith('/submit'))).toHaveLength(1);
    expect(reads).toBe(2);
  });
});
