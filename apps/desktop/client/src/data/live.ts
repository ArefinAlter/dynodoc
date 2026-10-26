// The client's live mode: the desktop app is signed in to a Dynodoc server, the screens show that
// server's workspace (remote.ts), and every change a person makes is sent to it. Changes are
// noticed after each render: the model is compared with the server's copy, the difference is sent
// (in order, one request at a time), and the server's state is read back.
import { state, render, toast, resetSample } from '../state.ts';
import { call, refreshDesktopState } from '../bridge.ts';
import type { Db } from './types.ts';
import { changed, load, loadThreads, reconcile, renameIds, RemoteError, type Ctx, type Reply, type Session, type Transport } from './remote.ts';

/** The desktop app's commands (src-tauri/src/commands.rs, remote.rs) for one place: the workspace
 *  on this computer (`local`), an account's server (`acct:<id>`), or, with no argument, the one
 *  configured server through the window's own session (0.0.1a). */
export function transportFor(env?: string | null): Transport {
  const e = env ?? null;
  return {
    apiGet: (path) => call('api_get', { path, env: e }) as Promise<Reply>,
    apiSend: (method, path, body, ifMatch) => call('api_send', { method, path, body: body ?? null, ifMatch: ifMatch ?? null, env: e }) as Promise<Reply>,
    svcGet: (path) => call('svc_get', { path, env: e }) as Promise<Reply>,
    svcSend: (method, path, body) => call('svc_send', { method, path, body: body ?? null, env: e }) as Promise<Reply>,
  };
}

export const desktopTransport: Transport = transportFor();

/** The transport of the place the screens show now. */
export function currentTransport(): Transport {
  return transport;
}

export interface SessionInfo {
  server: string | null;
  signed_in: boolean;
  identity: string | null;
}

let ctx: Ctx | null = null;
let transport: Transport = desktopTransport;
let timer: ReturnType<typeof setTimeout> | null = null;
let running = false;
let again = false;

export function isLive(): boolean {
  return state.live.mode === 'live';
}

/** The screens show the workspace on this computer. */
export function isLocal(): boolean {
  return state.live.mode === 'live' && state.live.env === 'local';
}

/** Shows another place: reads it and makes every later change go to it. */
export async function connectEnv(env: string): Promise<boolean> {
  state.live.env = env;
  return connect(transportFor(env === 'server' ? null : env));
}

/** Reads the server into the client. Keeps the current project when it is still there. */
export async function connect(t: Transport = desktopTransport): Promise<boolean> {
  transport = t;
  state.live.mode = 'connecting';
  state.live.error = '';
  render();
  try {
    const session = (await t.svcGet('session').then((r) => (r.status === 200 ? r.body : null)).catch(() => null)) as { identity_id?: string; visitor?: Session['visitor'] } | null;
    const threads = await loadThreads(t).catch(() => null);
    const out = await load(t, { identity: session?.identity_id ?? null, visitor: session?.visitor ?? null });
    adopt(out.db, out.ctx);
    if (Array.isArray(threads)) {
      state.ui.agent.threads = threads as typeof state.ui.agent.threads;
      ctx!.saved.threads = JSON.stringify(threads);
    } else ctx!.saved.threads = JSON.stringify(state.ui.agent.threads.length ? state.ui.agent.threads : null);
    state.live.mode = 'live';
    state.live.server = state.live.server || '';
    render();
    return true;
  } catch (e) {
    ctx = null;
    state.live.mode = 'offline';
    state.live.error = e instanceof Error ? e.message : String(e);
    render();
    return false;
  }
}

function adopt(db: Db, next: Ctx): void {
  // Picks a person made for conflicts survive reading the server again.
  const picks = new Map<string, 'team' | 'draft'>();
  for (const r of state.db.requests) for (const c of r.conflicts) if (c.choice) picks.set(`${r.id}:${c.blockId}`, c.choice);
  for (const r of db.requests) for (const c of r.conflicts) c.choice = picks.get(`${r.id}:${c.blockId}`) ?? c.choice;
  ctx = next;
  state.db = db;
  state.ui.verified = {};
  if (!db.projects.some((p) => p.id === state.ui.projectId)) {
    const files = (id: string) => db.docs.filter((d) => d.projectId === id).length;
    state.ui.projectId = [...db.projects].sort((a, b) => files(b.id) - files(a.id))[0]?.id ?? '';
  }
  // Revisions are made as the signed-in person: the commit box shows their name.
  const me = db.people.find((p) => p.id === db.me);
  if (me) {
    state.prefs.authorName = me.name;
    if (me.email) state.prefs.authorEmail = me.email;
  }
  for (const p of db.projects) {
    const b = state.ui.branchByProject[p.id];
    if (!b || (b !== 'main' && !db.drafts.some((d) => d.id === b))) state.ui.branchByProject[p.id] = 'main';
  }
  const ui = state.ui;
  if (ui.requests.selected && !db.requests.some((r) => r.id === ui.requests.selected)) ui.requests.selected = null;
  if (ui.history.selected && !db.commits.some((c) => c.id === ui.history.selected)) ui.history.selected = null;
  if (ui.documents.selected && !db.docs.some((d) => d.id === ui.documents.selected)) ui.documents.selected = null;
  if (ui.projects.selected && !db.projects.some((p) => p.id === ui.projects.selected)) ui.projects.selected = null;
}

/** Leaves live mode and goes back to the sample (after signing out). */
export function disconnect(): void {
  ctx = null;
  state.live.mode = 'offline';
  state.live.visitor = null;
  resetSample();
}

/** Read the server again (Fetch). */
export async function refresh(quiet = false): Promise<void> {
  if (!isLive()) return;
  if (running) {
    again = true;
    return;
  }
  running = true;
  state.ui.fetching = true;
  render();
  try {
    const session = { identity: state.db.me, visitor: state.db.remote?.visitor ?? null };
    const out = await load(transport, session);
    const saved = ctx?.saved;
    adopt(out.db, out.ctx);
    if (saved) ctx!.saved.threads = saved.threads;
    if (!quiet) toast('success', 'Up to date', 'Read the latest from the server.');
  } catch (e) {
    fail(e);
  } finally {
    running = false;
    state.ui.fetching = false;
    render();
  }
}

function fail(e: unknown): void {
  const message = e instanceof Error ? e.message : String(e);
  if (e instanceof RemoteError && e.status === 401) {
    toast('error', 'Signed out', message);
    disconnect();
    return;
  }
  toast('error', state.live.env === 'local' ? 'This workspace did not accept this' : 'The server did not accept this', message);
}

/** Called after every render: if the model differs from the server's copy, send the change. */
export function noticeChanges(): void {
  if (!isLive() || !ctx) return;
  if (timer) clearTimeout(timer);
  timer = setTimeout(() => {
    timer = null;
    void sync();
  }, 350);
}

async function sync(): Promise<void> {
  if (!ctx || !isLive()) return;
  if (running) {
    again = true;
    return;
  }
  const threads = state.ui.agent.threads.length ? state.ui.agent.threads : null;
  if (!changed(ctx, state.db, threads)) return;
  running = true;
  state.live.saving = true;
  render();
  const sent = JSON.stringify(state.db);
  try {
    const outcome = await reconcile(transport, ctx, JSON.parse(sent) as Db, JSON.parse(JSON.stringify(threads)));
    // Selections and branches that named something by its local id now use the server's.
    state.ui = renameIds(state.ui, outcome.ids);
    const session = { identity: state.db.me, visitor: state.db.remote?.visitor ?? null };
    const out = await load(transport, session);
    const saved = ctx.saved;
    if (JSON.stringify(state.db) === sent) {
      adopt(out.db, out.ctx);
      ctx.saved = { ...ctx.saved, threads: saved.threads };
    } else {
      // The person changed more while this was being sent: keep their newer changes on screen,
      // count what was just sent as the server's, and send the rest next.
      state.db = renameIds(state.db, outcome.ids);
      out.ctx.base = renameIds(JSON.parse(sent) as Db, outcome.ids);
      out.ctx.saved = saved;
      ctx = out.ctx;
      again = true;
    }
    if (outcome.errors.length) toast('error', state.live.env === 'local' ? 'This workspace did not accept a change' : 'The server did not accept a change', outcome.errors.join(' '));
    else if (outcome.done.length) toast('success', state.live.env === 'local' ? 'Saved on this computer' : 'Saved to the server', outcome.done.slice(0, 3).join(' · '));
  } catch (e) {
    fail(e);
    // Show the server's state rather than a change it did not take.
    try {
      const out = await load(transport, { identity: state.db.me, visitor: state.db.remote?.visitor ?? null });
      adopt(out.db, out.ctx);
    } catch {
      /* the error above stands */
    }
  } finally {
    running = false;
    state.live.saving = false;
    render();
    if (again) {
      again = false;
      noticeChanges();
    }
  }
}

/** Starts a sample workspace on the server (the app's visitor_start command), then reads it. */
export async function startVisitor(): Promise<void> {
  state.live.mode = 'connecting';
  render();
  try {
    const v = (await call('visitor_start')) as { display_name: string; expires_at: string };
    state.live.visitor = v;
    const ds = await refreshDesktopState();
    state.live.server = ds?.server ?? state.live.server;
    state.live.env = 'server';
    const ok = await connect(desktopTransport);
    if (ok) toast('success', 'Your sample workspace is ready', `It is yours alone and is removed ${v.expires_at ? `on ${new Date(v.expires_at).toLocaleString()}` : 'after 24 hours'}.`);
  } catch (e) {
    state.live.mode = 'offline';
    state.live.error = e instanceof Error ? e.message : String(e);
    toast('error', 'No sample workspace', state.live.error);
    render();
  }
}

export async function signOut(): Promise<void> {
  try {
    await call('sign_out');
  } catch (e) {
    toast('error', 'Sign-out did not complete', e instanceof Error ? e.message : String(e));
  }
  disconnect();
  toast('success', 'Signed out', 'This window now shows the offline sample.');
}

/** At start-up in the desktop app: live when this window holds a session for its server. */
export async function startLive(): Promise<SessionInfo | null> {
  try {
    const info = (await call('session_info')) as SessionInfo;
    state.live.server = info.server ?? '';
    if (info.signed_in) {
      state.live.env = 'server';
      await connect(desktopTransport);
    }
    return info;
  } catch (e) {
    state.live.error = e instanceof Error ? e.message : String(e);
    return null;
  }
}

// ---- preferences: a file in the app's config folder (prefs_load / prefs_save) -------------------------

let savedPrefs = '';
let prefsTimer: ReturnType<typeof setTimeout> | null = null;

export async function loadPrefs(): Promise<void> {
  try {
    const value = (await call('prefs_load')) as Record<string, unknown> | null;
    if (value && typeof value === 'object') Object.assign(state.prefs, value);
    savedPrefs = JSON.stringify(state.prefs);
  } catch {
    savedPrefs = JSON.stringify(state.prefs);
  }
}

export function noticePrefs(): void {
  if (!state.desktop || !savedPrefs) return;
  const now = JSON.stringify(state.prefs);
  if (now === savedPrefs) return;
  if (prefsTimer) clearTimeout(prefsTimer);
  prefsTimer = setTimeout(() => {
    prefsTimer = null;
    const text = JSON.stringify(state.prefs);
    call('prefs_save', { prefs: JSON.parse(text) })
      .then(() => {
        savedPrefs = text;
      })
      .catch((e) => toast('error', 'Preferences not saved', e instanceof Error ? e.message : String(e)));
  }, 800);
}
