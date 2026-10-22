// The message texts: what they say, that every interpolated value is escaped in the HTML, and
// that the HTML carries nothing remote (no images, no tracking, no styles or scripts to load).
import { describe, expect, it } from 'vitest';
import { escapeHtml, signInEmail } from './templates';
import { invitationEmail } from '../invitations';

const LINK = 'https://docs.example.org/auth/verify?token=a%2Bb%3D&x=1';
const invite = {
  to: 'tariq@example.test',
  inviterName: 'Olivia Owner',
  inviterEmail: 'olivia@example.test',
  itemName: 'Group report',
  kind: 'document' as const,
  role: 'contributor',
  link: 'https://docs.example.org/documents/1'
};
/** The tags of an HTML message and what each carries, read the way a mail client would. */
function structure(html: string) {
  const tags = [...html.matchAll(/<(\/?)([a-z][a-z0-9]*)([^>]*)>/gi)]
    .filter((m) => m[1] === '')
    .map((m) => ({
      name: m[2].toLowerCase(),
      attributes: [...m[3].matchAll(/([a-z-]+)="([^"]*)"/gi)].map((a) => [a[1].toLowerCase(), a[2]])
    }));
  return tags;
}
const ALLOWED_TAGS = ['a', 'blockquote', 'br', 'div', 'p', 'strong'];
/** Nothing remote or active: only plain tags, only style and href, nothing in a style that loads. */
function expectInert(html: string, origin: string) {
  const tags = structure(html);
  for (const tag of tags) {
    expect(ALLOWED_TAGS, tag.name).toContain(tag.name);
    for (const [name, value] of tag.attributes) {
      expect(['style', 'href'], name).toContain(name);
      if (name === 'style')
        expect(value).not.toMatch(/url\(|@import|expression|javascript:|behavior/i);
      if (name === 'href') expect(value.startsWith(origin)).toBe(true);
    }
  }
  expect(html).not.toMatch(
    /<\/?(img|link|script|iframe|style|object|embed|form|video|audio|svg|meta|base)\b/i
  );
}

describe('escapeHtml', () => {
  it('escapes the five characters that matter, and nothing else', () => {
    expect(escapeHtml(`<a href="x" onclick='y'>&</a>`)).toBe(
      '&lt;a href=&quot;x&quot; onclick=&#39;y&#39;&gt;&amp;&lt;/a&gt;'
    );
    expect(escapeHtml('plain text, ünïcode ✓')).toBe('plain text, ünïcode ✓');
  });
});

describe('the sign-in message', () => {
  it('says what the hosted service has always said, under the default name', () => {
    const { subject, text } = signInEmail({ link: LINK });
    expect(subject).toBe('Sign in to Dynodoc');
    expect(text).toBe(
      `Sign in to your Dynodoc workspace:\n\n${LINK}\n\nThis link expires in 15 minutes. If you did not request it, you can ignore this message.`
    );
  });
  it('uses the installation’s own product name in the subject, the text and the HTML', () => {
    const { subject, text, html } = signInEmail({ link: LINK, brand: 'Acme Docs' });
    expect(subject).toBe('Sign in to Acme Docs');
    expect(text).toContain('Sign in to your Acme Docs workspace:');
    expect(html).toContain('Sign in to your Acme Docs workspace:');
    expect(html).toContain('>Sign in to Acme Docs</a>');
    expect(`${subject}${text}${html}`).not.toContain('Dynodoc');
  });
  it('puts the link in the button and in plain view, escaped, and nowhere else', () => {
    const { html } = signInEmail({ link: LINK });
    const escaped = escapeHtml(LINK);
    expect(escaped).toContain('&amp;x=1');
    expect(html.split(escaped)).toHaveLength(3); // the button and the fallback line
    expect(html).toContain(`href="${escaped}"`);
    expect(html.match(/href=/g)).toHaveLength(1);
    expect(html.match(/https?:\/\//g)).toHaveLength(2);
  });
  it('escapes a hostile name and a hostile link, and loads nothing', () => {
    const { html } = signInEmail({
      link: 'https://docs.example.org/?q="><script>alert(1)</script>',
      brand: '<img src=x onerror=alert(1)> & "Co"'
    });
    expectInert(html, 'https://docs.example.org/');
    expect(structure(html).map((t) => t.name)).toEqual(['div', 'p', 'p', 'a', 'p', 'br', 'p']);
    expect(html).not.toContain('<script');
    expect(html).toContain('&lt;img src=x onerror=alert(1)&gt; &amp; &quot;Co&quot;');
    expect(html).toContain('?q=&quot;&gt;&lt;script&gt;');
  });
  it('is a simple, self-contained document: inline styles only, no remote part', () => {
    const { html } = signInEmail({ link: LINK, brand: 'Acme Docs' });
    expectInert(html, 'https://docs.example.org/');
    expect(structure(html).map((t) => t.name)).toEqual(['div', 'p', 'p', 'a', 'p', 'br', 'p']);
  });
});

describe('the invitation message', () => {
  it('keeps the hosted wording under the default settings', () => {
    const { subject, text } = invitationEmail(invite);
    expect(subject).toBe('Olivia Owner shared “Group report” with you on Dynodoc');
    expect(text).toContain(
      'Sign in with this email address (tariq@example.test): choose “Continue with Google” if it is a Google account, or ask for an email sign-in link.'
    );
  });
  it('names the installation and only the sign-in methods it really has', () => {
    const both = invitationEmail({ ...invite, brand: 'Acme Docs', google: true, email: true });
    expect(both.subject).toBe('Olivia Owner shared “Group report” with you on Acme Docs');
    expect(both.text).toContain('on Acme Docs.');
    expect(both.html).toContain('>Open in Acme Docs</a>');
    expect(`${both.subject}${both.text}${both.html}`).not.toContain('Dynodoc');
    expect(invitationEmail({ ...invite, google: false, email: true }).text).toContain(
      '(tariq@example.test): ask for an email sign-in link.'
    );
    expect(invitationEmail({ ...invite, google: true, email: false }).text).toContain(
      '(tariq@example.test): choose “Continue with Google”.'
    );
    const neither = invitationEmail({ ...invite, google: false, email: false }).text;
    expect(neither).toContain('Sign in with this email address (tariq@example.test).');
    expect(neither).not.toMatch(/Google|email sign-in link/);
  });
  it('escapes every interpolated value in the HTML', () => {
    const hostile = '<img src=x onerror=alert(1)> & "q" \'s\'';
    const escaped = escapeHtml(hostile);
    for (const field of [
      'inviterName',
      'inviterEmail',
      'itemName',
      'role',
      'note',
      'to',
      'brand',
      'link'
    ] as const) {
      const { html } = invitationEmail({ ...invite, [field]: hostile });
      if (field !== 'link') expectInert(html, 'https://docs.example.org/');
      else expect(structure(html).filter((t) => t.name === 'a')).toHaveLength(1);
      expect(html, field).toContain(escaped);
      expect(html, field).not.toContain(hostile);
    }
  });
  it('has no remote part in an ordinary message either', () => {
    const { html } = invitationEmail({
      ...invite,
      note: 'Please review section 2.',
      brand: 'Acme Docs'
    });
    expectInert(html, 'https://docs.example.org/');
    expect(html.match(/href=/g)).toHaveLength(1);
    expect(html.match(/https?:\/\//g)).toHaveLength(1);
  });
});
