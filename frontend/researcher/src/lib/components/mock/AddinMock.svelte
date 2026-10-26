<script lang="ts">
  import MockWindow from './MockWindow.svelte';
  import AddinCanvas from './AddinCanvas.svelte';
  import AddinPane from './AddinPane.svelte';
  import { useInView } from './useInView';
  import type { MockProps } from './types';

  type Host = 'word' | 'excel' | 'powerpoint' | 'docs' | 'sheets' | 'slides';
  let {
    host = 'word',
    animate = true,
    label,
    class: className = ''
  }: MockProps & {
    /** Which editor the add-in is shown inside. */
    host?: Host;
  } = $props();

  const google = $derived(host === 'docs' || host === 'sheets' || host === 'slides');
  const kind = $derived(
    host === 'word' || host === 'docs'
      ? 'text'
      : host === 'excel' || host === 'sheets'
        ? 'grid'
        : 'deck'
  );
  const ext = $derived(
    google ? '' : kind === 'text' ? '.docx' : kind === 'grid' ? '.xlsx' : '.pptx'
  );
  const file = $derived(
    (kind === 'text' ? 'Board update' : kind === 'grid' ? 'Q3 budget' : 'Launch plan') + ext
  );
  const tabs = $derived(
    google
      ? kind === 'text'
        ? ['File', 'Edit', 'View', 'Insert', 'Format', 'Extensions']
        : kind === 'grid'
          ? ['File', 'Edit', 'View', 'Insert', 'Data', 'Extensions']
          : ['File', 'Edit', 'View', 'Insert', 'Slide', 'Extensions']
      : [
          'File',
          'Home',
          'Insert',
          kind === 'text' ? 'Layout' : kind === 'grid' ? 'Formulas' : 'Design',
          'Review',
          'View'
        ]
  );
  const captured = $derived(
    kind === 'text'
      ? [
          'Edited · Summary, paragraph 1',
          'Edited · Risks, paragraph 1',
          'Edited · Summary, paragraph 2'
        ]
      : kind === 'grid'
        ? ['Edited · Software, Q3', 'Edited · Contractors, Q3', 'Edited · Travel, Q3 (C4)']
        : ['Edited · Slide 1 title', 'Edited · Slide 2 bullet 1', 'Edited · Slide 2 bullet 3']
  );
</script>

<MockWindow variant={host} title={file} {label} class={className}>
  <div class="ad" use:useInView={{ animate }}>
    <div class="menu">
      {#each tabs as t, i (t)}<span class="tab" class:on={i === (google ? tabs.length - 1 : 1)}
          >{t}</span
        >{/each}
    </div>
    <div class="tools" aria-hidden="true">
      <i></i><i></i><i></i><i class="sep"></i><i></i><i></i><i></i>
    </div>
    {#if kind === 'grid'}
      <div class="fx">
        <span class="nm mk-mono">C4</span><span class="mk-muted">fx</span><span class="mk-mono"
          >4650</span
        >
      </div>
    {/if}

    <div class="work">
      <div class="canvas"><AddinCanvas {kind} /></div>
      <AddinPane {file} {captured} />
    </div>
  </div>
</MockWindow>

<style>
  .ad {
    --page: #ffffff;
    --page-ink: #1f2328;
    --page-rule: #d0d7de;
    --page-chg: #aceebb;
  }
  .menu {
    display: flex;
    gap: 2px;
    padding: 6px 8px 0;
    background: var(--mock-subtle);
    font-size: 12px;
    overflow: hidden;
  }
  .tab {
    padding: 4px 9px 6px;
    border-bottom: 2px solid transparent;
    color: var(--mock-muted);
    white-space: nowrap;
  }
  .tab.on {
    border-color: var(--mock-host);
    color: var(--mock-fg);
    font-weight: 600;
  }
  .tools {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--mock-border);
    background: var(--mock-subtle);
  }
  .tools i {
    width: 22px;
    height: 16px;
    border-radius: 3px;
    background: var(--mock-inset);
  }
  .tools .sep {
    width: 1px;
    background: var(--mock-border);
  }
  .fx {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 4px 10px;
    border-bottom: 1px solid var(--mock-border);
    font-size: 12px;
  }
  .nm {
    min-width: 34px;
    padding-right: 10px;
    border-right: 1px solid var(--mock-border);
  }
  .work {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 184px;
  }
  .canvas {
    padding: 14px;
    background: var(--mock-inset);
    min-height: 210px;
  }

  @container mock (max-width: 400px) {
    .tab:not(.on):nth-child(n + 3) {
      display: none;
    }
  }
  @container mock (max-width: 470px) {
    .work {
      grid-template-columns: minmax(0, 1fr);
    }
    .canvas {
      min-height: 0;
    }
  }
</style>
