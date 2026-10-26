// The workspace on this computer: the app starts its own database and engine (src-tauri/src/local),
// and this module follows the start-up for the page: it polls the app for the step, lets the person
// say who they are and name a first project (nothing leaves the computer), then shows the local
// workspace in the screens through the same live code that shows a server.
import { state, render, toast } from '../state.ts';
import { call } from '../bridge.ts';
import { connectEnv, transportFor } from './live.ts';

export interface LocalStatus {
  phase: 'starting' | 'profile' | 'ready' | 'failed' | 'closing';
  step: string;
  detail: string | null;
  profile: { name: string; email: string } | null;
  first_run: boolean;
  folder: string;
  secrets_in: string;
  engine_restarts: number;
}

const sleep = (n: number) => new Promise<void>((r) => setTimeout(r, n));

function take(s: LocalStatus): void {
  const l = state.local;
  l.phase = s.phase;
  l.step = s.step;
  l.detail = s.detail ?? '';
  l.firstRun = s.first_run;
  l.folder = s.folder;
  l.secretsIn = s.secrets_in;
  l.restarts = s.engine_restarts;
  l.profile = s.profile;
}

/** Reads the app's view of the local workspace into `state.local`. */
export async function readLocal(): Promise<LocalStatus | null> {
  try {
    const s = (await call('local_status')) as LocalStatus;
    take(s);
    return s;
  } catch {
    return null;
  }
}

/** Follows the start-up until the workspace is ready (then shows it) or it failed. Returns whether
 *  the local workspace is now what the screens show. */
export async function startLocalWorkspace(): Promise<boolean> {
  state.local.splash = true;
  state.live.mode = 'connecting';
  render();
  for (let i = 0; i < 3000; i++) {
    const s = await readLocal();
    if (!s) {
      state.local.phase = 'failed';
      state.local.detail = 'The app did not answer. Close it and open it again.';
      render();
      return false;
    }
    if (s.phase === 'ready') break;
    if (s.phase === 'failed') {
      state.live.mode = 'offline';
      render();
      return false;
    }
    render();
    await sleep(s.phase === 'profile' ? 700 : 350);
  }
  return connectLocal();
}

/** Reads the (ready) local workspace into the screens. */
export async function connectLocal(): Promise<boolean> {
  const ok = await connectEnv('local');
  if (ok) {
    state.local.splash = false;
    state.live.server = '';
    render();
  } else {
    state.local.phase = 'failed';
    state.local.detail = state.live.error || 'The local workspace could not be read.';
    render();
  }
  return ok;
}

/** The first-run form: a name, optionally an address, and the first project. */
export async function createProfile(): Promise<void> {
  const f = state.local.form;
  if (f.busy) return;
  if (!f.name.trim()) {
    f.error = 'Enter your name.';
    render();
    return;
  }
  f.busy = true;
  f.error = '';
  render();
  try {
    await call('local_create_profile', { name: f.name, email: f.email.trim() || null });
    const t = transportFor('local');
    const projects = await t.apiGet('projects');
    const have = projects.status === 200 && Array.isArray(projects.body?.items) && projects.body.items.length > 0;
    if (!have) {
      const name = f.project.trim() || 'My documents';
      const made = await t.apiSend('POST', 'projects', { name, description: '' });
      if (made.status < 200 || made.status >= 300) throw new Error(typeof made.body?.error === 'string' ? made.body.error : 'The first project could not be created.');
    }
    await readLocal();
    await connectLocal();
  } catch (e) {
    f.error = e instanceof Error ? e.message : String(e);
  } finally {
    f.busy = false;
    render();
  }
}

/** After a failure: ask the app to try the start-up again. */
export async function retryLocal(): Promise<void> {
  state.local.phase = 'starting';
  state.local.step = 'Starting again';
  state.local.detail = '';
  render();
  try {
    await call('local_retry');
  } catch (e) {
    toast('error', 'Could not start again', e instanceof Error ? e.message : String(e));
  }
  await startLocalWorkspace();
}

/** The folder with the workspace's logs, in the system's file manager. */
export async function openLocalFolder(): Promise<void> {
  try {
    await call('local_open_folder');
  } catch (e) {
    toast('error', 'Could not open the folder', e instanceof Error ? e.message : String(e));
  }
}
