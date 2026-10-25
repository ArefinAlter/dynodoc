// Keyboard shortcuts: the default table (GitHub Desktop's where Dynodoc has the same action),
// how a key event becomes a combination string, and conflict detection for the Keybindings
// settings. Pure; tested in client/test/keys.test.mjs.

export interface Binding {
  command: string;
  label: string;
  group: string;
  combo: string;
}

export const DEFAULT_BINDINGS: Binding[] = [
  { command: 'palette', label: 'Command palette', group: 'General', combo: 'Ctrl+K' },
  { command: 'view.changes', label: 'Show Changes', group: 'View', combo: 'Ctrl+1' },
  { command: 'view.history', label: 'Show History', group: 'View', combo: 'Ctrl+2' },
  { command: 'view.requests', label: 'Show Change requests', group: 'View', combo: 'Ctrl+3' },
  { command: 'view.documents', label: 'Show Documents', group: 'View', combo: 'Ctrl+4' },
  { command: 'view.agent', label: 'Show Agent', group: 'View', combo: 'Ctrl+5' },
  { command: 'view.activity', label: 'Show Activity', group: 'View', combo: 'Ctrl+6' },
  { command: 'repo.list', label: 'Show project list', group: 'View', combo: 'Ctrl+T' },
  { command: 'branch.list', label: 'Show branch list', group: 'View', combo: 'Ctrl+B' },
  { command: 'sidebar.toggle', label: 'Toggle sidebar', group: 'View', combo: 'Ctrl+\\' },
  { command: 'settings', label: 'Settings', group: 'General', combo: 'Ctrl+,' },
  { command: 'theme.toggle', label: 'Toggle light and dark', group: 'General', combo: 'Ctrl+Shift+L' },
  { command: 'sync', label: 'Push or sync', group: 'Project', combo: 'Ctrl+Shift+P' },
  { command: 'fetch', label: 'Fetch', group: 'Project', combo: 'Ctrl+Shift+T' },
  { command: 'commit', label: 'Commit staged changes', group: 'Project', combo: 'Ctrl+Enter' },
  { command: 'stage.all', label: 'Stage all changes', group: 'Project', combo: 'Ctrl+Shift+A' },
  { command: 'branch.new', label: 'New draft', group: 'Branch', combo: 'Ctrl+Shift+N' },
  { command: 'branch.delete', label: 'Withdraw draft', group: 'Branch', combo: 'Ctrl+Shift+D' },
  { command: 'branch.discard', label: 'Discard all changes', group: 'Branch', combo: 'Ctrl+Shift+Backspace' },
  { command: 'branch.update', label: 'Update from main', group: 'Branch', combo: 'Ctrl+Shift+U' },
  { command: 'branch.compare', label: 'Compare with main', group: 'Branch', combo: 'Ctrl+Shift+B' },
  { command: 'branch.merge', label: 'Merge into main', group: 'Branch', combo: 'Ctrl+Shift+M' },
  { command: 'branch.rebase', label: 'Rebase onto main', group: 'Branch', combo: 'Ctrl+Shift+E' },
  { command: 'branch.request', label: 'Send for review', group: 'Branch', combo: 'Ctrl+Alt+R' },
  { command: 'agent.new', label: 'New agent thread', group: 'Agent', combo: 'Ctrl+Shift+O' },
];

const MODS = ['Ctrl', 'Alt', 'Shift', 'Meta'];

/** Names a key the way the table writes it. */
export function keyName(key: string): string {
  if (key === ' ') return 'Space';
  if (key.length === 1) return key.toUpperCase();
  const map: Record<string, string> = { Escape: 'Esc', ArrowUp: 'Up', ArrowDown: 'Down', ArrowLeft: 'Left', ArrowRight: 'Right' };
  return map[key] ?? key;
}

export interface KeyLike {
  key: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  code?: string;
}

/** "Ctrl+Shift+P" for a key event. Command on a Mac counts as Ctrl. A bare modifier is "". */
export function comboOf(e: KeyLike): string {
  if (['Control', 'Shift', 'Alt', 'Meta'].includes(e.key)) return '';
  let key = e.key;
  // Shift changes the character of digits and some marks; use the physical key for those.
  if (e.code && /^Digit[0-9]$/.test(e.code)) key = e.code.slice(5);
  if (e.code === 'Comma') key = ',';
  if (e.code === 'Backslash') key = '\\';
  const parts: string[] = [];
  if (e.ctrlKey || e.metaKey) parts.push('Ctrl');
  if (e.altKey) parts.push('Alt');
  if (e.shiftKey) parts.push('Shift');
  parts.push(keyName(key));
  return parts.join('+');
}

/** Puts modifiers in one order and the key's case right, so two spellings compare equal. */
export function normalize(combo: string): string {
  const parts = combo.split('+').map((p) => p.trim()).filter(Boolean);
  const mods = MODS.filter((m) => parts.some((p) => p.toLowerCase() === m.toLowerCase() || (m === 'Ctrl' && /^(control|cmd|command)$/i.test(p))));
  const rest = parts.filter((p) => !/^(ctrl|control|cmd|command|alt|shift|meta)$/i.test(p));
  if (rest.length !== 1) return '';
  return [...mods.filter((m) => m !== 'Meta'), keyName(rest[0].length === 1 ? rest[0] : rest[0])].join('+');
}

/** Combinations the app does not let a command take: the desktop window's own menu uses Ctrl+R
 *  (Reload), and Ctrl+C/V/X/A/Z belong to text fields. */
export const RESERVED = ['Ctrl+R', 'Ctrl+C', 'Ctrl+V', 'Ctrl+X', 'Ctrl+A', 'Ctrl+Z', 'Ctrl+Y'];

export interface Conflict {
  combo: string;
  commands: string[];
}

/** Every combination bound to more than one command, and every reserved one in use. */
export function conflicts(bindings: Binding[]): Conflict[] {
  const by = new Map<string, string[]>();
  for (const b of bindings) {
    const c = normalize(b.combo);
    if (!c) continue;
    by.set(c, [...(by.get(c) ?? []), b.command]);
  }
  const out: Conflict[] = [];
  for (const [combo, commands] of by) {
    if (commands.length > 1) out.push({ combo, commands });
    else if (RESERVED.includes(combo)) out.push({ combo, commands: [...commands, '(reserved)'] });
  }
  return out;
}

/** Whether a recorded combination may be used: needs a modifier unless it is a function key. */
export function usable(combo: string): boolean {
  const n = normalize(combo);
  if (!n) return false;
  if (RESERVED.includes(n)) return false;
  return n.includes('+') || /^F([1-9]|1[0-2])$/.test(n);
}

/** The command bound to a key event, or null. */
export function commandFor(bindings: Binding[], e: KeyLike): string | null {
  const c = comboOf(e);
  if (!c) return null;
  const hit = bindings.find((b) => normalize(b.combo) === c);
  return hit ? hit.command : null;
}
