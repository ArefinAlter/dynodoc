// The beacon route follows the configuration: off, it does nothing; on in a self-hosted
// installation, it accepts only that installation's own origins.
import { beforeEach, describe, expect, it, vi } from 'vitest';

const env = vi.hoisted(() => ({}) as Record<string, string | undefined>);
vi.mock('$env/dynamic/private', () => ({ env }));
const engine = vi.hoisted(() => ({ enginePost: vi.fn().mockResolvedValue({ ok: true }) }));
vi.mock('$lib/server/session', () => engine);

const SESSION = '4d7a0c3f9b2e6185d4a7c0f3b9e2618d4d7a0c3f9b2e6185d4a7c0f3b9e2618d';
const SERVICE = 'b5e2a9d6c3f01847b5e2a9d6c3f01847b5e2a9d6c3f01847b5e2a9d6c3f01847';
const sample = {
  page_id: 'a21f5eb4-28cf-4f78-a2c0-528cd74c8e17',
  metric: 'LCP',
  page: 'login',
  value: 1
};

async function route(values: Record<string, string>) {
  vi.resetModules();
  for (const key of Object.keys(env)) delete env[key];
  Object.assign(env, { SESSION_SECRET: SESSION, WEB_SERVICE_KEY: SERVICE }, values);
  return (await import('./+server')) as unknown as {
    OPTIONS: (event: unknown) => Promise<Response> | Response;
    POST: (event: unknown) => Promise<Response> | Response;
  };
}
const preflight = (origin: string) =>
  new Request('https://x.test/telemetry', {
    method: 'OPTIONS',
    headers: { origin, 'access-control-request-method': 'POST' }
  });
const beacon = (origin: string) =>
  new Request('https://x.test/telemetry', {
    method: 'POST',
    headers: { origin, 'content-type': 'application/json' },
    body: JSON.stringify(sample)
  });
beforeEach(() => engine.enginePost.mockClear());

describe('self-hosted, page metrics off (the default)', () => {
  const self = { DYNODOC_MODE: 'selfhosted', ORIGIN: 'https://dynodoc.example.test' };
  it('answers 404 to the preflight and the report, and stores nothing', async () => {
    const { OPTIONS, POST } = await route(self);
    expect((await OPTIONS({ request: preflight('https://dynodoc.example.test') })).status).toBe(
      404
    );
    expect((await POST({ request: beacon('https://dynodoc.example.test') })).status).toBe(404);
    // Not even the hosted origins get through.
    expect((await POST({ request: beacon('https://dynodoc.online') })).status).toBe(404);
    expect(engine.enginePost).not.toHaveBeenCalled();
  });
});

describe('self-hosted, page metrics on', () => {
  const on = {
    DYNODOC_MODE: 'selfhosted',
    ORIGIN: 'https://dynodoc.example.test',
    DYNODOC_TELEMETRY: 'on'
  };
  it('accepts only the installation’s own origin and labels it as the workspace', async () => {
    const { OPTIONS, POST } = await route(on);
    const own = 'https://dynodoc.example.test';
    expect((await OPTIONS({ request: preflight(own) })).status).toBe(204);
    expect((await POST({ request: beacon(own) })).status).toBe(204);
    expect(engine.enginePost).toHaveBeenCalledWith('/telemetry', { ...sample, host: 'workspace' });
    for (const origin of [
      'https://dynodoc.online',
      'https://app.dynodoc.online',
      'https://evil.test'
    ]) {
      expect((await OPTIONS({ request: preflight(origin) })).status, origin).toBe(403);
      expect((await POST({ request: beacon(origin) })).status, origin).toBe(403);
    }
    expect(engine.enginePost).toHaveBeenCalledTimes(1);
  });
});

describe('hosted, switched off by the operator', () => {
  it('does nothing', async () => {
    const { POST } = await route({ DYNODOC_TELEMETRY: 'off' });
    expect((await POST({ request: beacon('https://dynodoc.online') })).status).toBe(404);
    expect(engine.enginePost).not.toHaveBeenCalled();
  });
});
