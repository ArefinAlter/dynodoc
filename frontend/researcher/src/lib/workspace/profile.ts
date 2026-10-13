import { workspaceRequest } from './api';
export interface Profile {
  id: string;
  name: string;
  bio: string;
  revision: number;
}
export const loadProfile = (id?: string) =>
  workspaceRequest<Profile>(id ? `/profiles/${id}` : '/profile');
export const initials = (name: string) =>
  name
    .trim()
    .split(/\s+/)
    .slice(0, 2)
    .map((part) => part[0] || '')
    .join('')
    .toUpperCase() || 'D';

/** A project a profile starred that the viewer can also open (GET /profiles/{id}/starred). */
export interface StarredProject {
  id: string;
  name: string;
  starCount: number;
  openIssueCount: number;
}
export interface StarredList {
  items: StarredProject[];
  /** More than the server returns (100) exist. */
  truncated: boolean;
}
const count = (value: unknown) =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : 0;
/** Tolerant: a row without an id or a name is skipped, never guessed at. */
export function parseStarred(raw: unknown): StarredList {
  const body = raw && typeof raw === 'object' ? (raw as Record<string, unknown>) : {};
  const items: StarredProject[] = [];
  for (const entry of Array.isArray(body.items) ? body.items : []) {
    const row = entry && typeof entry === 'object' ? (entry as Record<string, unknown>) : null;
    if (typeof row?.id !== 'string' || typeof row.name !== 'string') continue;
    items.push({
      id: row.id,
      name: row.name,
      starCount: count(row.star_count),
      openIssueCount: count(row.open_issue_count)
    });
  }
  return { items, truncated: body.truncated === true };
}
export const loadStarred = async (id: string) =>
  parseStarred(await workspaceRequest<unknown>(`/profiles/${encodeURIComponent(id)}/starred`));
/** The server does not have the route (an older build): hide the section, do not complain. */
export const starredUnavailable = (error: unknown) =>
  [404, 405, 501].includes((error as { status?: number } | null)?.status ?? 0);
