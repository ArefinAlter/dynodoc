// Agent: threads in the manner of T3 Code's thread view and Ogun Studio's Agent screen. Each
// thread has an access level (Read only, Propose); the agent calls tools from a closed table,
// refused calls are shown, and anything with a consequence comes back as a card whose button a
// person presses. No model is called in this release: replies are prepared (data/agent.ts).
import { h, cx, type Child } from '../dom.ts';
import { icon } from '../icons.ts';
import { state, render, toast, go, type Thread, type AgentMessage } from '../state.ts';
import * as B from '../data/backend.ts';
import { plan, execute, STARTERS } from '../data/agent.ts';
import { ACCESS_INFO, NEVER, TOOLS, type AgentAccess } from '../lib/agentTools.ts';
import { active } from '../lib/ladder.ts';
import { button, iconButton, pill, segmented, badge } from '../ui/controls.ts';
import { openMenu } from '../ui/layer.ts';
import { short, currentBranch } from './common.ts';

function newThread(): Thread {
  const t: Thread = { id: `th_${Date.now().toString(36)}`, title: 'New thread', access: state.prefs.agentAccess, messages: [], createdAt: Date.now(), working: false };
  state.ui.agent.threads.unshift(t);
  state.ui.agent.active = t.id;
  return t;
}

export function startThread(): void {
  newThread();
  go('agent');
  requestAnimationFrame(() => document.getElementById('composer')?.focus());
}

function current(): Thread {
  const a = state.ui.agent;
  return a.threads.find((t) => t.id === a.active) ?? newThread();
}

/** Bold (**text**) and plain text; nothing else becomes markup. */
function inline(text: string): Child[] {
  return text.split(/(\*\*[^*]+\*\*)/g).map((part) => (part.startsWith('**') && part.endsWith('**') ? h('strong', null, part.slice(2, -2)) : part));
}

function prose(text: string): HTMLElement {
  const out: HTMLElement[] = [];
  let list: HTMLElement | null = null;
  for (const line of text.split('\n')) {
    if (line.startsWith('- ')) {
      if (!list) {
        list = h('ul', null);
        out.push(list);
      }
      list.appendChild(h('li', null, inline(line.slice(2))));
    } else {
      list = null;
      if (line.trim()) out.push(h('p', null, inline(line)));
    }
  }
  return h('div', { class: 'prose' }, out);
}

function send(text: string): void {
  const t = current();
  if (!text.trim() || t.working) return;
  const user: AgentMessage = { id: `m${Date.now()}`, role: 'user', text: text.trim(), at: Date.now() };
  t.messages.push(user);
  if (t.title === 'New thread') t.title = text.trim().slice(0, 48);
  state.ui.agent.draft = '';
  t.working = true;
  render();
  const p = plan(state.db, state.ui.projectId, currentBranch(), text);
  const { calls, refused } = execute(p, t.access, state.prefs.agentTools);
  const total = calls.reduce((n, c) => n + c.ms, 0);
  setTimeout(() => {
    const out = p.finish(refused);
    t.messages.push({ id: `m${Date.now()}a`, role: 'agent', text: out.text, at: Date.now(), calls, card: out.card });
    t.working = false;
    render();
  }, Math.min(1600, 300 + total));
}

function cardView(m: AgentMessage): HTMLElement | null {
  const c = m.card;
  if (!c) return null;
  const table = c.rows && c.columns ? h('table', { class: 'table compact' }, h('thead', null, h('tr', null, c.columns.map((x) => h('th', null, x)))), h('tbody', null, c.rows.map((r) => h('tr', null, r.map((x) => h('td', null, x)))))) : null;
  let action: Child = null;
  if (c.done) action = h('div', { class: 'card-done' }, icon('check', 14), c.done);
  else if (c.kind === 'summary')
    action = button('Use this summary', {
      variant: 'primary',
      size: 'sm',
      onClick: () => {
        state.ui.changes.summary = c.text ?? '';
        state.ui.changes.description = c.description ?? '';
        c.done = 'Put into the commit box. Committing is still yours.';
        go('changes');
      },
    });
  else if (c.kind === 'comment' && c.requestId)
    action = [
      button('Post as you', {
        variant: 'primary',
        size: 'sm',
        onClick: () => {
          try {
            B.comment(state.db, c.requestId!, c.text ?? '', c.blockId, true);
            c.done = 'Posted as you, marked as drafted by the agent.';
            toast('success', 'Comment posted');
          } catch (e) {
            toast('error', e instanceof Error ? e.message : String(e));
          }
          render();
        },
      }),
      button('Open the request', { size: 'sm', onClick: () => { state.ui.requests.selected = c.requestId!; go('requests'); } }),
    ];
  else if (c.kind === 'requests')
    action = button('Open Change requests', { size: 'sm', onClick: () => { if (c.requestId) state.ui.requests.selected = c.requestId; state.ui.requests.filter = 'open'; go('requests'); } });
  return h(
    'div',
    { class: cx('agent-card', `card-${c.kind}`) },
    h('div', { class: 'agent-card-head' }, icon(c.kind === 'comment' ? 'comment' : c.kind === 'summary' ? 'edit' : c.kind === 'verify' ? 'shield' : 'request', 15), h('strong', null, c.title)),
    c.text ? h('div', { class: c.kind === 'summary' ? 'agent-card-summary mono' : 'agent-card-text' }, c.text) : null,
    c.description ? h('pre', { class: 'agent-card-desc' }, c.description) : null,
    table,
    action ? h('div', { class: 'agent-card-actions' }, action) : null,
  );
}

function callsView(m: AgentMessage): HTMLElement | null {
  if (!m.calls?.length) return null;
  const refused = m.calls.filter((c) => c.refused).length;
  const ms = m.calls.reduce((n, c) => n + c.ms, 0);
  return h(
    'details',
    { class: 'calls' },
    h('summary', null, icon('chevronRight', 13, 'icon calls-chev'), `Worked · ${m.calls.length} tool call${m.calls.length === 1 ? '' : 's'}${refused ? ` · ${refused} refused` : ''} · ${(ms / 1000).toFixed(1)} s`),
    h('div', { class: 'calls-list' }, m.calls.map((c) => h('div', { class: cx('call', c.refused && 'is-refused') }, h('div', { class: 'call-head' }, h('code', null, c.tool), h('span', { class: 'muted' }, c.input), h('span', { class: 'spacer' }), c.refused ? pill('refused', 'error') : h('span', { class: 'muted small' }, `${c.ms} ms`)), h('div', { class: 'call-out' }, c.refused ? `Refused: ${c.refused}.` : c.output)))),
  );
}

function timeline(t: Thread): HTMLElement {
  if (!t.messages.length)
    return h(
      'div',
      { class: 'agent-empty' },
      h('div', { class: 'agent-empty-mark' }, icon('agent', 26)),
      h('h2', null, 'What should the agent look at?'),
      h('p', { class: 'muted' }, 'It reads history, diffs and change requests, and drafts comments and commit summaries for you to use. It cannot approve, merge or commit.'),
      h('div', { class: 'starters' }, STARTERS.map((s) => h('button', { type: 'button', class: 'starter', onClick: () => send(s) }, icon('chevronRight', 13), s))),
    );
  return h(
    'div',
    { class: 'timeline' },
    t.messages.map((m) =>
      m.role === 'user'
        ? h('div', { class: 'msg msg-user' }, h('div', { class: 'bubble' }, m.text))
        : h('div', { class: 'msg msg-agent' }, callsView(m), prose(m.text), cardView(m)),
    ),
    t.working ? h('div', { class: 'msg msg-agent working' }, h('span', { class: 'dots' }, h('span'), h('span'), h('span')), 'Working') : null,
  );
}

function composer(t: Thread): HTMLElement {
  const ladder = active(state.prefs.ladder);
  const ta = h('textarea', {
    id: 'composer',
    class: 'composer-input',
    rows: 2,
    placeholder: 'Ask about history, a diff or a change request',
    'aria-label': 'Message',
    onInput: (e: Event) => { state.ui.agent.draft = (e.target as HTMLTextAreaElement).value; },
    onKeydown: (e: KeyboardEvent) => {
      if (e.key === 'Enter' && !e.shiftKey) {
        e.preventDefault();
        send(state.ui.agent.draft);
      }
    },
  });
  ta.value = state.ui.agent.draft;
  return h(
    'div',
    { class: 'composer' },
    ta,
    h(
      'div',
      { class: 'composer-row' },
      h('button', { type: 'button', class: 'picker', onClick: (e: MouseEvent) => openMenu(e.currentTarget as HTMLElement, [
        { label: '', heading: 'Model' },
        { label: 'Dynodoc agent (prepared replies)', checked: true, note: 'No model is called in this release.' },
        ...state.prefs.ladder.map((l) => ({ label: `${l.kind} · ${l.model}`, disabled: true, note: l.enabled ? 'Configured; not called by this release.' : 'Off in Settings, Providers.' })),
      ]) }, icon('agent', 14), 'Dynodoc agent', icon('chevron', 12)),
      h('button', { type: 'button', class: 'picker', onClick: (e: MouseEvent) => openMenu(e.currentTarget as HTMLElement, (['read', 'propose'] as AgentAccess[]).map((a) => ({ label: ACCESS_INFO[a].label, note: ACCESS_INFO[a].blurb, checked: t.access === a, onSelect: () => { t.access = a; render(); } }))) }, icon('lock', 14), ACCESS_INFO[t.access].label, icon('chevron', 12)),
      ladder ? null : h('span', { class: 'muted small' }, 'No provider in use'),
      h('span', { class: 'spacer' }),
      h('button', { type: 'button', class: cx('send', t.working && 'is-busy'), 'aria-label': 'Send', disabled: t.working, onClick: () => send(state.ui.agent.draft) }, icon(t.working ? 'stop' : 'up', 16)),
    ),
  );
}

function details(t: Thread): HTMLElement {
  const calls = t.messages.flatMap((m) => m.calls ?? []);
  return h(
    'aside',
    { class: 'agent-details' },
    h('div', { class: 'details-block' }, h('div', { class: 'details-title' }, 'This thread'), h('dl', { class: 'facts small' }, h('dt', null, 'Access'), h('dd', null, ACCESS_INFO[t.access].label), h('dt', null, 'Model'), h('dd', null, 'Prepared replies'), h('dt', null, 'Tool calls'), h('dd', null, `${calls.length} (${calls.filter((c) => c.refused).length} refused)`), h('dt', null, 'Started'), h('dd', null, short(t.createdAt)))),
    h('div', { class: 'details-block' }, h('div', { class: 'details-title' }, 'Access'), segmented(t.access, [['read', 'Read only'], ['propose', 'Propose']], (v) => { t.access = v; render(); }, 'Access'), h('p', { class: 'muted small' }, ACCESS_INFO[t.access].blurb, ' The agent never raises its own access.')),
    h('div', { class: 'details-block' }, h('div', { class: 'details-title' }, 'Tools'), h('ul', { class: 'tool-list' }, TOOLS.map((tool) => {
      const on = state.prefs.agentTools.includes(tool.name);
      const allowed = on && (tool.access === 'read' || t.access === 'propose');
      return h('li', { class: cx(!allowed && 'is-off') }, icon(allowed ? 'check' : 'lock', 12), h('code', null, tool.name), tool.access === 'propose' ? badge('propose', 'info') : null);
    }))),
    h('div', { class: 'details-block' }, h('div', { class: 'details-title' }, 'No setting changes these'), h('ul', { class: 'never' }, NEVER.map((n) => h('li', null, n)))),
  );
}

export function agentView(): HTMLElement {
  const a = state.ui.agent;
  const t = current();
  return h(
    'div',
    { class: cx('agent-layout', a.detailsOpen && 'with-details') },
    h(
      'div',
      { class: 'agent-threads' },
      h('div', { class: 'agent-threads-head' }, h('strong', null, 'Threads'), h('span', { class: 'spacer' }), iconButton('plus', 'New thread', () => { newThread(); render(); })),
      a.threads.map((x) =>
        h('button', { type: 'button', class: cx('thread-row', x.id === t.id && 'is-selected'), onClick: () => { a.active = x.id; render(); } }, h('span', { class: cx('thread-dot', x.working && 'is-working') }), h('span', { class: 'thread-title' }, x.title), h('span', { class: 'muted small' }, short(x.createdAt))),
      ),
    ),
    h(
      'div',
      { class: 'agent-main' },
      h('div', { class: 'agent-head' }, h('strong', null, t.title), pill(ACCESS_INFO[t.access].label, t.access === 'read' ? 'neutral' : 'info', { mono: false }), h('span', { class: 'spacer' }), iconButton('sidebar', a.detailsOpen ? 'Hide details' : 'Show details', () => { a.detailsOpen = !a.detailsOpen; render(); }, { active: a.detailsOpen })),
      h('div', { class: 'agent-scroll', 'data-scroll-key': `agent-${t.id}`, 'data-stick-bottom': '1' }, h('div', { class: 'agent-column' }, timeline(t))),
      h('div', { class: 'agent-column' }, composer(t), h('p', { class: 'muted small center' }, 'Prepared replies: no model is called. Approvals and merges are always a person’s.')),
    ),
    a.detailsOpen ? details(t) : null,
  );
}
