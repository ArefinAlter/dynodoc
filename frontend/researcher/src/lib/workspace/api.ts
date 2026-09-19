import { API_BASE_URL } from '$lib/api/config';
import type { EventPayload, EventRow, DocumentDetail, DocumentState } from '$lib/api/types';
export async function workspaceRequest<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(API_BASE_URL + path, {
    method: body === undefined ? 'GET' : 'POST',
    headers: body === undefined ? {} : { 'content-type': 'application/json' },
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
    Object.assign(error, { status: response.status });
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
}
export type Kind = 'document' | 'spreadsheet' | 'questionnaire';
export function documentKind(document: DocumentDetail | { settings: unknown }): Kind {
  const settings = document.settings as { kind?: Kind } | null;
  return settings?.kind || 'questionnaire';
}
export function documentNodes(state: DocumentState) {
  return Object.values(state.nodes)
    .filter((n) => !n.deleted && n.type === 'item')
    .sort((a, b) => (a.pos < b.pos ? -1 : a.pos > b.pos ? 1 : 0));
}
