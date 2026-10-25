// Activity: notifications (the web workspace's bell: requests, reviews, merges, copies, access)
// and the activity feed of every project.
import { h, cx } from '../dom.ts';
import { icon } from '../icons.ts';
import { state, render, go } from '../state.ts';
import * as B from '../data/backend.ts';
import type { Notification } from '../data/types.ts';
import { button, tabs, avatar, empty, badge } from '../ui/controls.ts';
import { isAgent, personName, when, viewHeader } from './common.ts';
import { switchProject } from './actions.ts';

const KIND_ICON: Record<Notification['kind'], string> = {
  change_request_submitted: 'request',
  change_request_reviewed: 'check',
  change_request_merged: 'merge',
  change_request_declined: 'x',
  copy_detected: 'documents',
  access_granted: 'users',
  mention: 'comment',
};

function open(n: Notification): void {
  n.read = true;
  if (n.projectId !== state.ui.projectId) switchProject(n.projectId);
  if (n.requestId) {
    const r = state.db.requests.find((x) => x.id === n.requestId);
    state.ui.requests.selected = n.requestId;
    state.ui.requests.filter = r?.status === 'open' ? 'open' : r?.status === 'merged' ? 'merged' : 'closed';
    go('requests');
  } else if (n.docId) {
    state.ui.documents.selected = n.docId;
    state.ui.documents.tab = n.kind === 'copy_detected' ? 'sync' : 'overview';
    go('documents');
  } else {
    state.ui.projects.selected = n.projectId;
    go('projects');
  }
}

export function unread(): number {
  return state.db.notifications.filter((n) => !n.read).length;
}

export function activityView(): HTMLElement {
  const ui = state.ui.activity;
  const list = [...state.db.notifications].sort((a, b) => Number(a.read) - Number(b.read) || b.at - a.at);
  const body =
    ui.tab === 'notifications'
      ? list.length
        ? h('div', { class: 'list-card' }, list.map((n) =>
            h('div', { class: cx('list-item notif', !n.read && 'is-unread') },
              h('span', { class: 'notif-icon' }, icon(KIND_ICON[n.kind], 15)),
              avatar(personName(n.actorId), isAgent(n.actorId)),
              h('div', { class: 'grow' }, h('div', null, h('strong', null, personName(n.actorId)), ` ${n.text}`, isAgent(n.actorId) ? [' ', badge('agent key', 'info')] : null), h('div', { class: 'muted small' }, `${B.project(state.db, n.projectId).name} · ${when(n.at)}`)),
              n.kind === 'copy_detected' ? button('See copies', { size: 'sm', onClick: () => open(n) }) : button(n.requestId ? 'Open request' : 'Open', { size: 'sm', onClick: () => open(n) }),
              !n.read ? button('Mark read', { size: 'sm', variant: 'ghost', onClick: () => { n.read = true; render(); } }) : null)))
        : empty('bell', 'No notifications', 'Requests, reviews and merges you watch appear here.')
      : h('div', { class: 'list-card' }, state.db.activity.map((x) =>
          h('div', { class: 'list-item' }, avatar(personName(x.actorId), isAgent(x.actorId) || x.byAgent), h('div', { class: 'grow' }, h('strong', null, personName(x.actorId)), ` ${x.summary}`, h('div', { class: 'muted small' }, `${B.project(state.db, x.projectId).name} · ${when(x.at)}`)))));
  return h(
    'div',
    { class: 'page' },
    viewHeader('Activity', `${unread()} unread`, ui.tab === 'notifications' ? button('Mark all read', { size: 'sm', disabled: !unread(), onClick: () => { state.db.notifications.forEach((n) => (n.read = true)); render(); } }) : null),
    tabs(ui.tab, [['notifications', 'Notifications', unread()], ['activity', 'All activity']], (v) => { ui.tab = v; render(); }, 'Activity'),
    h('div', { class: 'tab-body' }, body),
  );
}
