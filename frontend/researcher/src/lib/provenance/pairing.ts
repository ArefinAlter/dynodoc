import type { Host } from './protocol';

export const HOST_LABELS: Record<Host, string> = {
  word: 'Microsoft Word',
  excel: 'Microsoft Excel',
  powerpoint: 'Microsoft PowerPoint',
  'google-docs': 'Google Docs',
  'google-sheets': 'Google Sheets',
  'google-slides': 'Google Slides'
};
export interface Pairing {
  token: string;
  challenge: string;
  url: string;
  expires: number;
}
const hex = (bytes: Uint8Array) => [...bytes].map((b) => b.toString(16).padStart(2, '0')).join('');

/** The approval browser sees only a hash. Only the editor possesses the bearer secret. */
export async function beginPairing(origin: string, host: Host): Promise<Pairing> {
  const base = new URL(origin);
  if (
    base.origin !== origin ||
    (base.protocol !== 'https:' &&
      !(base.protocol === 'http:' && ['localhost', '127.0.0.1'].includes(base.hostname)))
  )
    throw new Error('Use your HTTPS Dynodoc workspace address.');
  const token = hex(crypto.getRandomValues(new Uint8Array(32)));
  const challenge = hex(
    new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(token)))
  );
  const expires = Math.floor(Date.now() / 1000) + 600;
  const url = new URL('/connectors/authorize', origin);
  url.search = new URLSearchParams({ host, challenge, expires: String(expires) }).toString();
  return { token, challenge, url: url.href, expires };
}

export async function finishPairing(origin: string, host: Host, pairing: Pairing) {
  if (Date.now() / 1000 >= pairing.expires)
    throw new Error('Sign-in expired. Start again in this pane.');
  const response = await fetch(`${origin}/api/connectors/connection`, {
    headers: { authorization: `Bearer ${pairing.token}` },
    credentials: 'omit',
    cache: 'no-store',
    redirect: 'error'
  });
  if (response.status === 401)
    throw new Error('Approve this connection in your browser, then try again.');
  const data = await response.json();
  if (!response.ok) throw new Error(data.error || 'Could not connect.');
  if (data.host !== host) throw new Error('This connection belongs to a different editor.');
  return { documentId: data.document_id as string, token: pairing.token };
}
