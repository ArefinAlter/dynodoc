/**
 * The Resend transport: the hosted service's mail, an HTTPS request to api.resend.com. What it
 * sends is pinned byte for byte by resend-golden.test.ts; change nothing here without that test.
 */
import type { WebConfig } from '../config';
import type { MailMessage } from './index';

export async function sendViaResend(
  config: Readonly<WebConfig>,
  message: MailMessage
): Promise<{ messageId?: string }> {
  const response = await fetch('https://api.resend.com/emails', {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${config.secrets.resendApiKey?.expose()}`,
      'content-type': 'application/json'
    },
    body: JSON.stringify({
      from: config.emailFrom,
      to: Array.isArray(message.to) ? message.to : [message.to],
      subject: message.subject,
      text: message.text,
      // The sign-in link has always gone out as text only; invitations carry the HTML too.
      ...(message.html && message.kind !== 'signin' ? { html: message.html } : {}),
      ...(message.replyTo ? { reply_to: message.replyTo } : {})
    }),
    signal: AbortSignal.timeout(15_000)
  });
  if (!response.ok) throw new Error(`Email delivery failed (${response.status})`);
  const sent = await response.json().catch(() => null);
  return { messageId: typeof sent?.id === 'string' ? sent.id : undefined };
}
