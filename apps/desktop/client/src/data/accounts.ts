// Servers the person signs in to from the app, and the local projects linked to them (decision 020).
//
// The app does the sign-in (the system browser, a code handed back on this computer, a session kept
// in the credential store) and every request to a server (`acct:<id>` places in remote.rs); this
// module is the page's half: it shows the list, follows a sign-in while the browser is open, reads a
// server's projects, remembers which local project is linked to which, and runs a sync (data/sync.ts)
// between the two. The links are kept with the workspace on this computer (`state/links`), so they
// go with its backups.
import { state, render, toast, type ServerAccount, type ServerFlow } from '../state.ts';
import { call } from '../bridge.ts';
import { transportFor, refresh, isLocal } from './live.ts';
import * as S from './sync.ts';

const text = (e: unknown): string => (e instanceof Error ? e.message : String(e));
const sleep = (n: number) => new Promise<void>((r) => setTimeout(r, n));

const srv = () => state.servers;
const local = () => transportFor('local');
const remoteOf = (accountId: string) => transportFor(`acct:${accountId}`);

export const links = (): S.Links => srv().links as S.Links;
export const planOf = (projectId: string): S.Plan | null => (srv().plans[projectId] as S.Plan | undefined) ?? null;
export const accountOf = (id: string): ServerAccount | undefined => srv().accounts.find((a) => a.id === id);

// ---- the list -------------------------------------------------------------------------------------------------------

interface AccountList {
  cloud: string;
  accounts: ServerAccount[];
  store: string;
}

export async function loadServers(): Promise<void> {
  if (!state.desktop) return;
  try {
    const list = (await call('accounts_list')) as AccountList;
    srv().cloud = list.cloud;
    srv().accounts = list.accounts;
    srv().store = list.store;
    srv().loaded = true;
  } catch {
    /* keep what we had */
  }
  try {
    const got = await local().svcGet('state/links');
    const value = got.status === 200 ? got.body?.value : null;
    srv().links = value && typeof value === 'object' && !Array.isArray(value) ? (value as Record<string, unknown>) : {};
  } catch {
    /* the workspace is not ready yet */
  }
  render();
}

async function saveLinks(): Promise<void> {
  const r = await local().svcSend('PUT', 'state/links', { value: srv().links });
  if (r.status < 200 || r.status >= 300) throw new Error('The links could not be saved.');
}

export async function addServer(address: string): Promise<ServerAccount | null> {
  try {
    const account = (await call('account_add', { address })) as ServerAccount;
    srv().addDraft = '';
    await loadServers();
    return account;
  } catch (e) {
    toast('error', 'That server was not added', text(e));
    return null;
  }
}

export async function removeServer(id: string): Promise<void> {
  try {
    await call('account_remove', { id });
    for (const [pid, l] of Object.entries(links())) if (l.accountId === id) delete links()[pid];
    await saveLinks().catch(() => undefined);
    delete srv().projects[id];
    await loadServers();
    toast('success', 'The server was removed from this app', 'Projects that were linked to it stay here as they are.');
  } catch (e) {
    toast('error', 'The server was not removed', text(e));
  }
}

// ---- signing in --------------------------------------------------------------------------------------------------

let polling = false;

/** `viaLink`: for a server without the loopback hand-over, the sign-in ends with a dynodoc:// link
 *  that the system gives to the installed app. */
export async function startSignIn(id: string, viaLink = false): Promise<void> {
  try {
    const pairing = (await call('account_signin_start', { id, viaLink })) as string;
    srv().flow = { state: 'waiting', account_id: id, pairing, message: '' };
    render();
    void follow();
  } catch (e) {
    toast('error', 'The browser sign-in did not start', text(e));
  }
}

async function follow(): Promise<void> {
  if (polling) return;
  polling = true;
  try {
    for (let i = 0; i < 1000; i++) {
      await sleep(700);
      let flow: ServerFlow;
      try {
        flow = (await call('account_signin_status')) as ServerFlow;
      } catch {
        break;
      }
      srv().flow = flow;
      if (flow.state !== 'waiting') {
        render();
        if (flow.state === 'done' && flow.account_id) {
          await loadServers();
          const a = accountOf(flow.account_id);
          toast('success', `Signed in to ${a?.host ?? 'the server'}`, a?.email ? `As ${a.email}.` : undefined);
          await loadRemoteProjects(flow.account_id);
        } else if (flow.state === 'failed') {
          toast('error', 'The sign-in did not finish', flow.message);
        }
        break;
      }
      render();
    }
  } finally {
    polling = false;
  }
}

export async function cancelSignIn(): Promise<void> {
  try {
    await call('account_signin_cancel');
  } catch {
    /* it is over either way */
  }
  srv().flow = { state: 'none', account_id: null, pairing: null, message: '' };
  render();
}

export async function signOutOf(id: string): Promise<void> {
  try {
    const r = (await call('account_signout', { id })) as { account: ServerAccount; server_confirmed: boolean };
    delete srv().projects[id];
    await loadServers();
    toast('success', 'Signed out', r.server_confirmed ? 'The server ended the session, and it is gone from this computer.' : 'The session is gone from this computer. The server could not be reached to end it there: it ends by itself.');
  } catch (e) {
    toast('error', 'Not signed out', text(e));
  }
}

/** Asks the server whether the session still holds (and who it is for). */
export async function checkServer(id: string): Promise<void> {
  try {
    await call('account_check', { id });
  } catch (e) {
    toast('warning', 'The server could not be reached', text(e));
  }
  await loadServers();
}

// ---- a server's projects, and links ----------------------------------------------------------------------------

export async function loadRemoteProjects(id: string): Promise<void> {
  srv().busy = 'Reading the projects on the server…';
  render();
  try {
    const r = await remoteOf(id).apiGet('projects');
    if (r.status === 401) {
      await loadServers();
      throw new Error('Your session on the server has ended. Sign in again.');
    }
    if (r.status < 200 || r.status >= 300) throw new Error(`The server answered ${r.status}.`);
    srv().projects[id] = ((r.body?.items ?? []) as any[]).map((p) => ({ id: String(p.id), name: String(p.name ?? 'Untitled'), description: String(p.description ?? '') }));
  } catch (e) {
    srv().projects[id] ??= [];
    toast('error', 'The projects could not be read', text(e));
  } finally {
    srv().busy = '';
    render();
  }
}

/** Links a project on this computer to a project on a server (or makes the project there). Nothing
 *  is sent until the first sync. */
export async function linkProject(localProjectId: string, accountId: string, remoteProjectId: string | 'new'): Promise<void> {
  try {
    const project = state.db.projects.find((p) => p.id === localProjectId);
    let id = remoteProjectId;
    let name = srv().projects[accountId]?.find((p) => p.id === remoteProjectId)?.name ?? '';
    if (remoteProjectId === 'new') {
      const made = await remoteOf(accountId).apiSend('POST', 'projects', { name: project?.name ?? 'Project', description: project?.description ?? '' });
      if (made.status < 200 || made.status >= 300) throw new Error(typeof made.body?.error === 'string' ? made.body.error : 'The project could not be made on the server.');
      id = String(made.body.id);
      name = String(made.body.name ?? project?.name ?? 'Project');
      await loadRemoteProjects(accountId);
    }
    links()[localProjectId] = S.newLink(accountId, id, name);
    delete srv().plans[localProjectId];
    await saveLinks();
    toast('success', 'Linked', 'Nothing has been sent yet. Check what a sync would do, then sync.');
  } catch (e) {
    toast('error', 'The project was not linked', text(e));
  }
  render();
}

export async function unlinkProject(localProjectId: string): Promise<void> {
  delete links()[localProjectId];
  delete srv().plans[localProjectId];
  delete srv().picks[localProjectId];
  try {
    await saveLinks();
    toast('success', 'Unlinked', 'The files stay where they are, on both sides.');
  } catch (e) {
    toast('error', 'Not saved', text(e));
  }
  render();
}

// ---- syncing -------------------------------------------------------------------------------------------------------------

const sidesFor = (localProjectId: string, l: S.Link): S.Sides => ({ local: local(), remote: remoteOf(l.accountId), localProjectId });

function picksFor(localProjectId: string): Record<string, S.Picks> {
  return (srv().picks[localProjectId] ?? {}) as Record<string, S.Picks>;
}

export function choose(localProjectId: string, docId: string, key: string, side: 'local' | 'remote'): void {
  const p = (srv().picks[localProjectId] ??= {});
  (p[docId] ??= {})[key] = side;
  render();
}

/** Reads both sides and keeps what a sync would do; writes nothing. */
export async function checkSync(localProjectId: string): Promise<S.Plan | null> {
  const l = links()[localProjectId];
  if (!l || srv().running[localProjectId]) return null;
  srv().running[localProjectId] = true;
  srv().last[localProjectId] = 'Reading both sides…';
  render();
  try {
    const plan = await S.planSync(sidesFor(localProjectId, l), l, picksFor(localProjectId));
    srv().plans[localProjectId] = plan;
    srv().last[localProjectId] = '';
    return plan;
  } catch (e) {
    srv().last[localProjectId] = text(e);
    toast('error', 'Could not read both sides', text(e));
    return null;
  } finally {
    srv().running[localProjectId] = false;
    render();
  }
}

function summary(done: S.Done, host: string): string {
  const bits: string[] = [];
  if (done.created) bits.push(`${done.created} new file${done.created === 1 ? '' : 's'}`);
  if (done.sent) bits.push(`${done.sent} sent to ${host}`);
  if (done.received) bits.push(`${done.received} received`);
  if (!bits.length) bits.push('already in sync');
  if (done.needs) bits.push(`${done.needs} need${done.needs === 1 ? 's' : ''} your choice`);
  return bits.join(', ');
}

/** Syncs a linked project: plans, writes what has no conflict, remembers where the two agreed. */
export async function syncNow(localProjectId: string): Promise<void> {
  const l = links()[localProjectId];
  if (!l || srv().running[localProjectId]) return;
  const account = accountOf(l.accountId);
  const host = account?.host ?? 'the server';
  srv().running[localProjectId] = true;
  srv().last[localProjectId] = 'Syncing…';
  render();
  try {
    const sides = sidesFor(localProjectId, l);
    const plan = await S.planSync(sides, l, picksFor(localProjectId));
    const done = await S.applyPlan(sides, l, plan, {
      host,
      onNote: async (docId, from, to, summaryText, description, on) => {
        try {
          await (on === 'local' ? local() : remoteOf(l.accountId)).svcSend('POST', 'notes', { document_id: docId, branch: 'main', from_seq: from, to_seq: to, kind: 'revision', summary: summaryText.slice(0, 200), description: description.slice(0, 4000) });
        } catch {
          /* the revision is there without its message */
        }
      },
    });
    await saveLinks();
    // What is still held back is shown again, with the choices.
    srv().plans[localProjectId] = await S.planSync(sides, l, picksFor(localProjectId));
    const said = summary(done, host);
    srv().last[localProjectId] = `${said}.${done.held.length ? ` ${done.held.map((h) => `${h.title}: ${h.why}`).join(' ')}` : ''}`;
    toast(done.needs || done.held.length ? 'warning' : 'success', `Synced ${state.db.projects.find((p) => p.id === localProjectId)?.name ?? 'the project'}`, said);
    if (isLocal() && (done.created || done.received)) await refresh(true);
  } catch (e) {
    srv().last[localProjectId] = text(e);
    toast('error', 'The sync did not finish', text(e));
  } finally {
    srv().running[localProjectId] = false;
    render();
  }
}

/** For a file the server wants a review of: sends the changes as a change request. */
export async function sendForReview(localProjectId: string, localDocId: string): Promise<void> {
  const l = links()[localProjectId];
  const item = planOf(localProjectId)?.items.find((i) => i.localId === localDocId);
  if (!l || !item) return;
  try {
    await S.sendForReview(sidesFor(localProjectId, l), l, item, state.local.profile?.name ?? 'a computer');
    await saveLinks();
    toast('success', 'Sent for review', `The changes to ${item.title} are a change request on ${accountOf(l.accountId)?.host ?? 'the server'}. A person there merges it; the next sync here picks the result up.`);
    await checkSync(localProjectId);
  } catch (e) {
    toast('error', 'Not sent', text(e));
  }
}

/** The sentence under a linked project: when it last synced. */
export function lastSynced(l: S.Link, now = Date.now()): string {
  if (!l.lastSyncedAt) return 'not synced yet';
  const s = Math.max(0, Math.round((now - l.lastSyncedAt) / 1000));
  if (s < 60) return 'just now';
  if (s < 3600) return `${Math.round(s / 60)} min ago`;
  if (s < 86400) return `${Math.round(s / 3600)} h ago`;
  return new Date(l.lastSyncedAt).toLocaleDateString();
}
