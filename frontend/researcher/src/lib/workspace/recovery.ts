import type { DocumentState } from '$lib/api/types';

/** Optional browser recovery. Never stores tokens; opening still requires server access. */
export interface RecoveryCopy {
  key: string;
  identityId: string;
  documentId: string;
  draftId: string;
  draftName: string;
  draftRevision: number;
  baseSeq: number;
  base: DocumentState;
  state: DocumentState;
  updatedAt: number;
  version: 1;
}
const DATABASE = 'dynodoc-browser-recovery';
const STORE = 'copies';
const MAX_AGE = 7 * 24 * 60 * 60 * 1000;
const MAX_BYTES = 10 * 1024 * 1024;
const MAX_COPIES = 30;
function open(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const request = indexedDB.open(DATABASE, 1);
    request.onupgradeneeded = () => request.result.createObjectStore(STORE, { keyPath: 'key' });
    request.onsuccess = () => resolve(request.result);
    request.onerror = () => reject(new Error('Browser recovery storage is unavailable.'));
    request.onblocked = () =>
      reject(new Error('Close older Dynodoc tabs to enable browser recovery.'));
  });
}
async function transact<T>(
  mode: IDBTransactionMode,
  operation: (store: IDBObjectStore) => IDBRequest<T>
): Promise<T> {
  const db = await open();
  return new Promise((resolve, reject) => {
    let tx: IDBTransaction;
    let request: IDBRequest<T>;
    try {
      tx = db.transaction(STORE, mode);
      request = operation(tx.objectStore(STORE));
    } catch (error) {
      db.close();
      reject(error);
      return;
    }
    tx.oncomplete = () => {
      db.close();
      resolve(request.result);
    };
    tx.onabort = () => {
      db.close();
      reject(new Error('Browser recovery could not be saved. Check available storage.'));
    };
    tx.onerror = () => {
      /* onabort handles the failed transaction. */
    };
  });
}
export function validRecovery(
  value: unknown,
  identityId: string,
  documentId: string,
  now = Date.now()
): value is RecoveryCopy {
  if (!value || typeof value !== 'object') return false;
  const c = value as RecoveryCopy;
  return (
    c.version === 1 &&
    c.identityId === identityId &&
    c.documentId === documentId &&
    typeof c.key === 'string' &&
    typeof c.draftId === 'string' &&
    typeof c.draftName === 'string' &&
    Number.isSafeInteger(c.baseSeq) &&
    c.baseSeq >= 0 &&
    Number.isSafeInteger(c.draftRevision) &&
    c.draftRevision >= 0 &&
    Number.isFinite(c.updatedAt) &&
    c.updatedAt <= now + 60_000 &&
    now - c.updatedAt < MAX_AGE &&
    !!c.base?.nodes &&
    !!c.state?.nodes &&
    Array.isArray(c.base.comments) &&
    Array.isArray(c.state.comments) &&
    !!c.base.suggestions &&
    !!c.state.suggestions &&
    Array.isArray(c.base.removed_choices) &&
    Array.isArray(c.state.removed_choices)
  );
}
export async function recoveryCopies(
  identityId: string,
  documentId: string
): Promise<RecoveryCopy[]> {
  const copies = await transact<RecoveryCopy[]>('readonly', (s) => s.getAll());
  for (const copy of copies)
    if (!copy.updatedAt || Date.now() - copy.updatedAt >= MAX_AGE) await removeRecovery(copy.key);
  return copies
    .filter((c) => validRecovery(c, identityId, documentId))
    .sort((a, b) => b.updatedAt - a.updatedAt);
}
export async function saveRecovery(copy: RecoveryCopy) {
  if (!validRecovery(copy, copy.identityId, copy.documentId))
    throw new Error('This recovery copy is invalid.');
  if (new TextEncoder().encode(JSON.stringify(copy)).length > MAX_BYTES)
    throw new Error(
      'This document is too large for browser recovery. Keep a server draft or download a copy.'
    );
  await transact('readwrite', (s) => {
    // Read again inside the transaction: another tab may have signed out while opening storage.
    if (!recoveryPreference()) throw new Error('Browser recovery is disabled.');
    return s.put(copy);
  });
  const copies = await transact<RecoveryCopy[]>('readonly', (s) => s.getAll());
  const expired = copies.sort((a, b) => b.updatedAt - a.updatedAt).slice(MAX_COPIES);
  for (const old of expired) await removeRecovery(old.key);
}
export async function removeRecovery(key: string) {
  await transact('readwrite', (s) => s.delete(key));
}
export async function clearRecovery() {
  if (typeof indexedDB !== 'undefined') await transact('readwrite', (s) => s.clear());
}
export function recoveryPreference(): boolean {
  try {
    return localStorage.getItem('dynodoc.browser-recovery') === 'enabled';
  } catch {
    return false;
  }
}
export function setRecoveryPreference(enabled: boolean) {
  localStorage.setItem('dynodoc.browser-recovery', enabled ? 'enabled' : 'disabled');
}
