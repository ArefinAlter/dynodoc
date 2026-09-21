import { describe, expect, it } from 'vitest';
import { emptyState } from '$lib/materializer';
import { validRecovery, type RecoveryCopy } from './recovery';
const copy: RecoveryCopy = {
  version: 1,
  key: 'person:doc:team:tab',
  identityId: 'person',
  documentId: 'doc',
  draftId: '',
  draftName: '',
  draftRevision: 0,
  baseSeq: 3,
  base: emptyState(),
  state: emptyState(),
  updatedAt: 1000
};
describe('browser recovery boundaries', () => {
  it('requires the same signed-in identity and authorized document', () => {
    expect(validRecovery(copy, 'person', 'doc', 2000)).toBe(true);
    expect(validRecovery(copy, 'someone-else', 'doc', 2000)).toBe(false);
    expect(validRecovery(copy, 'person', 'other-doc', 2000)).toBe(false);
  });
  it('rejects expired, future, malformed and incompatible copies', () => {
    expect(validRecovery(copy, 'person', 'doc', 1000 + 7 * 86400000)).toBe(false);
    expect(validRecovery({ ...copy, updatedAt: 100000 }, 'person', 'doc', 2000)).toBe(false);
    expect(validRecovery({ ...copy, baseSeq: -1 }, 'person', 'doc', 2000)).toBe(false);
    expect(validRecovery({ ...copy, version: 2 }, 'person', 'doc', 2000)).toBe(false);
    expect(validRecovery({ ...copy, state: null }, 'person', 'doc', 2000)).toBe(false);
  });
});
