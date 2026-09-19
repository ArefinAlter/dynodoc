import { describe, expect, it, vi } from 'vitest';
vi.mock('$env/dynamic/private', () => ({ env: { WEB_SERVICE_KEY: 'test-service-key' } }));
vi.mock('$lib/server/session', () => ({ enginePost: vi.fn().mockResolvedValue({ ok: true }) }));
import { enginePost } from '$lib/server/session';
import { OPTIONS, POST } from './+server';
const event = (request: Request) => ({ request }) as Parameters<typeof POST>[0];
describe('anonymous cross-host performance reporting', () => {
  it('permits JSON preflight only from a first-party origin and only for POST', async () => {
    for (const origin of ['https://dynodoc.online', 'https://admin.dynodoc.online']) {
      const response = await OPTIONS(
        event(
          new Request('https://app.dynodoc.online/telemetry', {
            method: 'OPTIONS',
            headers: { origin, 'access-control-request-method': 'POST' }
          })
        )
      );
      expect(response.status).toBe(204);
      expect(response.headers.get('access-control-allow-origin')).toBe(origin);
      expect(response.headers.get('access-control-allow-credentials')).toBeNull();
    }
    for (const [origin, method] of [
      ['https://attacker.test', 'POST'],
      ['https://dynodoc.online', 'DELETE']
    ]) {
      expect(
        (
          await OPTIONS(
            event(
              new Request('https://app.dynodoc.online/telemetry', {
                method: 'OPTIONS',
                headers: { origin, 'access-control-request-method': method }
              })
            )
          )
        ).status
      ).toBe(403);
    }
  });
  it('derives host from origin and forwards only bounded metric fields', async () => {
    const sample = {
      page_id: 'a21f5eb4-28cf-4f78-a2c0-528cd74c8e17',
      metric: 'visit',
      page: 'home',
      value: 1
    };
    const response = await POST(
      event(
        new Request('https://app.dynodoc.online/telemetry', {
          method: 'POST',
          headers: { origin: 'https://dynodoc.online', 'content-type': 'application/json' },
          body: JSON.stringify({
            ...sample,
            host: 'admin',
            email: 'must-not-be-stored@example.test',
            document_id: 'private'
          })
        })
      )
    );
    expect(response.status).toBe(204);
    expect(enginePost).toHaveBeenCalledWith('/telemetry', { ...sample, host: 'homepage' });
    expect(response.headers.get('access-control-allow-origin')).toBe('https://dynodoc.online');
  });
});
