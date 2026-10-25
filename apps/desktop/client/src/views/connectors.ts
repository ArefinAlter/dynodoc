// Connectors: the catalogue of editor add-ins and imports, each with how it signs in, what it
// reaches and its status, and for the add-ins how many files are connected now.
import { h, cx } from '../dom.ts';
import { icon } from '../icons.ts';
import { state, render } from '../state.ts';
import { CONNECTORS, type Connector } from '../data/connectors.ts';
import { button, input, pill, segmented, notice } from '../ui/controls.ts';
import { openDialog, closeDialog } from '../ui/layer.ts';
import { viewHeader } from './common.ts';

export function installSteps(c: Connector): void {
  const office = c.host === 'word' || c.host === 'excel' || c.host === 'powerpoint';
  openDialog({
    title: `Install the ${c.name}`,
    width: 600,
    body: () => [
      h('ol', { class: 'steps' },
        office
          ? [
              h('li', null, 'Download the add-in manifest for your server from the web workspace (Connectors, Add Dynodoc to your editor).'),
              h('li', null, 'In the editor: Insert, Add-ins, My add-ins, Upload my add-in, and choose the manifest. An administrator can deploy it for everyone instead.'),
              h('li', null, 'Open a file, open the Dynodoc pane and choose Sign in to Dynodoc. Compare the pairing code, approve the connection in your browser, then choose "I approved this connection".'),
              h('li', null, 'Turn on capture. Your edits appear in this client under Changes.'),
            ]
          : [
              h('li', null, 'Download the Apps Script package for your server from the web workspace (Connectors).'),
              h('li', null, 'Create a container-bound script in the file (Extensions, Apps Script) and paste the package files.'),
              h('li', null, 'Reload the file, open Extensions, Dynodoc, and sign in with the pairing code.'),
              h('li', null, 'Turn on capture.'),
            ]),
      notice('warning', c.caveat),
    ],
    actions: () => button('Close', { onClick: closeDialog }),
  });
}

export function connectorsView(): HTMLElement {
  const ui = state.ui.connectors;
  const q = ui.filter.toLowerCase();
  const list = CONNECTORS.filter((c) => (ui.category === 'all' || c.category === ui.category) && `${c.name} ${c.summary} ${c.hosts.join(' ')}`.toLowerCase().includes(q));
  const connected = (c: Connector) => (c.host ? state.db.docs.filter((d) => d.connections.some((k) => k.host === c.host && !k.revoked && k.expiresAt > Date.now())).length : 0);
  return h(
    'div',
    { class: 'page' },
    viewHeader('Connectors', 'The editor add-ins where editing happens, and the imports the workspace offers.'),
    h('div', { class: 'toolbar' },
      h('div', { class: 'search-field' }, icon('search', 14), input(ui.filter, (v) => { ui.filter = v; render(); }, { id: 'connectors-filter', placeholder: 'Search connectors' })),
      segmented(ui.category, [['all', 'All'], ['Editor add-in', 'Editor add-ins'], ['Import', 'Imports'], ['History', 'History'], ['Agent access', 'Agents']], (v) => { ui.category = v; render(); }, 'Category')),
    h(
      'div',
      { class: 'connector-grid' },
      list.map((c) => {
        const n = connected(c);
        return h(
          'div',
          { class: 'connector-card' },
          h('div', { class: 'connector-head' }, h('span', { class: 'connector-icon' }, icon(c.category === 'Editor add-in' ? (c.host?.includes('sheet') || c.host === 'excel' ? 'sheet' : c.host?.includes('slide') || c.host === 'powerpoint' ? 'slides' : 'word') : c.category === 'Import' ? 'down' : c.category === 'History' ? 'history' : 'agent', 18)),
            h('div', { class: 'grow' }, h('strong', null, c.name), h('div', { class: 'muted small' }, c.category)), pill(c.status, c.status === 'Built' ? 'success' : 'neutral', { mono: false })),
          h('p', { class: 'small' }, c.summary),
          h('dl', { class: 'facts small' }, h('dt', null, 'Sign-in'), h('dd', null, c.auth), h('dt', null, 'Reaches'), h('dd', null, c.hosts.join(', ')), h('dt', null, 'In Dynodoc'), h('dd', { class: 'mono' }, c.where)),
          h('div', { class: cx('connector-foot') },
            c.host ? h('span', { class: cx('conn-state', n > 0 && 'is-on') }, h('span', { class: 'pill-dot' }), n ? `${n} file${n === 1 ? '' : 's'} connected` : 'No file connected') : h('span', { class: 'muted small' }, c.caveat),
            h('span', { class: 'spacer' }),
            c.category === 'Editor add-in' ? button('Install…', { size: 'sm', onClick: () => installSteps(c) }) : null),
          c.category === 'Editor add-in' ? h('div', { class: 'muted small' }, c.caveat) : null,
        );
      }),
    ),
  );
}
