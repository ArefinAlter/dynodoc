<script lang="ts">
  import { FileText, FileSpreadsheet, Presentation, File, LockKeyhole } from '@lucide/svelte';
  import MockWindow from './MockWindow.svelte';
  import { useInView } from './useInView';
  import type { MockProps } from './types';

  let { animate = true, label, class: className = '' }: MockProps = $props();

  const tabs = [
    ['Files', ''],
    ['Pull requests', '2'],
    ['Activity', ''],
    ['Settings', '']
  ];
  const files = [
    { name: 'Board update.docx', msg: 'Tighten the Q3 summary', when: '2 hours ago', kind: 'doc' },
    {
      name: 'Q3 budget.xlsx',
      msg: 'Update travel and contractor lines',
      when: 'yesterday',
      kind: 'xls'
    },
    {
      name: 'Launch plan.pptx',
      msg: 'Add the press embargo date',
      when: '3 days ago',
      kind: 'ppt'
    },
    { name: 'README.md', msg: 'Describe the review rules', when: 'last week', kind: 'md' }
  ];
  const formats = [
    ['Word', 'doc', 46],
    ['Excel', 'xls', 28],
    ['PowerPoint', 'ppt', 18],
    ['Markdown', 'md', 8]
  ] as const;
</script>

<MockWindow title="dynodoc.online/projects/acme/board-pack" {label} class={className}>
  <div class="rp" use:useInView={{ animate }}>
    <div class="crumb">
      <span class="path"><span class="mk-muted">acme</span> / <b>board-pack</b></span>
      <span class="mk-badge"><LockKeyhole size={11} /> Invite-only</span>
    </div>
    <div class="tabs">
      {#each tabs as [name, count], i (name)}
        <span class="tab" class:on={i === 0}
          >{name}{#if count}<span class="mk-pill cnt">{count}</span>{/if}</span
        >
      {/each}
    </div>
    <div class="cols">
      <div class="files">
        <div class="latest">
          <span class="mk-av is-maya">MC</span><b>Maya Chen</b>
          <span class="msg">Tighten the Q3 summary</span>
          <span class="when mk-muted">2 hours ago</span>
        </div>
        <ul class="mk-list">
          {#each files as f, i (f.name)}
            <li class="row" style="--i: {i}">
              <span class="ico {f.kind}">
                {#if f.kind === 'doc'}<FileText size={15} />
                {:else if f.kind === 'xls'}<FileSpreadsheet size={15} />
                {:else if f.kind === 'ppt'}<Presentation size={15} />
                {:else}<File size={15} />{/if}
              </span>
              <span class="name">{f.name}</span>
              <span class="msg mk-muted">{f.msg}</span>
              <span class="when mk-muted">{f.when}</span>
            </li>
          {/each}
        </ul>
      </div>
      <div class="about">
        <div class="h">About</div>
        <div class="mk-muted">Quarterly board pack, reviewed before it goes out.</div>
        <div class="h">Formats</div>
        <div class="bar">
          {#each formats as [, kind, pct], i (kind)}
            <i class="seg {kind}" style="width: {pct}%; --i: {i}"></i>
          {/each}
        </div>
        <ul class="mk-list legend">
          {#each formats as [name, kind, pct] (kind)}
            <li><i class="sw {kind}"></i>{name} <span class="mk-muted">{pct}%</span></li>
          {/each}
        </ul>
      </div>
    </div>
  </div>
</MockWindow>

<style>
  .rp {
    padding: 14px 16px 16px;
    --c-doc: #0969da;
    --c-xls: #1a7f37;
    --c-ppt: #bc4c00;
    --c-md: #6e7781;
  }
  .crumb {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px 10px;
    font-size: 15px;
  }
  .path b {
    font-weight: 600;
    color: var(--mock-accent-fg);
  }
  .tabs {
    display: flex;
    gap: 2px;
    margin: 12px 0 14px;
    border-bottom: 1px solid var(--mock-border);
    overflow: hidden;
  }
  .tab {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px 8px;
    margin-bottom: -1px;
    border-bottom: 2px solid transparent;
    color: var(--mock-muted);
    white-space: nowrap;
  }
  .tab.on {
    border-color: #fd8c73;
    color: var(--mock-fg);
    font-weight: 600;
  }
  .cols {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 170px;
    gap: 16px;
  }
  .files {
    border: 1px solid var(--mock-border);
    border-radius: 6px;
    overflow: hidden;
  }
  .latest,
  .row {
    display: grid;
    align-items: center;
    gap: 10px;
  }
  .latest {
    grid-template-columns: auto auto minmax(0, 1fr) auto;
    padding: 8px 12px;
    background: var(--mock-subtle);
    border-bottom: 1px solid var(--mock-border);
  }
  .row {
    grid-template-columns: 18px minmax(0, 1.1fr) minmax(0, 1.4fr) 84px;
    padding: 8px 12px;
  }
  .row + .row {
    border-top: 1px solid var(--mock-border);
  }
  .name {
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .msg {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .when {
    text-align: right;
    font-size: 12px;
    white-space: nowrap;
  }
  .ico {
    display: inline-flex;
  }
  .doc {
    color: var(--c-doc);
  }
  .xls {
    color: var(--c-xls);
  }
  .ppt {
    color: var(--c-ppt);
  }
  .md {
    color: var(--c-md);
  }
  .about {
    display: grid;
    align-content: start;
    gap: 8px;
    font-size: 12px;
  }
  .h {
    font-size: 12px;
    font-weight: 600;
    padding-top: 2px;
  }
  .bar {
    display: flex;
    gap: 2px;
    height: 8px;
    border-radius: 4px;
    overflow: hidden;
  }
  .seg,
  .sw {
    display: block;
    background: var(--c-doc);
  }
  .seg.xls,
  .sw.xls {
    background: var(--c-xls);
  }
  .seg.ppt,
  .sw.ppt {
    background: var(--c-ppt);
  }
  .seg.md,
  .sw.md {
    background: var(--c-md);
  }
  .legend {
    display: grid;
    gap: 3px;
  }
  .legend li {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .sw {
    width: 8px;
    height: 8px;
    border-radius: 50%;
  }

  @container mock (max-width: 540px) {
    .cols {
      grid-template-columns: minmax(0, 1fr);
    }
    .about {
      order: -1;
      grid-template-columns: 1fr;
    }
  }
  @container mock (max-width: 380px) {
    .tab:nth-child(3) {
      display: none;
    }
  }
  @container mock (max-width: 420px) {
    .row {
      grid-template-columns: 18px minmax(0, 1fr) auto;
    }
    .row .msg {
      display: none;
    }
    .latest {
      grid-template-columns: auto auto minmax(0, 1fr);
    }
    .latest .when {
      display: none;
    }
    .about {
      display: none;
    }
  }

  @media (prefers-reduced-motion: no-preference) {
    .rp:global([data-motion]) .row,
    .rp:global([data-motion]) .latest,
    .rp:global([data-motion]) .cnt {
      animation: rise 0.5s ease-out both paused;
      animation-delay: calc(var(--i, 0) * 110ms + 100ms);
    }
    .rp:global([data-motion]) .cnt {
      animation-name: pop;
      animation-delay: 700ms;
    }
    .rp:global([data-motion]) .seg {
      transform-origin: left;
      animation: grow 0.7s cubic-bezier(0.2, 0.7, 0.2, 1) both paused;
      animation-delay: calc(var(--i, 0) * 120ms + 400ms);
    }
    .rp:global([data-motion='play']) .row,
    .rp:global([data-motion='play']) .latest,
    .rp:global([data-motion='play']) .cnt,
    .rp:global([data-motion='play']) .seg {
      animation-play-state: running;
    }
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(8px);
    }
  }
  @keyframes pop {
    from {
      transform: scale(0.4);
      background: var(--mock-accent-bg);
    }
    60% {
      transform: scale(1.25);
    }
  }
  @keyframes grow {
    from {
      transform: scaleX(0);
    }
  }
</style>
