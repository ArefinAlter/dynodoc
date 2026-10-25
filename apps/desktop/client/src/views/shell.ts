// The T3 Code shell: a 272 px tinted sidebar (brand row, the current project and branch as GitHub
// Desktop shows them, search, grouped rows, open requests listed like T3's threads, icons at the
// bottom) and a 40 px top bar (breadcrumb, Fetch/Push with "last fetched", status pills).
import { h, cx, kbd, type Child } from '../dom.ts';
import { icon } from '../icons.ts';
import { state, render, go, type View } from '../state.ts';
import * as B from '../data/backend.ts';
import { openMenu } from '../ui/layer.ts';
import { openPalette } from '../ui/layer.ts';
import { pill } from '../ui/controls.ts';
import { toggleTheme, scheme } from '../theme.ts';
import { branchContext, branchLabel, currentBranch, currentProject, short, when } from './common.ts';
import { openBranchList, openBranchMenu, openProjectList, sync, fetchNow } from './actions.ts';
import { unread } from './activity.ts';
import { desktopState, openServer } from '../bridge.ts';
import { toast } from '../state.ts';
import { isLive } from '../data/live.ts';

interface NavItem {
  view: View;
  label: string;
  icon: string;
  count?: () => number;
  shortcut?: string;
}

export const NAV: { group: string; items: NavItem[] }[] = [
  {
    group: 'Version control',
    items: [
      { view: 'changes', label: 'Changes', icon: 'changes', count: () => B.working(state.db, state.ui.projectId, currentBranch()).length, shortcut: 'Ctrl+1' },
      { view: 'history', label: 'History', icon: 'history', shortcut: 'Ctrl+2' },
      { view: 'requests', label: 'Change requests', icon: 'request', count: () => state.db.requests.filter((r) => r.projectId === state.ui.projectId && r.status === 'open').length, shortcut: 'Ctrl+3' },
    ],
  },
  {
    group: 'Library',
    items: [
      { view: 'documents', label: 'Documents', icon: 'documents', shortcut: 'Ctrl+4' },
      { view: 'projects', label: 'Projects', icon: 'projects' },
      { view: 'connectors', label: 'Connectors', icon: 'connectors' },
    ],
  },
  {
    group: 'Team',
    items: [
      { view: 'activity', label: 'Activity', icon: 'activity', count: unread, shortcut: 'Ctrl+6' },
      { view: 'agent', label: 'Agent', icon: 'agent', shortcut: 'Ctrl+5' },
    ],
  },
];

export const VIEW_LABEL: Record<View, string> = {
  changes: 'Changes',
  history: 'History',
  requests: 'Change requests',
  documents: 'Documents',
  projects: 'Projects',
  connectors: 'Connectors',
  activity: 'Activity',
  agent: 'Agent',
  settings: 'Settings',
};

function navRow(it: NavItem): HTMLElement {
  const n = it.count?.() ?? 0;
  const on = state.ui.view === it.view;
  return h(
    'button',
    { type: 'button', class: cx('nav-row', on && 'is-selected'), 'aria-current': on ? 'page' : undefined, 'data-view': it.view, title: it.shortcut ? `${it.label} (${it.shortcut})` : it.label, onClick: () => go(it.view) },
    icon(it.icon, 16),
    h('span', { class: 'nav-label' }, it.label),
    n ? h('span', { class: 'nav-count' }, String(n)) : null,
  );
}

function picker(label: string, value: string, iconName: string, onClick: (e: MouseEvent) => void, id: string, title: string): HTMLElement {
  return h(
    'button',
    { type: 'button', class: 'picker-row', id, title, 'aria-haspopup': 'menu', onClick },
    icon(iconName, 16),
    h('span', { class: 'picker-text' }, h('span', { class: 'picker-label' }, label), h('span', { class: 'picker-value' }, value)),
    icon('chevron', 14, 'icon picker-chev'),
  );
}

function sidebar(): HTMLElement {
  const ctx = branchContext();
  const p = currentProject();
  const open = state.db.requests.filter((r) => r.projectId === p.id && r.status === 'open').sort((a, b) => b.createdAt - a.createdAt);
  const ds = desktopState();
  return h(
    'aside',
    { class: 'sidebar', 'aria-label': 'Navigation' },
    h('div', { class: 'brand-row' }, h('button', { type: 'button', class: 'icon-btn', 'aria-label': 'Hide sidebar', title: 'Hide sidebar (Ctrl+\\)', onClick: () => { state.prefs.sidebarOpen = false; render(); } }, icon('sidebar', 16)), h('img', { src: `${__ASSET_BASE__}mark.svg`, alt: '', class: 'mark-img', width: 18, height: 18 }), h('span', { class: 'brand-name' }, 'Dynodoc')),
    h(
      'div',
      { class: 'pickers' },
      picker('Current project', p.name, 'repo', (e) => openProjectList(e.currentTarget as HTMLElement), 'project-picker', 'Projects (Ctrl+T)'),
      h(
        'div',
        { class: 'branch-picker' },
        picker('Current branch', branchLabel(), 'branch', (e) => openBranchList(e.currentTarget as HTMLElement), 'branch-picker', 'Branches (Ctrl+B)'),
        h('button', { type: 'button', class: 'icon-btn branch-menu-btn', 'aria-label': 'Branch menu', title: 'Branch menu', 'aria-haspopup': 'menu', onClick: (e: MouseEvent) => openBranchMenu(e.currentTarget as HTMLElement) }, icon('more', 16)),
      ),
      ctx.kind === 'draft' ? h('div', { class: 'branch-state' }, (ctx.behind ?? 0) ? h('span', { title: 'Revisions of main this draft does not have' }, icon('down', 12), `${ctx.behind} behind`) : null, (ctx.ahead ?? 0) ? h('span', { title: 'Commits not pushed' }, icon('up', 12), `${ctx.ahead} to push`) : null, h('span', { class: 'muted' }, ctx.state === 'in_review' ? 'in review' : ctx.state)) : null,
    ),
    h('button', { type: 'button', class: 'search-row', onClick: () => openPalette() }, icon('search', 15), h('span', null, 'Search'), h('span', { class: 'spacer' }), kbd('Ctrl+K')),
    h(
      'nav',
      { class: 'nav', 'data-scroll-key': 'sidebar' },
      NAV.map((g) => h('div', { class: 'nav-group' }, h('div', { class: 'nav-group-label' }, g.group), g.items.map(navRow))),
      open.length
        ? h('div', { class: 'nav-group' }, h('div', { class: 'nav-group-label' }, 'Open requests'), open.map((r) => {
            const st = B.reviewState(state.db, r);
            return h('button', { type: 'button', class: cx('thread-row', state.ui.view === 'requests' && state.ui.requests.selected === r.id && 'is-selected'), onClick: () => { state.ui.requests.selected = r.id; state.ui.requests.filter = 'open'; go('requests'); } }, h('span', { class: cx('thread-dot', st.satisfied ? 'is-ready' : r.conflicts.length ? 'is-conflict' : 'is-waiting') }), h('span', { class: 'thread-title' }, `#${r.number} ${r.name}`), h('span', { class: 'muted small' }, short(r.createdAt)));
          }))
        : null,
    ),
    h(
      'div',
      { class: 'sidebar-foot' },
      h('button', { type: 'button', class: cx('icon-btn', state.ui.view === 'settings' && 'is-active'), 'aria-label': 'Settings', title: 'Settings (Ctrl+,)', 'data-view': 'settings', onClick: () => go('settings') }, icon('settings', 16)),
      h('button', { type: 'button', class: 'icon-btn', 'aria-label': 'Toggle light and dark', title: 'Light or dark (Ctrl+Shift+L)', onClick: () => { toggleTheme(); render(); } }, icon(scheme() === 'dark' ? 'sun' : 'moon', 16)),
      h('button', { type: 'button', class: 'icon-btn', 'aria-label': 'Account and server', title: ds?.server ? `Server: ${ds.server}` : 'Account and server', id: 'nav-account', onClick: () => { state.ui.settings.section = 'accounts'; go('settings'); } }, icon('user', 16)),
      h('span', { class: 'spacer' }),
      h('button', { type: 'button', class: 'release-pill', title: 'About this release', onClick: () => { state.ui.settings.section = 'about'; go('settings'); } }, 'Pre-alpha release'),
    ),
  );
}

function crumbs(): Child[] {
  const p = currentProject();
  const v = state.ui.view;
  const parts: Child[] = [];
  if (v !== 'settings' && v !== 'connectors' && v !== 'projects' && v !== 'activity') {
    parts.push(h('button', { type: 'button', class: 'crumb', onClick: (e: MouseEvent) => openProjectList(e.currentTarget as HTMLElement) }, p.name), h('span', { class: 'crumb-sep' }, '/'));
  }
  parts.push(h('span', { class: 'crumb crumb-current' }, VIEW_LABEL[v]));
  if (v === 'changes' || v === 'history') parts.push(h('span', { class: 'crumb-sep' }, '/'), h('span', { class: 'crumb-branch' }, icon('branch', 12), branchLabel()));
  if (v === 'requests' && state.ui.requests.selected) {
    const r = state.db.requests.find((x) => x.id === state.ui.requests.selected);
    if (r) parts.push(h('span', { class: 'crumb-sep' }, '/'), h('span', { class: 'crumb' }, `#${r.number}`));
  }
  return parts;
}

function topbar(): HTMLElement {
  const ctx = branchContext();
  const ahead = ctx.ahead ?? 0;
  const ds = desktopState();
  const fetching = state.ui.fetching;
  return h(
    'header',
    { class: 'topbar' },
    state.prefs.sidebarOpen ? null : h('button', { type: 'button', class: 'icon-btn', 'aria-label': 'Show sidebar', onClick: () => { state.prefs.sidebarOpen = true; render(); } }, icon('sidebar', 16)),
    h('nav', { class: 'crumbs-bar', 'aria-label': 'Breadcrumb' }, crumbs()),
    h('span', { class: 'spacer' }),
    h('span', { class: 'status-pills' },
      state.live.mode === 'connecting'
        ? pill('Connecting…', 'info', { mono: false, title: 'Reading your workspace from the server' })
        : isLive()
          ? pill(state.live.saving ? 'Saving…' : state.db.remote?.visitor ? 'Sample workspace' : 'Live', state.live.saving ? 'info' : 'success', { mono: false, title: state.db.remote?.visitor ? `Your own sample workspace on the server, until ${new Date(state.db.remote.visitor.expires_at).toLocaleString()}. Everything you do is saved there.` : 'Showing your workspace on the server. Everything you do is saved there.' })
          : pill('Sample data', 'warning', { mono: false, title: 'Offline sample: nothing leaves this window. See Settings, Accounts to use a server.' }),
      ds?.server
        ? h('button', { type: 'button', class: 'server-btn', title: 'Open your workspace on this server in this window', onClick: () => { openServer().catch((e) => toast('error', e instanceof Error ? e.message : String(e))); } }, icon('globe', 13), new URL(ds.server).host, icon('external', 12))
        : pill(ds ? 'No server' : 'Offline copy', 'neutral', { mono: false, title: ds ? 'Choose a server in Settings, Accounts' : 'This copy connects to nothing' }),
      ctx.kind === 'draft' && (ctx.behind ?? 0) ? pill(`${ctx.behind} behind main`, 'warning', { mono: false }) : null,
    ),
    h(
      'button',
      { type: 'button', class: cx('sync-btn', fetching && 'is-busy'), id: 'sync-button', title: ahead ? 'Push (Ctrl+Shift+P)' : 'Fetch (Ctrl+Shift+T)', disabled: fetching, onClick: () => (ahead ? sync() : fetchNow()) },
      icon(ahead ? 'up' : 'sync', 15, cx('icon', fetching && 'spin')),
      h('span', { class: 'sync-text' }, h('span', { class: 'sync-title' }, fetching ? (ahead ? 'Pushing…' : 'Fetching…') : ahead ? `Push ${ahead} commit${ahead === 1 ? '' : 's'}` : 'Fetch'), h('span', { class: 'sync-sub' }, `Last fetched ${when(state.db.lastFetched)}`)),
    ),
  );
}

function bottomBar(): HTMLElement {
  const items: [View, string, string][] = [['changes', 'Changes', 'changes'], ['history', 'History', 'history'], ['requests', 'Requests', 'request'], ['agent', 'Agent', 'agent']];
  return h(
    'nav',
    { class: 'bottombar', 'aria-label': 'Navigation' },
    items.map(([v, l, i]) => h('button', { type: 'button', class: cx('bottom-item', state.ui.view === v && 'is-on'), onClick: () => go(v) }, icon(i, 18), h('span', null, l))),
    h('button', { type: 'button', class: cx('bottom-item', !['changes', 'history', 'requests', 'agent'].includes(state.ui.view) && 'is-on'), onClick: (e: MouseEvent) => openMenu(e.currentTarget as HTMLElement, [
      { label: 'Documents', icon: 'documents', onSelect: () => go('documents') },
      { label: 'Projects', icon: 'projects', onSelect: () => go('projects') },
      { label: 'Connectors', icon: 'connectors', onSelect: () => go('connectors') },
      { label: 'Activity', icon: 'activity', onSelect: () => go('activity') },
      { label: 'Settings', icon: 'settings', onSelect: () => go('settings') },
      { label: '', separator: true },
      { label: 'Switch project…', icon: 'repo', onSelect: () => openProjectList(document.getElementById('topbar-anchor') ?? document.body) },
      { label: 'Switch branch…', icon: 'branch', onSelect: () => openBranchList(document.getElementById('topbar-anchor') ?? document.body) },
    ], { align: 'right', width: 240 }) }, icon('more', 18), h('span', null, 'More')),
  );
}

export function shell(main: HTMLElement): HTMLElement {
  return h(
    'div',
    { class: cx('app', !state.prefs.sidebarOpen && 'sidebar-closed') },
    state.prefs.sidebarOpen ? sidebar() : null,
    h('div', { class: 'main-col' }, topbar(), h('span', { id: 'topbar-anchor', class: 'anchor' }), h('main', { class: 'content', id: 'content' }, main)),
    bottomBar(),
  );
}
