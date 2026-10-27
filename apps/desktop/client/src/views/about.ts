// About: what this release is, what is real and where each part runs.
import { h, type Child } from '../dom.ts';
import { state } from '../state.ts';
import { section, rows, row, pill } from '../ui/controls.ts';
import { isLive } from '../data/live.ts';

export function aboutBody(): Child {
  const shell = state.desktop;
  const live = isLive();
  const local = live && state.live.env === 'local';
  return [
    h('div', { class: 'about-hero card pad' },
      h('div', { class: 'about-title' }, h('strong', null, 'Dynodoc'), ' ', pill('Pre-alpha release', 'warning', { mono: false })),
      h('p', null, 'Version control for documents, spreadsheets and presentations: edits in Word, Excel, PowerPoint and Google’s editors become change requests, are reviewed and merged, and land in a hash-chained history.'),
      h('p', { class: 'muted small' }, `Client ${__CLIENT_VERSION__}${shell ? `, desktop app ${state.desktopVersion}` : __WEB__ ? ', web copy' : ''}.`)),
    local
      ? section('This window shows the workspace on this computer', null, h('p', null,
          'Your files, their history, drafts, change requests and reviews are in a database this app started on this computer. It needs no server, no account and no network. Everything you do here is saved there, and nothing leaves this computer unless you sign in to a server and sync a project (Settings, Accounts) or use your own model provider for the agent (Settings, Providers).'))
      : live
        ? section('This window shows a real Dynodoc server', null, h('p', null,
            `The screens read and change your workspace on ${state.live.server || 'the server'}. `,
            state.db.remote?.visitor
              ? 'It is a sample workspace made for you alone: the projects, files and colleagues in it are invented (example.org), and it is removed after 24 hours.'
              : 'What you see is what the server holds; every change you make is sent to it.'))
        : section('This window shows an offline sample', null, h('p', null,
            'Every project, person, file, revision and figure here is invented, and a script inside the page stands in for a Dynodoc server: nothing you do leaves this ',
            shell ? 'window.' : 'tab. The page makes no network requests. The desktop app has the real workspace.')),
    section('Where each part runs', null, rows(
      row('On this computer', 'The workspace: a PostgreSQL database and the Dynodoc engine, started by the app on loopback addresses only and stopped when it closes. Your files are imported from and exported to Word, Excel and PowerPoint formats by a converter that runs in this window without network access. Preferences are a file in the app’s settings folder; secrets (the database passwords, sessions on servers, your provider keys) are in the operating system’s credential store.', null),
      row('On a server you sign in to', 'Only what you link and sync: the content of the files of a linked project. History, comments, change requests and discussions stay where they were made. Each side keeps its own hash-chained history and verifies it on its own.', null),
      row('At a model provider', 'Only when you turn the agent on with your own key, allow that provider’s address, and ask a question: the conversation and the tools’ answers go from this computer to that address, and nowhere else. No Dynodoc server sees them.', null),
      row('Not in this release', 'Editing a document’s content inside this window (use Word, Excel or PowerPoint on a copy; saving it in the project’s folder turns the changes into a draft), the Google editors without a server, macOS (not built yet), automatic background sync (you press Sync), and phones (the Android and iPhone apps are separate and still need a server).', null),
    )),
    section('Privacy', null, rows(
      row('No telemetry', 'No analytics, no crash reports, no usage statistics.', null, { fixed: true }),
      row('Keys stay in the credential store', 'Provider keys and sessions are kept by the operating system’s credential store, shown here only as “stored” with the last four characters, and never written to a log or this window’s storage. Where a system has no credential store (some Linux sessions) provider keys go to one file encrypted with a passphrase you type, and sessions to a file only you can read.', null, { fixed: true }),
    )),
    __WEB__ ? null : h('p', { class: 'muted small' }, 'A Verne Ltd. product.'),
  ];
}
