import { describe, expect, it, vi } from 'vitest';

const env: { PUBLIC_WORKSPACE_URL?: string } = {};
vi.mock('$env/dynamic/public', () => ({ env }));

const { load } = await import('./+page.server');
const run = (href: string) => (load as (e: { url: URL }) => unknown)({ url: new URL(href) });

describe('/login', () => {
  it('forwards the marketing host to the workspace sign-in, keeping the query', () => {
    env.PUBLIC_WORKSPACE_URL = 'https://app.dynodoc.online';
    expect(() => run('https://dynodoc.online/login?next=%2Fprojects')).toThrow(
      expect.objectContaining({
        status: 307,
        location: 'https://app.dynodoc.online/login?next=%2Fprojects'
      })
    );
  });

  it('renders on the workspace host itself, so there is no loop', () => {
    env.PUBLIC_WORKSPACE_URL = 'https://app.dynodoc.online';
    expect(run('https://app.dynodoc.online/login?token=abc')).toBeUndefined();
  });

  it('renders anywhere when no workspace host is configured', () => {
    delete env.PUBLIC_WORKSPACE_URL;
    expect(run('https://dynodoc.online/login')).toBeUndefined();
  });
});
