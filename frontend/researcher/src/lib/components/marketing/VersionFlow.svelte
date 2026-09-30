<script lang="ts">
  import { onMount } from 'svelte';
  let ready = $state(false);
  onMount(() => {
    ready = true;
  });
  import {
    FileText,
    GitBranch,
    Check,
    ArrowRight,
    RotateCcw,
    MessageSquare,
    CircleCheck
  } from '@lucide/svelte';
  let draft = $state<'maya' | 'sam'>('maya');
  let includeQuestion = $state(true);
  let includeNote = $state(false);
  let shared = $state(false);
  const changes = $derived(Number(includeQuestion) + Number(includeNote));
  function reset() {
    shared = false;
    includeQuestion = true;
    includeNote = false;
    draft = 'maya';
  }
</script>

<div class="version-demo">
  <div class="demo-header">
    <span><GitBranch size={16} /> One document, with room to explore</span><span
      class="example-label">Interactive example</span
    >
  </div>
  <div
    class="branch-map"
    aria-label="A shared document splits into Maya and Sam’s drafts, which can be reviewed and shared back"
  >
    <svg class="connectors" viewBox="0 0 900 160" preserveAspectRatio="none" aria-hidden="true"
      ><path
        d="M 140 80 H 230 Q 260 80 260 50 V 45 Q 260 30 300 30 H 425 M 230 80 Q 260 80 260 110 V 115 Q 260 130 300 130 H 425 M 475 30 H 600 Q 640 30 640 60 V 65 Q 640 80 670 80 H 760 M 475 130 H 600 Q 640 130 640 100 V 95 Q 640 80 670 80"
      /><path
        class="active-path"
        class:lower={draft === 'sam'}
        d={draft === 'maya'
          ? 'M 140 80 H 230 Q 260 80 260 50 V 45 Q 260 30 300 30 H 425 M 475 30 H 600 Q 640 30 640 60 V 65 Q 640 80 670 80 H 760'
          : 'M 140 80 H 230 Q 260 80 260 110 V 115 Q 260 130 300 130 H 425 M 475 130 H 600 Q 640 130 640 100 V 95 Q 640 80 670 80 H 760'}
      /></svg
    >
    <div class="map-node original">
      <span class="map-icon"><FileText size={21} /></span><b>Shared document</b><small
        >The team’s starting point</small
      >
    </div>
    <div class="drafts">
      <button
        disabled={!ready}
        class:active={draft === 'maya'}
        aria-pressed={draft === 'maya'}
        onclick={() => {
          draft = 'maya';
          shared = false;
        }}
        ><span class="person blue">M</span><span
          ><b>Maya’s draft</b><small>Refine the proposal</small></span
        ></button
      ><button
        disabled={!ready}
        class:active={draft === 'sam'}
        aria-pressed={draft === 'sam'}
        onclick={() => {
          draft = 'sam';
          shared = false;
        }}
        ><span class="person gold">S</span><span
          ><b>Sam’s draft</b><small>Explore a different approach</small></span
        ></button
      >
    </div>
    <div class="map-node team" class:shared>
      <span class="map-icon"
        >{#if shared}<CircleCheck size={22} />{:else}<FileText size={21} />{/if}</span
      ><b>{shared ? 'Team version updated' : 'Bring the best together'}</b><small
        >{shared ? 'With a record of the change' : 'Review before sharing'}</small
      >
    </div>
  </div>
  <div class="review">
    <div class="review-title">
      <span><MessageSquare size={15} /> {draft === 'maya' ? 'Maya' : 'Sam'}’s changes</span><span
        >{shared ? 'Shared with the example team' : 'Choose what belongs in the team version'}</span
      >
    </div>
    {#if shared}
      <div class="success" role="status">
        <CircleCheck size={30} />
        <div>
          <h3>The team has your {changes === 1 ? 'change' : 'changes'}.</h3>
          <p>
            {includeQuestion && includeNote
              ? 'The wording and review note are now included.'
              : includeQuestion
                ? 'The wording is shared. The review note stays in your draft.'
                : 'The review note is shared. The wording stays in your draft.'} The previous version
            remains in history.
          </p>
        </div>
        <button disabled={!ready} onclick={reset}><RotateCcw size={14} /> Try again</button>
      </div>
    {:else}
      <label class="change-row"
        ><input disabled={!ready} type="checkbox" bind:checked={includeQuestion} /><span
          class="change-type">Proposal wording</span
        ><span class="diff"
          ><del>Deliver the project by the end of August.</del><ins
            >{draft === 'maya'
              ? 'Deliver the first phase in September after review.'
              : 'Launch in August, with a second phase in October.'}</ins
          ></span
        ></label
      >
      <label class="change-row note"
        ><input disabled={!ready} type="checkbox" bind:checked={includeNote} /><span
          class="change-type">Review note</span
        ><span class="diff"
          ><ins
            >{draft === 'maya'
              ? 'Allow two weeks for accessibility review.'
              : 'Confirm the budget before approving phase two.'}</ins
          ></span
        ></label
      >
      <div class="review-bottom">
        <span><Check size={14} /> Unselected changes stay in your draft.</span><button
          disabled={!ready || changes === 0}
          onclick={() => (shared = true)}>Share selected changes <ArrowRight size={15} /></button
        >
      </div>
    {/if}
  </div>
</div>

<style>
  .version-demo {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 8px;
    overflow: hidden;
    color: var(--color-ink);
  }
  .demo-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 15px;
    padding: 18px 25px;
    background: var(--color-surface-sunken);
    border-bottom: 1px solid var(--color-border);
    font-size: 12px;
  }
  .demo-header > span:first-child {
    display: flex;
    align-items: center;
    gap: 9px;
  }
  .example-label {
    font-size: 10px;
    color: var(--color-ink-soft);
  }
  .branch-map {
    position: relative;
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    align-items: center;
    gap: 25px;
    padding: 32px 20px;
    min-height: 235px;
  }
  .connectors {
    position: absolute;
    width: 100%;
    height: 160px;
    top: calc(50% - 80px);
    left: 0;
    fill: none;
    stroke: var(--color-border);
    stroke-width: 2;
    pointer-events: none;
  }
  .active-path {
    stroke: color-mix(in srgb, var(--color-accent) 60%, transparent);
    stroke-dasharray: 5 6;
  }
  .active-path.lower {
    stroke: color-mix(in srgb, var(--color-warn) 70%, transparent);
  }
  .map-node,
  .drafts {
    position: relative;
    z-index: 1;
    justify-self: center;
  }
  .map-node {
    text-align: center;
    background: var(--color-surface);
    padding: 10px 14px;
    max-width: 185px;
  }
  .map-icon {
    display: grid;
    place-items: center;
    background: var(--color-accent-bg);
    border: 1px solid var(--color-accent-border);
    border-radius: 7px;
    width: 43px;
    height: 48px;
    margin: 0 auto 12px;
    color: var(--color-accent);
  }
  .map-node b {
    font-size: 12px;
    display: block;
    font-weight: 600;
  }
  .map-node small {
    font-size: 9px;
    display: block;
    color: var(--color-ink-soft);
    margin-top: 4px;
  }
  .team.shared .map-icon {
    background: var(--color-success-bg);
    border-color: var(--color-success-border);
    color: var(--color-success);
  }
  .drafts {
    display: grid;
    gap: 30px;
  }
  .drafts button {
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 5px;
    padding: 12px 16px;
    display: flex;
    align-items: center;
    gap: 13px;
    text-align: left;
    cursor: pointer;
    min-width: 190px;
    color: var(--color-ink-soft);
  }
  .drafts button.active {
    border-color: color-mix(in srgb, var(--color-accent) 60%, transparent);
    background: var(--color-accent-bg);
    box-shadow: 0 0 0 3px var(--color-accent-bg);
  }
  .drafts button:last-child.active {
    border-color: color-mix(in srgb, var(--color-warn) 70%, transparent);
    background: var(--color-warn-bg);
    box-shadow: 0 0 0 3px var(--color-warn-bg);
  }
  .person {
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    font-size: 10px;
    font-weight: 600;
    flex-shrink: 0;
  }
  .blue {
    background: color-mix(in srgb, var(--color-accent) 16%, var(--color-surface));
    color: var(--color-accent);
  }
  .gold {
    background: color-mix(in srgb, var(--color-warn) 24%, var(--color-surface));
    color: var(--color-warn-ink);
  }
  .drafts b {
    display: block;
    font-size: 11px;
    font-weight: 600;
  }
  .drafts small {
    display: block;
    font-size: 9px;
    color: var(--color-ink-soft);
    margin-top: 2px;
  }
  .review {
    margin: 0 25px 25px;
    border: 1px solid var(--color-border);
    border-radius: 5px;
    background: var(--color-surface-raised);
  }
  .review-title {
    background: var(--color-surface-sunken);
    border-bottom: 1px solid var(--color-border-soft);
    display: flex;
    justify-content: space-between;
    gap: 15px;
    padding: 14px 20px;
    font-size: 11px;
  }
  .review-title > span:first-child {
    display: flex;
    align-items: center;
    gap: 9px;
    font-weight: 600;
  }
  .review-title > span:last-child {
    font-size: 10px;
    color: var(--color-ink-soft);
  }
  .change-row {
    display: grid;
    grid-template-columns: 16px 135px 1fr;
    gap: 15px;
    align-items: start;
    padding: 22px 20px;
    border-bottom: 1px solid var(--color-border-soft);
    cursor: pointer;
  }
  .change-row input {
    accent-color: var(--color-accent);
    margin: 3px 0 0;
    width: 14px;
    height: 14px;
  }
  .change-type {
    font-size: 11px;
    color: var(--color-ink-soft);
  }
  .diff {
    display: grid;
    gap: 8px;
    font-size: 12px;
    line-height: 1.7;
  }
  .diff del {
    background: var(--color-diff-del-bg);
    color: var(--color-diff-del-ink);
    text-decoration-thickness: 1px;
    padding: 3px 8px;
    width: fit-content;
  }
  .diff ins {
    background: var(--color-diff-add-bg);
    color: var(--color-diff-add-ink);
    text-decoration: none;
    padding: 3px 8px;
    width: fit-content;
  }
  .review-bottom {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 15px 20px;
    gap: 20px;
    font-size: 10px;
  }
  .review-bottom > span {
    display: flex;
    align-items: center;
    gap: 7px;
    color: var(--color-ink-soft);
  }
  .review-bottom button,
  .success button {
    border: 0;
    background: var(--color-accent-solid);
    color: var(--color-on-accent);
    display: flex;
    gap: 12px;
    align-items: center;
    padding: 11px 16px;
    font-size: 11px;
    border-radius: 4px;
    cursor: pointer;
  }
  .review-bottom button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .success {
    display: flex;
    align-items: center;
    padding: 36px 30px;
    gap: 22px;
    min-height: 245px;
    color: var(--color-success);
  }
  .success h3 {
    font-size: 22px;
    line-height: 1.3;
    font-weight: 550;
    letter-spacing: -0.5px;
    margin: 0 0 8px;
  }
  .success p {
    font-size: 12px;
    line-height: 1.9;
    color: var(--color-ink-soft);
    max-width: 420px;
    margin: 0;
  }
  .success > :global(svg) {
    flex-shrink: 0;
  }
  .success button {
    background: var(--color-success-bg);
    color: var(--color-success);
    border: 1px solid var(--color-success-border);
    margin-left: auto;
    white-space: nowrap;
  }
  @media (prefers-reduced-motion: no-preference) {
    .active-path {
      animation: travel 2.5s linear 2;
    }
    .success {
      animation: appear 0.3s ease-out;
    }
    @keyframes travel {
      to {
        stroke-dashoffset: -44;
      }
    }
    @keyframes appear {
      from {
        opacity: 0;
        transform: translateY(6px);
      }
      to {
        opacity: 1;
        transform: translateY(0);
      }
    }
  }
  @media (max-width: 650px) {
    .demo-header {
      padding: 15px;
      font-size: 10px;
    }
    .example-label {
      display: none;
    }
    .branch-map {
      padding: 24px 7px;
      gap: 5px;
      min-height: 220px;
    }
    .map-node {
      padding: 8px 4px;
    }
    .map-node b {
      font-size: 10px;
      line-height: 1.4;
    }
    .map-node small {
      font-size: 8px;
      line-height: 1.5;
      max-width: 75px;
      margin: 5px auto 0;
    }
    .map-icon {
      width: 33px;
      height: 37px;
      margin-bottom: 10px;
    }
    .drafts {
      gap: 23px;
    }
    .drafts button {
      min-width: 0;
      padding: 10px 9px;
      gap: 6px;
    }
    .drafts b {
      font-size: 10px;
    }
    .drafts small {
      font-size: 8px;
      max-width: 82px;
      line-height: 1.5;
    }
    .person {
      width: 21px;
      height: 21px;
      font-size: 8px;
    }
    .review {
      margin: 0 12px 12px;
    }
    .review-title {
      padding: 13px;
      flex-wrap: wrap;
      gap: 5px;
    }
    .review-title > span:last-child {
      width: 100%;
      font-size: 9px;
      margin-left: 24px;
    }
    .change-row {
      padding: 17px 13px;
      grid-template-columns: 16px 1fr;
      gap: 9px;
    }
    .change-type {
      font-size: 10px;
    }
    .diff {
      grid-column: 2;
      font-size: 10px;
    }
    .review-bottom {
      padding: 14px;
      flex-direction: column;
      align-items: flex-start;
      gap: 13px;
    }
    .review-bottom > span {
      font-size: 9px;
    }
    .review-bottom button {
      width: 100%;
      justify-content: space-between;
    }
    .success {
      padding: 25px 18px;
      gap: 15px;
      flex-wrap: wrap;
    }
    .success > div {
      flex: 1;
      min-width: 170px;
    }
    .success h3 {
      font-size: 20px;
    }
    .success p {
      font-size: 11px;
    }
    .success button {
      margin: 0;
    }
  }
</style>
