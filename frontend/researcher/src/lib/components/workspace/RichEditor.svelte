<script lang="ts">
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
  import { onMount } from 'svelte';
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
  import {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Undo2,
    Redo2,
    Printer,
    List,
    ListOrdered,
    AlignLeft,
    AlignCenter,
    AlignRight,
    AlignJustify,
    Link,
    Table2,
    Highlighter,
    Search,
    ImagePlus,
    Minus,
    X,
    RemoveFormatting,
    Quote,
    IndentIncrease,
    IndentDecrease
  } from '@lucide/svelte';
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
    preview = $state(false);
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
  const selectedStyle = $derived.by(() => {
    void tick;
    return editor?.getAttributes('textStyle') || {};
  });
  const selectedHeading = $derived.by(() => {
    void tick;
    return editor?.isActive('heading')
      ? String(editor.getAttributes('heading').level)
      : 'paragraph';
  });
  const selectedSize = $derived(String(parseFloat(selectedStyle.fontSize) || 11));
  const sizes = $derived(
    [
      ...new Set([
        '8',
        '9',
        '10',
        '11',
        '12',
        '14',
        '16',
        '18',
        '20',
        '24',
        '28',
        '36',
        '48',
        '72',
        selectedSize
      ])
    ].sort((a, b) => Number(a) - Number(b))
  );
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
        'Your browser blocked clipboard access. Use Ctrl+C, Ctrl+X or Ctrl+V in the document.';
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
        PageBreak,
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
            'pageBreak'
          ],
          generateID: () => ulid()
        })
      ],
      content,
      editable,
      editorProps: {
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
    onPrint={() => (preview = true)}
    onNotice={(message) => (feedback = message)}
  />
  {#if ribbon === 'Home' || ribbon === 'Insert'}
    <div class="toolbar" aria-label="Formatting tools">
      <button
        aria-label="Superscript"
        title="Superscript"
        disabled={!editable}
        class:active={active('superscript')}
        onclick={() => editor?.chain().focus().toggleSuperscript().run()}>x²</button
      >
      <button
        aria-label="Subscript"
        title="Subscript"
        disabled={!editable}
        class:active={active('subscript')}
        onclick={() => editor?.chain().focus().toggleSubscript().run()}>x₂</button
      >
      <button
        aria-label="Checklist"
        title="Checklist"
        disabled={!editable}
        onclick={() => editor?.chain().focus().toggleTaskList().run()}>☑</button
      >
      <button
        aria-label="Insert LaTeX equation"
        title="Insert LaTeX equation"
        disabled={!editable}
        onclick={() => openMath()}>∑</button
      >
      <select
        aria-label="Line spacing"
        disabled={!editable}
        onchange={(e) => {
          editor
            ?.chain()
            .focus()
            .updateAttributes('paragraph', { lineHeight: e.currentTarget.value })
            .updateAttributes('heading', { lineHeight: e.currentTarget.value })
            .run();
        }}
        ><option value="1.6">Line spacing</option><option value="1">Single</option><option
          value="1.15">1.15</option
        ><option value="1.5">1.5</option><option value="2">Double</option></select
      >
      <select
        aria-label="More document tools"
        disabled={!editable}
        onchange={(e) => {
          const c = editor?.chain().focus();
          switch (e.currentTarget.value) {
            case 'code':
              c?.toggleCodeBlock().run();
              break;
            case 'row-before':
              c?.addRowBefore().run();
              break;
            case 'column-before':
              c?.addColumnBefore().run();
              break;
            case 'header':
              c?.toggleHeaderRow().run();
              break;
            case 'merge':
              c?.mergeCells().run();
              break;
            case 'split':
              c?.splitCell().run();
              break;
            case 'table-delete':
              c?.deleteTable().run();
              break;
            case 'unlink':
              c?.unsetLink().run();
              break;
          }
          e.currentTarget.value = '';
        }}
        ><option value="">More tools</option><option value="code">Code block</option><option
          value="unlink">Remove link</option
        ><option value="row-before">Table: row above</option><option value="column-before"
          >Table: column left</option
        ><option value="header">Table: header row</option><option value="merge"
          >Table: merge cells</option
        ><option value="split">Table: split cell</option><option value="table-delete"
          >Delete table</option
        ></select
      >
      <button
        title="Undo (Ctrl+Z)"
        aria-label="Undo"
        disabled={!editable}
        onclick={() => editor?.chain().focus().undo().run()}><Undo2 size={16} /></button
      ><button
        title="Redo (Ctrl+Shift+Z)"
        aria-label="Redo"
        disabled={!editable}
        onclick={() => editor?.chain().focus().redo().run()}><Redo2 size={16} /></button
      ><button title="Print" aria-label="Print" onclick={() => (preview = true)}
        ><Printer size={16} /></button
      ><button
        title="Find and replace"
        aria-label="Find and replace"
        onclick={() => (findOpen = !findOpen)}><Search size={16} /></button
      ><span class="sep"></span>
      <select aria-label="Zoom" bind:value={zoom}
        ><option>75</option><option>90</option><option>100</option><option>125</option><option
          >150</option
        ></select
      ><span class="percent">%</span><span class="sep"></span>
      <select
        aria-label="Paragraph style"
        value={selectedHeading}
        disabled={!editable}
        onchange={(e) => {
          const value = e.currentTarget.value;
          if (value === 'paragraph') editor?.chain().focus().setParagraph().run();
          else
            editor
              ?.chain()
              .focus()
              .toggleHeading({ level: Number(value) as 1 | 2 | 3 | 4 | 5 | 6 })
              .run();
        }}
        ><option value="paragraph">Normal text</option><option value="1">Heading 1</option><option
          value="2">Heading 2</option
        ><option value="3">Heading 3</option><option value="4">Heading 4</option><option value="5"
          >Heading 5</option
        ><option value="6">Heading 6</option></select
      ><span class="sep"></span>
      <select
        aria-label="Font family"
        value={selectedStyle.fontFamily || 'Arial'}
        disabled={!editable}
        onchange={(e) => editor?.chain().focus().setFontFamily(e.currentTarget.value).run()}
        >{#each fontChoices as font}<option value={font}>{font}</option>{/each}</select
      ><select
        class="size"
        aria-label="Font size"
        value={selectedSize}
        disabled={!editable}
        onchange={(e) =>
          editor
            ?.chain()
            .focus()
            .setFontSize(e.currentTarget.value + 'pt')
            .run()}
        >{#each sizes as size}<option value={size}>{size}</option>{/each}</select
      ><span class="sep"></span>
      <button
        aria-label="Bold"
        title="Bold (Ctrl+B)"
        aria-pressed={active('bold')}
        disabled={!editable}
        onclick={() => editor?.chain().focus().toggleBold().run()}><Bold size={16} /></button
      ><button
        aria-label="Italic"
        title="Italic (Ctrl+I)"
        aria-pressed={active('italic')}
        disabled={!editable}
        onclick={() => editor?.chain().focus().toggleItalic().run()}><Italic size={16} /></button
      ><button
        aria-label="Underline"
        title="Underline (Ctrl+U)"
        aria-pressed={active('underline')}
        disabled={!editable}
        onclick={() => editor?.chain().focus().toggleUnderline().run()}
        ><Underline size={16} /></button
      ><button
        aria-label="Strikethrough"
        title="Strikethrough"
        aria-pressed={active('strike')}
        disabled={!editable}
        onclick={() => editor?.chain().focus().toggleStrike().run()}
        ><Strikethrough size={16} /></button
      ><label class="color" title="Text color"
        >A<input
          aria-label="Text color"
          type="color"
          disabled={!editable}
          value="#202124"
          onchange={(e) => editor?.chain().focus().setColor(e.currentTarget.value).run()}
        /></label
      ><button
        aria-label="Highlight"
        title="Highlight text"
        disabled={!editable}
        onclick={() => editor?.chain().focus().toggleHighlight({ color: '#fff2a8' }).run()}
        ><Highlighter size={16} /></button
      ><span class="sep"></span>
      <button
        aria-label="Insert link"
        title="Insert link"
        disabled={!editable}
        onclick={() => {
          linkUrl = editor?.getAttributes('link').href || '';
          linkOpen = true;
        }}><Link size={16} /></button
      ><button
        aria-label="Insert image"
        title="Insert image from URL"
        disabled={!editable}
        onclick={() => (imageOpen = true)}><ImagePlus size={16} /></button
      ><button
        aria-label="Insert table"
        title="Insert a 3 × 3 table"
        disabled={!editable}
        onclick={() =>
          editor?.chain().focus().insertTable({ rows: 3, cols: 3, withHeaderRow: true }).run()}
        ><Table2 size={16} /></button
      ><span class="sep"></span>
      <button
        aria-label="Align left"
        title="Align left"
        disabled={!editable}
        onclick={() => editor?.chain().focus().setTextAlign('left').run()}
        ><AlignLeft size={16} /></button
      ><button
        aria-label="Align center"
        title="Align center"
        disabled={!editable}
        onclick={() => editor?.chain().focus().setTextAlign('center').run()}
        ><AlignCenter size={16} /></button
      ><button
        aria-label="Align right"
        title="Align right"
        disabled={!editable}
        onclick={() => editor?.chain().focus().setTextAlign('right').run()}
        ><AlignRight size={16} /></button
      ><button
        aria-label="Justify"
        title="Justify"
        disabled={!editable}
        onclick={() => editor?.chain().focus().setTextAlign('justify').run()}
        ><AlignJustify size={16} /></button
      ><button
        aria-label="Bulleted list"
        title="Bulleted list"
        disabled={!editable}
        onclick={() => editor?.chain().focus().toggleBulletList().run()}><List size={16} /></button
      ><button
        aria-label="Numbered list"
        title="Numbered list"
        disabled={!editable}
        onclick={() => editor?.chain().focus().toggleOrderedList().run()}
        ><ListOrdered size={16} /></button
      ><button
        aria-label="Increase indent"
        title="Indent list item"
        disabled={!editable}
        onclick={() => editor?.chain().focus().sinkListItem('listItem').run()}
        ><IndentIncrease size={16} /></button
      ><button
        aria-label="Decrease indent"
        title="Outdent list item"
        disabled={!editable}
        onclick={() => editor?.chain().focus().liftListItem('listItem').run()}
        ><IndentDecrease size={16} /></button
      ><button
        aria-label="Block quote"
        title="Block quote"
        disabled={!editable}
        onclick={() => editor?.chain().focus().toggleBlockquote().run()}><Quote size={16} /></button
      ><button
        aria-label="Horizontal rule"
        title="Insert divider"
        disabled={!editable}
        onclick={() => editor?.chain().focus().setHorizontalRule().run()}
        ><Minus size={16} /></button
      ><button
        aria-label="Clear formatting"
        title="Clear formatting"
        disabled={!editable}
        onclick={clearFormatting}><RemoveFormatting size={16} /></button
      >
    </div>
  {/if}
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
    <div class="ruler" aria-hidden="true">
      {#each Array.from({ length: 9 }, (_, i) => i) as i}<span>{i}</span>{/each}
    </div>
    <div
      class="paper"
      lang={page.language}
      style={`zoom:${Number(zoom) / 100};width:${dimensions[0]}mm;min-height:${dimensions[1]}mm;padding:${page.margin}mm`}
    >
      <div bind:this={host}></div>
    </div>
    <div class="document-foot">
      {words} words <span>·</span> Select text to format it. Use Ctrl+Z to undo.
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
    html={editor.getHTML()}
    {fonts}
    {documentId}
    {page}
    onClose={() => (preview = false)}
  />{/if}

<style>
  .paper :global([data-page-break]) {
    border-top: 2px dashed #7c9bc0;
    margin: 26px 0;
    position: relative;
    min-height: 20px;
  }
  .paper :global([data-page-break]::after) {
    content: 'Page break';
    font: 12px Arial;
    color: #456688;
    background: white;
    padding: 3px 8px;
    position: absolute;
    inset-inline-start: 40%;
    top: -10px;
  }
  .paper :global(.review-insert) {
    background: #e1f4e9;
    color: #15633a;
    text-decoration: underline;
  }
  .paper :global(.review-delete) {
    background: #fce7e7;
    color: #a02a32;
    text-decoration: line-through;
  }
  .paper :global(.source-comment) {
    border-bottom: 2px solid #b88b24;
    background: #fff4c8;
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
    border: 1px solid #b9cadc;
    border-radius: 10px;
    padding: 25px;
    max-width: 520px;
    width: calc(100% - 36px);
    color: #29445f;
  }
  dialog::backdrop {
    background: #1b355266;
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
    border: 1px solid #adbed0;
    border-radius: 5px;
    padding: 10px;
    margin-top: 7px;
  }
  dialog p {
    font-size: 14px;
    line-height: 1.6;
    color: #4c647d;
  }
  dialog button,
  .feedback button {
    padding: 9px 14px;
    border: 1px solid #b9cadc;
    background: #e4eefe;
    border-radius: 5px;
    color: #254f86;
    margin-right: 8px;
    cursor: pointer;
  }
  .feedback {
    padding: 12px 22px;
    color: #315675;
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
    background: #f5f7fa;
  }
  .toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 2px;
    padding: 6px 14px;
    background: #edf3fc;
    border-radius: 8px;
    margin: 0 12px;
    border: 1px solid #e4eaf4;
    min-height: 40px;
  }
  .toolbar button {
    width: 29px;
    height: 29px;
    padding: 6px;
    display: grid;
    place-items: center;
    background: none;
    border: 0;
    border-radius: 4px;
    color: #586473;
    cursor: pointer;
  }
  .toolbar button:hover,
  .toolbar button[aria-pressed='true'] {
    background: #d8e8fd;
    color: #315476;
  }
  .toolbar button:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .toolbar select {
    background: none;
    border: 0;
    color: #526172;
    font-size: 13px;
    max-width: 105px;
    padding: 5px 3px;
    height: 28px;
  }
  .toolbar select.size {
    width: 39px;
  }
  .toolbar .sep {
    height: 18px;
    width: 1px;
    background: #d8e1ed;
    margin: 0 5px;
  }
  .percent {
    font-size: 13px;
    margin-left: -3px;
    color: #46617a;
  }
  .color {
    position: relative;
    width: 27px;
    height: 27px;
    display: grid;
    place-items: center;
    color: #364e69;
    font: 600 14px Arial;
    cursor: pointer;
    border-bottom: 3px solid #527bb2;
  }
  .color input {
    position: absolute;
    opacity: 0;
    width: 100%;
    height: 100%;
    cursor: pointer;
  }
  .paper-area {
    padding: 15px 25px 40px;
    overflow: auto;
    max-height: calc(100vh - 165px);
  }
  .ruler {
    width: 794px;
    max-width: 100%;
    margin: auto;
    display: flex;
    justify-content: space-between;
    color: #46617a;
    font-size: 13px;
    border-bottom: 1px solid #d9dfe7;
    padding: 0 15px 4px;
    background: repeating-linear-gradient(90deg, transparent 0, transparent 9px, #d5dce6 10px);
    background-size: 100% 3px;
    background-position: bottom;
    background-repeat: no-repeat;
  }
  .paper {
    background: white;
    border: 1px solid #e0e4ea;
    box-shadow: 0 1px 5px #16273808;
    min-height: 1050px;
    width: 794px;
    max-width: 100%;
    padding: 75px 85px;
    margin: 12px auto 0;
    color: #202124;
    line-height: 1.65;
    font-size: 11pt;
  }
  .paper :global(.tiptap) {
    outline: none;
    min-height: 880px;
    font-family: Arial, sans-serif;
    white-space: pre-wrap;
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
    color: #1a73e8;
    text-decoration: underline;
  }
  .paper :global(.tiptap blockquote) {
    border-left: 3px solid #cbd8d1;
    margin: 18px 0;
    padding-left: 18px;
    color: #617069;
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
    border: 1px solid #cbd2da;
    padding: 6px 8px;
    vertical-align: top;
    position: relative;
  }
  .paper :global(.tiptap th) {
    background: #f2f5f7;
    font-weight: 600;
  }
  .paper :global(.selectedCell:after) {
    z-index: 2;
    content: '';
    position: absolute;
    inset: 0;
    background: #6699ff25;
    pointer-events: none;
  }
  .paper :global(.column-resize-handle) {
    position: absolute;
    right: -2px;
    top: 0;
    bottom: -2px;
    width: 4px;
    background: #73a0e5;
    pointer-events: none;
  }
  .paper :global(.tiptap ul),
  .paper :global(.tiptap ol) {
    padding-left: 25px;
  }
  .paper :global(.tiptap code) {
    font-family: monospace;
    background: #f3f5f7;
    padding: 1px 4px;
    border-radius: 3px;
  }
  .paper :global(.tiptap pre) {
    background: #f3f5f7;
    padding: 15px;
    white-space: pre-wrap;
  }
  .paper :global(.tiptap hr) {
    border: 0;
    border-top: 1px solid #cbd5df;
    margin: 22px 0;
  }
  .document-foot {
    font-size: 13px;
    color: #46617a;
    max-width: 794px;
    margin: 14px auto;
  }
  .document-foot span {
    margin: 0 7px;
  }
  .find-bar,
  .context-tools {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 22px;
    background: white;
    border-bottom: 1px solid #e5e9ef;
    color: #46617a;
    font-size: 13px;
    flex-wrap: wrap;
  }
  .find-bar input {
    border: 1px solid #dbe2ec;
    padding: 6px 9px;
    border-radius: 4px;
    font-size: 13px;
    max-width: 170px;
  }
  .find-bar button,
  .context-tools button {
    background: #f0f4fa;
    border: 1px solid #e1e7f0;
    color: #5c779d;
    border-radius: 4px;
    padding: 5px 9px;
    font-size: 13px;
    cursor: pointer;
  }
  .find-bar span {
    font-size: 13px;
    color: #46617a;
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
    background: #26334540;
    display: grid;
    place-items: center;
    z-index: 100;
    backdrop-filter: blur(3px);
  }
  .popover form {
    width: 350px;
    background: white;
    padding: 26px;
    border-radius: 12px;
    box-shadow: 0 10px 50px #16273020;
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
    color: #46617a;
  }
  .popover input {
    border: 1px solid #dbe2ea;
    border-radius: 5px;
    padding: 10px;
    font-size: 13px;
  }
  .popover form > div {
    display: flex;
    justify-content: end;
    gap: 10px;
    margin-top: 25px;
  }
  .popover button {
    border: 0;
    background: #f0f3f7;
    border-radius: 5px;
    padding: 8px 16px;
    font-size: 13px;
    color: #687789;
    cursor: pointer;
  }
  .popover button.apply {
    background: #185abd;
    color: white;
  }
  @media (max-width: 800px) {
    .paper {
      padding: 35px 25px;
      min-height: 850px;
    }
    .paper-area {
      padding: 10px 12px;
    }
    .toolbar {
      padding: 5px;
      margin: 0 7px;
    }
    .toolbar button {
      width: 27px;
      height: 27px;
    }
    .toolbar .sep {
      margin: 0 3px;
    }
    .toolbar select {
      max-width: 90px;
    }
  }
  @media print {
    .toolbar,
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
