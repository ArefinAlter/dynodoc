import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RequestEvent } from './$types';

const mock = vi.hoisted(() => ({
  readSession: vi.fn(),
  enginePost: vi.fn(),
  clearSession: vi.fn()
}));
vi.mock('$env/dynamic/private', () => ({ env: {} }));
vi.mock('$lib/server/session', () => ({
  ...mock,
  saveSession: vi.fn(),
  accessToken: vi.fn(),
  engineUrl: () => 'http://engine.invalid'
}));
import { POST } from './+server';

describe('web logout', () => {
  beforeEach(() => vi.resetAllMocks());
  function event(origin = 'https://app.example.test') {
    return {
      params: { action: 'logout' },
      url: new URL('https://app.example.test/auth/logout'),
      request: new Request('https://app.example.test/auth/logout', {
        method: 'POST',
        headers: { origin }
      }),
      cookies: { delete: vi.fn() }
    } as unknown as RequestEvent;
  }
  it('revokes the cookie refresh lineage before clearing the browser session', async () => {
    mock.readSession.mockResolvedValue({ access_token: 'expired', refresh_token: 'old-secret' });
    mock.enginePost.mockImplementation(async () => {
      expect(mock.clearSession).not.toHaveBeenCalled();
      return { ok: true };
    });
    const request = event();
    expect((await POST(request)).status).toBe(200);
    expect(mock.enginePost).toHaveBeenCalledWith('/auth/logout', { refresh_token: 'old-secret' });
    expect(mock.clearSession).toHaveBeenCalledWith(request.cookies);
    expect(request.cookies.delete).toHaveBeenCalledTimes(2);
  });
  it('keeps the cookie and reports failure when revocation is unavailable', async () => {
    mock.readSession.mockResolvedValue({ refresh_token: 'secret' });
    mock.enginePost.mockRejectedValue(new Error('unavailable'));
    const request = event();
    expect((await POST(request)).status).toBe(503);
    expect(mock.clearSession).not.toHaveBeenCalled();
    expect(request.cookies.delete).not.toHaveBeenCalled();
  });
  it('is idempotent without a session and rejects cross-origin logout', async () => {
    mock.readSession.mockResolvedValue(null);
    expect((await POST(event())).status).toBe(200);
    expect(mock.enginePost).not.toHaveBeenCalled();
    vi.clearAllMocks();
    expect((await POST(event('https://untrusted.example'))).status).toBe(403);
    expect(mock.readSession).not.toHaveBeenCalled();
    expect(mock.clearSession).not.toHaveBeenCalled();
  });
});
