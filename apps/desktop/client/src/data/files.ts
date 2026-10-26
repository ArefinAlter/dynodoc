// Files: adding Word, Excel and PowerPoint files to a project, writing a copy back out (and
// opening it in the program the system uses for it), and the folder watch that turns a file saved
// by that program into changes.
//
// The page has no file access and no network. The app shows the system's dialogs, reads and writes
// the files, and makes the requests (`call`); the Office codec worker (office-worker.js, the web
// workspace's own importers, exporters and push planner) turns bytes into operations and back. The
// engine receives the same operations from here as from the web workspace.
import { state, render, toast } from '../state.ts';
import { call } from '../bridge.ts';
import { currentTransport, refresh } from './live.ts';
import type { Reply, Transport } from './remote.ts';

interface OfficeApi {
  importFile(bytes: Uint8Array, name: string): Promise<ImportResult>;
  exportFile(kind: string, state: unknown, title: string, origin?: { documentId: string; seq: number }): Promise<Uint8Array>;
  planPush(bytes: Uint8Array, name: string, base: unknown, title: string): Promise<{ origin: { documentId?: string; seq?: number }; plan: Plan }>;
  originOf(bytes: Uint8Array): Promise<{ documentId?: string; seq?: number }>;
}

interface ImportResult {
  kind: 'document' | 'spreadsheet' | 'presentation' | 'questionnaire';
  ops: Record<string, unknown>[];
  warnings: string[];
  filename: string;
  rootId: string;
}

export interface Plan {
  ops: Record<string, unknown>[];
  changes: { kind: 'added' | 'edited' | 'removed' | 'moved'; nodeId: string; before: string; after: string; formatting?: boolean }[];
  unchanged: number;
  anchored: number;
  warnings: string[];
}

export interface Picked {
  token: string;
  name: string;
  size: number;
  path: string;
}

export interface WatchChange {
  project_id: string;
  rel_path: string;
  name: string;
  kind: 'new' | 'changed';
  document_id: string | null;
  base_seq: number | null;
  size: number;
  mtime_ms: number;
  token: string;
}

function office(): OfficeApi {
  const api = (window as unknown as { dynodocOffice?: OfficeApi }).dynodocOffice;
  if (!api) throw new Error('The file converter did not load. Close the app and open it again.');
  return api;
}

const text = (e: unknown) => (e instanceof Error ? e.message : String(e));

function fromBase64(b64: string): Uint8Array {
  const bin = atob(b64);
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

function toBase64(bytes: Uint8Array): string {
  let bin = '';
  for (let i = 0; i < bytes.length; i += 0x2000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x2000));
  return btoa(bin);
}

function body(r: Reply, what: string): any {
  if (r.status >= 200 && r.status < 300) return r.body;
  const detail = r.body && typeof r.body === 'object' && typeof r.body.error === 'string' ? r.body.error : '';
  throw new Error(detail && detail !== 'forbidden' ? detail : r.status === 403 ? `Your role does not allow this (${what}).` : `The workspace could not ${what} (${r.status}).`);
}

export const EXT: Record<string, string> = { document: 'docx', spreadsheet: 'xlsx', presentation: 'pptx' };
const EDITOR: Record<string, string> = { document: 'Word', spreadsheet: 'Excel', presentation: 'PowerPoint' };

/** Operations in requests of a few hundred kilobytes, in order (a node's parent comes first). */
function chunks(ops: Record<string, unknown>[], limit = 400_000): Record<string, unknown>[][] {
  const out: Record<string, unknown>[][] = [];
  let cur: Record<string, unknown>[] = [];
  let size = 0;
  for (const op of ops) {
    const n = JSON.stringify(op).length;
    if (cur.length && (size + n > limit || cur.length >= 500)) {
      out.push(cur);
      cur = [];
      size = 0;
    }
    cur.push(op);
    size += n;
  }
  if (cur.length) out.push(cur);
  return out;
}

export interface Added {
  id: string;
  title: string;
  kind: string;
  seq: number;
  warnings: string[];
}

/** Makes one file a document of the project: read, convert, create, write its blocks, keep the original. */
export async function addFile(t: Transport, projectId: string, token: string): Promise<Added> {
  const file = (await call('file_read', { token })) as { name: string; size: number; base64: string };
  const bytes = fromBase64(file.base64);
  const imported = await office().importFile(bytes, file.name);
  const title = file.name.replace(/\.[^.]+$/, '').slice(0, 200) || 'Untitled';
  const doc = body(await t.apiSend('POST', 'documents', { title }), 'create the file');
  const id: string = doc.id;
  try {
    body(await t.apiSend('POST', `documents/${id}/metadata`, { settings: { kind: imported.kind, import_warnings: imported.warnings } }), 'describe the file');
    let seq = 0;
    for (const part of chunks(imported.ops)) {
      const res = body(await t.apiSend('POST', `documents/${id}/batch`, { ops: part, base_seq: seq }), 'add the content');
      const events = (res.events ?? []) as { seq: number }[];
      if (events.length) seq = events[events.length - 1].seq;
    }
    try {
      body(await t.apiSend('POST', `documents/${id}/uploads`, { filename: file.name, content_type: 'application/octet-stream', content: file.base64, report: { warnings: imported.warnings } }), 'keep the original');
    } catch {
      /* the original is kept when it can be; the content is already in */
    }
    body(await t.apiSend('POST', `documents/${id}/location`, { space_id: projectId }), 'put the file in the project');
    try {
      // The engine records changes, not messages: the first revision says where the file came from.
      await t.svcSend('POST', 'notes', { document_id: id, branch: 'main', from_seq: 1, to_seq: seq, kind: 'revision', summary: `Added ${file.name}`.slice(0, 200), description: `Imported from ${file.name} on this computer.` });
    } catch {
      /* the revision is there without its message */
    }
    return { id, title, kind: imported.kind, seq, warnings: imported.warnings };
  } catch (e) {
    throw new Error(`${file.name} could not be added: ${text(e)} The partly added file is in the project's trash if it appears at all.`);
  }
}

/** Asks for files with the system dialog and adds each to the current project. */
export async function addFilesDialog(): Promise<void> {
  if (!state.desktop) return toast('info', 'Add files', 'Adding files needs the Dynodoc desktop app.');
  const projectId = state.ui.projectId;
  let picked: Picked[];
  try {
    picked = (await call('files_pick', { purpose: 'import' })) as Picked[];
  } catch (e) {
    return toast('error', 'The file dialog did not open', text(e));
  }
  if (!picked.length) return;
  await addPicked(projectId, picked);
}

export async function addPicked(projectId: string, picked: Picked[], onAdded?: (p: Picked, a: Added) => Promise<void> | void): Promise<number> {
  const t = currentTransport();
  state.live.saving = true;
  render();
  const problems: string[] = [];
  const warnings: string[] = [];
  let added = 0;
  for (const p of picked) {
    try {
      const a = await addFile(t, projectId, p.token);
      added += 1;
      for (const w of a.warnings) if (!warnings.includes(w)) warnings.push(w);
      await onAdded?.(p, a);
    } catch (e) {
      problems.push(text(e));
    }
  }
  state.live.saving = false;
  await refresh(true);
  if (added) toast('success', `Added ${added} file${added === 1 ? '' : 's'}`, warnings[0] ?? 'Each file keeps its history from here on.');
  if (problems.length) toast('error', 'Some files were not added', problems.join(' '));
  return added;
}

/** Writes a copy of a file of the project and (optionally) opens it in Word, Excel or PowerPoint. */
export async function exportFile(docId: string, open: boolean): Promise<string | null> {
  const d = state.db.docs.find((x) => x.id === docId);
  const project = state.db.projects.find((p) => p.id === d?.projectId);
  if (!d || !project) return null;
  try {
    const t = currentTransport();
    const detail = body(await t.apiGet(`documents/${docId}`), 'read the file');
    const seq: number = detail.latest_seq ?? 0;
    const bytes = await office().exportFile(d.kind, detail.state, detail.title ?? d.title, { documentId: docId, seq });
    const safe = (detail.title ?? d.title).replace(/[\\/:*?"<>|]+/g, ' ').trim() || 'Document';
    const out = (await call('file_export', {
      projectId: project.id,
      projectName: project.name,
      name: `${safe} (v${seq})`,
      ext: EXT[d.kind] ?? 'docx',
      base64: toBase64(bytes),
      open,
    })) as { path: string; opened: boolean };
    toast('success', open && out.opened ? `Opened in ${EDITOR[d.kind]}` : 'Copy saved', out.path);
    return out.path;
  } catch (e) {
    toast('error', 'The copy could not be written', text(e));
    return null;
  }
}

// ---- the folder watch --------------------------------------------------------------------------

let watching = false;
let busy = false;

/** Polls the app for files saved in bound folders (the app looks every two seconds). */
export function startWatch(): void {
  if (watching || !state.desktop) return;
  watching = true;
  setInterval(() => {
    if (busy || state.live.env !== 'local' || state.live.mode !== 'live') return;
    void tick();
  }, 2500);
}

async function tick(): Promise<void> {
  busy = true;
  try {
    const found = (await call('watch_poll')) as WatchChange[];
    for (const c of found) await onChange(c);
  } catch {
    /* the next look tries again */
  } finally {
    busy = false;
  }
}

async function onChange(c: WatchChange): Promise<void> {
  if (c.kind === 'new') {
    state.watch.newFiles = state.watch.newFiles.filter((n) => !(n.project_id === c.project_id && n.rel_path === c.rel_path));
    state.watch.newFiles.push(c);
    render();
    return;
  }
  await pushSaved(c);
}

/** A file of a bound folder was saved: its changes become a draft of the document it belongs to. */
async function pushSaved(c: WatchChange): Promise<void> {
  const t = currentTransport();
  const label = c.name;
  try {
    const file = (await call('file_read', { token: c.token })) as { name: string; base64: string };
    const bytes = fromBase64(file.base64);
    const origin = await office().originOf(bytes);
    const docId = c.document_id ?? origin.documentId ?? null;
    const doc = docId ? state.db.docs.find((d) => d.id === docId && d.projectId === c.project_id) : undefined;
    if (!doc) {
      state.watch.newFiles.push({ ...c, kind: 'new' });
      render();
      return;
    }
    const points = body(await t.apiGet(`documents/${doc.id}/history-points`), 'read the history');
    const latest: number = points.latest_seq ?? 0;
    const baseSeq = Math.min(latest, c.base_seq ?? origin.seq ?? latest);
    const baseState = body(await t.apiGet(`documents/${doc.id}/state?through_seq=${baseSeq}`), 'read the file').state;
    const planned = await office().planPush(bytes, file.name, baseState, doc.title);
    const plan = planned.plan;
    if (!plan.ops.length) {
      await call('watch_seen', { projectId: c.project_id, relPath: c.rel_path, token: c.token, documentId: doc.id, baseSeq: latest });
      toast('info', `${label} was saved`, 'Nothing in it differs from the project, so there is nothing to review.');
      return;
    }
    const stamp = new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    const name = `Edited in ${EDITOR[doc.kind]} ${stamp}`;
    const made = body(
      await t.apiSend('POST', `documents/${doc.id}/change-requests`, {
        name,
        note: `Saved from ${file.name} in the project's folder.`,
        base_seq: baseSeq,
        ops: plan.ops,
        submit: false,
        source: { kind: 'file', filename: file.name.slice(0, 255), base_reason: 'file', anchored: plan.anchored, summary: summarise(plan) },
      }),
      'keep the changes',
    );
    await call('watch_seen', { projectId: c.project_id, relPath: c.rel_path, token: c.token, documentId: doc.id, baseSeq: latest });
    state.watch.recent.unshift({ docId: doc.id, title: doc.title, name: file.name, draftId: String(made.id), draftName: name, changes: plan.changes.length, at: Date.now() });
    state.watch.recent = state.watch.recent.slice(0, 20);
    await refresh(true);
    toast('success', `${label} was saved`, `${plan.changes.length} change${plan.changes.length === 1 ? '' : 's'} are in the draft "${name}". Review them under Changes and send them for review.`);
  } catch (e) {
    toast('error', `${label} could not be read as changes`, text(e));
  }
}

function summarise(plan: Plan): Record<string, number> {
  const s = { added: 0, edited: 0, removed: 0, moved: 0 };
  for (const ch of plan.changes) s[ch.kind] += 1;
  return s;
}

/** Adds the files a bound folder holds that the project does not have yet. */
export async function addFolderFiles(projectId: string): Promise<void> {
  const mine = state.watch.newFiles.filter((n) => n.project_id === projectId);
  if (!mine.length) return;
  state.watch.newFiles = state.watch.newFiles.filter((n) => n.project_id !== projectId);
  render();
  const byToken = new Map(mine.map((m) => [m.token, m]));
  await addPicked(
    projectId,
    mine.map((m) => ({ token: m.token, name: m.name, size: m.size, path: m.rel_path })),
    async (p, a) => {
      const m = byToken.get(p.token);
      if (!m) return;
      await call('watch_seen', { projectId, relPath: m.rel_path, token: m.token, documentId: a.id, baseSeq: a.seq });
    },
  );
}

export function ignoreFolderFiles(projectId: string): void {
  state.watch.newFiles = state.watch.newFiles.filter((n) => n.project_id !== projectId);
  render();
}

export async function chooseFolder(projectId: string): Promise<void> {
  try {
    const picked = (await call('files_pick', { purpose: 'folder' })) as Picked[];
    if (!picked.length) return;
    const where = (await call('folder_bind', { projectId, token: picked[0].token })) as string;
    toast('success', 'Folder bound', `Word, Excel and PowerPoint files saved in ${where} are picked up as changes.`);
    state.watch.folders[projectId] = where;
    render();
  } catch (e) {
    toast('error', 'The folder could not be used', text(e));
  }
}

export async function stopWatching(projectId: string): Promise<void> {
  try {
    await call('folder_unbind', { projectId });
    delete state.watch.folders[projectId];
    state.watch.newFiles = state.watch.newFiles.filter((n) => n.project_id !== projectId);
    toast('success', 'Folder released', 'Files in it are no longer watched. Nothing in it was changed.');
    render();
  } catch (e) {
    toast('error', 'The folder could not be released', text(e));
  }
}

/** Reads which folders are bound (for the project pages). */
export async function loadFolders(): Promise<void> {
  if (!state.desktop || state.live.env !== 'local') return;
  for (const p of state.db.projects) {
    try {
      const s = (await call('folder_status', { projectId: p.id })) as { path: string; exists: boolean; files: number } | null;
      if (s) state.watch.folders[p.id] = s.path;
      else delete state.watch.folders[p.id];
    } catch {
      /* leave it */
    }
  }
  render();
}
