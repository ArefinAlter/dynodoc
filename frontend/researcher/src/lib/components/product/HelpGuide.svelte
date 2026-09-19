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
  let dialog: HTMLDialogElement;
  let index = $state(0),
    welcome = $state(false);
  const visible = $derived(
    $page.url.pathname === '/workspace' ||
      $page.url.pathname.startsWith('/documents/') ||
      $page.url.pathname === '/admin'
  );
  const step = $derived(guide[index]);
  $effect(() => {
    if ($page.url.pathname === '/workspace')
      try {
        welcome = !localStorage.getItem('dynodoc_guide_v1');
      } catch {
        welcome = true;
      }
  });
  function remember() {
    welcome = false;
    try {
      localStorage.setItem('dynodoc_guide_v1', 'seen');
    } catch {
      /* The guide remains usable when local storage is blocked. */
    }
  }
  function open() {
    remember();
    index = 0;
    dialog.showModal();
  }
</script>

{#if visible}<button
    class="help"
    style:bottom={$page.url.pathname.startsWith('/documents/') ? '52px' : '22px'}
    onclick={open}><HelpCircle size={18} /><span>Help & tutorials</span></button
  >{#if welcome && $page.url.pathname === '/workspace'}<aside
      class="welcome"
      aria-labelledby="welcome-title"
    >
      <button class="dismiss" aria-label="Dismiss welcome" onclick={remember}
        ><X size={16} /></button
      >
      <p id="welcome-title">Your research has a new home.</p>
      <span>A short guide to editing, team access and document history.</span>
      <div>
        <button onclick={open}>Take a tour <ArrowRight size={15} /></button><button
          onclick={remember}>Not now</button
        >
      </div>
    </aside>{/if}{/if}
<dialog bind:this={dialog} aria-labelledby="guide-title">
  <header>
    <a href="https://dynodoc.online"
      ><img src="/brand/logo.svg?v=2" alt="Dynodoc" width="145" height="32" /></a
    ><button aria-label="Close tutorial" onclick={() => dialog.close()}><X size={20} /></button>
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
        if (index === guide.length - 1) dialog.close();
        else index++;
      }}>{index === guide.length - 1 ? 'Start working' : 'Next'}<ArrowRight size={16} /></button
    >
  </footer>
</dialog>

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
    border: 1px solid #b9cde2;
    border-radius: 24px;
    background: #fffefb;
    color: #245696;
    box-shadow: 0 3px 14px #183e6415;
    font: 13px var(--font-ui);
    cursor: pointer;
  }
  .welcome {
    position: fixed;
    right: 20px;
    bottom: 76px;
    z-index: 85;
    max-width: 340px;
    width: calc(100vw - 40px);
    background: #fffefb;
    color: #29465e;
    border: 1px solid #b9cde2;
    border-radius: 10px;
    padding: 22px;
    box-shadow: 0 10px 28px #23456820;
  }
  .welcome > p {
    font-weight: 650;
    font-size: 18px;
    margin: 4px 0 9px;
  }
  .welcome > span {
    font-size: 14px;
    line-height: 1.7;
  }
  .welcome > div {
    display: flex;
    gap: 10px;
    margin-top: 17px;
  }
  .welcome button,
  dialog button {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    font: inherit;
    font-size: 14px;
    padding: 9px 12px;
    border: 1px solid #b9cde2;
    border-radius: 5px;
    background: #e8f1fd;
    color: #25568d;
    cursor: pointer;
  }
  .welcome .dismiss {
    position: absolute;
    top: 8px;
    right: 8px;
    background: none;
    border: 0;
    padding: 5px;
  }
  dialog {
    border: 1px solid #c3d2e1;
    border-radius: 14px;
    padding: 0;
    max-width: 610px;
    width: calc(100% - 32px);
    max-height: 90vh;
    color: #28465f;
    background: #fcfbf7;
  }
  dialog::backdrop {
    background: #173b5c66;
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
    background: #e7f0fd;
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
    background: white;
    border: 1px solid #bacee6;
    box-shadow: 0 5px 0 #ccdef2;
    color: #2460ae;
  }
  .lines i {
    display: block;
    height: 5px;
    width: 55px;
    margin: 9px 0;
    background: #c3d7ef;
    border-radius: 3px;
  }
  .connections {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 20px;
    color: #4e7093;
  }
  .connections :global(.active) {
    color: #107c41;
    animation: pulse 2s ease-in-out infinite;
  }
  .action {
    font-size: 13px;
    font-weight: 650;
    color: #275c9d;
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
    color: #456079;
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
    background: #b5c9df;
  }
  nav button.current::after {
    background: #2861ac;
    width: 18px;
    border-radius: 6px;
  }
  footer {
    border-top: 1px solid #d9e2ec;
  }
  footer a {
    font-size: 13px;
  }
  footer .next {
    background: #245baa;
    color: white;
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
</style>
