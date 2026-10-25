// Projects: the list (find, owned, shared, starred) and one project's page: overview, issues
// (with labels), people and roles, review rules, AI agent policy, and activity, as in the web
// workspace's project tabs.
import { h, cx } from '../dom.ts';
import { icon } from '../icons.ts';
import { state, render, toast } from '../state.ts';
import * as B from '../data/backend.ts';
import type { Project, Role, Issue } from '../data/types.ts';
import { button, empty, input, pill, tabs, textarea, field, avatar, select, toggle, row, rows, badge, notice, segmented } from '../ui/controls.ts';
import { openDialog, closeDialog, confirmDialog } from '../ui/layer.ts';
import { isAgent, personName, short, when, ROLE_LABEL, ROLE_SUMMARY, viewHeader } from './common.ts';
import { switchProject } from './actions.ts';

function newProjectDialog(): void {
  let name = '', description = '';
  openDialog({
    title: 'New project',
    width: 520,
    body: () => [
      field('Project name', input(name, (v) => { name = v; }, { id: 'np-name', autofocus: true, maxlength: 100 })),
      field('Description', textarea(description, (v) => { description = v; }, { id: 'np-desc', rows: 2 })),
      h('p', { class: 'muted small' }, 'Private: only invited people can access this project.'),
    ],
    actions: () => [
      button('Cancel', { onClick: closeDialog }),
      button('Create project', {
        variant: 'primary',
        onClick: () => {
          if (!name.trim()) return toast('warning', 'Name the project');
          const p: Project = {
            id: `p_${Date.now().toString(36)}`, name: name.trim(), description: description.trim(), ownerId: state.db.me, role: 'owner', private: true, starred: false, stars: 0, watch: 'requests', updatedAt: Date.now(),
            rules: { protectMain: true, requiredApprovals: 1, mergeRoles: 'editors' }, members: [{ personId: state.db.me, role: 'owner', via: 'direct' }], issues: [], labels: [],
            agentPolicy: { enabled: false, minRole: 'editor', callsPerMinute: 30, openProposals: 3, proposalsPerDay: 10, submitRequiresPerson: true }, space: 'sp_org',
          };
          state.db.projects.push(p);
          state.ui.branchByProject[p.id] = 'main';
          state.ui.projects.selected = p.id;
          closeDialog();
          toast('success', `Created ${p.name}`, 'Add files from the web workspace or an add-in.');
        },
      }),
    ],
  });
}

function projectCard(p: Project): HTMLElement {
  const docs = B.docsOf(state.db, p.id);
  const open = state.db.requests.filter((r) => r.projectId === p.id && r.status === 'open').length;
  return h(
    'div',
    { class: cx('project-card', state.ui.projectId === p.id && 'is-current'), tabindex: '0', role: 'button', onClick: () => { state.ui.projects.selected = p.id; render(); }, onKeydown: (e: KeyboardEvent) => { if (e.key === 'Enter') { state.ui.projects.selected = p.id; render(); } } },
    h('div', { class: 'project-card-head' }, icon('repo', 16), h('strong', null, p.name), p.private ? badge('Private') : null, h('span', { class: 'spacer' }), h('button', { type: 'button', class: cx('star', p.starred && 'is-on'), 'aria-label': p.starred ? 'Unstar' : 'Star', 'aria-pressed': String(p.starred), onClick: (e: Event) => { e.stopPropagation(); p.starred = !p.starred; p.stars += p.starred ? 1 : -1; render(); } }, icon('star', 14), String(p.stars))),
    h('p', { class: 'muted small' }, p.description),
    h('div', { class: 'project-card-meta' }, h('span', null, `${docs.length} files`), h('span', null, `${open} open request${open === 1 ? '' : 's'}`), h('span', null, `${p.issues.filter((i) => i.state === 'open').length} open issues`), h('span', null, `You: ${ROLE_LABEL[p.role]}`), h('span', { class: 'muted' }, `updated ${short(p.updatedAt)}`)),
  );
}

function overview(p: Project): HTMLElement {
  const contributors = new Map<string, number>();
  const docIds = new Set(B.docsOf(state.db, p.id).map((d) => d.id));
  for (const c of state.db.commits) if (docIds.has(c.docId)) contributors.set(c.authorId, (contributors.get(c.authorId) ?? 0) + 1);
  return h(
    'div',
    { class: 'two-col' },
    h(
      'div',
      null,
      h('div', { class: 'card pad' }, h('p', null, p.description || 'No description.'), h('div', { class: 'row-actions' },
        state.ui.projectId === p.id ? pill('current project', 'primary', { mono: false }) : button('Make current', { size: 'sm', variant: 'primary', onClick: () => switchProject(p.id) }),
        select(p.watch, [['requests', 'Watch: pull requests'], ['ignore', 'Watch: standard']], (v) => { p.watch = v; render(); }, { label: 'Watch' }))),
      h('div', { class: 'section-title' }, 'Files'),
      h('div', { class: 'list-card' }, B.docsOf(state.db, p.id).map((d) => h('div', { class: 'list-item' }, icon(d.kind === 'document' ? 'word' : d.kind === 'spreadsheet' ? 'sheet' : 'slides', 15), h('div', { class: 'grow' }, h('strong', null, d.filename)), h('span', { class: 'mono muted' }, `r${B.mainSeq(state.db, d.id)}`)))),
    ),
    h(
      'div',
      null,
      h('div', { class: 'card pad' }, h('dl', { class: 'facts' },
        h('dt', null, 'Your role'), h('dd', null, ROLE_LABEL[p.role]),
        h('dt', null, 'Access'), h('dd', null, p.private ? 'Invite-only project' : 'Shared'),
        h('dt', null, 'Stars'), h('dd', null, String(p.stars)),
        h('dt', null, 'Review rules'), h('dd', null, `${p.rules.requiredApprovals} approval${p.rules.requiredApprovals === 1 ? '' : 's'}; ${p.rules.protectMain ? 'main protected' : 'editors write main'}`),
      )),
      h('div', { class: 'section-title' }, 'Contributors'),
      h('div', { class: 'list-card' }, [...contributors.entries()].sort((a, b) => b[1] - a[1]).map(([id, n]) => h('div', { class: 'list-item' }, avatar(personName(id), isAgent(id)), h('div', { class: 'grow' }, personName(id)), h('span', { class: 'muted small' }, `${n} revision${n === 1 ? '' : 's'}`)))),
    ),
  );
}

let issueFilter: 'open' | 'closed' = 'open';
let issueDraft = { title: '', body: '' };
let issueComment = '';

function issues(p: Project): HTMLElement {
  const ui = state.ui.projects;
  if (ui.issue !== null) {
    const it = p.issues.find((i) => i.number === ui.issue);
    if (it) return issueDetail(p, it);
  }
  const list = p.issues.filter((i) => i.state === issueFilter).sort((a, b) => b.createdAt - a.createdAt);
  const label = (id: string) => p.labels.find((l) => l.id === id);
  return h(
    'div',
    null,
    h('div', { class: 'row-actions' },
      segmented(issueFilter, [['open', `Open ${p.issues.filter((i) => i.state === 'open').length}`], ['closed', `Closed ${p.issues.filter((i) => i.state === 'closed').length}`]], (v) => { issueFilter = v; render(); }, 'Issue state'),
      h('span', { class: 'spacer' }),
      button('New issue', { variant: 'primary', size: 'sm', icon: 'plus', disabled: p.role === 'viewer', onClick: () => newIssue(p) })),
    h('div', { class: 'list-card' },
      list.map((i) => h('button', { type: 'button', class: 'list-item list-button', onClick: () => { ui.issue = i.number; render(); } },
        icon('issue', 15, `icon issue-${i.state}`),
        h('div', { class: 'grow' }, h('div', null, h('strong', null, i.title), ' ', i.labels.map((l) => { const lb = label(l); return lb ? h('span', { class: 'label-chip', css: { '--label': `#${lb.color}` } }, lb.name) : null; })), h('div', { class: 'muted small' }, `#${i.number} opened ${when(i.createdAt)} by ${personName(i.authorId)}${i.assigneeId ? ` · assigned to ${personName(i.assigneeId)}` : ''}`)),
        i.comments.length ? h('span', { class: 'muted small' }, icon('comment', 13), ` ${i.comments.length}`) : null)),
      list.length ? null : h('div', { class: 'muted small pad' }, `No ${issueFilter} issues.`)),
  );
}

function issueDetail(p: Project, it: Issue): HTMLElement {
  return h(
    'div',
    null,
    h('button', { type: 'button', class: 'link', onClick: () => { state.ui.projects.issue = null; render(); } }, '← All issues'),
    h('h2', { class: 'issue-title' }, it.title, h('span', { class: 'muted' }, ` #${it.number}`)),
    h('div', { class: 'row-actions' }, pill(it.state, it.state === 'open' ? 'success' : 'primary'), h('span', { class: 'muted small' }, `${personName(it.authorId)} opened ${when(it.createdAt)}`), h('span', { class: 'spacer' }),
      select(it.assigneeId ?? '', [['', 'Unassigned'], ...p.members.map((m) => [m.personId, personName(m.personId)] as [string, string])], (v) => { it.assigneeId = v || undefined; render(); }, { label: 'Assignee' }),
      button(it.state === 'open' ? 'Close issue' : 'Reopen issue', { size: 'sm', onClick: () => { it.state = it.state === 'open' ? 'closed' : 'open'; it.closedAt = it.state === 'closed' ? Date.now() : undefined; render(); } })),
    h('div', { class: 'comments' },
      h('div', { class: 'comment' }, avatar(personName(it.authorId)), h('div', { class: 'comment-body' }, h('div', { class: 'comment-meta' }, h('strong', null, personName(it.authorId)), h('span', { class: 'muted' }, when(it.createdAt))), h('div', { class: 'comment-text' }, it.body))),
      it.comments.map((c) => h('div', { class: 'comment' }, avatar(personName(c.authorId)), h('div', { class: 'comment-body' }, h('div', { class: 'comment-meta' }, h('strong', null, personName(c.authorId)), h('span', { class: 'muted' }, when(c.at))), h('div', { class: 'comment-text' }, c.text))))),
    textarea(issueComment, (v) => { issueComment = v; }, { id: 'issue-comment', rows: 3, placeholder: 'Leave a comment (comments cannot be edited later)' }),
    h('div', { class: 'row-actions' }, h('span', { class: 'spacer' }), button('Comment', { variant: 'primary', size: 'sm', onClick: () => { if (!issueComment.trim()) return; it.comments.push({ authorId: state.db.me, at: Date.now(), text: issueComment.trim() }); issueComment = ''; render(); } })),
  );
}

function newIssue(p: Project): void {
  issueDraft = { title: '', body: '' };
  openDialog({
    title: 'New issue',
    width: 560,
    body: () => [field('Title', input(issueDraft.title, (v) => { issueDraft.title = v; }, { id: 'ni-title', autofocus: true })), field('Description', textarea(issueDraft.body, (v) => { issueDraft.body = v; }, { id: 'ni-body', rows: 5 }))],
    actions: () => [button('Cancel', { onClick: closeDialog }), button('Create issue', { variant: 'primary', onClick: () => {
      if (!issueDraft.title.trim()) return toast('warning', 'Give the issue a title');
      const n = p.issues.reduce((m, i) => Math.max(m, i.number), 0) + 1;
      p.issues.push({ number: n, title: issueDraft.title.trim(), body: issueDraft.body.trim(), state: 'open', authorId: state.db.me, labels: [], comments: [], createdAt: Date.now() });
      closeDialog();
      toast('success', `Opened issue #${n}`);
    } })],
  });
}

let invite = { email: '', role: 'editor' as Role };

function people(p: Project): HTMLElement {
  const manage = p.role === 'owner' || p.role === 'manager';
  return h(
    'div',
    null,
    !manage ? notice('info', `Your role (${ROLE_LABEL[p.role]}) cannot change who has access.`) : null,
    h('div', { class: 'list-card' },
      p.members.map((m) => h('div', { class: 'list-item' },
        avatar(personName(m.personId), isAgent(m.personId)),
        h('div', { class: 'grow' }, h('strong', null, personName(m.personId)), h('div', { class: 'muted small' }, B.person(state.db, m.personId).email)),
        manage && m.role !== 'owner' && m.personId !== state.db.me
          ? [select(m.role, (['manager', 'editor', 'approver', 'reviewer', 'viewer'] as Role[]).map((r) => [r, ROLE_LABEL[r]] as [Role, string]), (v) => { m.role = v; toast('success', `${personName(m.personId)} is now ${ROLE_LABEL[v]}`); render(); }, { label: 'Role' }),
             button('Remove', { size: 'sm', variant: 'ghost', onClick: () => confirmDialog('Remove access', h('p', null, `Remove ${personName(m.personId)} from ${p.name}? Their revisions stay in history.`), 'Remove access', () => { p.members = p.members.filter((x) => x !== m); render(); }, { danger: true }) })]
          : badge(ROLE_LABEL[m.role]),
      ))),
    manage
      ? h('div', { class: 'card pad' },
          h('div', { class: 'srow-title' }, 'Add a person'),
          h('div', { class: 'row-actions' },
            input(invite.email, (v) => { invite.email = v; }, { id: 'invite-email', placeholder: 'name@example.org', label: 'Email' }),
            select(invite.role, (['manager', 'editor', 'approver', 'reviewer', 'viewer'] as Role[]).map((r) => [r, ROLE_LABEL[r]] as [Role, string]), (v) => { invite.role = v; }, { label: 'Role' }),
            button('Add', { variant: 'primary', size: 'sm', onClick: () => {
              if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(invite.email.trim())) return toast('warning', 'Enter an email address');
              const id = `u_${Date.now().toString(36)}`;
              state.db.people.push({ id, name: invite.email.trim().split('@')[0], email: invite.email.trim() });
              p.members.push({ personId: id, role: invite.role, via: 'direct' });
              toast('success', `Invited ${invite.email.trim()}`, 'Sample data: no email is sent.');
              invite = { email: '', role: 'editor' };
              render();
            } })),
          h('p', { class: 'muted small' }, 'Owner and manager cannot be given through an invitation.'))
      : null,
    h('div', { class: 'section-title' }, 'What each role can do'),
    h('div', { class: 'list-card' }, (Object.keys(ROLE_SUMMARY) as Role[]).map((r) => h('div', { class: 'list-item' }, badge(ROLE_LABEL[r]), h('div', { class: 'grow muted small' }, ROLE_SUMMARY[r])))),
  );
}

function rules(p: Project): HTMLElement {
  const manage = p.role === 'owner' || p.role === 'manager';
  return h(
    'div',
    null,
    rows(
      row('Protect the team version', 'Editors send changes to main through review; nobody commits to main directly.', toggle(p.rules.protectMain, (v) => { p.rules.protectMain = v; render(); }, 'Protect the team version', { disabled: !manage })),
      row('Required approvals', 'Approvals a change request needs before it can be merged (0 to 5).', select(String(p.rules.requiredApprovals), ['0', '1', '2', '3', '4', '5'].map((n) => [n, n] as [string, string]), (v) => { p.rules.requiredApprovals = Number(v); render(); }, { label: 'Required approvals', disabled: !manage })),
      row('Who may merge', 'People who press Merge once the rules are met.', select(p.rules.mergeRoles, [['editors', 'Editors, managers and owners'], ['owners', 'Managers and owners']], (v) => { p.rules.mergeRoles = v; render(); }, { label: 'Who may merge', disabled: !manage })),
      row('Approvals and merges are a person’s', 'No agent key and no add-in can approve, request changes, merge or decline.', null, { fixed: true }),
      row('History is append-only', 'Revisions are hash-chained; undoing is a new revision (revert, restore).', null, { fixed: true }),
    ),
    manage ? null : notice('info', 'Only owners and managers change review rules.'),
  );
}

function agents(p: Project): HTMLElement {
  const a = p.agentPolicy;
  const manage = p.role === 'owner' || p.role === 'manager';
  const num = (v: number, set: (n: number) => void, label: string) => input(String(v), (s) => { const n = Number(s); if (Number.isInteger(n) && n >= 0) set(n); }, { id: `ap-${label}`, label, class: 'input-num', disabled: !manage });
  return h(
    'div',
    null,
    rows(
      row('AI agents may work in this project', 'Agent keys read files and history and propose changes as drafts.', toggle(a.enabled, (v) => { a.enabled = v; render(); }, 'Allow agents', { disabled: !manage })),
      row('Lowest role an agent key acts with', 'An agent never has more rights than this, whoever enrolled it.', select(a.minRole, (['editor', 'reviewer', 'viewer'] as Role[]).map((r) => [r, ROLE_LABEL[r]] as [Role, string]), (v) => { a.minRole = v; render(); }, { label: 'Agent role', disabled: !manage })),
      row('Calls per minute', 'Per agent key.', num(a.callsPerMinute, (n) => (a.callsPerMinute = n), 'calls')),
      row('Open proposals at once', 'More are refused until one is merged, declined or withdrawn.', num(a.openProposals, (n) => (a.openProposals = n), 'open')),
      row('Proposals per day', '', num(a.proposalsPerDay, (n) => (a.proposalsPerDay = n), 'daily')),
      row('A person submits proposals', 'An agent’s draft waits for a person to send it for review.', toggle(a.submitRequiresPerson, (v) => { a.submitRequiresPerson = v; render(); }, 'Person submits', { disabled: !manage })),
    ),
    h('div', { class: 'section-title' }, 'Usage (sample)'),
    h('table', { class: 'table card' }, h('thead', null, h('tr', null, h('th', null, 'Day (UTC)'), h('th', null, 'Calls'), h('th', null, 'Data read'), h('th', null, 'Refused'))),
      h('tbody', null, [['Today', '14', '182 KB', '1'], ['Yesterday', '31', '420 KB', '0'], ['2 days ago', '8', '96 KB', '2']].map((r) => h('tr', null, r.map((c) => h('td', null, c)))))),
    manage ? h('div', { class: 'row-actions' }, button('Stop all AI agents now', { variant: 'danger', onClick: () => confirmDialog('Stop all AI agents', h('p', null, `Every agent key loses access to ${p.name} at once and open AI proposals stay as they are for a person to review.`), 'Stop agents', () => { a.enabled = false; toast('success', 'Agents stopped'); render(); }, { danger: true }) })) : null,
  );
}

function projectActivity(p: Project): HTMLElement {
  const list = state.db.activity.filter((x) => x.projectId === p.id);
  return h('div', { class: 'list-card' }, list.map((x) => h('div', { class: 'list-item' }, avatar(personName(x.actorId), isAgent(x.actorId)), h('div', { class: 'grow' }, h('strong', null, personName(x.actorId)), ` ${x.summary}`), h('span', { class: 'muted small' }, when(x.at)))), list.length ? null : h('div', { class: 'muted small pad' }, 'No activity yet.'));
}

export function projectsView(): HTMLElement {
  const ui = state.ui.projects;
  const sel = state.db.projects.find((p) => p.id === ui.selected);
  if (!sel) {
    const q = ui.filter.toLowerCase();
    const list = state.db.projects
      .filter((p) => p.name.toLowerCase().includes(q))
      .filter((p) => (ui.type === 'owned' ? p.ownerId === state.db.me : ui.type === 'shared' ? p.ownerId !== state.db.me : ui.type === 'starred' ? p.starred : true));
    return h(
      'div',
      { class: 'page' },
      viewHeader('Projects', `${state.db.projects.length} projects you can open`, button('New project', { variant: 'primary', icon: 'plus', onClick: newProjectDialog })),
      h('div', { class: 'toolbar' }, h('div', { class: 'search-field' }, icon('search', 14), input(ui.filter, (v) => { ui.filter = v; render(); }, { id: 'projects-filter', placeholder: 'Find a project' })), segmented(ui.type, [['all', 'All'], ['owned', 'Owned by me'], ['shared', 'Shared with me'], ['starred', 'Starred']], (v) => { ui.type = v; render(); }, 'Project type')),
      list.length ? h('div', { class: 'project-grid' }, list.map(projectCard)) : empty('projects', 'No projects match', 'Try another filter, or create a project.'),
    );
  }
  const body = ui.tab === 'issues' ? issues(sel) : ui.tab === 'people' ? people(sel) : ui.tab === 'rules' ? rules(sel) : ui.tab === 'agents' ? agents(sel) : ui.tab === 'activity' ? projectActivity(sel) : overview(sel);
  return h(
    'div',
    { class: 'page' },
    h('div', { class: 'crumbs' }, h('button', { type: 'button', class: 'link', onClick: () => { ui.selected = null; ui.issue = null; render(); } }, 'Projects'), h('span', { class: 'muted' }, ' / '), sel.name),
    viewHeader(sel.name, sel.description),
    tabs(ui.tab, [['overview', 'Overview'], ['issues', 'Issues', sel.issues.filter((i) => i.state === 'open').length], ['people', 'People', sel.members.length], ['rules', 'Review rules'], ['agents', 'AI agents'], ['activity', 'Activity']], (v) => { ui.tab = v; ui.issue = null; render(); }, 'Project sections'),
    h('div', { class: 'tab-body' }, body),
  );
}
