<script lang="ts">
  import type { Editor } from '@tiptap/core';
  import { pageSetup, type PageSetup } from '$lib/workspace/office';
  import { reviewChanges, resolveReview } from '$lib/workspace/tracked-changes';
  import {
    FilePlus2,
    MessageSquare,
    ListTree,
    Accessibility,
    Check,
    X,
    Table2,
    Type,
    Printer
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
    onNotice
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
  } = $props();
  const tabs = ['Home', 'Insert', 'Layout', 'Review', 'View'];
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
    <div class="group">
      <span>Paragraph</span>
      <label
        >Direction<select
          aria-label="Text direction"
          value={currentParagraph.dir ?? 'auto'}
          disabled={!editable}
          onchange={(e) => paragraph({ dir: e.currentTarget.value })}
          ><option value="auto">Automatic</option><option value="ltr">Left to right</option><option
            value="rtl">Right to left</option
          ></select
        ></label
      >
      <label
        >Before<select
          aria-label="Space before paragraph"
          value={currentParagraph.spaceBefore ?? 0}
          disabled={!editable}
          onchange={(e) => paragraph({ spaceBefore: Number(e.currentTarget.value) })}
          >{#each [0, 3, 6, 8, 12, 18, 24] as n}<option value={n}>{n} pt</option>{/each}</select
        ></label
      >
      <label
        >After<select
          aria-label="Space after paragraph"
          value={currentParagraph.spaceAfter ?? 0}
          disabled={!editable}
          onchange={(e) => paragraph({ spaceAfter: Number(e.currentTarget.value) })}
          >{#each [0, 3, 6, 8, 12, 18, 24] as n}<option value={n}>{n} pt</option>{/each}</select
        ></label
      >
      <label
        >Indent<select
          aria-label="Paragraph indent"
          value={currentParagraph.indent ?? 0}
          disabled={!editable}
          onchange={(e) => paragraph({ indent: Number(e.currentTarget.value) })}
          >{#each [0, 18, 36, 54, 72] as n}<option value={n}>{n} pt</option>{/each}</select
        ></label
      >
    </div>
    {#if onFont}<div class="group">
        <span>Fonts</span><button disabled={!editable} onclick={() => fontInput?.click()}
          ><Type size={16} />Upload font</button
        ><input
          class="hidden"
          type="file"
          bind:this={fontInput}
          accept=".woff2,.woff,.ttf,.otf"
          onchange={fontFile}
        />
      </div>{/if}
  {:else if tab === 'Insert'}
    <div class="group">
      <span>Pages & comments</span><button
        disabled={!editable}
        onclick={() => editor?.chain().focus().insertContent({ type: 'pageBreak' }).run()}
        ><FilePlus2 size={16} />Page break</button
      >
      {#if onComment}<button onclick={onComment}><MessageSquare size={16} />Comment</button>{/if}
    </div>
    <div class="group">
      <span>Table</span><label
        >Rows<input
          aria-label="Table rows"
          type="number"
          min="1"
          max="30"
          bind:value={rows}
        /></label
      ><label
        >Columns<input
          aria-label="Table columns"
          type="number"
          min="1"
          max="12"
          bind:value={columns}
        /></label
      ><button
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
            .run()}><Table2 size={16} />Insert table</button
      >
    </div>
    <div class="group">
      <span>Symbols</span
      >{#each ['©', '®', '™', '°', '±', '×', '÷', '—', '…', ' '] as symbol}<button
          class="symbol"
          aria-label={symbol === ' ' ? 'Nonbreaking space' : `Insert ${symbol}`}
          disabled={!editable}
          onclick={() =>
            editor?.chain().focus().insertContent({ type: 'text', text: symbol }).run()}
          >{symbol === ' ' ? '␣' : symbol}</button
        >{/each}
    </div>
  {:else if tab === 'Layout'}
    <div class="group">
      <span>Page setup · saved with this document</span><label
        >Paper<select
          aria-label="Paper size"
          value={page.size}
          disabled={!editable || !onPageChange}
          onchange={(e) => updatePage({ size: e.currentTarget.value as PageSetup['size'] })}
          ><option>A4</option><option>Letter</option><option>Legal</option></select
        ></label
      >
      <label
        >Orientation<select
          aria-label="Page orientation"
          value={page.landscape ? 'landscape' : 'portrait'}
          disabled={!editable || !onPageChange}
          onchange={(e) => updatePage({ landscape: e.currentTarget.value === 'landscape' })}
          ><option value="portrait">Portrait</option><option value="landscape">Landscape</option
          ></select
        ></label
      >
      <label
        >Margins<select
          aria-label="Page margins"
          value={page.margin}
          disabled={!editable || !onPageChange}
          onchange={(e) => updatePage({ margin: Number(e.currentTarget.value) })}
          ><option value={12.7}>Narrow · ½ inch</option><option value={25.4}>Normal · 1 inch</option
          ><option value={38.1}>Wide · 1½ inches</option
          >{#if ![12.7, 25.4, 38.1].includes(page.margin)}<option value={page.margin}
              >{page.margin.toFixed(1)} mm</option
            >{/if}</select
        ></label
      >
      <button onclick={onPrint}><Printer size={16} />Preview pages</button>
    </div>
    <div class="group">
      <span>Header & footer</span><label
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
    <div class="group">
      <span>Text flow</span><button
        disabled={!editable}
        onclick={() => paragraph({ keepWithNext: true })}>Keep with next</button
      ><button
        disabled={!editable}
        onclick={() => paragraph({ keepWithNext: false, pageBreakBefore: false })}
        >Normal flow</button
      >
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
  {:else if tab === 'Review'}
    <div class="group">
      <span>Text revisions</span><button
        aria-pressed={tracking}
        disabled={!editable}
        onclick={() => (tracking = !tracking)}>Track text edits</button
      ><button
        disabled={!editable || !changes.length}
        onclick={() => editor && resolveReview(editor, true)}
        ><Check size={16} />Accept all text changes</button
      ><button
        disabled={!editable || !changes.length}
        onclick={() => editor && resolveReview(editor, false)}
        ><X size={16} />Reject all text changes</button
      >
    </div>
    <div class="group">
      <span>Review</span>{#if onComment}<button onclick={onComment}
          ><MessageSquare size={16} />Comments</button
        >{/if}<button aria-pressed={audit} onclick={() => (audit = !audit)}
        ><Accessibility size={16} />Accessibility check</button
      >
    </div>
    <p class="explain">
      Track insertions and deletions within existing paragraphs. Formatting and structural changes
      remain in version history. Use a personal draft to review a larger rewrite.
    </p>
  {:else}<div class="group">
      <span>Navigate & print</span><button
        aria-pressed={outline}
        onclick={() => (outline = !outline)}><ListTree size={16} />Document outline</button
      ><button onclick={onPrint}><Printer size={16} />Print preview / PDF</button><button
        onclick={() => editor?.commands.focus()}>Return to document</button
      >
    </div>
    <p class="explain">
      Edit in the continuous canvas. Preview pages to check pagination, headers, footers and page
      breaks.
    </p>{/if}
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
    background: #f3f6fb;
    padding: 0 14px;
    border-bottom: 1px solid #cbd8e9;
    overflow: auto;
  }
  .ribbon-tabs button {
    border: 0;
    border-bottom: 3px solid transparent;
    padding: 10px 17px;
    background: none;
    color: #294966;
    font: 14px Arial;
    cursor: pointer;
  }
  .ribbon-tabs button[aria-selected='true'] {
    border-bottom-color: #2766ad;
    background: #e5effc;
    font-weight: 600;
  }
  .office-panel {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    padding: 10px 16px;
    background: #f2f6fc;
    border-bottom: 1px solid #dce5f1;
    font: 13px Arial;
    color: #294966;
  }
  .group {
    display: flex;
    align-items: end;
    flex-wrap: wrap;
    gap: 7px;
    border-inline-end: 1px solid #c8d8e9;
    padding-inline-end: 12px;
    position: relative;
    padding-top: 22px;
  }
  .group > span {
    position: absolute;
    top: 0;
    font-size: 12px;
    color: #4f637b;
  }
  .group label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
  }
  .group .check {
    flex-direction: row;
    align-items: center;
    padding: 8px;
  }
  button,
  select,
  input {
    font: inherit;
    color: inherit;
    border: 1px solid #b8c9dc;
    border-radius: 4px;
    background: white;
    padding: 7px;
    min-height: 34px;
  }
  button {
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }
  button:hover,
  button[aria-pressed='true'] {
    background: #dceafa;
  }
  button:disabled,
  select:disabled,
  input:disabled {
    opacity: 0.5;
    cursor: default;
  }
  input {
    max-width: 155px;
  }
  input[type='number'] {
    max-width: 65px;
  }
  input[type='checkbox'] {
    min-height: 0;
  }
  input.hidden {
    display: none;
  }
  .symbol {
    min-width: 32px;
  }
  .explain {
    margin: 2px 0;
    font-size: 13px;
    line-height: 1.6;
    width: 100%;
    color: #4d637a;
  }
  .review-list,
  .outline,
  .audit {
    max-height: 250px;
    overflow: auto;
    background: #fffdf7;
    padding: 14px 22px;
    border-bottom: 1px solid #cfdaea;
    font: 14px/1.6 Arial;
    color: #284865;
  }
  .review-list > div {
    border-bottom: 1px solid #d3dfeb;
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
