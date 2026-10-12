<script lang="ts">
  import { Laptop, FolderGit2, Cloud, Check } from '@lucide/svelte';
  import MockWindow from './MockWindow.svelte';
  import { useInView } from './useInView';
  import type { MockProps } from './types';

  let { animate = true, label, class: className = '' }: MockProps = $props();
</script>

<MockWindow variant="plain" {label} class={className}>
  <div class="sy" use:useInView={{ animate, repeat: true }}>
    <div class="node">
      <Laptop size={18} />
      <b>Word on laptop</b>
      <span class="sub mk-mono">Board update.docx</span>
    </div>

    <div class="link">
      <div class="lane fwd">
        <span class="lbl mk-mono">capture</span><i class="pk"></i><i class="pk b"></i>
      </div>
      <div class="lane back">
        <span class="lbl mk-mono">pull</span><i class="pk"></i><i class="pk b"></i>
      </div>
    </div>

    <div class="node hub">
      <FolderGit2 size={18} />
      <b>Dynodoc project</b>
      <span class="sub mk-mono">acme / board-pack</span>
      <span class="stages">
        <span class="st" style="--d: 0s"><Check size={11} /> review</span>
        <span class="st" style="--d: 1.4s"><Check size={11} /> merge</span>
      </span>
    </div>

    <div class="link">
      <div class="lane fwd">
        <span class="lbl mk-mono">pull</span><i class="pk"></i><i class="pk b"></i>
      </div>
      <div class="lane back">
        <span class="lbl mk-mono">capture</span><i class="pk"></i><i class="pk b"></i>
      </div>
    </div>

    <div class="node">
      <Cloud size={18} />
      <b>Google Docs</b>
      <span class="sub mk-mono">Board update</span>
    </div>
  </div>
</MockWindow>

<style>
  .sy {
    display: grid;
    grid-template-columns:
      minmax(0, 1fr) minmax(56px, 0.55fr) minmax(0, 1.25fr) minmax(56px, 0.55fr)
      minmax(0, 1fr);
    align-items: center;
    padding: 22px 16px;
    background-image: radial-gradient(var(--mock-border) 1px, transparent 1px);
    background-size: 14px 14px;
  }
  .node {
    display: grid;
    justify-items: center;
    gap: 4px;
    padding: 12px 8px;
    border: 1px solid var(--mock-border);
    border-radius: 6px;
    background: var(--mock-canvas);
    text-align: center;
  }
  .node b {
    font-size: 13px;
    line-height: 1.25;
  }
  .sub {
    max-width: 100%;
    overflow-wrap: anywhere;
    color: var(--mock-muted);
    font-size: 11px;
  }
  .hub {
    border-color: var(--mock-accent);
    box-shadow: 0 0 0 3px var(--mock-accent-bg);
  }
  .stages {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 6px;
    margin-top: 6px;
  }
  .st {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    padding: 0 8px;
    border: 1px solid var(--mock-accent);
    border-radius: 999px;
    background: var(--mock-accent-bg);
    color: var(--mock-accent-fg);
    font: 600 11px/1 var(--mock-mono);
  }
  .link {
    display: grid;
    gap: 6px;
    padding: 0 6px;
  }
  .lane {
    position: relative;
    height: 30px;
  }
  .lane::before {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 21px;
    height: 1px;
    background: var(--mock-muted);
  }
  .lane::after {
    content: '';
    position: absolute;
    top: 17px;
    border: 4px solid transparent;
  }
  .fwd::after {
    right: -1px;
    border-left: 7px solid var(--mock-muted);
  }
  .back::after {
    left: -1px;
    border-right: 7px solid var(--mock-muted);
  }
  .lbl {
    position: absolute;
    left: 0;
    right: 0;
    top: 0;
    color: var(--mock-muted);
    font-size: 11px;
    text-align: center;
  }
  .pk {
    position: absolute;
    top: 17px;
    left: 22%;
    width: 9px;
    height: 9px;
    border-radius: 2px;
    background: var(--mock-accent);
  }
  .pk.b {
    left: 66%;
    background: var(--mock-success);
  }

  @container mock (max-width: 470px) {
    .sy {
      grid-template-columns: minmax(0, 260px);
      justify-content: center;
    }
    .link {
      grid-template-columns: 1fr 1fr;
      gap: 0;
      height: 64px;
      padding: 0 14px;
    }
    .lane {
      height: 100%;
    }
    .lane::before {
      left: 50%;
      right: auto;
      top: 0;
      bottom: 0;
      width: 1px;
      height: auto;
    }
    .lane::after {
      left: calc(50% - 4px);
      right: auto;
    }
    .fwd::after {
      top: auto;
      bottom: -1px;
      border: 4px solid transparent;
      border-top: 7px solid var(--mock-muted);
    }
    .back::after {
      top: -1px;
      border: 4px solid transparent;
      border-bottom: 7px solid var(--mock-muted);
    }
    .lbl {
      left: calc(50% + 8px);
      right: auto;
      top: 24px;
      text-align: left;
    }
    .pk {
      top: 20%;
      left: calc(50% - 4px);
    }
    .pk.b {
      top: 62%;
      left: calc(50% - 4px);
    }
  }

  @media (prefers-reduced-motion: no-preference) {
    .sy:global([data-motion]) .pk {
      animation: run-x 2.6s linear infinite paused;
    }
    .sy:global([data-motion]) .pk.b {
      animation-delay: -1.3s;
    }
    .sy:global([data-motion]) .back .pk {
      animation-direction: reverse;
    }
    .sy:global([data-motion]) .st {
      animation: lit 2.8s ease-in-out var(--d) infinite both paused;
    }
    .sy:global([data-motion='play']) :is(.pk, .st) {
      animation-play-state: running;
    }
    @container mock (max-width: 470px) {
      .sy:global([data-motion]) .pk {
        animation-name: run-y;
      }
    }
  }
  @keyframes run-x {
    0% {
      left: 0;
      opacity: 0;
    }
    12%,
    88% {
      opacity: 1;
    }
    100% {
      left: calc(100% - 9px);
      opacity: 0;
    }
  }
  @keyframes run-y {
    0% {
      top: 0;
      opacity: 0;
    }
    12%,
    88% {
      opacity: 1;
    }
    100% {
      top: calc(100% - 9px);
      opacity: 0;
    }
  }
  @keyframes lit {
    0%,
    100% {
      border-color: var(--mock-border);
      background: var(--mock-canvas);
      color: var(--mock-muted);
    }
    15%,
    70% {
      border-color: var(--mock-accent);
      background: var(--mock-accent-bg);
      color: var(--mock-accent-fg);
    }
  }
</style>
