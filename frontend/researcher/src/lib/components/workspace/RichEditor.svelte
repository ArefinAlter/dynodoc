<script lang="ts">
  import {
    DocumentNote,
    DocumentContents,
    documentNotes,
    contentsEntries,
    referencedHtml
  } from '$lib/workspace/document-references';
  import OfficeDialog, { type ToolDialog } from './OfficeDialog.svelte';
  let referenceDialog = $state<ToolDialog | null>(null);
  let notesOpen = $state(false);
  const notes = $derived.by(() => {
    void tick;
    return editor ? documentNotes(editor.state.doc) : [];
  });
  function openReference(action: string, pos?: number) {
    if (!editor) return;
    if (action === 'notes') {
      notesOpen = !notesOpen;
      return;
    }
    if (!editable) return;
    if (action === 'contents' || action === 'update-contents') {
      const entries = contentsEntries(editor);
      if (!entries.length) {
        feedback = 'Apply heading styles before inserting a table of contents.';
        return;
      }
      if (action === 'contents')
        editor.chain().focus().insertContent({ type: 'tableOfContents', attrs: { entries } }).run();
      else {
        const tr = editor.state.tr;
        editor.state.doc.descendants((node, p) => {
          if (node.type.name === 'tableOfContents')
            tr.setNodeMarkup(p, undefined, { ...node.attrs, entries });
        });
        editor.view.dispatch(tr);
      }
      return;
    }
    const existing = pos === undefined ? undefined : editor.state.doc.nodeAt(pos);
    referenceDialog = {
      title: existing ? 'Edit note' : `Insert ${action}`,
      fields: [
        {
          key: 'body',
          label: 'Note text',
          type: 'textarea',
          value: String(existing?.attrs.body || ''),
          required: true
        }
      ],
      submit: 'Save',
      onSubmit: (v) => {
        if (!editor) return;
        const attrs = {
          id: existing?.attrs.id || ulid(),
          kind: existing?.attrs.kind || action,
          body: v.body.slice(0, 10000)
        };
        if (pos !== undefined && existing) {
          const current = documentNotes(editor.state.doc).find((n) => n.id === existing.attrs.id);
          if (!current) throw new Error('This note was removed while the dialog was open.');
          editor.view.dispatch(editor.state.tr.setNodeMarkup(current.pos, undefined, attrs));
        } else editor.chain().focus().insertContent({ type: 'documentNote', attrs }).run();
        notesOpen = true;
      }
    };
  }

  import { LivePagination, PAGE_GAP, MM_TO_PX, type PageLayout } from '$lib/workspace/pagination';
  import OfficeRibbon from './OfficeRibbon.svelte';
  import PrintPreview from './PrintPreview.svelte';
  import {
    OfficeParagraph,
    PageBreak,
    SourceComment,
    OfficeFontFamily
  } from '$lib/workspace/office-extensions';
  import '$lib/workspace/fonts.css';
  import { TrackedChange } from '$lib/workspace/tracked-changes';
  import {
    ThemeInk,
    themeInkKey,
    readLightPages,
    writeLightPages
  } from '$lib/workspace/theme-colors';
  import { theme } from '$lib/theme.svelte';
  import {
    pageSetup,
    pageDimensions,
    officeFonts,
    documentFonts,
    safeFont,
    type PageSetup
  } from '$lib/workspace/office';
  import ContextMenu from '$lib/components/product/ContextMenu.svelte';
  import type { MenuAction } from '$lib/workspace/context-menu';
  import Mathematics from '@tiptap/extension-mathematics';
  import Superscript from '@tiptap/extension-superscript';
  import Subscript from '@tiptap/extension-subscript';
  import TaskList from '@tiptap/extension-task-list';
  import TaskItem from '@tiptap/extension-task-item';
  import 'katex/dist/katex.min.css';
  import { stableJson } from '$lib/workspace/equality';
  import { textOf, type TextBlock } from '$lib/workspace/files';
  import { onMount, untrack } from 'svelte';
  import { Editor, type JSONContent } from '@tiptap/core';
  import type { Node as PMNode } from '@tiptap/pm/model';
  import StarterKit from '@tiptap/starter-kit';
  import { TableKit } from '@tiptap/extension-table';
  import { TextStyleKit } from '@tiptap/extension-text-style';
  import TextAlign from '@tiptap/extension-text-align';
  import Highlight from '@tiptap/extension-highlight';
  import Image from '@tiptap/extension-image';
  import UniqueID from '@tiptap/extension-unique-id';
  import { ulid } from 'ulid';
  import { X, Minus, Plus, FileText, PanelTop, Search } from '@lucide/svelte';
  interface Props {
    content: JSONContent;
    editable: boolean;
    onChange: (content: JSONContent, changedIds?: Set<string>) => void;
    onSelect?: (id: string | null) => void;
    revision: number;
    onComment?: () => void;
    pageSettings?: unknown;
    fonts?: unknown;
    documentId?: string;
    onPageChange?: (settings: PageSetup) => void;
    onFont?: (file: File, family: string) => Promise<void>;
    reviewerName?: string;
    onCompositionChange?: (active: boolean) => void;
  }
  let {
    content,
    editable,
    onChange,
    onSelect,
    revision,
    onComment,
    pageSettings,
    fonts,
    documentId,
    onPageChange,
    onFont,
    reviewerName = 'Editor',
    onCompositionChange
  }: Props = $props();
  let ribbon = $state('Home'),
    tracking = $state(false),
    preview = $state(false),
    paginated = $state(true),
    lightPages = $state(readLightPages()),
    layout = $state<PageLayout>({ pages: 1, current: 1, overflow: false });
  $effect(() => writeLightPages(lightPages));
  // Night-mode ink for authored colours follows the mode and the white-pages option.
  $effect(() => {
    void theme.resolved;
    void lightPages;
    // Untracked: the transaction updates editor state this effect must not depend on,
    // or every dispatch would schedule another one.
    untrack(() => {
      if (editor && !editor.isDestroyed)
        editor.view.dispatch(
          editor.state.tr.setMeta(themeInkKey, true).setMeta('addToHistory', false)
        );
    });
  });
  $effect(() => {
    void page;
    void paginated;
    void zoom;
    editor?.view.dom.dispatchEvent(new Event('dynodoc-layout'));
  });
  const page = $derived(pageSetup(pageSettings));
  const dimensions = $derived(pageDimensions(page));
  const fontChoices = $derived([
    ...new Set([...officeFonts, ...documentFonts(fonts).map((f) => f.family)])
  ]);
  let composing = false,
    compositionTimer: ReturnType<typeof setTimeout> | undefined;
  const loadedFonts = new Map<string, FontFace>();
  $effect(() => {
    const list = documentFonts(fonts),
      id = documentId;
    if (!id || typeof window === 'undefined') return;
    for (const font of list) {
      const key = `${id}/${font.uploadId}`;
      if (loadedFonts.has(key)) continue;
      const face = new FontFace(
        safeFont(font.family),
        `url(/api/engine/documents/${id}/uploads/${font.uploadId})`
      );
      loadedFonts.set(key, face);
      void face
        .load()
        .then((f) => document.fonts.add(f))
        .catch(() => {
          feedback = `The font “${font.family}” could not load. A fallback font is being shown.`;
        });
    }
  });
  $effect(() => {
    if (editor) {
      const storage = editor.storage as unknown as {
        trackedChange: { enabled: boolean; author: string };
      };
      storage.trackedChange.enabled = tracking && editable;
      storage.trackedChange.author = reviewerName;
    }
  });
  let host: HTMLDivElement;
  let editor = $state.raw<Editor | null>(null),
    tick = $state(0),
    zoom = $state('100'),
    findOpen = $state(false),
    needle = $state(''),
    replacement = $state(''),
    matches = $state(0),
    linkOpen = $state(false),
    linkUrl = $state(''),
    imageOpen = $state(false),
    imageUrl = $state(''),
    imageAlt = $state('');
  let context = $state<{ x: number; y: number; items: MenuAction[] } | null>(null),
    feedback = $state('');
  let mathDialog: HTMLDialogElement,
    latex = $state(''),
    mathBlock = $state(false),
    mathPos = $state<number | null>(null);
  function openMath(block = false, source = '', pos: number | null = null) {
    if (!editable) return;
    latex = source;
    mathBlock = block;
    mathPos = pos;
    mathDialog.showModal();
  }
  function insertMath() {
    if (!editor || !latex.trim()) return;
    const value = { latex: latex.trim(), ...(mathPos === null ? {} : { pos: mathPos }) };
    const chain = editor.chain().focus();
    if (mathPos === null) {
      if (mathBlock) chain.insertBlockMath(value).run();
      else chain.insertInlineMath(value).run();
    } else {
      if (mathBlock) chain.updateBlockMath(value).run();
      else chain.updateInlineMath(value).run();
    }
    mathDialog.close();
  }
  async function clipboard(action: string) {
    if (!editor) return;
    try {
      if (action === 'paste') {
        const text = await navigator.clipboard.readText();
        editor.chain().focus().insertContent({ type: 'text', text }).run();
      } else {
        await navigator.clipboard.writeText(
          editor.state.doc.textBetween(editor.state.selection.from, editor.state.selection.to, '\n')
        );
        if (action === 'cut' && editable) editor.chain().focus().deleteSelection().run();
      }
    } catch {
      feedback =
        'Your browser blocked clipboard access. Use Ctrl+C, Ctrl+X or Ctrl+V on a keyboard, or press and hold in the document on a touch screen.';
    }
  }
  function contextMenu(event: MouseEvent) {
    if (!editor) return false;
    event.preventDefault();
    const position = editor.view.posAtCoords({ left: event.clientX, top: event.clientY })?.pos;
    const selection = editor.state.selection;
    if (
      position !== undefined &&
      (selection.empty || position < selection.from || position > selection.to)
    )
      editor.commands.setTextSelection(position);
    const point =
      event.clientX || event.clientY
        ? { x: event.clientX, y: event.clientY }
        : {
            x: editor.view.coordsAtPos(editor.state.selection.from).left,
            y: editor.view.coordsAtPos(editor.state.selection.from).bottom
          };
    const e = editor;
    context = {
      ...point,
      items: [
        {
          label: 'Cut',
          action: () => clipboard('cut'),
          disabled: !editable || e.state.selection.empty
        },
        { label: 'Copy', action: () => clipboard('copy'), disabled: e.state.selection.empty },
        { label: 'Paste plain text', action: () => clipboard('paste'), disabled: !editable },
        ...(onComment
          ? [{ label: 'Add comment', action: () => onComment?.(), separator: true }]
          : []),
        {
          label: 'Select all',
          action: () => {
            e.chain().focus().selectAll().run();
          }
        },
        {
          label: 'Bold',
          action: () => {
            e.chain().focus().toggleBold().run();
          },
          disabled: !editable,
          separator: true
        },
        {
          label: 'Italic',
          action: () => {
            e.chain().focus().toggleItalic().run();
          },
          disabled: !editable
        },
        {
          label: 'Insert or edit link',
          action: () => {
            linkOpen = true;
            linkUrl = String(e.getAttributes('link').href || '');
          },
          disabled: !editable
        },
        {
          label: 'Clear formatting',
          action: () => {
            clearFormatting();
          },
          disabled: !editable
        },
        ...(e.isActive('table')
          ? [
              {
                label: 'Insert row below',
                action: () => {
                  e.chain().focus().addRowAfter().run();
                },
                disabled: !editable,
                separator: true
              },
              {
                label: 'Insert column right',
                action: () => {
                  e.chain().focus().addColumnAfter().run();
                },
                disabled: !editable
              },
              {
                label: 'Merge / split cells',
                action: () => {
                  e.chain().focus().mergeOrSplit().run();
                },
                disabled: !editable
              },
              {
                label: 'Delete row',
                action: () => {
                  e.chain().focus().deleteRow().run();
                },
                disabled: !editable
              },
              {
                label: 'Delete column',
                action: () => {
                  e.chain().focus().deleteColumn().run();
                },
                disabled: !editable
              }
            ]
          : [])
      ]
    };
    return true;
  }
  let applyingRemote = false;
  let appliedRevision = -1;
  let serializedBlocks = new WeakMap<PMNode, JSONContent>();
  let previousBlocks = new Set<PMNode>();
  function seedSerialization(value: JSONContent) {
    serializedBlocks = new WeakMap();
    previousBlocks = new Set();
    editor?.state.doc.forEach((node, _pos, i) => {
      previousBlocks.add(node);
      const block = value.content?.[i];
      if (block && block.type === node.type.name) serializedBlocks.set(node, block);
    });
  }
  function notifyContent(e: Editor) {
    const blocks: JSONContent[] = [],
      changed = new Set<string>();
    const current = new Set<PMNode>();
    e.state.doc.forEach((node) => {
      current.add(node);
      if (!previousBlocks.has(node)) changed.add(String(node.attrs.blockId || ''));
      let value = serializedBlocks.get(node);
      if (!value) {
        value = node.toJSON();
        serializedBlocks.set(node, value!);
        changed.add(String(node.attrs.blockId || ''));
      }
      blocks.push(value!);
    });
    previousBlocks = current;
    onChange({ type: 'doc', content: blocks }, changed);
  }
  onMount(() => {
    const openPrint = () => {
      preview = true;
    };
    window.addEventListener('dynodoc-print', openPrint);
    return () => window.removeEventListener('dynodoc-print', openPrint);
  });
  onMount(() => {
    editor = new Editor({
      element: host,
      extensions: [
        StarterKit.configure({
          link: { openOnClick: false, protocols: ['http', 'https', 'mailto'] }
        }),
        TableKit.configure({ table: { resizable: true } }),
        TextStyleKit.configure({ fontFamily: false }),
        OfficeFontFamily,
        OfficeParagraph,
        LivePagination.configure({
          page: () => page,
          enabled: () => paginated,
          onLayout: (value) => {
            if (
              layout.pages !== value.pages ||
              layout.current !== value.current ||
              layout.overflow !== value.overflow
            )
              layout = value;
          }
        }),
        PageBreak,
        DocumentNote,
        DocumentContents,
        SourceComment,
        TrackedChange.configure({ onNotice: (message) => (feedback = message) }),
        Superscript,
        Subscript,
        TaskList,
        TaskItem.configure({ nested: true }),
        Mathematics.configure({
          katexOptions: { trust: false, throwOnError: false, maxExpand: 1000 },
          inlineOptions: { onClick: (node, pos) => openMath(false, String(node.attrs.latex), pos) },
          blockOptions: { onClick: (node, pos) => openMath(true, String(node.attrs.latex), pos) }
        }),
        TextAlign.configure({ types: ['heading', 'paragraph'] }),
        Highlight.configure({ multicolor: true }),
        ThemeInk.configure({ active: () => theme.resolved === 'dark' && !lightPages }),
        Image.configure({ allowBase64: true }),
        UniqueID.configure({
          attributeName: 'blockId',
          types: [
            'paragraph',
            'heading',
            'bulletList',
            'orderedList',
            'blockquote',
            'table',
            'image',
            'horizontalRule',
            'codeBlock',
            'taskList',
            'blockMath',
            'pageBreak',
            'tableOfContents'
          ],
          generateID: () => ulid()
        })
      ],
      content,
      editable,
      editorProps: {
        handleClick: (_view, pos, event) => {
          const target = event.target instanceof Element ? event.target : null;
          const heading = target?.closest('[data-heading-id]')?.getAttribute('data-heading-id');
          if (heading && editor) {
            event.preventDefault();
            editor.state.doc.descendants((node, p) => {
              if (node.attrs.blockId === heading)
                editor
                  ?.chain()
                  .focus()
                  .setTextSelection(p + 1)
                  .scrollIntoView()
                  .run();
            });
            return true;
          }
          if (target?.closest('[data-document-note]')) {
            const note = notes.find((n) => Math.abs(n.pos - pos) <= 1);
            if (note) {
              event.preventDefault();
              notesOpen = true;
              if (editable) openReference(note.kind, note.pos);
              return true;
            }
          }
          return false;
        },
        handleDOMEvents: {
          contextmenu: (_view, event) => contextMenu(event),
          compositionstart: () => {
            composing = true;
            onCompositionChange?.(true);
            return false;
          },
          compositionend: () => {
            clearTimeout(compositionTimer);
            compositionTimer = setTimeout(() => {
              composing = false;
              if (editor && !editor.isDestroyed) notifyContent(editor);
              onCompositionChange?.(false);
            }, 20);
            return false;
          }
        },
        handleKeyDown: (_view, event) => {
          if (event.isComposing || event.keyCode === 229) return false;
          if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'f') {
            findOpen = true;
            return true;
          }
          if ((event.ctrlKey || event.metaKey) && event.altKey && event.key.toLowerCase() === 'm') {
            onComment?.();
            return true;
          }
          return false;
        },
        attributes: {
          'aria-label': 'Document content',
          role: 'textbox',
          'aria-multiline': 'true',
          spellcheck: 'true',
          dir: 'auto'
        }
      },
      onUpdate: ({ editor: e }) => {
        if (!applyingRemote && !composing && !e.view.composing) notifyContent(e);
      },
      onSelectionUpdate: ({ editor: e }) => {
        tick++;
        const position = e.state.selection.$from;
        onSelect?.(position.depth > 0 ? position.node(1).attrs.blockId : null);
      },
      onTransaction: () => tick++
    });
    seedSerialization(content);
    return () => {
      clearTimeout(compositionTimer);
      editor?.destroy();
      for (const face of loadedFonts.values()) document.fonts.delete(face);
    };
  });
  $effect(() => {
    if (editor) editor.setEditable(editable, false);
  });
  $effect(() => {
    if (editor && !composing && !editor.view.composing && revision !== appliedRevision) {
      appliedRevision = revision;
      const next = stableJson(content);
      if (next !== stableJson(editor.getJSON())) {
        applyingRemote = true;
        try {
          editor.commands.setContent(content, { emitUpdate: false });
          seedSerialization(content);
        } finally {
          applyingRemote = false;
        }
      }
    }
  });
  function clearFormatting() {
    if (!editor) return;
    const chain = editor.chain().focus();
    for (const name of Object.keys(editor.schema.marks))
      if (!['trackedChange', 'sourceComment'].includes(name)) chain.unsetMark(name);
    chain.clearNodes().run();
  }
  function active(name: string) {
    void tick;
    return editor?.isActive(name) || false;
  }
  function findNext() {
    if (!editor || !needle) {
      matches = 0;
      return;
    }
    const found: { from: number; to: number }[] = [];
    editor.state.doc.descendants((node, pos) => {
      if (!node.isText || !node.text) return;
      let from = 0;
      const text = node.text.toLowerCase(),
        search = needle.toLowerCase();
      for (;;) {
        const at = text.indexOf(search, from);
        if (at < 0) break;
        found.push({ from: pos + at, to: pos + at + needle.length });
        from = at + needle.length;
      }
    });
    matches = found.length;
    const next = found.find((m) => m.from >= editor!.state.selection.to) || found[0];
    if (next) editor.chain().focus().setTextSelection(next).run();
  }
  function replaceAll() {
    if (!editor || !needle) return;
    const found: { from: number; to: number }[] = [];
    editor.state.doc.descendants((node, pos) => {
      if (!node.isText || !node.text) return;
      let from = 0;
      while (true) {
        const at = node.text.toLowerCase().indexOf(needle.toLowerCase(), from);
        if (at < 0) break;
        found.push({ from: pos + at, to: pos + at + needle.length });
        from = at + needle.length;
      }
    });
    const tr = editor.state.tr;
    for (const item of found.reverse()) {
      if (replacement) tr.insertText(replacement, item.from, item.to);
      else tr.delete(item.from, item.to);
    }
    editor.view.dispatch(tr);
    matches = found.length;
  }
  function setLink() {
    if (!editor) return;
    if (linkUrl && !/^(https?:\/\/|mailto:)/i.test(linkUrl)) return;
    editor.chain().focus().extendMarkRange('link').setLink({ href: linkUrl }).run();
    linkOpen = false;
  }
  const words = $derived(
    textOf(content as TextBlock)
      .trim()
      .split(/\s+/)
      .filter(Boolean).length
  );
</script>

<div class="rich-editor">
  {#if feedback}<p class="feedback" role="status">
      {feedback}<button onclick={() => (feedback = '')}>Dismiss</button>
    </p>{/if}
  <OfficeRibbon
    {editor}
    {editable}
    {tick}
    bind:tab={ribbon}
    bind:tracking
    settings={pageSettings}
    {onPageChange}
    {onComment}
    {onFont}
    fonts={fontChoices}
    bind:paginated
    bind:lightPages
    onClipboard={(action) => void clipboard(action)}
    onFind={() => (findOpen = !findOpen)}
    onLink={() => {
      linkUrl = editor?.getAttributes('link').href || '';
      linkOpen = true;
    }}
    onImage={() => (imageOpen = true)}
    onReference={openReference}
    onMath={() => openMath()}
    onPrint={() => (preview = true)}
    onNotice={(message) => (feedback = message)}
  />
  {#if notesOpen && notes.length}<aside class="reference-notes" aria-label="Document notes">
      {#each notes as note}<button
          onclick={() => {
            editor?.chain().focus().setNodeSelection(note.pos).run();
            if (editable) openReference(note.kind, note.pos);
          }}
          ><strong>{note.kind === 'footnote' ? 'Footnote' : 'Endnote'} {note.number}</strong>
          {note.body}</button
        >{/each}
    </aside>{/if}
  {#if active('table') && editable}<div class="context-tools">
      <span>Table</span><button onclick={() => editor?.chain().focus().addRowAfter().run()}
        >Add row</button
      ><button onclick={() => editor?.chain().focus().addColumnAfter().run()}>Add column</button
      ><button onclick={() => editor?.chain().focus().deleteRow().run()}>Delete row</button><button
        onclick={() => editor?.chain().focus().deleteColumn().run()}>Delete column</button
      ><button onclick={() => editor?.chain().focus().mergeCells().run()}>Merge cells</button
      ><button onclick={() => editor?.chain().focus().splitCell().run()}>Split cell</button><button
        onclick={() => editor?.chain().focus().deleteTable().run()}>Delete table</button
      >
    </div>{/if}
  {#if findOpen}<div class="find-bar">
      <Search size={15} /><input
        aria-label="Find text"
        bind:value={needle}
        placeholder="Find in document"
        onkeydown={(e) => {
          if (e.key === 'Enter') findNext();
        }}
      /><button onclick={findNext}>Find next</button><span
        >{matches ? `${matches} matches` : ''}</span
      >{#if editable}<input
          aria-label="Replace with"
          bind:value={replacement}
          placeholder="Replace with"
        /><button onclick={replaceAll}>Replace all</button>{/if}<button
        aria-label="Close find"
        onclick={() => (findOpen = false)}><X size={15} /></button
      >
    </div>{/if}
  <div class="paper-area">
    <div
      class="ruler"
      aria-hidden="true"
      style={`width:${dimensions[0]}mm;zoom:${Number(zoom) / 100};padding-inline:${page.margin}mm`}
    >
      {#each Array.from({ length: 17 }, (_, i) => i) as i}<span>{i % 2 === 0 ? i : ''}</span>{/each}
    </div>
    <div
      class="paper"
      class:paginated
      class:light-pages={lightPages}
      lang={page.language}
      style={`zoom:${Number(zoom) / 100};width:${dimensions[0]}mm;padding:${page.margin}mm;min-height:${paginated ? layout.pages * (dimensions[1] * MM_TO_PX + PAGE_GAP) - PAGE_GAP : dimensions[1] * MM_TO_PX}px;--page-height:${dimensions[1] * MM_TO_PX}px;--page-gap:${PAGE_GAP}px`}
    >
      {#if paginated}<div class="page-sheets" aria-hidden="true">
          {#each Array.from({ length: layout.pages }, (_, i) => i) as i}<div
              class="page-sheet"
              style={`top:${i * (dimensions[1] * MM_TO_PX + PAGE_GAP)}px;height:${dimensions[1]}mm`}
            >
              <div
                class="page-header"
                style={`inset-inline:${page.margin}mm;top:${page.margin / 2}mm`}
              >
                {page.header}
              </div>
              <div
                class="page-footer"
                style={`inset-inline:${page.margin}mm;bottom:${page.margin / 2}mm`}
              >
                <span>{page.footer}</span>{#if page.pageNumbers}<span>{i + 1}</span>{/if}
              </div>
            </div>{/each}
        </div>{/if}
      <div class="editor-content" bind:this={host}></div>
    </div>
  </div>
  <div class="document-foot" role="status" aria-label="Document status">
    <span>Page {layout.current} of {layout.pages}</span><span>{words} words</span><span
      >{page.language.toUpperCase()}</span
    >
    {#if layout.overflow}<span
        title="An image, equation or joined table row exceeds the page body. Resize it or use continuous view."
        >Oversized content</span
      >{/if}
    <div class="view-controls">
      <button
        aria-label="Print layout"
        title="Print layout"
        aria-pressed={paginated}
        onclick={() => (paginated = true)}><FileText size={15} /></button
      ><button
        aria-label="Continuous view"
        title="Continuous view"
        aria-pressed={!paginated}
        onclick={() => (paginated = false)}><PanelTop size={15} /></button
      >
      <button
        aria-label="Zoom out"
        title="Zoom out"
        onclick={() => (zoom = String(Math.max(50, Number(zoom) - 10)))}><Minus size={13} /></button
      >
      <input aria-label="Zoom" type="range" min="50" max="200" step="10" bind:value={zoom} />
      <button
        aria-label="Zoom in"
        title="Zoom in"
        onclick={() => (zoom = String(Math.min(200, Number(zoom) + 10)))}><Plus size={13} /></button
      ><span>{zoom}%</span>
    </div>
  </div>
  {#if linkOpen || imageOpen}<div
      class="popover"
      role="dialog"
      aria-modal="true"
      aria-label={imageOpen ? 'Insert image' : 'Insert link'}
    >
      <form
        onsubmit={(e) => {
          e.preventDefault();
          if (imageOpen) {
            if (/^https:\/\//i.test(imageUrl)) {
              editor?.chain().focus().setImage({ src: imageUrl, alt: imageAlt }).run();
              imageOpen = false;
            }
          } else setLink();
        }}
      >
        <h3>{imageOpen ? 'Insert image' : 'Insert link'}</h3>
        {#if imageOpen}<label
            >HTTPS image URL<input
              type="url"
              bind:value={imageUrl}
              placeholder="https://…"
              required
            /></label
          ><label
            >Image description<input
              aria-label="Image description"
              bind:value={imageAlt}
              placeholder="Describe the image for readers who cannot see it"
              required
            /></label
          >{:else}<label
            >Link URL<input bind:value={linkUrl} placeholder="https://…" required /></label
          >{/if}
        <div>
          <button
            type="button"
            onclick={() => {
              linkOpen = false;
              imageOpen = false;
            }}>Cancel</button
          ><button class="apply">Insert</button>
        </div>
      </form>
    </div>{/if}
</div>

{#if context}<ContextMenu
    {...context}
    onClose={() => {
      context = null;
    }}
  />{/if}
<dialog bind:this={mathDialog} aria-labelledby="math-title">
  <form
    onsubmit={(e) => {
      e.preventDefault();
      insertMath();
    }}
  >
    <h2 id="math-title">LaTeX equation</h2>
    <label
      >Equation source<textarea
        bind:value={latex}
        required
        maxlength="2000"
        rows="4"
        placeholder="E = mc^2"
      ></textarea></label
    ><label
      ><input type="checkbox" bind:checked={mathBlock} disabled={mathPos !== null} />Display on its
      own line</label
    >
    <p>KaTeX renders the equation. Click an equation in the document to edit its source.</p>
    <button type="button" onclick={() => mathDialog.close()}>Cancel</button><button type="submit"
      >{mathPos === null ? 'Insert equation' : 'Update equation'}</button
    >
  </form>
</dialog>

{#if preview && editor}<PrintPreview
    html={referencedHtml(editor)}
    {fonts}
    {documentId}
    {page}
    onClose={() => (preview = false)}
  />{/if}

{#if referenceDialog}<OfficeDialog
    config={referenceDialog}
    onClose={() => (referenceDialog = null)}
  />{/if}

<style>
  .reference-notes {
    max-height: 150px;
    overflow: auto;
    background: var(--color-surface-raised);
    border-bottom: 1px solid var(--color-border);
    padding: 8px 20px;
  }
  .reference-notes button {
    display: block;
    text-align: left;
    border: 0;
    background: transparent;
    padding: 5px;
    font-size: 12px;
  }
  :global(sup[data-document-note]) {
    font-size: 0;
    cursor: pointer;
    color: var(--page-link);
  }
  :global(sup[data-document-note]::after) {
    content: attr(data-note-number);
    font-size: 10px;
  }
  :global(nav[data-contents]) {
    border: 1px solid color-mix(in srgb, var(--color-page-ink) 16%, var(--color-page));
    padding: 16px;
  }
  :global(nav[data-contents] strong) {
    display: block;
    margin-bottom: 10px;
  }

  .paper :global([data-page-break]) {
    border-top: 2px dashed var(--color-subtle);
    margin: 26px 0;
    position: relative;
    min-height: 20px;
  }
  .paper :global([data-page-break]::after) {
    content: 'Page break';
    font: 12px Arial;
    color: color-mix(in srgb, var(--color-page-ink) 70%, var(--color-page));
    background: var(--color-page);
    padding: 3px 8px;
    position: absolute;
    inset-inline-start: 40%;
    top: -10px;
  }
  .paper :global(.review-insert) {
    background: var(--color-diff-add-bg);
    color: var(--color-diff-add-ink);
    text-decoration: underline;
  }
  .paper :global(.review-delete) {
    background: var(--color-diff-del-bg);
    color: var(--color-diff-del-ink);
    text-decoration: line-through;
  }
  .paper :global(.source-comment) {
    border-bottom: 2px solid var(--color-warn);
    background: var(--color-warn-bg);
  }
  .paper :global(.tiptap [dir='rtl']) {
    text-align: start;
  }
  .paper :global(.tiptap) {
    font-kerning: normal;
    font-variant-ligatures: normal;
    tab-size: 4;
  }
  @media print {
    .paper :global([data-page-break]) {
      border: 0;
      min-height: 0;
      margin: 0;
      break-after: page;
    }
    .paper :global([data-page-break]::after) {
      content: none;
    }
  }
  dialog {
    border: 1px solid var(--color-border-strong);
    border-radius: 10px;
    padding: 25px;
    max-width: 520px;
    width: calc(100% - 36px);
    color: var(--color-ink);
    background: var(--color-surface-raised);
  }
  dialog::backdrop {
    background: var(--color-overlay);
  }
  dialog label {
    display: block;
    margin: 14px 0;
    font-size: 14px;
  }
  dialog textarea {
    display: block;
    width: 100%;
    font: 15px monospace;
    border: 1px solid var(--color-border-strong);
    border-radius: 5px;
    padding: 10px;
    margin-top: 7px;
    background: var(--color-surface-raised);
    color: inherit;
  }
  dialog p {
    font-size: 14px;
    line-height: 1.6;
    color: var(--color-ink-soft);
  }
  dialog button,
  .feedback button {
    padding: 9px 14px;
    border: 1px solid var(--color-accent-border);
    background: var(--color-accent-bg);
    border-radius: 5px;
    color: var(--color-accent-strong);
    margin-right: 8px;
    cursor: pointer;
  }
  .feedback {
    padding: 12px 22px;
    color: var(--color-ink);
    font-size: 14px;
  }
  .rich-editor :global(ul[data-type='taskList']) {
    list-style: none;
    padding-left: 0;
  }
  .rich-editor :global(li[data-type='taskItem']) {
    display: flex;
    align-items: baseline;
    gap: 10px;
  }
  .rich-editor :global(li[data-type='taskItem'] > div) {
    flex: 1;
  }
  .rich-editor :global(.tiptap-mathematics-render) {
    max-width: 100%;
    overflow: auto;
  }

  .rich-editor {
    font-family:
      Arial, 'Noto Sans Bengali', 'Noto Naskh Arabic', 'Noto Sans Devanagari', 'Noto Sans Hebrew',
      sans-serif;
    flex: 1;
    min-width: 0;
    background: var(--color-chrome);
    display: flex;
    flex-direction: column;
    min-height: 0;
    --page-link: #1a73e8;
  }
  @media screen {
    :global([data-theme='dark']) .rich-editor {
      --page-link: var(--color-accent);
    }
  }
  .paper-area {
    padding: 0 24px 24px;
    overflow: auto;
    flex: 1;
    min-height: 0;
    background: var(--color-canvas);
    overflow-anchor: none;
  }
  .ruler {
    margin: 0 auto 18px;
    height: 22px;
    display: flex;
    justify-content: space-between;
    color: var(--color-ink-soft);
    font-size: 10px;
    border-bottom: 1px solid var(--color-border);
    background: repeating-linear-gradient(
      90deg,
      transparent 0,
      transparent 9px,
      var(--color-border-strong) 10px
    );
    background-size: 100% 5px;
    background-position: center bottom;
    background-repeat: no-repeat;
  }
  .paper {
    position: relative;
    background: var(--color-page);
    border: 0;
    margin: 0 auto;
    color: var(--color-page-ink);
    line-height: 1.65;
    font-size: 11pt;
    box-sizing: border-box;
    flex-shrink: 0;
  }
  .paper.paginated {
    background: transparent;
  }
  /* View > White pages: the page keeps its day colours in night mode, to check print. */
  .paper.light-pages {
    color-scheme: light;
    --color-page: #ffffff;
    --color-page-ink: #202124;
    --color-page-border: #dadce0;
    --page-link: #1a73e8;
    --color-diff-add-bg: #dcf3e3;
    --color-diff-add-ink: #1d5a33;
    --color-diff-del-bg: #fbe3e1;
    --color-diff-del-ink: #8a2f25;
    --color-warn: #a7792f;
    --color-warn-bg: #fff6e5;
  }
  .page-sheets {
    position: absolute;
    inset: 0;
    pointer-events: none;
    -webkit-user-select: none;
    user-select: none;
  }
  .page-sheet {
    position: absolute;
    width: 100%;
    background: var(--color-page);
    border: 1px solid var(--color-page-border);
    box-shadow: 0 1px 2px #0000000c;
    box-sizing: border-box;
  }
  .page-header,
  .page-footer {
    position: absolute;
    font:
      10pt/1.2 Arial,
      sans-serif;
    color: color-mix(in srgb, var(--color-page-ink) 70%, var(--color-page));
    white-space: pre-wrap;
  }
  .page-footer {
    display: flex;
    justify-content: space-between;
    gap: 15px;
  }
  .editor-content {
    position: relative;
  }
  .paper :global(.tiptap) {
    outline: none;
    min-height: 24px;
    font-family: Arial, sans-serif;
    white-space: pre-wrap;
    display: flow-root;
  }
  .paper :global(.pagination-gap) {
    border: 0 !important;
    background: transparent !important;
  }
  .paper.paginated :global([data-page-break]) {
    height: 0;
    min-height: 0;
    margin: 0;
    border: 0;
  }
  .paper.paginated :global([data-page-break]::after) {
    display: none;
  }
  .paper :global(.tiptap p) {
    margin: 0.7em 0;
  }
  .paper :global(.tiptap h1) {
    font-size: 26pt;
    line-height: 1.25;
    margin: 1em 0 0.5em;
  }
  .paper :global(.tiptap h2) {
    font-size: 19pt;
    line-height: 1.3;
    margin: 1em 0 0.5em;
  }
  .paper :global(.tiptap h3) {
    font-size: 15pt;
    margin: 1em 0 0.5em;
  }
  .paper :global(.tiptap a) {
    color: var(--page-link);
    text-decoration: underline;
  }
  .paper :global(.tiptap blockquote) {
    border-left: 3px solid color-mix(in srgb, var(--color-page-ink) 22%, var(--color-page));
    margin: 18px 0;
    padding-left: 18px;
    color: color-mix(in srgb, var(--color-page-ink) 70%, var(--color-page));
  }
  .paper :global(.tiptap img) {
    max-width: 100%;
    height: auto;
  }
  .paper :global(.tiptap table) {
    border-collapse: collapse;
    table-layout: fixed;
    width: 100%;
    margin: 15px 0;
  }
  .paper :global(.tiptap td),
  .paper :global(.tiptap th) {
    min-width: 1em;
    border: 1px solid color-mix(in srgb, var(--color-page-ink) 22%, var(--color-page));
    padding: 6px 8px;
    vertical-align: top;
    position: relative;
  }
  .paper :global(.tiptap th) {
    background: color-mix(in srgb, var(--color-page-ink) 5%, var(--color-page));
    font-weight: 600;
  }
  .paper :global(.selectedCell:after) {
    z-index: 2;
    content: '';
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--color-focus) 15%, transparent);
    pointer-events: none;
  }
  .paper :global(.column-resize-handle) {
    position: absolute;
    right: -2px;
    top: 0;
    bottom: -2px;
    width: 4px;
    background: var(--color-focus);
    pointer-events: none;
  }
  .paper :global(.tiptap ul),
  .paper :global(.tiptap ol) {
    padding-left: 25px;
  }
  .paper :global(.tiptap code) {
    font-family: monospace;
    background: color-mix(in srgb, var(--color-page-ink) 5%, var(--color-page));
    padding: 1px 4px;
    border-radius: 3px;
  }
  .paper :global(.tiptap pre) {
    background: color-mix(in srgb, var(--color-page-ink) 5%, var(--color-page));
    padding: 15px;
    white-space: pre-wrap;
  }
  .paper :global(.tiptap hr) {
    border: 0;
    border-top: 1px solid color-mix(in srgb, var(--color-page-ink) 20%, var(--color-page));
    margin: 22px 0;
  }
  .document-foot {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    gap: 20px;
    min-height: 25px;
    padding: 2px 12px;
    background: var(--color-chrome);
    border-top: 1px solid var(--color-border);
    font:
      11px 'Segoe UI',
      Arial,
      sans-serif;
    color: var(--color-ink);
  }
  .view-controls {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
  }
  .view-controls button {
    display: grid;
    place-items: center;
    border: 0;
    padding: 3px;
    color: var(--color-ink-soft);
    background: transparent;
    cursor: pointer;
  }
  .view-controls button[aria-pressed='true'] {
    background: var(--color-surface-selected);
  }
  .view-controls input {
    width: 95px;
    accent-color: var(--color-muted);
    height: 14px;
  }
  .find-bar,
  .context-tools {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 22px;
    background: var(--color-surface-raised);
    border-bottom: 1px solid var(--color-border);
    color: var(--color-ink-soft);
    font-size: 13px;
    flex-wrap: wrap;
  }
  .find-bar input {
    border: 1px solid var(--color-border);
    padding: 6px 9px;
    border-radius: 4px;
    font-size: 13px;
    max-width: 170px;
    background: var(--color-surface-raised);
    color: var(--color-ink);
  }
  .find-bar button,
  .context-tools button {
    background: var(--color-surface-hover);
    border: 1px solid var(--color-border);
    color: var(--color-ink-soft);
    border-radius: 4px;
    padding: 5px 9px;
    font-size: 13px;
    cursor: pointer;
  }
  .find-bar span {
    font-size: 13px;
    color: var(--color-ink-soft);
  }
  .context-tools {
    padding: 7px 20px;
    gap: 7px;
  }
  .context-tools > span {
    margin-right: 5px;
    font-size: 13px;
  }
  .popover {
    position: fixed;
    inset: 0;
    background: var(--color-overlay);
    display: grid;
    place-items: center;
    z-index: 100;
    -webkit-backdrop-filter: blur(3px);
    backdrop-filter: blur(3px);
  }
  .popover form {
    width: 350px;
    background: var(--color-surface-raised);
    padding: 26px;
    border-radius: 12px;
    box-shadow: var(--shadow-dialog);
  }
  .popover h3 {
    font-size: 18px;
    font-weight: 500;
    margin: 0 0 20px;
  }
  .popover label {
    display: grid;
    gap: 7px;
    font-size: 13px;
    color: var(--color-ink-soft);
  }
  .popover input {
    border: 1px solid var(--color-border);
    border-radius: 5px;
    padding: 10px;
    font-size: 13px;
    background: var(--color-surface-raised);
    color: var(--color-ink);
  }
  .popover form > div {
    display: flex;
    justify-content: end;
    gap: 10px;
    margin-top: 25px;
  }
  .popover button {
    border: 0;
    background: var(--color-surface-sunken);
    border-radius: 5px;
    padding: 8px 16px;
    font-size: 13px;
    color: var(--color-ink-soft);
    cursor: pointer;
  }
  .popover button.apply {
    background: var(--color-accent-solid);
    color: var(--color-on-accent);
  }
  @media (max-width: 600px) {
    .paper-area {
      padding: 0 12px 18px;
    }
    .document-foot {
      gap: 10px;
    }
    .view-controls input {
      width: 55px;
    }
  }
  @media print {
    .context-tools,
    .find-bar,
    .ruler,
    .document-foot {
      display: none;
    }
    .paper-area {
      padding: 0;
      max-height: none;
      overflow: visible;
    }
    .paper {
      border: 0;
      box-shadow: none;
      margin: 0;
      width: 100%;
      min-height: 0;
      max-width: none;
      padding: 15mm;
      zoom: 1 !important;
    }
    .paper :global(.tiptap) {
      min-height: 0;
    }
  }
</style>
