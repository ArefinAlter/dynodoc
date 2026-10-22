/** Where and from which hosts to report; from the server configuration (`site.telemetry`). */
export type TelemetryTarget = { endpoint: string; hosts: string[] } | null | undefined;

/**
 * Anonymous page-load measurements: no cookies, persistent IDs, URLs or content. Does
 * nothing, and makes no request, unless the configuration names a destination and this host.
 */
export async function observeVitals(target: TelemetryTarget) {
  if (
    !target ||
    !target.hosts.includes(location.hostname) ||
    document.cookie.split(';').some((value) => value.trim() === 'dynodoc_metrics=off') ||
    navigator.doNotTrack === '1' ||
    (navigator as Navigator & { globalPrivacyControl?: boolean }).globalPrivacyControl
  )
    return;
  const page_id = crypto.randomUUID();
  const path = location.pathname;
  const page =
    path === '/'
      ? 'home'
      : path.startsWith('/documents/')
        ? 'editor'
        : ['privacy', 'terms', 'login', 'workspace', 'admin'].includes(path.slice(1))
          ? path.slice(1)
          : 'other';
  const send = (metric: string, value: number) => {
    const body = JSON.stringify({ page_id, page, metric, value });
    // JSON uses an explicit CORS preflight; omit authentication cookies on every host.
    // Plain-text cross-origin POSTs are correctly rejected by SvelteKit's form CSRF guard.
    void fetch(target.endpoint, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body,
      credentials: 'omit',
      keepalive: true
    }).catch(() => {});
  };
  send('visit', 1);
  const { onLCP, onCLS, onINP, onFCP, onTTFB } = await import('web-vitals');
  for (const observe of [onLCP, onCLS, onINP, onFCP, onTTFB]) observe((m) => send(m.name, m.value));
}
