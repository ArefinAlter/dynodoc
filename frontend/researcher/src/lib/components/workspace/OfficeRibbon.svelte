<script lang="ts">
  import RibbonButton from './RibbonButton.svelte';
  import type { Editor } from '@tiptap/core';
  import { pageSetup, officeFonts, type PageSetup } from '$lib/workspace/office';
  import { reviewChanges, resolveReview } from '$lib/workspace/tracked-changes';
  import {
    Bold,
    Italic,
    Underline,
    Strikethrough,
    Undo2,
    Redo2,
    ClipboardPaste,
    Copy,
    Scissors,
    AlignLeft,
    AlignCenter,
    AlignRight,
    AlignJustify,
    List,
    ListOrdered,
    ListChecks,
    IndentIncrease,
    IndentDecrease,
    RemoveFormatting,
    Highlighter,
    Superscript,
    Subscript,
    Search,
    Link,
    ImagePlus,
    Sigma,
    Quote,
    Minus,
    FileText,
    PanelTop,
    FilePlus2,
    MessageSquare,
    ListTree,
    Accessibility,
    Check,
    X,
    Table2,
    Type,
    Printer,
    MessageSquarePlus,
    RefreshCw,
    ArrowDownToLine,
    NotebookText,
    FilePenLine,
    ChevronsDown,
    Pilcrow,
    TextCursor,
    Sun
  } from '@lucide/svelte';
  let {
    editor,
    editable,
    tick,
    tab = $bindable('Home'),
    settings,
    onPageChange,
    onPrint,
    onComment,
    onFont,
    tracking = $bindable(false),
    onNotice,
    onClipboard,
    onFind,
    onLink,
    onImage,
    onMath,
    onReference,
    fonts = officeFonts,
    paginated = $bindable(true),
    lightPages = $bindable(false)
  }: {
    editor: Editor | null;
    editable: boolean;
    tick: number;
    tab?: string;
    settings?: unknown;
    onPageChange?: (page: PageSetup) => void;
    onPrint: () => void;
    onComment?: () => void;
    onFont?: (file: File, family: string) => Promise<void>;
    tracking?: boolean;
    onNotice: (message: string) => void;
    onClipboard: (action: string) => void;
    onFind: () => void;
    onLink: () => void;
    onImage: () => void;
    onMath: () => void;
    onReference: (action: string) => void;
    fonts?: string[];
    paginated?: boolean;
    /** Keep document pages white in night mode (a per-browser option). */
    lightPages?: boolean;
  } = $props();
  function active(name: string) {
    void tick;
    return editor?.isActive(name) || false;
  }
  const style = $derived.by(() => {
    void tick;
    return editor?.getAttributes('textStyle') || {};
  });
  const heading = $derived.by(() => {
    void tick;
    return editor?.isActive('heading')
      ? String(editor.getAttributes('heading').level)
      : 'paragraph';
  });
  const size = $derived(String(parseFloat(style.fontSize) || 11));
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
        size
      ])
    ].sort((a, b) => Number(a) - Number(b))
  );
  function setHeading(value: string) {
    if (value === 'paragraph') editor?.chain().focus().setParagraph().run();
    else
      editor
        ?.chain()
        .focus()
        .setHeading({ level: Number(value) as 1 | 2 | 3 | 4 | 5 | 6 })
        .run();
  }
  function clearFormatting() {
    if (!editor) return;
    const chain = editor.chain().focus();
    for (const name of Object.keys(editor.schema.marks))
      if (!['trackedChange', 'sourceComment'].includes(name)) chain.unsetMark(name);
    chain.clearNodes().run();
  }
  const tabs = ['Home', 'Insert', 'Layout', 'References', 'Review', 'View'];
  let rows = $state(3),
    columns = $state(3),
    outline = $state(false),
    audit = $state(false),
    fontInput = $state<HTMLInputElement>();
  const page = $derived(pageSetup(settings));
  const currentParagraph = $derived.by(() => {
    void tick;
    return editor?.getAttributes(editor.isActive('heading') ? 'heading' : 'paragraph') || {};
  });
  const changes = $derived.by(() => {
    void tick;
    return editor ? reviewChanges(editor.state.doc) : [];
  });
  const importedComments = $derived.by(() => {
    void tick;
    const comments = new Map<string, { author: string; body: string; pos: number }>();
    editor?.state.doc.descendants((node, pos) => {
      const m = node.marks.find((mark) => mark.type.name === 'sourceComment');
      if (m && !comments.has(m.attrs.id))
        comments.set(m.attrs.id, {
          author: String(m.attrs.author),
          body: String(m.attrs.body),
          pos
        });
    });
    return [...comments.values()];
  });
  const headings = $derived.by(() => {
    void tick;
    const result: { pos: number; level: number; text: string }[] = [];
    editor?.state.doc.descendants((node, pos) => {
      if (node.type.name === 'heading')
        result.push({ pos, level: node.attrs.level, text: node.textContent });
    });
    return result;
  });
  const issues = $derived.by(() => {
    void tick;
    const result: { pos: number; text: string }[] = [];
    let level = 0;
    editor?.state.doc.descendants((node, pos) => {
      if (node.type.name === 'image' && !String(node.attrs.alt || '').trim())
        result.push({ pos, text: 'Add alternative text to this image.' });
      if (node.type.name === 'heading') {
        if (node.attrs.level > level + 1)
          result.push({ pos, text: `Heading jumps from level ${level} to ${node.attrs.level}.` });
        level = node.attrs.level;
      }
      if (
        node.type.name === 'table' &&
        !node.firstChild?.content.content.some((c) => c.type.name === 'tableHeader')
      )
        result.push({ pos, text: 'Consider a header row for this table.' });
      if (
        node.isText &&
        node.marks.some((m) => m.type.name === 'link') &&
        /^(click here|here|link)$/i.test(node.text || '')
      )
        result.push({ pos, text: 'Use meaningful link text instead of “click here”.' });
    });
    return result;
  });
  function paragraph(attrs: Record<string, unknown>) {
    editor
      ?.chain()
      .focus()
      .updateAttributes('paragraph', attrs)
      .updateAttributes('heading', attrs)
      .run();
  }
  function updatePage(patch: Partial<PageSetup>) {
    onPageChange?.(pageSetup({ ...page, ...patch }));
  }
  function tabKey(event: KeyboardEvent, index: number) {
    if (!['ArrowLeft', 'ArrowRight', 'Home', 'End'].includes(event.key)) return;
    event.preventDefault();
    const i =
      event.key === 'Home'
        ? 0
        : event.key === 'End'
          ? tabs.length - 1
          : (index + (event.key === 'ArrowRight' ? 1 : -1) + tabs.length) % tabs.length;
    tab = tabs[i];
    (event.currentTarget as HTMLElement).parentElement
      ?.querySelectorAll<HTMLButtonElement>('[role=tab]')
      [i].focus();
  }
  async function fontFile(e: Event) {
    const input = e.currentTarget as HTMLInputElement,
      file = input.files?.[0];
    if (!file) return;
    input.value = '';
    const family = prompt(
      'Font family name (use the name from the font file)',
      file.name.replace(/\.[^.]+$/, '').replace(/[-_]/g, ' ')
    )?.trim();
    if (!family) return;
    try {
      await onFont?.(file, family);
      onNotice(
        `“${family}” is available to members of this document. Only upload fonts you have permission to share.`
      );
    } catch (e) {
      onNotice(e instanceof Error ? e.message : 'Could not load this font.');
    }
  }
</script>

<div class="ribbon-tabs" role="tablist" aria-label="Document ribbon">
  {#each tabs as name, i}<button
      role="tab"
      id={`ribbon-${name}`}
      aria-selected={tab === name}
      aria-controls="office-panel"
      tabindex={tab === name ? 0 : -1}
      onclick={() => (tab = name)}
      onkeydown={(e) => tabKey(e, i)}>{name}</button
    >{/each}
</div>
<div class="office-panel" id="office-panel" role="tabpanel" aria-labelledby={`ribbon-${tab}`}>
  {#if tab === 'Home'}
    <div class="group clipboard">
      <span class="group-label">Clipboard</span>
      <div class="group-body">
        <RibbonButton
          label="Paste"
          icon={ClipboardPaste}
          large
          disabled={!editable}
          onclick={() => onClipboard('paste')}
        />
        <div class="stack">
          <RibbonButton
            label="Cut"
            icon={Scissors}
            disabled={!editable}
            onclick={() => onClipboard('cut')}
          /><RibbonButton label="Copy" icon={Copy} onclick={() => onClipboard('copy')} />
        </div>
      </div>
    </div>
    <div class="group font-group">
      <span class="group-label">Font</span>
      <div class="group-body">
        <div class="stack">
          <div class="row">
            <select
              aria-label="Font family"
              value={style.fontFamily || 'Arial'}
              disabled={!editable}
              onchange={(e) => editor?.chain().focus().setFontFamily(e.currentTarget.value).run()}
              >{#each fonts as font}<option>{font}</option>{/each}</select
            >
            <select
              class="size"
              aria-label="Font size"
              value={size}
              disabled={!editable}
              onchange={(e) =>
                editor
                  ?.chain()
                  .focus()
                  .setFontSize(e.currentTarget.value + 'pt')
                  .run()}
              >{#each sizes as value}<option>{value}</option>{/each}</select
            >
            <RibbonButton
              label="Clear formatting"
              icon={RemoveFormatting}
              disabled={!editable}
              onclick={clearFormatting}
            />
          </div>
          <div class="row">
            <RibbonButton
              label="Bold"
              icon={Bold}
              disabled={!editable}
              active={active('bold')}
              onclick={() => editor?.chain().focus().toggleBold().run()}
            />
            <RibbonButton
              label="Italic"
              icon={Italic}
              disabled={!editable}
              active={active('italic')}
              onclick={() => editor?.chain().focus().toggleItalic().run()}
            />
            <RibbonButton
              label="Underline"
              icon={Underline}
              disabled={!editable}
              active={active('underline')}
              onclick={() => editor?.chain().focus().toggleUnderline().run()}
            />
            <RibbonButton
              label="Strikethrough"
              icon={Strikethrough}
              disabled={!editable}
              active={active('strike')}
              onclick={() => editor?.chain().focus().toggleStrike().run()}
            />
            <RibbonButton
              label="Subscript"
              icon={Subscript}
              disabled={!editable}
              active={active('subscript')}
              onclick={() => editor?.chain().focus().toggleSubscript().run()}
            />
            <RibbonButton
              label="Superscript"
              icon={Superscript}
              disabled={!editable}
              active={active('superscript')}
              onclick={() => editor?.chain().focus().toggleSuperscript().run()}
            />
            <RibbonButton
              label="Highlight"
              icon={Highlighter}
              disabled={!editable}
              onclick={() => editor?.chain().focus().toggleHighlight({ color: '#fff2a8' }).run()}
            />
            <label class="color" title="Text color"
              >A<input
                aria-label="Text color"
                type="color"
                value={style.color || '#202124'}
                disabled={!editable}
                onchange={(e) => editor?.chain().focus().setColor(e.currentTarget.value).run()}
              /></label
            >
          </div>
        </div>
      </div>
    </div>
    <div class="group">
      <span class="group-label">Paragraph</span>
      <div class="group-body">
        <div class="stack">
          <div class="row">
            <RibbonButton
              label="Bulleted list"
              icon={List}
              disabled={!editable}
              active={active('bulletList')}
              onclick={() => editor?.chain().focus().toggleBulletList().run()}
            />
            <RibbonButton
              label="Numbered list"
              icon={ListOrdered}
              disabled={!editable}
              active={active('orderedList')}
              onclick={() => editor?.chain().focus().toggleOrderedList().run()}
            />
            <RibbonButton
              label="Checklist"
              icon={ListChecks}
              disabled={!editable}
              active={active('taskList')}
              onclick={() => editor?.chain().focus().toggleTaskList().run()}
            />
            <RibbonButton
              label="Decrease indent"
              icon={IndentDecrease}
              disabled={!editable}
              onclick={() =>
                paragraph({ indent: Math.max(0, Number(currentParagraph.indent || 0) - 18) })}
            />
            <RibbonButton
              label="Increase indent"
              icon={IndentIncrease}
              disabled={!editable}
              onclick={() =>
                paragraph({ indent: Math.min(144, Number(currentParagraph.indent || 0) + 18) })}
            />
            <select
              class="direction"
              title="Text direction"
              aria-label="Text direction"
              value={currentParagraph.dir ?? 'auto'}
              disabled={!editable}
              onchange={(e) => paragraph({ dir: e.currentTarget.value })}
              ><option value="auto">Auto</option><option value="ltr">LTR</option><option value="rtl"
                >RTL</option
              ></select
            >
          </div>
          <div class="row">
            {#each [{ label: 'Align left', value: 'left', icon: AlignLeft }, { label: 'Align center', value: 'center', icon: AlignCenter }, { label: 'Align right', value: 'right', icon: AlignRight }, { label: 'Justify', value: 'justify', icon: AlignJustify }] as align}
              <RibbonButton
                label={align.label}
                icon={align.icon}
                disabled={!editable}
                active={currentParagraph.textAlign === align.value}
                onclick={() => editor?.chain().focus().setTextAlign(align.value).run()}
              />
            {/each}
            <select
              class="spacing"
              aria-label="Line spacing"
              title="Line spacing"
              value={currentParagraph.lineHeight || '1.6'}
              disabled={!editable}
              onchange={(e) => paragraph({ lineHeight: e.currentTarget.value })}
              ><option value="1">1.0</option><option value="1.15">1.15</option><option value="1.5"
                >1.5</option
              ><option value="1.6">1.6</option><option value="2">2.0</option></select
            >
            <RibbonButton
              label="Block quote"
              icon={Quote}
              disabled={!editable}
              onclick={() => editor?.chain().focus().toggleBlockquote().run()}
            />
          </div>
        </div>
      </div>
    </div>
    <div class="group styles">
      <span class="group-label">Styles</span>
      <div class="group-body">
        {#each [{ value: 'paragraph', label: 'Normal' }, { value: '1', label: 'Heading 1' }, { value: '2', label: 'Heading 2' }] as item}
          <button
            class="style-card"
            class:heading={item.value !== 'paragraph'}
            aria-pressed={heading === item.value}
            disabled={!editable}
            onmousedown={(e) => e.preventDefault()}
            onclick={() => setHeading(item.value)}>{item.label}</button
          >
        {/each}
        <select
          class="style-picker"
          aria-label="Paragraph style"
          value={heading}
          disabled={!editable}
          onchange={(e) => setHeading(e.currentTarget.value)}
          ><option value="paragraph">Normal</option>{#each [1, 2, 3, 4, 5, 6] as level}<option
              value={level}>Heading {level}</option
            >{/each}</select
        >
      </div>
    </div>
    <div class="group">
      <span class="group-label">Editing</span>
      <div class="group-body">
        <div class="stack">
          <RibbonButton label="Find and replace" icon={Search} onclick={onFind} />
          <div class="row">
            <RibbonButton
              label="Undo"
              icon={Undo2}
              disabled={!editable}
              onclick={() => editor?.chain().focus().undo().run()}
            /><RibbonButton
              label="Redo"
              icon={Redo2}
              disabled={!editable}
              onclick={() => editor?.chain().focus().redo().run()}
            />
          </div>
        </div>
      </div>
    </div>
  {:else if tab === 'Insert'}
    <div class="group">
      <span class="group-label">Illustrations & links</span>
      <div class="group-body">
        <RibbonButton
          label="Insert image"
          text="Picture"
          icon={ImagePlus}
          large
          disabled={!editable}
          onclick={onImage}
        /><RibbonButton
          label="Insert link"
          text="Link"
          icon={Link}
          large
          disabled={!editable}
          onclick={onLink}
        /><RibbonButton
          label="Insert LaTeX equation"
          text="Equation"
          icon={Sigma}
          large
          disabled={!editable}
          onclick={onMath}
        /><RibbonButton
          label="Horizontal rule"
          text="Horizontal line"
          icon={Minus}
          large
          disabled={!editable}
          onclick={() => editor?.chain().focus().setHorizontalRule().run()}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Pages & comments</span>
      <div class="group-body">
        <RibbonButton
          label="Page break"
          icon={FilePlus2}
          large
          disabled={!editable}
          onclick={() => editor?.chain().focus().insertContent({ type: 'pageBreak' }).run()}
        />{#if onComment}<RibbonButton
            label="Comment"
            icon={MessageSquarePlus}
            large
            onclick={onComment}
          />{/if}
      </div>
    </div>
    <div class="group">
      <span class="group-label">Table</span>
      <div class="group-body">
        <div class="stack fields">
          <label class="inline"
            >Rows<input
              aria-label="Table rows"
              type="number"
              min="1"
              max="30"
              bind:value={rows}
            /></label
          ><label class="inline"
            >Columns<input
              aria-label="Table columns"
              type="number"
              min="1"
              max="12"
              bind:value={columns}
            /></label
          >
        </div>
        <RibbonButton
          label="Insert table"
          text="Table"
          icon={Table2}
          large
          disabled={!editable}
          onclick={() =>
            editor
              ?.chain()
              .focus()
              .insertTable({
                rows: Math.max(1, Math.min(30, rows)),
                cols: Math.max(1, Math.min(12, columns)),
                withHeaderRow: true
              })
              .run()}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Symbols</span>
      <div class="group-body">
        <div class="symbols">
          {#each ['©', '®', '™', '°', '±', '×', '÷', '—', '…', ' '] as symbol}<button
              class="symbol"
              title={symbol === ' ' ? 'Nonbreaking space' : `Insert ${symbol}`}
              aria-label={symbol === ' ' ? 'Nonbreaking space' : `Insert ${symbol}`}
              disabled={!editable}
              onmousedown={(e) => e.preventDefault()}
              onclick={() =>
                editor?.chain().focus().insertContent({ type: 'text', text: symbol }).run()}
              >{symbol === ' ' ? '␣' : symbol}</button
            >{/each}
        </div>
      </div>
    </div>
  {:else if tab === 'Layout'}
    <div class="group">
      <span class="group-label">Paragraph</span>
      <div class="group-body">
        <label
          >Before<select
            aria-label="Space before paragraph"
            value={currentParagraph.spaceBefore ?? 0}
            disabled={!editable}
            onchange={(e) => paragraph({ spaceBefore: Number(e.currentTarget.value) })}
            >{#each [0, 3, 6, 8, 12, 18, 24] as n}<option value={n}>{n} pt</option>{/each}</select
          ></label
        ><label
          >After<select
            aria-label="Space after paragraph"
            value={currentParagraph.spaceAfter ?? 0}
            disabled={!editable}
            onchange={(e) => paragraph({ spaceAfter: Number(e.currentTarget.value) })}
            >{#each [0, 3, 6, 8, 12, 18, 24] as n}<option value={n}>{n} pt</option>{/each}</select
          ></label
        ><label
          >Indent<select
            aria-label="Paragraph indent"
            value={currentParagraph.indent ?? 0}
            disabled={!editable}
            onchange={(e) => paragraph({ indent: Number(e.currentTarget.value) })}
            >{#each [0, 18, 36, 54, 72] as n}<option value={n}>{n} pt</option>{/each}</select
          ></label
        >
      </div>
    </div>
    <div class="group">
      <span class="group-label">Page setup</span>
      <div class="group-body">
        <label
          >Paper<select
            aria-label="Paper size"
            value={page.size}
            disabled={!editable || !onPageChange}
            onchange={(e) => updatePage({ size: e.currentTarget.value as PageSetup['size'] })}
            ><option>A4</option><option>Letter</option><option>Legal</option></select
          ></label
        ><label
          >Orientation<select
            aria-label="Page orientation"
            value={page.landscape ? 'landscape' : 'portrait'}
            disabled={!editable || !onPageChange}
            onchange={(e) => updatePage({ landscape: e.currentTarget.value === 'landscape' })}
            ><option value="portrait">Portrait</option><option value="landscape">Landscape</option
            ></select
          ></label
        ><label
          >Margins<select
            aria-label="Page margins"
            value={page.margin}
            disabled={!editable || !onPageChange}
            onchange={(e) => updatePage({ margin: Number(e.currentTarget.value) })}
            ><option value={12.7}>Narrow · ½ inch</option><option value={25.4}
              >Normal · 1 inch</option
            ><option value={38.1}>Wide · 1½ inches</option
            >{#if ![12.7, 25.4, 38.1].includes(page.margin)}<option value={page.margin}
                >{page.margin.toFixed(1)} mm</option
              >{/if}</select
          ></label
        ><RibbonButton label="Preview pages" icon={Printer} large onclick={onPrint} />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Header & footer</span>
      <div class="group-body">
        <label
          >Header<input
            aria-label="Page header"
            value={page.header}
            maxlength="200"
            disabled={!editable || !onPageChange}
            onchange={(e) => updatePage({ header: e.currentTarget.value })}
          /></label
        ><label
          >Footer<input
            aria-label="Page footer"
            value={page.footer}
            maxlength="200"
            disabled={!editable || !onPageChange}
            onchange={(e) => updatePage({ footer: e.currentTarget.value })}
          /></label
        ><label class="check"
          ><input
            type="checkbox"
            checked={page.pageNumbers}
            disabled={!editable || !onPageChange}
            onchange={(e) => updatePage({ pageNumbers: e.currentTarget.checked })}
          />Page numbers</label
        >
      </div>
    </div>
    <div class="group">
      <span class="group-label">Text flow</span>
      <div class="group-body">
        <div class="stack">
          <RibbonButton
            label="Keep with next"
            icon={ChevronsDown}
            size="small"
            disabled={!editable}
            onclick={() => paragraph({ keepWithNext: true })}
          /><RibbonButton
            label="Normal flow"
            icon={Pilcrow}
            size="small"
            disabled={!editable}
            onclick={() => paragraph({ keepWithNext: false, pageBreakBefore: false })}
          />
        </div>
        <label
          >Language<select
            aria-label="Document language"
            value={page.language}
            disabled={!editable || !onPageChange}
            onchange={(e) => updatePage({ language: e.currentTarget.value })}
            >{#each [['en', 'English'], ['bn', 'বাংলা'], ['ar', 'العربية'], ['he', 'עברית'], ['hi', 'हिन्दी'], ['zh', '中文'], ['ja', '日本語'], ['fr', 'Français'], ['es', 'Español'], ['de', 'Deutsch']] as [code, name]}<option
                value={code}>{name}</option
              >{/each}</select
          ></label
        >
      </div>
    </div>
  {:else if tab === 'References'}
    <div class="group">
      <span class="group-label">Table of Contents</span>
      <div class="group-body">
        <RibbonButton
          icon={ListTree}
          label="Insert table of contents"
          text="Table of Contents"
          large
          disabled={!editable}
          onclick={() => onReference('contents')}
        /><RibbonButton
          icon={RefreshCw}
          label="Update table of contents"
          text="Update Table"
          size="small"
          disabled={!editable}
          onclick={() => onReference('update-contents')}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Footnotes</span>
      <div class="group-body">
        <RibbonButton
          icon={Superscript}
          label="Insert footnote"
          text="Insert Footnote"
          large
          disabled={!editable}
          onclick={() => onReference('footnote')}
        />
        <div class="stack">
          <RibbonButton
            icon={ArrowDownToLine}
            label="Insert endnote"
            text="Insert Endnote"
            size="small"
            disabled={!editable}
            onclick={() => onReference('endnote')}
          /><RibbonButton
            icon={NotebookText}
            label="Show notes"
            text="Show Notes"
            size="small"
            onclick={() => onReference('notes')}
          />
        </div>
      </div>
    </div>
  {:else if tab === 'Review'}
    <div class="group">
      <span class="group-label">Tracking</span>
      <div class="group-body">
        <RibbonButton
          label="Track text edits"
          text="Track Changes"
          icon={FilePenLine}
          large
          active={tracking}
          disabled={!editable}
          onclick={() => (tracking = !tracking)}
        />
        <div class="stack">
          <RibbonButton
            label="Accept all text changes"
            text="Accept all"
            icon={Check}
            size="small"
            disabled={!editable || !changes.length}
            onclick={() => editor && resolveReview(editor, true)}
          /><RibbonButton
            label="Reject all text changes"
            text="Reject all"
            icon={X}
            size="small"
            disabled={!editable || !changes.length}
            onclick={() => editor && resolveReview(editor, false)}
          />
        </div>
      </div>
    </div>
    <div class="group">
      <span class="group-label">Comments & checks</span>
      <div class="group-body">
        {#if onComment}<RibbonButton
            label="Comments"
            icon={MessageSquare}
            large
            onclick={onComment}
          />{/if}<RibbonButton
          label="Accessibility check"
          text="Check Accessibility"
          icon={Accessibility}
          large
          active={audit}
          onclick={() => (audit = !audit)}
        />
      </div>
    </div>
  {:else}<div class="group">
      <span class="group-label">Views</span>
      <div class="group-body">
        <RibbonButton
          label="Print layout"
          icon={FileText}
          large
          active={paginated}
          onclick={() => (paginated = true)}
        /><RibbonButton
          label="Continuous view"
          text="Continuous"
          icon={PanelTop}
          large
          active={!paginated}
          onclick={() => (paginated = false)}
        /><RibbonButton
          label="Always show white pages"
          text="White pages"
          icon={Sun}
          large
          active={lightPages}
          onclick={() => (lightPages = !lightPages)}
        />
      </div>
    </div>
    <div class="group">
      <span class="group-label">Navigate & print</span>
      <div class="group-body">
        <RibbonButton
          label="Print preview / PDF"
          text="Print Preview"
          icon={Printer}
          large
          onclick={onPrint}
        />
        <div class="stack">
          <RibbonButton
            label="Document outline"
            icon={ListTree}
            size="small"
            active={outline}
            onclick={() => (outline = !outline)}
          /><RibbonButton
            label="Return to document"
            icon={TextCursor}
            size="small"
            onclick={() => editor?.commands.focus()}
          />
        </div>
      </div>
    </div>
    {#if onFont}<div class="group">
        <span class="group-label">Fonts</span>
        <div class="group-body">
          <RibbonButton
            label="Upload font"
            icon={Type}
            large
            disabled={!editable}
            onclick={() => fontInput?.click()}
          /><input
            class="hidden"
            type="file"
            bind:this={fontInput}
            accept=".woff2,.woff,.ttf,.otf"
            onchange={fontFile}
          />
        </div>
      </div>{/if}
  {/if}
</div>
{#if tab === 'Review' && changes.length}<section
    class="review-list"
    aria-label="Tracked text changes"
  >
    {#each changes as change}<div>
        <span
          ><strong>{change.kind === 'insert' ? 'Inserted' : 'Deleted'}</strong> · {change.author}</span
        >
        <p dir="auto">{change.text.slice(0, 240)}{change.text.length > 240 ? '…' : ''}</p>
        <button
          disabled={!editable}
          onclick={() => editor && resolveReview(editor, true, change.id)}>Accept change</button
        ><button
          disabled={!editable}
          onclick={() => editor && resolveReview(editor, false, change.id)}>Reject change</button
        >
      </div>{/each}
  </section>{/if}
{#if tab === 'Review' && importedComments.length}<section
    class="review-list"
    aria-label="Imported Word comments"
  >
    <strong>Comments from the original Word file</strong>{#each importedComments as comment}<div>
        <strong>{comment.author}</strong>
        <p dir="auto">{comment.body}</p>
        <button
          onclick={() =>
            editor?.chain().focus().setTextSelection(comment.pos).scrollIntoView().run()}
          >Go to comment</button
        >
      </div>{/each}
  </section>{/if}
{#if outline && tab === 'View'}<nav class="outline" aria-label="Document outline">
    {#each headings as heading}<button
        style={`padding-inline-start:${12 + heading.level * 12}px`}
        onclick={() =>
          editor
            ?.chain()
            .focus()
            .setTextSelection(heading.pos + 1)
            .scrollIntoView()
            .run()}>{heading.text || 'Untitled heading'}</button
      >{:else}<p>Add headings to build the outline.</p>{/each}
  </nav>{/if}
{#if audit && tab === 'Review'}<section class="audit" aria-label="Accessibility findings">
    <p>
      {issues.length
        ? `${issues.length} things to review`
        : 'No issues found by these basic checks.'} This checks heading order, image descriptions, table
      headers and link text; it is not a full accessibility certification.
    </p>
    {#each issues as issue}<button
        onclick={() =>
          editor
            ?.chain()
            .focus()
            .setTextSelection(issue.pos + 1)
            .scrollIntoView()
            .run()}>{issue.text}</button
      >{/each}
  </section>{/if}

<style>
  .ribbon-tabs {
    display: flex;
    gap: 2px;
    background: var(--color-chrome);
    padding: 0 16px;
    overflow: auto;
    flex-shrink: 0;
  }
  .ribbon-tabs button {
    border: 0;
    border-bottom: 2px solid transparent;
    padding: 8px 13px 6px;
    background: none;
    color: var(--color-ink-strong);
    font:
      13px 'Segoe UI',
      Arial,
      sans-serif;
    cursor: pointer;
  }
  .ribbon-tabs button[aria-selected='true'] {
    border-bottom-color: var(--color-word);
    font-weight: 600;
  }
  .office-panel {
    display: flex;
    gap: 9px;
    padding: 5px 12px 3px;
    margin: 0 5px;
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border);
    border-radius: 5px;
    box-shadow: var(--shadow-panel);
    font:
      12px 'Segoe UI',
      Arial,
      sans-serif;
    color: var(--color-ink-strong);
    min-height: 96px;
    overflow-x: auto;
    flex-shrink: 0;
  }
  /* Each group is its commands above a one-line caption; the group is as wide as the
     wider of the two, so captions never wrap over or overlap the commands. */
  .group {
    display: flex;
    flex-direction: column;
    gap: 2px;
    border-inline-end: 1px solid var(--color-border);
    padding: 0 10px 0 6px;
    flex-shrink: 0;
  }
  .group:first-child {
    padding-inline-start: 0;
  }
  .group:last-child {
    border: 0;
  }
  .group-body {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: 1;
    min-height: 70px;
  }
  .group-label {
    order: 1;
    text-align: center;
    font-size: 11px;
    line-height: 14px;
    color: var(--color-ink-soft);
    white-space: nowrap;
    padding-bottom: 2px;
  }
  .group label {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 11px;
  }
  .group .check {
    flex-direction: row;
    align-items: center;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 2px;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  button,
  select,
  input {
    font: inherit;
    color: inherit;
    border: 1px solid var(--color-border-strong);
    border-radius: 2px;
    background: var(--color-surface-raised);
    padding: 4px;
    min-height: 27px;
  }
  button {
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
  }
  button:hover,
  button[aria-pressed='true'] {
    background: var(--color-surface-selected);
    border-color: var(--color-word);
  }
  button:disabled,
  select:disabled,
  input:disabled {
    opacity: 0.45;
    cursor: default;
  }
  input {
    max-width: 140px;
  }
  input[type='number'] {
    max-width: 48px;
  }
  input[type='checkbox'] {
    min-height: 0;
  }
  input.hidden {
    display: none;
  }
  .font-group select {
    width: 155px;
  }
  .font-group .size {
    width: 48px;
  }
  .direction,
  .spacing {
    width: 58px;
  }
  .symbols {
    display: grid;
    grid-template-columns: repeat(5, 26px);
    gap: 3px;
  }
  .symbol {
    min-width: 26px;
    min-height: 26px;
    padding: 0;
    border-color: transparent;
    font-size: 14px;
  }
  .stack.fields {
    gap: 5px;
  }
  .group label.inline {
    flex-direction: row;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .styles .group-body {
    gap: 4px;
  }
  .style-card {
    min-width: 82px;
    height: 56px;
    font-size: 15px;
    border: 1px solid transparent;
  }
  .style-card.heading {
    color: var(--color-accent-strong);
    font-size: 17px;
  }
  .style-picker {
    width: 25px;
    max-width: 25px;
    height: 56px;
    font-size: 0;
  }
  .style-picker option {
    font-size: 13px;
  }
  /* On a touch screen the icon-only select would still zoom the page, so show its name. */
  @media (pointer: coarse) {
    .style-picker {
      width: 104px;
      max-width: 104px;
    }
  }
  .group .color {
    position: relative;
    width: 26px;
    height: 26px;
    display: grid;
    place-items: center;
    border-bottom: 3px solid var(--color-danger);
    font-weight: 600;
    font-size: 15px;
  }
  .color input {
    position: absolute;
    opacity: 0;
    inset: 0;
    width: 100%;
    height: 100%;
    padding: 0;
    cursor: pointer;
  }
  .review-list,
  .outline,
  .audit {
    max-height: 250px;
    overflow: auto;
    background: var(--color-surface);
    padding: 14px 22px;
    border-bottom: 1px solid var(--color-border);
    font: 14px/1.6 Arial;
    color: var(--color-ink);
  }
  .review-list > div {
    border-bottom: 1px solid var(--color-border);
    padding: 10px 0;
  }
  .review-list p {
    white-space: pre-wrap;
    margin: 5px 0;
  }
  .review-list button {
    margin-right: 8px;
  }
  .outline button,
  .audit button {
    display: flex;
    width: 100%;
    margin-bottom: 6px;
    text-align: start;
    justify-content: start;
  }
  @media print {
    .ribbon-tabs,
    .office-panel,
    .review-list,
    .outline,
    .audit {
      display: none;
    }
  }
  @media (max-width: 600px) {
    .ribbon-tabs button {
      padding: 9px 13px;
    }
    .office-panel {
      padding: 10px;
    }
    .group {
      border: 0;
    }
  }
</style>
