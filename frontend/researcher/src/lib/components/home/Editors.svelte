<script lang="ts">
  import { onMount } from 'svelte';
  import { fade } from 'svelte/transition';
  import { AddinMock } from '$lib/components/mock';
  import { Btn, Section } from '$lib/components/site';

  type Host = 'word' | 'excel' | 'powerpoint' | 'docs' | 'sheets' | 'slides';
  const hosts: { id: Host; name: string; where: string; captures: string; detail: string }[] = [
    {
      id: 'word',
      name: 'Word',
      where: 'Home ribbon > Dynodoc',
      captures: 'Existing paragraph text in .docx files.',
      detail: 'Content-control tags keep each paragraph’s identity across saves. Needs WordApi 1.3.'
    },
    {
      id: 'excel',
      name: 'Excel',
      where: 'Home ribbon > Dynodoc',
      captures: 'Existing cell values, formula sources and date-only values.',
      detail:
        'Named rows keep each cell’s identity. The pane captures up to 2,000 projected cells. Needs ExcelApi 1.13.'
    },
    {
      id: 'powerpoint',
      name: 'PowerPoint',
      where: 'Home ribbon > Dynodoc',
      captures: 'Text in existing slide shapes.',
      detail:
        'Slide and shape tags keep identity. Needs PowerPointApi 1.4 (Microsoft 365 or Office 2024).'
    },
    {
      id: 'docs',
      name: 'Google Docs',
      where: 'Extensions > Dynodoc > Open Dynodoc',
      captures: 'Existing paragraph text in the tab you select.',
      detail:
        'Named ranges anchor paragraphs. The sidebar polls every five seconds while capture is on.'
    },
    {
      id: 'sheets',
      name: 'Google Sheets',
      where: 'Extensions > Dynodoc > Open Dynodoc',
      captures: 'Existing cell values and formula sources.',
      detail:
        'Named ranges anchor cells. Each connected file keeps a capture queue of up to 100 KB.'
    },
    {
      id: 'slides',
      name: 'Google Slides',
      where: 'Extensions > Dynodoc > Open Dynodoc',
      captures: 'Text in existing slide shapes.',
      detail: 'The add-on uses the current-file scope only, plus Dynodoc’s connector API.'
    }
  ];

  const uid = $props.id();
  let active = $state<Host>('word');
  let ready = $state(false);
  let reduced = $state(true);
  const current = $derived(hosts.find((host) => host.id === active) ?? hosts[0]);

  onMount(() => {
    ready = true;
    reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
  });

  function onKey(event: KeyboardEvent, index: number) {
    const last = hosts.length - 1;
    let next = -1;
    if (event.key === 'ArrowRight' || event.key === 'ArrowDown')
      next = index === last ? 0 : index + 1;
    else if (event.key === 'ArrowLeft' || event.key === 'ArrowUp')
      next = index === 0 ? last : index - 1;
    else if (event.key === 'Home') next = 0;
    else if (event.key === 'End') next = last;
    if (next < 0) return;
    event.preventDefault();
    active = hosts[next].id;
    document.getElementById(`${uid}-tab-${hosts[next].id}`)?.focus();
  }
</script>

<Section
  id="editors"
  tone="muted"
  eyebrow="EDITORS"
  title="Stay in the editor you use."
  lede="The same capture protocol runs in six hosts. Choose one to see the pane that opens beside your file."
>
  <div class="tabs" role="tablist" aria-label="Editor">
    {#each hosts as host, index (host.id)}
      <button
        type="button"
        role="tab"
        id="{uid}-tab-{host.id}"
        class="tab"
        class:on={active === host.id}
        aria-selected={active === host.id}
        aria-controls="{uid}-panel"
        tabindex={active === host.id ? 0 : -1}
        disabled={!ready}
        onclick={() => (active = host.id)}
        onkeydown={(event) => onKey(event, index)}>{host.name}</button
      >
    {/each}
  </div>

  <div
    class="panel"
    role="tabpanel"
    id="{uid}-panel"
    aria-labelledby="{uid}-tab-{active}"
    tabindex="-1"
  >
    <div class="stack">
      {#key active}
        <div
          class="slide"
          in:fade={{ duration: reduced ? 0 : 260, delay: reduced ? 0 : 120 }}
          out:fade={{ duration: reduced ? 0 : 140 }}
        >
          <div class="mock">
            <AddinMock
              host={current.id}
              label="Dynodoc add-in pane open in {current.name}, listing three captured changes."
            />
          </div>
          <dl class="facts">
            <div>
              <dt class="num">OPENS FROM</dt>
              <dd class="where">{current.where}</dd>
            </div>
            <div>
              <dt class="num">CAPTURES</dt>
              <dd class="h-3">{current.captures}</dd>
            </div>
            <div>
              <dt class="num">DETAIL</dt>
              <dd class="detail">{current.detail}</dd>
            </div>
          </dl>
        </div>
      {/key}
    </div>
  </div>

  <p class="foot">
    Capture runs while the pane is open, and there is no background agent. Structure and formatting
    changes go through edited-file push. Add-ins ship as test installs today; marketplace listings
    are not live.
    <Btn href="/add-ins" variant="link" arrow>Install steps and limits</Btn>
  </p>
</Section>

<style>
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 24px;
    padding: 4px;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--site-radius);
    background: var(--color-surface);
    width: fit-content;
    max-width: 100%;
  }
  .tab {
    flex: 1 1 auto;
    min-height: 40px;
    padding: 0 14px;
    border: 1px solid transparent;
    border-radius: var(--site-radius-sm);
    background: transparent;
    color: var(--color-ink-soft);
    font: inherit;
    font-size: 0.9375rem;
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
    transition:
      background-color 0.18s var(--site-ease),
      color 0.18s var(--site-ease);
  }
  .tab:hover:not(:disabled) {
    background: var(--color-surface-hover);
    color: var(--color-ink);
  }
  .tab:disabled {
    cursor: default;
  }
  .tab.on {
    background: var(--color-ink-strong);
    color: var(--color-parchment);
  }
  .panel:focus-visible {
    outline-offset: 6px;
  }
  /* Outgoing and incoming slides share one grid cell, so they crossfade in place. */
  .stack {
    display: grid;
  }
  .slide {
    grid-area: 1 / 1;
    display: grid;
    gap: 24px;
    align-items: start;
    min-width: 0;
  }
  .mock {
    min-width: 0;
  }
  .facts {
    display: grid;
    gap: 0;
    margin: 0;
    border-top: 1px solid var(--color-border-strong);
  }
  .facts > div {
    display: grid;
    gap: 6px;
    padding: 16px 0;
    border-bottom: 1px solid var(--color-border);
  }
  .facts dt,
  .facts dd {
    margin: 0;
  }
  .where {
    font-family: var(--site-font-mono);
    font-size: 0.875rem;
    color: var(--color-ink);
  }
  .detail {
    color: var(--color-ink-soft);
    line-height: 1.6;
    max-width: 38ch;
  }
  .foot {
    margin: 28px 0 0;
    max-width: 78ch;
    color: var(--color-muted);
    font-size: 0.9375rem;
    line-height: 1.6;
  }
  .foot :global(.btn) {
    display: inline-flex;
    margin-left: 6px;
  }
  @media (max-width: 520px) {
    .tab {
      padding: 0 8px;
      font-size: 0.875rem;
    }
  }
  @media (min-width: 960px) {
    .slide {
      grid-template-columns: minmax(0, 7fr) minmax(0, 4fr);
      gap: clamp(28px, 4vw, 56px);
      align-items: center;
    }
  }
</style>
