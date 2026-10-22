/**
 * Outgoing mail for the web server: one entry point, `sendMail`, and the transport behind it.
 *
 *   resend  the hosted service's HTTPS mail provider (./resend.ts, unchanged)
 *   smtp    a mail server of the operator's choice (./smtp.js, loaded only when used)
 *   none    mail is off. `sendMail` refuses, `mailConfigured()` says false, and the callers tell
 *           people so: no sign-in by email, and an invitation link to copy instead of an email.
 *
 * Which transport runs is decided once by the configuration (../config/mail.js). Only two kinds
 * of message exist: the sign-in link and the invitation. Notifications are not emailed.
 */
import { getConfig } from '../config';
import { MailError } from './errors.js';
import { formatLog } from './log.js';
import { sendViaResend } from './resend';
import type { OutgoingMessage } from './message.js';

export type MailKind = 'signin' | 'invitation' | 'test';
export type MailMessage = OutgoingMessage & { kind: MailKind };

export type MailState = { transport: 'resend' | 'smtp' | 'none'; configured: boolean };
/** Which transport is in use and whether it can send. */
export function mailState(): MailState {
  const { mail } = getConfig();
  return { transport: mail.transport, configured: mail.configured };
}
export const mailConfigured = () => mailState().configured;

/**
 * The origin that links in mail are built on. A self-hosted installation always uses its
 * configured ORIGIN, never what a request claims; the hosted service keeps using the origin of
 * the request, as it always has.
 */
export function linkOrigin(requestOrigin: string): string {
  const config = getConfig();
  return config.mode === 'selfhosted' && config.origin ? config.origin : requestOrigin;
}

type Sender = { send: (message: MailMessage) => Promise<{ messageId: string }> };
const senders = new WeakMap<object, Promise<Sender>>();

/** The SMTP sender for this configuration, created on first use. */
function smtpSender(): Promise<Sender> {
  const smtp = getConfig().mail.smtp;
  if (!smtp) throw new MailError('config');
  let sender = senders.get(smtp);
  if (!sender) {
    sender = import('./smtp.js').then(
      ({ createSmtpSender }) =>
        createSmtpSender({
          host: smtp.host,
          port: smtp.port,
          security: smtp.security,
          user: smtp.user,
          password: smtp.password?.expose(),
          from: smtp.from,
          replyTo: smtp.replyTo,
          ca: smtp.ca ? [...smtp.ca] : undefined,
          timeoutMs: smtp.timeoutMs,
          helloName: smtp.helloName
        }) as Sender
    );
    senders.set(smtp, sender);
  }
  return sender;
}

/**
 * Send one message. Resolves when the transport accepted it; rejects when it did not, with an
 * error whose message holds no secret and no address. Callers answer people in their own
 * plain words and do not pass the error text on.
 */
export async function sendMail(message: MailMessage): Promise<{ messageId?: string }> {
  const config = getConfig();
  if (!config.mail.configured) throw new MailError('config');
  if (config.mail.transport === 'smtp') return (await smtpSender()).send(message);
  try {
    const sent = await sendViaResend(config, message);
    console.info(
      formatLog({
        provider: 'resend',
        outcome: 'sent',
        kind: message.kind,
        messageId: sent.messageId
      })
    );
    return sent;
  } catch (error) {
    console.warn(
      formatLog({
        provider: 'resend',
        outcome: 'failed',
        kind: message.kind,
        errorClass: /\((\d{3})\)/.test(String((error as Error).message))
          ? `http-${/\((\d{3})\)/.exec(String((error as Error).message))?.[1]}`
          : 'network'
      })
    );
    throw error;
  }
}
