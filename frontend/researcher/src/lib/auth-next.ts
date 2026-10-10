/** Pages that need a signed-in account. Visitors without a session are sent to sign in first. */
export function requiresSignIn(pathname: string) {
  return (
    pathname === '/connectors/authorize' ||
    pathname === '/workspace' ||
    pathname === '/projects' ||
    pathname.startsWith('/projects/') ||
    pathname.startsWith('/documents/')
  );
}

/**
 * Where to return after sign-in. Only same-origin workspace paths are accepted, so a crafted
 * `next` parameter cannot redirect someone to another site after they authenticate.
 */
export function safeNext(next: string | null | undefined): string | null {
  if (!next || next.length > 500 || !next.startsWith('/') || /^\/[\\/]/.test(next)) return null;
  if ([...next].some((c) => c.charCodeAt(0) < 32)) return null;
  const path = next.split(/[?#]/)[0];
  return path === '/connectors/authorize' ||
    path === '/workspace' ||
    path === '/projects' ||
    /^\/(documents|projects)\/[0-9a-f-]{36}$/i.test(path)
    ? next
    : null;
}

const key = 'dynodoc.login-next';
/** Email links open in a new tab, so the destination is kept briefly in this browser. */
export function rememberNext(next: string | null) {
  try {
    if (next) localStorage.setItem(key, JSON.stringify({ next, at: Date.now() }));
    else localStorage.removeItem(key);
  } catch {
    /* Storage can be unavailable in private browsing. */
  }
}
export function takeRememberedNext(): string | null {
  try {
    const saved = JSON.parse(localStorage.getItem(key) || 'null');
    localStorage.removeItem(key);
    return saved && Date.now() - saved.at < 20 * 60_000 ? safeNext(saved.next) : null;
  } catch {
    return null;
  }
}
