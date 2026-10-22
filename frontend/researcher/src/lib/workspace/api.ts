import { API_BASE_URL, EDITOR_HEADERS } from '$lib/api/config';
import type { EventPayload, EventRow, DocumentDetail, DocumentState } from '$lib/api/types';
export async function workspaceRequest<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(API_BASE_URL + path, {
    method: body === undefined ? 'GET' : 'POST',
    headers:
      body === undefined
        ? EDITOR_HEADERS
        : { ...EDITOR_HEADERS, 'content-type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body)
  });
  const data = await response.json().catch(() => ({
    error:
      response.status === 413
        ? 'This change is too large. Split the file or share fewer changes at a time.'
        : `The server could not complete the request (${response.status}). Please try again.`
  }));
  if (!response.ok) {
    const error = new Error(data.error || `Request failed (${response.status})`);
    // `code` is the server's stable name for the refusal (for example `request_changed`);
    // `details` carries what a screen needs to act on it.
    Object.assign(error, { status: response.status, code: data.code, details: data.details });
    throw error;
  }
  return data as T;
}
export function saveBatch(id: string, ops: EventPayload[], base_seq: number) {
  return workspaceRequest<{ events: EventRow[] }>(`/documents/${id}/batch`, { ops, base_seq });
}
export interface Member {
  id: string;
  email: string;
  name: string | null;
  role: string;
}
export interface Version {
  id: string;
  name: string;
  note: string;
  snapshot_id: string;
  through_seq: number;
  created_at: string;
  author: string | null;
}
export interface Draft {
  id: string;
  name: string;
  base_seq: number;
  created_at: string;
  merged_at: string | null;
  /** Set when an AI agent wrote this draft for the person. */
  agent?: { id: string; client_kind: string; display_name: string | null } | null;
}
export type Kind = 'document' | 'spreadsheet' | 'questionnaire' | 'presentation';
export function documentKind(document: DocumentDetail | { settings: unknown }): Kind {
  const settings = document.settings as { kind?: Kind } | null;
  return settings?.kind || 'questionnaire';
}
/**
 * Order siblings by position key. Keys are compared by code unit, the order `between()`
 * generates them in; locale-aware comparison would reorder keys such as "W" and "j".
 */
export function byPosition(a: { pos: string }, b: { pos: string }) {
  return a.pos < b.pos ? -1 : a.pos > b.pos ? 1 : 0;
}
export function documentNodes(state: DocumentState) {
  return Object.values(state.nodes)
    .filter((n) => !n.deleted && n.type === 'item')
    .sort(byPosition);
}
