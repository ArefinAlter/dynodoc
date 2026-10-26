import { describe, expect, it } from 'vitest';
import { signInHref, workspaceHref } from './nav';

describe('workspace links', () => {
  it('sends Sign in to the workspace host, which has the sign-in methods', () => {
    expect(signInHref('https://app.dynodoc.online')).toBe('https://app.dynodoc.online/login');
    expect(signInHref('https://app.dynodoc.online/')).toBe('https://app.dynodoc.online/login');
    expect(workspaceHref('https://app.dynodoc.online')).toBe('https://app.dynodoc.online/projects');
  });

  it('stays on the same origin when no workspace host is configured', () => {
    expect(signInHref(undefined)).toBe('/login');
    expect(workspaceHref(undefined)).toBe('/projects');
  });
});
