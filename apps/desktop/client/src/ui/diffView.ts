// Draws a diff from the diff model: hunks with a header, then unified or side-by-side rows. A
// block's label (¶ 4, Budget!C7 · Travel, Slide 3 · Title) stands where a line number would be.
import { h, cx, type Child } from '../dom.ts';
import { hunks, split, unified, type Block, type Edit, type Seg } from '../lib/diff.ts';
import { state } from '../state.ts';
import { icon } from '../icons.ts';

function segs(list: Seg[], word: boolean): Child[] {
  if (!list.length) return [h('span', { class: 'seg-empty' }, '(empty)')];
  return list.map((g) => (g.t === 'same' || !word ? h('span', null, g.s) : h('span', { class: g.t === 'add' ? 'w-add' : 'w-del' }, g.s)));
}

/** Column widths for a fixed table layout (the header row spans all columns). */
function cols(kinds: string[]): HTMLElement {
  return h('colgroup', null, kinds.map((k) => h('col', { class: `col-${k}` })));
}

export interface DiffOpts {
  /** Block ids left out (a reviewer's choice); their rows are dimmed. */
  excluded?: string[];
  /** Adds a checkbox per changed block. */
  onToggle?: (blockId: string, include: boolean) => void;
  /** Comments per block, shown under the block. */
  comments?: Record<string, Child[]>;
  /** Adds a "comment" button per changed block. */
  onComment?: (blockId: string) => void;
  /** Overrides the settings. */
  layout?: 'unified' | 'split';
  context?: number;
  /** Shown when there is nothing to compare. */
  emptyText?: string;
}

export function diffView(base: Block[], edits: Edit[], o: DiffOpts = {}): HTMLElement {
  const p = state.prefs;
  const layout = o.layout ?? p.diffLayout;
  const context = o.context ?? p.diffContext;
  const list = hunks(base, edits, context);
  if (!list.length) return h('div', { class: 'diff-empty' }, o.emptyText ?? 'No changes.');
  const excluded = new Set(o.excluded ?? []);
  const seenToggle = new Set<string>();
  const control = (blockId: string, changed: boolean): Child => {
    if (!changed) return h('span', { class: 'diff-gutter-ctl' });
    const parts: Child[] = [];
    if (o.onToggle && !seenToggle.has(blockId)) {
      seenToggle.add(blockId);
      parts.push(
        h('input', {
          type: 'checkbox',
          class: 'check',
          checked: !excluded.has(blockId),
          'aria-label': `Include ${blockId}`,
          onChange: (e: Event) => o.onToggle?.(blockId, (e.target as HTMLInputElement).checked),
        }),
      );
    } else if (p.checkMarks && !o.onToggle) {
      parts.push(h('span', { class: 'diff-mark' }, icon('check', 12)));
    }
    return h('span', { class: 'diff-gutter-ctl' }, parts);
  };
  const after = (blockId: string, changed: boolean): Child[] => {
    const out: Child[] = [];
    if (changed && o.onComment) out.push(h('button', { type: 'button', class: 'diff-comment-btn', title: 'Comment on this change', 'aria-label': 'Comment on this change', onClick: () => o.onComment?.(blockId) }, icon('comment', 13)));
    return out;
  };
  const commentRow = (blockId: string, span: number): Child => {
    const c = o.comments?.[blockId];
    if (!c || !c.length) return null;
    return h('tr', { class: 'diff-comments' }, h('td', { colspan: span }, h('div', { class: 'diff-comment-list' }, c)));
  };

  return h(
    'div',
    { class: cx('diff', `diff-${layout}`), css: { '--tab-size': String(p.tabSize) } },
    list.map((hk) => {
      if (layout === 'split') {
        const lines = split(hk);
        const done = new Set<string>();
        return h(
          'table',
          { class: 'diff-table' },
          cols(['ctl', 'no', 'cell', 'no', 'cell', 'after']),
          h('thead', null, h('tr', null, h('th', { colspan: 6, class: 'diff-hunk' }, hk.header))),
          h(
            'tbody',
            null,
            lines.map((l) => {
              const changed = l.kind !== 'context';
              const first = !done.has(l.blockId);
              done.add(l.blockId);
              return [
                h(
                  'tr',
                  { class: cx('diff-row', `k-${l.kind}`, excluded.has(l.blockId) && 'is-excluded') },
                  h('td', { class: 'diff-ctl' }, first ? control(l.blockId, changed) : null),
                  h('td', { class: 'diff-no' }, l.left ? String(l.left.no) : ''),
                  h('td', { class: cx('diff-cell', l.left && changed && (l.kind === 'del' || l.kind === 'edit') && 'side-del', !l.left && 'side-none') }, l.left ? [h('div', { class: 'diff-label' }, l.left.label), h('div', { class: 'diff-text' }, segs(l.left.segs, p.wordDiff))] : null),
                  h('td', { class: 'diff-no' }, l.right ? String(l.right.no) : ''),
                  h('td', { class: cx('diff-cell', l.right && changed && (l.kind === 'add' || l.kind === 'edit') && 'side-add', !l.right && 'side-none') }, l.right ? [h('div', { class: 'diff-label' }, l.right.label), h('div', { class: 'diff-text' }, segs(l.right.segs, p.wordDiff))] : null),
                  h('td', { class: 'diff-after' }, after(l.blockId, changed)),
                ),
                commentRow(l.blockId, 6),
              ];
            }),
          ),
        );
      }
      const lines = unified(hk);
      return h(
        'table',
        { class: 'diff-table' },
        cols(['ctl', 'no', 'no', 'sign', 'cell', 'after']),
        h('thead', null, h('tr', null, h('th', { colspan: 6, class: 'diff-hunk' }, hk.header))),
        h(
          'tbody',
          null,
          lines.map((l, i) => {
            const changed = l.sign !== ' ';
            const last = !lines[i + 1] || lines[i + 1].blockId !== l.blockId;
            return [
              h(
                'tr',
                { class: cx('diff-row', l.sign === '-' ? 'k-del' : l.sign === '+' ? 'k-add' : 'k-context', excluded.has(l.blockId) && 'is-excluded') },
                h('td', { class: 'diff-ctl' }, control(l.blockId, changed)),
                h('td', { class: 'diff-no' }, l.oldNo === undefined ? '' : String(l.oldNo)),
                h('td', { class: 'diff-no' }, l.newNo === undefined ? '' : String(l.newNo)),
                h('td', { class: 'diff-sign', 'aria-hidden': 'true' }, l.sign),
                h('td', { class: 'diff-cell' }, h('div', { class: 'diff-label' }, l.label), h('div', { class: 'diff-text' }, segs(l.segs, p.wordDiff))),
                h('td', { class: 'diff-after' }, l.sign === '+' || (l.sign === '-' && last) ? after(l.blockId, changed) : null),
              ),
              last ? commentRow(l.blockId, 6) : null,
            ];
          }),
        ),
      );
    }),
  );
}
