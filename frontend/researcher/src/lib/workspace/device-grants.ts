/**
 * Client for signing keys: enrolment, revocation, each key's project access mode and its
 * per-project grants (docs/DEVICE-KEY-GRANTS-API.md, schema 40). Only the signed-in account
 * holder can change what a key may open; the browser never sees a key's private half.
 *
 * The generated OpenAPI types describe these bodies only as `unknown`, so the shapes below
 * mirror the contract. Parsers are tolerant: a malformed key or grant is skipped, never
 * guessed at, and a server without grants (before schema 40) simply has no access mode.
 *
 * A key is connected for a person or for an AI agent (schema 42); the choice is fixed. An agent
 * key can hold only `content:read`, a person's key never can, and the server refuses a mix.
 */
import { API_BASE_URL, EDITOR_HEADERS } from '$lib/api/config';

export type DeviceScope =
  | 'project:read'
  | 'history:read'
  | 'history:write'
  | 'history:review'
  | 'content:read';
export type ProjectAccess = 'granted' | 'all';
/** Fixed when a key is connected: for a person, or for an AI agent that can only read. */
export type KeyClass = 'person' | 'agent';

/** A person's scopes: display order, plain-language names and what each lets a key do. */
export const SCOPES: readonly { id: DeviceScope; label: string; help: string }[] = [
  {
    id: 'project:read',
    label: 'Read project data',
    help: 'List the project’s files and recent activity.'
  },
  {
    id: 'history:read',
    label: 'Read history',
    help: 'Fetch commits, branches, receipts and review decisions. Needed to pull and plan a push.'
  },
  {
    id: 'history:write',
    label: 'Write history',
    help: 'Push commits and move the branches the project rules allow.'
  },
  {
    id: 'history:review',
    label: 'Review',
    help: 'Record approvals and requests for changes. Needs an approving role.'
  }
];
/** The one scope of an AI agent's key. It never mixes with a person's scopes. */
export const AGENT_SCOPES: readonly { id: DeviceScope; label: string; help: string }[] = [
  {
    id: 'content:read',
    label: 'Read documents',
    help: 'List the project’s files and read their text, cells and slides, and compare versions. Read only: the agent cannot change anything.'
  }
];
const SCOPE_IDS = [...SCOPES, ...AGENT_SCOPES].map((s) => s.id);
export const scopeLabel = (scope: string) =>
  [...SCOPES, ...AGENT_SCOPES].find((s) => s.id === scope)?.label ?? scope;
/** Which kind of key may hold a scope; null for a name this page does not know. */
export const scopeClass = (scope: string): KeyClass | null =>
  AGENT_SCOPES.some((s) => s.id === scope)
    ? 'agent'
    : SCOPES.some((s) => s.id === scope)
      ? 'person'
      : null;
export const scopesFor = (keyClass: KeyClass) => (keyClass === 'agent' ? AGENT_SCOPES : SCOPES);
/** What a new grant starts with: enough for the command line to pull and push, no review. */
export const DEFAULT_SCOPES: readonly DeviceScope[] = ['history:read', 'history:write'];
export const DEFAULT_AGENT_SCOPES: readonly DeviceScope[] = ['content:read'];
/** The server's per-key limit when an older response does not say. */
export const GRANT_LIMIT = 50;

export const ACCESS_LABEL: Record<ProjectAccess, string> = {
  granted: 'Only projects you grant',
  all: 'All projects you can access'
};

export interface DeviceGrant {
  projectId: string;
  /** Null when the account is no longer a member: the grant does nothing until it rejoins. */
  projectName: string | null;
  role: string | null;
  scopes: DeviceScope[];
  createdAt: string;
}
export interface DeviceKey {
  id: string;
  publicKey: string;
  registeredAt: string;
  revokedAt: string | null;
  /** Null from a server that predates per-project grants. */
  projectAccess: ProjectAccess | null;
  /** A server that predates key classes lists none: every key is then a person's. */
  keyClass: KeyClass;
  /** The name given at enrolment; null when none. */
  label: string | null;
  grants: DeviceGrant[];
}
export interface DeviceRegistry {
  authority: string;
  identity: string;
  keys: DeviceKey[];
  activeLimit: number;
  totalLimit: number;
  grantLimit: number;
}
/** A project the account can open, as listed by GET /projects. */
export interface ProjectChoice {
  id: string;
  name: string;
  role: string;
}
export interface Enrolled {
  id: string;
  publicKey: string;
  projectAccess: ProjectAccess | null;
  keyClass: KeyClass;
}

const record = (value: unknown): Record<string, unknown> | null =>
  value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
const text = (value: unknown) => (typeof value === 'string' ? value : null);
const whole = (value: unknown) =>
  typeof value === 'number' && Number.isSafeInteger(value) && value >= 0 ? value : null;
const access = (value: unknown): ProjectAccess | null =>
  value === 'granted' || value === 'all' ? value : null;
const keyClassOf = (value: unknown): KeyClass => (value === 'agent' ? 'agent' : 'person');

/** Known scopes only, without duplicates, in display order. */
export function normalizeScopes(values: readonly unknown[]): DeviceScope[] {
  return SCOPE_IDS.filter((id) => values.includes(id));
}
export const shortKey = (publicKey: string) => publicKey.slice(0, 8);

function parseGrant(raw: unknown): DeviceGrant | null {
  const row = record(raw),
    projectId = text(row?.project_id),
    createdAt = text(row?.created_at);
  if (!row || !projectId || !createdAt) return null;
  const scopes = Array.isArray(row.scopes) ? normalizeScopes(row.scopes) : [];
  // A grant with no scope the page knows can neither be shown nor meaningfully edited.
  if (!scopes.length) return null;
  return {
    projectId,
    projectName: text(row.project_name),
    role: text(row.role),
    scopes,
    createdAt
  };
}
function parseKey(raw: unknown): DeviceKey | null {
  const row = record(raw),
    id = text(row?.id),
    publicKey = text(row?.public_key),
    registeredAt = text(row?.registered_at);
  if (!row || !id || !publicKey || !registeredAt) return null;
  return {
    id,
    publicKey,
    registeredAt,
    revokedAt: text(row.revoked_at),
    projectAccess: access(row.project_access),
    keyClass: keyClassOf(row.class),
    label: text(row.label)?.trim() || null,
    grants: (Array.isArray(row.grants) ? row.grants : [])
      .map(parseGrant)
      .filter((g): g is DeviceGrant => !!g)
  };
}
/** GET /devices. Null when the body is not a registry at all. */
export function parseRegistry(raw: unknown): DeviceRegistry | null {
  const body = record(raw),
    authority = text(body?.authority),
    identity = text(body?.identity);
  if (!body || !authority || !identity || !Array.isArray(body.keys)) return null;
  return {
    authority,
    identity,
    keys: body.keys.map(parseKey).filter((k): k is DeviceKey => !!k),
    activeLimit: whole(body.active_limit) ?? 0,
    totalLimit: whole(body.total_limit) ?? 0,
    grantLimit: whole(body.grant_limit) || GRANT_LIMIT
  };
}
/** GET /projects, reduced to what a grant picker needs. */
export function parseProjectChoices(raw: unknown): ProjectChoice[] {
  const items = Array.isArray(record(raw)?.items) ? (record(raw)?.items as unknown[]) : [];
  const choices: ProjectChoice[] = [];
  for (const item of items) {
    const row = record(item),
      id = text(row?.id),
      name = text(row?.name);
    if (id && name) choices.push({ id, name, role: text(row?.role) ?? '' });
  }
  return choices;
}

// --- roles ------------------------------------------------------------------

/** Mirrors crates/engine-api/src/device_sessions.rs: who may write history, who may review. */
const ROLES = ['owner', 'manager', 'editor', 'approver', 'reviewer', 'viewer'];
const WRITING = ['owner', 'manager', 'editor', 'approver', 'reviewer'];
const REVIEWING = ['owner', 'manager', 'editor', 'approver'];
/**
 * Whether a project role can use a scope. The server rechecks the role on every request, so
 * this only explains a grant that would be refused; an unknown role is never flagged.
 */
export function scopeUsable(role: string | null, scope: DeviceScope): boolean {
  if (role === null) return false;
  if (!ROLES.includes(role)) return true;
  if (scope === 'history:write') return WRITING.includes(role);
  if (scope === 'history:review') return REVIEWING.includes(role);
  return true;
}
export const unusableScopes = (role: string | null, scopes: readonly DeviceScope[]) =>
  scopes.filter((scope) => !scopeUsable(role, scope));

/** Projects that do not have a grant yet, by name. */
export function grantableProjects(
  projects: readonly ProjectChoice[],
  grants: readonly DeviceGrant[]
): ProjectChoice[] {
  const granted = new Set(grants.map((g) => g.projectId));
  return projects
    .filter((p) => !granted.has(p.id))
    .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }));
}

// --- requests ---------------------------------------------------------------

export interface DeviceRequest {
  method: 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';
  path: string;
  body?: unknown;
}
const seg = encodeURIComponent;
export const registryRequest = (): DeviceRequest => ({ method: 'GET', path: '/devices' });
export const challengeRequest = (publicKey: string): DeviceRequest => ({
  method: 'POST',
  path: '/devices/challenge',
  body: { public_key: publicKey.trim() }
});
/** What the person chose for a key they are connecting. A label only means something for an agent. */
export interface EnrollChoice {
  keyClass: KeyClass;
  label?: string;
}
/**
 * Why a key's name cannot be used, or null. The server's rule: 1 to 80 characters, no control
 * characters. Checked here so the person is told before the one-time proof is sent.
 */
export function labelProblem(label: string): string | null {
  const name = label.trim();
  if (!name) return 'Give the key a name so you can tell it from your other keys.';
  if ([...name].length > 80) return 'Use at most 80 characters.';
  if (/\p{Cc}/u.test(name)) return 'Remove line breaks and other control characters.';
  return null;
}
/**
 * The proof is sent as it is for a person's key, so connecting one is unchanged and still works
 * against a server that predates key classes. An agent key adds `class` and `label`, which are
 * not part of the signed proof.
 */
export function enrollRequest(proof: unknown, choice?: EnrollChoice): DeviceRequest {
  const signed = record(proof);
  if (choice?.keyClass !== 'agent' || !signed)
    return { method: 'POST', path: '/devices', body: proof };
  const label = choice.label?.trim();
  return {
    method: 'POST',
    path: '/devices',
    body: { ...signed, class: 'agent', ...(label ? { label } : {}) }
  };
}
export const revokeKeyRequest = (keyId: string): DeviceRequest => ({
  method: 'POST',
  path: `/devices/${seg(keyId)}/revoke`,
  body: {}
});
export const grantRequest = (
  keyId: string,
  projectId: string,
  scopes: readonly DeviceScope[]
): DeviceRequest => {
  const known = normalizeScopes(scopes);
  // The server refuses a mix of a person's scopes and the agent scope; so does this builder.
  if (new Set(known.map(scopeClass)).size > 1)
    throw new Error('A key holds either a person’s permissions or Read documents, never both.');
  return {
    method: 'PUT',
    path: `/devices/${seg(keyId)}/projects/${seg(projectId)}`,
    body: { scopes: known }
  };
};
export const revokeGrantRequest = (keyId: string, projectId: string): DeviceRequest => ({
  method: 'DELETE',
  path: `/devices/${seg(keyId)}/projects/${seg(projectId)}`
});
/**
 * `all` is the broad mode: the server refuses it without `confirm_all_projects`, and so does
 * this builder, so a caller cannot send it by accident. `granted` needs no confirmation.
 */
export function accessRequest(
  keyId: string,
  mode: ProjectAccess,
  confirmed = false
): DeviceRequest {
  if (mode === 'all' && !confirmed)
    throw new Error('Allowing all projects needs an explicit confirmation.');
  return {
    method: 'PATCH',
    path: `/devices/${seg(keyId)}`,
    body:
      mode === 'all'
        ? { project_access: 'all', confirm_all_projects: true }
        : { project_access: 'granted' }
  };
}

// --- errors -----------------------------------------------------------------

export class DeviceError extends Error {
  constructor(
    readonly status: number,
    readonly reason: string
  ) {
    super(reason || `Request failed (${status})`);
  }
}
/** A problem found in the file the person chose, before anything was sent. */
export class ProofFileError extends Error {}
export const isUnavailable = (error: unknown) =>
  error instanceof DeviceError && [404, 405, 501].includes(error.status);

export type Subject =
  | 'load'
  | 'challenge'
  | 'enroll'
  | 'revoke-key'
  | 'grant'
  | 'revoke-grant'
  | 'mode';
const WRITES: Subject[] = ['grant', 'revoke-grant', 'mode'];
/** A short, specific sentence for a failed request; never a raw status or server text. */
export function deviceMessage(error: unknown, subject: Subject): string {
  if (error instanceof ProofFileError) return error.message;
  if (!(error instanceof DeviceError))
    return error instanceof TypeError
      ? 'Could not reach the server. Check your connection and try again.'
      : 'That could not be completed. Try again in a moment.';
  const { status, reason } = error;
  if (status === 401) return 'Your session has ended. Sign in again to change signing keys.';
  if (status === 403) {
    if (subject === 'grant')
      return 'You are not a member of that project, so it cannot be granted to a key.';
    if (subject === 'enroll')
      return 'This proof was signed for a different account. Create a new challenge from this account.';
    return 'You do not have permission to do that.';
  }
  if (status === 404 || status === 405 || status === 501) {
    if (subject === 'load') return 'Signing keys are not available on this server yet.';
    return 'This signing key was not found. Reload the page and try again.';
  }
  if (status === 409) {
    if (subject === 'grant' || subject === 'mode' || subject === 'revoke-grant') {
      if (/\b50\b|at most|limit/i.test(reason))
        return 'This key already has 50 project grants. Remove one before adding another.';
      if (/revoked/i.test(reason))
        return 'This key was revoked, so its access can no longer change.';
    }
    if (subject === 'challenge')
      return 'This key cannot be connected. It may already be connected or revoked; use a fresh key.';
    if (subject === 'enroll')
      return 'The challenge expired or was already used, or the key limit was reached. Create a new challenge and try again.';
    return 'This changed while you were working. Reload the page and try again.';
  }
  if (status === 400) {
    if (subject === 'grant') {
      // A person's permission on an AI agent's key, or the reverse: the key's type is fixed.
      if (/agent key can hold only/i.test(reason))
        return 'A key for an AI agent can only be given Read documents.';
      if (/content scopes need|enrolled for an AI agent/i.test(reason))
        return 'Only a key set up for an AI agent can be given Read documents. A key’s type is fixed when it is connected.';
      return 'Choose at least one permission for the project.';
    }
    if (subject === 'enroll' && /label must/i.test(reason))
      return 'The key name must be 1 to 80 characters, with no line breaks.';
    if (subject === 'enroll' && /class/i.test(reason))
      return 'Choose whether this key is for you or for an AI agent, then try again.';
    if (subject === 'mode')
      return 'Confirm that this key may open every project you can access, then try again.';
    if (subject === 'challenge')
      return 'That is not a valid public key. Use the 64 hexadecimal characters the command prints.';
    if (subject === 'enroll')
      return 'The enrollment proof was not accepted. Create a new challenge and sign it again.';
    return 'That request was not accepted.';
  }
  if (status === 413) return 'That file is too large to send.';
  if (status === 422)
    return subject === 'enroll'
      ? 'The server did not accept that. Keys for an AI agent may not be available on this installation yet.'
      : 'The server did not understand that request. Reload the page and try again.';
  if (status === 429) return 'Too many requests. Wait a moment, then try again.';
  if (status >= 500 && status < 600)
    return WRITES.includes(subject)
      ? 'The server could not save that change. Try again in a moment.'
      : 'The server is busy or timed out. Try again in a moment.';
  if (status === 0) return 'The server sent a response this page does not understand.';
  return `That could not be completed (${status}).`;
}

async function send(request: DeviceRequest, signal?: AbortSignal): Promise<unknown> {
  const headers: Record<string, string> = { ...EDITOR_HEADERS };
  if (request.body !== undefined) headers['content-type'] = 'application/json';
  const response = await fetch(`${API_BASE_URL}${request.path}`, {
    method: request.method,
    headers,
    body: request.body === undefined ? undefined : JSON.stringify(request.body),
    signal
  });
  const data: unknown = await response.json().catch(() => null);
  if (!response.ok) {
    const failure = record(data),
      details = record(failure?.details);
    throw new DeviceError(response.status, text(details?.reason) || text(failure?.error) || '');
  }
  return data;
}
const unexpected = () => new DeviceError(0, 'Unexpected response');

export async function loadRegistry(signal?: AbortSignal): Promise<DeviceRegistry> {
  const registry = parseRegistry(await send(registryRequest(), signal));
  if (!registry) throw unexpected();
  return registry;
}
export async function loadProjectChoices(signal?: AbortSignal): Promise<ProjectChoice[]> {
  return parseProjectChoices(await send({ method: 'GET', path: '/projects' }, signal));
}
/** The challenge is kept exactly as the server sent it: it is what the person signs. */
export async function createChallenge(publicKey: string): Promise<Record<string, unknown>> {
  const challenge = record(await send(challengeRequest(publicKey)));
  if (!challenge) throw unexpected();
  return challenge;
}
export async function enrollKey(proof: unknown, choice?: EnrollChoice): Promise<Enrolled> {
  const body = record(await send(enrollRequest(proof, choice)));
  return {
    id: text(body?.id) ?? '',
    publicKey: text(body?.public_key) ?? '',
    projectAccess: access(body?.project_access),
    keyClass: keyClassOf(body?.class)
  };
}
export async function revokeKey(keyId: string): Promise<void> {
  await send(revokeKeyRequest(keyId));
}
export async function setGrant(
  keyId: string,
  projectId: string,
  scopes: readonly DeviceScope[]
): Promise<{ scopes: DeviceScope[]; changed: boolean }> {
  const body = record(await send(grantRequest(keyId, projectId, scopes)));
  if (!body) throw unexpected();
  return {
    scopes: Array.isArray(body.scopes) ? normalizeScopes(body.scopes) : [],
    changed: body.changed !== false
  };
}
export async function clearGrant(keyId: string, projectId: string): Promise<{ changed: boolean }> {
  const body = record(await send(revokeGrantRequest(keyId, projectId)));
  if (!body) throw unexpected();
  return { changed: body.changed !== false };
}
export async function setAccess(
  keyId: string,
  mode: ProjectAccess,
  confirmed = false
): Promise<{ projectAccess: ProjectAccess; changed: boolean }> {
  const body = record(await send(accessRequest(keyId, mode, confirmed)));
  const next = access(body?.project_access);
  if (!body || !next) throw unexpected();
  return { projectAccess: next, changed: body.changed !== false };
}
