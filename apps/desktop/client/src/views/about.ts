// About: what this release is, what is real and what is prepared.
import { h, type Child } from '../dom.ts';
import { state } from '../state.ts';
import { section, rows, row, pill } from '../ui/controls.ts';


export function aboutBody(): Child {
  const shell = state.desktop;
  return [
    h('div', { class: 'about-hero card pad' },
      h('div', { class: 'about-title' }, h('strong', null, 'Dynodoc'), ' ', pill('Pre-alpha release', 'warning', { mono: false })),
      h('p', null, 'Version control for documents, spreadsheets and presentations: edits in Word, Excel, PowerPoint and Google’s editors become change requests, are reviewed and merged, and land in a hash-chained history.'),
      h('p', { class: 'muted small' }, `Client ${__CLIENT_VERSION__}${shell ? `, desktop app ${state.desktopVersion}` : __WEB__ ? ', web copy' : ''}.`)),
    section('This release runs on sample data', null, h('p', null,
      'Every project, person, file, revision and figure here is invented. There is no backend behind these screens: a script inside the page stands in for a Dynodoc server, and nothing you do here leaves this ',
      shell ? 'window.' : 'tab. The page makes no network requests.')),
    section('Real and prepared', null, rows(
      row('Real', 'The interface and its rules: the diff model (paragraph, cell and slide text, with word marks and context), staging and committing, the hash chain (each revision’s SHA-256 chain hash is computed and “Verify” recomputes it), drafts, rebase with conflict choices, the review rules (approvals, who may merge), revert and cherry-pick as recovery drafts, the agent’s closed tool table and its access check, the keybinding conflict check, the provider ladder’s validation.', null),
      row('Prepared', 'The data, the arrival of new revisions when you fetch, and the agent’s replies: no model is called. Connection keys shown here are random characters that connect nothing.', null),
      shell ? row('Real, in the desktop app', 'Accounts, Dynodoc server: choosing a server, signing in with your browser or an emailed link, and this computer’s signing key use the app’s own commands, unchanged. Opening your server shows its real workspace in this window.', null) : null,
      row('Not in this release', 'Reading your real server’s data in these screens, editing documents (that is the add-ins’ job), uploading files, exports and administration actions.', null),
    )),
    section('Privacy', null, rows(
      row('No telemetry', 'No analytics, no crash reports, no usage statistics.', null, { fixed: true }),
      row('Keys stay in memory', 'Provider keys typed in Settings are held in memory only, shown as “stored” with the last four characters, and never sent.', null, { fixed: true }),
    )),
    __WEB__ ? null : h('p', { class: 'muted small' }, 'A Verne Ltd. product.'),
  ];
}
