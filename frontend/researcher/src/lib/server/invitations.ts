/**
 * Invitation emails sent when someone is given access to a document or workspace.
 * Access itself is granted by the engine; this only tells the person. Content is
 * built here so it can be tested without sending anything.
 */
import { getConfig } from './config';

export const ROLE_TEXT: Record<string, string> = {
  editor: 'Editor — you can edit the team version and merge changes',
  author: 'Editor — you can edit the team version and merge changes',
  reviewer: 'Reviewer — you can approve or request changes, comment and suggest',
  approver: 'Reviewer — you can approve or request changes, comment and suggest',
  contributor: 'Contributor — you can comment, suggest and send changes for review',
  viewer: 'Viewer — you can read and download',
  auditor: 'Viewer — you can read and download',
  owner: 'Owner',
  manager: 'Manager — you can share, set review rules and manage files'
};

export interface Invitation {
  to: string;
  inviterName: string;
  inviterEmail: string;
  itemName: string;
  kind: 'document' | 'workspace';
  role: string;
  link: string;
  note?: string;
}

const escapeHtml = (value: string) =>
  value.replace(
    /[&<>"']/g,
    (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!
  );
/** Plain printable text, one line for names, a few lines for notes. */
export const clean = (value: string, max: number, lines = false) =>
  [...value]
    .filter((c) => (c === '\n' ? lines : c.charCodeAt(0) >= 32 && c.charCodeAt(0) !== 127))
    .join('')
    .trim()
    .slice(0, max);

export function invitationEmail(invite: Invitation) {
  const who = invite.inviterName
    ? `${invite.inviterName} (${invite.inviterEmail})`
    : invite.inviterEmail;
  const what =
    invite.kind === 'document' ? `“${invite.itemName}”` : `the ${invite.itemName} workspace`;
  const role = ROLE_TEXT[invite.role] || invite.role;
  const subject = `${invite.inviterName || invite.inviterEmail} shared ${what} with you on Dynodoc`;
  const signIn = `Sign in with this email address (${invite.to}): choose “Continue with Google” if it is a Google account, or ask for an email sign-in link.`;
  const text = [
    `${who} shared ${what} with you on Dynodoc.`,
    `Your access: ${role}.`,
    invite.note ? `\nTheir note:\n${invite.note}\n` : '',
    `Open it: ${invite.link}`,
    '',
    signIn,
    '',
    `You are receiving this because ${invite.inviterEmail} added you. If you were not expecting it, you can ignore this email.`
  ]
    .filter((line, i, all) => line !== '' || all[i - 1] !== '')
    .join('\n');
  const html = `<div style="font-family:Arial,Helvetica,sans-serif;font-size:15px;line-height:1.55;color:#1d2f40;max-width:560px">
<p><strong>${escapeHtml(who)}</strong> shared ${escapeHtml(what)} with you on Dynodoc.</p>
<p>Your access: ${escapeHtml(role)}.</p>
${invite.note ? `<blockquote style="margin:16px 0;padding:10px 14px;border-left:3px solid #b9d0ee;background:#f3f6f9;white-space:pre-wrap">${escapeHtml(invite.note)}</blockquote>` : ''}
<p><a href="${escapeHtml(invite.link)}" style="display:inline-block;padding:10px 18px;border-radius:8px;background:#205bb0;color:#ffffff;text-decoration:none;font-weight:bold">Open in Dynodoc</a></p>
<p style="color:#46617a;font-size:13px">${escapeHtml(signIn)}</p>
<p style="color:#64788b;font-size:12px">You are receiving this because ${escapeHtml(invite.inviterEmail)} added you. If you were not expecting it, you can ignore this email.</p>
</div>`;
  return { subject, text, html };
}

/**
 * At most 30 invitation emails per person per hour, and one per recipient and item
 * every 10 minutes, so re-saving a role does not flood anyone's inbox.
 */
const perInviter = new Map<string, number[]>();
const perRecipient = new Map<string, number>();
export function allowInvitation(inviter: string, recipientKey: string, now = Date.now()) {
  const recent = (perInviter.get(inviter) || []).filter((t) => now - t < 3_600_000);
  const last = perRecipient.get(recipientKey);
  if (recent.length >= 30 || (last !== undefined && now - last < 600_000)) return false;
  recent.push(now);
  perInviter.set(inviter, recent);
  perRecipient.set(recipientKey, now);
  if (perRecipient.size > 10_000)
    for (const [key, time] of perRecipient) if (now - time > 600_000) perRecipient.delete(key);
  return true;
}

export const emailConfigured = () => {
  const config = getConfig();
  return Boolean(config.secrets.resendApiKey && config.emailFrom);
};

export async function sendEmail(message: {
  to: string;
  subject: string;
  text: string;
  html: string;
  replyTo?: string;
}) {
  const config = getConfig();
  const response = await fetch('https://api.resend.com/emails', {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${config.secrets.resendApiKey?.expose()}`,
      'content-type': 'application/json'
    },
    body: JSON.stringify({
      from: config.emailFrom,
      to: [message.to],
      subject: message.subject,
      text: message.text,
      html: message.html,
      ...(message.replyTo ? { reply_to: message.replyTo } : {})
    }),
    signal: AbortSignal.timeout(15_000)
  });
  if (!response.ok) throw new Error(`Email delivery failed (${response.status})`);
}
