/**
 * Read-only client for shared project history: published branch heads, the signed
 * receipt the server issued for each head, and the review decisions recorded for a
 * commit. The browser only reads and displays this data. It never signs, publishes,
 * advances a branch or checks a receipt signature (the CLI trust journal does that).
 *
 * The generated OpenAPI types describe these responses only as `unknown`, so the shapes
 * below mirror crates/engine-api/src/project_history.rs and docs/SHARED-PROJECT-HISTORY.md.
 * Parsers are tolerant: a malformed entry is skipped rather than guessed at.
 */
import { API_BASE_URL, EDITOR_HEADERS } from '$lib/api/config';

/** Branch heads per request (the server allows 1 to 256). */
export const BRANCH_PAGE = 50;
/** Most branches the page will hold; a project may have up to 4,096. */
export const BRANCH_CAP = 500;
/** Review rows per request (the server allows 1 to 256). */
export const REVIEW_PAGE = 64;
/** Most review rows the page will hold for one commit. */
export const REVIEW_CAP = 256;
/** Longest commit message text shown, and most file paths listed. */
export const MESSAGE_CAP = 600;
export const FILE_LIST_CAP = 20;
/** Literal `main` is the project's team branch: protected, with review rules. */
export const DEFAULT_BRANCH = 'main';

export interface HeadReceipt {
  version: number;
  authority: string;
  project: string;
  branch: string;
  commit: string;
  generation: number;
  /** Address of the preceding receipt; null for the first update of a branch. */
  previous: string | null;
  issuedAtMs: number;
  /** ISO form of the declared time; a claim by the issuer, not a trusted timestamp. */
  issuedAt: string;
  issuerKey: string;
}
export interface Branch {
  name: string;
  commit: string;
  generation: number;
  receipt: HeadReceipt | null;
  isDefault: boolean;
  isProtected: boolean;
}
export interface BranchPage {
  branches: Branch[];
  next: string | null;
  /** A full page came back, so another may exist. */
  more: boolean;
  skipped: number;
}
export interface CommitSummary {
  message: string;
  messageCut: boolean;
  authorKey: string;
  authoredAtMs: number;
  authoredAt: string;
  parents: number;
  recovery: boolean;
  fileCount: number;
  files: string[];
}
export type Decision = 'approve' | 'request_changes';
export interface Review {
  id: number;
  branch: string;
  baseGeneration: number;
  baseCommit: string | null;
  reviewer: string;
  decision: Decision;
  createdAt: string;
}
export interface ReviewPage {
  items: Review[];
  next: number | null;
  more: boolean;
  skipped: number;
}
export interface ReviewRow extends Review {
  /** The reviewer's newest decision on this base; earlier ones are history. */
  current: boolean;
}
export interface ReviewGroup {
  generation: number;
  commit: string | null;
  rows: ReviewRow[];
  approvals: number;
  changeRequests: number;
}

const record = (value: unknown): Record<string, unknown> | null =>
  value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
const text = (value: unknown) => (typeof value === 'string' ? value : null);
const whole = (value: unknown) =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : null;
const HEX64 = /^[0-9a-f]{64}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
export const isAddress = (value: unknown): value is string =>
  typeof value === 'string' && HEX64.test(value);
const isoFromMs = (ms: number) => {
  const date = new Date(ms);
  return Number.isFinite(date.getTime()) ? date.toISOString() : '';
};

function parseReceipt(raw: unknown): HeadReceipt | null {
  const signed = record(raw),
    receipt = record(signed?.receipt),
    scope = record(receipt?.scope),
    head = record(receipt?.head);
  if (!receipt || !scope || !head) return null;
  const authority = text(scope.authority),
    project = text(scope.project),
    branch = text(scope.branch),
    commit = head.commit,
    generation = whole(head.generation),
    issuedAtMs = whole(receipt.issued_at_ms),
    version = whole(receipt.version),
    issuerKey = receipt.issuer_key,
    previous = receipt.previous;
  if (
    !authority ||
    !UUID.test(authority) ||
    !project ||
    !UUID.test(project) ||
    !branch ||
    !isAddress(commit) ||
    !generation ||
    issuedAtMs === null ||
    version === null ||
    !isAddress(issuerKey) ||
    !(previous === null || isAddress(previous))
  )
    return null;
  return {
    version,
    authority,
    project,
    branch,
    commit,
    generation,
    previous,
    issuedAtMs,
    issuedAt: isoFromMs(issuedAtMs),
    issuerKey
  };
}

/** GET /history/refs: heads plus the receipts that were issued for exactly those heads. */
export function parseRefs(raw: unknown, limit = BRANCH_PAGE): BranchPage {
  const body = record(raw),
    items = Array.isArray(body?.items) ? body.items : [],
    receipts = new Map<string, HeadReceipt>();
  if (Array.isArray(body?.receipts))
    for (const entry of body.receipts) {
      const parsed = parseReceipt(entry);
      if (parsed) receipts.set(parsed.branch, parsed);
    }
  const branches: Branch[] = [];
  let skipped = 0;
  for (const item of items) {
    const row = record(item),
      head = record(row?.head),
      name = text(row?.branch),
      commit = head?.commit,
      generation = whole(head?.generation);
    if (!name || !isAddress(commit) || !generation) {
      skipped++;
      continue;
    }
    const receipt = receipts.get(name);
    branches.push({
      name,
      commit,
      generation,
      // Show a receipt only when it describes this exact head.
      receipt:
        receipt && receipt.commit === commit && receipt.generation === generation ? receipt : null,
      isDefault: name === DEFAULT_BRANCH,
      isProtected: name === DEFAULT_BRANCH
    });
  }
  const next = text(body?.next);
  return { branches, next, more: items.length >= limit && !!next, skipped };
}

/** The signed commit envelope: who signed (a key), when they declared it, what it holds. */
export function parseCommit(raw: unknown): CommitSummary | null {
  const signed = record(raw),
    commit = record(signed?.commit);
  if (!commit) return null;
  const authorKey = commit.author_key,
    authoredAtMs = whole(commit.authored_at_ms),
    message = text(commit.message),
    parents = Array.isArray(commit.parents) ? commit.parents.length : null,
    files = record(commit.files);
  if (
    !isAddress(authorKey) ||
    authoredAtMs === null ||
    message === null ||
    parents === null ||
    !files
  )
    return null;
  const paths = Object.keys(files).sort();
  return {
    message: message.length > MESSAGE_CAP ? message.slice(0, MESSAGE_CAP) : message,
    messageCut: message.length > MESSAGE_CAP,
    authorKey,
    authoredAtMs,
    authoredAt: isoFromMs(authoredAtMs),
    parents,
    recovery: !!commit.recovery,
    fileCount: paths.length,
    files: paths.slice(0, FILE_LIST_CAP)
  };
}

/** GET /history/reviews: append-only decisions for one candidate, oldest first. */
export function parseReviews(raw: unknown, limit = REVIEW_PAGE): ReviewPage {
  const body = record(raw),
    list = Array.isArray(body?.items) ? body.items : [],
    items: Review[] = [];
  let skipped = 0;
  for (const entry of list) {
    const row = record(entry),
      id = whole(row?.id),
      branch = text(row?.branch),
      generation = whole(row?.expected_generation),
      base = row?.expected_commit,
      reviewer = text(row?.reviewer),
      decision = row?.decision,
      createdAt = text(row?.created_at);
    if (
      id === null ||
      !branch ||
      generation === null ||
      !(base === null || isAddress(base)) ||
      !reviewer ||
      (decision !== 'approve' && decision !== 'request_changes') ||
      !createdAt
    ) {
      skipped++;
      continue;
    }
    items.push({
      id,
      branch,
      baseGeneration: generation,
      baseCommit: base,
      reviewer,
      decision,
      createdAt
    });
  }
  const next = whole(body?.next);
  return { items, next, more: list.length >= limit && next !== null, skipped };
}

/**
 * Reviews are bound to one exact candidate, branch and base head. Group the ones for
 * `branch` by base (newest base first); within a group the newest decision per reviewer
 * is the one that counts.
 */
export function groupReviews(items: Review[], branch: string): ReviewGroup[] {
  const groups = new Map<string, ReviewGroup>();
  for (const review of [...items].sort((a, b) => a.id - b.id)) {
    if (review.branch !== branch) continue;
    const key = `${review.baseGeneration}:${review.baseCommit ?? ''}`;
    let group = groups.get(key);
    if (!group) {
      group = {
        generation: review.baseGeneration,
        commit: review.baseCommit,
        rows: [],
        approvals: 0,
        changeRequests: 0
      };
      groups.set(key, group);
    }
    group.rows.push({ ...review, current: false });
  }
  for (const group of groups.values()) {
    const newest = new Map<string, ReviewRow>();
    for (const row of group.rows) newest.set(row.reviewer, row);
    for (const row of newest.values()) {
      row.current = true;
      if (row.decision === 'approve') group.approvals++;
      else group.changeRequests++;
    }
  }
  return [...groups.values()].sort((a, b) => b.generation - a.generation);
}

/** Approvals the project rules ask for before `main` moves; other branches need none. */
export const approvalsRequired = (branch: string, projectRule: number): number | null =>
  branch === DEFAULT_BRANCH ? Math.max(0, Math.floor(projectRule || 0)) : null;

export class HistoryError extends Error {
  constructor(
    readonly status: number,
    readonly reason: string
  ) {
    super(reason || `Request failed (${status})`);
  }
}
export const isAbort = (error: unknown) =>
  !!error && typeof error === 'object' && (error as { name?: unknown }).name === 'AbortError';

export type Subject = 'branches' | 'commit' | 'reviews';
/** A short, specific sentence for a failed read; never a raw status or server text. */
export function historyMessage(error: unknown, subject: Subject): string {
  if (!(error instanceof HistoryError))
    return error instanceof TypeError
      ? 'Could not reach the server. Check your connection and try again.'
      : 'Shared history could not be loaded.';
  const { status, reason } = error;
  if (status === 401) return 'Your session has ended. Sign in again to see shared history.';
  if (status === 403)
    return subject === 'branches'
      ? "You do not have access to this project's shared history. Reading it needs access to every file in each branch head, including files that were later moved or removed."
      : 'You cannot read this commit. It includes a file you can no longer access.';
  if (status === 404)
    return subject === 'branches'
      ? 'Shared history is not available for this project on this server.'
      : subject === 'commit'
        ? 'This commit is no longer published.'
        : 'No published commit matches these reviews.';
  if (status === 409)
    return /erasure|erased|unavailable/i.test(reason)
      ? 'A branch head was removed when a document was permanently erased, so it cannot be shown. History is never rewound to an older version.'
      : 'Shared history changed while loading. Reload to see the current state.';
  if ([429, 503, 504, 507].includes(status))
    return 'The server is busy or timed out. Try again in a moment.';
  if (status === 0) return 'The server sent a response this page does not understand.';
  return `Shared history could not be loaded (${status}).`;
}

async function get(path: string, signal?: AbortSignal): Promise<unknown> {
  const response = await fetch(`${API_BASE_URL}${path}`, { headers: EDITOR_HEADERS, signal });
  const body: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const failure = record(body),
      details = record(failure?.details);
    throw new HistoryError(response.status, text(details?.reason) || text(failure?.error) || '');
  }
  return body;
}
const base = (project: string) => `/projects/${encodeURIComponent(project)}/history`;
const unexpected = () => new HistoryError(0, 'Unexpected response');

export async function loadBranches(
  project: string,
  options: { after?: string | null; limit?: number; signal?: AbortSignal } = {}
): Promise<BranchPage> {
  const limit = Math.min(Math.max(options.limit ?? BRANCH_PAGE, 1), 256),
    query = new URLSearchParams({ limit: String(limit) });
  if (options.after) query.set('after', options.after);
  const raw = await get(`${base(project)}/refs?${query}`, options.signal);
  if (!record(raw)) throw unexpected();
  return parseRefs(raw, limit);
}
export async function loadCommit(
  project: string,
  address: string,
  signal?: AbortSignal
): Promise<CommitSummary> {
  if (!isAddress(address)) throw unexpected();
  const parsed = parseCommit(await get(`${base(project)}/commits/${address}`, signal));
  if (!parsed) throw unexpected();
  return parsed;
}
export async function loadReviews(
  project: string,
  candidate: string,
  options: { after?: number | null; limit?: number; signal?: AbortSignal } = {}
): Promise<ReviewPage> {
  if (!isAddress(candidate)) throw unexpected();
  const limit = Math.min(Math.max(options.limit ?? REVIEW_PAGE, 1), 256),
    query = new URLSearchParams({ candidate, limit: String(limit) });
  if (options.after) query.set('after', String(options.after));
  const raw = await get(`${base(project)}/reviews?${query}`, options.signal);
  if (!record(raw)) throw unexpected();
  return parseReviews(raw, limit);
}
