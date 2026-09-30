import type { DocumentState, EventPayload, MaterializedNode } from '$lib/api/types';
import { workspaceRequest } from './api';
import type { DocxImport } from './docx-import';
import { matchScore, planPush, readOrigin, revisionFromName, type PushPlan } from './file-push';

export interface Person {
  id: string;
  name: string | null;
  email: string;
}
export interface ChangeRequest {
  id: string;
  name: string;
  note: string;
  source: {
    kind?: string;
    filename?: string;
    base_reason?: string;
    upload_id?: string;
    summary?: Record<string, number>;
    start_seq?: number;
    /** Set by the server when the file shares little with the revision it started from. */
    unrelated?: boolean;
    similarity?: number;
    retention?: number;
    relation?: string;
  };
  base_seq: number;
  created_at: string;
  submitted_at: string | null;
  merged_at: string | null;
  review_outcome: 'merged' | 'declined' | 'withdrawn' | null;
  reviewed_at: string | null;
  review_note: string;
  revision: number;
  author: Person;
  reviewer: Person | null;
  edits: number;
  status: 'open' | 'merged' | 'declined' | 'withdrawn';
  mine: boolean;
}
export interface Conflict {
  key: string;
  node_id: string;
  field: string;
  ancestor: unknown;
  team: unknown;
  draft: unknown;
}
export interface RequestDetail {
  request: ChangeRequest;
  team_seq: number;
  revision?: number;
  ops?: EventPayload[];
  conflicts?: Conflict[];
  own?: EventPayload[];
  nodes?: Record<
    string,
    {
      ancestor: MaterializedNode | null;
      draft: MaterializedNode | null;
      team: MaterializedNode | null;
    }
  >;
  behind?: number;
  review?: import('./collaboration').ReviewState;
}
const base = (documentId: string) => `/documents/${documentId}`;
export const listRequests = (documentId: string) =>
  workspaceRequest<{ items: ChangeRequest[] }>(`${base(documentId)}/change-requests`);
export const requestDetail = (documentId: string, id: string) =>
  workspaceRequest<RequestDetail>(`${base(documentId)}/change-requests/${id}`);
export const mergeRequest = (
  documentId: string,
  id: string,
  body: {
    team_seq: number;
    revision: number;
    resolutions: Record<string, string>;
    included_nodes: string[];
    note?: string;
    override_rules?: boolean;
  }
) =>
  workspaceRequest<{ completed: boolean; remaining: number; team_seq: number; events: unknown[] }>(
    `${base(documentId)}/change-requests/${id}/merge`,
    body
  );
export const declineRequest = (documentId: string, id: string, note: string) =>
  workspaceRequest(`${base(documentId)}/change-requests/${id}/decline`, { note });
export const submitDraft = (documentId: string, id: string, note: string) =>
  workspaceRequest(`${base(documentId)}/drafts/${id}/submit`, { note });
export const withdrawDraft = (documentId: string, id: string) =>
  workspaceRequest(`${base(documentId)}/drafts/${id}/withdraw`, {});

export interface BaseCandidate {
  seq: number;
  reason: 'file' | 'name' | 'original' | 'version' | 'latest';
  label: string;
  score?: number;
}
export interface PushPreparation {
  filename: string;
  bytes: Uint8Array;
  incoming: DocxImport;
  candidates: BaseCandidate[];
  chosen: BaseCandidate;
  plan: PushPlan;
  base: DocumentState;
  /** The file names another Dynodoc document; pushing here is probably a mistake. */
  otherDocument?: string;
}

async function readWordFile(file: File) {
  const ext = file.name.split('.').pop()?.toLowerCase();
  if (file.size > 10 * 1024 * 1024) throw new Error('Please use a file smaller than 10 MB.');
  let buffer = await file.arrayBuffer();
  if (ext === 'doc') {
    const data = new FormData();
    data.set('file', file);
    const response = await fetch('/api/convert-doc', { method: 'POST', body: data });
    if (!response.ok)
      throw new Error((await response.json()).error || 'Legacy Word conversion is unavailable.');
    buffer = await response.arrayBuffer();
  } else if (ext !== 'docx')
    throw new Error('Choose the Word file (.docx or .doc) you edited on your computer.');
  const bytes = new Uint8Array(buffer.slice(0));
  const { convertDocx } = await import('./office-import');
  const incoming =
    typeof Worker === 'undefined'
      ? (await import('./docx-import')).readDocx(bytes)
      : await convertDocx(buffer);
  return { bytes, incoming };
}
async function stateAt(documentId: string, seq: number) {
  return (
    await workspaceRequest<{ state: DocumentState }>(`${base(documentId)}/state?through_seq=${seq}`)
  ).state;
}
/** Plan a push against one starting revision, including its Word round trip. */
export async function planAgainst(
  documentId: string,
  title: string,
  incoming: DocxImport,
  seq: number,
  known?: DocumentState
) {
  const state = known || (await stateAt(documentId, seq));
  const { writeDocx } = await import('./docx-export'),
    { readDocx } = await import('./docx-import');
  const exported = new Uint8Array(await (await writeDocx(state, title)).arrayBuffer());
  return { state, plan: planPush(state, incoming, readDocx(exported)) };
}

/**
 * Read a pushed Word file and decide which revision it started from: recorded file
 * metadata first, then the `(v42)` naming convention, then the closest saved revision.
 */
export async function preparePush(
  documentId: string,
  title: string,
  file: File
): Promise<PushPreparation> {
  const { bytes, incoming } = await readWordFile(file);
  const origin = readOrigin(bytes);
  const points = await workspaceRequest<{
    latest_seq: number;
    initial_seq: number;
    versions: { name: string; through_seq: number }[];
  }>(`${base(documentId)}/history-points`);
  const candidates: BaseCandidate[] = [];
  const add = (candidate: BaseCandidate) => {
    if (candidate.seq < 0 || candidate.seq > points.latest_seq) return;
    if (!candidates.some((c) => c.seq === candidate.seq)) candidates.push(candidate);
  };
  if (origin.documentId === documentId && origin.seq !== undefined)
    add({
      seq: origin.seq,
      reason: 'file',
      label: `The copy you downloaded (v${origin.seq}${origin.exportedAt ? `, ${new Date(origin.exportedAt).toLocaleString()}` : ''})`
    });
  const named = revisionFromName(file.name);
  if (named !== undefined && !origin.documentId)
    add({ seq: named, reason: 'name', label: `v${named}, from the file name` });
  if (points.initial_seq)
    add({ seq: points.initial_seq, reason: 'original', label: 'The original upload' });
  for (const v of points.versions.slice(0, 8))
    add({ seq: v.through_seq, reason: 'version', label: `Named version “${v.name}”` });
  add({ seq: points.latest_seq, reason: 'latest', label: 'The current team version' });
  let chosen = candidates.find((c) => c.reason === 'file');
  const states = new Map<number, DocumentState>();
  if (!chosen) {
    // Score each candidate by how many of the file's paragraphs it already contains.
    for (const candidate of candidates.slice(0, 10)) {
      const state = await stateAt(documentId, candidate.seq);
      states.set(candidate.seq, state);
      candidate.score = matchScore(state, incoming);
    }
    const priority = { file: 5, name: 4, original: 3, version: 2, latest: 1 };
    chosen = [...candidates].sort(
      (a, b) =>
        (b.score ?? 0) - (a.score ?? 0) || priority[b.reason] - priority[a.reason] || b.seq - a.seq
    )[0];
  }
  const { state, plan } = await planAgainst(
    documentId,
    title,
    incoming,
    chosen.seq,
    states.get(chosen.seq)
  );
  return {
    filename: file.name,
    bytes,
    incoming,
    candidates,
    chosen,
    plan,
    base: state,
    otherDocument:
      origin.documentId && origin.documentId !== documentId ? origin.documentId : undefined
  };
}

/** Create the draft (and optionally the review request) from a prepared push. */
export async function sendPush(
  documentId: string,
  prepared: PushPreparation,
  options: { name: string; note: string; submit: boolean; keepFile: boolean }
) {
  let upload_id: string | undefined;
  if (options.keepFile) {
    let binary = '';
    for (let i = 0; i < prepared.bytes.length; i += 8192)
      binary += String.fromCharCode(...prepared.bytes.subarray(i, i + 8192));
    try {
      upload_id = (
        await workspaceRequest<{ id: string }>(`${base(documentId)}/uploads`, {
          filename: prepared.filename,
          content_type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document',
          content: btoa(binary),
          report: { kind: 'change_request', warnings: prepared.plan.warnings }
        })
      ).id;
    } catch {
      upload_id = undefined; // Keeping the original is optional; the changes still go through.
    }
  }
  const summary = { added: 0, edited: 0, removed: 0, moved: 0 };
  for (const change of prepared.plan.changes) summary[change.kind]++;
  return workspaceRequest<{ id: string; revision: number; submitted: boolean }>(
    `${base(documentId)}/change-requests`,
    {
      name: options.name,
      note: options.note,
      base_seq: prepared.chosen.seq,
      ops: prepared.plan.ops,
      submit: options.submit,
      source: {
        kind: 'file',
        filename: prepared.filename.slice(0, 255),
        base_reason: prepared.chosen.reason,
        upload_id,
        anchored: prepared.plan.anchored,
        summary
      }
    }
  );
}
