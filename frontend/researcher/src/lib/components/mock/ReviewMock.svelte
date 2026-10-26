<script lang="ts">
  import { GitPullRequest, GitMerge, Check } from '@lucide/svelte';
  import MockWindow from './MockWindow.svelte';
  import { useInView } from './useInView';
  import type { MockProps } from './types';

  let { animate = true, label, class: className = '' }: MockProps = $props();
</script>

<MockWindow title="Change request #14 · acme / board-pack" {label} class={className}>
  <div class="rv" use:useInView={{ animate }}>
    <div class="head">
      <div class="ttl">Tighten the Q3 summary <span class="mk-muted">#14</span></div>
      <div class="meta">
        <span class="mk-badge is-success"><GitPullRequest size={12} /> Open</span>
        <span class="mk-muted"
          ><b>Maya Chen</b> wants to merge 1 change into <span class="mk-mono">main</span> · Board update.docx</span
        >
      </div>
    </div>

    <div class="cols">
      <div class="changes">
        <section class="chg">
          <div class="chg-h"><span class="mk-mono">Q3 summary · paragraph 1</span></div>
          <div class="chg-b">
            Revenue <del class="w" style="--d: 0.4s">grew sharply</del><ins
              class="w"
              style="--d: 0.7s">rose 12%</ins
            >
            in Q3, led by <del class="w" style="--d: 1s">the new enterprise tier</del><ins
              class="w"
              style="--d: 1.3s">enterprise renewals</ins
            >.
          </div>
          <div class="chg-f">
            <span class="mk-btn is-primary ok" style="--d: 1.9s"><Check size={13} /> Approve</span>
            <span class="mk-btn">Request changes</span>
            <span class="by mk-muted" style="--d: 2s">Approved by Sam Okoye</span>
          </div>
        </section>
        <section class="chg">
          <div class="chg-h"><span class="mk-mono">Q3 summary · paragraph 2</span></div>
          <div class="chg-b">
            Operating costs <del class="w" style="--d: 2.4s">increased</del><ins
              class="w"
              style="--d: 2.7s">held steady</ins
            >
            against the revised budget.
          </div>
          <div class="chg-f">
            <span class="mk-btn is-primary ok" style="--d: 3.3s"><Check size={13} /> Approve</span>
            <span class="mk-btn">Request changes</span>
            <span class="by mk-muted" style="--d: 3.4s">Approved by Priya Nair</span>
          </div>
        </section>
      </div>

      <div class="side">
        <div class="h">Reviewers</div>
        <div class="who">
          <span class="mk-av is-sam">SO</span>Sam Okoye<span class="tick" style="--d: 1.9s"
            ><Check size={14} /></span
          >
        </div>
        <div class="who">
          <span class="mk-av is-priya">PN</span>Priya Nair<span class="tick" style="--d: 3.3s"
            ><Check size={14} /></span
          >
        </div>
        <div class="gate">
          <div class="steps">
            <i class="step" style="--d: 1.9s"></i><i class="step" style="--d: 3.3s"></i>
          </div>
          <div class="gate-t"><b>2 of 2 approvals</b></div>
          <span class="mk-btn is-primary merge" style="--d: 3.8s"><GitMerge size={13} /> Merge</span
          >
        </div>
      </div>
    </div>
  </div>
</MockWindow>

<style>
  .rv {
    padding: 14px 16px 16px;
  }
  .ttl {
    font-size: 16px;
    font-weight: 600;
    line-height: 1.3;
  }
  .meta {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin-top: 8px;
    font-size: 12px;
  }
  .meta b {
    color: var(--mock-fg);
  }
  .meta .mk-badge {
    flex: none;
  }
  .cols {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 170px;
    gap: 16px;
    margin-top: 14px;
  }
  .changes {
    display: grid;
    gap: 10px;
  }
  .chg {
    border: 1px solid var(--mock-border);
    border-radius: 6px;
    overflow: hidden;
  }
  .chg-h {
    padding: 6px 12px;
    border-bottom: 1px solid var(--mock-border);
    background: var(--mock-subtle);
    color: var(--mock-muted);
    font-size: 12px;
  }
  .chg-b {
    padding: 10px 12px;
    font-size: 14px;
    line-height: 1.6;
  }
  .w {
    padding: 1px 3px;
    border-radius: 3px;
    text-decoration: none;
  }
  del.w {
    background: var(--mock-del);
    text-decoration: line-through;
  }
  ins.w {
    background: var(--mock-ins);
  }
  .chg-f {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    padding: 8px 12px;
    border-top: 1px solid var(--mock-border);
  }
  .chg-f .mk-btn {
    height: 26px;
  }
  .by {
    margin-left: auto;
    font-size: 12px;
  }
  .side {
    display: grid;
    align-content: start;
    gap: 8px;
    font-size: 12px;
  }
  .h {
    font-weight: 600;
  }
  .who {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .tick {
    display: inline-flex;
    margin-left: auto;
    color: var(--mock-success);
  }
  .gate {
    display: grid;
    gap: 8px;
    padding: 10px;
    border: 1px solid var(--mock-border);
    border-radius: 6px;
    background: var(--mock-subtle);
  }
  .steps {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 3px;
  }
  .step {
    height: 6px;
    border-radius: 3px;
    background: var(--mock-success);
  }
  .merge {
    justify-content: center;
  }

  @container mock (max-width: 500px) {
    .cols {
      grid-template-columns: minmax(0, 1fr);
    }
    .by {
      margin-left: 0;
      flex-basis: 100%;
    }
  }

  @media (prefers-reduced-motion: no-preference) {
    .rv:global([data-motion]) .w {
      animation: mark 0.45s ease-out both paused;
      animation-delay: var(--d);
    }
    .rv:global([data-motion]) .ok {
      animation: approve 0.35s ease-out both paused;
      animation-delay: var(--d);
    }
    .rv:global([data-motion]) .by {
      animation: fade 0.35s ease-out both paused;
      animation-delay: var(--d);
    }
    .rv:global([data-motion]) .tick {
      animation: stamp 0.35s ease-out both paused;
      animation-delay: var(--d);
    }
    .rv:global([data-motion]) .step {
      transform-origin: left;
      animation: fill 0.4s ease-out both paused;
      animation-delay: var(--d);
    }
    .rv:global([data-motion]) .merge {
      animation:
        enable 0.3s ease-out both paused,
        ring 0.9s ease-out both paused;
      animation-delay: var(--d), calc(var(--d) + 0.3s);
    }
    .rv:global([data-motion='play']) :is(.w, .ok, .by, .tick, .step, .merge) {
      animation-play-state: running;
    }
  }
  @keyframes mark {
    from {
      background: transparent;
      text-decoration-color: transparent;
    }
  }
  @keyframes approve {
    from {
      border-color: var(--mock-border);
      background: var(--mock-subtle);
      color: var(--mock-fg);
    }
  }
  @keyframes fade {
    from {
      opacity: 0;
    }
  }
  @keyframes stamp {
    from {
      opacity: 0;
      transform: scale(0.3);
    }
  }
  @keyframes fill {
    from {
      transform: scaleX(0);
    }
  }
  @keyframes enable {
    from {
      border-color: var(--mock-border);
      background: var(--mock-subtle);
      color: var(--mock-muted);
      opacity: 0.6;
    }
  }
  @keyframes ring {
    from {
      box-shadow: 0 0 0 0 color-mix(in srgb, var(--mock-success) 55%, transparent);
    }
    to {
      box-shadow: 0 0 0 10px transparent;
    }
  }
</style>
