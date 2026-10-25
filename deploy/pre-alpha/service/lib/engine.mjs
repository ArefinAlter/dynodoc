// A small client for the Dynodoc engine API on the stack's internal network (http://api:8080).
// The provisioner signs sample accounts in with the web tier's service key (`/auth/exchange`,
// what the web tier itself does after a Google sign-in); the service only forwards a person's
// own access token. Neither ever logs a token.

export class EngineError extends Error {
  constructor(method, path, status, body) {
    super(`${method} ${path.split('?')[0]}: ${status} ${typeof body === 'object' && body ? [body.error, body.details?.reason].filter(Boolean).join(' / ') : ''}`.trim());
    this.status = status;
    this.body = body;
  }
}

export function engine(base, { fetchImpl = fetch, timeoutMs = 20_000 } = {}) {
  async function call(method, path, { token, body, headers = {} } = {}) {
    const h = { accept: 'application/json', ...headers };
    if (token) h.authorization = `Bearer ${token}`;
    if (body !== undefined) h['content-type'] = 'application/json';
    const res = await fetchImpl(`${base}${path}`, {
      method,
      headers: h,
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: AbortSignal.timeout(timeoutMs),
    });
    const text = await res.text();
    let data = null;
    try {
      data = text ? JSON.parse(text) : null;
    } catch {
      data = { error: text.slice(0, 200) };
    }
    if (!res.ok) throw new EngineError(method, path, res.status, data);
    return data;
  }
  return {
    call,
    get: (path, token) => call('GET', path, { token }),
    post: (path, body, token, headers) => call('POST', path, { token, body, headers }),
    put: (path, body, token, headers) => call('PUT', path, { token, body, headers }),
    patch: (path, body, token) => call('PATCH', path, { token, body }),
    del: (path, token) => call('DELETE', path, { token }),
  };
}

/** Sessions for the sample accounts, kept in memory and renewed before they lapse. */
export function sessions(api, serviceKey) {
  const cache = new Map();
  const service = { 'x-dynodoc-service-key': serviceKey };
  async function token(email, displayName) {
    const now = Date.now();
    const held = cache.get(email);
    if (held && held.expiresAt > now + 120_000) return held.access;
    let tokens;
    if (held) {
      try {
        tokens = await api.post('/auth/refresh', { refresh_token: held.refresh });
      } catch {
        tokens = undefined;
      }
    }
    tokens ??= await api.post('/auth/exchange', { email, display_name: displayName }, undefined, service);
    cache.set(email, { access: tokens.access_token, refresh: tokens.refresh_token, expiresAt: now + tokens.expires_in * 1000 });
    return tokens.access_token;
  }
  async function magicLink(email, displayName) {
    return api.post('/auth/magic-link', { email, display_name: displayName }, undefined, service);
  }
  /** A one-off session (a visitor's, while their sandbox is built); log it out afterwards. */
  async function once(email, displayName) {
    return api.post('/auth/exchange', { email, display_name: displayName }, undefined, service);
  }
  async function logout(refreshToken) {
    try {
      await api.post('/auth/logout', { refresh_token: refreshToken });
    } catch {
      /* best effort */
    }
  }
  return { token, magicLink, once, logout };
}
