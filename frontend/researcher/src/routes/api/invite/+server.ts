import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { accessToken, engineUrl } from '$lib/server/session';
import {
  allowInvitation,
  clean,
  emailConfigured,
  invitationEmail,
  sendEmail
} from '$lib/server/invitations';

const uuid = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

/**
 * Give someone access to a document or workspace, then (optionally) email them an
 * invitation. The engine decides whether the caller may share; nothing is sent
 * unless it succeeded.
 */
export const POST: RequestHandler = async ({ request, url, cookies }) => {
  if (request.headers.get('origin') !== url.origin)
    return json({ error: 'Invalid origin' }, { status: 403 });
  const token = await accessToken(cookies, url.protocol === 'https:');
  if (!token) return json({ error: 'Please sign in' }, { status: 401 });
  const body = await request.json().catch(() => null);
  const documentId = String(body?.document_id || ''),
    spaceId = String(body?.space_id || '');
  const email = String(body?.email || '')
    .trim()
    .toLowerCase();
  const role = String(body?.role || '');
  if ((!uuid.test(documentId) && !uuid.test(spaceId)) || !email || !role)
    return json(
      { error: 'Choose a document or workspace, an email address and a role.' },
      { status: 400 }
    );

  const engine = (path: string, init?: RequestInit) =>
    fetch(`${engineUrl()}${path}`, {
      ...init,
      headers: {
        authorization: `Bearer ${token}`,
        'content-type': 'application/json',
        ...(init?.headers || {})
      }
    });
  const target = uuid.test(documentId) ? `/documents/${documentId}` : `/spaces/${spaceId}`;
  const shared = await engine(`${target}/members`, {
    method: 'POST',
    body: JSON.stringify({ email, role })
  });
  const result = await shared.json().catch(() => ({}));
  if (!shared.ok) return json(result, { status: shared.status });

  let email_status: 'sent' | 'skipped' | 'not_configured' | 'rate_limited' | 'failed' = 'skipped';
  if (body?.notify !== false && role !== 'remove') {
    const me = await engine('/auth/me').then((r) => (r.ok ? r.json() : null));
    if (!emailConfigured()) email_status = 'not_configured';
    else if (!me || me.email === email) email_status = 'skipped';
    else if (!allowInvitation(me.identity_id, `${target}:${email}`)) email_status = 'rate_limited';
    else {
      try {
        let itemName = 'Dynodoc',
          project = false;
        if (uuid.test(documentId)) {
          const doc = await engine(`/documents/${documentId}`).then((r) =>
            r.ok ? r.json() : null
          );
          itemName = clean(String(doc?.title || 'a document'), 200);
        } else {
          const spaces = await engine('/spaces').then((r) => (r.ok ? r.json() : null));
          const space = (spaces?.items || []).find((s: { id: string }) => s.id === spaceId);
          itemName = clean(String(space?.name || 'team'), 160);
          project = space?.kind === 'project';
        }
        const link = uuid.test(documentId)
          ? `${url.origin}/documents/${documentId}`
          : project
            ? `${url.origin}/projects/${spaceId}`
            : `${url.origin}/workspace`;
        const message = invitationEmail({
          to: email,
          inviterName: clean(String(me.display_name || ''), 120),
          inviterEmail: me.email,
          itemName,
          kind: uuid.test(documentId) ? 'document' : 'workspace',
          role,
          link,
          note: clean(String(body?.note || ''), 500, true) || undefined
        });
        await sendEmail({ to: email, ...message, replyTo: me.email });
        email_status = 'sent';
      } catch {
        email_status = 'failed';
      }
    }
  }
  return json({ ...result, email_status });
};
