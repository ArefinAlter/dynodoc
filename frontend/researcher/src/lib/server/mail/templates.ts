/**
 * The text of the messages the web server sends, as a subject, a plain-text body and a simple
 * HTML alternative built from the same words. Pure, so tests can read every message without
 * sending one. Rules for the HTML: every interpolated value is escaped, there are no images,
 * no tracking pixels, no remote styles or scripts, and the only link is the one given (built
 * from the configured origin by the caller, see linkOrigin in ./index.ts).
 */
export const escapeHtml = (value: string) =>
  value.replace(
    /[&<>"']/g,
    (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!
  );

const FONT =
  'font-family:Arial,Helvetica,sans-serif;font-size:15px;line-height:1.55;color:#1d2f40;max-width:560px';
const BUTTON =
  'display:inline-block;padding:10px 18px;border-radius:8px;background:#205bb0;color:#ffffff;text-decoration:none;font-weight:bold';

/**
 * The sign-in link message. With the default product name the subject and the text are exactly
 * what the hosted service has always sent (the Resend transport sends no HTML for it).
 */
export function signInEmail(options: { link: string; brand?: string }) {
  const brand = options.brand || 'Dynodoc';
  const expiry =
    'This link expires in 15 minutes. If you did not request it, you can ignore this message.';
  const subject = `Sign in to ${brand}`;
  const text = `Sign in to your ${brand} workspace:\n\n${options.link}\n\n${expiry}`;
  const html = `<div style="${FONT}">
<p>Sign in to your ${escapeHtml(brand)} workspace:</p>
<p><a href="${escapeHtml(options.link)}" style="${BUTTON}">Sign in to ${escapeHtml(brand)}</a></p>
<p style="color:#46617a;font-size:13px">If the button does not open, copy this address into your browser:<br>${escapeHtml(options.link)}</p>
<p style="color:#64788b;font-size:12px">${escapeHtml(expiry)}</p>
</div>`;
  return { subject, text, html };
}
