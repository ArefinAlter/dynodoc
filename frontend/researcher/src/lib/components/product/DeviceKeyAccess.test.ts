import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import DeviceKeyAccess from './DeviceKeyAccess.svelte';
import type { DeviceKey, ProjectChoice } from '$lib/workspace/device-grants';

// Synthetic values only. Shapes mirror docs/DEVICE-KEY-GRANTS-API.md.
const KEY = '11111111-1111-4111-8111-111111111111';
const BOARD = '22222222-2222-4222-8222-222222222222';
const LAUNCH = '33333333-3333-4333-8333-333333333333';
const PUBLIC = 'ab12cd34'.repeat(8);
const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

const device = (extra: Partial<DeviceKey> = {}): DeviceKey => ({
  id: KEY,
  publicKey: PUBLIC,
  registeredAt: '2026-10-13T09:00:00Z',
  revokedAt: null,
  projectAccess: 'granted',
  grants: [],
  ...extra
});
const boardGrant = {
  projectId: BOARD,
  projectName: 'Board packs',
  role: 'editor',
  scopes: ['history:read', 'history:write'] as DeviceKey['grants'][number]['scopes'],
  createdAt: '2026-10-13T09:30:00Z'
};
const projects: ProjectChoice[] = [
  { id: BOARD, name: 'Board packs', role: 'editor' },
  { id: LAUNCH, name: 'Launch plan', role: 'viewer' }
];

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
  body: unknown;
}
async function settle() {
  for (let i = 0; i < 8; i++) await tick();
  await new Promise((resolve) => setTimeout(resolve, 0));
  flushSync();
}
async function render(
  props: Record<string, unknown>,
  respond: (call: Call) => Response = (call) =>
    call.method === 'PATCH'
      ? json({
          key_id: KEY,
          project_access: (call.body as { project_access: string }).project_access,
          changed: true
        })
      : json({ changed: true, scopes: [], revoked: true })
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
      return respond(call);
    })
  );
  const onChanged = vi.fn(async () => {});
  target = document.createElement('div');
  document.body.appendChild(target);
  component = mount(DeviceKeyAccess as never, {
    target,
    props: {
      device: device(),
      projects,
      projectsState: 'ready',
      grantLimit: 50,
      onChanged,
      ...props
    }
  }) as Record<string, unknown>;
  await settle();
  return { calls, onChanged };
}
const button = (name: RegExp | string) =>
  [...target.querySelectorAll('button')].find((b) =>
    typeof name === 'string'
      ? (b.getAttribute('aria-label') ?? b.textContent ?? '').trim() === name
      : name.test(b.getAttribute('aria-label') ?? b.textContent ?? '')
  ) as HTMLButtonElement | undefined;
const click = async (el: HTMLElement | undefined | null) => {
  expect(el).toBeTruthy();
  el!.click();
  await settle();
};

describe('DeviceKeyAccess', () => {
  it('shows the mode, the grant count against the limit, and each grant with its scopes', async () => {
    await render({ device: device({ grants: [boardGrant] }) });
    expect(target.textContent).toContain('Only projects you grant');
    expect(target.textContent).toContain('1 of 50 projects granted');
    expect(target.querySelector('a[href="/projects/' + BOARD + '"]')?.textContent).toBe(
      'Board packs'
    );
    expect(target.textContent).toContain('Read history');
    expect(target.textContent).toContain('Write history');
    expect(target.querySelector('[role="group"]')?.getAttribute('aria-label')).toBe(
      'Project access for key ab12cd34'
    );
  });

  it('revokes one grant with a DELETE, then reloads the list and says what happens next', async () => {
    const { calls, onChanged } = await render({ device: device({ grants: [boardGrant] }) });
    await click(button(/^Revoke grant for Board packs/));
    expect(calls).toEqual([
      { method: 'DELETE', path: `/devices/${KEY}/projects/${BOARD}`, body: undefined }
    ]);
    expect(onChanged).toHaveBeenCalledTimes(1);
    expect(target.querySelector('.repo-notice')?.textContent).toMatch(/next request/);
    // No second status region: the live region has no role.
    expect(target.querySelector('[role="status"]')).toBeNull();
  });

  it('adds a project with the chosen scopes and nothing else', async () => {
    const { calls, onChanged } = await render({
      device: device({ projectAccess: 'granted', grants: [] }),
      fresh: true
    });
    expect(target.textContent).toContain(
      "This key can't open any project yet. Grant it a project."
    );
    await click(button('Grant a project to key ab12cd34'));
    const select = target.querySelector('select') as HTMLSelectElement;
    select.value = LAUNCH;
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();
    // Defaults are Read history and Write history; add Review and drop Write.
    const boxes = [...target.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')];
    expect(boxes.map((b) => b.checked)).toEqual([false, true, true, false]);
    await click(boxes[3]);
    await click(boxes[2]);
    // The viewer role cannot review: the form says so before the server would refuse it.
    expect(target.textContent).toMatch(/does not include Review/);
    await click(button('Save grant'));
    expect(calls).toEqual([
      {
        method: 'PUT',
        path: `/devices/${KEY}/projects/${LAUNCH}`,
        body: { scopes: ['history:read', 'history:review'] }
      }
    ]);
    expect(onChanged).toHaveBeenCalledTimes(1);
    expect(target.querySelector('.repo-notice')?.textContent).toContain('Launch plan');
  });

  it('will not save a grant with no scope', async () => {
    const { calls } = await render({ device: device({ grants: [boardGrant] }) });
    await click(button('Add project to key ab12cd34'));
    const select = target.querySelector('select') as HTMLSelectElement;
    select.value = LAUNCH;
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();
    for (const box of target.querySelectorAll<HTMLInputElement>('input[type="checkbox"]:checked'))
      await click(box);
    expect(button('Save grant')?.disabled).toBe(true);
    expect(calls).toHaveLength(0);
  });

  it('needs a second, explicit step before it sends all projects', async () => {
    const { calls, onChanged } = await render({ device: device({ grants: [boardGrant] }) });
    await click(button('Allow all projects for key ab12cd34'));
    expect(calls).toHaveLength(0);
    expect(target.textContent).toContain('Allow every project you can access?');
    await click(button('Cancel'));
    expect(calls).toHaveLength(0);
    await click(button('Allow all projects for key ab12cd34'));
    await click(button('Allow all projects'));
    expect(calls).toEqual([
      {
        method: 'PATCH',
        path: `/devices/${KEY}`,
        body: { project_access: 'all', confirm_all_projects: true }
      }
    ]);
    expect(onChanged).toHaveBeenCalledTimes(1);
  });

  it('marks an all-projects key as broad, keeps its grants as unused, and offers the way back', async () => {
    const { calls } = await render({
      device: device({ projectAccess: 'all', grants: [boardGrant] })
    });
    expect(target.textContent).toContain('All projects you can access');
    expect(target.textContent).toMatch(/can open every project you can access/);
    expect(target.textContent).toContain('Not used while all projects are allowed');
    expect(button(/Add project/)).toBeUndefined();
    await click(button('Use only granted projects for key ab12cd34'));
    expect(calls).toEqual([
      { method: 'PATCH', path: `/devices/${KEY}`, body: { project_access: 'granted' } }
    ]);
  });

  it('shows a grant for a project the account left as inert, with a way to remove it', async () => {
    await render({
      device: device({
        grants: [{ ...boardGrant, projectName: null, role: null }]
      })
    });
    expect(target.textContent).toContain('No access');
    expect(target.textContent).toMatch(/no longer a member/);
    expect(button(/^Change permissions/)).toBeUndefined();
    expect(button(/^Revoke grant/)).toBeTruthy();
  });

  it('maps a server refusal to a short sentence, never to raw server text', async () => {
    const { onChanged } = await render({ device: device({ grants: [boardGrant] }) }, () =>
      json(
        {
          error: 'conflict: db error 23505',
          details: { reason: 'A key can have at most 50 project grants' }
        },
        409
      )
    );
    await click(button('Add project to key ab12cd34'));
    const select = target.querySelector('select') as HTMLSelectElement;
    select.value = LAUNCH;
    select.dispatchEvent(new Event('change', { bubbles: true }));
    await settle();
    await click(button('Save grant'));
    const alert = target.querySelector('[role="alert"]');
    expect(alert?.textContent).toBe(
      'This key already has 50 project grants. Remove one before adding another.'
    );
    expect(target.textContent).not.toContain('23505');
    expect(onChanged).not.toHaveBeenCalled();
  });

  it('stops offering Add project at the limit', async () => {
    await render({ device: device({ grants: [boardGrant] }), grantLimit: 1 });
    expect(button(/Add project/)).toBeUndefined();
    expect(target.textContent).toContain('limit of 1 project grants');
  });
});
