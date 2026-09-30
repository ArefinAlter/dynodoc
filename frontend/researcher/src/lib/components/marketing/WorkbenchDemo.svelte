<script lang="ts">
  import { onMount } from 'svelte';
  let ready = $state(false);
  onMount(() => {
    ready = true;
  });
  import {
    FileText,
    Table2,
    ListChecks,
    Undo2,
    Redo2,
    Bold,
    Italic,
    AlignLeft,
    List,
    MessageSquare,
    History,
    Check,
    ChevronDown,
    ArrowUpRight,
    MousePointer2,
    LockKeyhole
  } from '@lucide/svelte';
  let mode = $state<'document' | 'spreadsheet' | 'questionnaire'>('document');
  let wording = $state(false);
  let reviewed = $state(false);
  let showHistory = $state(false);
  let selection = $state('interview');
  let north = $state(28);
  let south = $state(34);
  let east = $state(19);
  const total = $derived(north + south + east);
</script>

<div class="explorer" class:sheet={mode === 'spreadsheet'}>
  <div class="switcher" aria-label="Explore the editors">
    <button disabled={!ready} aria-pressed={mode === 'document'} onclick={() => (mode = 'document')}
      ><FileText size={17} /> Documents</button
    >
    <button
      disabled={!ready}
      aria-pressed={mode === 'spreadsheet'}
      onclick={() => (mode = 'spreadsheet')}><Table2 size={17} /> Spreadsheets</button
    >
    <button
      disabled={!ready}
      aria-pressed={mode === 'questionnaire'}
      onclick={() => (mode = 'questionnaire')}><ListChecks size={17} /> Questionnaires</button
    >
    <span><MousePointer2 size={13} /> Try the example</span>
  </div>
  <div class="app-window">
    <div class="app-heading">
      <span class="file-icon"
        >{#if mode === 'spreadsheet'}<Table2
            size={25}
          />{:else if mode === 'questionnaire'}<ListChecks size={25} />{:else}<FileText
            size={25}
          />{/if}</span
      >
      <div>
        <strong
          >{mode === 'document'
            ? 'Project launch · Working proposal'
            : mode === 'spreadsheet'
              ? 'Project launch · Delivery tracker'
              : 'Community voices · Survey'}</strong
        >
        <div class="app-meta">
          <span>Shared team workspace</span><span class="private"
            ><LockKeyhole size={10} /> Personal draft</span
          >
        </div>
      </div>
      <button
        disabled={!ready}
        class="history"
        aria-label="Show example version history"
        aria-pressed={showHistory}
        onclick={() => (showHistory = !showHistory)}
        ><History size={17} /><span>History</span></button
      >
      <span class="avatar" aria-label="Example collaborator Maya">M</span>
    </div>
    <div class="toolbar" aria-hidden="true">
      <Undo2 size={14} /><Redo2 size={14} /><i></i><span>100% <ChevronDown size={11} /></span><i
      ></i><span
        >{mode === 'spreadsheet' ? 'Currency' : 'Normal text'} <ChevronDown size={11} /></span
      ><i></i><Bold size={15} /><Italic size={15} /><AlignLeft size={16} /><List size={17} /><i
      ></i><MessageSquare size={15} />
    </div>
    <div class="canvas">
      {#if showHistory}
        <div class="history-panel" role="status">
          <span class="eyebrow">Example version history</span><strong
            >The story behind the current draft</strong
          >
          <div>
            <span class="history-dot"></span>
            <p>
              <b
                >{wording ? 'Proposal wording refined' : 'Working draft'}<small
                  >Maya · Your current work</small
                ></b
              >
            </p>
          </div>
          <div>
            <span class="history-dot older"></span>
            <p><b>Team review saved<small>Sam · Shared version</small></b></p>
          </div>
          <div>
            <span class="history-dot older"></span>
            <p><b>Original document imported<small>Maya · Source file retained</small></b></p>
          </div>
          <button disabled={!ready} onclick={() => (showHistory = false)}
            >Back to the example <ArrowUpRight size={14} /></button
          >
        </div>
      {:else if mode === 'document'}
        <div class="document-scene">
          <div class="paper">
            <span class="paper-kicker">PROJECT PROPOSAL</span>
            <h2>A clear plan. A shared starting point.</h2>
            <p class="paper-intro">
              A working proposal that keeps decisions, ownership and revisions together.
            </p>
            <div class="paper-rule"></div>
            <h3>Delivery and review</h3>
            <p>
              Give each stage an owner and a review date. Keep a record of why the plan changes.
            </p>
            <div class="selected-paragraph" class:changed={wording}>
              <span class="block-mark"></span>{wording
                ? 'Deliver the first phase in September, following two weeks of review.'
                : 'Deliver the complete project by the end of August.'}
            </div>
            <p class="prompt">
              <b>Before approval</b><br />Confirm the budget, accessibility review and delivery
              owners.
            </p>
            <div class="paper-lines" aria-hidden="true"><span></span><span></span></div>
          </div>
          <div class="comment">
            <div class="comment-top">
              <span class="comment-avatar">S</span>
              <div><b>Sam</b><small>Reviewing the proposal</small></div>
              <MessageSquare size={14} />
            </div>
            <p>
              Could we allow time for review? A phased delivery would make this commitment more
              realistic.
            </p>
            <button disabled={!ready} onclick={() => (wording = !wording)}
              >{#if wording}<Check size={15} /> Suggestion applied{:else}Try this wording <ArrowUpRight
                  size={14}
                />{/if}</button
            ><span class="comment-note"
              >{wording
                ? 'Click again to compare the original.'
                : 'See how a small edit changes the commitment.'}</span
            >
          </div>
        </div>
      {:else if mode === 'spreadsheet'}
        <div class="sheet-scene">
          <div class="formula">
            <span>B5</span><i>ƒx</i><code>=SUM(B2:B4)</code><span class="formula-hint"
              >Change a value below</span
            >
          </div>
          <div class="sheet-scroll">
            <table aria-label="Interactive delivery tracker">
              <thead><tr><th></th><th>A</th><th>B</th><th>C</th><th>D</th></tr></thead><tbody
                ><tr
                  ><th>1</th><td class="table-heading">Workstream</td><td class="table-heading"
                    >Tasks</td
                  ><td class="table-heading">Team lead</td><td class="table-heading">Status</td></tr
                ><tr
                  ><th>2</th><td>Design</td><td
                    ><input
                      disabled={!ready}
                      type="number"
                      min="0"
                      max="999"
                      aria-label="Design tasks"
                      value={north}
                      oninput={(e) =>
                        (north = Math.max(0, Math.min(999, Number(e.currentTarget.value) || 0)))}
                    /></td
                  ><td>Maya</td><td><span class="status-label">In progress</span></td></tr
                ><tr
                  ><th>3</th><td>Development</td><td
                    ><input
                      disabled={!ready}
                      type="number"
                      min="0"
                      max="999"
                      aria-label="Development tasks"
                      value={south}
                      oninput={(e) =>
                        (south = Math.max(0, Math.min(999, Number(e.currentTarget.value) || 0)))}
                    /></td
                  ><td>Sam</td><td><span class="status-label">In progress</span></td></tr
                ><tr
                  ><th>4</th><td>Review</td><td
                    ><input
                      disabled={!ready}
                      type="number"
                      min="0"
                      max="999"
                      aria-label="Review tasks"
                      value={east}
                      oninput={(e) =>
                        (east = Math.max(0, Math.min(999, Number(e.currentTarget.value) || 0)))}
                    /></td
                  ><td>Rafi</td><td><span class="status-label complete">Complete</span></td></tr
                ><tr
                  ><th>5</th><td class="total-label">Total tasks</td><td
                    class="total"
                    aria-live="polite">{total}</td
                  ><td></td><td></td></tr
                >{#each [6, 7, 8] as row}<tr class="empty-row"
                    ><th>{row}</th><td></td><td></td><td></td><td></td></tr
                  >{/each}</tbody
              >
            </table>
          </div>
          <div class="sheet-tabs">
            <span>+</span><strong>Recruitment <ChevronDown size={12} /></strong>
          </div>
          <p class="sheet-callout">
            <Table2 size={19} /><span
              ><b>Your formulas, with a history.</b> Keep track of how the dataset took shape, alongside
              the values.</span
            >
          </p>
        </div>
      {:else}
        <div class="survey-scene">
          <div class="survey">
            <span class="paper-kicker">PARTICIPANT EXPERIENCE</span>
            <h2>Make every question count.</h2>
            <p>How would you prefer to take part?</p>
            <fieldset>
              <legend class="sr-only">Preferred participation format</legend
              >{#each [{ value: 'interview', label: 'An individual interview' }, { value: 'group', label: 'A group conversation' }, { value: 'written', label: 'A written response' }] as option}<label
                  class:selected={selection === option.value}
                  ><input
                    disabled={!ready}
                    type="radio"
                    name="participation"
                    value={option.value}
                    bind:group={selection}
                  />{option.label}</label
                >{/each}
            </fieldset>
            <span class="survey-foot"
              ><Check size={13} /> Example response · nothing is submitted</span
            >
          </div>
          <div class="survey-note">
            <ListChecks size={26} />
            <h3>The wording is part of the method.</h3>
            <p>
              Review question text, choices and notes together. Keep a record of what changed before
              the next round of fieldwork.
            </p>
          </div>
        </div>
      {/if}
    </div>
    <div class="app-status">
      <span><span class="status-dot"></span>Interactive example · edits stay on this page</span
      ><button disabled={!ready} aria-pressed={reviewed} onclick={() => (reviewed = !reviewed)}
        >{reviewed ? 'Marked as reviewed' : 'Mark as reviewed'} <Check size={13} /></button
      >
    </div>
  </div>
  <div class="explorer-caption">
    <p>
      {mode === 'document'
        ? 'Write, format, comment. Keep the conversation beside the paragraph.'
        : mode === 'spreadsheet'
          ? 'Work with cells, formulas and multiple sheets. Keep the decisions beside the data.'
          : 'Shape questions, choices and survey structure in one shared workspace.'}
    </p>
    <span>Familiar tools. A clearer record.</span>
  </div>
</div>

<style>
  .explorer {
    --editor: var(--color-word);
    --tint: var(--color-word-bg);
    width: 100%;
    text-align: left;
  }
  .explorer.sheet {
    --editor: var(--color-excel);
    --tint: var(--color-excel-bg);
  }
  .switcher {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-bottom: 18px;
    flex-wrap: wrap;
  }
  .switcher > button {
    display: flex;
    align-items: center;
    gap: 8px;
    border: 1px solid transparent;
    background: transparent;
    padding: 10px 17px;
    border-radius: 5px;
    font-size: 13px;
    color: var(--color-ink-soft);
    cursor: pointer;
  }
  .switcher > button[aria-pressed='true'] {
    background: var(--color-surface-raised);
    border-color: var(--color-border-strong);
    color: var(--editor);
    box-shadow: var(--shadow-panel);
    font-weight: 650;
  }
  .switcher > span {
    display: flex;
    align-items: center;
    gap: 7px;
    margin-left: auto;
    font-size: 11px;
    color: var(--color-ink-soft);
  }
  .app-window {
    background: var(--color-surface-raised);
    border: 1px solid var(--color-border-strong);
    border-radius: 9px;
    overflow: hidden;
    box-shadow: var(--shadow-dialog);
  }
  .app-heading {
    display: flex;
    align-items: center;
    gap: 13px;
    padding: 20px 24px;
    background: var(--color-surface-raised);
  }
  .file-icon {
    color: var(--editor);
  }
  .app-heading strong {
    font-size: 14px;
    line-height: 1.5;
    display: block;
    font-weight: 600;
    color: var(--color-ink);
  }
  .app-meta {
    display: flex;
    gap: 18px;
    align-items: center;
    color: var(--color-ink-soft);
    font-size: 10px;
    margin-top: 4px;
  }
  .private {
    display: flex;
    gap: 5px;
    align-items: center;
  }
  .history {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 7px;
    background: none;
    border: 0;
    color: var(--color-ink-soft);
    font-size: 11px;
    cursor: pointer;
    padding: 8px;
    border-radius: 4px;
  }
  .history:hover,
  .history[aria-pressed='true'] {
    background: var(--tint);
  }
  .avatar {
    width: 29px;
    height: 29px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    background: var(--color-accent-bg);
    color: var(--color-accent);
    font-size: 11px;
    font-weight: 600;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 21px;
    margin: 0 12px;
    background: var(--tint);
    border-radius: 5px;
    padding: 12px 18px;
    color: var(--color-ink-soft);
  }
  .toolbar i {
    width: 1px;
    height: 16px;
    background: var(--color-border);
  }
  .toolbar > span {
    font-size: 10px;
    display: flex;
    gap: 20px;
    align-items: center;
  }
  .canvas {
    min-height: 440px;
    background: var(--color-canvas);
  }
  .document-scene {
    padding: 30px 40px 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 230px;
    gap: 22px;
    min-height: 440px;
  }
  .paper {
    padding: 35px 44px 25px;
    background: var(--color-page);
    border: 1px solid var(--color-page-border);
    box-shadow: var(--shadow-panel);
    align-self: stretch;
  }
  .paper-kicker {
    font-size: 9px;
    letter-spacing: 1.4px;
    color: var(--color-ink-soft);
    font-weight: 600;
  }
  .paper h2,
  .survey h2 {
    font-size: 24px;
    letter-spacing: -0.8px;
    font-weight: 600;
    margin: 16px 0 12px;
    color: var(--color-ink-strong);
  }
  .paper p {
    font-size: 11px;
    line-height: 1.85;
    color: var(--color-ink-soft);
  }
  .paper .paper-intro {
    max-width: 340px;
  }
  .paper-rule {
    height: 1px;
    background: var(--color-border-soft);
    margin: 23px 0;
  }
  .paper h3 {
    font-size: 13px;
    margin: 0 0 9px;
    font-weight: 650;
  }
  .selected-paragraph {
    position: relative;
    background: var(--color-accent-bg);
    border: 1px solid var(--color-accent-border);
    padding: 12px 16px;
    font-size: 12px;
    line-height: 1.7;
    color: var(--color-accent-strong);
    margin: 17px -16px;
  }
  .selected-paragraph.changed {
    background: var(--color-success-bg);
    border-color: var(--color-success-border);
    color: var(--color-diff-add-ink);
  }
  .block-mark {
    position: absolute;
    width: 3px;
    inset: 0 auto 0 0;
    background: var(--editor);
  }
  .changed .block-mark {
    background: var(--color-success);
  }
  .prompt b {
    font-weight: 600;
  }
  .paper-lines span {
    display: block;
    height: 5px;
    background: var(--color-border-soft);
    margin: 10px 0;
    width: 95%;
  }
  .paper-lines span:last-child {
    width: 65%;
  }
  .comment {
    align-self: start;
    background: var(--color-surface-raised);
    padding: 17px;
    border: 1px solid var(--color-border);
    border-left: 3px solid var(--color-accent);
    margin-top: 122px;
    border-radius: 4px;
    box-shadow: var(--shadow-panel);
  }
  .comment-top {
    display: flex;
    gap: 9px;
    align-items: center;
  }
  .comment-avatar {
    display: grid;
    place-items: center;
    width: 27px;
    height: 27px;
    border-radius: 50%;
    font-size: 11px;
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
  }
  .comment-top b {
    display: block;
    font-size: 11px;
    font-weight: 600;
  }
  .comment-top small {
    display: block;
    font-size: 8px;
    color: var(--color-ink-soft);
  }
  .comment-top > :global(svg) {
    margin-left: auto;
    color: var(--color-accent);
  }
  .comment p {
    font-size: 11px;
    line-height: 1.8;
    color: var(--color-ink-soft);
  }
  .comment button {
    display: flex;
    gap: 7px;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--color-accent-border);
    background: var(--color-accent-bg);
    color: var(--color-accent);
    width: 100%;
    font-size: 10px;
    font-weight: 600;
    padding: 10px 4px;
    border-radius: 4px;
    cursor: pointer;
  }
  .comment-note {
    display: block;
    font-size: 9px;
    line-height: 1.6;
    color: var(--color-ink-soft);
    margin-top: 10px;
  }
  .app-status {
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: var(--color-surface-raised);
    padding: 11px 20px;
    border-top: 1px solid var(--color-border);
    font-size: 9px;
    color: var(--color-ink-soft);
    gap: 12px;
  }
  .app-status > span {
    display: flex;
    gap: 7px;
    align-items: center;
  }
  .status-dot {
    background: var(--color-success);
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex-shrink: 0;
  }
  .app-status > button {
    display: flex;
    gap: 7px;
    align-items: center;
    background: none;
    border: 0;
    font-size: 10px;
    color: var(--editor);
    cursor: pointer;
    text-align: right;
  }
  .explorer-caption {
    display: flex;
    justify-content: space-between;
    gap: 24px;
    font-size: 11px;
    color: var(--color-ink-soft);
    padding-top: 16px;
  }
  .explorer-caption p {
    margin: 0;
  }
  .explorer-caption > span {
    white-space: nowrap;
    color: var(--color-accent);
    font-weight: 550;
  }
  .formula {
    display: flex;
    gap: 18px;
    align-items: center;
    background: var(--color-surface-raised);
    font-size: 12px;
    border-bottom: 1px solid var(--color-border);
    height: 40px;
  }
  .formula > span:first-child {
    padding: 5px 35px;
    border-right: 1px solid var(--color-border);
  }
  .formula i {
    color: var(--color-ink-soft);
    font-size: 16px;
  }
  .formula code {
    font-size: 11px;
    color: var(--color-excel);
  }
  .formula-hint {
    margin-left: auto;
    margin-right: 24px;
    color: var(--color-ink-soft);
    font-size: 10px;
  }
  .sheet-scroll {
    overflow: auto;
  }
  .sheet-scene table {
    border-collapse: collapse;
    width: 100%;
    min-width: 580px;
    background: var(--color-grid-bg);
    font-size: 11px;
  }
  .sheet-scene th {
    background: var(--color-grid-header-bg);
    font-size: 10px;
    font-weight: 400;
    color: var(--color-grid-header-ink);
    height: 26px;
    text-align: center;
    border: 1px solid var(--color-grid-line);
  }
  .sheet-scene td {
    border: 1px solid var(--color-grid-line);
    height: 39px;
    padding: 7px 17px;
    color: var(--color-grid-ink);
    width: 23%;
  }
  .sheet-scene td.table-heading {
    background: var(--color-excel-bg);
    color: var(--color-diff-add-ink);
    font-weight: 650;
  }
  .sheet-scene td:first-of-type {
    width: 27%;
  }
  .sheet-scene input {
    width: 100%;
    min-width: 65px;
    background: none;
    border: 0;
    font-size: 12px;
    color: var(--color-excel);
    padding: 2px;
    text-align: left;
    border-bottom: 1px dashed color-mix(in srgb, var(--color-excel) 50%, transparent);
    border-radius: 0;
  }
  .status-label {
    background: var(--color-warn-bg);
    font-size: 9px;
    border-radius: 3px;
    padding: 4px 8px;
    color: var(--color-warn-ink);
    white-space: nowrap;
  }
  .status-label.complete {
    background: var(--color-success-bg);
    color: var(--color-success);
  }
  .sheet-scene .total {
    outline: 2px solid var(--color-excel);
    outline-offset: -2px;
    font-weight: 700;
    color: var(--color-excel);
    background: var(--color-success-bg);
    font-size: 13px;
  }
  .total-label {
    font-weight: 600;
  }
  .sheet-scene .empty-row td {
    height: 30px;
  }
  .sheet-tabs {
    display: flex;
    gap: 30px;
    font-size: 11px;
    align-items: center;
    border-top: 1px solid var(--color-border);
    padding: 0 25px;
    background: var(--color-grid-header-bg);
    height: 38px;
    color: var(--color-ink-soft);
  }
  .sheet-tabs strong {
    display: flex;
    align-items: center;
    gap: 18px;
    height: 38px;
    padding: 0 18px;
    background: var(--color-excel-bg);
    color: var(--color-diff-add-ink);
    border-bottom: 2px solid var(--color-excel);
    font-weight: 500;
  }
  .sheet-callout {
    display: flex;
    gap: 14px;
    align-items: center;
    padding: 15px 28px;
    font-size: 10px;
    margin: 0;
    line-height: 1.8;
    color: var(--color-ink-soft);
  }
  .sheet-callout b {
    display: block;
    font-weight: 600;
  }
  .survey-scene {
    display: grid;
    grid-template-columns: 1.4fr 1fr;
    gap: 50px;
    align-items: center;
    min-height: 440px;
    padding: 38px 55px;
  }
  .survey {
    padding: 28px;
    background: var(--color-page);
    border: 1px solid var(--color-page-border);
    border-radius: 4px;
  }
  .survey h2 {
    font-size: 22px;
  }
  .survey p {
    font-size: 12px;
    color: var(--color-ink);
    margin: 23px 0 15px;
  }
  .survey fieldset {
    padding: 0;
    border: 0;
    display: grid;
    gap: 10px;
  }
  .survey label {
    display: flex;
    align-items: center;
    gap: 10px;
    border: 1px solid var(--color-border);
    padding: 11px 10px;
    border-radius: 4px;
    font-size: 11px;
    color: var(--color-ink-soft);
    cursor: pointer;
  }
  .survey label.selected {
    background: var(--color-accent-bg);
    border-color: color-mix(in srgb, var(--color-accent) 50%, transparent);
    color: var(--color-word);
  }
  .survey input {
    accent-color: var(--color-accent);
  }
  .survey-foot {
    display: flex;
    gap: 6px;
    align-items: center;
    margin-top: 20px;
    font-size: 8px;
    color: var(--color-ink-soft);
  }
  .survey-note {
    color: var(--color-accent-strong);
  }
  .survey-note h3 {
    font-size: 22px;
    line-height: 1.3;
    font-weight: 550;
    letter-spacing: -0.6px;
    max-width: 230px;
  }
  .survey-note p {
    font-size: 12px;
    line-height: 1.9;
    color: var(--color-ink-soft);
    max-width: 250px;
  }
  .history-panel {
    padding: 40px;
    max-width: 520px;
    margin: auto;
    min-height: 440px;
  }
  .eyebrow {
    display: block;
    color: var(--color-ink-soft);
    font-size: 10px;
    letter-spacing: 0.5px;
    margin-bottom: 10px;
  }
  .history-panel > strong {
    font-size: 21px;
    font-weight: 550;
    letter-spacing: -0.5px;
    display: block;
    margin-bottom: 24px;
  }
  .history-panel > div {
    display: flex;
    align-items: center;
    gap: 20px;
    position: relative;
  }
  .history-dot {
    width: 11px;
    height: 11px;
    background: var(--color-accent);
    outline: 4px solid var(--color-accent-bg);
    border-radius: 50%;
    flex-shrink: 0;
  }
  .history-dot.older {
    background: var(--color-surface-raised);
    border: 2px solid var(--color-border-strong);
  }
  .history-panel p {
    font-size: 12px;
  }
  .history-panel b {
    font-weight: 500;
  }
  .history-panel small {
    display: block;
    font-size: 10px;
    color: var(--color-ink-soft);
    font-weight: 400;
    margin-top: 3px;
  }
  .history-panel button {
    background: none;
    border: 0;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 15px 0;
    color: var(--color-accent);
    font-size: 11px;
    cursor: pointer;
  }
  @media (prefers-reduced-motion: no-preference) {
    .paper,
    .comment,
    .survey,
    .survey-note,
    .history-panel,
    .sheet-scene {
      animation: appear 0.35s ease-out;
    }
    .selected-paragraph {
      transition: background 0.25s;
    }
    .comment {
      animation-delay: 0.1s;
      animation-fill-mode: backwards;
    }
    @keyframes appear {
      from {
        opacity: 0;
        transform: translateY(8px);
      }
      to {
        opacity: 1;
        transform: translateY(0);
      }
    }
  }
  @media (max-width: 900px) {
    .document-scene {
      padding: 24px 24px 0;
      gap: 16px;
      grid-template-columns: minmax(0, 1fr) 190px;
    }
    .paper {
      padding: 28px 30px 18px;
    }
    .paper h2 {
      font-size: 22px;
    }
    .comment {
      padding: 13px;
    }
    .toolbar {
      gap: 17px;
    }
    .survey-scene {
      padding: 30px;
      gap: 28px;
    }
    .explorer-caption > span {
      display: none;
    }
  }
  @media (max-width: 600px) {
    .switcher {
      gap: 2px;
      margin-bottom: 12px;
    }
    .switcher > button {
      font-size: 10px;
      gap: 5px;
      padding: 9px 8px;
    }
    .switcher :global(svg) {
      width: 14px;
    }
    .switcher > span {
      width: 100%;
      margin: 7px 0 0;
      font-size: 10px;
    }
    .app-heading {
      padding: 15px 12px;
      gap: 9px;
    }
    .app-heading strong {
      font-size: 10px;
    }
    .app-meta {
      font-size: 8px;
      gap: 8px;
    }
    .app-meta > span:first-child {
      display: none;
    }
    .avatar {
      display: none;
    }
    .history {
      padding: 5px;
    }
    .history span {
      display: none;
    }
    .toolbar {
      padding: 10px;
      gap: 12px;
      margin: 0 6px;
    }
    .toolbar > span {
      font-size: 9px;
      gap: 5px;
    }
    .toolbar > :global(svg):last-child,
    .toolbar i:last-of-type {
      display: none;
    }
    .document-scene {
      display: flex;
      flex-direction: column;
      padding: 16px 16px 0;
      gap: 12px;
    }
    .paper {
      padding: 24px;
    }
    .paper h2 {
      font-size: 23px;
      line-height: 1.2;
    }
    .paper p {
      font-size: 11px;
    }
    .paper-rule {
      margin: 15px 0;
    }
    .selected-paragraph {
      font-size: 11px;
    }
    .paper-lines {
      display: none;
    }
    .comment {
      margin: 0 0 15px;
      padding: 12px;
    }
    .comment p {
      margin: 9px 0;
    }
    .comment-top small {
      font-size: 9px;
    }
    .comment button {
      width: auto;
      padding: 8px 13px;
    }
    .comment-note {
      font-size: 9px;
    }
    .app-status {
      padding: 10px;
      align-items: flex-start;
      font-size: 8px;
    }
    .app-status > span {
      max-width: 160px;
      line-height: 1.6;
    }
    .app-status > button {
      font-size: 9px;
      max-width: 120px;
      padding: 0;
    }
    .explorer-caption {
      font-size: 10px;
      padding-top: 12px;
    }
    .formula-hint {
      display: none;
    }
    .formula > span:first-child {
      padding: 5px 20px;
    }
    .sheet-callout {
      padding: 20px 18px;
    }
    .survey-scene {
      padding: 20px;
      display: flex;
      flex-direction: column;
      gap: 20px;
    }
    .survey {
      padding: 22px;
      width: 100%;
    }
    .survey-note {
      display: none;
    }
    .history-panel {
      padding: 28px 24px;
    }
    .history-panel > strong {
      font-size: 21px;
    }
  }
</style>
