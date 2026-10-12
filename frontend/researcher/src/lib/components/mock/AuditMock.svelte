<script lang="ts">
  import { ShieldCheck, Check } from '@lucide/svelte';
  import MockWindow from './MockWindow.svelte';
  import { useInView } from './useInView';
  import type { MockProps } from './types';

  let { animate = true, label, class: className = '' }: MockProps = $props();

  // Newest first; each row is verified bottom-up, like the chain is walked from the start.
  const events = [
    {
      seq: 48,
      at: '14:32',
      type: 'change_request.merged',
      who: 'Maya Chen',
      av: 'maya',
      ini: 'MC',
      sig: '9c1e07a3'
    },
    {
      seq: 47,
      at: '14:29',
      type: 'change_request.approved',
      who: 'Priya Nair',
      av: 'priya',
      ini: 'PN',
      sig: '41d8b6f0'
    },
    {
      seq: 46,
      at: '14:11',
      type: 'change_request.approved',
      who: 'Sam Okoye',
      av: 'sam',
      ini: 'SO',
      sig: 'e2a3590c'
    },
    {
      seq: 45,
      at: '13:58',
      type: 'change_request.opened',
      who: 'Maya Chen',
      av: 'maya',
      ini: 'MC',
      sig: '7b60d4e1'
    },
    {
      seq: 44,
      at: '10:04',
      type: 'version.named',
      who: 'Sam Okoye',
      av: 'sam',
      ini: 'SO',
      sig: '08f5c2a9'
    }
  ];
</script>

<MockWindow title="dynodoc.online/admin/audit" {label} class={className}>
  <div class="au" use:useInView={{ animate }}>
    <div class="top">
      <div>
        <div class="ttl"><ShieldCheck size={16} /> Audit trail</div>
        <div class="sub mk-muted">acme / board-pack · 48 events</div>
      </div>
      <div class="ver">
        <svg class="ring" viewBox="0 0 40 40" aria-hidden="true">
          <circle class="track" cx="20" cy="20" r="16" />
          <circle class="arc" cx="20" cy="20" r="16" pathLength="100" />
          <path class="tick" d="M13 20.5l5 5 9-10.5" pathLength="100" />
        </svg>
        <div>
          <div class="state">Verified</div>
          <div class="mk-muted sub">Chain intact · head <span class="mk-mono">e7d31b09</span></div>
        </div>
      </div>
    </div>

    <ul class="mk-list tbl">
      <li class="tr th">
        <span>Seq</span><span>Time</span><span>Event</span><span class="actor">Actor</span><span
          >Signature</span
        ><span class="chk">Chain</span>
      </li>
      {#each events as e, i (e.seq)}
        <li class="tr" style="--k: {events.length - 1 - i}">
          <span class="mk-mono mk-muted">#{e.seq}</span>
          <span class="mk-muted">{e.at}</span>
          <span class="mk-mono type">{e.type}</span>
          <span class="actor"><span class="mk-av is-{e.av}">{e.ini}</span>{e.who}</span>
          <span class="mk-hash">hash · {e.sig}</span>
          <span class="chk ok"><Check size={14} /><span class="vt">Verified</span></span>
        </li>
      {/each}
    </ul>
  </div>
</MockWindow>

<style>
  .au {
    padding: 14px 16px 16px;
  }
  .top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: 12px;
    margin-bottom: 14px;
  }
  .ttl {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 15px;
    font-weight: 600;
  }
  .sub {
    font-size: 12px;
  }
  .ver {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .ring {
    flex: none;
    width: 40px;
    height: 40px;
  }
  .ring circle,
  .ring path {
    fill: none;
    stroke-width: 3;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .track {
    stroke: var(--mock-border);
  }
  .arc {
    stroke: var(--mock-success);
    stroke-dasharray: 100;
    stroke-dashoffset: 0;
    transform: rotate(-90deg);
    transform-origin: 20px 20px;
  }
  .tick {
    stroke: var(--mock-success);
    stroke-width: 2.5;
    stroke-dasharray: 100;
    stroke-dashoffset: 0;
  }
  .state {
    color: var(--mock-success-fg);
    font-weight: 600;
  }
  .tbl {
    border: 1px solid var(--mock-border);
    border-radius: 6px;
    overflow: hidden;
  }
  .tr {
    display: grid;
    grid-template-columns: 44px 48px minmax(0, 1.5fr) minmax(0, 1fr) 140px 82px;
    align-items: center;
    gap: 10px;
    padding: 7px 12px;
    font-size: 12px;
  }
  .tr + .tr {
    border-top: 1px solid var(--mock-border);
  }
  .th {
    background: var(--mock-subtle);
    color: var(--mock-muted);
    font-weight: 600;
  }
  .type {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .actor {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
    white-space: nowrap;
  }
  .chk {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .ok {
    color: var(--mock-success-fg);
    font-weight: 500;
  }

  @container mock (max-width: 640px) {
    .tr {
      grid-template-columns: 40px 44px minmax(0, 1fr) 130px 82px;
    }
    .actor {
      display: none;
    }
  }
  @container mock (max-width: 470px) {
    .tr {
      grid-template-columns: 38px minmax(0, 1fr) 24px;
    }
    .tr > :nth-child(2),
    .tr > :nth-child(5),
    .th .chk,
    .vt {
      display: none;
    }
    .top {
      gap: 10px;
    }
  }

  @media (prefers-reduced-motion: no-preference) {
    .au:global([data-motion]) .tr:not(.th) {
      animation: rise 0.4s ease-out both paused;
      animation-delay: calc(var(--k) * 0.18s);
    }
    .au:global([data-motion]) .ok {
      animation: pop 0.4s ease-out both paused;
      animation-delay: calc(var(--k) * 0.45s + 0.5s);
    }
    .au:global([data-motion]) .arc {
      animation: sweep 2.6s cubic-bezier(0.4, 0, 0.2, 1) 0.2s both paused;
    }
    .au:global([data-motion]) .tick {
      animation: sweep 0.5s ease-out 2.6s both paused;
    }
    .au:global([data-motion]) .state {
      animation: fade 0.4s ease-out 2.8s both paused;
    }
    .au:global([data-motion='play']) :is(.tr:not(.th), .ok, .arc, .tick, .state) {
      animation-play-state: running;
    }
  }
  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(6px);
    }
  }
  @keyframes pop {
    from {
      opacity: 0;
      transform: scale(0.6);
    }
  }
  @keyframes sweep {
    from {
      stroke-dashoffset: 100;
    }
  }
  @keyframes fade {
    from {
      opacity: 0.35;
    }
  }
</style>
