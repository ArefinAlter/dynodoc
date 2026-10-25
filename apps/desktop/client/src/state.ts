// Application state: the database (scripted backend), the interface state, and the preferences.
// Preferences live in memory for the life of the window. This page may not use the web view's
// storage (the launcher's policy test forbids it), so a restart brings back the defaults; the
// About page says so.
import { DEFAULT_BINDINGS, type Binding } from './lib/keys.ts';
import { DEFAULT_INSTRUCTIONS, TOOLS, type AgentAccess } from './lib/agentTools.ts';
import type { LadderEntry } from './lib/ladder.ts';
import type { FormatPrefs } from './lib/format.ts';
import type { Db, FileKind } from './data/types.ts';
import { seed } from './data/sample.ts';

export type View =
  | 'changes'
  | 'history'
  | 'requests'
  | 'documents'
  | 'projects'
  | 'connectors'
  | 'activity'
  | 'agent'
  | 'settings';

export type SettingsSection =
  | 'general'
  | 'accounts'
  | 'appearance'
  | 'keybindings'
  | 'integrations'
  | 'versioncontrol'
  | 'notifications'
  | 'prompts'
  | 'providers'
  | 'agent'
  | 'skills'
  | 'connectors'
  | 'team'
  | 'keys'
  | 'admin'
  | 'advanced'
  | 'accessibility'
  | 'about';

export interface Skill {
  id: string;
  name: string;
  description: string;
  enabled: boolean;
  tools: string[];
  body: string;
  builtIn: boolean;
}

export interface Prefs extends FormatPrefs {
  theme: 'system' | 'light' | 'dark';
  palette: 'neutral' | 'paper' | 'slate' | 'contrast';
  accent: string;
  density: 'comfortable' | 'compact';
  fontSize: 'small' | 'default' | 'large';
  diffLayout: 'unified' | 'split';
  diffContext: number;
  wordDiff: boolean;
  tabSize: number;
  authorName: string;
  authorEmail: string;
  signCommits: boolean;
  blame: boolean;
  editorFor: Record<FileKind, 'office' | 'google'>;
  notify: { enabled: boolean; requests: boolean; reviews: boolean; merges: boolean; copies: boolean; access: boolean };
  prompts: { discard: boolean; withdraw: boolean; merge: boolean; decline: boolean; recovery: boolean; switchWithChanges: 'ask' | 'bring' | 'leave'; commitLength: boolean };
  backgroundFetch: boolean;
  fetchMinutes: number;
  usageStats: boolean;
  statusIcons: boolean;
  underlineLinks: boolean;
  checkMarks: boolean;
  reduceMotion: boolean;
  bindings: Binding[];
  agentTools: string[];
  agentAccess: AgentAccess;
  instructions: string;
  ladder: LadderEntry[];
  skills: Skill[];
  sidebarOpen: boolean;
}

export const DEFAULT_PREFS: Prefs = {
  theme: 'system',
  palette: 'neutral',
  accent: '',
  density: 'comfortable',
  fontSize: 'default',
  dateFormat: 'medium',
  timeFormat: '24h',
  numberFormat: 'comma',
  absoluteDates: false,
  diffLayout: 'unified',
  diffContext: 2,
  wordDiff: true,
  tabSize: 4,
  authorName: 'Jordan Lee',
  authorEmail: 'jordan.lee@example.org',
  signCommits: true,
  blame: true,
  editorFor: { document: 'office', spreadsheet: 'office', presentation: 'office' },
  notify: { enabled: true, requests: true, reviews: true, merges: true, copies: true, access: true },
  prompts: { discard: true, withdraw: true, merge: true, decline: true, recovery: true, switchWithChanges: 'ask', commitLength: true },
  backgroundFetch: true,
  fetchMinutes: 10,
  usageStats: false,
  statusIcons: true,
  underlineLinks: false,
  checkMarks: true,
  reduceMotion: false,
  bindings: DEFAULT_BINDINGS.map((b) => ({ ...b })),
  agentTools: TOOLS.map((t) => t.name),
  agentAccess: 'read',
  instructions: DEFAULT_INSTRUCTIONS,
  ladder: [
    { id: 'lp1', kind: 'anthropic', enabled: false, model: 'claude-sonnet-4-5', base: 'https://api.anthropic.com', keyHint: '' },
    { id: 'lp2', kind: 'openai', enabled: false, model: 'gpt-5', base: 'https://api.openai.com/v1', keyHint: '' },
    { id: 'lp3', kind: 'ollama', enabled: false, model: 'llama3.1', base: 'http://127.0.0.1:11434', keyHint: '' },
  ],
  skills: [
    { id: 'sk_summary', name: 'Summarise a change request', description: 'What a request changes, who reviewed it and what is left before it can merge.', enabled: true, tools: ['requests.list', 'requests.summarise', 'diff.get'], builtIn: true, body: 'Read the request and its diff.\nList the changed blocks by label, then the review state.\nEnd with what a person must still do (approvals, conflicts).' },
    { id: 'sk_budget', name: 'Check a budget change', description: 'Looks for totals that no longer add up after a cell edit.', enabled: true, tools: ['diff.get', 'documents.outline'], builtIn: true, body: 'For each edited cell, find the total that sums it.\nSay whether the total was edited by the same change.\nNever change a figure yourself.' },
    { id: 'sk_commit', name: 'Draft a commit summary', description: 'A summary of up to 60 characters and a short description from the staged changes.', enabled: true, tools: ['commit.draft_summary'], builtIn: true, body: 'Summary: imperative, at most 60 characters.\nDescription: one line per file, what changed and why if the text says.' },
    { id: 'sk_chain', name: 'Check the hash chain', description: 'Recomputes a file’s chain and reports the first revision that does not verify.', enabled: true, tools: ['history.verify', 'history.read'], builtIn: true, body: 'Verify the chain of the file asked about.\nQuote the revision and the hash prefix of any failure.' },
    { id: 'sk_tone', name: 'Kind review comments', description: 'Keeps proposed review comments short, specific and kind.', enabled: false, tools: ['review.propose_comment'], builtIn: false, body: 'One point per comment.\nQuote the words you mean.\nSuggest, do not order.' },
  ],
  sidebarOpen: true,
};

export interface Toast {
  id: number;
  kind: 'info' | 'success' | 'warning' | 'error';
  title: string;
  text?: string;
}

export interface ToolCall {
  tool: string;
  input: string;
  output: string;
  refused?: string;
  ms: number;
}

export interface AgentCard {
  kind: 'summary' | 'comment' | 'requests' | 'table' | 'verify';
  title: string;
  /** For a proposed comment: the request and the text a person may post. */
  requestId?: string;
  blockId?: string;
  text?: string;
  description?: string;
  rows?: string[][];
  columns?: string[];
  /** Pressed by the person: then it shows what happened. */
  done?: string;
}

export interface AgentMessage {
  id: string;
  role: 'user' | 'agent';
  text: string;
  at: number;
  calls?: ToolCall[];
  card?: AgentCard;
}

export interface Thread {
  id: string;
  title: string;
  access: AgentAccess;
  messages: AgentMessage[];
  createdAt: number;
  working: boolean;
}

export interface UiState {
  view: View;
  projectId: string;
  branchByProject: Record<string, string>;
  changes: { selected: string | null; filter: string; summary: string; description: string; amend: boolean };
  history: { selected: string | null; mode: 'list' | 'compare' | 'graph'; filter: string; compareBranch: string; docFilter: string };
  requests: { filter: 'open' | 'merged' | 'closed'; selected: string | null; comment: string };
  documents: { selected: string | null; tab: 'overview' | 'versions' | 'drafts' | 'discussions' | 'people' | 'sync' };
  projects: { selected: string | null; tab: 'overview' | 'issues' | 'people' | 'rules' | 'agents' | 'activity'; filter: string; type: 'all' | 'owned' | 'shared' | 'starred'; issue: number | null };
  activity: { tab: 'notifications' | 'activity' };
  settings: { section: SettingsSection; filter: string };
  agent: { threads: Thread[]; active: string | null; draft: string; detailsOpen: boolean };
  connectors: { filter: string; category: string };
  verified: Record<string, { ok: boolean; at: number }>;
  toasts: Toast[];
  fetching: boolean;
  online: boolean;
}

export const state = {
  db: seed() as Db,
  ui: {
    view: 'changes',
    projectId: 'p_handbook',
    branchByProject: { p_handbook: 'main', p_field: 'main', p_grant: 'main' },
    changes: { selected: null, filter: '', summary: '', description: '', amend: false },
    history: { selected: null, mode: 'list', filter: '', compareBranch: '', docFilter: '' },
    requests: { filter: 'open', selected: null, comment: '' },
    documents: { selected: null, tab: 'overview' },
    projects: { selected: null, tab: 'overview', filter: '', type: 'all', issue: null },
    activity: { tab: 'notifications' },
    settings: { section: 'general', filter: '' },
    agent: { threads: [], active: null, draft: '', detailsOpen: true },
    connectors: { filter: '', category: 'all' },
    verified: {},
    toasts: [],
    fetching: false,
    online: true,
  } as UiState,
  prefs: structuredCloneSafe(DEFAULT_PREFS),
  /** Provider keys typed in Settings: in memory only, never rendered, never sent. */
  keys: new Map<string, string>(),
  /** The desktop shell's bridge, when this page runs inside the app's window. */
  desktop: false,
  /** The desktop app's version, from its get_state command. */
  desktopVersion: '',
};

function structuredCloneSafe<T>(v: T): T {
  return JSON.parse(JSON.stringify(v)) as T;
}

export function branchOf(projectId = state.ui.projectId): string {
  return state.ui.branchByProject[projectId] ?? 'main';
}

let scheduled = false;
let renderer: () => void = () => {};

export function setRenderer(fn: () => void): void {
  renderer = fn;
}

/** Re-renders on the next frame (several changes in one handler render once). */
export function render(): void {
  if (scheduled) return;
  scheduled = true;
  requestAnimationFrame(() => {
    scheduled = false;
    renderer();
  });
}

let toastId = 0;
export function toast(kind: Toast['kind'], title: string, text?: string): void {
  const id = ++toastId;
  state.ui.toasts = [...state.ui.toasts.slice(-3), { id, kind, title, text }];
  render();
  setTimeout(() => {
    state.ui.toasts = state.ui.toasts.filter((t) => t.id !== id);
    render();
  }, kind === 'error' ? 7000 : 4500);
}

export function go(view: View): void {
  state.ui.view = view;
  render();
}

export function resetSample(): void {
  state.db = seed();
  state.ui.verified = {};
  state.ui.changes = { selected: null, filter: '', summary: '', description: '', amend: false };
  state.ui.history.selected = null;
  state.ui.requests.selected = null;
  state.ui.branchByProject = { p_handbook: 'main', p_field: 'main', p_grant: 'main' };
  render();
}
