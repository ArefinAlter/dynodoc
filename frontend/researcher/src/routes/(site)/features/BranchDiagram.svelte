<script lang="ts">
  import { MockWindow } from '$lib/components/mock';

  /** Mockup: a private draft branches from main, main moves on, and the merge has two parents. */
  let { label }: { label?: string } = $props();
</script>

<MockWindow title="acme / board-pack · history" {label}>
  <div class="bd">
    <svg viewBox="0 0 360 184" aria-hidden="true" focusable="false">
      <text class="lane" x="30" y="14">main</text>
      <text class="lane" x="150" y="174">draft · Maya Chen</text>

      <path class="line" d="M30 56H330" />
      <path class="line fork" d="M100 56C100 96 118 126 150 126H215C247 126 255 96 255 56" />

      <circle class="node" cx="30" cy="56" r="7" />
      <circle class="node" cx="100" cy="56" r="7" />
      <circle class="node" cx="175" cy="56" r="7" />
      <circle class="node merge" cx="255" cy="56" r="9" />
      <circle class="node" cx="330" cy="56" r="7" />
      <circle class="node draft" cx="150" cy="126" r="7" />
      <circle class="node draft" cx="215" cy="126" r="7" />

      <text class="id" x="30" y="36" text-anchor="middle">a1f3</text>
      <text class="id" x="100" y="36" text-anchor="middle">5be2</text>
      <text class="id" x="175" y="36" text-anchor="middle">c04a</text>
      <text class="id" x="255" y="34" text-anchor="middle">e7d3</text>
      <text class="id" x="330" y="36" text-anchor="middle">02b9</text>
      <text class="id" x="150" y="150" text-anchor="middle">9a40</text>
      <text class="id" x="215" y="150" text-anchor="middle">31c8</text>
    </svg>
    <ul class="mk-list key">
      <li><i class="k main"></i>Main moves on while the draft is private.</li>
      <li><i class="k draft"></i>The draft is compared with main, then reviewed.</li>
      <li><i class="k merge"></i>The merge commit records both parents.</li>
    </ul>
  </div>
</MockWindow>

<style>
  .bd {
    display: grid;
    gap: 12px;
    padding: 16px;
  }
  svg {
    display: block;
    width: 100%;
    height: auto;
    overflow: visible;
  }
  .line {
    fill: none;
    stroke: var(--mock-border);
    stroke-width: 2;
    stroke-linecap: round;
  }
  .fork {
    stroke: var(--mock-done);
  }
  .node {
    fill: var(--mock-canvas);
    stroke: var(--mock-accent);
    stroke-width: 2.5;
  }
  .node.draft {
    stroke: var(--mock-done);
  }
  .node.merge {
    fill: var(--mock-success);
    stroke: var(--mock-success);
  }
  text {
    font-family: var(--mock-mono);
    font-size: 11px;
    fill: var(--mock-muted);
  }
  .lane {
    font-size: 12px;
    font-weight: 600;
    fill: var(--mock-fg);
  }
  .key {
    display: grid;
    gap: 6px;
    font-size: 12px;
    color: var(--mock-muted);
  }
  .key li {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .k {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 2px solid var(--mock-accent);
  }
  .k.draft {
    border-color: var(--mock-done);
  }
  .k.merge {
    border-color: var(--mock-success);
    background: var(--mock-success);
  }
</style>
