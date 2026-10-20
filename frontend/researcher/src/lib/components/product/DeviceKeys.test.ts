import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import DeviceKeys from './DeviceKeys.svelte';

// Synthetic values only. Shapes mirror docs/DEVICE-KEY-GRANTS-API.md (schema 42).
const KEY = '11111111-1111-4111-8111-111111111111';
const AUTHORITY = '33333333-3333-4333-8333-333333333333';
const IDENTITY = '44444444-4444-4444-8444-444444444444';
const CHALLENGE_ID = '55555555-5555-4555-8555-555555555555';
const PUBLIC = 'ab12cd34'.repeat(8);
const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });
const device = (extra: Record<string, unknown> = {}) => ({
  id: KEY,
  public_key: PUBLIC,
  registered_at: '2026-10-20T09:00:00Z',
  revoked_at: null,
  project_access: 'granted',
  class: 'person',
  label: null,
  grants: [],
  ...extra
});
const registry = (keys: unknown[]) => ({
  authority: AUTHORITY,
  identity: IDENTITY,
  active_limit: 20,
  total_limit: 128,
  grant_limit: 50,
  keys
});
const challenge = {
  id: CHALLENGE_ID,
  authority: AUTHORITY,
  identity: IDENTITY,
  public_key: PUBLIC
};

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
/** `keys` is what GET /devices lists; it changes after the enrolment is posted. */
async function render(keys: unknown[], afterEnrol: unknown[] = keys) {
  const calls: Call[] = [];
  let enrolled = false;
  vi.stubGlobal(
    'fetch',
    vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const call: Call = {
        method: init?.method ?? 'GET',
        path: String(input).replace('/api/engine', ''),
        body: init?.body ? JSON.parse(String(init.body)) : undefined
      };
      calls.push(call);
      if (call.path === '/devices' && call.method === 'GET')
        return json(registry(enrolled ? afterEnrol : keys));
      if (call.path === '/projects') return json({ items: [] });
      if (call.path === '/devices/challenge') return json(challenge);
      if (call.path === '/devices' && call.method === 'POST') {
        enrolled = true;
        const sent = call.body as { class?: string };
        return json({
          id: KEY,
          public_key: PUBLIC,
          project_access: 'granted',
          class: sent.class ?? 'person'
        });
      }
      return json({}, 404);
    })
  );
  target = document.createElement('div');
  document.body.appendChild(target);
  component = mount(DeviceKeys as never, { target }) as Record<string, unknown>;
  await settle();
  return { calls };
}
const radio = (name: RegExp) =>
  [...target.querySelectorAll<HTMLInputElement>('input[type="radio"]')].find((r) =>
    name.test(r.closest('label')?.textContent ?? '')
  );
const button = (name: RegExp) =>
  [...target.querySelectorAll('button')].find((b) => name.test(b.textContent ?? '')) as
    | HTMLButtonElement
    | undefined;
const field = (name: RegExp) =>
  [...target.querySelectorAll<HTMLInputElement>('label')]
    .find((l) => name.test(l.firstChild?.textContent ?? ''))
    ?.querySelector('input') as HTMLInputElement | undefined;
const type = async (input: HTMLInputElement | undefined, value: string) => {
  expect(input).toBeTruthy();
  input!.value = value;
  input!.dispatchEvent(new Event('input', { bubbles: true }));
  await settle();
};
async function createChallenge(publicKey = PUBLIC) {
  await type(field(/^Public key/), publicKey);
  button(/Create enrollment challenge/)!.click();
  await settle();
}
async function upload(proof: unknown) {
  const input = target.querySelector<HTMLInputElement>('input[type="file"]')!;
  const file = { size: 120, text: async () => JSON.stringify(proof) };
  Object.defineProperty(input, 'files', { value: [file], configurable: true });
  input.dispatchEvent(new Event('change', { bubbles: true }));
  await settle();
}
const signed = { challenge, signature: 'ab' };

describe('DeviceKeys: who a key is for', () => {
  it('asks who the key is for, and defaults to the person', async () => {
    await render([]);
    const me = radio(/This key is for me/);
    const agent = radio(/This key is for an AI agent/);
    expect(me?.checked).toBe(true);
    expect(agent?.checked).toBe(false);
    expect(target.querySelector('fieldset.key-for legend')?.textContent).toBe(
      'Who is this key for?'
    );
    // Nothing about a name until the person says the key is for an agent.
    expect(field(/^Name for this key/)).toBeUndefined();
  });

  it('asks for a name when the key is for an AI agent, and says it cannot change anything', async () => {
    await render([]);
    radio(/This key is for an AI agent/)!.click();
    await settle();
    const name = field(/^Name for this key/);
    expect(name?.required).toBe(true);
    expect(name?.maxLength).toBe(80);
    expect(target.textContent).toMatch(/It cannot change anything/);
    expect(target.textContent).toMatch(/Do\s+not put a secret in it/);
  });

  it('connects a key for me exactly as before: the proof is sent without class or name', async () => {
    const { calls } = await render([], [device()]);
    await createChallenge();
    await upload(signed);
    expect(calls.find((c) => c.method === 'POST' && c.path === '/devices')?.body).toEqual(signed);
    expect(target.querySelector('[role="status"]')?.textContent).toBe(
      'Signing key connected to your account.'
    );
    expect(target.querySelector('.repo-badge.agent')).toBeNull();
  });

  it('connects a key for an AI agent with its class and name, then lists it as one', async () => {
    const { calls } = await render([], [device({ class: 'agent', label: 'Claude on my laptop' })]);
    radio(/This key is for an AI agent/)!.click();
    await settle();
    await type(field(/^Name for this key/), '  Claude on my laptop ');
    await createChallenge();
    expect(target.textContent).toContain('AI agent key named “Claude on my laptop”');
    expect(target.querySelector('pre.key-command')?.textContent).toContain('--key agent.key');
    await upload(signed);
    expect(calls.find((c) => c.method === 'POST' && c.path === '/devices')?.body).toEqual({
      ...signed,
      class: 'agent',
      label: 'Claude on my laptop'
    });
    expect(target.querySelector('[role="status"]')?.textContent).toBe(
      'AI agent key connected to your account. It can read nothing until you grant it a project.'
    );
    const row = target.querySelector('.key-row');
    expect(row?.querySelector('.repo-badge.agent')?.textContent).toContain('AI agent');
    expect(row?.querySelector('.key-label')?.textContent).toBe('Claude on my laptop');
    // The choice is spent: the next key starts again as "for me".
    expect(radio(/This key is for me/)?.checked).toBe(true);
  });

  it('will not ask for a challenge for an agent key without a usable name', async () => {
    const { calls } = await render([]);
    radio(/This key is for an AI agent/)!.click();
    await settle();
    await type(field(/^Name for this key/), '   ');
    await createChallenge();
    expect(calls.some((c) => c.path === '/devices/challenge')).toBe(false);
    expect(target.querySelector('[role="alert"]')?.textContent).toMatch(/Give the key a name/);
  });

  it('maps an unknown-class refusal from the server to a plain sentence', async () => {
    await render([]);
    radio(/This key is for an AI agent/)!.click();
    await settle();
    await type(field(/^Name for this key/), 'Claude');
    await createChallenge();
    vi.stubGlobal(
      'fetch',
      vi.fn(async () =>
        json({ error: 'bad request', details: { reason: 'class must be person or agent' } }, 400)
      )
    );
    await upload(signed);
    expect(target.querySelector('[role="alert"]')?.textContent).toBe(
      'Choose whether this key is for you or for an AI agent, then try again.'
    );
  });
});

describe('DeviceKeys: the list', () => {
  it('marks a key for an AI agent with a badge and its name, and leaves other keys alone', async () => {
    const other = '22222222-2222-4222-8222-222222222222';
    await render([
      device({ class: 'agent', label: 'Claude on my laptop' }),
      device({ id: other, public_key: 'cd34ab12'.repeat(8) })
    ]);
    const rows = [...target.querySelectorAll('.key-row')];
    expect(rows).toHaveLength(2);
    expect(rows[0].querySelector('.repo-badge.agent')?.textContent).toContain('AI agent');
    expect(rows[0].querySelector('.key-label')?.textContent).toBe('Claude on my laptop');
    expect(rows[1].querySelector('.repo-badge.agent')).toBeNull();
    expect(rows[1].querySelector('.key-label')).toBeNull();
    expect(rows[0].textContent).toContain('For an AI agent.');
    expect(rows[1].textContent).not.toContain('For an AI agent.');
  });

  it('warns on revoke that the agent is refused from its next request', async () => {
    await render([device({ class: 'agent', label: 'Claude' })]);
    button(/^Revoke key$/)!.click();
    await settle();
    expect(target.textContent?.replace(/\s+/g, ' ')).toMatch(
      /AI agent using it is refused from its next request/
    );
  });
});
