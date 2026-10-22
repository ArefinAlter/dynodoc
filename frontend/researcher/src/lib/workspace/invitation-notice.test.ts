// What the person who invited is told about the invitation email, in plain words. With no mail
// set up, or when it failed, they are given the link to pass on themselves.
import { describe, expect, it } from 'vitest';
import { invitationNotice } from './collaboration';

const LINK = 'https://docs.example.org/documents/123';

describe('invitationNotice', () => {
  it('says plainly that nothing was emailed when mail is not set up, and shows the link', () => {
    expect(invitationNotice('not_configured', 'tariq@example.test', LINK)).toBe(
      `Access updated. Email is not set up on this server, so no invitation was sent. Send tariq@example.test this link yourself: ${LINK}`
    );
  });
  it('shows the link too when the email could not be sent', () => {
    expect(invitationNotice('failed', 'tariq@example.test', LINK)).toBe(
      `Access updated, but the invitation email could not be sent. Send tariq@example.test this link yourself: ${LINK}`
    );
  });
  it('keeps the earlier words when there is no link to show', () => {
    expect(invitationNotice('not_configured', 'tariq@example.test')).toBe(
      'Access updated. Email is not set up on this server, so send tariq@example.test the link yourself.'
    );
    expect(invitationNotice('failed', 'tariq@example.test')).toBe(
      'Access updated, but the invitation email could not be sent. Copy the document link and send it to tariq@example.test yourself.'
    );
  });
  it('is unchanged for the other outcomes, and never shows a link they did not need', () => {
    expect(invitationNotice('sent', 'tariq@example.test', LINK)).toBe(
      'Access updated. We emailed tariq@example.test an invitation with a link.'
    );
    expect(invitationNotice('rate_limited', 'tariq@example.test', LINK)).not.toContain(LINK);
    expect(invitationNotice('skipped', 'tariq@example.test', LINK)).toBe(
      'Access updated. They will see it in their workspace when they sign in.'
    );
  });
});
