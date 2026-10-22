// The server stops at start with one aggregated error, in both modes; a development server
// only warns. Secrets never appear in what is printed.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

const env = vi.hoisted(() => ({}) as Record<string, string | undefined>);
vi.mock('$env/dynamic/private', () => ({ env }));

const SESSION = 'c1d8e5f20a7b3946c1d8e5f20a7b3946c1d8e5f20a7b3946c1d8e5f20a7b3946';
const SERVICE = '9e3a6d0b4f8c1275e9a3d6b0f4c8e1579e3a6d0b4f8c1275e9a3d6b0f4c8e157';
const PLACEHOLDER = 'change-me-to-a-long-random-value-please';

async function boot(dev: boolean, values: Record<string, string>) {
  vi.resetModules();
  vi.doMock('$app/environment', () => ({ dev, building: false, browser: false, version: 'test' }));
  for (const key of Object.keys(env)) delete env[key];
  Object.assign(env, values);
  return import('./index');
}

let warn: ReturnType<typeof vi.spyOn>;
beforeEach(() => {
  warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
});
afterEach(() => {
  warn.mockRestore();
  vi.doUnmock('$app/environment');
});
const printed = () => warn.mock.calls.map((call) => call.join(' ')).join('\n');

describe.each([
  ['hosted', {}],
  ['selfhosted', { DYNODOC_MODE: 'selfhosted', ORIGIN: 'https://dynodoc.example.test' }]
])('a production server in %s mode', (_mode, mode) => {
  const good = { ...mode, SESSION_SECRET: SESSION, WEB_SERVICE_KEY: SERVICE };
  it('starts when the configuration is valid', async () => {
    const { startupCheck, getConfig } = await boot(false, good);
    expect(() => startupCheck()).not.toThrow();
    expect(getConfig().secrets.session?.expose()).toBe(SESSION);
  });
  it('refuses a missing SESSION_SECRET at start, not at the first request', async () => {
    const { startupCheck, getConfig } = await boot(false, { ...good, SESSION_SECRET: '' });
    expect(() => startupCheck()).toThrow(
      /^invalid configuration \(1 problem\):\n {2}- SESSION_SECRET is not set/
    );
    expect(() => getConfig()).toThrow(/SESSION_SECRET is not set/);
  });
  it('refuses a documented placeholder as SESSION_SECRET', async () => {
    const { startupCheck } = await boot(false, { ...good, SESSION_SECRET: PLACEHOLDER });
    let message = '';
    try {
      startupCheck();
    } catch (error) {
      message = (error as Error).message;
    }
    expect(message).toContain('SESSION_SECRET looks like a placeholder');
    expect(message).not.toContain(PLACEHOLDER);
  });
  it('reports every problem in one error', async () => {
    const { startupCheck } = await boot(false, {
      ...good,
      SESSION_SECRET: 'short',
      WEB_SERVICE_KEY: 'short-too',
      ENGINE_API_URL: 'nope',
      DYNODOC_TELEMETRY: 'maybe'
    });
    let message = '';
    try {
      startupCheck();
    } catch (error) {
      message = (error as Error).message;
    }
    expect(message.startsWith('invalid configuration (4 problems):')).toBe(true);
    expect(message).not.toContain('short-too');
  });
  it('prints warnings, never a secret', async () => {
    const { startupCheck } = await boot(false, { ...good, DYNODOC_TELEMTRY: 'on' });
    startupCheck();
    expect(printed()).toContain(
      'DYNODOC_TELEMTRY is not a known setting; did you mean DYNODOC_TELEMETRY?'
    );
    expect(printed()).not.toContain(SESSION);
    expect(printed()).not.toContain(SERVICE);
  });
});

describe('a development server', () => {
  it('warns about the same problems and keeps serving the public pages', async () => {
    const { startupCheck, getConfig } = await boot(true, { SESSION_SECRET: '' });
    expect(() => startupCheck()).not.toThrow();
    expect(printed()).toContain('configuration problem: SESSION_SECRET is not set');
    expect(getConfig().mode).toBe('hosted');
  });
});

describe('the public environment', () => {
  it('reaches the configuration through the start hook', async () => {
    const { startupCheck, getConfig } = await boot(false, {
      SESSION_SECRET: SESSION,
      WEB_SERVICE_KEY: SERVICE
    });
    startupCheck({ PUBLIC_WORKSPACE_URL: 'https://app.example.test/' });
    expect(getConfig().workspaceUrl).toBe('https://app.example.test');
  });
});
