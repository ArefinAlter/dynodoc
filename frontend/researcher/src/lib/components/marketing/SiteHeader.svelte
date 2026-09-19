<script lang="ts">
  import { onMount } from 'svelte';
  let ready = $state(false);
  onMount(() => {
    ready = true;
  });
  import { env } from '$env/dynamic/public';
  import { ArrowUpRight, Menu, X } from '@lucide/svelte';
  let open = $state(false);
  const workspace = $derived(
    env.PUBLIC_WORKSPACE_URL
      ? `${env.PUBLIC_WORKSPACE_URL.replace(/\/$/, '')}/workspace`
      : '/workspace'
  );
</script>

<a class="skip" href="#main">Skip to content</a>
<header>
  <a class="brand" href="/" aria-label="Dynodoc home"
    ><img src="/brand/mark.svg" width="35" height="37" alt="" />dynodoc<span class="dot">.</span></a
  >
  <nav aria-label="Main navigation" class:open>
    <a href="/#workspace" onclick={() => (open = false)}>The workspace</a>
    <a href="/#collaboration" onclick={() => (open = false)}>How it works</a>
    <a href="/#integrations" onclick={() => (open = false)}>Integrations</a>
  </nav>
  <a class="enter" href={workspace}>Open workspace <ArrowUpRight size={16} /></a>
  <button
    disabled={!ready}
    class="menu"
    aria-label={open ? 'Close menu' : 'Open menu'}
    aria-expanded={open}
    onclick={() => (open = !open)}
    >{#if open}<X size={23} />{:else}<Menu size={23} />{/if}</button
  >
</header>

<style>
  header {
    max-width: 1360px;
    margin: auto;
    min-height: 94px;
    padding: 20px 48px;
    display: flex;
    align-items: center;
    gap: 36px;
    position: relative;
    z-index: 20;
    font-family: var(--font-ui);
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 27px;
    font-weight: 750;
    letter-spacing: -1.2px;
    color: #1a3045;
    text-decoration: none;
  }
  .brand > img {
    display: grid;
    place-items: center;
    width: 35px;
    height: 37px;
    border-radius: 7px;
  }
  .brand .dot {
    color: #2059b0;
    margin-left: -9px;
  }
  nav {
    display: flex;
    gap: 30px;
    margin-left: auto;
  }
  nav a {
    font-size: 13px;
    font-weight: 550;
    color: #405469;
    text-decoration: none;
  }
  nav a:hover {
    color: #185abd;
    text-decoration: underline;
    text-underline-offset: 6px;
  }
  .enter {
    display: flex;
    align-items: center;
    gap: 12px;
    background: #e4eefb;
    border: 1px solid #cedef3;
    padding: 11px 16px;
    border-radius: 5px;
    text-decoration: none;
    font-size: 12px;
    font-weight: 650;
    color: #184c96;
    white-space: nowrap;
  }
  .enter:hover {
    background: #d7e7fb;
  }
  .menu {
    display: none;
  }
  .skip {
    position: fixed;
    top: -100px;
    left: 20px;
    z-index: 100;
    background: white;
    padding: 12px;
  }
  .skip:focus {
    top: 12px;
  }
  @media (max-width: 900px) {
    header {
      padding: 18px 24px;
      gap: 18px;
    }
    nav {
      gap: 18px;
    }
    .brand {
      font-size: 24px;
    }
  }
  @media (max-width: 720px) {
    header {
      min-height: 80px;
      padding: 16px 20px;
    }
    .brand {
      font-size: 23px;
      gap: 8px;
    }
    .brand > img {
      width: 30px;
      height: 32px;
    }
    .enter {
      margin-left: auto;
      font-size: 11px;
      padding: 9px 10px;
    }
    .enter :global(svg) {
      display: none;
    }
    .menu {
      display: grid;
      place-items: center;
      border: 0;
      background: none;
      color: #273b52;
      padding: 4px;
      cursor: pointer;
    }
    nav {
      display: none;
    }
    nav.open {
      display: flex;
      flex-direction: column;
      position: absolute;
      top: 76px;
      left: 16px;
      right: 16px;
      background: #fffefb;
      border: 1px solid #d7e0e9;
      padding: 24px;
      box-shadow: 0 12px 24px #243b5312;
      border-radius: 5px;
    }
  }
  @media (max-width: 360px) {
    header {
      gap: 10px;
      padding-left: 16px;
      padding-right: 16px;
    }
    .brand {
      font-size: 21px;
    }
    .enter {
      font-size: 10px;
      padding: 9px;
    }
  }
  @media (prefers-reduced-motion: no-preference) {
    a {
      transition:
        background 0.2s,
        color 0.2s;
    }
  }
</style>
