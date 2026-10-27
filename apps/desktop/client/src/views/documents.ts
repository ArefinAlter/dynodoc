// Documents: the files of the project and, for one file, what the web workspace shows around
// the editor: the read-only content at main, named versions (save, compare, restore as new
// changes), drafts, discussions, people and review rules, and Sync & connections (connection keys
// for the add-ins, public read-only links, copies Dynodoc found). The editor itself is not here:
// editing happens in the Office and Google add-ins, and "Open in ..." says how.
import { h, cx } from '../dom.ts';
import { icon } from '../icons.ts';
import { state, render, toast } from '../state.ts';
import * as B from '../data/backend.ts';
import type { AddinHost, Doc } from '../data/types.ts';
import { HOST_LABEL, CONNECTORS } from '../data/connectors.ts';
import { compareBlocks } from '../lib/diff.ts';
import { button, empty, input, pill, tabs, textarea, field, avatar, notice, select, badge, hash } from '../ui/controls.ts';
import { diffView } from '../ui/diffView.ts';
import { openDialog, closeDialog, confirmDialog } from '../ui/layer.ts';
import { act, fileIcon, isAgent, personName, short, when, ROLE_LABEL, KIND_LABEL, draftTone, draftWord, runVerify, viewHeader } from './common.ts';
import { newDraftDialog, switchBranch } from './actions.ts';
import { isLive, isLocal } from '../data/live.ts';
import { addFilesDialog, exportFile } from '../data/files.ts';

const HOSTS_FOR: Record<Doc['kind'], [AddinHost, AddinHost]> = {
  document: ['word', 'google-docs'],
  spreadsheet: ['excel', 'google-sheets'],
  presentation: ['powerpoint', 'google-slides'],
};

function activeKeys(d: Doc) {
  return d.connections.filter((k) => !k.revoked && k.expiresAt > Date.now());
}

/** "Open in Word": the client has no editor. It explains where the file opens and its add-in's
 *  connection state; on the desktop, nothing is launched by this release. */
export function openInEditor(docId: string): void {
  // On a workspace the app can write files for: make a copy and open it where the system opens it.
  if (state.desktop && isLive()) {
    void exportFile(docId, true);
    return;
  }
  const d = B.doc(state.db, docId);
  const pref = state.prefs.editorFor[d.kind];
  const host = HOSTS_FOR[d.kind][pref === 'office' ? 0 : 1];
  const keys = activeKeys(d).filter((k) => k.host === host);
  openDialog({
    title: `Open in ${HOST_LABEL[host]}`,
    width: 560,
    body: () => [
      h('p', { class: 'dialog-text' }, `Dynodoc edits happen in ${HOST_LABEL[host]} with the Dynodoc add-in, not in this window. Open ${d.filename} there; the add-in captures your edits and they appear under Changes.`),
      h(
        'dl',
        { class: 'facts' },
        h('dt', null, 'Add-in'), h('dd', null, CONNECTORS.find((c) => c.host === host)?.name ?? host),
        h('dt', null, 'Connection'), h('dd', null, keys.length ? `Connected with a key that expires ${when(keys[0].expiresAt)}${keys[0].lastUsed ? `, last used ${when(keys[0].lastUsed)}` : ''}` : 'Not connected: create a connection key under Sync & connections.'),
        h('dt', null, 'Install'), h('dd', null, 'Settings, Integrations has the steps for each editor.'),
      ),
      notice('info', 'This release does not start the editor for you. Your preferred editor per file type is in Settings, Integrations.'),
    ],
    actions: () => [button('Close', { onClick: closeDialog }), button('Sync & connections', { variant: 'primary', onClick: () => { closeDialog(); state.ui.view = 'documents'; state.ui.documents.selected = d.id; state.ui.documents.tab = 'sync'; render(); } })],
  });
}

function fileTable(docs: Doc[]): HTMLElement {
  return h(
    'table',
    { class: 'table' },
    h('thead', null, h('tr', null, h('th', null, 'Name'), h('th', null, 'Latest'), h('th', null, 'Last change'), h('th', null, 'Drafts'), h('th', null, 'Requests'), h('th', null, 'Editor'), h('th', null, 'Updated'))),
    h(
      'tbody',
      null,
      docs.map((d) => {
        const last = B.branchCommits(state.db, d.id, 'main').slice(-1)[0];
        const drafts = state.db.drafts.filter((x) => x.docId === d.id && (x.state === 'private' || x.state === 'in_review')).length;
        const reqs = state.db.requests.filter((r) => r.docId === d.id && r.status === 'open').length;
        const keys = activeKeys(d);
        return h(
          'tr',
          { class: cx('row-link', state.ui.documents.selected === d.id && 'is-selected'), tabindex: '0', onClick: () => { state.ui.documents.selected = d.id; render(); }, onKeydown: (e: KeyboardEvent) => { if (e.key === 'Enter') { state.ui.documents.selected = d.id; render(); } } },
          h('td', null, h('span', { class: 'cell-file' }, fileIcon(d.kind), h('span', null, h('strong', null, d.title), h('span', { class: 'muted small block' }, d.filename)))),
          h('td', { class: 'mono' }, `r${B.mainSeq(state.db, d.id)}`),
          h('td', null, last ? h('span', null, last.summary, h('span', { class: 'muted' }, ` · ${personName(last.authorId)}`)) : '—'),
          h('td', null, drafts ? String(drafts) : h('span', { class: 'muted' }, '0')),
          h('td', null, reqs ? pill(`${reqs} open`, 'success') : h('span', { class: 'muted' }, '0')),
          h('td', null, isLocal() ? pill({ document: 'Word', spreadsheet: 'Excel', presentation: 'PowerPoint' }[d.kind], 'neutral', { mono: false, title: 'Opens in the program your computer uses for this kind of file' }) : keys.length ? pill(`${HOST_LABEL[keys[0].host]}`, 'success', { mono: false }) : pill('not connected', 'neutral')),
          h('td', { class: 'muted' }, last ? short(last.at) : ''),
        );
      }),
    ),
  );
}

function overview(d: Doc): HTMLElement {
  const commits = B.branchCommits(state.db, d.id, 'main');
  const last = commits[commits.length - 1];
  return h(
    'div',
    { class: 'doc-overview' },
    h(
      'div',
      { class: 'doc-facts card' },
      h('dl', { class: 'facts' },
        h('dt', null, 'Type'), h('dd', null, KIND_LABEL[d.kind]),
        h('dt', null, 'Team version'), h('dd', null, `main, revision ${B.mainSeq(state.db, d.id)}`),
        h('dt', null, 'Revision hash'), h('dd', null, last ? hash(last.chainHash, 16) : '—'),
        h('dt', null, 'Last change'), h('dd', null, last ? `${last.summary}, ${personName(last.authorId)}, ${when(last.at)}` : '—'),
        h('dt', null, 'Created'), h('dd', null, when(d.createdAt)),
      ),
      h('div', { class: 'row-actions' },
        button('Verify hash chain', { icon: 'shield', size: 'sm', onClick: () => { const r = runVerify(d.id); act(() => (r.bad ? `The hash chain does not verify at ${r.bad} revisions` : `Verified: the hash chain is intact through r${B.mainSeq(state.db, d.id)}`)); } }),
        button(`Open in ${HOST_LABEL[HOSTS_FOR[d.kind][state.prefs.editorFor[d.kind] === 'office' ? 0 : 1]]}`, { icon: 'external', size: 'sm', onClick: () => openInEditor(d.id) }),
        button('New draft…', { icon: 'branch', size: 'sm', onClick: () => newDraftDialog(d.id) }),
        state.desktop && isLive()
          ? button('Save a copy…', { icon: 'down', size: 'sm', id: 'doc-save-copy', title: 'Writes a Word, Excel or PowerPoint copy of the team version to your folder, without opening it.', onClick: () => void exportFile(d.id, false) })
          : button('Download', { icon: 'down', size: 'sm', disabled: true, title: 'Exporting a native file is in the web workspace; not in this release of the client.' }),
      ),
    ),
    h('div', { class: 'section-title' }, 'Content at main', h('span', { class: 'muted small' }, ' · read only')),
    h(
      'div',
      { class: cx('doc-preview', `preview-${d.kind}`) },
      d.blocks.map((b) =>
        h('div', { class: cx('pv-block', /Heading|Title/.test(b.label) && 'is-heading') }, h('span', { class: 'pv-label' }, b.label), h('span', { class: 'pv-text' }, b.text)),
      ),
    ),
  );
}

function versions(d: Doc): HTMLElement {
  return h(
    'div',
    null,
    h('div', { class: 'row-actions' }, button('Save named version…', { icon: 'plus', size: 'sm', onClick: () => saveVersionDialog(d) })),
    d.versions.length
      ? h(
          'div',
          { class: 'list-card' },
          d.versions.map((v) =>
            h(
              'div',
              { class: 'list-item' },
              icon('pin', 15),
              h('div', { class: 'grow' }, h('div', null, h('strong', null, v.name), h('span', { class: 'muted' }, ` · through r${v.throughSeq} · ${personName(v.authorId)} · ${when(v.at)}`)), v.note ? h('div', { class: 'muted small' }, v.note) : null),
              button('Compare with main', { size: 'sm', onClick: () => compareVersion(d, v.id) }),
              button('Restore as new changes', { size: 'sm', onClick: () => act(() => { const dr = B.restoreVersion(state.db, d.id, v.id); switchBranch(dr.id); return `Restored "${v.name}" into the draft ${dr.name}`; }) }),
            ),
          ),
        )
      : empty('pin', 'No named versions', 'Name a version to find it again: an edition, a submitted copy.'),
  );
}

function compareVersion(d: Doc, versionId: string): void {
  const v = d.versions.find((x) => x.id === versionId);
  if (!v) return;
  const then = B.blocksAt(state.db, d.id, v.throughSeq);
  const edits = compareBlocks(then, d.blocks);
  openDialog({ title: `"${v.name}" compared with main`, width: 900, body: () => diffView(then, edits, { emptyText: 'main still matches this version.' }), actions: () => button('Close', { onClick: closeDialog }) });
}

function saveVersionDialog(d: Doc): void {
  let name = '', note = '';
  openDialog({
    title: 'Save named version',
    width: 520,
    body: () => [
      h('p', { class: 'dialog-text' }, `Names main as it is now (r${B.mainSeq(state.db, d.id)}). History is not changed.`),
      field('Name', input(name, (v) => { name = v; }, { id: 'ver-name', autofocus: true, placeholder: 'Winter 2027 edition' })),
      field('Note', textarea(note, (v) => { note = v; }, { id: 'ver-note', rows: 2 })),
    ],
    actions: () => [button('Cancel', { onClick: closeDialog }), button('Save version', { variant: 'primary', onClick: () => { if (act(() => B.saveVersion(state.db, d.id, name, note), 'Saved the version')) closeDialog(); } })],
  });
}

function drafts(d: Doc): HTMLElement {
  const list = state.db.drafts.filter((x) => x.docId === d.id).sort((a, b) => b.createdAt - a.createdAt);
  return h(
    'div',
    null,
    h('div', { class: 'row-actions' }, button('New draft…', { icon: 'plus', size: 'sm', onClick: () => newDraftDialog(d.id) })),
    h(
      'div',
      { class: 'list-card' },
      list.map((x) =>
        h(
          'div',
          { class: 'list-item' },
          icon('branch', 15),
          h('div', { class: 'grow' }, h('div', null, h('strong', null, x.name), ' ', pill(draftWord(x.state), draftTone(x.state))), h('div', { class: 'muted small' }, `${personName(x.authorId)} · based on r${x.baseSeq} · ${B.branchCommits(state.db, d.id, x.id).length} commit(s) · ${when(x.createdAt)}${B.behind(state.db, x) && (x.state === 'private' || x.state === 'in_review') ? ` · ${B.behind(state.db, x)} behind main` : ''}`)),
          (x.state === 'private' || x.state === 'in_review') && x.authorId === state.db.me ? button('Switch to', { size: 'sm', onClick: () => switchBranch(x.id) }) : null,
        ),
      ),
      list.length ? null : h('div', { class: 'muted small pad' }, 'No drafts of this file.'),
    ),
  );
}

function discussions(d: Doc): HTMLElement {
  return h(
    'div',
    { class: 'list-card' },
    d.discussions.map((t) => {
      const block = d.blocks.find((b) => b.id === t.blockId);
      return h(
        'div',
        { class: cx('thread', t.resolved && 'is-resolved') },
        h('div', { class: 'thread-quote' }, h('span', { class: 'muted small' }, block?.label ?? t.blockId), h('div', { class: 'quote' }, block?.text ?? '')),
        [{ authorId: t.authorId, text: t.text, at: t.at }, ...t.replies].map((c) => h('div', { class: 'comment' }, avatar(personName(c.authorId)), h('div', { class: 'comment-body' }, h('div', { class: 'comment-meta' }, h('strong', null, personName(c.authorId)), h('span', { class: 'muted' }, when(c.at))), h('div', { class: 'comment-text' }, c.text)))),
        h('div', { class: 'row-actions' },
          button(t.resolved ? 'Reopen' : 'Resolve', { size: 'sm', onClick: () => { t.resolved = !t.resolved; render(); } }),
          button('Reply…', { size: 'sm', variant: 'ghost', onClick: () => replyDialog(t) })),
      );
    }),
    d.discussions.length ? null : h('div', { class: 'muted small pad' }, 'No discussions. Comments are added in the editor.'),
  );
}

function replyDialog(t: Doc['discussions'][number]): void {
  let text = '';
  openDialog({ title: 'Reply', width: 480, body: () => textarea(text, (v) => { text = v; }, { id: 'reply-text', rows: 3 }), actions: () => [button('Cancel', { onClick: closeDialog }), button('Reply', { variant: 'primary', onClick: () => { if (!text.trim()) return; t.replies.push({ authorId: state.db.me, text: text.trim(), at: Date.now() }); closeDialog(); } })] });
}

function people(d: Doc): HTMLElement {
  const p = B.project(state.db, d.projectId);
  return h(
    'div',
    null,
    h('div', { class: 'card pad' }, h('div', { class: 'srow-title' }, 'Review rules'), h('div', { class: 'muted small' }, `From the project ${p.name}: ${p.rules.protectMain ? 'main is protected (edits go through review)' : 'editors may write main'}, ${p.rules.requiredApprovals} approval${p.rules.requiredApprovals === 1 ? '' : 's'} needed, ${p.rules.mergeRoles === 'owners' ? 'owners and managers' : 'editors and above'} merge. Change them in Projects, Review rules.`)),
    h('div', { class: 'list-card' }, p.members.map((m) => h('div', { class: 'list-item' }, avatar(personName(m.personId), isAgent(m.personId)), h('div', { class: 'grow' }, h('strong', null, personName(m.personId)), h('div', { class: 'muted small' }, B.person(state.db, m.personId).email)), badge(ROLE_LABEL[m.role]), h('span', { class: 'muted small' }, m.via === 'workspace' ? 'via workspace' : 'direct')))),
  );
}

function sync(d: Doc): HTMLElement {
  const hosts = HOSTS_FOR[d.kind];
  let host: AddinHost = hosts[0];
  return h(
    'div',
    null,
    h('div', { class: 'section-title' }, 'Connection keys'),
    h('p', { class: 'muted small' }, 'An add-in connects to one file with a key. A key works for 7 days, can read the file and its history and propose changes; it cannot merge.'),
    h(
      'div',
      { class: 'row-actions' },
      select(host, hosts.map((x) => [x, HOST_LABEL[x]] as [AddinHost, string]), (v) => { host = v; }, { label: 'Editor' }),
      button('Create connection key', { size: 'sm', variant: 'primary', onClick: () => createKey(d, host) }),
    ),
    h(
      'div',
      { class: 'list-card' },
      d.connections.map((k) => {
        const expired = k.expiresAt <= Date.now();
        return h(
          'div',
          { class: 'list-item' },
          icon('key', 15),
          h('div', { class: 'grow' }, h('strong', null, HOST_LABEL[k.host]), h('div', { class: 'muted small' }, `created ${when(k.createdAt)} · ${k.revoked ? 'revoked' : expired ? 'expired' : `expires ${when(k.expiresAt)}`}${k.lastUsed ? ` · last used ${when(k.lastUsed)}` : ''}`)),
          pill(k.revoked ? 'revoked' : expired ? 'expired' : 'active', k.revoked || expired ? 'neutral' : 'success'),
          !k.revoked && !expired ? button('Revoke', { size: 'sm', variant: 'ghost', onClick: () => confirmDialog('Revoke connection key', h('p', null, `The ${HOST_LABEL[k.host]} add-in loses access to ${d.filename} at once.`), 'Revoke', () => { k.revoked = true; toast('success', 'Key revoked'); render(); }, { danger: true }) }) : null,
        );
      }),
      d.connections.length ? null : h('div', { class: 'muted small pad' }, 'No connection keys.'),
    ),
    h('div', { class: 'section-title' }, 'Read-only links'),
    h('div', { class: 'row-actions' }, button('Publish a read-only copy', { size: 'sm', onClick: () => { d.publicLinks.unshift({ id: `pl${Date.now()}`, createdAt: Date.now(), expiresAt: Date.now() + 30 * 86400000, revoked: false }); toast('success', 'Published a read-only copy', 'It shows main as it is now and does not include later edits.'); render(); } })),
    h('div', { class: 'list-card' }, d.publicLinks.map((l) => h('div', { class: 'list-item' }, icon('link', 15), h('div', { class: 'grow' }, h('strong', null, 'Published copy'), h('div', { class: 'muted small' }, `${when(l.createdAt)} · ${l.revoked ? 'revoked' : l.expiresAt ? `until ${when(l.expiresAt)}` : 'until revoked'}`)), l.revoked ? pill('revoked') : button('Revoke', { size: 'sm', variant: 'ghost', onClick: () => { l.revoked = true; render(); } }))), d.publicLinks.length ? null : h('div', { class: 'muted small pad' }, 'No published copies.')),
    h('div', { class: 'section-title' }, 'Copies and similar files'),
    h('div', { class: 'list-card' }, d.copies.map((c) => h('div', { class: 'list-item' }, icon('documents', 15), h('div', { class: 'grow' }, h('strong', null, c.title), h('div', { class: 'muted small' }, `${c.relation.replace('_', ' ')} · ${Math.round(c.score * 100)}% similar`)), pill(c.status), c.status === 'open' ? [button('Invite to send changes', { size: 'sm', onClick: () => { c.status = 'invited'; render(); } }), button('Dismiss', { size: 'sm', variant: 'ghost', onClick: () => { c.status = 'dismissed'; render(); } })] : null)), d.copies.length ? null : h('div', { class: 'muted small pad' }, 'Dynodoc has found no copies of this file.')),
  );
}

function createKey(d: Doc, host: AddinHost): void {
  // A sample key: random characters, shown once, kept nowhere.
  const bytes = new Uint8Array(24);
  crypto.getRandomValues(bytes);
  const token = 'ddk_sample_' + Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
  d.connections.unshift({ id: `ck${Date.now()}`, host, createdAt: Date.now(), expiresAt: Date.now() + 7 * 86400000, revoked: false });
  openDialog({
    title: 'Connection key created',
    width: 560,
    body: () => [
      h('p', { class: 'dialog-text' }, `Paste this key into the ${HOST_LABEL[host]} add-in under "Advanced: connect with a key". It is shown once and works for 7 days.`),
      h('input', { class: 'input mono', readonly: true, value: token, id: 'conn-key', 'aria-label': 'Connection key' }),
      notice('info', 'Sample data: this key is random characters and connects nothing.'),
    ],
    actions: () => [button('Copy', { onClick: async () => { try { await navigator.clipboard.writeText(token); toast('success', 'Copied'); } catch { toast('warning', 'Select the key and copy it'); } } }), button('Done', { variant: 'primary', onClick: closeDialog })],
  });
}

export function documentsView(): HTMLElement {
  const ui = state.ui.documents;
  const docs = B.docsOf(state.db, state.ui.projectId);
  const sel = docs.find((d) => d.id === ui.selected);
  if (!sel) {
    return h(
      'div',
      { class: 'page' },
      viewHeader('Documents', `${docs.length} file${docs.length === 1 ? '' : 's'} in ${B.project(state.db, state.ui.projectId).name}`, button('New draft…', { icon: 'branch', onClick: () => newDraftDialog() }), state.desktop && isLive() ? button('Add files…', { icon: 'plus', variant: 'primary', id: 'add-files', onClick: () => void addFilesDialog() }) : button('Add file', { icon: 'plus', disabled: true, title: 'Uploading files is in the web workspace and the add-ins; not in this release of the client.' })),
      docs.length ? h('div', { class: 'card flush' }, fileTable(docs)) : empty('documents', 'No files yet', isLocal() ? 'Add Word, Excel or PowerPoint files from your computer. Each keeps its history from the moment you add it.' : 'Add a file in the web workspace or from an add-in.', state.desktop && isLive() ? button('Add files…', { variant: 'primary', icon: 'plus', onClick: () => void addFilesDialog() }) : undefined),
    );
  }
  const counts = { drafts: state.db.drafts.filter((x) => x.docId === sel.id).length, versions: sel.versions.length, disc: sel.discussions.filter((t) => !t.resolved).length };
  const body = ui.tab === 'versions' ? versions(sel) : ui.tab === 'drafts' ? drafts(sel) : ui.tab === 'discussions' ? discussions(sel) : ui.tab === 'people' ? people(sel) : ui.tab === 'sync' ? sync(sel) : overview(sel);
  return h(
    'div',
    { class: 'page' },
    h('div', { class: 'crumbs' }, h('button', { type: 'button', class: 'link', onClick: () => { ui.selected = null; render(); } }, 'Documents'), h('span', { class: 'muted' }, ' / '), sel.filename),
    viewHeader(sel.title, h('span', { class: 'inline-meta' }, fileIcon(sel.kind, 14), `${sel.filename} · main r${B.mainSeq(state.db, sel.id)}`)),
    tabs(ui.tab, [['overview', 'Overview'], ['versions', 'Versions', counts.versions], ['drafts', 'Drafts', counts.drafts], ['discussions', 'Discussions', counts.disc], ['people', 'People & rules'], ['sync', 'Sync & connections']], (v) => { ui.tab = v; render(); }, 'File sections'),
    h('div', { class: 'tab-body' }, body),
  );
}
