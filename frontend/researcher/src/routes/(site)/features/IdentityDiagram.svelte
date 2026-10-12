<script lang="ts">
  import { MockWindow } from '$lib/components/mock';

  /** Mockup: one paragraph node keeps its ID while its formatting runs split and join. */
  let { label }: { label?: string } = $props();

  // [text, bold]: the same paragraph at two revisions. Runs are maximal equal-format spans.
  const before: [string, boolean][] = [
    ['Revenue rose ', false],
    ['12%', true],
    [' in Q3, led by enterprise renewals.', false]
  ];
  const after: [string, boolean][] = [
    ['Revenue rose ', false],
    ['12%', true],
    [' in ', false],
    ['Q3', true],
    [', led by enterprise renewals and multi-year contracts.', false]
  ];
</script>

<MockWindow title="Board update.docx" {label}>
  <div class="id">
    <section class="state">
      <div class="hd">
        <span class="mk-hash">node 01K7R2M4XQ</span>
        <span class="mk-badge">paragraph</span>
        <span class="mk-muted when">revision 41 · Sam Okoye</span>
      </div>
      <p class="runs">
        {#each before as [text, bold], index (index)}<span class="run" class:b={bold}>{text}</span
          >{/each}
      </p>
      <div class="count mk-mono mk-muted">3 runs</div>
    </section>

    <div class="between">
      <span class="stem" aria-hidden="true"></span>
      <span class="mk-badge is-success">same node ID</span>
      <span class="mk-muted note">Maya Chen bolds “Q3” and extends the sentence</span>
      <span class="stem" aria-hidden="true"></span>
    </div>

    <section class="state is-after">
      <div class="hd">
        <span class="mk-hash">node 01K7R2M4XQ</span>
        <span class="mk-badge">paragraph</span>
        <span class="mk-muted when">revision 42 · Maya Chen</span>
      </div>
      <p class="runs">
        {#each after as [text, bold], index (index)}<span class="run" class:b={bold}>{text}</span
          >{/each}
      </p>
      <div class="count mk-mono mk-muted">5 runs · run boundaries are not identities</div>
    </section>
  </div>
</MockWindow>

<style>
  .id {
    display: grid;
    gap: 0;
    padding: 16px;
    background-color: var(--mock-canvas);
  }
  .state {
    display: grid;
    gap: 10px;
    padding: 14px;
    border: 1px solid var(--mock-border);
    border-radius: 6px;
    background: var(--mock-subtle);
  }
  .is-after {
    border-color: var(--mock-accent);
    box-shadow: 0 0 0 3px var(--mock-accent-bg);
  }
  .hd {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
  }
  .when {
    margin-left: auto;
    font-size: 12px;
  }
  .runs {
    margin: 0;
    font-size: 14px;
    line-height: 2;
  }
  .run {
    margin-inline: 2px;
    padding: 2px 1px;
    border-radius: 2px;
    outline: 1px dashed var(--mock-border);
    outline-offset: 1px;
    white-space: pre-wrap;
    background: var(--mock-canvas);
  }
  .run.b {
    font-weight: 700;
    background: var(--mock-accent-bg);
    outline-color: var(--mock-accent);
  }
  .count {
    font-size: 11px;
  }
  .between {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: center;
    gap: 6px 10px;
    padding: 10px 0;
    font-size: 12px;
    text-align: center;
  }
  .stem {
    display: none;
  }
  @container mock (min-width: 460px) {
    .stem {
      display: block;
      flex: 1;
      height: 1px;
      min-width: 12px;
      background: var(--mock-border);
    }
  }
</style>
