<script lang="ts">
  import { stableJson } from '$lib/workspace/equality';
  import { textOf, type TextBlock } from '$lib/workspace/files';
  import { onMount } from 'svelte';
  import { Editor, type JSONContent } from '@tiptap/core';
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
    onChange: (content: JSONContent) => void;
    onSelect?: (id: string | null) => void;
    revision: number;
  }
  let { content, editable, onChange, onSelect, revision }: Props = $props();
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
    imageUrl = $state('');
  let applyingRemote = false;
  let appliedRevision = -1;
  onMount(() => {
    editor = new Editor({
      element: host,
      extensions: [
        StarterKit.configure({
          link: { openOnClick: false, protocols: ['http', 'https', 'mailto'] }
        }),
        TableKit.configure({ table: { resizable: true } }),
        TextStyleKit,
        TextAlign.configure({ types: ['heading', 'paragraph'] }),
        Highlight.configure({ multicolor: true }),
        Image.configure({ allowBase64: false }),
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
            'codeBlock'
          ],
          generateID: () => ulid()
        })
      ],
      content,
      editable,
      editorProps: {
        attributes: {
          'aria-label': 'Document content',
          role: 'textbox',
          'aria-multiline': 'true',
          spellcheck: 'true'
        }
      },
      onUpdate: ({ editor: e }) => {
        const value = e.getJSON();
        if (!applyingRemote) onChange(value);
      },
      onSelectionUpdate: ({ editor: e }) => {
        tick++;
        const position = e.state.selection.$from;
        onSelect?.(position.depth > 0 ? position.node(1).attrs.blockId : null);
      },
      onTransaction: () => tick++
    });
    return () => editor?.destroy();
  });
  $effect(() => {
    if (editor) editor.setEditable(editable, false);
  });
  $effect(() => {
    const next = stableJson(content);
    if (editor && revision !== appliedRevision) {
      appliedRevision = revision;
      if (next !== stableJson(editor.getJSON())) {
        applyingRemote = true;
        try {
          editor.commands.setContent(content, { emitUpdate: false });
        } finally {
          applyingRemote = false;
        }
      }
    }
  });
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
  <div class="toolbar" aria-label="Formatting tools">
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
    ><button title="Print" aria-label="Print" onclick={() => window.print()}
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
      disabled={!editable}
      onchange={(e) => {
        const value = e.currentTarget.value;
        if (value === 'paragraph') editor?.chain().focus().setParagraph().run();
        else
          editor
            ?.chain()
            .focus()
            .toggleHeading({ level: Number(value) as 1 | 2 | 3 })
            .run();
      }}
      ><option value="paragraph">Normal text</option><option value="1">Heading 1</option><option
        value="2">Heading 2</option
      ><option value="3">Heading 3</option></select
    ><span class="sep"></span>
    <select
      aria-label="Font family"
      disabled={!editable}
      onchange={(e) => editor?.chain().focus().setFontFamily(e.currentTarget.value).run()}
      ><option value="Arial">Arial</option><option value="Georgia">Georgia</option><option
        value="Times New Roman">Times New Roman</option
      ><option value="Verdana">Verdana</option><option value="Courier New">Courier New</option
      ></select
    ><select
      class="size"
      aria-label="Font size"
      disabled={!editable}
      onchange={(e) =>
        editor
          ?.chain()
          .focus()
          .setFontSize(e.currentTarget.value + 'pt')
          .run()}
      ><option>11</option><option>10</option><option>12</option><option>14</option><option
        >16</option
      ><option>18</option><option>24</option><option>36</option></select
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
      onclick={() => editor?.chain().focus().setHorizontalRule().run()}><Minus size={16} /></button
    ><button
      aria-label="Clear formatting"
      title="Clear formatting"
      disabled={!editable}
      onclick={() => editor?.chain().focus().unsetAllMarks().clearNodes().run()}
      ><RemoveFormatting size={16} /></button
    >
  </div>
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
    <div class="paper" style={`zoom:${Number(zoom) / 100}`}><div bind:this={host}></div></div>
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
              editor?.chain().focus().setImage({ src: imageUrl }).run();
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

<style>
  .rich-editor {
    font-family: Arial, sans-serif;
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
    background: #edf2fa;
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
    background: #d8e5f7;
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
    font-size: 11px;
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
    font-size: 10px;
    margin-left: -3px;
    color: #6f7a87;
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
    color: #a3abb6;
    font-size: 8px;
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
    font-size: 10px;
    color: #9ba5b0;
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
    color: #788693;
    font-size: 11px;
    flex-wrap: wrap;
  }
  .find-bar input {
    border: 1px solid #dbe2ec;
    padding: 6px 9px;
    border-radius: 4px;
    font-size: 11px;
    max-width: 170px;
  }
  .find-bar button,
  .context-tools button {
    background: #f0f4fa;
    border: 1px solid #e1e7f0;
    color: #5c779d;
    border-radius: 4px;
    padding: 5px 9px;
    font-size: 10px;
    cursor: pointer;
  }
  .find-bar span {
    font-size: 10px;
    color: #9ba6b2;
  }
  .context-tools {
    padding: 7px 20px;
    gap: 7px;
  }
  .context-tools > span {
    margin-right: 5px;
    font-size: 10px;
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
    font-size: 12px;
    color: #6d7c90;
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
    font-size: 12px;
    color: #687789;
    cursor: pointer;
  }
  .popover button.apply {
    background: #2e674d;
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
