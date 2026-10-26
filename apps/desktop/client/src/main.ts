// Dynodoc client: entry point. Renders the shell and the current view, keeps focus and scroll
// across renders, wires the keyboard and the command palette, and in the desktop app starts the
// launcher's own controls (bridge.ts) and gives them a place in Settings, Accounts.
import { h, clear } from './dom.ts';
import { state, setRenderer, setAfterRender, render, go, toast, resetSample } from './state.ts';
import { loadAgentCfg, refreshKeyHints } from './data/agentRun.ts';
import { loadServers } from './data/accounts.ts';
import { isLive, loadPrefs, noticeChanges, noticePrefs, refresh, signOut, startVisitor } from './data/live.ts';
import * as B from './data/backend.ts';
import { applyAppearance, toggleTheme } from './theme.ts';
import { commandFor } from './lib/keys.ts';
import { branchMenu } from './lib/branchMenu.ts';
import { renderLayer, setCommands, openPalette, paletteIsOpen, dialogOpen, closeMenu, type Command } from './ui/layer.ts';
import { shell, NAV, VIEW_LABEL } from './views/shell.ts';
import { changesView, commitStaged } from './views/changes.ts';
import { historyView } from './views/history.ts';
import { requestsView } from './views/requests.ts';
import { documentsView } from './views/documents.ts';
import { projectsView } from './views/projects.ts';
import { connectorsView } from './views/connectors.ts';
import { activityView } from './views/activity.ts';
import { agentView, startThread } from './views/agent.ts';
import { settingsView, SECTIONS, captureRecording } from './views/settings.ts';
import { branchContext, currentBranch, docOf } from './views/common.ts';
import { fetchNow, sync, runBranchAction, switchProject, switchBranch, openProjectList, openBranchList } from './views/actions.ts';
import { startBridge, launcherSection, focusSection, inDesktop, openServer } from './bridge.ts';
import { splash } from './views/splash.ts';
import { startLocalWorkspace } from './data/local.ts';
import { loadFolders, startWatch } from './data/files.ts';

const VIEWS = {
  changes: changesView,
  history: historyView,
  requests: requestsView,
  documents: documentsView,
  projects: projectsView,
  connectors: connectorsView,
  activity: activityView,
  agent: agentView,
  settings: settingsView,
};

let root: HTMLElement;
let layer: HTMLElement;
let launcher: HTMLElement | null = null;
let launcherHome: HTMLElement | null = null;

function remember(): { id: string | null; start: number | null; end: number | null; scroll: Map<string, number>; stick: Set<string> } {
  const a = document.activeElement as HTMLInputElement | null;
  const scroll = new Map<string, number>();
  const stick = new Set<string>();
  document.querySelectorAll<HTMLElement>('[data-scroll-key]').forEach((el) => {
    const key = el.getAttribute('data-scroll-key') ?? '';
    scroll.set(key, el.scrollTop);
    if (el.hasAttribute('data-stick-bottom') && el.scrollHeight - el.scrollTop - el.clientHeight < 40) stick.add(key);
  });
  let start: number | null = null, end: number | null = null;
  try {
    start = a && 'selectionStart' in a ? a.selectionStart : null;
    end = a && 'selectionEnd' in a ? a.selectionEnd : null;
  } catch {
    /* some inputs have no selection */
  }
  return { id: a && a.id && !(launcher && launcher.contains(a)) ? a.id : null, start, end, scroll, stick };
}

function restore(saved: ReturnType<typeof remember>): void {
  document.querySelectorAll<HTMLElement>('[data-scroll-key]').forEach((el) => {
    const key = el.getAttribute('data-scroll-key') ?? '';
    if (el.hasAttribute('data-stick-bottom') && (saved.stick.has(key) || !saved.scroll.has(key))) el.scrollTop = el.scrollHeight;
    else if (saved.scroll.has(key)) el.scrollTop = saved.scroll.get(key) ?? 0;
  });
  if (saved.id) {
    const el = document.getElementById(saved.id) as HTMLInputElement | null;
    if (el && document.activeElement !== el) {
      el.focus({ preventScroll: true });
      try {
        if (saved.start !== null && 'setSelectionRange' in el) el.setSelectionRange(saved.start, saved.end ?? saved.start);
      } catch {
        /* not a text field */
      }
    }
  }
}

function draw(): void {
  const saved = remember();
  const view = VIEWS[state.ui.view] ?? changesView;
  const start = splash();
  const app = start ?? shell(view());
  // Keep the launcher's element alive across renders: take it out before the old tree goes.
  if (launcher && launcherHome && launcher.parentElement !== launcherHome) launcherHome.appendChild(launcher);
  clear(root);
  root.appendChild(app);
  const slot = document.getElementById('launcher-slot');
  if (launcher && slot) slot.appendChild(launcher);
  renderLayer(layer);
  restore(saved);
  document.title = state.ui.view === 'settings' ? 'Dynodoc' : `${VIEW_LABEL[state.ui.view]} · Dynodoc`;
}

// ---- commands -----------------------------------------------------------------------------------

function commands(): Command[] {
  const out: Command[] = [];
  for (const g of NAV) for (const it of g.items) out.push({ id: `view.${it.view}`, label: `Go to ${it.label}`, group: 'Go to', icon: it.icon, shortcut: it.shortcut, run: () => go(it.view) });
  out.push({ id: 'settings', label: 'Open Settings', group: 'Go to', icon: 'settings', shortcut: 'Ctrl+,', run: () => go('settings') });
  for (const s of SECTIONS) out.push({ id: `settings.${s.id}`, label: `Settings: ${s.label}`, group: 'Settings', icon: s.icon, keywords: 'options preferences', run: () => { state.ui.settings.section = s.id; go('settings'); } });
  const ahead = branchContext().ahead ?? 0;
  out.push({ id: 'sync', label: ahead ? `Push ${ahead} commit${ahead === 1 ? '' : 's'}` : 'Fetch', group: 'Project', icon: ahead ? 'up' : 'sync', shortcut: 'Ctrl+Shift+P', run: sync });
  out.push({ id: 'commit', label: 'Commit staged changes', group: 'Project', icon: 'check', shortcut: 'Ctrl+Enter', run: () => { go('changes'); commitStaged(); } });
  out.push({ id: 'stage', label: 'Stage all changes', group: 'Project', icon: 'up', run: () => { B.setStaged(state.db, B.working(state.db, state.ui.projectId, currentBranch()).map((w) => w.id), true); render(); } });
  for (const a of branchMenu(branchContext())) out.push({ id: `branch.${a.id}`, label: a.label.replace('…', ''), group: 'Branch', icon: 'branch', shortcut: a.shortcut, disabled: !a.enabled, keywords: a.note, run: () => runBranchAction(a.id) });
  for (const p of state.db.projects) out.push({ id: `project.${p.id}`, label: `Switch to project ${p.name}`, group: 'Projects', icon: 'repo', keywords: 'repository', run: () => switchProject(p.id) });
  out.push({ id: 'branch.main', label: 'Switch to main', group: 'Branches', icon: 'branch', run: () => switchBranch('main') });
  for (const d of state.db.drafts.filter((x) => x.projectId === state.ui.projectId && (x.state === 'private' || x.state === 'in_review') && x.authorId === state.db.me))
    out.push({ id: `branch.${d.id}`, label: `Switch to ${d.name}`, group: 'Branches', icon: 'branch', keywords: docOf(d.docId).filename, run: () => switchBranch(d.id) });
  for (const d of B.docsOf(state.db, state.ui.projectId)) out.push({ id: `doc.${d.id}`, label: d.filename, group: 'Files', icon: 'file', keywords: d.title, run: () => { state.ui.documents.selected = d.id; state.ui.documents.tab = 'overview'; go('documents'); } });
  for (const r of state.db.requests.filter((x) => x.projectId === state.ui.projectId)) out.push({ id: `cr.${r.id}`, label: `#${r.number} ${r.name}`, group: 'Change requests', icon: 'request', keywords: r.status, run: () => { state.ui.requests.selected = r.id; state.ui.requests.filter = r.status === 'open' ? 'open' : r.status === 'merged' ? 'merged' : 'closed'; go('requests'); } });
  out.push({ id: 'agent.new', label: 'New agent thread', group: 'Agent', icon: 'agent', shortcut: 'Ctrl+Shift+O', run: startThread });
  out.push({ id: 'theme', label: 'Toggle light and dark', group: 'Appearance', icon: 'moon', shortcut: 'Ctrl+Shift+L', run: () => { toggleTheme(); render(); } });
  if (!isLive()) out.push({ id: 'reset', label: 'Reset the sample project', group: 'Advanced', icon: 'revert', run: () => { resetSample(); toast('success', 'Sample data reset'); } });
  if (state.desktop && !isLive()) out.push({ id: 'visitor', label: 'Start a sample workspace on the pre-alpha server', group: 'Account', icon: 'globe', run: () => void startVisitor() });
  if (isLive()) out.push({ id: 'reload', label: 'Read everything from the server again', group: 'Account', icon: 'sync', run: () => void refresh() });
  if (isLive()) out.push({ id: 'signout', label: 'Sign out of the server', group: 'Account', icon: 'user', run: () => void signOut() });
  if (state.desktop) out.push({ id: 'server', label: 'Open my server’s workspace in this window', group: 'Account', icon: 'globe', run: () => { openServer().then((s) => { if (!s) { state.ui.settings.section = 'accounts'; go('settings'); toast('info', 'Choose a server first'); } }).catch((e) => toast('error', e instanceof Error ? e.message : String(e))); } });
  return out;
}

function runCommand(cmd: string): void {
  switch (cmd) {
    case 'palette': return openPalette();
    case 'view.changes': return go('changes');
    case 'view.history': return go('history');
    case 'view.requests': return go('requests');
    case 'view.documents': return go('documents');
    case 'view.agent': return go('agent');
    case 'view.activity': return go('activity');
    case 'repo.list': return openProjectList(document.getElementById('project-picker') ?? document.getElementById('topbar-anchor') ?? document.body);
    case 'branch.list': return openBranchList(document.getElementById('branch-picker') ?? document.getElementById('topbar-anchor') ?? document.body);
    case 'sidebar.toggle': state.prefs.sidebarOpen = !state.prefs.sidebarOpen; return render();
    case 'settings': return go('settings');
    case 'theme.toggle': toggleTheme(); return render();
    case 'sync': return sync();
    case 'fetch': return fetchNow();
    case 'commit': return commitStaged();
    case 'stage.all': B.setStaged(state.db, B.working(state.db, state.ui.projectId, currentBranch()).map((w) => w.id), true); return render();
    case 'branch.new': return runBranchAction('new');
    case 'branch.delete': return runBranchAction('delete');
    case 'branch.discard': return runBranchAction('discard');
    case 'branch.update': return runBranchAction('update');
    case 'branch.compare': return runBranchAction('compare');
    case 'branch.merge': return runBranchAction('merge');
    case 'branch.rebase': return runBranchAction('rebase');
    case 'branch.request': return runBranchAction('request');
    case 'agent.new': return startThread();
    default: return;
  }
}

function onKey(e: KeyboardEvent): void {
  if (captureRecording(e)) return;
  if (e.key === 'Escape') closeMenu();
  if (dialogOpen()) return;
  if (paletteIsOpen() && !(e.ctrlKey && e.key.toLowerCase() === 'k')) return;
  const target = e.target as HTMLElement | null;
  if (target && launcher && launcher.contains(target)) return; // the launcher's own fields keep their keys
  const cmd = commandFor(state.prefs.bindings, e);
  if (!cmd) return;
  const typing = target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT');
  // In a text field only combinations with Ctrl or Alt run commands (Ctrl+Enter commits).
  if (typing && !(e.ctrlKey || e.metaKey || e.altKey)) return;
  e.preventDefault();
  if (cmd === 'palette' && paletteIsOpen()) return;
  runCommand(cmd);
}

// ---- start ---------------------------------------------------------------------------------------

async function boot(): Promise<void> {
  root = document.getElementById('app') ?? document.body.appendChild(h('div', { id: 'app' }));
  layer = document.body.appendChild(h('div', { id: 'layer' }));
  document.body.appendChild(h('div', { id: 'menus' }));
  launcher = document.getElementById('launcher');
  if (launcher) {
    launcherHome = document.body.appendChild(h('div', { id: 'launcher-home', hidden: true }));
    launcherHome.appendChild(launcher);
    launcher.classList.add('is-embedded');
  }
  document.documentElement.classList.add('client');
  applyAppearance();
  setRenderer(draw);
  setAfterRender(() => {
    noticeChanges();
    noticePrefs();
  });
  setCommands(commands);
  document.addEventListener('keydown', onKey);
  window.matchMedia?.('(prefers-color-scheme: dark)').addEventListener?.('change', () => render());
  // Times on screen ("3 min ago") move on.
  setInterval(() => { if (!dialogOpen() && !paletteIsOpen()) render(); }, 60_000);
  setInterval(() => { if (state.prefs.backgroundFetch && Date.now() - state.db.lastFetched > state.prefs.fetchMinutes * 60_000) { if (isLive()) void refresh(true); else fetchNow(); } }, 30_000);

  const section = launcherSection();
  if (inDesktop() && launcher) {
    state.desktop = true;
    // Never show the offline sample first: the start-up screen is what a person sees until the
    // workspace on this computer is ready.
    state.local.splash = true;
    draw();
    const ds = await startBridge();
    if (ds) {
      state.desktopVersion = ds.version;
      await loadPrefs();
      applyAppearance();
      // The agent's keys live in the credential store: the page learns only which entries have one.
      void loadAgentCfg().then(refreshKeyHints);
      state.local.splash = true;
      draw();
      // The workspace on this computer is what the screens show: the app starts its own database
      // and engine (no server, no account). A server is an addition (Settings, Accounts).
      const here = await startLocalWorkspace();
      if (here) {
        void loadAgentCfg().then(refreshKeyHints);
        void loadServers();
        startWatch();
        void loadFolders();
      }
      // The launcher shows because the app was asked for it (File menu, a sign-in, a changed
      // saved address): go straight to it.
      if (section || ds.unconfirmed) {
        state.ui.settings.section = 'accounts';
        state.ui.view = 'settings';
      }
    }
    draw();
    focusSection(section);
  } else {
    draw();
  }
}

boot();
