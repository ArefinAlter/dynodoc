/**
 * Client for project issues, labels and stars (docs/PROJECT-ISSUES-API.md). Issues are an
 * invite-only list of tracked questions and tasks about a project; nothing here touches an
 * event, a hash or a change request. Text is shown exactly as typed and rendered as plain
 * text, never as HTML.
 *
 * The generated OpenAPI types describe these bodies only as `unknown`, so the shapes below
 * mirror the contract. Parsers are tolerant: a malformed row is skipped, never guessed at.
 */
import { API_BASE_URL, EDITOR_HEADERS } from '$lib/api/config';
export { isAbort } from './project-history';

/** Issues per request (the server allows 1 to 100, default 30). */
export const ISSUE_PAGE = 30;
/** Most issues the list will hold; a project may have up to 5,000. */
export const ISSUE_CAP = 300;
/** Comments per request (the server allows 1 to 100, default 50). */
export const COMMENT_PAGE = 50;
/** Most comments the page will hold for one issue; the server allows 1,000. */
export const COMMENT_CAP = 200;
export const TITLE_MAX = 200;
export const BODY_MAX = 20_000;
export const COMMENT_MAX = 10_000;
export const LABEL_NAME_MAX = 50;
export const LABEL_DESCRIPTION_MAX = 200;
export const LABELS_PER_ISSUE = 20;
export const SEARCH_MAX = 100;

export type IssueState = 'open' | 'closed';
export interface Person {
  id: string;
  name: string;
}
export interface Assignee extends Person {
  /** False when the assignee has since left the project (the assignment is kept). */
  member: boolean;
}
export interface Label {
  id: string;
  name: string;
  /** Six lowercase hexadecimal digits, no `#`. */
  color: string;
  description: string;
}
export interface LabelRow extends Label {
  openIssueCount: number;
}
export interface IssueSummary {
  id: string;
  number: number;
  title: string;
  state: IssueState;
  author: Person;
  assignee: Assignee | null;
  labels: Label[];
  commentCount: number;
  createdAt: string;
  updatedAt: string;
  closedAt: string | null;
}
export interface IssueViewer {
  role: string;
  canComment: boolean;
  /** Title, description, labels, assignee and state. */
  canEdit: boolean;
}
export interface Issue extends IssueSummary {
  body: string;
  revision: number;
  closedBy: Person | null;
  viewer: IssueViewer;
}
export interface IssueComment {
  id: string;
  seq: number;
  author: Person;
  body: string;
  createdAt: string;
}
export interface IssueCounts {
  open: number;
  closed: number;
}
export interface IssuePage {
  items: IssueSummary[];
  /** Send as `before` for the next page; null on the last page. */
  nextBefore: number | null;
  counts: IssueCounts;
  skipped: number;
}
export interface IssueThread {
  issue: Issue;
  comments: IssueComment[];
  nextAfter: number | null;
}
export interface LabelList {
  items: LabelRow[];
  canManage: boolean;
}
export interface StarState {
  starred: boolean;
  starCount: number;
}
/** Where the parent wants the Issues tab to start; `at` makes repeated requests distinct. */
export interface IssuesRequest {
  view: 'list' | 'new' | 'issue';
  number?: number;
  at: number;
}
/** The fields the project payloads gain with schema 39; absent on an older server. */
export interface ProjectEngagement {
  starred?: boolean;
  star_count?: number;
  open_issue_count?: number;
}

const record = (value: unknown): Record<string, unknown> | null =>
  value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
const text = (value: unknown) => (typeof value === 'string' ? value : null);
const whole = (value: unknown) =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : null;
const positive = (value: unknown) => {
  const n = whole(value);
  return n !== null && n > 0 ? n : null;
};
const UNKNOWN: Person = { id: '', name: 'Project member' };

/** Name to show for a person; the server sends `Deleted account` after erasure. */
export const personName = (person: { name?: string | null } | null | undefined) =>
  person?.name?.trim() || 'Project member';

function parsePerson(raw: unknown): Person | null {
  const row = record(raw),
    id = text(row?.id);
  if (!row || !id) return null;
  return { id, name: text(row.name)?.trim() || UNKNOWN.name };
}
function parseAssignee(raw: unknown): Assignee | null {
  const person = parsePerson(raw);
  if (!person) return null;
  return { ...person, member: record(raw)?.member !== false };
}

/** Six lowercase hex digits for a stored or typed colour, or null when it is not one. */
export function normalizeColor(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  const hex = value.trim().replace(/^#/, '').toLowerCase();
  return /^[0-9a-f]{6}$/.test(hex) ? hex : null;
}
export const FALLBACK_COLOR = '6e7781';

function parseLabel(raw: unknown): Label | null {
  const row = record(raw),
    id = text(row?.id),
    name = text(row?.name);
  if (!row || !id || !name) return null;
  return {
    id,
    name,
    color: normalizeColor(row.color) ?? FALLBACK_COLOR,
    description: text(row.description) ?? ''
  };
}
const parseLabelArray = (raw: unknown): Label[] =>
  Array.isArray(raw) ? raw.map(parseLabel).filter((label): label is Label => !!label) : [];

export function parseSummary(raw: unknown): IssueSummary | null {
  const row = record(raw),
    id = text(row?.id),
    number = positive(row?.number),
    title = text(row?.title),
    state = row?.state;
  if (!row || !id || !number || title === null || (state !== 'open' && state !== 'closed'))
    return null;
  const created = text(row.created_at) ?? '';
  return {
    id,
    number,
    title,
    state,
    author: parsePerson(row.author) ?? UNKNOWN,
    assignee: parseAssignee(row.assignee),
    labels: parseLabelArray(row.labels),
    commentCount: whole(row.comment_count) ?? 0,
    createdAt: created,
    updatedAt: text(row.updated_at) ?? created,
    closedAt: text(row.closed_at)
  };
}
export function parseIssue(raw: unknown): Issue | null {
  const summary = parseSummary(raw),
    row = record(raw);
  if (!summary || !row) return null;
  const viewer = record(row.viewer);
  return {
    ...summary,
    body: text(row.body) ?? '',
    revision: whole(row.revision) ?? 0,
    closedBy: parsePerson(row.closed_by),
    viewer: {
      role: text(viewer?.role) ?? '',
      canComment: viewer?.can_comment === true,
      canEdit: viewer?.can_edit === true
    }
  };
}
export function parseComment(raw: unknown): IssueComment | null {
  const row = record(raw),
    id = text(row?.id),
    seq = positive(row?.seq),
    body = text(row?.body);
  if (!row || !id || !seq || body === null) return null;
  return {
    id,
    seq,
    author: parsePerson(row.author) ?? UNKNOWN,
    body,
    createdAt: text(row.created_at) ?? ''
  };
}
/** GET /issues: newest number first, with the page marker and the project totals. */
export function parseIssuePage(raw: unknown): IssuePage {
  const body = record(raw),
    list = Array.isArray(body?.items) ? body.items : [],
    items: IssueSummary[] = [];
  let skipped = 0;
  for (const entry of list) {
    const parsed = parseSummary(entry);
    if (parsed) items.push(parsed);
    else skipped++;
  }
  const counts = record(body?.counts);
  return {
    items,
    nextBefore: positive(body?.next_before),
    counts: { open: whole(counts?.open) ?? 0, closed: whole(counts?.closed) ?? 0 },
    skipped
  };
}
/** GET /issues/{number}: the issue plus a page of comments, oldest first. */
export function parseThread(raw: unknown): IssueThread | null {
  const issue = parseIssue(raw),
    comments = record(record(raw)?.comments);
  if (!issue) return null;
  const items = Array.isArray(comments?.items)
    ? comments.items.map(parseComment).filter((c): c is IssueComment => !!c)
    : [];
  return { issue, comments: items, nextAfter: positive(comments?.next_after) };
}
export function parseLabelList(raw: unknown): LabelList {
  const body = record(raw),
    items: LabelRow[] = [];
  if (Array.isArray(body?.items))
    for (const entry of body.items) {
      const label = parseLabel(entry);
      if (label)
        items.push({ ...label, openIssueCount: whole(record(entry)?.open_issue_count) ?? 0 });
    }
  return { items, canManage: body?.can_manage === true };
}
export function parseStar(raw: unknown): StarState | null {
  const body = record(raw),
    count = whole(body?.star_count);
  if (!body || typeof body.starred !== 'boolean' || count === null) return null;
  return { starred: body.starred, starCount: count };
}

// --- colour ---------------------------------------------------------------

/** The label colours offered in the manager; any other 6-digit colour can be typed. */
export const LABEL_PALETTE: readonly { color: string; name: string }[] = [
  { color: 'b60205', name: 'Dark red' },
  { color: 'd93f0b', name: 'Orange' },
  { color: 'fbca04', name: 'Yellow' },
  { color: '0e8a16', name: 'Green' },
  { color: '006b75', name: 'Teal' },
  { color: '1d76db', name: 'Blue' },
  { color: '0052cc', name: 'Dark blue' },
  { color: '5319e7', name: 'Purple' },
  { color: 'e99695', name: 'Pink' },
  { color: 'fef2c0', name: 'Cream' },
  { color: 'c5def5', name: 'Light blue' },
  { color: 'd4c5f9', name: 'Lavender' }
];
export const DEFAULT_LABEL_COLOR = '1d76db';

/** WCAG relative luminance of a 6-digit hex colour. */
export function luminance(hex: string): number {
  const value = normalizeColor(hex) ?? FALLBACK_COLOR;
  const channel = (offset: number) => {
    const c = parseInt(value.slice(offset, offset + 2), 16) / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(0) + 0.7152 * channel(2) + 0.0722 * channel(4);
}
export function contrastRatio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}
/** Black or white, whichever reads better on this background (always at least 4.5:1). */
export function labelTextColor(hex: string): '#000000' | '#ffffff' {
  return contrastRatio(hex, '000000') >= contrastRatio(hex, 'ffffff') ? '#000000' : '#ffffff';
}

// --- roles and queries -----------------------------------------------------

/** Rank of a project role: owner 5, manager 4, editor 3, reviewer and contributor 2, viewer 1. */
export const roleRank = (role: string) =>
  ({ owner: 5, manager: 4, editor: 3, approver: 2, reviewer: 2, viewer: 1 })[role] ?? 0;
/** Everyone except a viewer may open an issue or comment. */
export const canOpenIssues = (role: string) => roleRank(role) >= 2;
export const canManageLabels = (role: string) => roleRank(role) >= 4;

export interface IssueFilters {
  state: IssueState | 'all';
  q: string;
  /** A label id. */
  label: string;
  /** A person id, `me` or `none`. */
  assignee: string;
  /** A person id or `me`. */
  author: string;
}
export const DEFAULT_FILTERS: IssueFilters = {
  state: 'open',
  q: '',
  label: '',
  assignee: '',
  author: ''
};
/** True when something other than the Open/Closed switch narrows the list. */
export const hasFilters = (f: IssueFilters) => !!(f.q.trim() || f.label || f.assignee || f.author);

/** Query string for GET /issues; empty values are left out. */
export function issueQuery(
  filters: IssueFilters,
  options: { before?: number | null; limit?: number } = {}
): string {
  const query = new URLSearchParams({ state: filters.state });
  const q = filters.q.trim().slice(0, SEARCH_MAX);
  if (q) query.set('q', q);
  if (filters.label) query.set('label', filters.label);
  if (filters.assignee) query.set('assignee', filters.assignee);
  if (filters.author) query.set('author', filters.author);
  if (options.before && Number.isSafeInteger(options.before) && options.before > 0)
    query.set('before', String(options.before));
  const limit = Math.min(Math.max(Math.floor(options.limit ?? ISSUE_PAGE), 1), 100);
  query.set('limit', String(limit));
  return query.toString();
}
/** Append a page, drop repeats by number and keep at most `cap` rows. */
export function appendIssues(
  current: IssueSummary[],
  next: IssueSummary[],
  cap = ISSUE_CAP
): IssueSummary[] {
  const known = new Set(current.map((issue) => issue.number));
  return [...current, ...next.filter((issue) => !known.has(issue.number))].slice(0, cap);
}

// --- errors ----------------------------------------------------------------

export class IssueError extends Error {
  constructor(
    readonly status: number,
    readonly reason: string
  ) {
    super(reason || `Request failed (${status})`);
  }
}
/** The server does not have these routes (an older build): show a calm note, not an error. */
export const isUnavailable = (error: unknown) =>
  error instanceof IssueError && [404, 405, 501].includes(error.status);

export type Subject =
  | 'list'
  | 'issue'
  | 'create'
  | 'edit'
  | 'comment'
  | 'labels'
  | 'label'
  | 'star';
const READS: Subject[] = ['list', 'issue', 'labels'];
/** A short, specific sentence for a failed request; never a raw status or server text. */
export function issueMessage(error: unknown, subject: Subject): string {
  if (!(error instanceof IssueError))
    return error instanceof TypeError
      ? 'Could not reach the server. Check your connection and try again.'
      : 'That could not be completed. Try again in a moment.';
  const { status, reason } = error;
  if (status === 401) return 'Your session has ended. Sign in again to continue.';
  if (status === 403) {
    if (READS.includes(subject) || subject === 'star')
      return 'You do not have access to this project.';
    if (subject === 'edit') return "Only the issue's author, a manager or an owner can change it.";
    if (subject === 'label') return 'Only managers and owners can change labels.';
    return 'Viewers can read issues but cannot open them or comment.';
  }
  if (status === 404 || status === 405 || status === 501) {
    if (subject === 'label') return 'That label no longer exists. Reload the labels and try again.';
    if (subject === 'star') return 'Starring is not available on this server yet.';
    if (subject === 'list' || subject === 'labels' || subject === 'create')
      return 'Issues are not available on this server yet.';
    return 'This issue does not exist.';
  }
  if (status === 409) {
    if (subject === 'edit')
      return 'This issue changed since you opened it. Reload it to see the latest version, then apply your change again.';
    if (subject === 'create') return 'This project has reached its limit of 5,000 issues.';
    if (subject === 'comment') return 'This issue has reached its limit of 1,000 comments.';
    if (subject === 'label')
      return /\b100\b|limit/i.test(reason)
        ? 'This project already has 100 labels.'
        : 'A label with that name already exists.';
    return 'This changed while you were working. Reload and try again.';
  }
  if (status === 400) {
    if (/label/i.test(reason) && /exist|choose/i.test(reason) && subject !== 'label')
      return 'A selected label no longer exists. Reload the labels and try again.';
    if (/at most 20 labels/i.test(reason)) return 'An issue can have at most 20 labels.';
    if (/assign|member/i.test(reason))
      return 'The assignee is not a member of this project. Choose someone else.';
    if (subject === 'create' || subject === 'edit')
      return 'Check the title (1 to 200 characters, one line) and the description (up to 20,000 characters).';
    if (subject === 'comment') return 'A comment must be 1 to 10,000 characters.';
    if (subject === 'label')
      return 'Check the label name (1 to 50 characters) and colour (6 hexadecimal digits).';
    if (subject === 'list') return 'Those filters were not accepted. Clear them and try again.';
    return 'That request was not accepted.';
  }
  if (status === 413) return 'That text is too large to send.';
  if (status === 422)
    return 'The server did not understand that request. Reload the page and try again.';
  if (status === 429) return 'Too many requests. Wait a moment, then try again.';
  if ([503, 504, 507].includes(status))
    return 'The server is busy or timed out. Try again in a moment.';
  if (status === 0) return 'The server sent a response this page does not understand.';
  return `That could not be completed (${status}).`;
}

// --- requests --------------------------------------------------------------

type Method = 'GET' | 'POST' | 'PATCH' | 'PUT' | 'DELETE';
async function call(
  method: Method,
  path: string,
  body?: unknown,
  signal?: AbortSignal
): Promise<unknown> {
  const headers: Record<string, string> = { ...EDITOR_HEADERS };
  if (body !== undefined) headers['content-type'] = 'application/json';
  const response = await fetch(`${API_BASE_URL}${path}`, {
    method,
    headers,
    body: body === undefined ? undefined : JSON.stringify(body),
    signal
  });
  const data: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const failure = record(data),
      details = record(failure?.details);
    throw new IssueError(response.status, text(details?.reason) || text(failure?.error) || '');
  }
  return data;
}
const base = (project: string) => `/projects/${encodeURIComponent(project)}`;
const unexpected = () => new IssueError(0, 'Unexpected response');

export async function listIssues(
  project: string,
  filters: IssueFilters,
  options: { before?: number | null; limit?: number; signal?: AbortSignal } = {}
): Promise<IssuePage> {
  const raw = await call(
    'GET',
    `${base(project)}/issues?${issueQuery(filters, options)}`,
    undefined,
    options.signal
  );
  if (!record(raw)) throw unexpected();
  return parseIssuePage(raw);
}
export interface NewIssue {
  title: string;
  body: string;
  labels: string[];
  assigneeId: string | null;
}
export async function createIssue(project: string, input: NewIssue): Promise<Issue> {
  const body: Record<string, unknown> = { title: input.title.trim() };
  if (input.body.trim()) body.body = input.body;
  if (input.labels.length) body.labels = input.labels;
  if (input.assigneeId) body.assignee_id = input.assigneeId;
  const issue = parseIssue(await call('POST', `${base(project)}/issues`, body));
  if (!issue) throw unexpected();
  return issue;
}
export async function getIssue(
  project: string,
  number: number,
  options: { after?: number | null; limit?: number; signal?: AbortSignal } = {}
): Promise<IssueThread> {
  const query = new URLSearchParams({ limit: String(options.limit ?? COMMENT_PAGE) });
  if (options.after) query.set('after', String(options.after));
  const thread = parseThread(
    await call('GET', `${base(project)}/issues/${number}?${query}`, undefined, options.signal)
  );
  if (!thread) throw unexpected();
  return thread;
}
export interface IssueChange {
  title?: string;
  body?: string;
  state?: IssueState;
  /** The complete new set; an empty array clears. */
  labels?: string[];
  /** A person id, or null to unassign. */
  assigneeId?: string | null;
  /** Required with a title or description change. */
  revision?: number;
}
export async function updateIssue(
  project: string,
  number: number,
  change: IssueChange
): Promise<Issue> {
  const body: Record<string, unknown> = {};
  if (change.title !== undefined) body.title = change.title.trim();
  if (change.body !== undefined) body.body = change.body;
  if (change.state !== undefined) body.state = change.state;
  if (change.labels !== undefined) body.labels = change.labels;
  if (change.assigneeId !== undefined) body.assignee_id = change.assigneeId;
  if (change.revision !== undefined) body.revision = change.revision;
  const issue = parseIssue(await call('PATCH', `${base(project)}/issues/${number}`, body));
  if (!issue) throw unexpected();
  return issue;
}
export async function addComment(
  project: string,
  number: number,
  body: string
): Promise<{ comment: IssueComment; commentCount: number }> {
  const raw = record(
      await call('POST', `${base(project)}/issues/${number}/comments`, { body: body.trim() })
    ),
    comment = parseComment(raw?.comment),
    commentCount = whole(raw?.comment_count);
  if (!comment || commentCount === null) throw unexpected();
  return { comment, commentCount };
}
export async function listLabels(project: string, signal?: AbortSignal): Promise<LabelList> {
  const raw = await call('GET', `${base(project)}/labels`, undefined, signal);
  if (!record(raw)) throw unexpected();
  return parseLabelList(raw);
}
export interface LabelInput {
  name: string;
  color: string;
  description: string;
}
export async function createLabel(project: string, input: LabelInput): Promise<Label> {
  const label = parseLabel(
    await call('POST', `${base(project)}/labels`, {
      name: input.name.trim(),
      color: input.color,
      description: input.description.trim()
    })
  );
  if (!label) throw unexpected();
  return label;
}
export async function updateLabel(
  project: string,
  id: string,
  input: Partial<LabelInput>
): Promise<Label> {
  const body: Record<string, unknown> = {};
  if (input.name !== undefined) body.name = input.name.trim();
  if (input.color !== undefined) body.color = input.color;
  if (input.description !== undefined) body.description = input.description.trim();
  const label = parseLabel(
    await call('PATCH', `${base(project)}/labels/${encodeURIComponent(id)}`, body)
  );
  if (!label) throw unexpected();
  return label;
}
export async function deleteLabel(project: string, id: string): Promise<{ issuesUpdated: number }> {
  const raw = record(await call('DELETE', `${base(project)}/labels/${encodeURIComponent(id)}`));
  return { issuesUpdated: whole(raw?.issues_updated) ?? 0 };
}
export async function setStar(project: string, starred: boolean): Promise<StarState> {
  const state = parseStar(await call(starred ? 'PUT' : 'DELETE', `${base(project)}/star`));
  if (!state) throw unexpected();
  return state;
}
