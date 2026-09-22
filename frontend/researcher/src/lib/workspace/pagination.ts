import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { Decoration, DecorationSet, type EditorView } from '@tiptap/pm/view';
import type { Node as PMNode } from '@tiptap/pm/model';
import { pageDimensions, type PageSetup } from './office';

export const PAGE_GAP = 24;
export const MM_TO_PX = 96 / 25.4;
export interface PageLayout {
  pages: number;
  current: number;
  overflow: boolean;
}
interface Line {
  pos: number;
  top: number;
  bottom: number;
  kind: 'block' | 'line' | 'row';
  force?: boolean;
  keep?: boolean;
}
export interface PageGap {
  pos: number;
  height: number;
  kind: Line['kind'];
}

/** The plan contains view coordinates only. It never creates document nodes. */
export function planPages(lines: Line[], height: number, margin: number) {
  const body = height - margin * 2;
  const pitch = height + PAGE_GAP;
  const gaps: PageGap[] = [];
  let offset = 0,
    page = 0,
    overflow = false;
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    let bottom = line.bottom;
    if (line.keep && lines[i + 1]) bottom = lines[i + 1].bottom;
    const top = line.top + offset;
    const end = page * pitch + body;
    if (bottom - line.top > body) overflow = true;
    if (line.force || (bottom + offset > end + 0.5 && top > page * pitch + 0.5)) {
      // A pixel of clearance absorbs subpixel line-box rounding at CSS zooms.
      const next = (page + 1) * pitch + 1;
      const delta = Math.max(0, next - top);
      if (delta > 0) {
        gaps.push({ pos: line.pos, height: delta, kind: line.kind });
        offset += delta;
      }
      page++;
    }
    // An indivisible object taller than a page remains visible; do not loop forever.
    page = Math.max(page, Math.floor((line.bottom + offset - 0.5) / pitch));
  }
  return { gaps, pages: page + 1, overflow };
}

export const paginationKey = new PluginKey<DecorationSet>('livePagination');

function measure(view: EditorView): Line[] {
  const root = view.dom;
  const scale = root.getBoundingClientRect().width / root.offsetWidth || 1;
  const origin = root.getBoundingClientRect().top;
  const lines: Line[] = [];
  function box(el: Element) {
    const r = el.getBoundingClientRect();
    return { top: (r.top - origin) / scale, bottom: (r.bottom - origin) / scale };
  }
  function visit(node: PMNode, pos: number) {
    const dom = view.nodeDOM(pos);
    if (!(dom instanceof HTMLElement)) return;
    if (node.type.name === 'pageBreak') {
      const r = box(dom);
      lines.push({ pos: pos + node.nodeSize, ...r, bottom: r.top, kind: 'block', force: true });
      return;
    }
    if (node.type.name === 'table') {
      // Keep rows joined by rowspan together. A row must never be split by a widget.
      let rowPos = pos + 1;
      let through = -1;
      let group: Line | undefined;
      node.forEach((row, _offset, index) => {
        const rowDOM = view.nodeDOM(rowPos);
        if (rowDOM instanceof HTMLElement) {
          const r = box(rowDOM);
          if (index > through || !group) {
            group = { pos: rowPos, ...r, kind: 'row' };
            lines.push(group);
          } else group.bottom = r.bottom;
          row.forEach((cell) => {
            through = Math.max(through, index + Number(cell.attrs.rowspan || 1) - 1);
          });
        }
        rowPos += row.nodeSize;
      });
      return;
    }
    if (node.isTextblock) {
      const found = new Map<number, Line>();
      const walker = document.createTreeWalker(dom, NodeFilter.SHOW_TEXT);
      let text: Node | null;
      while ((text = walker.nextNode())) {
        if (!text.textContent?.length || text.parentElement?.closest('[contenteditable=false]'))
          continue;
        const range = document.createRange();
        range.selectNodeContents(text);
        const rects = Array.from(range.getClientRects()).filter((r) => r.height && r.width);
        const tops = [...new Set(rects.map((r) => Math.round(r.top * 2) / 2))];
        for (const top of tops) {
          // Binary search the first UTF-16 position on this visual line. Reading one
          // range per character makes large paragraphs prohibitively expensive.
          let low = 0,
            high = text.textContent.length;
          while (low < high) {
            const mid = Math.floor((low + high) / 2);
            range.setStart(text, mid);
            range.setEnd(text, Math.min(mid + 1, text.textContent.length));
            if (range.getBoundingClientRect().top < top - 0.5) low = mid + 1;
            else high = mid;
          }
          if (low >= text.textContent.length) continue;
          // Do not put a boundary between the two halves of a surrogate pair.
          if (low > 0 && /[\uDC00-\uDFFF]/.test(text.textContent[low])) low--;
          const at = view.posAtDOM(text, low);
          const matching = rects.filter((r) => Math.abs(r.top - top) < 0.6);
          const bottom = Math.max(...matching.map((r) => r.bottom));
          const key = Math.round((top - origin) / scale);
          const existing = found.get(key);
          if (existing) {
            existing.pos = Math.min(existing.pos, at);
            existing.bottom = Math.max(existing.bottom, (bottom - origin) / scale);
          } else
            found.set(key, {
              pos: at,
              top: (top - origin) / scale,
              bottom: (bottom - origin) / scale,
              kind: 'line'
            });
        }
      }
      const values = [...found.values()].sort((a, b) => a.top - b.top);
      if (!values.length) values.push({ pos, ...box(dom), kind: 'block' });
      // At the start of a block, insert outside it so paragraph spacing stays intact.
      values[0].pos = pos;
      values[0].kind = 'block';
      values[0].force = node.attrs.pageBreakBefore === true;
      values[values.length - 1].keep = node.attrs.keepWithNext === true;
      lines.push(...values);
    } else if (node.isLeaf) lines.push({ pos, ...box(dom), kind: 'block' });
    else node.forEach((child, offset) => visit(child, pos + 1 + offset));
  }
  view.state.doc.forEach(visit);
  return lines;
}

function gapWidget(gap: PageGap, view: EditorView) {
  const el = document.createElement(gap.kind === 'row' ? 'tr' : 'span');
  el.className = 'pagination-gap';
  el.setAttribute('aria-hidden', 'true');
  el.setAttribute('contenteditable', 'false');
  if (gap.kind === 'row') {
    const cell = document.createElement('td');
    const table = view.state.doc.resolve(gap.pos).parent;
    let columns = 0;
    table.firstChild?.forEach((cell) => {
      columns += Number(cell.attrs.colspan || 1);
    });
    cell.colSpan = Math.max(1, columns);
    cell.style.cssText = `height:${gap.height}px;padding:0;border:0;line-height:0;`;
    el.append(cell);
  } else
    el.style.cssText = `display:block;height:${gap.height}px;margin:0;padding:0;border:0;line-height:0;pointer-events:none;user-select:none;`;
  return el;
}

export const LivePagination = Extension.create<{
  page: () => PageSetup;
  enabled: () => boolean;
  onLayout: (layout: PageLayout) => void;
}>({
  name: 'livePagination',
  addProseMirrorPlugins() {
    const options = this.options;
    return [
      new Plugin<DecorationSet>({
        key: paginationKey,
        state: {
          init: () => DecorationSet.empty,
          apply: (tr, value) => tr.getMeta(paginationKey) ?? value.map(tr.mapping, tr.doc)
        },
        props: { decorations: (state) => paginationKey.getState(state) },
        view(view) {
          let frame = 0,
            destroyed = false,
            layingOut = false;
          let pages = 1,
            overflow = false,
            scrollToCursor = false;
          const report = () => {
            const height = pageDimensions(options.page())[1] * MM_TO_PX + PAGE_GAP;
            const scale = view.dom.getBoundingClientRect().width / view.dom.offsetWidth || 1;
            const top =
              view.coordsAtPos(view.state.selection.head).top -
              view.dom.getBoundingClientRect().top;
            options.onLayout({
              pages,
              overflow,
              current: Math.max(1, Math.min(pages, Math.floor(top / scale / height) + 1))
            });
          };
          const layout = () => {
            frame = 0;
            if (destroyed || view.composing || !view.dom.isConnected) return;
            layingOut = true;
            const scroller = view.dom.closest('.paper-area');
            const scroll = scroller?.scrollTop ?? 0;
            try {
              view.dispatch(
                view.state.tr
                  .setMeta(paginationKey, DecorationSet.empty)
                  .setMeta('addToHistory', false)
              );
              const page = options.page();
              const plan = options.enabled()
                ? planPages(
                    measure(view),
                    pageDimensions(page)[1] * MM_TO_PX,
                    page.margin * MM_TO_PX
                  )
                : { gaps: [], pages: 1, overflow: false };
              const decorations = plan.gaps.map((gap) =>
                Decoration.widget(gap.pos, () => gapWidget(gap, view), {
                  side: -1,
                  ignoreSelection: true,
                  key: `${gap.pos}:${gap.height}`
                })
              );
              view.dispatch(
                view.state.tr
                  .setMeta(paginationKey, DecorationSet.create(view.state.doc, decorations))
                  .setMeta('addToHistory', false)
              );
              pages = plan.pages;
              overflow = plan.overflow;
              report();
              if (scroller) scroller.scrollTop = scroll;
              if (scrollToCursor && view.hasFocus())
                view.dispatch(view.state.tr.scrollIntoView().setMeta('addToHistory', false));
              scrollToCursor = false;
            } finally {
              layingOut = false;
            }
          };
          const schedule = () => {
            if (!frame && !destroyed) frame = requestAnimationFrame(layout);
          };
          const observer = new ResizeObserver(schedule);
          observer.observe(view.dom);
          view.dom.addEventListener('load', schedule, true);
          view.dom.addEventListener('compositionend', schedule);
          view.dom.addEventListener('dynodoc-layout', schedule);
          document.fonts.addEventListener('loadingdone', schedule);
          void document.fonts.ready.then(schedule);
          schedule();
          return {
            update(next, previous) {
              view = next;
              if (layingOut) return;
              if (next.state.doc !== previous.doc) {
                scrollToCursor = next.hasFocus();
                schedule();
              } else if (!next.state.selection.eq(previous.selection)) report();
            },
            destroy() {
              destroyed = true;
              cancelAnimationFrame(frame);
              observer.disconnect();
              view.dom.removeEventListener('load', schedule, true);
              view.dom.removeEventListener('compositionend', schedule);
              view.dom.removeEventListener('dynodoc-layout', schedule);
              document.fonts.removeEventListener('loadingdone', schedule);
            }
          };
        }
      })
    ];
  }
});
