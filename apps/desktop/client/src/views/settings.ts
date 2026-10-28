// Settings: T3 Code's sections (General, Appearance, Keybindings, Providers, ...) and GitHub
// Desktop's Options mapped to Dynodoc (Accounts, Integrations, Version control, Notifications,
// Prompts, Advanced, Accessibility), with Git GUI's diff options where they mean something here.
import { h, cx, kbd, type Child } from '../dom.ts';
import { icon } from '../icons.ts';
import { state, render, toast, resetSample, type SettingsSection, type Skill, DEFAULT_PREFS } from '../state.ts';
import * as B from '../data/backend.ts';
import { CONNECTORS, HOST_LABEL } from '../data/connectors.ts';
import { DEFAULT_BINDINGS, comboOf, conflicts, normalize, usable } from '../lib/keys.ts';
import { ACCESS_INFO, NEVER, TOOLS, DEFAULT_INSTRUCTIONS } from '../lib/agentTools.ts';
import { PROVIDERS, validate, active, keyHint, type LadderEntry, type ProviderKind } from '../lib/ladder.ts';
import { serversSection } from './servers.ts';
import { loadServers } from '../data/accounts.ts';
import { dropEntry, forgetKey, loadAgentCfg, setAgentCfg, setConsent, storeKey, unlockKeys } from '../data/agentRun.ts';
import { button, input, row, rows, section, select, toggle, textarea, segmented, pill, badge, notice, field, avatar, iconButton } from '../ui/controls.ts';
import { openDialog, closeDialog, confirmDialog } from '../ui/layer.ts';
import { applyAppearance } from '../theme.ts';
import { ROLE_LABEL, personName, when, isAgent } from './common.ts';
import { installSteps } from './connectors.ts';
import { aboutBody } from './about.ts';
import { isLive, refresh, signOut, startVisitor, currentTransport } from '../data/live.ts';
import { backupNow, restoreNow, openLocalFolder } from '../data/local.ts';
import type { AddinHost, Role } from '../data/types.ts';

export const SECTIONS: { id: SettingsSection; label: string; icon: string; group: string }[] = [
  { id: 'general', label: 'General', icon: 'settings', group: 'App' },
  { id: 'accounts', label: 'Accounts', icon: 'user', group: 'App' },
  { id: 'workspace', label: 'This computer', icon: 'documents', group: 'App' },
  { id: 'appearance', label: 'Appearance', icon: 'palette', group: 'App' },
  { id: 'keybindings', label: 'Keybindings', icon: 'keyboard', group: 'App' },
  { id: 'integrations', label: 'Integrations', icon: 'external', group: 'App' },
  { id: 'versioncontrol', label: 'Version control', icon: 'branch', group: 'Work' },
  { id: 'notifications', label: 'Notifications', icon: 'bell', group: 'Work' },
  { id: 'prompts', label: 'Prompts', icon: 'prompt', group: 'Work' },
  { id: 'providers', label: 'Providers', icon: 'server', group: 'Agent' },
  { id: 'agent', label: 'Agent', icon: 'agent', group: 'Agent' },
  { id: 'skills', label: 'Skills', icon: 'skill', group: 'Agent' },
  { id: 'connectors', label: 'Connectors', icon: 'connectors', group: 'Agent' },
  { id: 'team', label: 'Team and roles', icon: 'users', group: 'Organisation' },
  { id: 'keys', label: 'Signing keys', icon: 'key', group: 'Organisation' },
  { id: 'admin', label: 'Administration', icon: 'admin', group: 'Organisation' },
  { id: 'advanced', label: 'Advanced', icon: 'advanced', group: 'System' },
  { id: 'accessibility', label: 'Accessibility', icon: 'accessibility', group: 'System' },
  { id: 'about', label: 'About', icon: 'info', group: 'System' },
];

const p = () => state.prefs;
const set = (fn: () => void) => { fn(); applyAppearance(); render(); };

// ---- sections ----------------------------------------------------------------------------------------

function general(): Child {
  return [
    section('Start-up', null, rows(
      row('Current project', 'GitHub Desktop’s current repository.', select(state.ui.projectId, state.db.projects.map((x) => [x.id, x.name] as [string, string]), (v) => { state.ui.projectId = v; render(); }, { label: 'Current project' })),
      row('Language', 'More languages come with the web workspace’s translations.', select('en', [['en', 'English']], () => undefined, { label: 'Language', disabled: true })),
    )),
    section('Links', null, rows(
      row('Web links open in your browser', 'Links to other sites never open in this window.', null, { fixed: true }),
      row(state.desktop ? 'Preferences are saved on this computer' : 'Preferences are kept for this session', state.desktop ? 'Theme, keybindings, providers (never their keys) and the other choices on these pages are written to a settings file in the app’s folder, and used again the next time the app opens. This window does not write the web view’s storage.' : 'The hosted copy keeps nothing in your browser: settings last until you close the tab.', null, { fixed: true }),
    )),
  ];
}

let profile = { name: 'Jordan Lee', bio: 'Operations, Sample organisation.' };

function workspaceCard(): Child {
  const live = state.live;
  const visitor = state.db.remote?.visitor;
  if (live.mode === 'connecting') return notice('info', 'Reading your workspace from the server…');
  if (isLive())
    return h('div', { class: 'card pad' },
      h('p', null, visitor
        ? [h('strong', null, 'Your sample workspace on the pre-alpha server. '), `It is yours alone (signed in as ${visitor.display_name}) and is removed on ${new Date(visitor.expires_at).toLocaleString()}. Every click is saved there; nothing private belongs in it.`]
        : [h('strong', null, 'Signed in. '), `These screens show your workspace on ${live.server || 'the server'}, and every change is saved there.`]),
      h('div', { class: 'row-actions' },
        button('Read everything again', { size: 'sm', icon: 'sync', onClick: () => void refresh() }),
        button('Sign out', { size: 'sm', variant: 'ghost', onClick: () => confirmDialog('Sign out', h('p', null, 'This window signs out of the server and shows the offline sample again.'), 'Sign out', () => void signOut()) })));
  return h('div', { class: 'card pad' },
    h('p', null, 'These screens show an offline sample: nothing you do leaves this window. To try them on a real Dynodoc server, start a sample workspace on the hosted pre-alpha server (no account needed, removed after 24 hours), or sign in to your server below.'),
    live.error ? notice('error', live.error) : null,
    h('div', { class: 'row-actions' }, button('Start a sample workspace', { size: 'sm', variant: 'primary', icon: 'globe', onClick: () => void startVisitor() })));
}

async function saveProfile(name: string, bio: string): Promise<void> {
  const t = currentTransport();
  const current = await t.apiGet('profile');
  const reply = await t.apiSend('POST', 'profile', { name, bio, revision: current.body?.revision ?? 0 });
  if (reply.status >= 300) throw new Error(reply.body?.error || 'The server did not save the profile.');
}

function accounts(): Child {
  const slot = h('div', { id: 'launcher-slot', class: 'launcher-slot' });
  const me = isLive() ? state.db.people.find((x) => x.id === state.db.me) : null;
  if (me && profile.name === 'Jordan Lee') profile = { name: me.name, bio: '' };
  return [
    state.desktop ? section('Workspace', 'Where the data on these screens comes from.', workspaceCard()) : null,
    section('Profile', 'Your name on revisions, reviews and comments.', h('div', { class: 'card pad' },
      h('div', { class: 'profile-row' }, avatar(profile.name, false, 'md'), h('div', null, h('strong', null, profile.name), h('div', { class: 'muted small' }, (me && me.email) || p().authorEmail))),
      field('Name', input(profile.name, (v) => { profile.name = v; }, { id: 'profile-name' })),
      field('Bio', textarea(profile.bio, (v) => { profile.bio = v; }, { id: 'profile-bio', rows: 2, maxlength: 280 }), 'Up to 280 characters.'),
      h('div', { class: 'row-actions' }, button('Save profile', { size: 'sm', variant: 'primary', onClick: () => {
        p().authorName = profile.name;
        if (!isLive()) { toast('success', 'Profile saved', 'Offline sample: kept on this computer only.'); render(); return; }
        saveProfile(profile.name, profile.bio).then(() => { const meNow = state.db.people.find((x) => x.id === state.db.me); if (meNow) meNow.name = profile.name; toast('success', 'Profile saved on the server'); render(); }).catch((e) => toast('error', 'Profile not saved', e instanceof Error ? e.message : String(e)));
      } })),
    )),
    serversSection(),
    section(
      state.desktop ? 'Web workspace in your browser' : 'Dynodoc server',
      state.desktop
        ? 'Optional and separate from syncing: this opens the server’s web workspace in your system browser. This desktop app manages document workflow and sync; editing happens in Office and Google applications through the add-ins and extensions.'
        : 'Signing in to a server and keeping a key for this computer are in the desktop app. This copy runs on sample data and connects to nothing.',
      state.desktop ? slot : notice('info', 'Pre-alpha release on sample data: no server, no sign-in. Install the desktop app to connect to the Dynodoc cloud or your own server.'),
    ),
  ];
}

/** The workspace on this computer: where it lives, what holds its keys, backup and restore. */
function workspaceSection(): Child {
  const l = state.local;
  if (!state.desktop) return notice('info', 'The workspace on this computer is part of the desktop app. This copy runs on sample data and stores nothing.');
  const running = l.phase === 'ready';
  return [
    section('Workspace on this computer', 'Your documents, their history and your reviews live in a database this app starts for you when it opens and stops when it closes. No server and no account are needed.', rows(
      row('Status', running ? 'The database and the engine are running on this computer, on a loopback address nothing else can reach.' : l.detail || l.step, pill(running ? 'Running' : l.phase === 'failed' ? 'Stopped' : 'Starting', running ? 'success' : l.phase === 'failed' ? 'error' : 'info', { mono: false })),
      row('Folder', 'Everything the workspace stores on this computer (the logs are in its logs folder).', h('div', { class: 'row-actions' }, h('code', { class: 'small folder-path', id: 'workspace-folder' }, l.folder), button('Open the logs', { size: 'sm', onClick: () => void openLocalFolder() }))),
      row('Secrets', 'The database passwords and the engine keys are made on the first start and kept in ' + (l.secretsIn || 'the credential store') + '. They are never in a file you could copy by mistake, in a backup, or in a log.', null, { fixed: true }),
      row('Engine restarts', 'The app starts the engine again if it stops. This counts the restarts since the app opened.', h('span', { class: 'mono' }, String(l.restarts))),
    )),
    section('Backup and restore', 'One archive holds the whole workspace. It opens only with the same kind of operating system and the same PostgreSQL version, and it holds your documents unencrypted.', rows(
      row('Back up this workspace', 'The database closes for a few seconds while the archive is written, then opens again.', button('Back up…', { size: 'sm', id: 'backup-now', disabled: !running, onClick: () => void backupNow() })),
      row('Restore from a backup', 'Replaces everything in this workspace with the backup. What it replaces is kept beside it, not deleted. New passwords and keys are made for the restored data.', button('Restore…', { size: 'sm', id: 'restore-now', disabled: !running, onClick: () => confirmDialog('Restore a backup', h('p', null, 'This replaces everything in this workspace (documents, history, reviews, and the settings kept with the workspace) with the contents of a backup you choose. What it replaces is kept in the workspace folder and can be put back by hand.'), 'Choose a backup…', () => void restoreNow(), { danger: true }) })),
    )),
  ];
}

const ACCENTS = ['', '#2563eb', '#7c3aed', '#0f766e', '#be123c', '#b45309'];

function appearance(): Child {
  const themeCard = (v: 'light' | 'dark' | 'system', label: string) =>
    h('button', { type: 'button', class: cx('theme-card', p().theme === v && 'is-on'), 'aria-pressed': String(p().theme === v), onClick: () => set(() => (p().theme = v)) },
      h('span', { class: `theme-swatch sw-${v}` }, h('span', { class: 'sw-side' }), h('span', { class: 'sw-main' }, h('span', { class: 'sw-line' }), h('span', { class: 'sw-line sw-add' }), h('span', { class: 'sw-line sw-del' }))),
      h('span', null, label));
  return [
    section('Theme', null, h('div', { class: 'theme-cards' }, themeCard('light', 'Light'), themeCard('dark', 'Dark'), themeCard('system', 'System'))),
    section('Colours', null, rows(
      row('Palette', 'Neutral is T3 Code’s zinc. Paper is warmer; Slate is cooler; High contrast strengthens lines and text.', select(p().palette, [['neutral', 'Neutral'], ['paper', 'Paper'], ['slate', 'Slate'], ['contrast', 'High contrast']], (v) => set(() => (p().palette = v)), { label: 'Palette' })),
      row('Accent', 'One colour for the active row, the primary button and links.', h('div', { class: 'accents' },
        ACCENTS.map((c) => h('button', { type: 'button', class: cx('accent-swatch', p().accent === c && 'is-on'), 'aria-label': c ? `Accent ${c}` : 'Dynodoc teal', css: { '--swatch': c || 'var(--brand)' }, onClick: () => set(() => (p().accent = c)) })),
        input(p().accent, (v) => { if (/^#[0-9a-fA-F]{6}$/.test(v) || v === '') set(() => (p().accent = v)); }, { id: 'accent-custom', placeholder: '#rrggbb', label: 'Custom accent', class: 'input-hex', maxlength: 7 }))),
      row('Density', 'Compact uses 28 px rows and controls.', segmented(p().density, [['comfortable', 'Comfortable'], ['compact', 'Compact']], (v) => set(() => (p().density = v)), 'Density')),
      row('Text size', null, segmented(p().fontSize, [['small', 'Small'], ['default', 'Default'], ['large', 'Large']], (v) => set(() => (p().fontSize = v)), 'Text size')),
    )),
    section('Formatting', 'GitHub Desktop’s formats, used for every date and figure in the client.', rows(
      row('Date format', null, select(p().dateFormat, [['medium', 'Oct 19, 2026'], ['iso', '2026-10-19'], ['dmy', '19/10/2026']], (v) => set(() => (p().dateFormat = v)), { label: 'Date format' })),
      row('Time format', null, select(p().timeFormat, [['24h', '14:30'], ['12h', '2:30 pm']], (v) => set(() => (p().timeFormat = v)), { label: 'Time format' })),
      row('Number format', null, select(p().numberFormat, [['comma', '1,234,567.89'], ['space', '1 234 567.89'], ['dot', '1.234.567,89']], (v) => set(() => (p().numberFormat = v)), { label: 'Number format' })),
      row('Prefer absolute dates over relative', '“Oct 19, 2026 14:30” instead of “3 h ago”.', toggle(p().absoluteDates, (v) => set(() => (p().absoluteDates = v)), 'Absolute dates')),
    )),
    section('Diff', null, rows(
      row('Layout', 'Unified, or side by side.', segmented(p().diffLayout, [['unified', 'Unified'], ['split', 'Side by side']], (v) => set(() => (p().diffLayout = v)), 'Diff layout')),
      row('Mark changed words', 'Highlight the words that changed inside a paragraph, cell or slide text.', toggle(p().wordDiff, (v) => set(() => (p().wordDiff = v)), 'Word diff')),
      row('Tab size', 'For plain-text and Markdown files.', select(String(p().tabSize), [['2', '2'], ['4', '4 (default)'], ['8', '8']], (v) => set(() => (p().tabSize = Number(v))), { label: 'Tab size' })),
    )),
  ];
}

let recording: string | null = null;

function keybindings(): Child {
  const list = p().bindings;
  const cf = conflicts(list);
  const conflictOf = (cmd: string) => cf.find((c) => c.commands.includes(cmd));
  return [
    section('Keybindings', 'Press Record and the new keys. A key used twice is shown in red; Ctrl+R stays the window’s Reload, and copy, paste and undo stay with text fields.',
      cf.length ? notice('warning', `${cf.length} conflict${cf.length === 1 ? '' : 's'}: ${cf.map((c) => `${c.combo} (${c.commands.join(', ')})`).join('; ')}`) : null,
      h('table', { class: 'table card' },
        h('thead', null, h('tr', null, h('th', null, 'Command'), h('th', null, 'Group'), h('th', null, 'Keys'), h('th', null, ''))),
        h('tbody', null, list.map((b) => {
          const c = conflictOf(b.command);
          return h('tr', { class: cx(c && 'is-conflict') },
            h('td', null, b.label),
            h('td', { class: 'muted' }, b.group),
            h('td', null, recording === b.command ? h('span', { class: 'recording', id: 'recording' }, 'Press keys… (Esc cancels)') : b.combo ? kbd(b.combo) : h('span', { class: 'muted' }, 'none')),
            h('td', { class: 'right' },
              button(recording === b.command ? 'Cancel' : 'Record', { size: 'sm', variant: 'ghost', onClick: () => { recording = recording === b.command ? null : b.command; render(); } }),
              button('Clear', { size: 'sm', variant: 'ghost', disabled: !b.combo, onClick: () => { b.combo = ''; render(); } })));
        }))),
      h('div', { class: 'row-actions' }, button('Restore defaults', { size: 'sm', onClick: () => { p().bindings = DEFAULT_BINDINGS.map((b) => ({ ...b })); render(); } }))),
  ];
}

/** Called by the app's key handler first: while recording, a key press is captured here. */
export function captureRecording(e: KeyboardEvent): boolean {
  if (!recording) return false;
  e.preventDefault();
  e.stopPropagation();
  if (e.key === 'Escape') {
    recording = null;
    render();
    return true;
  }
  const combo = comboOf(e);
  if (!combo) return true;
  if (!usable(combo)) {
    toast('warning', `${combo} cannot be used`, 'Use a combination with Ctrl or Alt (Ctrl+R and the text-editing keys are reserved).');
    return true;
  }
  const b = p().bindings.find((x) => x.command === recording);
  if (b) b.combo = normalize(combo);
  recording = null;
  render();
  return true;
}

function integrations(): Child {
  const kinds: ['document' | 'spreadsheet' | 'presentation', string, AddinHost, AddinHost][] = [
    ['document', 'Documents', 'word', 'google-docs'],
    ['spreadsheet', 'Spreadsheets', 'excel', 'google-sheets'],
    ['presentation', 'Presentations', 'powerpoint', 'google-slides'],
  ];
  return [
    section('External editor', 'GitHub Desktop’s “external editor”: where “Open in …” sends each kind of file. The client does not edit files itself.', rows(
      kinds.map(([k, label, office, google]) => row(label, null, select(p().editorFor[k], [['office', HOST_LABEL[office]], ['google', HOST_LABEL[google]]], (v) => set(() => (p().editorFor[k] = v)), { label }))),
    )),
    section('Add-ins', 'Install state as Dynodoc knows it: a file is connected when its add-in holds a live connection key.', h('div', { class: 'list-card' },
      CONNECTORS.filter((c) => c.host).map((c) => {
        const n = state.db.docs.filter((d) => d.connections.some((k) => k.host === c.host && !k.revoked && k.expiresAt > Date.now())).length;
        return h('div', { class: 'list-item' }, h('div', { class: 'grow' }, h('strong', null, c.name), h('div', { class: 'muted small' }, c.caveat)), pill(n ? `${n} connected` : 'not connected', n ? 'success' : 'neutral'), button('Install…', { size: 'sm', onClick: () => installSteps(c) }));
      }))),
  ];
}

function versionControl(): Child {
  return [
    section('Author', 'GitHub Desktop’s Git author. Dynodoc records the account that signed in; these are shown on commits made from this computer.', rows(
      row('Name', null, input(p().authorName, (v) => { p().authorName = v; }, { id: 'vc-name', label: 'Author name' })),
      row('Email', null, input(p().authorEmail, (v) => { p().authorEmail = v; }, { id: 'vc-email', label: 'Author email', type: 'email' })),
      row('Sign commits with this computer’s key', 'Pushes carry a signature from the device key in Accounts.', toggle(p().signCommits, (v) => set(() => (p().signCommits = v)), 'Sign commits')),
    )),
    section('Branches', null, rows(
      row('Default branch', 'The team version of every file is called main.', h('code', null, 'main'), { fixed: true }),
      row('Drafts', 'A draft is a private branch of one file. Rename, squash and stash do not exist in Dynodoc.', null, { fixed: true }),
      row('Hooks', 'Dynodoc runs no hooks on this computer. Review rules and the agent policy play that part on the server.', null, { fixed: true }),
    )),
    section('Diff and history', 'Git GUI’s options, as they apply to documents.', rows(
      row('Context', 'Unchanged blocks shown around a change.', select(String(p().diffContext), [['0', 'None'], ['1', '1 block'], ['2', '2 blocks'], ['3', '3 blocks'], ['5', '5 blocks'], ['999', 'Whole file']], (v) => set(() => (p().diffContext = Number(v))), { label: 'Context' })),
      row('Who changed this', 'Git GUI’s blame: show the author of each block in History.', toggle(p().blame, (v) => set(() => (p().blame = v)), 'Blame')),
      row('Encoding', 'Text in Dynodoc is Unicode throughout.', h('code', null, 'UTF-8'), { fixed: true }),
    )),
  ];
}

function notifications(): Child {
  const n = p().notify;
  const t = (key: keyof typeof n, label: string, desc: string) => row(label, desc, toggle(n[key], (v) => set(() => (n[key] = v)), label, { disabled: key !== 'enabled' && !n.enabled }));
  return section('Notifications', 'What appears under Activity. The desktop window asks the system for no notification permission.', rows(
    t('enabled', 'Enable notifications', 'High-signal events in projects you watch.'),
    t('requests', 'Change requests', 'Opened, or proposed by an agent key.'),
    t('reviews', 'Reviews', 'Approvals and requests for changes on your requests.'),
    t('merges', 'Merges and declines', 'Your requests merged or declined.'),
    t('copies', 'Copies and similar files', 'Dynodoc found a copy of a file you own.'),
    t('access', 'Access', 'Someone gave you access to a project.'),
  ));
}

function prompts(): Child {
  const q = p().prompts;
  const c = (key: 'discard' | 'withdraw' | 'merge' | 'decline' | 'recovery', label: string) => row(label, null, toggle(q[key], (v) => set(() => (q[key] = v)), label));
  return [
    section('Show a confirmation before…', null, rows(
      c('discard', 'Discarding captured changes'),
      c('withdraw', 'Withdrawing a draft'),
      c('merge', 'Merging a change request'),
      c('decline', 'Declining a change request (always asks for a note)'),
      c('recovery', 'Reverting or cherry-picking a revision'),
    )),
    section('If I have changes and I switch branches…', null, h('div', { class: 'radios card pad' },
      ([['ask', 'Ask me where I want the changes to go'], ['leave', 'Always leave my changes on the current branch'], ['bring', 'Always bring my changes to the new branch']] as const).map(([v, l]) =>
        h('label', { class: cx('radio', v === 'bring' && 'is-disabled') }, h('input', { type: 'radio', name: 'switch', checked: q.switchWithChanges === v, disabled: v === 'bring', onChange: () => set(() => (q.switchWithChanges = v)) }), l, v === 'bring' ? h('span', { class: 'muted small' }, ' (not in Dynodoc: captured edits belong to the file and branch the add-in was on)') : null)))),
    section('Commit length', null, rows(row('Show commit length warning', 'Summaries over 60 characters turn the counter red.', toggle(q.commitLength, (v) => set(() => (q.commitLength = v)), 'Commit length warning')))),
  ];
}

let keyDraft: Record<string, string> = {};
let passDraft = '';
let capDraft: { thread: string; day: string; cost: string } | null = null;
let priceDraft = { model: '', input: '', output: '' };

const n0 = (v: number) => v.toLocaleString('en-US');
const dollars = (micros: number) => (micros / 1_000_000).toString();

function hostOf(base: string): string {
  try {
    return new URL(base).host;
  } catch {
    return '';
  }
}

/** Offline, limits, prices and what the last 24 hours used (desktop app only). */
function agentLimits(): Child {
  const cfg = state.agentCfg;
  if (!cfg) return section('Limits', null, h('p', { class: 'muted small' }, 'Reading the agent’s settings…'));
  if (!capDraft) capDraft = { thread: String(cfg.thread_token_cap), day: String(cfg.day_token_cap), cost: dollars(cfg.day_cost_cap) };
  const draft = capDraft;
  const whole = (v: string) => (/^\d{1,13}$/.test(v.trim()) ? Number(v.trim()) : null);
  const saveCaps = async () => {
    const t = whole(draft.thread);
    const d = whole(draft.day);
    const c = /^\d{1,7}(\.\d{1,6})?$/.test(draft.cost.trim()) ? Math.round(Number(draft.cost.trim()) * 1_000_000) : null;
    if (t === null || d === null || c === null) return toast('warning', 'Limits are whole numbers', 'Tokens are whole numbers; the cost is in dollars. 0 means no model is called.');
    if (await setAgentCfg({ thread_token_cap: t, day_token_cap: d, day_cost_cap: c })) {
      capDraft = null;
      toast('success', 'Limits saved');
    }
  };
  const prices = Object.entries(cfg.prices);
  const models = [...new Set(p().ladder.map((e) => e.model))];
  const savePrice = async () => {
    const m = priceDraft.model.trim();
    const i = Number(priceDraft.input);
    const o = Number(priceDraft.output);
    if (!m || !priceDraft.input.trim() || !priceDraft.output.trim() || !Number.isFinite(i) || !Number.isFinite(o) || i < 0 || o < 0) return toast('warning', 'Name the model and give both prices', 'Dollars per million tokens, for example 3 and 15.');
    const next: Record<string, { input: number; output: number }> = { ...cfg.prices, [m]: { input: Math.round(i * 1_000_000), output: Math.round(o * 1_000_000) } };
    if (await setAgentCfg({ prices: next })) priceDraft = { model: '', input: '', output: '' };
  };
  return [
    section('Offline', 'One switch for every model call. While it is on the agent answers from prepared replies only.',
      rows(row('Offline: call no model', 'Takes effect at once, including for a run in progress at its next step.', toggle(cfg.offline, (v) => { void setAgentCfg({ offline: v }); }, 'Offline')))),
    section('Limits', 'A model call is refused once a limit is reached. A limit of 0 means no model is called at all. They count what the providers report.',
      rows(
        row('Tokens per thread', null, input(draft.thread, (v) => { draft.thread = v; }, { id: 'cap-thread', label: 'Tokens per thread' })),
        row('Tokens per 24 hours', null, input(draft.day, (v) => { draft.day = v; }, { id: 'cap-day', label: 'Tokens per 24 hours' })),
        row('Estimated cost per 24 hours (US dollars)', 'Needs a price for the model below; without one the cost is not estimated and this limit never trips.', input(draft.cost, (v) => { draft.cost = v; }, { id: 'cap-cost', label: 'Cost per 24 hours' })),
      ),
      h('div', { class: 'row-actions' }, button('Save limits', { variant: 'primary', size: 'sm', id: 'caps-save', onClick: () => { void saveCaps(); } }))),
    section('Prices', 'Dynodoc ships no prices (a price it made up would be a claim about somebody’s pricing). Enter the ones you pay, in US dollars per million tokens, and usage shows an estimate.',
      prices.length
        ? h('table', { class: 'table card' }, h('thead', null, h('tr', null, h('th', null, 'Model'), h('th', null, 'Input per million'), h('th', null, 'Output per million'), h('th', null, ''))),
            h('tbody', null, prices.map(([m, v]) => h('tr', null, h('td', null, h('code', null, m)), h('td', null, `$${dollars(v.input)}`), h('td', null, `$${dollars(v.output)}`),
              h('td', null, iconButton('trash', `Remove the price of ${m}`, () => { const next = { ...cfg.prices }; delete next[m]; void setAgentCfg({ prices: next }); }))))))
        : h('p', { class: 'muted small' }, 'No prices set.'),
      h('div', { class: 'price-form' },
        field('Model', input(priceDraft.model, (v) => { priceDraft.model = v; }, { id: 'price-model', label: 'Model for the price', placeholder: models[0] ?? 'model id' })),
        field('Input, $ per million', input(priceDraft.input, (v) => { priceDraft.input = v; }, { id: 'price-in', label: 'Input price' })),
        field('Output, $ per million', input(priceDraft.output, (v) => { priceDraft.output = v; }, { id: 'price-out', label: 'Output price' })),
        button('Set price', { size: 'sm', id: 'price-save', onClick: () => { void savePrice(); } }))),
    section('Used in the last 24 hours', null, h('div', { class: 'stats' },
      h('div', { class: 'stat card' }, h('div', { class: 'muted small' }, 'Tokens'), h('div', { class: 'stat-value', id: 'used-tokens' }, `${n0(cfg.day_tokens)} of ${n0(cfg.day_token_cap)}`)),
      h('div', { class: 'stat card' }, h('div', { class: 'muted small' }, 'Estimated cost'), h('div', { class: 'stat-value' }, cfg.day_cost_micros > 0 ? `$${(cfg.day_cost_micros / 1_000_000).toFixed(2)}` : 'not estimated')))),
  ];
}

function providers(): Child {
  const list = p().ladder;
  const issues = validate(list);
  const use = active(list);
  const cfg = state.agentCfg;
  const desktop = state.desktop;
  const move = (i: number, d: number) => { const j = i + d; if (j < 0 || j >= list.length) return; [list[i], list[j]] = [list[j], list[i]]; render(); };
  const forget = (e: LadderEntry) => {
    if (desktop) { void forgetKey(e); return; }
    state.keys.delete(e.id);
    e.keyHint = '';
    render();
  };
  const store = (e: LadderEntry) => {
    const k = (keyDraft[e.id] ?? '').trim();
    if (!k) return;
    keyDraft[e.id] = '';
    if (desktop) { void storeKey(e, k); return; }
    state.keys.set(e.id, k);
    e.keyHint = keyHint(k);
    render();
  };
  return [
    section('Provider ladder', desktop ? 'Use an API key or a supported installed agent’s existing sign-in. A new thread uses the first enabled entry that is complete; a thread can pick another in its composer. Requests go from this computer straight to the provider, and only after you have seen what would be sent and allowed that address once.' : 'Bring your own key. The agent would use the first enabled entry that is complete. A hosted copy calls no model: the ladder is configured and checked, and the agent’s replies are prepared.',
      notice('info', desktop
        ? `Keys are kept in ${cfg?.keys_where ?? 'the operating system’s credential store'}. They are never written to the preferences file, a log or this window: after you store one, only “stored” and its last four characters are shown.`
        : 'Keys stay in this tab’s memory: never stored in the browser, never shown again (only “stored” and the last four characters), and never sent anywhere.'),
      desktop && cfg?.keys_locked
        ? h('div', { class: 'card pad key-unlock' },
            h('strong', null, cfg.keys_file_exists ? 'Enter the passphrase for your keys' : 'Choose a passphrase for your keys'),
            h('p', { class: 'muted small' }, 'No credential store was found on this computer, so keys are kept in one file encrypted with a passphrase. It is asked once each time the app starts and is never saved. At least 8 characters.'),
            h('div', { class: 'key-entry' },
              h('input', { type: 'password', class: 'input', id: 'key-pass', autocomplete: 'off', 'aria-label': 'Passphrase for the keys', onInput: (ev: Event) => { passDraft = (ev.target as HTMLInputElement).value; } }),
              button(cfg.keys_file_exists ? 'Unlock' : 'Create', { size: 'sm', id: 'key-unlock', onClick: async () => { const ok = await unlockKeys(passDraft); passDraft = ''; if (ok) render(); } })))
        : null,
      h('div', { class: 'ladder' }, list.map((e, i) => {
        const info = PROVIDERS.find((x) => x.kind === e.kind)!;
        const mine = issues.filter((x) => x.id === e.id);
        const consent = cfg?.consent[e.id];
        const host = hostOf(e.base);
        return h('div', { class: cx('ladder-row card', use?.id === e.id && 'is-active'), 'data-entry': e.id },
          h('div', { class: 'ladder-top' },
            h('span', { class: 'ladder-rank' }, String(i + 1)),
            select(e.kind, PROVIDERS.map((x) => [x.kind, x.label] as [ProviderKind, string]), (v) => { const pi = PROVIDERS.find((x) => x.kind === v)!; dropEntry(e); e.kind = v; e.model = pi.model; e.base = pi.base; e.keyHint = ''; state.keys.delete(e.id); render(); }, { label: 'Provider' }),
            use?.id === e.id ? pill('in use', 'success') : null,
            h('span', { class: 'spacer' }),
            toggle(e.enabled, (v) => { e.enabled = v; render(); }, `Enable ${info.label}`),
            iconButton('up', 'Move up', () => move(i, -1), { disabled: i === 0 }),
            iconButton('down', 'Move down', () => move(i, 1), { disabled: i === list.length - 1 }),
            iconButton('trash', 'Remove', () => { dropEntry(e); state.keys.delete(e.id); p().ladder = list.filter((x) => x !== e); render(); })),
          h('div', { class: 'ladder-fields' },
            field('Model', input(e.model, (v) => { e.model = v; }, { id: `lm-${e.id}` })),
            ['codex','claude','cursor','antigravity','grok'].includes(e.kind) ? field('Account', h('span', { class: 'muted small' }, e.kind === 'grok' ? 'Set GROK_HOME to <your home>/.verne/local-agents/grok and run grok login once. Grok Build owns this separate sign-in.' : 'Uses the sign-in saved by the installed agent.')) : field('Base address', input(e.base, (v) => { e.base = v; }, { id: `lb-${e.id}` })),
            info.needsKey
              ? field('API key', e.keyHint
                  ? h('div', { class: 'key-stored' }, pill(`stored ····${e.keyHint}`, 'success'), button('Forget', { size: 'sm', variant: 'ghost', onClick: () => forget(e) }))
                  : h('div', { class: 'key-entry' }, h('input', { type: 'password', class: 'input', id: `lk-${e.id}`, autocomplete: 'off', placeholder: 'Paste a key', 'aria-label': `${info.label} API key`, onInput: (ev: Event) => { keyDraft[e.id] = (ev.target as HTMLInputElement).value; } }),
                      button('Store', { size: 'sm', id: `lks-${e.id}`, onClick: () => store(e) })))
              : field(e.kind === 'ollama' ? 'API key' : 'Account', h('span', { class: 'muted small' }, e.kind === 'ollama' ? 'None: Ollama runs on this computer.' : 'Uses the installed agent’s own sign-in. Sign in there first. Your plan limits apply; content can leave this computer.'))),
          desktop
            ? h('div', { class: 'ladder-consent small' },
                consent && consent.host === host
                  ? [pill(`allowed: ${host.includes('.local-agent.invalid') ? info.label : consent.host}`, 'success'), button('Withdraw', { size: 'sm', variant: 'ghost', onClick: () => { void setConsent(e.id, null, false); } })]
                  : h('span', { class: 'muted' }, consent ? `Allowed for ${consent.host}; the address changed, so you are asked again before the next send.` : 'Not allowed yet: before the first send you see exactly what would go to this address, and decide.'))
            : null,
          mine.length ? notice('warning', mine.map((x) => x.message).join(' ')) : null);
      })),
      h('div', { class: 'row-actions' }, select('', [['', 'Add a provider…'], ...PROVIDERS.map((x) => [x.kind, x.label] as [string, string])], (v) => { if (!v) return; const pi = PROVIDERS.find((x) => x.kind === v)!; list.push({ id: `lp${Date.now().toString(36)}`, kind: pi.kind, enabled: false, model: pi.model, base: pi.base, keyHint: '' }); render(); }, { label: 'Add a provider' }))),
    desktop ? agentLimits() : null,
  ];
}

function agent(): Child {
  return [
    section('Default access', 'New threads start at this level. A thread can be raised or lowered by you, never by the agent.', segmented(p().agentAccess, [['read', 'Read only'], ['propose', 'Propose']], (v) => { p().agentAccess = v; render(); }, 'Default access'), h('p', { class: 'muted small' }, ACCESS_INFO[p().agentAccess].blurb)),
    section('Tools', 'The closed set the agent can call. Switch a tool off and a call to it is refused and shown in the thread.', h('table', { class: 'table card' },
      h('thead', null, h('tr', null, h('th', null, 'Tool'), h('th', null, 'Needs'), h('th', null, 'What it does'), h('th', null, 'Reads'), h('th', null, 'On'))),
      h('tbody', null, TOOLS.map((t) => h('tr', null, h('td', null, h('code', null, t.name)), h('td', null, badge(ACCESS_INFO[t.access].label, t.access === 'read' ? 'neutral' : 'info')), h('td', null, t.summary), h('td', { class: 'mono small muted' }, t.endpoint),
        h('td', null, toggle(p().agentTools.includes(t.name), (v) => { p().agentTools = v ? [...p().agentTools, t.name] : p().agentTools.filter((x) => x !== t.name); render(); }, `Enable ${t.name}`))))))),
    section('No setting changes these', null, rows(NEVER.map((n) => row(n, null, null, { fixed: true })))),
    section('Instructions', 'Given to the agent at the start of every thread.', textarea(p().instructions, (v) => { p().instructions = v; }, { id: 'agent-instructions', rows: 6 }), h('div', { class: 'row-actions' }, button('Restore default', { size: 'sm', onClick: () => { p().instructions = DEFAULT_INSTRUCTIONS; render(); } }))),
  ];
}

function skillDialog(sk: Skill | null): void {
  const draft: Skill = sk ? { ...sk, tools: [...sk.tools] } : { id: `sk_${Date.now().toString(36)}`, name: '', description: '', enabled: true, tools: [], body: '', builtIn: false };
  openDialog({
    title: sk ? `Skill: ${sk.name}` : 'New skill',
    width: 640,
    body: () => [
      field('Name', input(draft.name, (v) => { draft.name = v; }, { id: 'sk-name', autofocus: !sk })),
      field('Description', input(draft.description, (v) => { draft.description = v; }, { id: 'sk-desc' })),
      field('Tools it may use', h('div', { class: 'chips' }, TOOLS.map((t) => h('label', { class: cx('chip', draft.tools.includes(t.name) && 'is-on') }, h('input', { type: 'checkbox', checked: draft.tools.includes(t.name), onChange: (e: Event) => { draft.tools = (e.target as HTMLInputElement).checked ? [...draft.tools, t.name] : draft.tools.filter((x) => x !== t.name); render(); } }), t.name)))),
      field('Steps', textarea(draft.body, (v) => { draft.body = v; }, { id: 'sk-body', rows: 6 }), 'Plain text the agent follows. A skill cannot add a tool the table does not have.'),
    ],
    actions: () => [
      sk && !sk.builtIn ? button('Delete', { variant: 'danger', onClick: () => { p().skills = p().skills.filter((x) => x.id !== sk.id); closeDialog(); } }) : null,
      h('span', { class: 'spacer' }),
      button('Cancel', { onClick: closeDialog }),
      button(sk ? 'Save skill' : 'Add skill', { variant: 'primary', onClick: () => {
        if (!draft.name.trim()) return toast('warning', 'Name the skill');
        if (sk) Object.assign(sk, draft); else p().skills.push(draft);
        closeDialog();
      } }),
    ],
  });
}

function skills(): Child {
  return section('Skills', 'Reusable instructions the agent can follow. Enable, inspect and edit them; a skill uses only tools from the closed table.',
    h('div', { class: 'list-card' }, p().skills.map((sk) => h('div', { class: 'list-item' },
      icon('skill', 16),
      h('div', { class: 'grow' }, h('div', null, h('strong', null, sk.name), ' ', sk.builtIn ? badge('built in') : badge('yours', 'info')), h('div', { class: 'muted small' }, sk.description), h('div', { class: 'mono small muted' }, sk.tools.join(' · '))),
      button(sk.builtIn ? 'Inspect' : 'Edit', { size: 'sm', variant: 'ghost', onClick: () => skillDialog(sk) }),
      toggle(sk.enabled, (v) => { sk.enabled = v; render(); }, `Enable ${sk.name}`)))),
    h('div', { class: 'row-actions' }, button('New skill', { icon: 'plus', size: 'sm', onClick: () => skillDialog(null) })));
}

function connectors(): Child {
  return section('Connector catalogue', 'Every system Dynodoc reaches, with its sign-in and status. The full catalogue is under Connectors.',
    h('table', { class: 'table card' }, h('thead', null, h('tr', null, h('th', null, 'Connector'), h('th', null, 'Kind'), h('th', null, 'Sign-in'), h('th', null, 'Status'))),
      h('tbody', null, CONNECTORS.map((c) => h('tr', null, h('td', null, h('strong', null, c.name)), h('td', { class: 'muted' }, c.category), h('td', { class: 'small' }, c.auth), h('td', null, pill(c.status, 'success', { mono: false })))))),
    h('div', { class: 'row-actions' }, button('Open Connectors', { size: 'sm', onClick: () => { state.ui.view = 'connectors'; render(); } })));
}

function team(): Child {
  return [
    section('Workspaces', 'Organisations, teams and folders, as in the web workspace’s “Access & folders”. Access set on a workspace is inherited by its projects.',
      h('div', { class: 'list-card' }, state.db.spaces.map((s) => h('div', { class: 'space' },
        h('div', { class: 'space-head' }, icon(s.kind === 'organization' ? 'admin' : 'users', 15), h('strong', null, s.name), badge(s.kind === 'organization' ? 'Organisation' : s.kind === 'team' ? 'Team' : 'Folder'), s.parentId ? h('span', { class: 'muted small' }, `in ${state.db.spaces.find((x) => x.id === s.parentId)?.name}`) : null),
        h('div', { class: 'space-members' }, s.members.map((m) => h('span', { class: 'member-chip' }, avatar(personName(m.personId)), personName(m.personId), h('span', { class: 'muted' }, ROLE_LABEL[m.role])))))))),
    section('Roles', 'The six roles, with the words the interface uses.', h('table', { class: 'table card' },
      h('thead', null, h('tr', null, h('th', null, 'Role'), h('th', null, 'Read'), h('th', null, 'Comment'), h('th', null, 'Propose'), h('th', null, 'Approve'), h('th', null, 'Write main'), h('th', null, 'Merge'), h('th', null, 'Manage'))),
      h('tbody', null, (['owner', 'manager', 'editor', 'approver', 'reviewer', 'viewer'] as Role[]).map((r) => {
        const yes = (b: boolean) => h('td', null, b ? icon('check', 13) : h('span', { class: 'muted' }, '—'));
        return h('tr', null, h('td', null, h('strong', null, ROLE_LABEL[r])), yes(true), yes(r !== 'viewer'), yes(r !== 'viewer'), yes(['owner', 'manager', 'editor', 'approver'].includes(r)), yes(['owner', 'manager', 'editor'].includes(r)), yes(['owner', 'manager', 'editor'].includes(r)), yes(['owner', 'manager'].includes(r)));
      })))),
  ];
}

function keys(): Child {
  return [
    section('Signing keys', 'Keys enrolled to your account (the web workspace’s Profile, Signing keys). A person’s key signs history; an agent key only reads documents.',
      h('div', { class: 'list-card' }, state.db.deviceKeys.map((k) => h('div', { class: 'list-item' },
        icon(k.kind === 'agent' ? 'agent' : 'key', 16),
        h('div', { class: 'grow' }, h('div', null, h('strong', null, k.name), ' ', badge(k.kind === 'agent' ? 'AI agent' : 'person', k.kind === 'agent' ? 'info' : 'neutral')), h('div', { class: 'mono small muted' }, `${k.publicKey.slice(0, 24)}…`), h('div', { class: 'small muted' }, `${k.scopes.join(', ')} · enrolled ${when(k.createdAt)}`)),
        k.revoked ? pill('revoked') : button('Revoke', { size: 'sm', variant: 'ghost', onClick: () => confirmDialog('Revoke key', h('p', null, `${k.name} can no longer sign or read. Revisions it signed stay valid in history.`), 'Revoke key', () => { k.revoked = true; render(); }, { danger: true }) }))))),
    section('This computer’s key', null, state.desktop ? notice('info', 'Create or show this computer’s key under Accounts, Dynodoc server. The private part is kept by the operating system’s key store; enrolling happens in the web workspace.') : notice('info', 'The desktop app creates a key for the computer it runs on and keeps it in the operating system’s key store.')),
  ];
}

function admin(): Child {
  return [
    notice('info', 'For administrators of a self-hosted server. Shown from sample data and read only; the real controls are in the web workspace under /admin.'),
    section('At a glance', null, h('div', { class: 'stat-grid' },
      [['Services running', '6 of 6'], ['Latest local backup', '3 h ago, checksum verified'], ['Server disk free', '412 GB'], ['Database size', '1.8 GB']].map(([k, v]) => h('div', { class: 'stat card' }, h('div', { class: 'muted small' }, k), h('div', { class: 'stat-value' }, v))))),
    section('People, content and access', null, h('table', { class: 'table card' },
      h('thead', null, h('tr', null, h('th', null, 'Account'), h('th', null, 'Projects'), h('th', null, 'Last active'), h('th', null, ''))),
      h('tbody', null, state.db.people.filter((x) => !x.agent).map((x, i) => h('tr', null, h('td', null, h('strong', null, x.name), h('div', { class: 'muted small' }, x.email)), h('td', null, String(state.db.projects.filter((pr) => pr.members.some((m) => m.personId === x.id)).length)), h('td', { class: 'muted' }, when(Date.now() - (i + 1) * 3_600_000 * 5)), h('td', { class: 'right' }, button('Sign out all sessions', { size: 'sm', variant: 'ghost', disabled: true, title: 'In the web workspace’s administration only' }))))))),
  ];
}

function advanced(): Child {
  return [
    section('Background updates', null, rows(
      row('Fetch in the background', 'Look for new revisions and reviews while the client is open.', toggle(p().backgroundFetch, (v) => set(() => (p().backgroundFetch = v)), 'Background fetch')),
      row('Every', null, select(String(p().fetchMinutes), [['5', '5 minutes'], ['10', '10 minutes'], ['30', '30 minutes'], ['60', 'Hour']], (v) => set(() => (p().fetchMinutes = Number(v))), { label: 'Fetch interval', disabled: !p().backgroundFetch })),
      row('Show status icons in the project list', 'Marks projects with local commits or open requests.', toggle(p().statusIcons, (v) => set(() => (p().statusIcons = v)), 'Status icons')),
    )),
    section('Usage', null, rows(row('Send usage statistics', 'Off by default. This release has no telemetry at all: the switch only records your choice.', toggle(p().usageStats, (v) => set(() => (p().usageStats = v)), 'Usage statistics')))),
    section(isLive() ? 'Settings' : 'Sample data', null, rows(
      isLive() ? null : row('Reset the sample project', 'Brings back the sample history, drafts and requests as they were at start.', button('Reset', { size: 'sm', onClick: () => confirmDialog('Reset sample data', h('p', null, 'Every change you made to the sample is lost.'), 'Reset', () => { resetSample(); toast('success', 'Sample data reset'); }) })),
      row('Restore default settings', null, button('Restore', { size: 'sm', onClick: () => { const keep = p().ladder; state.prefs = JSON.parse(JSON.stringify(DEFAULT_PREFS)); state.prefs.ladder = keep; applyAppearance(); render(); } })),
    )),
  ];
}

function accessibility(): Child {
  return section('Accessibility', null, rows(
    row('Underline links', 'In messages, comments and text.', toggle(p().underlineLinks, (v) => set(() => (p().underlineLinks = v)), 'Underline links')),
    row('Show check marks in the diff', 'A mark beside each changed block, besides its colour.', toggle(p().checkMarks, (v) => set(() => (p().checkMarks = v)), 'Check marks')),
    row('Reduce motion', 'No transitions or animations.', toggle(p().reduceMotion, (v) => set(() => (p().reduceMotion = v)), 'Reduce motion')),
    row('States are words', 'Every state is printed as a word; colour is a second channel.', null, { fixed: true }),
  ));
}

const RENDER: Record<SettingsSection, () => Child> = {
  general, accounts, workspace: workspaceSection, appearance, keybindings, integrations, versioncontrol: versionControl, notifications, prompts, providers, agent, skills, connectors, team, keys, admin, advanced, accessibility, about: aboutBody,
};

export function settingsView(): HTMLElement {
  const ui = state.ui.settings;
  const q = ui.filter.toLowerCase();
  const groups = [...new Set(SECTIONS.map((s) => s.group))];
  const current = SECTIONS.find((s) => s.id === ui.section) ?? SECTIONS[0];
  return h(
    'div',
    { class: 'settings' },
    h('nav', { class: 'settings-nav', 'aria-label': 'Settings sections' },
      h('div', { class: 'pane-filter' }, icon('search', 14), input(ui.filter, (v) => { ui.filter = v; render(); }, { id: 'settings-filter', placeholder: 'Search settings' })),
      groups.map((g) => {
        const items = SECTIONS.filter((s) => s.group === g && s.label.toLowerCase().includes(q));
        if (!items.length) return null;
        return [h('div', { class: 'nav-group' }, g), items.map((s) => h('button', { type: 'button', class: cx('nav-row', s.id === ui.section && 'is-selected'), 'aria-current': s.id === ui.section ? 'page' : undefined, 'data-section': s.id, onClick: () => { ui.section = s.id; if (s.id === 'providers' && state.desktop) void loadAgentCfg().then(render); if (s.id === 'accounts' && state.desktop) void loadServers(); render(); } }, icon(s.icon, 15), h('span', null, s.label)))];
      })),
    h('div', { class: 'settings-body', 'data-scroll-key': `settings-${ui.section}` }, h('div', { class: 'settings-inner' }, h('h1', { class: 'view-title' }, current.label), RENDER[current.id]())),
  );
}

export { isAgent };
void B;
