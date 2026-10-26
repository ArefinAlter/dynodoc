// Settings, Accounts: the servers added to this app, signing in with the browser, and syncing
// projects between this computer and a server (decision 020). A server is optional: the workspace
// on this computer needs none.
import { h, cx, type Child } from '../dom.ts';
import { state } from '../state.ts';
import { button, input, notice, pill, section, select, field } from '../ui/controls.ts';
import { addServer, accountOf, cancelSignIn, checkServer, checkSync, choose, lastSynced, linkProject, links, loadRemoteProjects, planOf, removeServer, sendForReview, signOutOf, startSignIn, syncNow, unlinkProject } from '../data/accounts.ts';
import { confirmDialog } from '../ui/layer.ts';
import type { Action, Item } from '../data/sync.ts';

const LABEL: Record<Action, string> = {
  same: 'In sync',
  send: 'Send changes',
  receive: 'Receive changes',
  both: 'Send and receive',
  conflict: 'Needs your choice',
  'add-remote': 'New here: will be created on the server',
  'add-local': 'New on the server: will be added here',
  'missing-remote': 'No longer on the server',
  'missing-local': 'Removed from this computer',
  waiting: 'Waiting for review on the server',
  'needs-review': 'The server wants a review',
};

const TONE: Record<Action, 'neutral' | 'success' | 'info' | 'warning' | 'error'> = {
  same: 'success',
  send: 'info',
  receive: 'info',
  both: 'info',
  conflict: 'warning',
  'add-remote': 'info',
  'add-local': 'info',
  'missing-remote': 'neutral',
  'missing-local': 'neutral',
  waiting: 'neutral',
  'needs-review': 'warning',
};

function accountRow(id: string): Child {
  const a = accountOf(id);
  if (!a) return null;
  const flow = state.servers.flow;
  const mine = flow.account_id === a.id;
  const waiting = mine && flow.state === 'waiting';
  return h(
    'div',
    { class: 'card pad server-card', 'data-server': a.id },
    h(
      'div',
      { class: 'server-head' },
      h('div', { class: 'grow' }, h('strong', null, a.cloud ? `Dynodoc cloud (${a.host})` : a.host), h('div', { class: 'muted small' }, a.signed_in ? `Signed in${a.email || a.name ? ` as ${a.email ?? a.name}` : ''}.` : 'Not signed in.')),
      a.signed_in ? pill('Signed in', 'success', { mono: false }) : pill('Not signed in', 'neutral', { mono: false }),
    ),
    waiting
      ? h(
          'div',
          { class: 'notice-block', role: 'status' },
          h('p', null, 'Your browser has opened this server’s sign-in page. Sign in there, check that the pairing code is ', h('strong', { id: 'signin-pairing' }, flow.pairing ?? ''), ', and choose Continue. The page then hands the sign-in back to this app. This request is good for 10 minutes. If that page says the link is not valid, the server is older than this app and needs to be updated.'),
          h('div', { class: 'row-actions' }, button('Cancel', { size: 'sm', id: 'signin-cancel', onClick: () => void cancelSignIn() })),
        )
      : null,
    mine && flow.state === 'failed' ? notice('warning', flow.message || 'The sign-in did not finish.') : null,
    h(
      'div',
      { class: 'row-actions' },
      a.signed_in
        ? [
            button('Sign out', { size: 'sm', id: `signout-${a.id}`, onClick: () => confirmDialog('Sign out', h('p', null, `This ends the session on ${a.host} and removes it from this computer. Linked projects stay as they are on both sides.`), 'Sign out', () => void signOutOf(a.id)) }),
            button('Check the session', { size: 'sm', variant: 'ghost', onClick: () => void checkServer(a.id) }),
          ]
        : button('Sign in with your browser', { size: 'sm', variant: 'primary', id: `signin-${a.id}`, disabled: waiting, onClick: () => void startSignIn(a.id) }),
      button('Remove', { size: 'sm', variant: 'ghost', onClick: () => confirmDialog('Remove this server', h('p', null, `${a.host} is taken out of this app and its session is removed from this computer. Nothing on the server changes, and projects linked to it stay as they are here.`), 'Remove', () => void removeServer(a.id), { danger: true }) }),
    ),
  );
}

function conflictList(projectId: string, item: Item, host: string): Child {
  if (!item.localId || !item.conflicts.length) return null;
  const docId = item.localId;
  const picks = state.servers.picks[projectId]?.[docId] ?? {};
  return h(
    'div',
    { class: 'conflicts' },
    item.conflicts.map((c, i) =>
      h(
        'fieldset',
        { class: 'conflict', 'data-conflict': c.key },
        h('legend', null, c.label || 'A change'),
        (['local', 'remote'] as const).map((side) =>
          h(
            'label',
            { class: cx('radio', picks[c.key] === side && 'is-on') },
            h('input', { type: 'radio', name: `c-${projectId}-${docId}-${i}`, checked: picks[c.key] === side, onChange: () => choose(projectId, docId, c.key, side) }),
            h('span', null, h('strong', null, side === 'local' ? 'This computer: ' : `${host}: `), side === 'local' ? c.local : c.remote),
          ),
        ),
      ),
    ),
  );
}

function planView(projectId: string, host: string): Child {
  const plan = planOf(projectId);
  if (!plan) return null;
  if (!plan.items.length) return h('p', { class: 'muted small' }, 'There are no files on either side yet.');
  return h(
    'div',
    { class: 'sync-plan' },
    h(
      'table',
      { class: 'table compact' },
      h('thead', null, h('tr', null, h('th', null, 'File'), h('th', null, 'A sync would'))),
      h(
        'tbody',
        null,
        plan.items.map((i) =>
          h(
            'tr',
            { 'data-action': i.action },
            h('td', null, h('strong', null, i.title), h('div', { class: 'muted small' }, i.kind)),
            h(
              'td',
              null,
              pill(LABEL[i.action], TONE[i.action], { mono: false }),
              i.note ? h('div', { class: 'muted small' }, i.note) : null,
              i.action === 'send' || i.action === 'both' || i.action === 'receive' ? h('div', { class: 'muted small' }, `${i.toRemote.length ? `${i.toRemote.length} change${i.toRemote.length === 1 ? '' : 's'} to the server` : ''}${i.toRemote.length && i.toLocal.length ? ', ' : ''}${i.toLocal.length ? `${i.toLocal.length} change${i.toLocal.length === 1 ? '' : 's'} here` : ''}`) : null,
              conflictList(projectId, i, host),
              i.action === 'needs-review' && i.localId ? button('Send for review', { size: 'sm', onClick: () => void sendForReview(projectId, i.localId!) }) : null,
            ),
          ),
        ),
      ),
    ),
  );
}

function projectCard(projectId: string, name: string): Child {
  const l = links()[projectId];
  const running = Boolean(state.servers.running[projectId]);
  const last = state.servers.last[projectId];
  if (l) {
    const a = accountOf(l.accountId);
    const host = a?.host ?? 'the server';
    return h(
      'div',
      { class: 'card pad sync-card', 'data-project': projectId },
      h('div', { class: 'server-head' }, h('div', { class: 'grow' }, h('strong', null, name), h('div', { class: 'muted small' }, `Linked to “${l.remoteName || 'a project'}” on ${host} · ${lastSynced(l)}`)), a?.signed_in ? null : pill('Sign in to sync', 'warning', { mono: false })),
      h(
        'div',
        { class: 'row-actions' },
        button('Check', { size: 'sm', id: `sync-check-${projectId}`, disabled: running || !a?.signed_in, onClick: () => void checkSync(projectId) }),
        button('Sync now', { size: 'sm', variant: 'primary', id: `sync-now-${projectId}`, disabled: running || !a?.signed_in, onClick: () => void syncNow(projectId) }),
        button('Unlink', { size: 'sm', variant: 'ghost', disabled: running, onClick: () => confirmDialog('Unlink', h('p', null, `${name} stops syncing with ${host}. The files stay where they are, on both sides.`), 'Unlink', () => void unlinkProject(projectId)) }),
      ),
      last ? h('p', { class: 'small', role: 'status', id: `sync-status-${projectId}` }, last) : null,
      planView(projectId, host),
    );
  }
  const options: [string, string][] = [['', 'Choose a project on a server…']];
  for (const a of state.servers.accounts.filter((x) => x.signed_in)) {
    options.push([`new|${a.id}`, `Make a new project on ${a.host}`]);
    for (const p of state.servers.projects[a.id] ?? []) options.push([`${a.id}|${p.id}`, `${p.name} on ${a.host}`]);
  }
  return h(
    'div',
    { class: 'card pad sync-card', 'data-project': projectId },
    h('div', { class: 'server-head' }, h('div', { class: 'grow' }, h('strong', null, name), h('div', { class: 'muted small' }, 'Not linked to a server.'))),
    options.length > 1
      ? h('div', { class: 'row-actions' }, select('', options, (v) => {
          if (!v) return;
          const [first, second] = v.split('|');
          void (first === 'new' ? linkProject(projectId, second, 'new') : linkProject(projectId, first, second));
        }, { label: `Link ${name} to a server project`, id: `link-${projectId}` }))
      : h('p', { class: 'muted small' }, 'Sign in to a server above to link this project to one of its projects.'),
  );
}

export function serversSection(): Child {
  if (!state.desktop) return null;
  const s = state.servers;
  const signedIn = s.accounts.filter((a) => a.signed_in);
  // The projects of a server are read once it has a session (and again by the button).
  for (const a of signedIn) if (!s.projects[a.id] && !s.busy) void loadRemoteProjects(a.id);
  const inLocal = state.live.env === 'local';
  return [
    section(
      'Servers',
      'A server is optional: the workspace on this computer works without one. Add the Dynodoc cloud or a server your organisation runs, sign in with your browser, and link projects to sync them.',
      notice('info', `Your sign-in to a server is kept in ${s.store || 'the credential store'}. The page that signs you in is the server’s own, in your browser; this app never sees your password or an emailed link.`),
      s.accounts.length ? h('div', { class: 'servers' }, s.accounts.map((a) => accountRow(a.id))) : h('p', { class: 'muted small' }, 'No server added yet.'),
      h(
        'div',
        { class: 'server-add' },
        field('Server address', input(s.addDraft, (v) => { s.addDraft = v; }, { id: 'server-address', placeholder: 'https://dynodoc.example.org', onEnter: () => void addServer(s.addDraft) }), 'An https:// address (http://localhost works for testing).'),
        h('div', { class: 'row-actions' }, button('Add server', { size: 'sm', id: 'server-add', onClick: () => void addServer(s.addDraft) }), s.cloud && !s.accounts.some((a) => a.cloud) ? button('Add the Dynodoc cloud', { size: 'sm', id: 'server-add-cloud', onClick: () => void addServer(s.cloud) }) : null),
      ),
    ),
    section(
      'Sync projects',
      'Linking matches a project on this computer with a project on a server. A sync sends changes that were made here and receives changes made there, file by file. Changes to the same text on both sides wait for your choice; nothing is overwritten. History stays on each side: only the content travels.',
      !inLocal ? notice('info', 'Sync works from the workspace on this computer. The screens show something else now.') : null,
      signedIn.length ? h('div', { class: 'row-actions' }, signedIn.map((a) => button(`Read projects on ${a.host}`, { size: 'sm', variant: 'ghost', onClick: () => void loadRemoteProjects(a.id) }))) : null,
      inLocal ? h('div', { class: 'servers' }, state.db.projects.map((p) => projectCard(p.id, p.name))) : null,
    ),
  ];
}
