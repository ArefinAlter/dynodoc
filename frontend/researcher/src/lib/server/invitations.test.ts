import { describe, expect, it } from 'vitest';
import { allowInvitation, clean, invitationEmail } from './invitations';

const invite = {
  to: 'tariq@example.test',
  inviterName: 'Olivia Owner',
  inviterEmail: 'olivia@example.test',
  itemName: 'Group report',
  kind: 'document' as const,
  role: 'contributor',
  link: 'https://app.dynodoc.online/documents/123'
};

describe('invitation emails', () => {
  it('names the inviter, the document, the role, the link and how to sign in', () => {
    const { subject, text, html } = invitationEmail({ ...invite, note: 'Please check section 2.' });
    expect(subject).toBe('Olivia Owner shared “Group report” with you on Dynodoc');
    expect(text).toContain('Olivia Owner (olivia@example.test) shared “Group report” with you');
    expect(text).toContain('Contributor — you can comment, suggest and send changes for review');
    expect(text).toContain('Please check section 2.');
    expect(text).toContain('Open it: https://app.dynodoc.online/documents/123');
    expect(text).toContain('Sign in with this email address (tariq@example.test)');
    expect(html).toContain('href="https://app.dynodoc.online/documents/123"');
  });
  it('escapes names and notes in the HTML version', () => {
    const { html } = invitationEmail({
      ...invite,
      itemName: '<script>alert(1)</script>',
      note: '"quoted" & <b>bold</b>'
    });
    expect(html).not.toContain('<script>');
    expect(html).toContain('&lt;script&gt;');
    expect(html).toContain('&quot;quoted&quot; &amp; &lt;b&gt;');
  });
  it('describes workspace invitations', () => {
    const { subject } = invitationEmail({ ...invite, kind: 'workspace', itemName: 'Field team' });
    expect(subject).toBe('Olivia Owner shared the Field team workspace with you on Dynodoc');
  });
  it('strips control characters and keeps note line breaks only when asked', () => {
    expect(clean('Group\u0000 report\n', 50)).toBe('Group report');
    expect(clean('line one\nline two', 50, true)).toBe('line one\nline two');
    expect(clean('x'.repeat(600), 500)).toHaveLength(500);
  });
  it('limits repeat invitations to the same person and bursts from one inviter', () => {
    const now = 1_000_000;
    expect(allowInvitation('olivia', '/documents/a:t@x', now)).toBe(true);
    expect(allowInvitation('olivia', '/documents/a:t@x', now + 60_000)).toBe(false);
    expect(allowInvitation('olivia', '/documents/a:t@x', now + 700_000)).toBe(true);
    for (let i = 0; i < 28; i++) allowInvitation('olivia', `/documents/b:${i}@x`, now + 700_001);
    expect(allowInvitation('olivia', '/documents/c:new@x', now + 700_002)).toBe(false);
    expect(allowInvitation('olivia', '/documents/c:new@x', now + 4_400_000)).toBe(true);
  });
});
