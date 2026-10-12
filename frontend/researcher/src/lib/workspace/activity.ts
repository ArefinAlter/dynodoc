/**
 * Project and profile activity for the Projects area. The activity, contributors and
 * profile-activity endpoints are optional: when the server does not offer one, the
 * helpers rebuild a smaller answer from endpoints every deployment has.
 */
import { workspaceRequest } from './api';
import type { ProjectFile, ProjectRequest } from './projects';
import { displayName, formatBreakdown, type FormatShare } from './repo-ui';

export interface FileChange {
  seq: number;
  at: string;
  actor_name: string | null;
  summary: string | null;
}
/** A file row; `last_change` is present only when the server provides it. */
export type RepoFile = ProjectFile & { last_change?: FileChange | null };
export type RepoRequest = ProjectRequest & {
  submitted_at?: string | null;
  merged_at?: string | null;
  reviewed_at?: string | null;
  reviewer?: { id: string; name: string | null; email: string } | null;
  edits?: number;
};
export interface MemberLite {
  id: string;
  name: string | null;
  email: string;
  role: string;
}
export interface ActivityItem {
  at: string;
  kind: string;
  actor_name: string | null;
  document_id: string | null;
  document_title: string | null;
  summary: string | null;
  seq: number | null;
}
export interface ActivityPage {
  items: ActivityItem[];
  next: string | null;
  /** True when the list was rebuilt from per-file history rather than served whole. */
  derived: boolean;
}
export interface Contributor {
  identity_id: string;
  name: string;
  merges: number;
  edits: number;
  last_at: string | null;
}
export interface ActivityDay {
  date: string;
  count: number;
}
export interface ProfileActivity {
  year: number;
  total: number;
  days: ActivityDay[];
  derived: boolean;
}

const text = (value: unknown) => (typeof value === 'string' && value.trim() ? value : null);
const count = (value: unknown) =>
  typeof value === 'number' && Number.isFinite(value) && value > 0 ? Math.floor(value) : 0;

/** Run `work` over `items` with at most `size` requests in flight. Failures become `null`. */
export async function mapPool<T, R>(
  items: T[],
  size: number,
  work: (item: T) => Promise<R>
): Promise<(R | null)[]> {
  const results: (R | null)[] = new Array(items.length).fill(null);
  let next = 0;
  async function worker() {
    while (next < items.length) {
      const index = next++;
      try {
        results[index] = await work(items[index]);
      } catch {
        results[index] = null;
      }
    }
  }
  await Promise.all(Array.from({ length: Math.min(size, items.length) }, worker));
  return results;
}

const EVENT_TEXT: Record<string, string> = {
  NodeCreated: 'Added content',
  NodeMoved: 'Moved content',
  NodeDeleted: 'Removed content',
  NodeRestored: 'Restored content',
  FieldEdited: 'Edited content',
  RichTextPatched: 'Edited text',
  ChoiceAdded: 'Changed options',
  ChoiceRemoved: 'Changed options',
  CommentAdded: 'Added a comment',
  CommentReplied: 'Replied to a comment',
  CommentResolved: 'Resolved a comment',
  SuggestionProposed: 'Proposed a change',
  SuggestionAccepted: 'Accepted a suggestion',
  SuggestionRejected: 'Declined a suggestion'
};
/** Plain-language label for a recorded event or activity kind. */
export function kindLabel(kind: string): string {
  if (EVENT_TEXT[kind]) return EVENT_TEXT[kind];
  const lower = kind.toLowerCase();
  if (lower.includes('merge')) return 'Merged a pull request';
  if (lower.includes('declin') || lower.includes('withdraw')) return 'Closed a pull request';
  if (lower.includes('request') || lower.includes('submit')) return 'Opened a pull request';
  const words = kind
    .replace(/[_.-]+/g, ' ')
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .trim()
    .toLowerCase();
  return words ? words[0].toUpperCase() + words.slice(1) : 'Changed the file';
}
export type ActivityTone = 'merge' | 'closed' | 'request' | 'change';
export function activityTone(kind: string): ActivityTone {
  const lower = kind.toLowerCase();
  if (lower.includes('merge')) return 'merge';
  if (lower.includes('declin') || lower.includes('withdraw')) return 'closed';
  if (lower.includes('request') || lower.includes('submit')) return 'request';
  return 'change';
}

function normalise(raw: unknown): ActivityItem | null {
  const row = raw as Record<string, unknown> | null;
  const at = text(row?.at) || text(row?.created_at);
  if (!row || !at || !Number.isFinite(Date.parse(at))) return null;
  return {
    at,
    kind: text(row.kind) || '',
    actor_name: text(row.actor_name),
    document_id: text(row.document_id),
    document_title: text(row.document_title),
    summary: text(row.summary),
    seq: typeof row.seq === 'number' ? row.seq : null
  };
}

interface EventLite {
  seq: number;
  type: string;
  actor_id: string;
  created_at: string;
}
/** Names by identity id, from the project's member list. */
export function nameIndex(members: MemberLite[]) {
  return new Map(members.map((m) => [m.id, displayName(m.name, m.email)]));
}

/** Recent history of the most recently changed files, newest first. */
async function deriveActivity(
  files: RepoFile[],
  names: Map<string, string>,
  limit: number
): Promise<ActivityItem[]> {
  const recent = files.filter((f) => f.latest_seq > 0).slice(0, 8);
  const pages = await mapPool(recent, 4, (f) =>
    workspaceRequest<{ items: EventLite[] }>(
      `/documents/${f.id}/events?from_seq=${Math.max(1, f.latest_seq - 9)}&to_seq=${f.latest_seq}&limit=10`
    )
  );
  const items: ActivityItem[] = [];
  pages.forEach((page, index) => {
    for (const event of page?.items || [])
      items.push({
        at: event.created_at,
        kind: event.type,
        actor_name: names.get(event.actor_id) || null,
        document_id: recent[index].id,
        document_title: recent[index].title,
        summary: null,
        seq: event.seq
      });
  });
  return items.sort((a, b) => Date.parse(b.at) - Date.parse(a.at)).slice(0, limit);
}

export async function loadActivity(
  projectId: string,
  files: RepoFile[],
  names: Map<string, string>,
  before: string | null = null,
  limit = 50
): Promise<ActivityPage> {
  try {
    const query = `limit=${limit}${before ? `&before=${encodeURIComponent(before)}` : ''}`;
    const result = await workspaceRequest<{ items?: unknown[]; next?: unknown }>(
      `/projects/${projectId}/activity?${query}`
    );
    if (!Array.isArray(result?.items)) throw new Error('Activity is unavailable');
    return {
      items: result.items.map(normalise).filter((i): i is ActivityItem => !!i),
      next: text(result.next),
      derived: false
    };
  } catch {
    if (before) return { items: [], next: null, derived: true };
    return {
      items: await deriveActivity(files, names, limit).catch(() => []),
      next: null,
      derived: true
    };
  }
}

export interface ContributorList {
  contributors: Contributor[];
  /** True when only the member list is known, so counts are not shown. */
  derived: boolean;
}
export async function loadContributors(
  projectId: string,
  members: MemberLite[]
): Promise<ContributorList> {
  try {
    const result = await workspaceRequest<{ contributors?: Record<string, unknown>[] }>(
      `/projects/${projectId}/contributors`
    );
    if (!Array.isArray(result?.contributors)) throw new Error('Contributors are unavailable');
    const contributors = result.contributors
      .filter((c) => text(c.identity_id))
      .map((c) => ({
        identity_id: String(c.identity_id),
        name: text(c.name) || 'Project member',
        merges: count(c.merges),
        edits: count(c.edits),
        last_at: text(c.last_at)
      }));
    return { contributors, derived: false };
  } catch {
    const seen = new Set<string>();
    return {
      contributors: members
        .filter((m) => !seen.has(m.id) && !!seen.add(m.id))
        .map((m) => ({
          identity_id: m.id,
          name: displayName(m.name, m.email),
          merges: 0,
          edits: 0,
          last_at: null
        })),
      derived: true
    };
  }
}

/** Days with activity, counted from activity items whose actor matches `name`. */
export function daysFromActivity(items: ActivityItem[], year: number, name: string): ActivityDay[] {
  const counts = new Map<string, number>();
  for (const item of items) {
    if (!name || item.actor_name !== name) continue;
    const at = new Date(item.at);
    if (at.getFullYear() !== year) continue;
    const key = localDate(at);
    counts.set(key, (counts.get(key) || 0) + 1);
  }
  return [...counts].map(([date, n]) => ({ date, count: n }));
}
const two = (n: number) => String(n).padStart(2, '0');
const localDate = (d: Date) => `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`;

export async function loadProfileActivity(
  profileId: string,
  year: number,
  projectIds: string[],
  name: string
): Promise<ProfileActivity> {
  try {
    const result = await workspaceRequest<{ year?: number; total?: number; days?: unknown[] }>(
      `/profiles/${profileId}/activity?year=${year}`
    );
    if (!Array.isArray(result?.days)) throw new Error('Profile activity is unavailable');
    const days = result.days
      .map((d) => d as Record<string, unknown>)
      .filter((d) => typeof d.date === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(d.date))
      .map((d) => ({ date: String(d.date), count: count(d.count) }));
    return {
      year: typeof result.year === 'number' ? result.year : year,
      total:
        typeof result.total === 'number' ? result.total : days.reduce((n, d) => n + d.count, 0),
      days,
      derived: false
    };
  } catch {
    const pages = await mapPool(projectIds.slice(0, 10), 3, (id) =>
      loadActivity(id, [], new Map(), null, 50)
    );
    const items = pages.flatMap((p) => p?.items || []);
    const days = daysFromActivity(items, year, name);
    return { year, total: days.reduce((n, d) => n + d.count, 0), days, derived: true };
  }
}

export interface HeatCell {
  date: string;
  count: number;
  /** 0 (none) to 4 (busiest day of the year). */
  level: number;
  /** Outside the year or after today: drawn empty and not announced. */
  outside: boolean;
}
export interface HeatGrid {
  weeks: HeatCell[][];
  months: { label: string; week: number }[];
  total: number;
}
/** Sunday-first weeks covering a calendar year, as in GitHub's contribution graph. */
export function buildHeatGrid(year: number, days: ActivityDay[], today = new Date()): HeatGrid {
  const counts = new Map(days.map((d) => [d.date, d.count]));
  const cursor = new Date(year, 0, 1);
  cursor.setDate(cursor.getDate() - cursor.getDay());
  const last = new Date(year, 11, 31);
  const weeks: HeatCell[][] = [];
  let max = 0;
  let total = 0;
  while (cursor <= last) {
    const week: HeatCell[] = [];
    for (let i = 0; i < 7; i++) {
      const outside = cursor.getFullYear() !== year || cursor > today;
      const n = outside ? 0 : counts.get(localDate(cursor)) || 0;
      max = Math.max(max, n);
      total += n;
      week.push({ date: localDate(cursor), count: n, level: 0, outside });
      cursor.setDate(cursor.getDate() + 1);
    }
    weeks.push(week);
  }
  for (const week of weeks)
    for (const cell of week)
      if (cell.count) cell.level = Math.min(4, Math.max(1, Math.ceil((cell.count / max) * 4)));
  const months: { label: string; week: number }[] = [];
  let previous = -1;
  let lastWeek = -3;
  weeks.forEach((week, index) => {
    const first = week.find((c) => !c.outside);
    if (!first) return;
    const month = Number(first.date.slice(5, 7)) - 1;
    if (month === previous || index - lastWeek < 3) return;
    previous = month;
    lastWeek = index;
    months.push({
      label: new Date(year, month, 1).toLocaleString(undefined, { month: 'short' }),
      week: index
    });
  });
  return { weeks, months, total };
}

export interface ProjectSummary {
  files: number;
  filesMore: boolean;
  formats: FormatShare[];
  updated_at: string | null;
  open_requests: number | null;
}
/** What a project row shows beyond its name: formats, last update and open pull requests. */
export async function summarizeProject(projectId: string): Promise<ProjectSummary> {
  const [files, requests] = await Promise.all([
    workspaceRequest<{ items: ProjectFile[]; more: boolean }>(`/projects/${projectId}/files`),
    workspaceRequest<{ items: ProjectRequest[]; more: boolean }>(
      `/projects/${projectId}/change-requests`
    ).catch(() => null)
  ]);
  const newest = files.items.reduce<string | null>(
    (best, f) => (!best || Date.parse(f.updated_at) > Date.parse(best) ? f.updated_at : best),
    null
  );
  return {
    files: files.items.length,
    filesMore: files.more,
    formats: formatBreakdown(files.items),
    updated_at: newest,
    open_requests: requests ? requests.items.filter((r) => r.status === 'open').length : null
  };
}
