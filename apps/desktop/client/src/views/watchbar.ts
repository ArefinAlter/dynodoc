// Notices from the folder watch: files in the project's folder that are not in the project yet,
// and files an editor saved (which became a draft). Shown at the top of Changes and Documents.
import { h } from '../dom.ts';
import { state, render } from '../state.ts';
import { button, notice } from '../ui/controls.ts';
import { addFolderFiles, ignoreFolderFiles } from '../data/files.ts';
import { short } from './common.ts';
import { switchBranch } from './actions.ts';
import { go } from '../state.ts';

export function watchNotices(projectId: string): HTMLElement | null {
  const fresh = state.watch.newFiles.filter((n) => n.project_id === projectId);
  const recent = state.watch.recent.filter((r) => state.db.docs.some((d) => d.id === r.docId && d.projectId === projectId)).slice(0, 3);
  if (!fresh.length && !recent.length) return null;
  const out: HTMLElement[] = [];
  if (fresh.length) {
    const names = fresh.slice(0, 3).map((f) => f.name).join(', ') + (fresh.length > 3 ? ` and ${fresh.length - 3} more` : '');
    out.push(
      h(
        'div',
        { class: 'watch-notice', id: 'watch-new' },
        notice('info', h('strong', null, `${fresh.length} file${fresh.length === 1 ? '' : 's'} in the project's folder ${fresh.length === 1 ? 'is' : 'are'} not in the project yet`), h('div', { class: 'muted small' }, names)),
        h('div', { class: 'watch-actions' }, button('Add to the project', { variant: 'primary', size: 'sm', id: 'watch-add', onClick: () => void addFolderFiles(projectId) }), button('Ignore', { size: 'sm', onClick: () => ignoreFolderFiles(projectId) })),
      ),
    );
  }
  for (const r of recent) {
    out.push(
      h(
        'div',
        { class: 'watch-notice' },
        notice('success', h('strong', null, `${r.name} was saved in its editor ${short(r.at)} ago`), h('div', { class: 'muted small' }, `${r.changes} change${r.changes === 1 ? '' : 's'} are in the draft "${r.draftName}" of ${r.title}.`)),
        h(
          'div',
          { class: 'watch-actions' },
          button('Review the changes', {
            variant: 'primary',
            size: 'sm',
            onClick: () => {
              if (state.db.drafts.some((d) => d.id === r.draftId)) switchBranch(r.draftId);
              state.watch.recent = state.watch.recent.filter((x) => x !== r);
              go('history');
            },
          }),
          button('Dismiss', { size: 'sm', onClick: () => { state.watch.recent = state.watch.recent.filter((x) => x !== r); render(); } }),
        ),
      ),
    );
  }
  return h('div', { class: 'watch-notices' }, out);
}
