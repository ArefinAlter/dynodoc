// The client on a real Dynodoc server. `load` reads the server (through the desktop app's
// commands: the page itself has no network) into the client's model; `reconcile` turns what a
// person did in the client into the server's own requests. The views keep working on the same
// model as with the scripted backend (backend.ts); after every change this module sends it to the
// server and reads the server back, so what is on screen is what the server holds.
//
// Where things live:
//   the Dynodoc server (its engine API, through the web workspace's /api/engine proxy): projects,
//     files and their content, the hash-chained history, drafts, change requests, reviews,
//     merges, versions, discussions, published copies, people and roles, review rules, the AI
//     agent policy, issues, labels, stars, watching, notifications, signing keys;
//   the pre-alpha service (/svc/v1 on the same server, deploy/pre-alpha): what the server has no
//     place for: changes the editor add-ins captured and nobody committed yet, commits not yet
//     pushed, commit messages, agent threads, and the few choices of this client (dismissed copies).
import { applyEdits, compareBlocks, type Block, type Edit } from '../lib/diff.ts';
import type { ActivityItem, ChangeRequest, Commit, Conflict, Db, Doc, Draft, Issue, Member, Notification, Person, Project, Review, ReviewComment, Role, Space, WorkingChange } from './types.ts';
import { applyEvent, blockOfKey, blocksOf, cloneNodes, CLIENT_ROLE_TO_POLICY, describeRun, editsToOps, filenameOf, hex, nodeOf, POLICY_ROLE_TO_CLIENT, runsOf, textOf, type Nodes, type REvent, type Slot } from './remote-map.ts';

export interface Reply {
  status: number;
  body: any;
}

/** The desktop app's commands (remote.rs), or a fake in tests. */
export interface Transport {
  apiGet(path: string): Promise<Reply>;
  apiSend(method: 'POST' | 'PUT' | 'PATCH' | 'DELETE', path: string, body?: unknown, ifMatch?: string): Promise<Reply>;
  svcGet(path: string): Promise<Reply>;
  svcSend(method: 'POST' | 'PUT', path: string, body?: unknown): Promise<Reply>;
}

export class RemoteError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

function ok(r: Reply, what: string): any {
  if (r.status >= 200 && r.status < 300) return r.body;
  const text = r.body && typeof r.body === 'object' && typeof r.body.error === 'string' ? r.body.error : '';
  const plain =
    r.status === 401 ? 'Your session on the server has ended. Sign in again.' :
    r.status === 403 ? text && text !== 'forbidden' ? text : `Your role does not allow this (${what}).` :
    r.status === 409 ? text && text !== 'conflict' ? text : 'Someone changed this on the server first. The latest version is shown; try again.' :
    r.status === 429 ? 'The server asks to slow down. Try again in a moment.' :
    text || `The server could not ${what} (${r.status}).`;
  throw new RemoteError(r.status, plain);
}

const optional = async (p: Promise<Reply>, fallback: any): Promise<any> => {
  const r = await p;
  return r.status >= 200 && r.status < 300 ? r.body : fallback;
};

async function pool<T>(items: T[], n: number, fn: (x: T) => Promise<void>): Promise<void> {
  let i = 0;
  const workers = Array.from({ length: Math.min(n, items.length) }, async () => {
    while (i < items.length) {
      const x = items[i++];
      await fn(x);
    }
  });
  await Promise.all(workers);
}

const ms = (s: string | null | undefined): number => (s ? Date.parse(s) : 0) || 0;

const sleep = (n: number) => new Promise((r) => setTimeout(r, n));

/** The engine admits two "expensive" reads per person at a time (activity, issue lists, signing
 *  keys; engine-api workloads.rs) and the whole server a bounded number: this client sends those
 *  one at a time, at most eight requests at once in all, and tries a refused one again shortly. */
export function gentle(t: Transport): Transport {
  let active = 0;
  const waiting: (() => void)[] = [];
  let serial: Promise<unknown> = Promise.resolve();
  const slot = async <T>(fn: () => Promise<T>): Promise<T> => {
    while (active >= 8) await new Promise<void>((r) => waiting.push(r));
    active += 1;
    try {
      return await fn();
    } finally {
      active -= 1;
      waiting.shift()?.();
    }
  };
  const retry = async (fn: () => Promise<Reply>): Promise<Reply> => {
    let r = await slot(fn);
    for (let i = 1; i <= 4 && r.status === 429; i++) {
      await sleep(400 * i + Math.floor(Math.random() * 200));
      r = await slot(fn);
    }
    return r;
  };
  const expensive = (path: string) => /^(devices|projects\/[^/]+\/(activity|issues|issues\/\d+|contributors))(\?|$)/.test(path);
  return {
    apiGet: (path) => {
      if (!expensive(path)) return retry(() => t.apiGet(path));
      const next = serial.then(() => retry(() => t.apiGet(path)));
      serial = next.catch(() => undefined);
      return next;
    },
    apiSend: (m, p, b, i) => retry(() => t.apiSend(m, p, b, i)),
    svcGet: (p) => retry(() => t.svcGet(p)),
    svcSend: (m, p, b) => retry(() => t.svcSend(m, p, b)),
  };
}

export interface Note {
  id: number;
  document_id: string;
  branch: string;
  from_seq: number;
  to_seq: number;
  kind: string;
  summary: string;
  description: string;
  merged_from: string | null;
  author_id: string;
  edits: Edit[] | null;
  created_at: number;
}

interface DocCtx {
  id: string;
  kind: string;
  projectId: string;
  latest: number;
  nodes: Nodes;
  slots: Record<string, Slot>;
  manage: boolean;
}
interface DraftCtx {
  id: string;
  docId: string;
  revision: number;
  own: boolean;
  submitted: boolean;
  nodes: Nodes;
  slots: Record<string, Slot>;
}
interface CrCtx {
  id: string;
  docId: string;
  revision: number;
  teamSeq: number;
  keys: Record<string, string[]>;
  titleNote: boolean;
}
interface ProjectCtx {
  id: string;
  name: string;
  description: string;
  revision: number;
  policyRevision: number | null;
  emails: Record<string, string>;
}

/** What the client knows about the server's copy, to turn changes into requests. */
export interface Ctx {
  me: string;
  docs: Record<string, DocCtx>;
  drafts: Record<string, DraftCtx>;
  crs: Record<string, CrCtx>;
  projects: Record<string, ProjectCtx>;
  threads: Record<string, number>;
  issues: Record<string, number>;
  /** The model as the server last had it (what changes are computed against). */
  base: Db;
  /** The client-only state as last saved in the pre-alpha service. */
  saved: { working: string; local: string; threads: string; client: string };
  client: ClientState;
}

/** The client's own choices, kept in the pre-alpha service. */
export interface ClientState {
  copies: Record<string, string>;
  hiddenDrafts: string[];
}

export interface Session {
  identity: string | null;
  visitor: { display_name: string; expires_at: string } | null;
}

const KIND_MAP: Record<string, ActivityItem['kind']> = { edit: 'commit', file_added: 'commit', merged: 'merge', request_opened: 'request', declined: 'request', version: 'version', member_added: 'member' };

function person(people: Map<string, Person>, id: string | undefined | null, name?: string, email?: string): void {
  if (!id) return;
  const had = people.get(id);
  if (!had) people.set(id, { id, name: name || 'Someone', email: email || '' });
  else {
    if (name && had.name === 'Someone') had.name = name;
    if (email && !had.email) had.email = email;
  }
}

function draftState(row: any, request?: any): Draft['state'] {
  const status = request?.status ?? (row?.merged_at ? 'merged' : row?.review_outcome === 'declined' ? 'declined' : row?.submitted_at ? 'open' : 'private');
  if (status === 'merged') return 'merged';
  if (status === 'declined' || status === 'withdrawn') return 'withdrawn';
  if (status === 'open') return 'in_review';
  return 'private';
}

/** What does not change while a file's latest revision stays the same: its state, its events and
 *  the server's check of its chain. Earlier states never change. Kept for the window's life. */
const fileCache = new Map<string, { latest: number; doc: any; events: REvent[]; check: any }>();
const stateCache = new Map<string, Nodes>();

async function allEvents(t: Transport, docId: string, after = 0, known: REvent[] = []): Promise<REvent[]> {
  const out: REvent[] = [...known];
  let cursor = '';
  for (let page = 0; page < 50; page++) {
    const body = ok(await t.apiGet(`documents/${docId}/events?limit=1000${after ? `&from_seq=${after + 1}` : ''}${cursor ? `&cursor=${cursor}` : ''}`), 'read the history');
    out.push(...(body.items ?? []));
    if (!body.next_cursor) break;
    cursor = String(body.next_cursor);
    if (!/^[A-Za-z0-9_.:-]{1,64}$/.test(cursor)) break;
  }
  return out;
}

/** Reads the whole workspace the signed-in person can see. */
export async function load(raw0: Transport, session: Session, now = Date.now()): Promise<{ db: Db; ctx: Ctx }> {
  const t = gentle(raw0);
  const people = new Map<string, Person>();
  const [profile, projectList, spaceList, notes0, devices, notifications] = await Promise.all([
    t.apiGet('profile').then((r) => ok(r, 'read your profile')),
    t.apiGet('projects').then((r) => ok(r, 'list your projects')),
    optional(t.apiGet('spaces'), { items: [] }),
    Promise.resolve(null),
    optional(t.apiGet('devices'), { keys: [] }),
    optional(t.apiGet('notifications'), { items: [] }),
  ]);
  void notes0;
  const me: string = profile.id;
  person(people, me, profile.name);

  const ctx: Ctx = { me, docs: {}, drafts: {}, crs: {}, projects: {}, threads: {}, issues: {}, base: null as unknown as Db, saved: { working: '', local: '', threads: '', client: '' }, client: { copies: {}, hiddenDrafts: [] } };
  const projects: Project[] = [];
  const docs: Doc[] = [];
  const drafts: Draft[] = [];
  const requests: ChangeRequest[] = [];
  const commits: Commit[] = [];
  const activity: ActivityItem[] = [];
  const raw: { project: any; files: any[]; crs: any[]; ownDrafts: any[] }[] = [];

  // ---- projects ----------------------------------------------------------------------------------
  await pool<any>(projectList.items ?? [], 4, async (p) => {
    const [files, crs, labels, issues, act, policy, members] = await Promise.all([
      t.apiGet(`projects/${p.id}/files`).then((r) => ok(r, 'list the files')),
      optional(t.apiGet(`projects/${p.id}/change-requests`), { items: [] }),
      optional(t.apiGet(`projects/${p.id}/labels`), { items: [] }),
      optional(t.apiGet(`projects/${p.id}/issues?state=all&limit=100`), { items: [] }),
      optional(t.apiGet(`projects/${p.id}/activity?limit=100`), { items: [] }),
      optional(t.apiGet(`projects/${p.id}/agent-policy`), null),
      optional(t.apiGet(`spaces/${p.id}/members`), { items: [] }),
    ]);
    const emails: Record<string, string> = {};
    const mem: Member[] = [];
    for (const m of members.items ?? []) {
      person(people, m.id, m.name, m.email);
      emails[m.id] = m.email;
      if (!mem.some((x) => x.personId === m.id)) mem.push({ personId: m.id, role: m.role as Role, via: m.inherited ? 'workspace' : 'direct' });
    }
    const issueList: Issue[] = [];
    await pool<any>(issues.items ?? [], 4, async (it) => {
      const full = await optional(t.apiGet(`projects/${p.id}/issues/${it.number}`), it);
      ctx.issues[`${p.id}:${it.number}`] = full.revision ?? 0;
      person(people, it.author?.id, it.author?.name);
      if (it.assignee) person(people, it.assignee.id, it.assignee.name);
      const comments = (full.comments?.items ?? []).map((c: any) => {
        person(people, c.author?.id, c.author?.name);
        return { authorId: c.author?.id ?? '', at: ms(c.created_at), text: c.body };
      });
      issueList.push({ number: it.number, title: it.title, body: full.body ?? '', state: it.state, authorId: it.author?.id ?? '', assigneeId: it.assignee?.id, labels: (it.labels ?? []).map((l: any) => l.id), comments, createdAt: ms(it.created_at), closedAt: it.closed_at ? ms(it.closed_at) : undefined });
    });
    issueList.sort((a, b) => b.number - a.number);
    const owner = mem.find((m) => m.role === 'owner' && m.via === 'direct') ?? mem.find((m) => m.role === 'owner');
    const project: Project = {
      id: p.id,
      name: p.name,
      description: p.description ?? '',
      ownerId: owner?.personId ?? '',
      role: p.role as Role,
      private: true,
      starred: Boolean(p.starred),
      stars: p.star_count ?? 0,
      watch: p.watch === 'requests' ? 'requests' : 'ignore',
      updatedAt: Math.max(0, ...(files.items ?? []).map((f: any) => ms(f.updated_at))) || now,
      rules: { protectMain: Boolean(p.policy?.protect_team_version), requiredApprovals: p.policy?.required_approvals ?? 0, mergeRoles: p.policy?.merge_roles === 'owners' ? 'owners' : 'editors' },
      members: mem,
      issues: issueList,
      labels: (labels.items ?? []).map((l: any) => ({ id: l.id, name: l.name, color: String(l.color ?? '').replace('#', '') })),
      agentPolicy: policy
        ? { enabled: Boolean(policy.enabled), minRole: (POLICY_ROLE_TO_CLIENT[policy.min_role] ?? 'editor') as Role, callsPerMinute: policy.calls_per_minute ?? 30, openProposals: policy.open_proposals ?? 3, proposalsPerDay: policy.proposals_per_day ?? 10, submitRequiresPerson: policy.submit_requires_person !== false }
        : { enabled: false, minRole: 'editor', callsPerMinute: 30, openProposals: 3, proposalsPerDay: 10, submitRequiresPerson: true },
      space: '',
    };
    ctx.projects[p.id] = { id: p.id, name: p.name, description: p.description ?? '', revision: p.revision ?? 0, policyRevision: policy ? (policy.revision ?? 0) : null, emails };
    projects.push(project);
    for (const a of act.items ?? []) {
      person(people, a.actor_id, a.actor_name);
      const title = a.document_title ?? '';
      const summary =
        a.kind === 'edit' ? `made ${a.count ?? 1} change${(a.count ?? 1) === 1 ? '' : 's'} to ${title}` :
        a.kind === 'merged' ? `merged ${a.summary} into ${title}` :
        a.kind === 'request_opened' ? `opened the change request ${a.summary} on ${title}` :
        a.kind === 'declined' ? `declined ${a.summary} on ${title}` :
        a.kind === 'version' ? `saved the version "${a.summary}" of ${title}` :
        a.kind === 'file_added' ? `added the file ${title}` :
        a.kind === 'member_added' ? `added ${a.summary}` : String(a.summary ?? a.kind);
      activity.push({ id: `act_${p.id.slice(0, 8)}_${activity.length}`, at: ms(a.at), actorId: a.actor_id, projectId: p.id, docId: a.document_id ?? undefined, kind: KIND_MAP[a.kind] ?? 'commit', summary, byAgent: Boolean(a.by_agent) });
    }
    raw.push({ project: p, files: files.items ?? [], crs: crs.items ?? [], ownDrafts: [] });
  });

  // ---- the pre-alpha service: notes and the client's own state ----------------------------------------
  const allFiles = raw.flatMap((r) => r.files.map((f) => ({ ...f, projectId: r.project.id })));
  const notes: Note[] = [];
  for (let i = 0; i < allFiles.length; i += 40) {
    const q = allFiles.slice(i, i + 40).map((f) => `document=${f.id}`).join('&');
    const body = await optional(t.svcGet(`notes?${q}`), { items: [] });
    notes.push(...(body.items ?? []));
  }
  const [working, local, threads, client] = await Promise.all(
    (['working', 'local', 'threads', 'client'] as const).map((k) => optional(t.svcGet(`state/${k}`), { value: null })),
  );

  // ---- files ----------------------------------------------------------------------------------------
  const verify: Record<string, { ok: boolean; brokenAt?: number; detail?: string }> = {};
  await pool<any>(allFiles, 3, async (f) => {
    const cached = fileCache.get(f.id);
    const fresh = cached && cached.latest === (f.latest_seq ?? -1);
    const [doc, events, ownDrafts, versions, discussions, links, connectors, copies, check] = await Promise.all([
      fresh ? cached.doc : t.apiGet(`documents/${f.id}`).then((r) => ok(r, 'open a file')),
      fresh ? cached.events : cached && cached.latest < (f.latest_seq ?? 0) ? allEvents(t, f.id, cached.latest, cached.events) : allEvents(t, f.id),
      optional(t.apiGet(`documents/${f.id}/drafts`), { items: [] }),
      optional(t.apiGet(`documents/${f.id}/versions`), { items: [] }),
      optional(t.apiGet(`documents/${f.id}/discussions`), { items: [] }),
      optional(t.apiGet(`documents/${f.id}/public-links`), { items: [] }),
      optional(t.apiGet(`documents/${f.id}/connectors`), { items: [] }),
      optional(t.apiGet(`documents/${f.id}/copies`), { items: [] }),
      fresh ? cached.check : optional(t.apiGet(`documents/${f.id}/verify`), { ok: true }),
    ]);
    fileCache.set(f.id, { latest: doc.latest_seq ?? f.latest_seq ?? 0, doc, events, check });
    const kind = String(f.kind ?? doc.settings?.kind ?? 'document');
    const nodes: Nodes = doc.state?.nodes ?? {};
    const mapped = blocksOf(kind, nodes);
    verify[f.id] = { ok: Boolean(check.ok), brokenAt: check.broken_at_seq ?? undefined, detail: check.detail ?? undefined };
    ctx.docs[f.id] = { id: f.id, kind, projectId: f.projectId, latest: doc.latest_seq ?? f.latest_seq ?? 0, nodes: cloneNodes(nodes), slots: mapped.slots, manage: f.role === 'owner' || f.role === 'manager' };
    const blockForNode = (node: string | null | undefined) => (node ? (mapped.slots[node] ? node : mapped.blocks.find((b) => nodeOf(b.id) === node)?.id ?? node) : '');
    const d: Doc = {
      id: f.id,
      projectId: f.projectId,
      title: f.title,
      filename: filenameOf(f.title, kind),
      kind: (['document', 'spreadsheet', 'presentation'].includes(kind) ? kind : 'document') as Doc['kind'],
      blocks: mapped.blocks,
      versions: (versions.items ?? []).map((v: any) => ({ id: v.id, name: v.name, note: v.note ?? '', throughSeq: v.through_seq, at: ms(v.created_at), authorId: v.created_by })),
      connections: (connectors.items ?? []).map((c: any) => ({ id: String(c.id ?? c.grant ?? c.grant_id), host: (c.host ?? c.kind ?? 'word') as Doc['connections'][number]['host'], createdAt: ms(c.created_at), expiresAt: ms(c.expires_at) || ms(c.created_at) + 7 * 86_400_000, revoked: Boolean(c.revoked_at), lastUsed: c.last_used_at ? ms(c.last_used_at) : undefined })),
      createdAt: ms(doc.created_at),
      discussions: (discussions.items ?? []).map((th: any) => {
        ctx.threads[th.id] = th.revision ?? 0;
        person(people, th.actor_id, th.author);
        return {
          id: th.id,
          blockId: blockForNode(th.node_id),
          authorId: th.actor_id,
          text: th.body,
          at: ms(th.created_at),
          resolved: Boolean(th.resolved),
          replies: (th.replies ?? []).map((rp: any) => {
            person(people, rp.actor_id, rp.author);
            return { authorId: rp.actor_id, text: rp.body, at: ms(rp.created_at) };
          }),
        };
      }),
      copies: (copies.items ?? []).map((c: any) => ({ id: String(c.id ?? c.match_id), title: c.title ?? c.document?.title ?? 'A similar file', relation: c.relation ?? 'copy', score: Number(c.similarity ?? c.score ?? 0), status: c.status ?? 'open' })),
      publicLinks: (links.items ?? []).map((l: any) => ({ id: l.id, createdAt: ms(l.created_at), expiresAt: l.expires_at ? ms(l.expires_at) : undefined, revoked: Boolean(l.revoked_at) })),
    };
    docs.push(d);

    // History: the events, grouped into revisions, with the commit messages the service holds.
    const docNotes = notes.filter((n) => n.document_id === f.id);
    const mainNotes = docNotes.filter((n) => n.branch === 'main' && n.kind !== 'request');
    const runs = runsOf(events, mainNotes.map((n) => ({ from: n.from_seq, to: n.to_seq })));
    const live: Nodes = {};
    let before = blocksOf(kind, live).blocks;
    for (const run of runs) {
      for (const e of run.events) applyEvent(live, e.payload);
      const after = blocksOf(kind, live);
      const edits = compareBlocks(before, after.blocks);
      const note = mainNotes.find((n) => n.from_seq === run.from && n.to_seq === run.to);
      const labels: Record<string, string> = {};
      for (const b of [...before, ...after.blocks]) labels[b.id] = b.label;
      person(people, run.actor);
      const first = run.events[0];
      const last = run.events[run.events.length - 1];
      commits.push({
        id: `ev_${first.id}`,
        docId: f.id,
        branch: 'main',
        seq: run.to,
        summary: note?.summary ?? describeRun(run, edits, labels),
        description: note?.description ?? '',
        authorId: run.actor,
        at: run.at,
        edits,
        kind: (note?.kind as Commit['kind']) ?? 'revision',
        mergedFrom: note?.merged_from ?? undefined,
        contentHash: hex(last.content_hash),
        prevChainHash: hex(first.prev_chain_hash),
        chainHash: hex(last.chain_hash),
        local: false,
      });
      before = after.blocks;
    }
    for (const row of ownDrafts.items ?? []) raw.find((r) => r.project.id === f.projectId)!.ownDrafts.push({ ...row, docId: f.id });
  });

  // ---- drafts and change requests -------------------------------------------------------------------
  const stateAt = new Map<string, Promise<Nodes>>();
  const nodesAt = (docId: string, seq: number): Promise<Nodes> => {
    const k = `${docId}@${seq}`;
    const known = stateCache.get(k);
    if (known) return Promise.resolve(cloneNodes(known));
    if (!stateAt.has(k))
      stateAt.set(k, t.apiGet(`documents/${docId}/state?through_seq=${seq}`).then((r) => {
        const nodes = ok(r, 'read an earlier revision').state?.nodes ?? {};
        stateCache.set(k, nodes);
        return cloneNodes(nodes);
      }));
    return stateAt.get(k)!.then(cloneNodes);
  };
  for (const r of raw) {
    const p = r.project;
    // Own drafts first (they carry their own state), then the requests of others.
    await pool<any>(r.ownDrafts, 3, async (row) => {
      const dc = ctx.docs[row.docId];
      const detail = await optional(t.apiGet(`documents/${row.docId}/drafts/${row.id}`), null);
      const nodes: Nodes = detail?.state?.nodes ?? dc.nodes;
      const mapped = blocksOf(dc.kind, nodes);
      ctx.drafts[row.id] = { id: row.id, docId: row.docId, revision: detail?.draft?.revision ?? row.revision ?? 0, own: true, submitted: Boolean(row.submitted_at), nodes, slots: mapped.slots };
      drafts.push({ id: row.id, docId: row.docId, projectId: p.id, name: row.name, authorId: row.created_by ?? me, baseSeq: row.base_seq ?? 0, createdAt: ms(row.created_at), state: draftState(row), blocks: mapped.blocks });
    });
    const sortedCrs = [...r.crs].sort((a, b) => ms(a.created_at) - ms(b.created_at));
    await pool<any>(sortedCrs, 3, async (cr) => {
      const docId = cr.document?.id;
      const dc = ctx.docs[docId];
      if (!dc) return;
      const detail = await optional(t.apiGet(`documents/${docId}/change-requests/${cr.id}`), { request: cr });
      person(people, cr.author?.id, cr.author?.name, cr.author?.email);
      const open = cr.status === 'open';
      const baseNodes = open ? await nodesAt(docId, cr.base_seq ?? 0).catch(() => dc.nodes) : dc.nodes;
      const base = blocksOf(dc.kind, baseNodes);
      let draftNodes: Nodes = cloneNodes(baseNodes);
      if (open && detail.nodes) {
        for (const [id, v] of Object.entries<any>(detail.nodes)) {
          if (v?.draft) draftNodes[id] = { ...v.draft, current_fields: v.draft.current_fields ?? {} };
          else if (v && v.draft === null) delete draftNodes[id];
        }
      } else draftNodes = cloneNodes(dc.nodes);
      const draftMapped = blocksOf(dc.kind, draftNodes);
      const keys: Record<string, string[]> = {};
      const conflicts: Conflict[] = [];
      for (const c of detail.conflicts ?? []) {
        const blockId = blockOfKey(c.key, { ...base.slots, ...draftMapped.slots });
        (keys[blockId] ??= []).push(c.key);
        if (conflicts.some((x) => x.blockId === blockId)) continue;
        const label = [...draftMapped.blocks, ...base.blocks].find((b) => b.id === blockId)?.label ?? blockId;
        const val = (v: unknown) => textOf(v);
        conflicts.push({ blockId, label, ancestor: val(c.ancestor), team: val(c.team), draft: val(c.draft) });
      }
      const titleNote = notes.filter((n) => n.branch === cr.id && n.kind === 'request').pop();
      const mergeCommit = commits.find((c) => c.mergedFrom === cr.id);
      const edits = open ? compareBlocks(base.blocks, draftMapped.blocks) : titleNote?.edits ?? mergeCommit?.edits ?? [];
      ctx.crs[cr.id] = { id: cr.id, docId, revision: detail.revision ?? cr.revision ?? 0, teamSeq: detail.team_seq ?? dc.latest, keys, titleNote: Boolean(titleNote?.edits?.length) };
      const reviews: Review[] = [];
      const comments: ReviewComment[] = [];
      for (const rv of detail.review?.reviews ?? []) {
        person(people, rv.reviewer?.id, rv.reviewer?.name, rv.reviewer?.email);
        reviews.push({ reviewerId: rv.reviewer?.id, verdict: rv.verdict, note: rv.note ?? '', at: ms(rv.created_at), stale: Boolean(rv.stale) });
        if (rv.note) comments.push({ id: rv.id, authorId: rv.reviewer?.id, blockId: rv.node_id ? draftMapped.blocks.find((b) => nodeOf(b.id) === rv.node_id)?.id ?? rv.node_id : undefined, text: rv.note, at: ms(rv.created_at) });
      }
      const number = sortedCrs.indexOf(cr) + 1;
      requests.push({
        id: cr.id,
        number,
        projectId: p.id,
        docId,
        draftId: cr.id,
        name: titleNote?.summary ?? cr.name,
        note: cr.note ?? '',
        authorId: cr.author?.id ?? '',
        status: cr.status,
        createdAt: ms(cr.submitted_at ?? cr.created_at),
        mergedAt: cr.merged_at ? ms(cr.merged_at) : undefined,
        baseSeq: cr.base_seq ?? 0,
        edits,
        excluded: [],
        conflicts,
        reviews,
        comments,
        byAgent: Boolean(cr.agent),
        agentStatement: cr.agent ? String(cr.agent.statement ?? cr.agent.note ?? '') : undefined,
      });
      const own = drafts.find((x) => x.id === cr.id);
      if (own) {
        own.state = draftState(null, cr);
        own.name = cr.name;
      } else {
        drafts.push({ id: cr.id, docId, projectId: p.id, name: cr.name, authorId: cr.author?.id ?? '', baseSeq: cr.base_seq ?? 0, createdAt: ms(cr.created_at), state: draftState(null, cr), blocks: draftMapped.blocks });
        ctx.drafts[cr.id] = { id: cr.id, docId, revision: detail.revision ?? cr.revision ?? 0, own: cr.author?.id === me, submitted: true, nodes: draftNodes, slots: draftMapped.slots };
      }
    });
  }

  // Draft history: the commit messages recorded for each draft's revisions.
  for (const d of drafts) {
    const dn = notes.filter((n) => n.document_id === d.docId && n.branch === d.id && n.kind !== 'request').sort((a, b) => a.to_seq - b.to_seq);
    const base = await (async () => {
      const dc = ctx.docs[d.docId];
      if (d.state !== 'private' && d.state !== 'in_review') return blocksOf(dc.kind, dc.nodes).blocks;
      try {
        return blocksOf(dc.kind, await nodesAt(d.docId, d.baseSeq)).blocks;
      } catch {
        return blocksOf(dc.kind, dc.nodes).blocks;
      }
    })();
    const all = d.state === 'private' || d.state === 'in_review' ? compareBlocks(base, d.blocks) : [];
    const list = dn.length ? dn : [{ id: 0, summary: `Changes in ${d.name}`, description: '', author_id: d.authorId, created_at: d.createdAt, to_seq: 1, edits: null, kind: 'revision' } as unknown as Note];
    list.forEach((n, i) => {
      commits.push({
        id: `dn_${d.id}_${n.id}`,
        docId: d.docId,
        branch: d.id,
        seq: i + 1,
        summary: n.summary,
        description: n.description ?? '',
        authorId: n.author_id,
        at: Number(n.created_at) || d.createdAt,
        edits: n.edits ?? (i === list.length - 1 ? all : []),
        kind: (n.kind as Commit['kind']) ?? 'revision',
        contentHash: '',
        prevChainHash: '',
        chainHash: '',
        local: false,
      });
    });
  }

  // ---- the client's own state ----------------------------------------------------------------------------
  const knownDocs = new Set(docs.map((d) => d.id));
  const knownBranches = new Set(['main', ...drafts.map((d) => d.id)]);
  const workingList: WorkingChange[] = (Array.isArray(working.value) ? working.value : []).filter((w: WorkingChange) => knownDocs.has(w.docId) && knownBranches.has(w.branch));
  const localList: Commit[] = (Array.isArray(local.value) ? local.value : []).filter((c: Commit) => knownDocs.has(c.docId) && knownBranches.has(c.branch)).map((c: Commit) => ({ ...c, local: true }));
  for (const c of localList.sort((a, b) => a.at - b.at)) {
    if (c.branch === 'main') {
      const d = docs.find((x) => x.id === c.docId)!;
      d.blocks = applyEdits(d.blocks, c.edits);
      const top = commits.filter((x) => x.docId === c.docId && x.branch === 'main').reduce((m, x) => Math.max(m, x.seq), 0);
      c.seq = top + 1;
    } else {
      const d = drafts.find((x) => x.id === c.branch)!;
      d.blocks = applyEdits(d.blocks, c.edits);
      c.seq = commits.filter((x) => x.branch === c.branch).length + 1;
    }
    commits.push(c);
  }
  ctx.client = { copies: {}, hiddenDrafts: [], ...(client.value && typeof client.value === 'object' ? client.value : {}) };
  for (const d of docs) for (const c of d.copies) if (ctx.client.copies[c.id]) c.status = ctx.client.copies[c.id] as Doc['copies'][number]['status'];

  // ---- spaces, notifications, keys ---------------------------------------------------------------------
  const spaces: Space[] = [];
  for (const s of (spaceList.items ?? []).filter((x: any) => x.kind === 'organization' || x.kind === 'team')) {
    const members = await optional(t.apiGet(`spaces/${s.id}/members`), { items: [] });
    for (const m of members.items ?? []) person(people, m.id, m.name, m.email);
    spaces.push({ id: s.id, name: s.name, kind: s.kind, parentId: s.parent_id ?? undefined, members: (members.items ?? []).filter((m: any) => !m.inherited).map((m: any) => ({ personId: m.id, role: m.role, via: 'direct' })) });
  }
  const notif: Notification[] = (notifications.items ?? []).map((n: any) => {
    person(people, n.actor?.id, n.actor?.name, n.actor?.email);
    if (n.actor?.is_agent && people.get(n.actor.id)) people.get(n.actor.id)!.agent = true;
    const name = n.request ? requests.find((r) => r.id === n.request.id)?.name ?? n.request.name : '';
    const title = n.document?.title ?? 'a file';
    const text =
      n.kind === 'change_request_submitted' ? `opened ${name} on ${title}` :
      n.kind === 'change_request_reviewed' ? `reviewed ${name} on ${title}` :
      n.kind === 'change_request_merged' ? `merged ${name} into ${title}` :
      n.kind === 'change_request_declined' ? `declined ${name} on ${title}` :
      n.kind === 'copy_detected' ? `found a copy of ${title}` :
      n.kind === 'access_granted' ? `gave you access to ${title}` :
      n.kind === 'mention' ? `mentioned you in ${title}` : `${String(n.kind).replace(/_/g, ' ')} ${title}`;
    return { id: n.id, kind: n.kind, at: ms(n.created_at), read: Boolean(n.read), actorId: n.actor?.id ?? '', projectId: n.document?.project_id ?? projects[0]?.id ?? '', docId: n.document?.id, requestId: n.request?.id, text };
  });
  const deviceKeys = (devices.keys ?? []).map((k: any) => ({ id: k.id, name: k.label || 'Signing key', kind: k.class === 'agent' ? 'agent' : 'person', publicKey: k.public_key, createdAt: ms(k.registered_at), revoked: Boolean(k.revoked_at), scopes: [...new Set<string>((k.grants ?? []).flatMap((g: any) => g.scopes ?? []))] })) as Db['deviceKeys'];

  projects.sort((a, b) => b.updatedAt - a.updatedAt);
  docs.sort((a, b) => a.title.localeCompare(b.title));
  activity.sort((a, b) => b.at - a.at);
  const db: Db = {
    me,
    people: [...people.values()],
    spaces,
    projects,
    docs,
    commits,
    drafts: drafts.filter((d) => !ctx.client.hiddenDrafts.includes(d.id)),
    requests,
    working: workingList,
    notifications: notif,
    activity,
    deviceKeys,
    lastFetched: now,
    incoming: 0,
    remote: { verify, visitor: session.visitor, identity: me },
  };
  ctx.saved = { working: JSON.stringify(workingList), local: JSON.stringify(localList), threads: JSON.stringify(threads.value ?? null), client: JSON.stringify(ctx.client) };
  ctx.base = JSON.parse(JSON.stringify(db));
  return { db, ctx };
}

/** Agent threads kept for this account by the pre-alpha service (null: none yet). */
export async function loadThreads(t: Transport): Promise<unknown> {
  return (await optional(t.svcGet('state/threads'), { value: null })).value;
}

// ---- from what changed to the server's requests ------------------------------------------------------

export interface Outcome {
  /** Local ids that now have server ids. */
  ids: Record<string, string>;
  /** What was sent, in the person's words (for a toast). */
  done: string[];
  /** Requests the server refused: the client shows the server's state afterwards. */
  errors: string[];
}

function newUlid(): string {
  const A = '0123456789ABCDEFGHJKMNPQRSTVWXYZ';
  let t = Date.now();
  let time = '';
  for (let i = 0; i < 10; i++) {
    time = A[t % 32] + time;
    t = Math.floor(t / 32);
  }
  const bytes = new Uint8Array(16);
  globalThis.crypto.getRandomValues(bytes);
  let rand = '';
  for (let i = 0; i < 16; i++) rand += A[bytes[i] % 32];
  return time + rand;
}

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b);
const isLocalId = (id: string) => !/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(id);

/** The client's model without what only this client holds (the parts the server never sees). */
function serverView(db: Db): unknown {
  return { ...db, working: [], commits: db.commits.filter((c) => !c.local), lastFetched: 0, incoming: 0 };
}

/** True when `next` differs from the server's copy in something the server or the service keeps. */
export function changed(ctx: Ctx, next: Db, threads: unknown): boolean {
  return (
    !same(serverView(ctx.base), serverView(next)) ||
    JSON.stringify(next.working) !== ctx.saved.working ||
    JSON.stringify(next.commits.filter((c) => c.local)) !== ctx.saved.local ||
    JSON.stringify(threads ?? null) !== ctx.saved.threads
  );
}

/**
 * Sends what changed between the server's copy (`ctx.base`) and `next` to the server, in an
 * order that keeps references valid (a draft before its commits, a request after its draft).
 * Each request stands alone; a refusal is collected and the rest go on.
 */
export async function reconcile(raw0: Transport, ctx: Ctx, next: Db, threads: unknown): Promise<Outcome> {
  const t = gentle(raw0);
  const prev = ctx.base;
  const out: Outcome = { ids: {}, done: [], errors: [] };
  const run = async (what: string, fn: () => Promise<void>) => {
    try {
      await fn();
    } catch (e) {
      out.errors.push(e instanceof Error ? e.message : String(e));
      void what;
    }
  };
  const idOf = (id: string) => out.ids[id] ?? id;
  const pDoc = (id: string) => prev.docs.find((d) => d.id === id);
  const note = (n: Record<string, unknown>) => t.svcSend('POST', 'notes', n).then((r) => ok(r, 'record the commit message'));

  // Projects.
  for (const p of next.projects) {
    const before = prev.projects.find((x) => x.id === p.id);
    if (!before) {
      await run('create a project', async () => {
        const body = ok(await t.apiSend('POST', 'projects', { name: p.name, description: p.description }), 'create the project');
        out.ids[p.id] = body.id;
        out.done.push(`Created the project ${p.name}`);
      });
      continue;
    }
    const pc = ctx.projects[p.id];
    if (p.starred !== before.starred) await run('star', async () => void ok(await t.apiSend(p.starred ? 'PUT' : 'DELETE', `projects/${p.id}/star`), 'star the project'));
    if (p.watch !== before.watch) await run('watch', async () => void ok(await t.apiSend('POST', `projects/${p.id}/watch`, { level: p.watch }), 'change watching'));
    if (!same(p.rules, before.rules) || p.name !== before.name || p.description !== before.description)
      await run('rules', async () => {
        ok(await t.apiSend('POST', `projects/${p.id}/settings`, { name: p.name, description: p.description, revision: pc.revision, protect_team_version: p.rules.protectMain, required_approvals: p.rules.requiredApprovals, merge_roles: p.rules.mergeRoles }), 'save the review rules');
        out.done.push('Saved the project settings');
      });
    if (!same(p.agentPolicy, before.agentPolicy))
      await run('agents', async () => {
        const a = p.agentPolicy;
        ok(await t.apiSend('PUT', `projects/${p.id}/agent-policy`, { enabled: a.enabled, min_role: CLIENT_ROLE_TO_POLICY[a.minRole] ?? 'editor', calls_per_minute: a.callsPerMinute, open_proposals: a.openProposals, proposals_per_day: a.proposalsPerDay, submit_requires_person: a.submitRequiresPerson }, `"${pc.policyRevision ?? 0}"`), 'save the AI agent policy');
        out.done.push('Saved the AI agent policy');
      });
    for (const m of p.members) {
      const was = before.members.find((x) => x.personId === m.personId);
      const email = next.people.find((x) => x.id === m.personId)?.email ?? pc.emails[m.personId];
      if ((!was || was.role !== m.role) && email)
        await run('member', async () => {
          ok(await t.apiSend('POST', `spaces/${p.id}/members`, { email, role: m.role }), 'change access');
          out.done.push(was ? `Changed a role in ${p.name}` : `Gave ${email} access to ${p.name}`);
        });
    }
    for (const m of before.members)
      if (!p.members.some((x) => x.personId === m.personId) && pc.emails[m.personId])
        await run('remove', async () => void ok(await t.apiSend('POST', `spaces/${p.id}/members`, { email: pc.emails[m.personId], role: 'remove' }), 'remove access'));
    for (const it of p.issues) {
      const was = before.issues.find((x) => x.number === it.number);
      if (!was) {
        await run('issue', async () => {
          ok(await t.apiSend('POST', `projects/${p.id}/issues`, { title: it.title, body: it.body, labels: it.labels.filter((l) => !isLocalId(l)) }), 'open the issue');
          out.done.push(`Opened the issue ${it.title}`);
        });
        continue;
      }
      const patch: Record<string, unknown> = {};
      if (it.state !== was.state) patch.state = it.state;
      if ((it.assigneeId ?? null) !== (was.assigneeId ?? null)) patch.assignee_id = it.assigneeId ?? null;
      if (!same(it.labels, was.labels)) patch.labels = it.labels;
      if (it.title !== was.title || it.body !== was.body) Object.assign(patch, { title: it.title, body: it.body, revision: ctx.issues[`${p.id}:${it.number}`] ?? 0 });
      if (Object.keys(patch).length) await run('issue', async () => void ok(await t.apiSend('PATCH', `projects/${p.id}/issues/${it.number}`, patch), 'change the issue'));
      for (const c of it.comments.slice(was.comments.length)) await run('comment', async () => void ok(await t.apiSend('POST', `projects/${p.id}/issues/${it.number}/comments`, { body: c.text }), 'comment on the issue'));
    }
    for (const l of p.labels) if (!before.labels.some((x) => x.id === l.id)) await run('label', async () => void ok(await t.apiSend('POST', `projects/${p.id}/labels`, { name: l.name, color: l.color.replace('#', '') }), 'add the label'));
    for (const l of before.labels) if (!p.labels.some((x) => x.id === l.id)) await run('label', async () => void ok(await t.apiSend('DELETE', `projects/${p.id}/labels/${l.id}`), 'remove the label'));
  }

  // Files: versions, discussions, published copies, connection keys, copies.
  for (const d of next.docs) {
    const before = pDoc(d.id);
    if (!before) continue;
    for (const v of d.versions) if (!before.versions.some((x) => x.id === v.id)) await run('version', async () => {
      ok(await t.apiSend('POST', `documents/${d.id}/versions`, { name: v.name, note: v.note }), 'save the version');
      out.done.push(`Saved the version ${v.name}`);
    });
    for (const th of d.discussions) {
      const was = before.discussions.find((x) => x.id === th.id);
      if (was && was.resolved !== th.resolved) await run('discussion', async () => void ok(await t.apiSend('POST', `documents/${d.id}/discussions/${th.id}`, { revision: ctx.threads[th.id] ?? 0, resolved: th.resolved }), 'resolve the discussion'));
      if (was) for (const rp of th.replies.slice(was.replies.length)) await run('reply', async () => void ok(await t.apiSend('POST', `documents/${d.id}/discussions/${th.id}`, { revision: ctx.threads[th.id] ?? 0, body: rp.text }), 'reply'));
    }
    for (const l of d.publicLinks) if (l.revoked && before.publicLinks.some((x) => x.id === l.id && !x.revoked)) await run('link', async () => void ok(await t.apiSend('POST', `documents/${d.id}/public-links/${l.id}/revoke`), 'revoke the published copy'));
    for (const k of d.connections) if (k.revoked && before.connections.some((x) => x.id === k.id && !x.revoked)) await run('key', async () => void ok(await t.apiSend('POST', `documents/${d.id}/connectors/${k.id}/revoke`), 'revoke the connection key'));
    for (const c of d.copies) if (before.copies.some((x) => x.id === c.id && x.status !== c.status)) ctx.client.copies[c.id] = c.status;
  }

  // Commits pushed (local before, not local now).
  // A commit made and pushed before the last save is new and not local; one made by the client's
  // own recovery or rebase belongs to its draft's handling below.
  const newDrafts = new Set(next.drafts.filter((d) => !prev.drafts.some((x) => x.id === d.id)).map((d) => d.id));
  const pushed = next.commits.filter(
    (c) =>
      !c.local &&
      (prev.commits.some((x) => x.id === c.id && x.local) ||
        (!prev.commits.some((x) => x.id === c.id) && c.kind === 'revision' && c.id.startsWith('rev_') && !newDrafts.has(c.branch))),
  );
  for (const c of pushed.sort((a, b) => a.at - b.at)) {
    const dc = ctx.docs[c.docId];
    await run('push', async () => {
      if (c.branch === 'main') {
        const ops = editsToOps(c.edits, { kind: dc.kind, nodes: dc.nodes, slots: dc.slots, newId: newUlid });
        if (!ops.length) return;
        const res = ok(await t.apiSend('POST', `documents/${c.docId}/batch`, { ops, base_seq: dc.latest }), 'push to main');
        const seqs = (res.events ?? []).map((e: any) => e.seq as number);
        for (const e of res.events ?? []) applyEvent(dc.nodes, e.payload);
        dc.slots = blocksOf(dc.kind, dc.nodes).slots;
        dc.latest = Math.max(dc.latest, ...seqs);
        if (seqs.length) await note({ document_id: c.docId, branch: 'main', from_seq: Math.min(...seqs), to_seq: Math.max(...seqs), kind: 'revision', summary: c.summary, description: c.description });
      } else {
        const branch = idOf(c.branch);
        const dr = ctx.drafts[branch];
        if (!dr) throw new Error('The draft is not on the server yet.');
        const ops = editsToOps(c.edits, { kind: dc.kind, nodes: dr.nodes, slots: dr.slots, newId: newUlid });
        if (!ops.length) return;
        const res = ok(await t.apiSend('POST', `documents/${c.docId}/drafts/${branch}`, { ops, revision: dr.revision }), 'push to the draft');
        dr.revision = res.revision;
        dr.nodes = res.state?.nodes ?? dr.nodes;
        dr.slots = blocksOf(dc.kind, dr.nodes).slots;
        await note({ document_id: c.docId, branch, from_seq: dr.revision, to_seq: dr.revision, kind: 'revision', summary: c.summary, description: c.description, edits: c.edits });
      }
    });
  }
  if (pushed.length) out.done.push(`Pushed ${pushed.length} commit${pushed.length === 1 ? '' : 's'}`);

  // Drafts: new ones (and the commits a recovery or restore wrote into them), withdrawals, rebases.
  for (const d of next.drafts) {
    const before = prev.drafts.find((x) => x.id === d.id);
    const dc = ctx.docs[d.docId];
    if (!before) {
      await run('draft', async () => {
        const body = ok(await t.apiSend('POST', `documents/${d.docId}/drafts`, { name: d.name, base_seq: dc.latest }), 'create the draft');
        out.ids[d.id] = body.id;
        let revision = 0;
        const own = next.commits.filter((c) => c.branch === d.id && !c.local).sort((a, b) => a.seq - b.seq);
        let nodes = dc.nodes;
        try {
          nodes = (ok(await t.apiGet(`documents/${d.docId}/drafts/${body.id}`), 'open the draft').state?.nodes as Nodes) ?? dc.nodes;
        } catch {
          /* the team version's nodes stand in */
        }
        let slots = blocksOf(dc.kind, nodes).slots;
        for (const c of own) {
          const ops = editsToOps(c.edits, { kind: dc.kind, nodes, slots, newId: newUlid });
          if (!ops.length) continue;
          const res = ok(await t.apiSend('POST', `documents/${d.docId}/drafts/${body.id}`, { ops, revision }), 'write to the draft');
          revision = res.revision;
          nodes = res.state?.nodes ?? nodes;
          slots = blocksOf(dc.kind, nodes).slots;
          await note({ document_id: d.docId, branch: body.id, from_seq: revision, to_seq: revision, kind: c.kind, summary: c.summary, description: c.description, edits: c.edits });
        }
        out.done.push(`Created the draft ${d.name}`);
      });
      continue;
    }
    if (d.state === 'withdrawn' && before.state !== 'withdrawn' && d.authorId === next.me) {
      const submitted = ctx.drafts[d.id]?.submitted;
      if (submitted) await run('withdraw', async () => void ok(await t.apiSend('POST', `documents/${d.docId}/drafts/${d.id}/withdraw`, { note: '' }), 'withdraw the request'));
      else if (!ctx.client.hiddenDrafts.includes(d.id)) ctx.client.hiddenDrafts.push(d.id);
      out.done.push(`Withdrew ${d.name}`);
    }
    if (d.baseSeq !== before.baseSeq && (d.state === 'private' || d.state === 'in_review'))
      await run('rebase', async () => {
        const dr = ctx.drafts[d.id];
        const preview = ok(await t.apiSend('POST', `documents/${d.docId}/drafts/${d.id}/sync`, { action: 'preview', team_seq: dc.latest, revision: dr?.revision ?? 0 }), 'compare the draft with main');
        const resolutions: Record<string, 'team' | 'draft'> = {};
        const slots = blocksOf(dc.kind, dc.nodes).slots;
        for (const c of preview.conflicts ?? []) {
          const blockId = blockOfKey(c.key, slots);
          const mine = d.blocks.find((b) => b.id === blockId)?.text;
          resolutions[c.key] = mine !== undefined && mine === textOf(c.team) ? 'team' : 'draft';
        }
        ok(await t.apiSend('POST', `documents/${d.docId}/drafts/${d.id}/sync`, { action: 'rebase', team_seq: preview.team_seq, revision: preview.revision, resolutions }), 'rebase the draft');
        out.done.push(`${d.name} now builds on the latest main`);
      });
  }

  // Change requests: opened, reviewed, commented, merged, declined.
  for (const r of next.requests) {
    const before = prev.requests.find((x) => x.id === r.id);
    const draftId = idOf(r.draftId);
    if (!before) {
      await run('request', async () => {
        ok(await t.apiSend('POST', `documents/${r.docId}/drafts/${draftId}/submit`, { note: r.note }), 'open the change request');
        await note({ document_id: r.docId, branch: draftId, from_seq: 0, to_seq: 0, kind: 'request', summary: r.name, description: '', edits: r.edits });
        out.ids[r.id] = draftId;
        out.done.push(`Opened the change request ${r.name}`);
      });
      continue;
    }
    const cc = ctx.crs[r.id];
    const newReviews = r.reviews.slice(before.reviews.length);
    for (const rv of newReviews)
      await run('review', async () => {
        const body: Record<string, unknown> = { verdict: rv.verdict, note: rv.note || (rv.verdict === 'approved' ? '' : 'Comment'), revision: cc?.revision ?? 0 };
        ok(await t.apiSend('POST', `documents/${r.docId}/change-requests/${r.id}/reviews`, body), 'review the request');
        out.done.push(rv.verdict === 'approved' ? `Approved ${r.name}` : rv.verdict === 'changes_requested' ? `Asked for changes on ${r.name}` : `Commented on ${r.name}`);
      });
    for (const c of r.comments.slice(before.comments.length)) {
      if (newReviews.some((rv) => rv.note && rv.note === c.text)) continue; // sent with its review
      if (r.status !== 'open' && before.status === 'open' && r.status === 'declined') continue; // sent with the decline
      await run('comment', async () => {
        const body: Record<string, unknown> = { verdict: 'commented', note: c.text, revision: cc?.revision ?? 0 };
        if (c.blockId) body.node_id = nodeOf(c.blockId);
        ok(await t.apiSend('POST', `documents/${r.docId}/change-requests/${r.id}/reviews`, body), 'comment on the request');
      });
    }
    if (r.status === 'merged' && before.status === 'open')
      await run('merge', async () => {
        const fresh = ok(await t.apiGet(`documents/${r.docId}/change-requests/${r.id}`), 'read the request');
        const resolutions: Record<string, 'team' | 'draft'> = {};
        for (const c of r.conflicts) for (const key of cc?.keys[c.blockId] ?? []) if (c.choice) resolutions[key] = c.choice;
        const body: Record<string, unknown> = { team_seq: fresh.team_seq, revision: fresh.revision, resolutions };
        if (r.excluded.length) body.included_nodes = [...new Set(r.edits.filter((e) => !r.excluded.includes(e.blockId)).map((e) => nodeOf(e.blockId)))];
        const res = ok(await t.apiSend('POST', `documents/${r.docId}/change-requests/${r.id}/merge`, body), 'merge the request');
        const seqs = (res.events ?? []).map((e: any) => e.seq as number);
        if (seqs.length) await note({ document_id: r.docId, branch: 'main', from_seq: Math.min(...seqs), to_seq: Math.max(...seqs), kind: 'merge', summary: `Merge #${r.number}: ${r.name}`, description: r.note, merged_from: r.id });
        out.done.push(`Merged ${r.name}`);
      });
    if (r.status === 'declined' && before.status === 'open')
      await run('decline', async () => {
        const last = r.comments.slice(before.comments.length).pop();
        ok(await t.apiSend('POST', `documents/${r.docId}/change-requests/${r.id}/decline`, { note: last?.text ?? '' }), 'decline the request');
        out.done.push(`Declined ${r.name}`);
      });
  }

  // Notifications read, signing keys revoked.
  const read = next.notifications.filter((n) => n.read && prev.notifications.some((x) => x.id === n.id && !x.read)).map((n) => n.id);
  if (read.length) await run('read', async () => void ok(await t.apiSend('POST', 'notifications/read', { ids: read }), 'mark notifications read'));
  for (const k of next.deviceKeys) if (k.revoked && prev.deviceKeys.some((x) => x.id === k.id && !x.revoked)) await run('key', async () => void ok(await t.apiSend('POST', `devices/${k.id}/revoke`), 'revoke the key'));

  // The client-only state, in the pre-alpha service.
  const rename = (v: unknown) => {
    let s = JSON.stringify(v);
    for (const [from, to] of Object.entries(out.ids)) s = s.split(JSON.stringify(from)).join(JSON.stringify(to));
    return s;
  };
  const workingNow = rename(next.working);
  const localNow = rename(next.commits.filter((c) => c.local));
  const threadsNow = JSON.stringify(threads ?? null);
  const clientNow = JSON.stringify(ctx.client);
  if (workingNow !== ctx.saved.working) await run('save', async () => void ok(await t.svcSend('PUT', 'state/working', { value: JSON.parse(workingNow) }), 'save the captured changes'));
  if (localNow !== ctx.saved.local) await run('save', async () => void ok(await t.svcSend('PUT', 'state/local', { value: JSON.parse(localNow) }), 'save the unpushed commits'));
  if (threadsNow !== ctx.saved.threads) await run('save', async () => void ok(await t.svcSend('PUT', 'state/threads', { value: threads ?? null }), 'save the agent threads'));
  if (clientNow !== ctx.saved.client) await run('save', async () => void ok(await t.svcSend('PUT', 'state/client', { value: ctx.client }), 'save your choices'));
  ctx.saved = { working: workingNow, local: localNow, threads: threadsNow, client: clientNow };
  return out;
}

/** Replaces local ids by the server's in any part of the client's state. */
export function renameIds<T>(value: T, ids: Record<string, string>): T {
  if (!Object.keys(ids).length) return value;
  let s = JSON.stringify(value);
  for (const [from, to] of Object.entries(ids)) s = s.split(JSON.stringify(from)).join(JSON.stringify(to));
  return JSON.parse(s) as T;
}

export { blocksOf };
export type { Block };
