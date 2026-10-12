<script lang="ts">
  import { FileClock } from '@lucide/svelte';
  /** The Dynodoc task pane: project, file, captured changes, push button, sync status. */
  let { file, captured }: { file: string; captured: string[] } = $props();
</script>

<div class="pane">
  <div class="ph"><FileClock size={14} /><b>Dynodoc</b></div>
  <div class="kv">
    <span class="mk-muted">Project</span><span class="mk-mono">acme / board-pack</span>
  </div>
  <div class="kv"><span class="mk-muted">File</span><span class="mk-mono">{file}</span></div>
  <div class="cap">
    <span class="cnt"><span class="n-new">3</span></span>
    changes captured
  </div>
  <ul class="mk-list chs">
    {#each captured as c, i (c)}<li class="c" class:fresh={i === 2}>{c}</li>{/each}
  </ul>
  <span class="mk-btn is-accent push">Push for review</span>
  <div class="sync"><i class="dot"></i>Synced 12 s ago</div>
</div>

<style>
  .pane {
    display: grid;
    align-content: start;
    gap: 8px;
    padding: 12px;
    border-left: 1px solid var(--mock-border);
    font-size: 12px;
  }
  .ph {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .kv {
    display: grid;
    gap: 1px;
  }
  .kv .mk-mono {
    overflow-wrap: anywhere;
  }
  .cap {
    padding-top: 8px;
    border-top: 1px solid var(--mock-border);
    font-weight: 600;
  }
  .cnt {
    position: relative;
    display: inline-block;
    min-width: 0.7em;
    overflow: hidden;
    vertical-align: bottom;
  }
  .cnt::before {
    content: '2';
    position: absolute;
    inset: 0;
    opacity: 0;
  }
  .chs {
    display: grid;
    gap: 4px;
  }
  .c {
    padding: 4px 8px;
    border: 1px solid var(--mock-border);
    border-radius: 6px;
    background: var(--mock-subtle);
  }
  .c.fresh {
    border-color: var(--mock-accent);
    background: var(--mock-accent-bg);
  }
  .push {
    justify-content: center;
  }
  .sync {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--mock-muted);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--mock-success);
  }

  @container mock (max-width: 470px) {
    .pane {
      border-top: 1px solid var(--mock-border);
      border-left: 0;
    }
  }

  @media (prefers-reduced-motion: no-preference) {
    :global([data-motion]) .n-new {
      display: inline-block;
      animation: in 0.4s ease-out 1.3s both paused;
    }
    :global([data-motion]) .cnt::before {
      animation: out 0.4s ease-out 1.3s both paused;
    }
    :global([data-motion]) .fresh {
      animation: rise 0.45s ease-out 1.6s both paused;
    }
    :global([data-motion]) .push {
      animation: ring 0.9s ease-out 2.3s both paused;
    }
    :global([data-motion]) .dot {
      animation: blink 0.8s ease-out 2.6s 2 both paused;
    }
    :global([data-motion='play']) .cnt::before,
    :global([data-motion='play']) :is(.n-new, .fresh, .push, .dot) {
      animation-play-state: running;
    }
  }
  @keyframes in {
    from {
      opacity: 0;
      transform: translateY(100%);
    }
  }
  @keyframes out {
    from {
      opacity: 1;
      transform: none;
    }
    to {
      opacity: 0;
      transform: translateY(-100%);
    }
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
  @keyframes ring {
    from {
      box-shadow: 0 0 0 0 color-mix(in srgb, var(--mock-accent) 55%, transparent);
    }
    to {
      box-shadow: 0 0 0 10px transparent;
    }
  }
  @keyframes blink {
    from {
      box-shadow: 0 0 0 0 color-mix(in srgb, var(--mock-success) 60%, transparent);
    }
    to {
      box-shadow: 0 0 0 7px transparent;
    }
  }
</style>
