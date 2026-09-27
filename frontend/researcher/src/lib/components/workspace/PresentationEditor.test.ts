import { afterEach, describe, expect, it, vi } from 'vitest';
import { flushSync, mount, tick, unmount } from 'svelte';
import { emptyState, applyPayload } from '$lib/materializer';
import type { EventPayload } from '$lib/api/types';
import { initialPresentationOps, newSlideOps, readDeck } from '$lib/workspace/presentation';
import PresentationEditor from './PresentationEditor.svelte';

function deckState() {
  const state = emptyState();
  for (const op of initialPresentationOps('Team pitch', 'root')) applyPayload(state, op);
  for (const op of newSlideOps(readDeck(state), 'title-content', undefined, 'Plan').ops)
    applyPayload(state, op);
  return state;
}
let target: HTMLElement;
let component: Record<string, unknown> | null = null;
afterEach(() => {
  if (component) unmount(component);
  component = null;
  target?.remove();
  vi.useRealTimers();
});
function render(editable = true) {
  const state = deckState();
  const onChange = vi.fn<(ops: EventPayload[]) => void>();
  const onSelect = vi.fn();
  target = document.createElement('div');
  document.body.appendChild(target);
  component = mount(PresentationEditor as never, {
    target,
    props: { documentState: state, editable, onChange, onSelect, onComment: vi.fn() }
  }) as Record<string, unknown>;
  flushSync();
  return { state, onChange, onSelect, deck: readDeck(state) };
}
const shapeEl = (id: string) => target.querySelector<HTMLElement>(`[data-shape="${id}"]`)!;

describe('PresentationEditor', () => {
  it('renders the slide rail and targets the active slide for comments', () => {
    const { onSelect, deck } = render();
    expect(target.querySelectorAll('.thumb')).toHaveLength(2);
    expect(onSelect).toHaveBeenCalledWith(deck.slides[0].id);
    // Rendered lines carry no template whitespace (text uses pre-wrap).
    expect(shapeEl(deck.slides[0].shapes[0].id).querySelector('p')?.textContent).toBe('Team pitch');
    target.querySelectorAll<HTMLButtonElement>('.thumb')[1].click();
    flushSync();
    expect(onSelect).toHaveBeenLastCalledWith(deck.slides[1].id);
  });

  it('adds slides and deletes the selected object through engine operations', () => {
    const { onChange, deck } = render();
    target.querySelector<HTMLButtonElement>('button[aria-label="New slide"]')!.click();
    const created = onChange.mock.calls[0][0];
    expect(created[0]).toMatchObject({ type: 'NodeCreated', node_type: 'section' });
    expect(
      created.slice(1).every((op) => op.type === 'NodeCreated' && op.node_type === 'item')
    ).toBe(true);
    const subtitle = deck.slides[0].shapes[1];
    shapeEl(subtitle.id).focus();
    shapeEl(subtitle.id).dispatchEvent(
      new KeyboardEvent('keydown', { key: 'Delete', bubbles: true })
    );
    expect(onChange.mock.calls.at(-1)![0]).toEqual([{ type: 'NodeDeleted', node_id: subtitle.id }]);
  });

  it('coalesces keyboard nudges into one edit per field', async () => {
    vi.useFakeTimers();
    const { onChange, deck } = render();
    const title = deck.slides[0].shapes[0];
    const el = shapeEl(title.id);
    el.focus();
    for (let i = 0; i < 3; i++)
      el.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
    expect(onChange).not.toHaveBeenCalled();
    vi.advanceTimersByTime(600);
    expect(onChange).toHaveBeenCalledTimes(1);
    expect(onChange.mock.calls[0][0]).toEqual([
      { type: 'FieldEdited', node_id: title.id, field: 'x', value: title.x + 3 }
    ]);
  });

  it('edits text in place and saves it when leaving the box', async () => {
    const { onChange, deck } = render();
    const title = deck.slides[0].shapes[0];
    shapeEl(title.id).dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    await tick();
    const editor = target.querySelector<HTMLTextAreaElement>('textarea.editor')!;
    expect(editor.value).toBe('Team pitch');
    editor.value = 'Our pitch';
    editor.dispatchEvent(new Event('input', { bubbles: true }));
    expect(onChange).not.toHaveBeenCalled();
    editor.dispatchEvent(new FocusEvent('blur'));
    expect(onChange.mock.calls[0][0]).toEqual([
      { type: 'FieldEdited', node_id: title.id, field: 'text', value: 'Our pitch' },
      { type: 'FieldEdited', node_id: title.id, field: 'label', value: 'Our pitch' },
      { type: 'FieldEdited', node_id: deck.slides[0].id, field: 'label', value: 'Our pitch' }
    ]);
  });

  it('keeps view-only decks read-only', () => {
    const { onChange, deck } = render(false);
    expect(target.querySelector('button[aria-label="New slide"]')).toBeNull();
    shapeEl(deck.slides[0].shapes[0].id).dispatchEvent(
      new MouseEvent('dblclick', { bubbles: true })
    );
    flushSync();
    expect(target.querySelector('textarea.editor')).toBeNull();
    expect(onChange).not.toHaveBeenCalled();
  });
});
