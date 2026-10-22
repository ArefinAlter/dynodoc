import { json } from '@sveltejs/kit';
import type { RequestHandler } from './$types';
import { accessToken, engineUrl } from '$lib/server/session';
import { getConfig } from '$lib/server/config';
import { linkOrigin } from '$lib/server/mail';
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
  // What was shared, and the link to it, built on the configured origin. The link goes in the
  // email, or, when no email goes out, is shown to the person who invited so they can pass it on.
  const lookUp = async () => {
    if (uuid.test(documentId)) {
      const doc = await engine(`/documents/${documentId}`).then((r) => (r.ok ? r.json() : null));
      return { itemName: clean(String(doc?.title || 'a document'), 200), project: false };
    }
    const spaces = await engine('/spaces').then((r) => (r.ok ? r.json() : null));
    const space = (spaces?.items || []).find((s: { id: string }) => s.id === spaceId);
    return {
      itemName: clean(String(space?.name || 'team'), 160),
      project: space?.kind === 'project'
    };
  };
  const linkTo = (project: boolean) => {
    const origin = linkOrigin(url.origin);
    return uuid.test(documentId)
      ? `${origin}/documents/${documentId}`
      : project
        ? `${origin}/projects/${spaceId}`
        : `${origin}/workspace`;
  };
  // A document's link needs no lookup. A workspace's does (project or not); without it, no link.
  const linkWithoutLookup = () =>
    uuid.test(documentId)
      ? Promise.resolve(linkTo(false))
      : lookUp().then(
          (found) => linkTo(found.project),
          () => undefined
        );
  let invitation_link: string | undefined;
  if (body?.notify !== false && role !== 'remove') {
    const me = await engine('/auth/me').then((r) => (r.ok ? r.json() : null));
    if (!emailConfigured()) {
      email_status = 'not_configured';
      // No mail goes out, so the person who invited gets the link to pass on themselves.
      invitation_link = await linkWithoutLookup();
    } else if (!me || me.email === email) email_status = 'skipped';
    else if (!allowInvitation(me.identity_id, `${target}:${email}`)) email_status = 'rate_limited';
    else {
      try {
        const { itemName, project } = await lookUp();
        const link = linkTo(project);
        invitation_link = link;
        const config = getConfig();
        // A self-hosted installation names its own product and the sign-in methods it really has.
        const self =
          config.mode === 'selfhosted'
            ? {
                brand: config.site.brand,
                google: Boolean(config.googleClientId && config.secrets.googleClientSecret),
                email: config.emailSignIn
              }
            : {};
        const message = invitationEmail({
          to: email,
          inviterName: clean(String(me.display_name || ''), 120),
          inviterEmail: me.email,
          itemName,
          kind: uuid.test(documentId) ? 'document' : 'workspace',
          role,
          link,
          note: clean(String(body?.note || ''), 500, true) || undefined,
          ...self
        });
        await sendEmail({ to: email, ...message, replyTo: me.email });
        email_status = 'sent';
        invitation_link = undefined;
      } catch {
        email_status = 'failed';
        invitation_link ??= await linkWithoutLookup();
      }
    }
  }
  return json({ ...result, email_status, ...(invitation_link ? { invitation_link } : {}) });
};
