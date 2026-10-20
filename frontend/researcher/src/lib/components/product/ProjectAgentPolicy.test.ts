import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import ProjectAgentPolicy from './ProjectAgentPolicy.svelte';

// Synthetic values only. Shapes mirror the generated PolicyView, KillView and UsageView.
const PROJECT = '22222222-2222-4222-8222-222222222222';
const FILE = '99999999-aaaa-4bbb-8ccc-dddddddddddd';
const OTHER_FILE = '88888888-aaaa-4bbb-8ccc-dddddddddddd';
const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
const policy = (extra: Record<string, unknown> = {}) => ({
  project_id: PROJECT,
  enabled: false,
  min_role: 'viewer',
  kinds: ['document', 'spreadsheet', 'presentation'],
  calls_per_minute: 60,
  read_bytes_per_day: 20_000_000,
  revision: 0,
  configured: false,
  gateway: true,
  killed_at: null,
  updated_at: null,
  file_overrides: [],
  file_overrides_truncated: false,
  ...extra
});
const usage = (items: unknown[] = [], extra: Record<string, unknown> = {}) => ({
  project_id: PROJECT,
  from: '2026-10-14',
  to: '2026-10-20',
  truncated: false,
  items,
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
interface Call {
  method: string;
  path: string;
  headers: Record<string, string>;
  body: unknown;
}
async function settle() {
  for (let i = 0; i < 8; i++) await tick();
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}
type Responder = (call: Call) => Response | undefined;
/** Default answers: a saved-nothing policy and an empty usage report. Override per test. */
async function render(
  props: Record<string, unknown> = {},
  respond: Responder = () => undefined,
  initial: Record<string, unknown> = policy()
) {
  const calls: Call[] = [];
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const call: Call = {
        method: init?.method ?? 'GET',
        path: String(input).replace('/api/engine', ''),
        headers: (init?.headers ?? {}) as Record<string, string>,
        body: init?.body ? JSON.parse(String(init.body)) : undefined
      };
      calls.push(call);
      const custom = respond(call);
      if (custom) return custom;
      if (call.path.includes('/agent-usage')) return json(usage());
      if (call.path.endsWith('/agent-policy') && call.method === 'GET') return json(initial);
      return json({}, 404);
    })
  );
  target = document.createElement('div');
  document.body.appendChild(target);
  component = mount(ProjectAgentPolicy as never, {
    target,
    props: { projectId: PROJECT, canManage: true, ...props }
  }) as Record<string, unknown>;
  await settle();
  return { calls };
}
const button = (name: RegExp | string) =>
  [...target.querySelectorAll('button')].find((b) =>
    typeof name === 'string'
      ? (b.textContent ?? '').trim() === name
      : name.test(b.textContent ?? '')
  ) as HTMLButtonElement | undefined;
const click = async (el: HTMLElement | undefined | null) => {
  expect(el).toBeTruthy();
  el!.click();
  await settle();
};
const input = (name: RegExp) =>
  [...target.querySelectorAll<HTMLInputElement>('label')]
    .find((l) => name.test(l.textContent ?? ''))
    ?.querySelector('input') as HTMLInputElement;
const type = async (el: HTMLInputElement, value: string) => {
  el.value = value;
  el.dispatchEvent(new Event('input', { bubbles: true }));
  await settle();
};
const flat = () => (target.textContent ?? '').replace(/\s+/g, ' ');
const puts = (calls: Call[]) => calls.filter((c) => c.method === 'PUT');
const sw = () => target.querySelector<HTMLInputElement>('input[role="switch"]')!;

describe('ProjectAgentPolicy for someone who cannot manage the project', () => {
  it('explains in one line and asks the server for nothing', async () => {
    const { calls } = await render({ canManage: false });
    expect(target.querySelector('h3')?.textContent).toBe('AI agents');
    expect(flat()).toContain(
      'Only a project owner or manager can see or change this project’s AI agent settings.'
    );
    expect(target.querySelector('form')).toBeNull();
    expect(button(/Stop all AI agents/)).toBeUndefined();
    expect(calls).toHaveLength(0);
  });
});

describe('ProjectAgentPolicy: reading the settings', () => {
  it('says AI agents are turned on in this installation when the server serves them', async () => {
    await render();
    expect(flat()).toContain('AI agents are turned on in this installation.');
    expect(flat()).not.toContain('not available on this installation yet');
  });

  it('says honestly when this installation does not serve AI agents yet', async () => {
    await render({}, () => undefined, policy({ gateway: false }));
    expect(flat()).toContain('AI agents are not available on this installation yet.');
    expect(flat()).toContain('no effect until your operator turns the agent connector on');
    expect(flat()).not.toContain('turned on in this installation');
  });

  it('shows the saved values with their units and bounds', async () => {
    await render(
      {},
      () => undefined,
      policy({
        enabled: true,
        min_role: 'editor',
        kinds: ['document', 'presentation'],
        calls_per_minute: 30,
        read_bytes_per_day: 5_000_000,
        revision: 3,
        configured: true
      })
    );
    expect(sw().checked).toBe(true);
    expect(target.querySelector<HTMLSelectElement>('select')!.value).toBe('editor');
    expect(
      [...target.querySelectorAll<HTMLInputElement>('fieldset.kinds input')].map((b) => b.checked)
    ).toEqual([true, false, true]);
    expect(input(/Reads per minute/).value).toBe('30');
    expect(input(/Data read per day/).value).toBe('5');
    expect(flat()).toContain('From 1 to 600.');
    expect(flat()).toContain('From 0.01 MB to 1,000 MB a day');
    expect(flat()).toContain('Members below Editor cannot have an AI agent read this project.');
    // The role floor uses the names the app already shows.
    expect([...target.querySelectorAll('option')].map((o) => o.textContent)).toEqual([
      'Viewer',
      'Contributor',
      'Reviewer',
      'Editor',
      'Manager',
      'Owner'
    ]);
    expect(flat()).toContain('in this release it can only read');
    expect(flat()).toContain('An AI agent can only ever propose changes, which a person reviews');
  });

  it('labels every control and announces results politely', async () => {
    await render();
    for (const control of target.querySelectorAll<HTMLElement>(
      'input:not([type="hidden"]), select'
    )) {
      const named = control.closest('label') || control.getAttribute('aria-label');
      expect(named, control.outerHTML).toBeTruthy();
    }
    // A polite live region without a role: the project page has its own status region.
    expect(target.querySelector('div.live')?.getAttribute('aria-live')).toBe('polite');
    expect(target.querySelector('[role="status"]')).toBeNull();
    expect(target.querySelector('form')?.getAttribute('aria-label')).toBe('AI agent settings');
    expect(sw().getAttribute('role')).toBe('switch');
  });

  it('says so when the settings are not available on this server', async () => {
    await render({}, (call) =>
      call.path.endsWith('/agent-policy') && call.method === 'GET'
        ? json({ error: 'Not found' }, 404)
        : undefined
    );
    expect(target.querySelector('[role="alert"]')?.textContent).toBe(
      'AI agent settings are not available on this server yet.'
    );
    expect(target.querySelector('form')).toBeNull();
    expect(button(/Stop all AI agents/)).toBeUndefined();
  });

  it('offers to try again when loading fails, and still offers the stop button', async () => {
    let failing = true;
    const { calls } = await render({}, (call) =>
      failing && call.path.endsWith('/agent-policy') && call.method === 'GET'
        ? json({ error: 'busy' }, 503)
        : undefined
    );
    expect(target.querySelector('[role="alert"]')?.textContent).toMatch(/busy or timed out/);
    expect(target.querySelector('form')).toBeNull();
    expect(button(/^Stop all AI agents now/)).toBeTruthy();
    failing = false;
    await click(button('Try again'));
    expect(target.querySelector('form')).toBeTruthy();
    expect(target.querySelector('[role="alert"]')).toBeNull();
    expect(calls.filter((c) => c.path.endsWith('/agent-policy'))).toHaveLength(2);
  });
});

describe('ProjectAgentPolicy: saving', () => {
  it('turns agents on with the full settings and the revision it loaded', async () => {
    const { calls } = await render({}, (call) =>
      call.method === 'PUT'
        ? json(policy({ enabled: true, revision: 1, configured: true, changed: true }))
        : undefined
    );
    await click(sw());
    expect(target.textContent).toContain('You have unsaved changes.');
    await click(button('Save AI agent settings'));
    expect(puts(calls)).toEqual([
      {
        method: 'PUT',
        path: `/projects/${PROJECT}/agent-policy`,
        headers: expect.objectContaining({ 'if-match': '"0"' }),
        body: {
          enabled: true,
          min_role: 'viewer',
          kinds: ['document', 'spreadsheet', 'presentation'],
          calls_per_minute: 60,
          read_bytes_per_day: 20_000_000
        }
      }
    ]);
    expect(target.querySelector('.repo-notice')?.textContent).toBe(
      'Saved. AI agents are on for this project.'
    );
    expect(target.textContent).not.toContain('You have unsaved changes.');
  });

  it('warns that nothing works yet when the installation does not serve agents', async () => {
    await render(
      {},
      (call) =>
        call.method === 'PUT'
          ? json(policy({ enabled: true, revision: 1, gateway: false, changed: true }))
          : undefined,
      policy({ gateway: false })
    );
    await click(sw());
    await click(button('Save AI agent settings'));
    expect(target.querySelector('.repo-notice')?.textContent).toContain(
      'only once your operator turns the agent connector on'
    );
  });

  it('sends the role floor, kinds and budgets the person chose, and the next save uses the new revision', async () => {
    const { calls } = await render(
      {},
      (call) =>
        call.method === 'PUT'
          ? json(
              policy({
                enabled: true,
                min_role: 'manager',
                kinds: ['spreadsheet'],
                calls_per_minute: 10,
                read_bytes_per_day: 1_500_000,
                revision: 1,
                configured: true,
                changed: true
              })
            )
          : undefined,
      policy({ enabled: true, revision: 0 })
    );
    const select = target.querySelector<HTMLSelectElement>('select')!;
    select.value = 'manager';
    select.dispatchEvent(new Event('change', { bubbles: true }));
    const boxes = [...target.querySelectorAll<HTMLInputElement>('fieldset.kinds input')];
    await click(boxes[0]);
    await click(boxes[2]);
    await type(input(/Reads per minute/), '10');
    await type(input(/Data read per day/), '1.5');
    await click(button('Save AI agent settings'));
    expect(puts(calls)[0].body).toEqual({
      enabled: true,
      min_role: 'manager',
      kinds: ['spreadsheet'],
      calls_per_minute: 10,
      read_bytes_per_day: 1_500_000
    });
    await type(input(/Reads per minute/), '20');
    await click(button('Save AI agent settings'));
    expect(puts(calls)[1].headers['if-match']).toBe('"1"');
  });

  it('will not send values outside the bounds, says why and moves focus to the field', async () => {
    const { calls } = await render();
    await type(input(/Reads per minute/), '601');
    await click(button('Save AI agent settings'));
    expect(puts(calls)).toHaveLength(0);
    const field = input(/Reads per minute/);
    expect(field.getAttribute('aria-invalid')).toBe('true');
    expect(target.querySelector('.field-problem')?.textContent).toBe(
      'Enter a whole number from 1 to 600.'
    );
    expect(field.getAttribute('aria-describedby')).toContain('-calls-problem');
    expect(document.activeElement).toBe(field);
    await type(field, '600');
    await type(input(/Data read per day/), '0.001');
    await click(button('Save AI agent settings'));
    expect(target.querySelector('.field-problem')?.textContent).toBe(
      'Enter an amount from 0.01 MB to 1,000 MB.'
    );
    expect(puts(calls)).toHaveLength(0);
  });

  it('needs at least one kind of file', async () => {
    const { calls } = await render();
    for (const box of target.querySelectorAll<HTMLInputElement>('fieldset.kinds input'))
      await click(box);
    await click(button('Save AI agent settings'));
    expect(target.querySelector('.field-problem')?.textContent).toBe(
      'Choose at least one kind of file.'
    );
    expect(puts(calls)).toHaveLength(0);
  });

  it('reloads and says so when someone else saved first', async () => {
    let reads = 0;
    const { calls } = await render(
      {},
      (call) => {
        if (call.method === 'PUT') return json({ error: 'conflict' }, 409);
        if (call.path.endsWith('/agent-policy') && call.method === 'GET') {
          reads += 1;
          return reads === 1
            ? undefined
            : json(policy({ enabled: true, min_role: 'owner', revision: 7, configured: true }));
        }
        return undefined;
      },
      policy()
    );
    await click(sw());
    await click(button('Save AI agent settings'));
    expect(target.querySelector('[role="alert"]')?.textContent).toContain(
      'Someone else changed these settings'
    );
    expect(
      calls.filter((c) => c.path.endsWith('/agent-policy') && c.method === 'GET')
    ).toHaveLength(2);
    // The form now shows what is saved, not the stale edit.
    expect(target.querySelector<HTMLSelectElement>('select')!.value).toBe('owner');
    expect(sw().checked).toBe(true);
    expect(target.textContent).not.toContain('You have unsaved changes.');
  });

  it('says what went wrong in plain words and keeps the edit', async () => {
    await render({}, (call) =>
      call.method === 'PUT'
        ? json({ error: 'db error 23505', details: { reason: 'violates constraint' } }, 500)
        : undefined
    );
    await click(sw());
    await click(button('Save AI agent settings'));
    expect(target.querySelector('[role="alert"]')?.textContent).toBe(
      'The server could not save that change. Try again in a moment.'
    );
    expect(sw().checked).toBe(true);
  });

  it('reports a change that changed nothing', async () => {
    await render({}, (call) =>
      call.method === 'PUT' ? json(policy({ revision: 0, changed: false })) : undefined
    );
    await click(button('Save AI agent settings'));
    expect(target.querySelector('.repo-notice')?.textContent).toMatch(/already saved/);
  });
});

describe('ProjectAgentPolicy: usage', () => {
  it('asks for seven UTC days and adds the clients of a day together', async () => {
    const { calls } = await render({}, (call) =>
      call.path.includes('/agent-usage')
        ? json(
            usage([
              {
                day: '2026-10-20',
                client: 'claude-code',
                calls: 3,
                bytes_out: 1_000,
                denials: 1,
                errors: 0
              },
              {
                day: '2026-10-20',
                client: 'other',
                calls: 4,
                bytes_out: 2_500_000,
                denials: 0,
                errors: 1
              },
              { day: '2026-10-18', client: null, calls: 12, bytes_out: 900, denials: 5, errors: 0 }
            ])
          )
        : undefined
    );
    const request = calls.find((c) => c.path.includes('/agent-usage'))!;
    expect(request.path).toMatch(/\?from=\d{4}-\d{2}-\d{2}&to=\d{4}-\d{2}-\d{2}$/);
    // The table is a labelled region, named by its heading.
    const region = target.querySelector('.table-wrap')!;
    expect(region.getAttribute('role')).toBe('region');
    expect(document.getElementById(region.getAttribute('aria-labelledby')!)?.textContent).toBe(
      'Usage, last 7 days'
    );
    const rows = [...target.querySelectorAll('tbody tr')].map((tr) =>
      [...tr.children].map((c) => c.textContent)
    );
    expect(rows).toEqual([
      ['20 Oct 2026', '7', '2.5 MB', '1'],
      ['18 Oct 2026', '12', '900 B', '5']
    ]);
    expect([...target.querySelectorAll('thead th')].map((th) => th.textContent)).toEqual([
      'Day (UTC)',
      'Calls',
      'Data read',
      'Refused'
    ]);
  });

  it('says so when there was no activity', async () => {
    await render();
    expect(flat()).toContain('No AI agent activity in the last 7 days.');
    expect(target.querySelector('table')).toBeNull();
  });

  it('says when the report is cut off', async () => {
    await render({}, (call) =>
      call.path.includes('/agent-usage')
        ? json(
            usage(
              [{ day: '2026-10-20', client: 'a', calls: 1, bytes_out: 1, denials: 0, errors: 0 }],
              {
                truncated: true
              }
            )
          )
        : undefined
    );
    expect(flat()).toContain('Only the first 500 rows were returned');
  });

  it('shows a usage failure without hiding the settings', async () => {
    await render({}, (call) =>
      call.path.includes('/agent-usage') ? json({ error: 'busy' }, 503) : undefined
    );
    expect(target.querySelector('form')).toBeTruthy();
    expect(target.querySelector('[role="alert"]')?.textContent).toMatch(/busy or timed out/);
  });
});

describe('ProjectAgentPolicy: files with their own setting', () => {
  it('lists them read-only, by name when known and otherwise by the start of the id', async () => {
    await render(
      { fileTitles: { [FILE]: 'Contract.docx' } },
      () => undefined,
      policy({
        file_overrides: [
          { document_id: FILE, access: 'none', updated_at: '2026-10-18T10:00:00Z' },
          { document_id: OTHER_FILE, access: 'read', updated_at: '2026-10-17T10:00:00Z' }
        ]
      })
    );
    const items = [...target.querySelectorAll('ul.overrides li')];
    expect(items).toHaveLength(2);
    expect(items[0].querySelector('a')?.textContent).toBe('Contract.docx');
    expect(items[0].querySelector('a')?.getAttribute('href')).toBe(
      `/projects/${PROJECT}/files/${FILE}`
    );
    expect(items[0].textContent).toContain('Hidden from AI agents');
    expect(items[1].querySelector('a')?.textContent).toBe('File 88888888');
    expect(items[1].textContent).toContain('never gives agents more than the project settings');
    expect(flat()).toContain('changing a file’s setting is not available on this screen yet');
    // Read-only: nothing here can be edited.
    expect(
      target.querySelectorAll('ul.overrides button, ul.overrides input, ul.overrides select')
    ).toHaveLength(0);
  });

  it('says when no file has one', async () => {
    await render();
    expect(target.querySelector('ul.overrides')).toBeNull();
    expect(flat()).toContain('No file has its own setting.');
  });
});

describe('ProjectAgentPolicy: Stop all AI agents now', () => {
  const stopped = {
    project_id: PROJECT,
    enabled: false,
    was_enabled: true,
    revoked_grants: 2,
    revision: 5
  };

  it('asks first, saying exactly what it does, and sends nothing until confirmed', async () => {
    const { calls } = await render({}, () => undefined, policy({ enabled: true, revision: 4 }));
    await click(button('Stop all AI agents now'));
    const group = target.querySelector('.confirm')!;
    expect(group.getAttribute('role')).toBe('group');
    expect(group.getAttribute('aria-labelledby')).toBeTruthy();
    expect(group.textContent!.replace(/\s+/g, ' ')).toBe(
      'Stop all AI agents in this project now? ' +
        'This takes effect at once and does the following: ' +
        'Turns AI agents off for this project. ' +
        'Ends every AI agent key’s access to this project, and every open agent session. ' +
        'Leaves people’s own keys untouched. ' +
        'Turning AI agents on again does not bring the old access back. ' +
        'Each AI agent key must be given this project again, and its agent must start a new session. ' +
        'Cancel Yes, stop all AI agents'
    );
    // Focus lands on the safe choice.
    expect(document.activeElement?.textContent).toBe('Cancel');
    expect(calls.some((c) => c.method === 'POST')).toBe(false);
  });

  it('can be cancelled with the button or Escape, and focus returns to the stop button', async () => {
    const { calls } = await render();
    await click(button('Stop all AI agents now'));
    await click(button('Cancel'));
    expect(target.querySelector('.confirm')).toBeNull();
    expect(document.activeElement).toBe(button('Stop all AI agents now'));
    await click(button('Stop all AI agents now'));
    target
      .querySelector('.confirm')!
      .dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await settle();
    expect(target.querySelector('.confirm')).toBeNull();
    expect(calls.some((c) => c.method === 'POST')).toBe(false);
  });

  it('pulls the switch and shows the result, with agents now off in the form', async () => {
    let killed = false;
    const { calls } = await render(
      {},
      (call) => {
        if (call.method === 'POST') {
          killed = true;
          return json(stopped);
        }
        if (killed && call.path.endsWith('/agent-policy') && call.method === 'GET')
          return json(
            policy({
              enabled: false,
              revision: 5,
              configured: true,
              killed_at: '2026-10-20T09:00:00Z'
            })
          );
        return undefined;
      },
      policy({ enabled: true, revision: 4, configured: true })
    );
    expect(sw().checked).toBe(true);
    await click(button('Stop all AI agents now'));
    await click(button('Yes, stop all AI agents'));
    const posts = calls.filter((c) => c.method === 'POST');
    expect(posts).toEqual([
      expect.objectContaining({ path: `/projects/${PROJECT}/agent-policy/kill`, body: undefined })
    ]);
    expect(target.querySelector('[aria-live="polite"] .repo-notice')?.textContent).toBe(
      'AI agents are off for this project. 2 AI agent keys lost their access to this project. Every open agent session is refused from now on. People’s own keys are untouched.'
    );
    expect(target.querySelector('.confirm')).toBeNull();
    expect(sw().checked).toBe(false);
    expect(document.activeElement).toBe(button('Stop all AI agents now'));
  });

  it('says plainly that nothing was stopped when the request fails', async () => {
    await render({}, (call) => (call.method === 'POST' ? json({ error: 'busy' }, 503) : undefined));
    await click(button('Stop all AI agents now'));
    await click(button('Yes, stop all AI agents'));
    expect(target.querySelector('[role="alert"]')?.textContent).toMatch(
      /^AI agents were not stopped\. /
    );
    expect(target.querySelector('.repo-notice')).toBeNull();
  });

  it('keeps unsaved edits when it succeeds, except that agents are now off', async () => {
    let killed = false;
    await render(
      {},
      (call) => {
        if (call.method === 'POST') {
          killed = true;
          return json({ ...stopped, revoked_grants: 0 });
        }
        if (killed && call.path.endsWith('/agent-policy') && call.method === 'GET')
          return json(policy({ enabled: false, revision: 5, configured: true }));
        return undefined;
      },
      policy({ enabled: true, revision: 4, configured: true })
    );
    await type(input(/Reads per minute/), '12');
    await click(button('Stop all AI agents now'));
    await click(button('Yes, stop all AI agents'));
    expect(input(/Reads per minute/).value).toBe('12');
    expect(sw().checked).toBe(false);
    expect(target.querySelector('.repo-notice')?.textContent).toContain(
      'No AI agent key had access to this project.'
    );
  });
});
