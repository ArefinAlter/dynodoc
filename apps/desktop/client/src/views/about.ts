// About: what this release is, what is real and where each part runs.
import { h, type Child } from '../dom.ts';
import { state } from '../state.ts';
import { section, rows, row, pill } from '../ui/controls.ts';
import { isLive } from '../data/live.ts';

export function aboutBody(): Child {
  const shell = state.desktop;
  const live = isLive();
  return [
    h('div', { class: 'about-hero card pad' },
      h('div', { class: 'about-title' }, h('strong', null, 'Dynodoc'), ' ', pill('Pre-alpha release', 'warning', { mono: false })),
      h('p', null, 'Version control for documents, spreadsheets and presentations: edits in Word, Excel, PowerPoint and Google’s editors become change requests, are reviewed and merged, and land in a hash-chained history.'),
      h('p', { class: 'muted small' }, `Client ${__CLIENT_VERSION__}${shell ? `, desktop app ${state.desktopVersion}` : __WEB__ ? ', web copy' : ''}.`)),
    live
      ? section('This window shows a real Dynodoc server', null, h('p', null,
          `The screens read and change your workspace on ${state.live.server || 'the server'}. `,
          state.db.remote?.visitor
            ? 'It is a sample workspace made for you alone: the projects, files and colleagues in it are invented (example.org), and it is removed after 24 hours.'
            : 'What you see is what the server holds; every change you make is sent to it.'))
      : section('This window shows an offline sample', null, h('p', null,
          'Every project, person, file, revision and figure here is invented, and a script inside the page stands in for a Dynodoc server: nothing you do leaves this ',
          shell ? 'window. Settings, Accounts starts a sample workspace on the hosted pre-alpha server, or signs in to yours.' : 'tab. The page makes no network requests.')),
    section('Where each part runs', null, rows(
      row('On the Dynodoc server', 'Projects, files and their content, the hash-chained history (verified by the server), drafts, change requests, reviews, merges with conflict choices, versions, discussions, published copies, people and roles, review rules, the AI agent policy, issues and labels, stars and watching, notifications and signing keys. The desktop app reaches them through the server’s own web workspace routes, with your session, so the server’s permission rules apply unchanged.', null),
      row('On the pre-alpha service', 'Beside the server (the same address, under /svc): the changes your editor add-ins captured and nobody committed yet, commits you have not pushed, commit messages (the server records changes, not messages), agent threads and dismissed copies. It also makes the sample workspaces. It is part of this pre-alpha release, not of the product.', null),
      row('On this computer', 'Your preferences (theme, keybindings, providers without their keys) in the app’s settings file; this computer’s signing key in the operating system’s key store. Provider keys are held in memory only.', null),
      row('Prepared, not computed', 'The agent’s replies: no model is called. The connector catalogue lists what exists; connection keys come from the server.', null),
      row('Not in this release', 'Editing a document’s content in this client (that is the add-ins’ and the web workspace’s job), uploads, exports and administration actions. macOS is not built yet.', null),
    )),
    section('Privacy', null, rows(
      row('No telemetry', 'No analytics, no crash reports, no usage statistics.', null, { fixed: true }),
      row('Keys stay in memory', 'Provider keys typed in Settings are held in memory only, shown as “stored” with the last four characters, and never sent or saved.', null, { fixed: true }),
    )),
    __WEB__ ? null : h('p', { class: 'muted small' }, 'A Verne Ltd. product.'),
  ];
}
