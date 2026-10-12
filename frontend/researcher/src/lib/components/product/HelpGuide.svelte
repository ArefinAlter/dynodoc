<script lang="ts">
  import { page } from '$app/stores';
  import {
    HelpCircle,
    X,
    ArrowRight,
    ArrowLeft,
    FileText,
    Table2,
    Users,
    History,
    GitBranch,
    Share2
  } from '@lucide/svelte';
  import { guide } from '$lib/workspace/guide';
  let dialog = $state<HTMLDialogElement>();
  let index = $state(0);
  const step = $derived(guide[index]);
  $effect(() => {
    window.addEventListener('dynodoc:help', open);
    return () => window.removeEventListener('dynodoc:help', open);
  });
  // Marketing (site) pages have their own header, help and support links. The tutorial
  // dialog and its button belong to the signed-in application.
  const marketing = $derived(($page.route.id ?? '').startsWith('/(site)'));
  function open() {
    index = 0;
    dialog?.showModal();
  }
</script>

{#if !marketing}{#if $page.url.pathname === '/admin'}<button
      class="help"
      title="Help & tutorials"
      aria-label="Help & tutorials"
      onclick={open}><HelpCircle size={18} /><span>Help & tutorials</span></button
    >{/if}
  <dialog bind:this={dialog} aria-labelledby="guide-title">
    <header>
      <a href="https://dynodoc.online"
        ><img
          class="logo-day"
          src="/brand/logo.svg?v=2"
          alt="Dynodoc"
          width="145"
          height="32"
        /><img
          class="logo-night"
          src="/brand/logo-dark.svg?v=2"
          alt="Dynodoc"
          width="145"
          height="32"
        /></a
      ><button aria-label="Close tutorial" onclick={() => dialog?.close()}><X size={20} /></button>
    </header>
    <div class="guide-body">
      <div class="visual" aria-hidden="true">
        <div class="document">
          <FileText size={29} />
          <div class="lines"><i></i><i></i><i></i></div>
          <Table2 size={26} />
        </div>
        <div class="connections">
          <Users class={index === 2 ? 'active' : ''} size={27} /><GitBranch
            class={index === 3 ? 'active' : ''}
            size={27}
          /><History class={index === 4 ? 'active' : ''} size={27} /><Share2
            class={index === 5 ? 'active' : ''}
            size={27}
          />
        </div>
      </div>
      <p class="action">{step.action}</p>
      <h2 id="guide-title">{step.title}</h2>
      <p class="body">{step.body}</p>
      <nav aria-label="Tutorial topics">
        {#each guide as item, i}<button
            class:current={i === index}
            aria-label={item.title}
            aria-current={i === index ? 'step' : undefined}
            onclick={() => (index = i)}
          ></button>{/each}
      </nav>
    </div>
    <footer>
      <button disabled={index === 0} onclick={() => index--}><ArrowLeft size={16} />Back</button><a
        href="https://dynodoc.online/help"
        target="_blank"
        rel="noopener noreferrer">Full guide</a
      ><button
        class="next"
        onclick={() => {
          if (index === guide.length - 1) dialog?.close();
          else index++;
        }}>{index === guide.length - 1 ? 'Start working' : 'Next'}<ArrowRight size={16} /></button
      >
    </footer>
  </dialog>{/if}

<style>
  .help {
    position: fixed;
    right: 20px;
    bottom: 18px;
    z-index: 80;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border: 1px solid var(--color-border-strong);
    border-radius: 24px;
    background: var(--color-surface-raised);
    color: var(--color-accent);
    box-shadow: var(--shadow-menu);
    font: 13px var(--font-ui);
    cursor: pointer;
  }
  dialog button {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    font: inherit;
    font-size: 14px;
    padding: 9px 12px;
    border: 1px solid var(--color-border-strong);
    border-radius: 5px;
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
    cursor: pointer;
  }
  dialog {
    border: 1px solid var(--color-border-strong);
    border-radius: 14px;
    padding: 0;
    max-width: 610px;
    width: calc(100% - 32px);
    max-height: 90vh;
    color: var(--color-ink);
    background: var(--color-surface-raised);
  }
  dialog::backdrop {
    background: var(--color-overlay);
    backdrop-filter: blur(3px);
  }
  header,
  footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 20px 25px;
    gap: 14px;
  }
  header > button {
    border: 0;
    background: none;
  }
  .guide-body {
    padding: 0 35px 22px;
  }
  .visual {
    border-radius: 10px;
    background: var(--color-accent-bg);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 45px;
    padding: 24px;
    min-height: 160px;
  }
  .document {
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 23px 18px;
    background: var(--color-page);
    border: 1px solid var(--color-accent-border);
    box-shadow: 0 5px 0 var(--color-accent-border);
    color: var(--color-accent);
  }
  .lines i {
    display: block;
    height: 5px;
    width: 55px;
    margin: 9px 0;
    background: var(--color-accent-border);
    border-radius: 3px;
  }
  .connections {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 20px;
    color: var(--color-ink-soft);
  }
  .connections :global(.active) {
    color: var(--color-success);
    animation: pulse 2s ease-in-out infinite;
  }
  .action {
    font-size: 13px;
    font-weight: 650;
    color: var(--color-accent);
    margin: 24px 0 7px;
  }
  h2 {
    font-size: 29px;
    line-height: 1.25;
    margin: 0 0 16px;
    letter-spacing: -0.7px;
  }
  .body {
    font-size: 16px;
    line-height: 1.8;
    margin: 0;
    color: var(--color-ink-soft);
  }
  nav {
    display: flex;
    justify-content: center;
    gap: 6px;
    margin-top: 18px;
  }
  nav button {
    padding: 0;
    width: 24px;
    height: 24px;
    background: transparent;
    border: 0;
    position: relative;
  }
  nav button::after {
    content: '';
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--color-border-strong);
  }
  nav button.current::after {
    background: var(--color-accent);
    width: 18px;
    border-radius: 6px;
  }
  footer {
    border-top: 1px solid var(--color-border);
  }
  footer a {
    font-size: 13px;
  }
  footer .next {
    background: var(--color-accent-solid);
    color: var(--color-on-accent);
  }
  button:disabled {
    opacity: 0.5;
  }
  @keyframes pulse {
    50% {
      transform: scale(1.14);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .connections :global(.active) {
      animation: none;
    }
  }
  @media (max-width: 500px) {
    .help span {
      display: none;
    }
    .guide-body {
      padding: 0 23px 20px;
    }
    .visual {
      gap: 18px;
    }
    .document {
      gap: 8px;
    }
    .lines i {
      width: 25px;
    }
    h2 {
      font-size: 25px;
    }
  }
  /* Wordmark with light lettering for night mode (printing keeps the day logo). */
  .logo-night {
    display: none;
  }
  @media screen {
    :global([data-theme='dark']) .logo-day {
      display: none;
    }
    :global([data-theme='dark']) .logo-night {
      display: inline;
    }
  }
</style>
