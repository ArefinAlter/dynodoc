<script lang="ts">
  import { flushSync, onMount, tick, untrack } from 'svelte';
  import {
    Plus,
    Copy,
    Trash2,
    ArrowUp,
    ArrowDown,
    EyeOff,
    Eye,
    X,
    ChevronLeft,
    ChevronRight,
    StickyNote,
    Presentation
  } from '@lucide/svelte';
  import type { DocumentState, EventPayload } from '$lib/api/types';
  import SlideRibbon from './SlideRibbon.svelte';
  import SlideView from './SlideView.svelte';
  import SlideShape from './SlideShape.svelte';
  import {
    MAX_IMAGE_BYTES,
    addShapeOps,
    applyLayoutOps,
    arrangeShapeOps,
    deleteSlideOps,
    duplicateSlideOps,
    editShapeOps,
    editSlideOps,
    moveSlideOps,
    newSlideOps,
    readDeck,
    round,
    safeColor,
    slideSizes,
    themeName,
    themes,
    titleOf,
    type ShapeFields,
    type ShapeStyle,
    type SlideLayout,
    type SlideShape as Shape
  } from '$lib/workspace/presentation';
  type Box = Pick<ShapeFields, 'x' | 'y' | 'w' | 'h'>;
  let {
    documentState,
    editable,
    onChange,
    onSelect,
    onComment,
    onUndo,
    onRedo
  }: {
    documentState: DocumentState;
    editable: boolean;
    onChange: (ops: EventPayload[]) => void;
    onSelect?: (id: string | null) => void;
    onComment?: () => void;
    onUndo?: () => void;
    onRedo?: () => void;
  } = $props();

  const deck = $derived(readDeck(documentState));
  const ratio = $derived(slideSizes[deck.size].ratio);
  let tab = $state('Home'),
    activeId = $state(''),
    selectedId = $state(''),
    editingId = $state(''),
    draft = $state(''),
    notice = $state(''),
    notesOpen = $state(true),
    notesDraft = $state(''),
    notesSlide = '',
    notesFocused = false;
  let canvas = $state<HTMLDivElement>(),
    rail = $state<HTMLOListElement>(),
    showEl = $state<HTMLDivElement>(),
    imageInput = $state<HTMLInputElement>(),
    textArea = $state<HTMLTextAreaElement>();
  const activeIndex = $derived(
    Math.max(
      0,
      deck.slides.findIndex((s) => s.id === activeId)
    )
  );
  const active = $derived(deck.slides[activeIndex]);
  const activeSlideId = $derived(active?.id ?? null);
  const selected = $derived(active?.shapes.find((s) => s.id === selectedId));
  // Comments, history and suggestions follow the slide being worked on.
  $effect(() => {
    const id = activeSlideId;
    untrack(() => onSelect?.(id));
  });
  $effect(() => {
    const notes = active?.notes ?? '',
      id = activeSlideId ?? '';
    untrack(() => {
      if (!notesFocused) {
        notesDraft = notes;
        notesSlide = id;
      }
    });
  });

  function describeShape(shape: Shape) {
    const kind = shape.role
      ? `${shape.role[0].toUpperCase()}${shape.role.slice(1)} placeholder`
      : shape.kind === 'image'
        ? 'Picture'
        : shape.kind === 'shape'
          ? 'Rectangle'
          : 'Text box';
    const content =
      shape.kind === 'image' ? shape.alt || 'no description' : shape.text.slice(0, 80) || 'empty';
    return `${kind}: ${content}`;
  }
  function slideOf(shapeId: string) {
    return deck.slides.find((s) => s.shapes.some((shape) => shape.id === shapeId));
  }
  function change(ops: EventPayload[]) {
    if (editable && ops.length) onChange(ops);
  }
  /** Keyboard focus returns to the slide list, which also carries the deck shortcuts. */
  async function focusSlide() {
    await tick();
    rail
      ?.querySelector<HTMLButtonElement>(`[data-slide="${active?.id}"]`)
      ?.focus({ preventScroll: true });
  }
  async function activate(id: string, focusRail = false) {
    if (editingId) commitEdit();
    activeId = id;
    selectedId = '';
    if (focusRail) {
      await tick();
      rail?.querySelector<HTMLButtonElement>(`[data-slide="${id}"]`)?.focus();
    }
  }

  // --- slides -------------------------------------------------------------------
  function addSlide(layout: SlideLayout = 'title-content') {
    if (!editable) return;
    const { ops, id } = newSlideOps(deck, layout, active?.id);
    change(ops);
    void activate(id);
  }
  function duplicateSlide() {
    if (!active) return;
    const { ops, id } = duplicateSlideOps(deck, active.id);
    change(ops);
    void activate(id);
  }
  function deleteSlide(focusRail = false) {
    if (!active || !editable) return;
    const next = deck.slides[activeIndex + 1] || deck.slides[activeIndex - 1];
    change(deleteSlideOps(documentState, active.id));
    notice = 'Slide deleted. Use Undo or version history to bring it back.';
    void activate(next?.id || '', focusRail);
  }
  async function moveSlide(delta: number) {
    if (!active) return;
    change(moveSlideOps(deck, active.id, delta));
    await tick();
    rail?.querySelector<HTMLButtonElement>(`[data-slide="${active.id}"]`)?.focus();
  }
  function toggleHidden() {
    if (active) change(editSlideOps(active, { hidden: !active.hidden }));
  }
  function railKey(e: KeyboardEvent, index: number) {
    const slide = deck.slides[index];
    if ((e.altKey || (e.ctrlKey && e.shiftKey)) && (e.key === 'ArrowUp' || e.key === 'ArrowDown')) {
      if (editable) void moveSlide(e.key === 'ArrowUp' ? -1 : 1);
    } else if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      const target = deck.slides[index + (e.key === 'ArrowUp' ? -1 : 1)];
      if (target) void activate(target.id, true);
    } else if (e.key === 'Home' || e.key === 'End') {
      const target = e.key === 'Home' ? deck.slides[0] : deck.slides.at(-1);
      if (target) void activate(target.id, true);
    } else if ((e.key === 'Delete' || e.key === 'Backspace') && editable) {
      activeId = slide.id;
      deleteSlide(true);
    } else return commonKey(e);
    e.preventDefault();
  }

  // --- shapes ---------------------------------------------------------------------
  function selectShape(id: string) {
    if (editingId && editingId !== id) commitEdit();
    selectedId = id;
  }
  function removeShape() {
    if (!selected || !editable) return;
    change([{ type: 'NodeDeleted', node_id: selected.id }]);
    selectedId = '';
    void focusSlide();
  }
  function insertShape(fields: ShapeFields, edit = false) {
    if (!active || !editable) return;
    const { id, ops } = addShapeOps(active, fields);
    change(ops);
    selectedId = id;
    if (edit) void tick().then(() => startEdit(id));
  }
  function applyStyle(patch: Partial<ShapeStyle>) {
    if (!selected || !active || selected.kind === 'image') return;
    if (editingId === selected.id) commitEdit(false);
    const style: ShapeStyle = { ...selected.style, ...patch };
    for (const key of Object.keys(style) as (keyof ShapeStyle)[])
      if (style[key] === undefined) delete style[key];
    change(editShapeOps(active, selected, { style }));
  }
  async function chooseImage(event: Event) {
    const input = event.target as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file || !active) return;
    if (!/^image\/(png|jpeg|gif|webp)$/.test(file.type)) {
      notice = 'Choose a PNG, JPEG, GIF or WebP picture.';
      return;
    }
    if (file.size > MAX_IMAGE_BYTES) {
      notice = 'Pictures can be up to 1 MB each. Compress or resize the picture and try again.';
      return;
    }
    const src = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result));
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(file);
    });
    const aspect = await new Promise<number>((resolve) => {
      const img = new Image();
      img.onload = () => resolve(img.naturalWidth / img.naturalHeight || 1);
      img.onerror = () => resolve(1);
      img.src = src;
    });
    // Percent boxes are relative to slide width and height respectively.
    let w = 50,
      h = (w * ratio) / aspect;
    if (h > 70) {
      h = 70;
      w = (h * aspect) / ratio;
    }
    insertShape({
      kind: 'image',
      text: '',
      x: round((100 - w) / 2),
      y: round((100 - h) / 2),
      w: round(w),
      h: round(h),
      style: {},
      src,
      alt: file.name.replace(/\.[^.]+$/, '').slice(0, 120)
    });
    notice = 'Picture added. Add a description with Review → Alt text.';
  }

  // --- in-place text editing -------------------------------------------------------
  let editTimer: ReturnType<typeof setTimeout> | undefined;
  async function startEdit(id: string) {
    const shape = active?.shapes.find((s) => s.id === id);
    if (!editable || !shape || shape.kind === 'image') return;
    selectedId = id;
    editingId = id;
    draft = shape.text;
    await tick();
    textArea?.focus();
    textArea?.setSelectionRange(draft.length, draft.length);
  }
  function commitEdit(exit = true) {
    clearTimeout(editTimer);
    const id = editingId;
    if (!id) return;
    if (exit) editingId = '';
    const slide = slideOf(id);
    const shape = slide?.shapes.find((s) => s.id === id);
    if (!slide || !shape) return;
    if (exit && shape.kind === 'text' && !shape.role && !draft.trim()) {
      // An empty free text box disappears when you leave it, as in PowerPoint.
      change([{ type: 'NodeDeleted', node_id: id }]);
      if (selectedId === id) selectedId = '';
      return;
    }
    change(editShapeOps(slide, shape, { text: draft }));
  }
  function editKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key === 'Escape') {
      e.preventDefault();
      const id = editingId;
      commitEdit();
      void tick().then(() => canvas?.querySelector<HTMLElement>(`[data-shape="${id}"]`)?.focus());
    }
  }

  // --- move, resize and nudge (one edit per field when the gesture ends) -------------
  let drag: {
    id: string;
    mode: 'move' | 'resize';
    x: number;
    y: number;
    origin: Box;
    width: number;
    height: number;
    moved: boolean;
  } | null = null;
  let preview = $state<{ id: string; box: Box } | null>(null);
  let nudgeTimer: ReturnType<typeof setTimeout> | undefined;
  function startDrag(e: PointerEvent, shape: Shape, mode: 'move' | 'resize') {
    if (e.button !== 0 || editingId === shape.id) return;
    e.stopPropagation();
    selectShape(shape.id);
    (e.currentTarget as HTMLElement).focus({ preventScroll: true });
    if (!editable || !canvas) return;
    e.preventDefault();
    const rect = canvas.getBoundingClientRect();
    commitNudge();
    drag = {
      id: shape.id,
      mode,
      x: e.clientX,
      y: e.clientY,
      origin: { x: shape.x, y: shape.y, w: shape.w, h: shape.h },
      width: rect.width,
      height: rect.height,
      moved: false
    };
    (e.currentTarget as HTMLElement).setPointerCapture?.(e.pointerId);
  }
  function moveDrag(e: PointerEvent) {
    if (!drag) return;
    const px = e.clientX - drag.x,
      py = e.clientY - drag.y;
    if (!drag.moved && Math.hypot(px, py) < 3) return;
    drag.moved = true;
    const dx = (px / drag.width) * 100,
      dy = (py / drag.height) * 100,
      o = drag.origin;
    const box =
      drag.mode === 'move'
        ? {
            ...o,
            x: Math.min(99 - 1, Math.max(1 - o.w, o.x + dx)),
            y: Math.min(99 - 1, Math.max(1 - o.h, o.y + dy))
          }
        : { ...o, w: Math.max(2, o.w + dx), h: Math.max(2, o.h + dy) };
    preview = { id: drag.id, box };
  }
  function endDrag() {
    const shape = drag && active?.shapes.find((s) => s.id === drag!.id);
    if (drag?.moved && preview && shape && active) change(editShapeOps(active, shape, preview.box));
    drag = null;
    preview = null;
  }
  function nudge(shape: Shape, dx: number, dy: number) {
    const base = preview?.id === shape.id ? preview.box : shape;
    preview = { id: shape.id, box: { x: base.x + dx, y: base.y + dy, w: base.w, h: base.h } };
    clearTimeout(nudgeTimer);
    nudgeTimer = setTimeout(commitNudge, 500);
  }
  function commitNudge() {
    clearTimeout(nudgeTimer);
    if (drag || !preview) return;
    const slide = slideOf(preview.id);
    const shape = slide?.shapes.find((s) => s.id === preview!.id);
    if (slide && shape) change(editShapeOps(slide, shape, preview.box));
    preview = null;
  }
  function shapeKey(e: KeyboardEvent, shape: Shape) {
    const step = e.shiftKey ? 5 : 1;
    const arrows: Record<string, [number, number]> = {
      ArrowLeft: [-step, 0],
      ArrowRight: [step, 0],
      ArrowUp: [0, -step],
      ArrowDown: [0, step]
    };
    if (arrows[e.key] && editable) nudge(shape, ...arrows[e.key]);
    else if ((e.key === 'Enter' || e.key === 'F2') && editable) void startEdit(shape.id);
    else if ((e.key === 'Delete' || e.key === 'Backspace') && editable) removeShape();
    else if (e.key === 'Escape') {
      commitNudge();
      selectedId = '';
      void focusSlide();
    } else return commonKey(e);
    e.preventDefault();
    e.stopPropagation();
  }
  function commonKey(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 'z' && onUndo && editable) {
      if (e.shiftKey) onRedo?.();
      else onUndo();
    } else if (mod && e.key.toLowerCase() === 'y' && onRedo && editable) onRedo();
    else if ((e.key === 'PageDown' || e.key === 'PageUp') && active) {
      const target = deck.slides[activeIndex + (e.key === 'PageDown' ? 1 : -1)];
      if (target) void activate(target.id, true);
    } else return;
    e.preventDefault();
  }

  // --- notes, design and review ------------------------------------------------------
  let notesTimer: ReturnType<typeof setTimeout> | undefined;
  function commitNotes() {
    clearTimeout(notesTimer);
    const slide = deck.slides.find((s) => s.id === notesSlide);
    if (slide && notesDraft !== slide.notes)
      change(editSlideOps(slide, { notes: notesDraft.slice(0, 20000) }));
  }
  function root(field: string, value: unknown) {
    if (deck.rootId) change([{ type: 'FieldEdited', node_id: deck.rootId, field, value }]);
  }
  function check() {
    const untitled = deck.slides.filter((s) => !titleOf(s).trim()).length;
    const images = deck.slides.flatMap((s) =>
      s.shapes.filter((x) => x.kind === 'image' && !x.alt?.trim())
    ).length;
    notice =
      untitled || images
        ? `${untitled ? `${untitled} slide${untitled === 1 ? '' : 's'} without a title. ` : ''}${images ? `${images} picture${images === 1 ? '' : 's'} without alt text.` : ''} Titles help screen reader users navigate; alt text describes pictures.`
        : 'No issues found by these basic checks (slide titles and picture alt text).';
  }
  function action(name: string, value?: string) {
    notice = '';
    if (name === 'new-slide') addSlide((value as SlideLayout) || 'title-content');
    else if (name === 'duplicate-slide') duplicateSlide();
    else if (name === 'delete-slide') deleteSlide();
    else if (name === 'layout' && active) change(applyLayoutOps(active, value as SlideLayout));
    else if (name === 'undo') onUndo?.();
    else if (name === 'redo') onRedo?.();
    else if (name === 'text-box')
      insertShape(
        { kind: 'text', text: '', x: 25, y: 40, w: 50, h: 14, style: { size: 20 } },
        true
      );
    else if (name === 'rectangle')
      insertShape({ kind: 'shape', text: '', x: 35, y: 30, w: 30, h: 40, style: {} });
    else if (name === 'image') imageInput?.click();
    else if ((name === 'front' || name === 'back') && active && selected)
      change(arrangeShapeOps(active, selected.id, name));
    else if (name === 'delete-shape') removeShape();
    else if (name === 'theme') root('theme', themeName(value));
    else if (name === 'size') root('slide_size', value === '4:3' ? '4:3' : '16:9');
    else if (name === 'background' && active)
      change(editSlideOps(active, { background: safeColor(value) ?? null }));
    else if (name === 'background-all' && active)
      change(
        deck.slides.flatMap((s) => editSlideOps(s, { background: active.background ?? null }))
      );
    else if (name === 'background-reset' && active)
      change(editSlideOps(active, { background: null }));
    else if (name === 'present') void present(false);
    else if (name === 'present-current') void present(true);
    else if (name === 'hide-slide') toggleHidden();
    else if (name === 'comment' && active) {
      onSelect?.(active.id);
      onComment?.();
    } else if (name === 'alt-text' && active && selected?.kind === 'image') {
      const alt = prompt('Describe this picture for people who cannot see it', selected.alt || '');
      if (alt !== null) change(editShapeOps(active, selected, { alt: alt.trim().slice(0, 500) }));
    } else if (name === 'check') check();
  }

  // --- slide show ------------------------------------------------------------------
  let printing = $state(false),
    showing = $state(false),
    showIndex = $state(0),
    ended = $state(false);
  const shown = $derived(deck.slides.filter((s) => !s.hidden));
  const current = $derived(shown[Math.min(showIndex, shown.length - 1)]);
  async function present(fromCurrent: boolean) {
    if (editingId) commitEdit();
    if (!shown.length) {
      notice = 'Every slide is hidden. Unhide a slide to present.';
      return;
    }
    const start = fromCurrent ? shown.findIndex((s) => deck.slides.indexOf(s) >= activeIndex) : 0;
    showIndex = Math.max(0, start);
    ended = false;
    showing = true;
    await tick();
    showEl?.focus();
    try {
      await showEl?.requestFullscreen?.();
    } catch {
      // The slide show still covers the window when fullscreen is refused.
    }
  }
  function exitShow() {
    showing = false;
    if (document.fullscreenElement) void document.exitFullscreen().catch(() => {});
    void focusSlide();
  }
  function next() {
    if (ended) exitShow();
    else if (showIndex < shown.length - 1) showIndex++;
    else ended = true;
  }
  function previous() {
    if (ended) ended = false;
    else if (showIndex > 0) showIndex--;
  }
  function showKey(e: KeyboardEvent) {
    if (['ArrowRight', 'ArrowDown', 'PageDown', ' ', 'Enter', 'n', 'N'].includes(e.key)) next();
    else if (['ArrowLeft', 'ArrowUp', 'PageUp', 'Backspace', 'p', 'P'].includes(e.key)) previous();
    else if (e.key === 'Home') {
      ended = false;
      showIndex = 0;
    } else if (e.key === 'End') {
      ended = false;
      showIndex = shown.length - 1;
    } else if (e.key === 'Escape') exitShow();
    else return;
    e.preventDefault();
  }
  onMount(() => {
    const fullscreen = () => {
      if (!document.fullscreenElement && showing) showing = false;
    };
    const print = () => {
      if (editingId) commitEdit();
      window.print();
    };
    // Slides for printing are rendered only while the browser prepares a print.
    const before = () => flushSync(() => (printing = true));
    const after = () => (printing = false);
    document.addEventListener('fullscreenchange', fullscreen);
    window.addEventListener('dynodoc-print', print);
    window.addEventListener('beforeprint', before);
    window.addEventListener('afterprint', after);
    return () => {
      document.removeEventListener('fullscreenchange', fullscreen);
      window.removeEventListener('dynodoc-print', print);
      window.removeEventListener('beforeprint', before);
      window.removeEventListener('afterprint', after);
      clearTimeout(editTimer);
      clearTimeout(notesTimer);
      clearTimeout(nudgeTimer);
    };
  });
</script>

<div class="presentation" role="region" aria-label="Presentation editor">
  <SlideRibbon
    bind:tab
    {editable}
    slide={active}
    shape={selected}
    theme={deck.theme}
    size={deck.size}
    canComment={!!onComment}
    canUndo={!!onUndo}
    onAction={action}
    onStyle={applyStyle}
  />
  <input
    class="file"
    type="file"
    accept="image/png,image/jpeg,image/gif,image/webp"
    tabindex="-1"
    aria-hidden="true"
    bind:this={imageInput}
    onchange={chooseImage}
  />
  {#if notice}<div class="notice" role="status">
      {notice}<button aria-label="Dismiss message" onclick={() => (notice = '')}
        ><X size={13} /></button
      >
    </div>{/if}
  <div class="work">
    <nav class="rail" aria-label="Slides">
      {#if editable}<div class="rail-tools">
          <button title="New slide" aria-label="New slide" onclick={() => addSlide()}
            ><Plus size={15} /></button
          ><button
            title="Duplicate slide"
            aria-label="Duplicate slide"
            disabled={!active}
            onclick={duplicateSlide}><Copy size={15} /></button
          ><button
            title="Move slide up (Alt+Up)"
            aria-label="Move slide up"
            disabled={!active || activeIndex === 0}
            onclick={() => void moveSlide(-1)}><ArrowUp size={15} /></button
          ><button
            title="Move slide down (Alt+Down)"
            aria-label="Move slide down"
            disabled={!active || activeIndex >= deck.slides.length - 1}
            onclick={() => void moveSlide(1)}><ArrowDown size={15} /></button
          ><button
            title={active?.hidden ? 'Unhide slide' : 'Hide slide'}
            aria-label={active?.hidden ? 'Unhide slide' : 'Hide slide'}
            aria-pressed={!!active?.hidden}
            disabled={!active}
            onclick={toggleHidden}
            >{#if active?.hidden}<Eye size={15} />{:else}<EyeOff size={15} />{/if}</button
          ><button
            title="Delete slide"
            aria-label="Delete slide"
            disabled={!active}
            onclick={() => deleteSlide()}><Trash2 size={15} /></button
          >
        </div>{/if}
      <ol bind:this={rail}>
        {#each deck.slides as slide, index (slide.id)}<li>
            <span class="number" aria-hidden="true">{index + 1}</span>
            <button
              class="thumb"
              class:active={slide.id === active?.id}
              class:hidden-slide={slide.hidden}
              data-slide={slide.id}
              aria-current={slide.id === active?.id ? 'true' : undefined}
              aria-label={`Slide ${index + 1}: ${titleOf(slide) || 'Untitled'}${slide.hidden ? ' (hidden)' : ''}`}
              tabindex={slide.id === active?.id ? 0 : -1}
              onclick={() => void activate(slide.id)}
              onkeydown={(e) => railKey(e, index)}
              ><SlideView {slide} theme={deck.theme} {ratio} />{#if slide.hidden}<span
                  class="hidden-mark"
                  title="Hidden in slide show"><EyeOff size={12} /></span
                >{/if}</button
            >
          </li>{/each}
      </ol>
    </nav>
    <div class="center">
      <div class="stage">
        {#if active}<div
            class="canvas"
            bind:this={canvas}
            role="group"
            aria-roledescription="slide"
            aria-label={`Slide ${activeIndex + 1}. Tab to its objects: Enter edits text, arrow keys move, Delete removes, Escape returns to the slide list.`}
            style:--ratio={ratio}
            style:aspect-ratio={ratio}
            style:background-color={active.background || themes[deck.theme].background}
            onpointerdown={(e) => {
              if (e.target !== e.currentTarget) return;
              if (editingId) commitEdit();
              commitNudge();
              selectedId = '';
              void focusSlide();
            }}
            onpointermove={moveDrag}
            onpointerup={endDrag}
            onpointercancel={endDrag}
          >
            {#each active.shapes as shape (shape.id)}<SlideShape
                {shape}
                theme={deck.theme}
                box={preview?.id === shape.id ? preview.box : undefined}
                prompt={editable}
                hideText={editingId === shape.id}
                selected={selectedId === shape.id}
                interactive={{
                  id: shape.id,
                  label: describeShape(shape),
                  onfocus: () => selectShape(shape.id),
                  onpointerdown: (e) => startDrag(e, shape, 'move'),
                  ondblclick: () => void startEdit(shape.id),
                  onkeydown: (e) => shapeKey(e, shape)
                }}
                >{#if editingId === shape.id}<textarea
                    class="editor"
                    aria-label="Edit text"
                    bind:this={textArea}
                    bind:value={draft}
                    onpointerdown={(e) => e.stopPropagation()}
                    ondblclick={(e) => e.stopPropagation()}
                    oninput={() => {
                      clearTimeout(editTimer);
                      editTimer = setTimeout(() => commitEdit(false), 1000);
                    }}
                    onblur={() => commitEdit()}
                    onkeydown={editKey}
                  ></textarea>{:else if editable && selectedId === shape.id}<span
                    class="handle"
                    role="presentation"
                    title="Drag to resize"
                    onpointerdown={(e) => startDrag(e, shape, 'resize')}
                  ></span>{/if}</SlideShape
              >{/each}
          </div>
        {:else}<div class="empty">
            <Presentation size={34} strokeWidth={1.2} />
            <p>This presentation has no slides.</p>
            {#if editable}<button onclick={() => addSlide('title')}>Add a slide</button>{/if}
          </div>{/if}
      </div>
      {#if notesOpen && active}<label class="notes"
          ><span>Speaker notes</span><textarea
            placeholder={editable ? 'Click to add notes' : 'No notes'}
            readonly={!editable}
            bind:value={notesDraft}
            onfocus={() => {
              notesFocused = true;
              notesSlide = active?.id || '';
            }}
            oninput={() => {
              clearTimeout(notesTimer);
              notesTimer = setTimeout(commitNotes, 1000);
            }}
            onblur={() => {
              notesFocused = false;
              commitNotes();
            }}
          ></textarea></label
        >{/if}
    </div>
  </div>
  <div class="status">
    <span
      >{deck.slides.length
        ? `Slide ${activeIndex + 1} of ${deck.slides.length}`
        : 'No slides'}{#if deck.slides.some((s) => s.hidden)}
        · {deck.slides.filter((s) => s.hidden).length} hidden{/if}</span
    >
    <button aria-pressed={notesOpen} onclick={() => (notesOpen = !notesOpen)}
      ><StickyNote size={13} /> Notes</button
    >
  </div>
  {#if printing}<div class="print-deck" aria-hidden="true">
      {#each shown as slide (slide.id)}<div class="page">
          <SlideView {slide} theme={deck.theme} {ratio} />
        </div>{/each}
    </div>{/if}
</div>
{#if showing}<div
    class="show"
    bind:this={showEl}
    role="dialog"
    aria-modal="true"
    aria-label="Slide show. Arrow keys or space move between slides; Escape ends."
    tabindex="-1"
    style:--ratio={ratio}
    onkeydown={showKey}
  >
    {#if ended || !current}<button class="end" onclick={exitShow}
        >End of slide show. Click or press Escape to exit.</button
      >{:else}<button class="frame" aria-label="Next slide" onclick={next}
        ><SlideView slide={current} theme={deck.theme} {ratio} /></button
      >{/if}
    <div class="show-controls">
      <button aria-label="Previous slide" onclick={previous}><ChevronLeft size={18} /></button>
      <span aria-live="polite">{Math.min(showIndex + 1, shown.length)} / {shown.length}</span>
      <button aria-label="Next slide" onclick={next}><ChevronRight size={18} /></button>
      <button aria-label="End slide show" onclick={exitShow}><X size={18} /></button>
    </div>
  </div>{/if}

<style>
  .presentation {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: #f3f3f3;
    font:
      13px 'Segoe UI',
      Arial,
      sans-serif;
    color: #242424;
  }
  .file {
    display: none;
  }
  .notice {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 0 8px 5px;
    padding: 6px 10px;
    background: #fbe9e4;
    border: 1px solid #f0c4b7;
    border-radius: 5px;
  }
  .notice button {
    margin-left: auto;
    border: 0;
    background: none;
    cursor: pointer;
    display: grid;
  }
  .work {
    flex: 1;
    min-height: 0;
    display: flex;
  }
  .rail {
    width: 196px;
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    border-right: 1px solid #ddd;
    background: #fafafa;
  }
  .rail-tools {
    display: flex;
    justify-content: space-between;
    padding: 5px 8px;
    border-bottom: 1px solid #e3e3e3;
  }
  .rail-tools button,
  .status button {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    border: 1px solid transparent;
    background: none;
    border-radius: 3px;
    padding: 4px;
    color: #333;
    cursor: pointer;
    font: inherit;
  }
  .rail-tools button:hover,
  .status button:hover,
  .status button[aria-pressed='true'] {
    background: #e7e7e7;
  }
  .rail-tools button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  ol {
    list-style: none;
    margin: 0;
    padding: 10px 10px 20px 4px;
    overflow-y: auto;
    flex: 1;
  }
  li {
    display: flex;
    gap: 5px;
    align-items: flex-start;
    margin-bottom: 12px;
  }
  .number {
    width: 18px;
    text-align: right;
    font-size: 12px;
    color: #555;
    padding-top: 2px;
  }
  .thumb {
    position: relative;
    flex: 1;
    min-width: 0;
    padding: 0;
    border: 2px solid transparent;
    border-radius: 4px;
    background: none;
    cursor: pointer;
    box-shadow: 0 0 0 1px #d0d0d0;
  }
  .thumb :global(.slide) {
    pointer-events: none;
    border-radius: 2px;
  }
  .thumb.active {
    border-color: #c43e1c;
    box-shadow: none;
  }
  .thumb.hidden-slide :global(.slide) {
    opacity: 0.45;
  }
  .hidden-mark {
    position: absolute;
    right: 4px;
    bottom: 4px;
    display: grid;
    padding: 2px;
    background: #fffc;
    border-radius: 3px;
  }
  .center {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  .stage {
    flex: 1;
    min-height: 0;
    container-type: size;
    display: grid;
    place-items: center;
    overflow: hidden;
  }
  .canvas {
    position: relative;
    width: min(calc(100cqw - 48px), calc((100cqh - 40px) * var(--ratio)));
    container-type: size;
    box-shadow:
      0 1px 3px #0003,
      0 0 0 1px #0000000d;
    overflow: visible;
    touch-action: none;
    user-select: none;
  }
  .thumb:focus-visible,
  .rail-tools button:focus-visible,
  .status button:focus-visible,
  .notes textarea:focus-visible {
    outline: 2px solid #185abd;
    outline-offset: 2px;
  }
  .canvas :global(.shape[role='button']) {
    cursor: move;
  }
  .editor {
    position: absolute;
    inset: 0;
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    margin: 0;
    padding: inherit;
    border: 0;
    outline: 2px solid #c43e1c;
    resize: none;
    background: #ffffffe6;
    color: #1f1f1f;
    font: inherit;
    line-height: inherit;
    text-align: inherit;
    cursor: text;
    user-select: text;
  }
  .handle {
    position: absolute;
    right: -6px;
    bottom: -6px;
    width: 11px;
    height: 11px;
    background: white;
    border: 1.5px solid #c43e1c;
    border-radius: 2px;
    cursor: nwse-resize;
  }
  .empty {
    display: grid;
    justify-items: center;
    gap: 10px;
    color: #555;
  }
  .empty button {
    border: 1px solid #c43e1c;
    background: white;
    color: #c43e1c;
    border-radius: 4px;
    padding: 7px 14px;
    cursor: pointer;
    font: inherit;
  }
  .notes {
    display: flex;
    flex-direction: column;
    gap: 3px;
    margin: 0 12px 8px;
    font-size: 12px;
    color: #555;
  }
  .notes textarea {
    height: 76px;
    resize: vertical;
    border: 1px solid #d6d6d6;
    border-radius: 4px;
    padding: 7px 9px;
    font:
      13px 'Segoe UI',
      Arial,
      sans-serif;
    color: #242424;
    background: white;
  }
  .status {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 2px 12px;
    font-size: 12px;
    color: #444;
    border-top: 1px solid #e0e0e0;
    background: #f7f7f7;
  }
  .print-deck {
    display: none;
  }
  .show {
    position: fixed;
    inset: 0;
    z-index: 1000;
    display: grid;
    place-items: center;
    background: #000;
    outline: none;
  }
  .frame {
    width: min(100vw, calc(100vh * var(--ratio)));
    padding: 0;
    border: 0;
    background: none;
    cursor: pointer;
  }
  .end {
    border: 0;
    background: none;
    color: #ddd;
    font-size: 18px;
    cursor: pointer;
  }
  .show-controls {
    position: fixed;
    left: 16px;
    bottom: 14px;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    border-radius: 6px;
    background: #222c;
    color: #eee;
    opacity: 0.25;
    transition: opacity 0.2s;
  }
  .show-controls:hover,
  .show-controls:focus-within {
    opacity: 1;
  }
  .show-controls button {
    display: grid;
    border: 0;
    background: none;
    color: inherit;
    cursor: pointer;
    padding: 4px;
  }
  .show button:focus-visible {
    outline: 2px solid #fff;
  }
  @media (max-width: 760px) {
    .work {
      flex-direction: column;
    }
    .rail {
      width: auto;
      border-right: 0;
      border-bottom: 1px solid #ddd;
    }
    ol {
      display: flex;
      gap: 8px;
      overflow-x: auto;
      overflow-y: hidden;
      padding: 8px;
    }
    li {
      flex: 0 0 120px;
      margin: 0;
    }
    .stage {
      min-height: 240px;
    }
  }
  @media print {
    .presentation > :not(.print-deck) {
      display: none;
    }
    .print-deck {
      display: block;
    }
    .page {
      break-after: page;
      break-inside: avoid;
    }
    /* One landscape page per visible slide. */
    @page {
      size: landscape;
      margin: 10mm;
    }
  }
</style>
