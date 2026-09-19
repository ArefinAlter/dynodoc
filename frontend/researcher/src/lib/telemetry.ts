/** Anonymous page-load measurements: no cookies, persistent IDs, URLs or content. */
export async function observeVitals() {
  if (
    ![
      'dynodoc.online',
      'www.dynodoc.online',
      'app.dynodoc.online',
      'admin.dynodoc.online'
    ].includes(location.hostname) ||
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
    void fetch('https://app.dynodoc.online/telemetry', {
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
