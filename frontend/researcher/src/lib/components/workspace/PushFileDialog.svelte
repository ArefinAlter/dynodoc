<script lang="ts">
  import { onMount } from 'svelte';
  import { FileUp, X, AlertCircle, GitPullRequest, GitBranch, LoaderCircle } from '@lucide/svelte';
  import DiffText from './DiffText.svelte';
  import {
    PUSH_FILES,
    planAgainst,
    preparePush,
    sendPush,
    type BaseCandidate,
    type PushKind,
    type PushPreparation
  } from '$lib/workspace/reviews';
  import { percent } from '$lib/workspace/collaboration';
  import { titleFromName } from '$lib/workspace/file-push';
  let {
    documentId,
    title,
    kind = 'document',
    initialFile = null,
    onClose,
    onCreated
  }: {
    documentId: string;
    title: string;
    kind?: PushKind;
    initialFile?: File | null;
    onClose: () => void;
    onCreated: (result: { id: string; submitted: boolean }) => void;
  } = $props();
  let prepared = $state<PushPreparation | null>(null),
    busy = $state(''),
    error = $state(''),
    name = $state(''),
    note = $state(''),
    keepFile = $state(true),
    dragging = $state(false),
    baseSeq = $state(0);
  let input = $state<HTMLInputElement | null>(null);
  const counts = $derived.by(() => {
    const c = { added: 0, edited: 0, removed: 0, moved: 0 };
    for (const change of prepared?.plan.changes || []) c[change.kind]++;
    return c;
  });
  onMount(() => {
    if (initialFile) void choose(initialFile);
  });
  async function choose(file: File | undefined) {
    if (!file) return;
    busy = 'Comparing your file with the version it started from…';
    error = '';
    prepared = null;
    try {
      prepared = await preparePush(documentId, title, file, kind);
      baseSeq = prepared.chosen.seq;
      name = `Changes from ${titleFromName(file.name) || 'my copy'}`.slice(0, 200);
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not read this file.';
    } finally {
      busy = '';
      if (input) input.value = '';
    }
  }
  async function changeBase(seq: number) {
    if (!prepared) return;
    const candidate = prepared.candidates.find((c) => c.seq === seq);
    if (!candidate) return;
    busy = 'Comparing with that version…';
    error = '';
    try {
      const { state, plan } = await planAgainst(documentId, title, prepared.incoming, seq);
      prepared = { ...prepared, chosen: candidate, plan, base: state };
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not compare with that version.';
    } finally {
      busy = '';
    }
  }
  async function send(submit: boolean) {
    if (!prepared || !name.trim()) return;
    busy = submit ? 'Sending your changes for review…' : 'Creating your personal draft…';
    error = '';
    try {
      const result = await sendPush(documentId, prepared, {
        name: name.trim(),
        note: note.trim(),
        submit,
        keepFile
      });
      onCreated({ id: result.id, submitted: submit });
    } catch (e) {
      error = e instanceof Error ? e.message : 'Could not send these changes.';
    } finally {
      busy = '';
    }
  }
  const files = $derived(PUSH_FILES[kind]);
  function describe(candidate: BaseCandidate) {
    return candidate.score === undefined
      ? candidate.label
      : kind === 'document'
        ? `${candidate.label} · ${candidate.score} matching paragraphs`
        : `${candidate.label} · ${percent(candidate.score)} of ${files.unit} match`;
  }
</script>

<div class="shade" role="presentation">
  <div class="dialog" role="dialog" aria-modal="true" aria-labelledby="push-title">
    <header>
      <span class="icon"><FileUp size={20} /></span>
      <div>
        <h2 id="push-title">Upload your edited copy</h2>
        <p>
          Edited the {files.name} file on your computer? Dynodoc compares it with the version you started
          from and turns your edits into changes your team can review. Nothing in the team version changes
          until someone merges them.
        </p>
      </div>
      <button class="close" aria-label="Close" disabled={!!busy} onclick={onClose}
        ><X size={18} /></button
      >
    </header>
    {#if !prepared}
      <label
        class="drop"
        class:dragging
        ondragover={(e) => {
          e.preventDefault();
          dragging = true;
        }}
        ondragleave={() => (dragging = false)}
        ondrop={(e) => {
          e.preventDefault();
          dragging = false;
          void choose(e.dataTransfer?.files[0]);
        }}
      >
        <FileUp size={28} strokeWidth={1.3} />
        <strong>Choose your edited {files.name} file</strong>
        <span
          >or drop it here · {files.accept.replaceAll(',', ' or ')}, up to {kind === 'document'
            ? 10
            : 20} MB</span
        >
        <input
          bind:this={input}
          type="file"
          accept={files.accept}
          disabled={!!busy}
          onchange={(e) => void choose(e.currentTarget.files?.[0])}
        />
      </label>
      <p class="hint">
        Tip: files downloaded from Dynodoc remember which version they came from, and are named like
        “{title} (v12){files.accept.split(',')[0]}”. Files from elsewhere are matched by their
        content, even when renamed.
      </p>
    {:else}
      <section class="summary" aria-live="polite">
        <label
          >This copy started from
          <select
            value={baseSeq}
            disabled={!!busy}
            onchange={(e) => {
              baseSeq = Number(e.currentTarget.value);
              void changeBase(baseSeq);
            }}
            >{#each prepared.candidates as candidate}<option value={candidate.seq}
                >{describe(candidate)}</option
              >{/each}</select
          ></label
        >
        <p class="how">
          {prepared.plan.anchored
            ? 'Paragraphs were matched exactly using hidden markers saved in your download.'
            : kind === 'document'
              ? 'Paragraphs were matched by their wording.'
              : `${files.unit[0].toUpperCase()}${files.unit.slice(1)} were matched by their content.`}
          {prepared.chosen.reason === 'file'
            ? ' The version was read from the file itself.'
            : prepared.chosen.reason === 'name'
              ? ' The version was read from the file name.'
              : ' Change it above if you started from a different version.'}
        </p>
        <div class="chips">
          <span class="chip edited">{counts.edited} edited</span><span class="chip added"
            >{counts.added} added</span
          ><span class="chip removed">{counts.removed} removed</span>{#if counts.moved}<span
              class="chip">{counts.moved} moved</span
            >{/if}<span class="chip">{prepared.plan.unchanged} unchanged</span>
        </div>
        {#if prepared.relation?.unrelated}<p class="warning">
            <AlertCircle size={15} />This file shares only {percent(prepared.relation.score)} of its content
            with “{title}”. It may be a different file with the same name. If you send it, the
            person who merges must choose, for every existing {kind === 'document'
              ? 'paragraph'
              : kind === 'spreadsheet'
                ? 'row'
                : 'text box'} it changes or removes, between the team's version and yours.
          </p>{/if}
        {#if prepared.otherDocument}<p class="warning">
            <AlertCircle size={15} />This file was downloaded from a different Dynodoc document.
            Check that you chose the right file.
          </p>{/if}
        {#each prepared.plan.warnings as warning}<p class="warning">
            <AlertCircle size={15} />{warning}
          </p>{/each}
      </section>
      {#if prepared.plan.changes.length}
        <ol class="changes" aria-label="Changes in your file">
          {#each prepared.plan.changes.slice(0, 300) as change}<li class={change.kind}>
              <span class="badge"
                >{change.kind === 'edited' && change.formatting ? 'Formatting' : change.kind}</span
              >
              {#if change.kind === 'edited' || change.kind === 'moved'}<DiffText
                  before={change.before}
                  after={change.after}
                />{:else if change.kind === 'added'}<p class="text" dir="auto">
                  {change.after || 'Empty paragraph'}
                </p>{:else}<p class="text gone" dir="auto">
                  {change.before || 'Empty paragraph'}
                </p>{/if}
            </li>{/each}
          {#if prepared.plan.changes.length > 300}<li class="more">
              …and {prepared.plan.changes.length - 300} more
            </li>{/if}
        </ol>
        <form
          onsubmit={(e) => {
            e.preventDefault();
            void send(true);
          }}
        >
          <label>Name these changes<input bind:value={name} maxlength="200" required /></label>
          <label
            >What did you change and why? <small>(optional)</small><textarea
              bind:value={note}
              rows="2"
              maxlength="4000"
              placeholder="e.g. Updated the method section after our meeting"
            ></textarea></label
          >
          <label class="check"
            ><input type="checkbox" bind:checked={keepFile} /> Keep a copy of my file with these changes</label
          >
          <div class="actions">
            <button
              type="button"
              class="secondary"
              disabled={!!busy}
              onclick={() => void send(false)}
              ><GitBranch size={15} /> Open as my personal draft</button
            ><button class="primary" disabled={!!busy || !name.trim()}
              ><GitPullRequest size={15} /> Send for review</button
            >
          </div>
        </form>
      {:else}
        <p class="nothing">
          No differences found — this file matches the version it started from. There is nothing to
          upload.
        </p>
      {/if}
      <button class="text-button" disabled={!!busy} onclick={() => (prepared = null)}
        >Choose a different file</button
      >
    {/if}
    {#if busy}<p class="busy" role="status"><LoaderCircle size={15} />{busy}</p>{/if}
    {#if error}<p class="error" role="alert"><AlertCircle size={15} />{error}</p>{/if}
  </div>
</div>

<style>
  .shade {
    position: fixed;
    inset: 0;
    background: var(--color-overlay);
    -webkit-backdrop-filter: blur(4px);
    backdrop-filter: blur(4px);
    z-index: 100;
    display: grid;
    place-items: center;
    padding: 24px;
  }
  .dialog {
    background: var(--color-surface-raised);
    width: 760px;
    max-width: 100%;
    max-height: 92vh;
    overflow: auto;
    padding: 26px 28px;
    border-radius: 12px;
    box-shadow: var(--shadow-dialog);
    font-family: var(--font-ui);
    color: var(--color-ink);
  }
  header {
    display: flex;
    gap: 14px;
    align-items: start;
  }
  header h2 {
    margin: 2px 0 6px;
    font-size: 19px;
    font-weight: 600;
    color: var(--color-ink-strong);
  }
  header p,
  .hint,
  .how {
    margin: 0;
    font-size: 13px;
    line-height: 1.65;
    color: var(--color-ink-soft);
  }
  .icon {
    display: grid;
    place-items: center;
    flex-shrink: 0;
    width: 40px;
    height: 40px;
    border-radius: 10px;
    background: var(--color-accent-bg);
    color: var(--color-accent);
  }
  .close {
    margin-left: auto;
    border: 0;
    background: none;
    color: var(--color-ink-soft);
    cursor: pointer;
    padding: 4px;
  }
  .drop {
    display: grid;
    justify-items: center;
    gap: 6px;
    margin: 22px 0 12px;
    padding: 34px 20px;
    border: 2px dashed var(--color-accent-border);
    border-radius: 12px;
    background: var(--color-surface-sunken);
    color: var(--color-accent);
    cursor: pointer;
    text-align: center;
  }
  .drop.dragging,
  .drop:hover,
  .drop:focus-within {
    border-color: var(--color-accent);
    background: var(--color-surface-selected);
  }
  .drop span {
    font-size: 12px;
    color: var(--color-ink-soft);
  }
  .drop input {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
  }
  .summary {
    margin: 20px 0 12px;
    display: grid;
    gap: 10px;
  }
  .summary > label,
  form label {
    display: grid;
    gap: 6px;
    font-size: 13px;
    color: var(--color-ink);
  }
  select,
  input:not([type='checkbox']),
  textarea {
    font: inherit;
    font-size: 13px;
    padding: 8px 10px;
    border: 1px solid var(--color-border-strong);
    border-radius: 8px;
    color: var(--color-ink-strong);
    background: var(--color-surface-raised);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 7px;
  }
  .chip {
    font-size: 12px;
    padding: 3px 9px;
    border-radius: 12px;
    background: var(--color-surface-sunken);
    color: var(--color-ink);
  }
  .chip.edited {
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
  }
  .chip.added {
    background: var(--color-diff-add-bg);
    color: var(--color-diff-add-ink);
  }
  .chip.removed {
    background: var(--color-diff-del-bg);
    color: var(--color-diff-del-ink);
  }
  .warning,
  .error,
  .busy {
    display: flex;
    gap: 8px;
    align-items: start;
    margin: 0;
    font-size: 13px;
    line-height: 1.55;
  }
  .warning {
    color: var(--color-warn-ink);
  }
  .error {
    color: var(--color-danger);
    margin-top: 12px;
  }
  .busy {
    color: var(--color-accent);
    margin-top: 12px;
  }
  .busy :global(svg) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .busy :global(svg) {
      animation: none;
    }
  }
  .changes {
    list-style: none;
    margin: 6px 0 16px;
    padding: 0;
    max-height: 300px;
    overflow: auto;
    border: 1px solid var(--color-border);
    border-radius: 10px;
  }
  .changes li {
    display: grid;
    grid-template-columns: 86px 1fr;
    gap: 10px;
    padding: 10px 12px;
    border-bottom: 1px solid var(--color-border-soft);
  }
  .changes li:last-child {
    border-bottom: 0;
  }
  .badge {
    align-self: start;
    font-size: 11px;
    text-transform: capitalize;
    padding: 2px 7px;
    border-radius: 10px;
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
    text-align: center;
  }
  .added .badge {
    background: var(--color-diff-add-bg);
    color: var(--color-diff-add-ink);
  }
  .removed .badge {
    background: var(--color-diff-del-bg);
    color: var(--color-diff-del-ink);
  }
  .text {
    margin: 0;
    font-size: 13px;
    line-height: 1.7;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }
  .gone {
    text-decoration: line-through;
    color: var(--color-diff-del-ink);
  }
  .more {
    display: block !important;
    font-size: 12px;
    color: var(--color-ink-soft);
  }
  form {
    display: grid;
    gap: 12px;
  }
  small {
    color: var(--color-muted);
  }
  .check {
    display: flex !important;
    align-items: center;
  }
  .actions {
    display: flex;
    justify-content: end;
    gap: 10px;
    flex-wrap: wrap;
  }
  .primary,
  .secondary {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    border-radius: 8px;
    padding: 9px 14px;
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .primary {
    border: 0;
    background: var(--color-accent-solid);
    color: var(--color-on-accent);
  }
  .secondary {
    border: 1px solid var(--color-border-strong);
    background: var(--color-surface-raised);
    color: var(--color-accent-strong);
  }
  button:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .text-button {
    border: 0;
    background: none;
    color: var(--color-ink-soft);
    text-decoration: underline;
    text-underline-offset: 3px;
    font-size: 13px;
    cursor: pointer;
    padding: 0;
    margin-top: 10px;
  }
  .nothing {
    font-size: 13px;
    background: var(--color-surface-sunken);
    padding: 14px;
    border-radius: 8px;
  }
  @media (max-width: 600px) {
    .dialog {
      padding: 20px 16px;
    }
    .changes li {
      grid-template-columns: 1fr;
    }
    .badge {
      justify-self: start;
    }
  }
</style>
