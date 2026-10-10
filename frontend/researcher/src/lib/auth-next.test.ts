import { it, expect } from 'vitest';
import { requiresSignIn, safeNext } from './auth-next';
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
