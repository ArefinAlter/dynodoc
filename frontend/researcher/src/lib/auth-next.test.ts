import { it, expect } from 'vitest';
import { requiresSignIn, safeNext, withoutToken } from './auth-next';

it('takes the sign-in link token out of an address and keeps everything else', () => {
  const token = 'a'.repeat(64);
  expect(withoutToken(`/login?token=${token}`)).toBe('/login');
  expect(withoutToken(`/login?token=${token}&next=%2Fprojects`)).toBe('/login?next=%2Fprojects');
  expect(withoutToken(`/login?next=%2Fworkspace&token=${token}#top`)).toBe(
    '/login?next=%2Fworkspace#top'
  );
  // Twice, odd spelling, another origin in front: the token is never in the result.
  expect(withoutToken(`/login?token=${token}&token=${token}`)).toBe('/login');
  expect(withoutToken(`https://docs.example.org/login?token=${token}`)).toBe('/login');
  expect(withoutToken(new URL(`http://127.0.0.1:4175/login?token=${token}`))).toBe('/login');
  // Nothing to take out: unchanged.
  expect(withoutToken('/login')).toBe('/login');
  expect(withoutToken('/login?next=%2Fprojects')).toBe('/login?next=%2Fprojects');
  for (const address of [`/login?token=${token}`, `/login?x=1&token=${token}#${token}`])
    expect(withoutToken(address).includes('token=')).toBe(false);
});

it('requires a session for Projects and restores project destinations safely', () => {
  const path = '/projects/11111111-2222-3333-4444-555555555555';
  expect(requiresSignIn('/projects')).toBe(true);
  expect(requiresSignIn(path)).toBe(true);
  expect(safeNext(path + '?tab=requests')).toBe(path + '?tab=requests');
  expect(safeNext('//evil.test/projects')).toBeNull();
  expect(safeNext('/projects/../admin')).toBeNull();
  expect(requiresSignIn('/connectors/word')).toBe(false);
  expect(requiresSignIn('/profile')).toBe(true);
  expect(requiresSignIn('/profiles/11111111-2222-3333-4444-555555555555')).toBe(true);
  expect(safeNext('/profile')).toBe('/profile');
  const file = path + '/files/11111111-2222-3333-4444-555555555555?tab=history';
  expect(safeNext(file)).toBe(file);
  expect(safeNext('/profiles/../admin')).toBeNull();
});
