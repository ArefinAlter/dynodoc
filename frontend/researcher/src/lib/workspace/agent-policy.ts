/**
 * Client for a project's AI agent settings (docs/AI-AGENT-CONNECTORS.md sections 5 and 7.2):
 * the policy an owner or manager saves, the usage report and the kill switch. Only a project
 * owner or manager may call these routes; a person with any other role is refused.
 *
 * The routes exist whether or not the installation serves agents (`AGENT_GATEWAY=on`), and the
 * policy says which. A saved policy replaces the whole of the old one and carries the revision
 * it was made from in `If-Match`, so two people saving at once cannot overwrite each other.
 *
 * The generated OpenAPI types describe these bodies, but parsers here stay tolerant like the
 * device-key client: a malformed row is skipped, never guessed at, and a policy this page
 * cannot read in full is refused, because saving over a guess could widen who may read.
 */
import { API_BASE_URL, EDITOR_HEADERS } from '$lib/api/config';

export type AgentKind = 'document' | 'spreadsheet' | 'presentation';
/** File kinds in the order the server stores and the page shows them. */
export const KIND_OPTIONS: readonly { id: AgentKind; label: string }[] = [
  { id: 'document', label: 'Documents' },
  { id: 'spreadsheet', label: 'Spreadsheets' },
  { id: 'presentation', label: 'Presentations' }
];
const KIND_IDS: readonly string[] = KIND_OPTIONS.map((k) => k.id);

/**
 * Project roles in the policy's own vocabulary, lowest first. These are not the storage names
 * `projectRoleName` translates (there `approver` shows as Reviewer and `reviewer` as
 * Contributor); the policy already uses the names people see, so do not pass them through it.
 */
export const ROLE_FLOORS: readonly { id: string; label: string }[] = [
  { id: 'viewer', label: 'Viewer' },
  { id: 'contributor', label: 'Contributor' },
  { id: 'reviewer', label: 'Reviewer' },
  { id: 'editor', label: 'Editor' },
  { id: 'manager', label: 'Manager' },
  { id: 'owner', label: 'Owner' }
];
export const roleFloorLabel = (role: string) =>
  ROLE_FLOORS.find((r) => r.id === role)?.label ?? role;
export const floorHelp = (role: string) =>
  role === 'viewer'
    ? 'Every member of the project can have an AI agent read it.'
    : `Members below ${roleFloorLabel(role)} cannot have an AI agent read this project.`;

/** The server's bounds for the two budgets. */
export const CALLS_PER_MINUTE = { min: 1, max: 600, fallback: 60 };
export const READ_BYTES_PER_DAY = { min: 10_000, max: 1_000_000_000, fallback: 20_000_000 };
const BYTES_PER_MB = 1_000_000;
export const bytesToMb = (bytes: number) => bytes / BYTES_PER_MB;
export const mbToBytes = (mb: number) => Math.round(mb * BYTES_PER_MB);
/** Bounds as the page shows them: megabytes, not bytes. */
export const MB_RANGE = {
  min: bytesToMb(READ_BYTES_PER_DAY.min),
  max: bytesToMb(READ_BYTES_PER_DAY.max)
};

export interface FileOverride {
  documentId: string;
  /** `none` hides the file from agents; `read` never widens the project's settings. */
  access: 'none' | 'read';
  updatedAt: string;
}
export interface AgentPolicy {
  projectId: string;
  enabled: boolean;
  minRole: string;
  kinds: AgentKind[];
  callsPerMinute: number;
  readBytesPerDay: number;
  /** Send back in `If-Match`. Zero until the first save. */
  revision: number;
  /** False until an owner or manager first saves: agents are then off. */
  configured: boolean;
  /** Whether this installation serves the agent routes at all. */
  gateway: boolean;
  /** When the kill switch was last pulled. */
  killedAt: string | null;
  fileOverrides: FileOverride[];
  overridesTruncated: boolean;
  /** On a save only: false when nothing was written because nothing differed. */
  changed: boolean | null;
}
export interface KillResult {
  projectId: string;
  wasEnabled: boolean;
  revokedGrants: number;
  revision: number;
}
export interface UsageRow {
  day: string;
  client: string | null;
  calls: number;
  bytesOut: number;
  /** Calls the policy, a grant check or a budget refused. */
  refused: number;
  /** Calls that failed for another reason. */
  errors: number;
}
export interface Usage {
  from: string;
  to: string;
  items: UsageRow[];
  truncated: boolean;
}
export interface DayTotal {
  day: string;
  calls: number;
  bytesOut: number;
  refused: number;
}

const record = (value: unknown): Record<string, unknown> | null =>
  value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
const text = (value: unknown) => (typeof value === 'string' ? value : null);
const whole = (value: unknown) =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : null;

function parseOverride(raw: unknown): FileOverride | null {
  const row = record(raw),
    documentId = text(row?.document_id),
    updatedAt = text(row?.updated_at);
  const access = row?.access;
  if (!documentId || !updatedAt || (access !== 'none' && access !== 'read')) return null;
  return { documentId, access, updatedAt };
}
/** A policy view (GET and PUT answer with the same shape). Null when it cannot be read in full. */
export function parsePolicy(raw: unknown): AgentPolicy | null {
  const body = record(raw),
    projectId = text(body?.project_id),
    minRole = text(body?.min_role),
    calls = whole(body?.calls_per_minute),
    bytes = whole(body?.read_bytes_per_day),
    revision = whole(body?.revision);
  if (
    !body ||
    !projectId ||
    !minRole ||
    !ROLE_FLOORS.some((r) => r.id === minRole) ||
    calls === null ||
    bytes === null ||
    revision === null ||
    typeof body.enabled !== 'boolean' ||
    typeof body.gateway !== 'boolean' ||
    !Array.isArray(body.kinds)
  )
    return null;
  const kinds = KIND_OPTIONS.filter((k) => (body.kinds as unknown[]).includes(k.id)).map(
    (k) => k.id
  );
  // A kind this page does not know could not be shown, so saving would silently drop it.
  if (!kinds.length || (body.kinds as unknown[]).some((k) => !KIND_IDS.includes(String(k))))
    return null;
  return {
    projectId,
    enabled: body.enabled,
    minRole,
    kinds,
    callsPerMinute: calls,
    readBytesPerDay: bytes,
    revision,
    configured: body.configured === true,
    gateway: body.gateway,
    killedAt: text(body.killed_at),
    fileOverrides: (Array.isArray(body.file_overrides) ? body.file_overrides : [])
      .map(parseOverride)
      .filter((o): o is FileOverride => !!o),
    overridesTruncated: body.file_overrides_truncated === true,
    changed: typeof body.changed === 'boolean' ? body.changed : null
  };
}
export function parseKill(raw: unknown): KillResult | null {
  const body = record(raw),
    projectId = text(body?.project_id),
    revoked = whole(body?.revoked_grants),
    revision = whole(body?.revision);
  if (!body || !projectId || revoked === null || revision === null) return null;
  return { projectId, wasEnabled: body.was_enabled === true, revokedGrants: revoked, revision };
}
export function parseUsage(raw: unknown): Usage | null {
  const body = record(raw),
    from = text(body?.from),
    to = text(body?.to);
  if (!body || !from || !to || !Array.isArray(body.items)) return null;
  const items: UsageRow[] = [];
  for (const item of body.items) {
    const row = record(item),
      day = text(row?.day),
      calls = whole(row?.calls),
      bytesOut = whole(row?.bytes_out);
    if (!row || !day || calls === null || bytesOut === null) continue;
    items.push({
      day,
      client: text(row.client),
      calls,
      bytesOut,
      refused: whole(row.denials) ?? 0,
      errors: whole(row.errors) ?? 0
    });
  }
  return { from, to, items, truncated: body.truncated === true };
}

// --- the form ---------------------------------------------------------------

/** What the person is editing. Numbers are null while a field is empty. */
export interface PolicyDraft {
  enabled: boolean;
  minRole: string;
  kinds: AgentKind[];
  calls: number | null;
  megabytes: number | null;
}
export const draftOf = (policy: AgentPolicy): PolicyDraft => ({
  enabled: policy.enabled,
  minRole: policy.minRole,
  kinds: [...policy.kinds],
  calls: policy.callsPerMinute,
  megabytes: bytesToMb(policy.readBytesPerDay)
});
export function sameDraft(a: PolicyDraft, b: PolicyDraft): boolean {
  const kinds = (d: PolicyDraft) => KIND_IDS.filter((k) => d.kinds.includes(k as AgentKind));
  return (
    a.enabled === b.enabled &&
    a.minRole === b.minRole &&
    a.calls === b.calls &&
    a.megabytes === b.megabytes &&
    kinds(a).join() === kinds(b).join()
  );
}
export interface DraftProblems {
  kinds: string | null;
  calls: string | null;
  megabytes: string | null;
}
const grouped = (n: number) => n.toLocaleString('en');
/** Messages for what the server would refuse, so the person is told before saving. */
export function draftProblems(draft: PolicyDraft): DraftProblems {
  const calls = draft.calls,
    mb = draft.megabytes;
  return {
    kinds: draft.kinds.length ? null : 'Choose at least one kind of file.',
    calls:
      calls !== null &&
      Number.isInteger(calls) &&
      calls >= CALLS_PER_MINUTE.min &&
      calls <= CALLS_PER_MINUTE.max
        ? null
        : `Enter a whole number from ${CALLS_PER_MINUTE.min} to ${CALLS_PER_MINUTE.max}.`,
    megabytes:
      mb !== null &&
      Number.isFinite(mb) &&
      mbToBytes(mb) >= READ_BYTES_PER_DAY.min &&
      mbToBytes(mb) <= READ_BYTES_PER_DAY.max
        ? null
        : `Enter an amount from ${grouped(MB_RANGE.min)} MB to ${grouped(MB_RANGE.max)} MB.`
  };
}
export const hasProblems = (problems: DraftProblems) => Object.values(problems).some(Boolean);

// --- requests ---------------------------------------------------------------

export interface AgentRequest {
  method: 'GET' | 'PUT' | 'POST';
  path: string;
  headers?: Record<string, string>;
  body?: unknown;
}
const seg = encodeURIComponent;
export const policyRequest = (projectId: string): AgentRequest => ({
  method: 'GET',
  path: `/projects/${seg(projectId)}/agent-policy`
});
/**
 * A save replaces the whole policy, so every field is sent. `revision` is the one the page
 * loaded; the server answers 409 when the policy has moved on since.
 */
export function saveRequest(projectId: string, revision: number, draft: PolicyDraft): AgentRequest {
  if (hasProblems(draftProblems(draft)))
    throw new Error('These settings are outside the allowed ranges.');
  return {
    method: 'PUT',
    path: `/projects/${seg(projectId)}/agent-policy`,
    headers: { 'if-match': `"${revision}"` },
    body: {
      enabled: draft.enabled,
      min_role: draft.minRole,
      kinds: KIND_IDS.filter((k) => draft.kinds.includes(k as AgentKind)),
      calls_per_minute: draft.calls,
      read_bytes_per_day: mbToBytes(draft.megabytes ?? 0)
    }
  };
}
export const killRequest = (projectId: string): AgentRequest => ({
  method: 'POST',
  path: `/projects/${seg(projectId)}/agent-policy/kill`
});
const isoDay = (date: Date) => date.toISOString().slice(0, 10);
/** The last `days` UTC days up to and including today; the server allows at most 31. */
export function usageRequest(projectId: string, days = 7, now = new Date()): AgentRequest {
  const count = Math.min(31, Math.max(1, Math.trunc(days)));
  const from = new Date(now.getTime() - (count - 1) * 86_400_000);
  return {
    method: 'GET',
    path: `/projects/${seg(projectId)}/agent-usage?from=${isoDay(from)}&to=${isoDay(now)}`
  };
}

// --- what the screen shows ----------------------------------------------------

/** Rows are per day and per client name; the table shows one line a day, newest first. */
export function dailyTotals(items: readonly UsageRow[]): DayTotal[] {
  const days = new Map<string, DayTotal>();
  for (const row of items) {
    const total = days.get(row.day) ?? { day: row.day, calls: 0, bytesOut: 0, refused: 0 };
    total.calls += row.calls;
    total.bytesOut += row.bytesOut;
    total.refused += row.refused;
    days.set(row.day, total);
  }
  return [...days.values()].sort((a, b) => (a.day < b.day ? 1 : a.day > b.day ? -1 : 0));
}
const SIZE_UNITS = ['B', 'kB', 'MB', 'GB'];
/** Decimal units, like the budget the owner sets (20 MB is 20,000,000 bytes). */
export function formatBytes(bytes: number): string {
  let value = Math.max(0, bytes),
    unit = 0;
  while (value >= 1000 && unit < SIZE_UNITS.length - 1) {
    value /= 1000;
    unit++;
  }
  const digits = unit === 0 ? 0 : 1;
  return `${new Intl.NumberFormat('en', { maximumFractionDigits: digits }).format(value)} ${SIZE_UNITS[unit]}`;
}
/** A UTC calendar day (YYYY-MM-DD) as a short date, without the viewer's time zone moving it. */
export function formatDay(day: string): string {
  const [year, month, date] = day.split('-').map(Number);
  const at = new Date(Date.UTC(year, month - 1, date));
  return Number.isFinite(at.getTime())
    ? at.toLocaleDateString('en-GB', {
        timeZone: 'UTC',
        day: 'numeric',
        month: 'short',
        year: 'numeric'
      })
    : day;
}
/** What a save did, in a sentence. */
export function savedMessage(policy: AgentPolicy): string {
  if (policy.changed === false) return 'Nothing changed: these settings were already saved.';
  const state = `Saved. AI agents are ${policy.enabled ? 'on' : 'off'} for this project.`;
  return policy.enabled && !policy.gateway
    ? `${state} They will work only once your operator turns the agent connector on.`
    : state;
}
/** What the kill switch did, in a sentence. */
export function killMessage(result: KillResult): string {
  const ended =
    result.revokedGrants === 0
      ? 'No AI agent key had access to this project.'
      : result.revokedGrants === 1
        ? 'One AI agent key lost its access to this project.'
        : `${result.revokedGrants} AI agent keys lost their access to this project.`;
  return `${result.wasEnabled ? '' : 'AI agents were already off. '}AI agents are off for this project. ${ended} Every open agent session is refused from now on. People’s own keys are untouched.`;
}
export function overrideLabel(access: FileOverride['access']): string {
  return access === 'none'
    ? 'Hidden from AI agents'
    : 'Marked read; this never gives agents more than the project settings';
}
/** A file's name when the page knows it, otherwise the start of its id (what agents quote). */
export const fileLabel = (documentId: string, titles: Readonly<Record<string, string>> = {}) =>
  titles[documentId] || `File ${documentId.slice(0, 8)}`;

// --- errors -----------------------------------------------------------------

export class AgentPolicyError extends Error {
  constructor(
    readonly status: number,
    readonly reason: string
  ) {
    super(reason || `Request failed (${status})`);
  }
}
export const isUnavailable = (error: unknown) =>
  error instanceof AgentPolicyError && [404, 405, 501].includes(error.status);
/** A save made from a revision that is no longer the current one. */
export const isConflict = (error: unknown) =>
  error instanceof AgentPolicyError && error.status === 409;

export type Subject = 'load' | 'save' | 'kill' | 'usage';
/** A short, specific sentence for a failed request; never a raw status or server text. */
export function agentMessage(error: unknown, subject: Subject): string {
  // A stop that did not happen must say so first: the person may be relying on it.
  const lead = subject === 'kill' ? 'AI agents were not stopped. ' : '';
  if (!(error instanceof AgentPolicyError))
    return `${lead}${
      error instanceof TypeError
        ? 'Could not reach the server. Check your connection and try again.'
        : 'That could not be completed. Try again in a moment.'
    }`;
  const { status } = error;
  if (status === 401)
    return `${lead}Your session has ended. Sign in again to change AI agent settings.`;
  if (status === 403)
    return `${lead}Only a project owner or manager can see or change AI agent settings.`;
  if (status === 404 || status === 405 || status === 501)
    return subject === 'load'
      ? 'AI agent settings are not available on this server yet.'
      : `${lead}This project was not found. Reload the page and try again.`;
  if (status === 409)
    return 'Someone else changed these settings while you were editing. The latest settings are shown below; check them and save again if you still want your change.';
  if (status === 400)
    return subject === 'save'
      ? 'One of the values was not accepted. Check the numbers and try again.'
      : 'That request was not accepted.';
  if (status === 422)
    return `${lead}The server did not understand that request. Reload the page and try again.`;
  if (status === 429) return `${lead}Too many requests. Wait a moment, then try again.`;
  if (status >= 500 && status < 600)
    return subject === 'load' || subject === 'usage'
      ? 'The server is busy or timed out. Try again in a moment.'
      : `${lead}The server could not save that change. Try again in a moment.`;
  if (status === 0) return `${lead}The server sent a response this page does not understand.`;
  return `${lead}That could not be completed (${status}).`;
}

async function send(request: AgentRequest): Promise<unknown> {
  const headers: Record<string, string> = { ...EDITOR_HEADERS, ...request.headers };
  if (request.body !== undefined) headers['content-type'] = 'application/json';
  const response = await fetch(`${API_BASE_URL}${request.path}`, {
    method: request.method,
    headers,
    body: request.body === undefined ? undefined : JSON.stringify(request.body)
  });
  const data: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const failure = record(data),
      details = record(failure?.details);
    throw new AgentPolicyError(
      response.status,
      text(details?.reason) || text(failure?.error) || ''
    );
  }
  return data;
}
const unexpected = () => new AgentPolicyError(0, 'Unexpected response');

export async function loadPolicy(projectId: string): Promise<AgentPolicy> {
  const policy = parsePolicy(await send(policyRequest(projectId)));
  if (!policy) throw unexpected();
  return policy;
}
export async function savePolicy(
  projectId: string,
  revision: number,
  draft: PolicyDraft
): Promise<AgentPolicy> {
  const policy = parsePolicy(await send(saveRequest(projectId, revision, draft)));
  if (!policy) throw unexpected();
  return policy;
}
export async function killAgents(projectId: string): Promise<KillResult> {
  const result = parseKill(await send(killRequest(projectId)));
  if (!result) throw unexpected();
  return result;
}
export async function loadUsage(projectId: string, days = 7): Promise<Usage> {
  const usage = parseUsage(await send(usageRequest(projectId, days)));
  if (!usage) throw unexpected();
  return usage;
}
