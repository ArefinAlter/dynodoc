<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { page } from '$app/stores';
  import { env } from '$env/dynamic/public';
  import { ArrowUpRight, ChevronDown, Menu, X } from '@lucide/svelte';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  import {
    NAV_DIRECT,
    NAV_GROUPS,
    isAppHost,
    signInHref,
    siteHref,
    type NavLink
  } from '$lib/components/site/nav';

  // Sticky site header. Works with or without the `.site` wrapper: it only uses
  // `--color-*` tokens. Links are relative on the marketing site and point at the
  // canonical origin when the page is served from an app host (e.g. /login, /view).
  const DESKTOP = '(min-width: 961px)';
  // Static, first-party CSS: without JavaScript the dropdowns and the menu button cannot
  // work, so flatten every link into one wrapping row.
  const NAV = 'header[data-site-header] [data-site-nav]';
  const noscript =
    '<noscript><style>' +
    [
      `header[data-site-header]{position:static!important}`,
      `header[data-site-header] .bar{flex-wrap:wrap!important;padding-block:12px}`,
      `header[data-site-header] [data-site-menu],${NAV} [data-group-button],${NAV} .theme-toggle,${NAV} .item-desc{display:none!important}`,
      `${NAV}{display:flex!important;position:static!important;flex:1 1 100%!important;flex-direction:column!important;align-items:flex-start!important;gap:12px!important;padding:0!important;background:none!important;overflow:visible!important}`,
      `${NAV} .list{flex-direction:row!important;flex-wrap:wrap!important;gap:2px 18px!important}`,
      `${NAV} .group,${NAV} .panel{display:contents!important;visibility:visible!important;opacity:1!important;transform:none!important}`,
      `${NAV} .direct{border:0!important}`,
      `${NAV} .direct a,${NAV} .panel a{display:inline-flex!important;width:auto!important;min-height:32px!important;padding:0!important;border:0!important;font-size:15px!important;font-weight:500!important}`,
      `${NAV} .actions{flex-wrap:wrap!important}`
    ].join('') +
    '</style></noscript>';
  let header = $state<HTMLElement>();
  let menuButton = $state<HTMLButtonElement>();
  let ready = $state(false);
  let desktop = $state(true);
  let menuOpen = $state(false);
  let openGroup = $state<string | null>(null);
  let scrolled = $state(false);
  let hoverTimer: ReturnType<typeof setTimeout> | undefined;

  const relative = $derived(!isAppHost($page.url.hostname));
  const href = (path: string) => siteHref(path, relative);
  const current = (link: NavLink) =>
    !link.external && link.href === $page.url.pathname ? 'page' : undefined;
  const groupActive = (links: NavLink[]) => links.some((link) => current(link));

  onMount(() => {
    ready = true;
    const query = window.matchMedia(DESKTOP);
    const sync = () => {
      desktop = query.matches;
      if (desktop) menuOpen = false;
      openGroup = null;
    };
    sync();
    query.addEventListener('change', sync);
    const scroll = () => (scrolled = window.scrollY > 8);
    scroll();
    window.addEventListener('scroll', scroll, { passive: true });
    return () => {
      query.removeEventListener('change', sync);
      window.removeEventListener('scroll', scroll);
      clearTimeout(hoverTimer);
    };
  });

  // The open full-screen menu owns the page: lock background scrolling.
  $effect(() => {
    if (!menuOpen) return;
    const root = document.documentElement;
    const previous = root.style.overflow;
    root.style.overflow = 'hidden';
    return () => {
      root.style.overflow = previous;
    };
  });

  function closeAll() {
    menuOpen = false;
    openGroup = null;
  }

  function toggleMenu() {
    menuOpen = !menuOpen;
    openGroup = null;
  }

  function toggleGroup(event: MouseEvent, id: string) {
    clearTimeout(hoverTimer);
    // A mouse click follows the hover that already opened the panel: keep it open.
    const mouse = desktop && (event as PointerEvent).pointerType === 'mouse' && event.detail > 0;
    openGroup = mouse ? id : openGroup === id ? null : id;
  }

  // Hover intent (desktop mouse only): a short delay in, a longer one out, so a
  // pointer crossing the gap between the button and its panel does not close it.
  function hoverIn(event: PointerEvent, id: string) {
    if (!desktop || event.pointerType !== 'mouse') return;
    clearTimeout(hoverTimer);
    hoverTimer = setTimeout(() => (openGroup = id), openGroup ? 0 : 90);
  }
  function hoverOut(event: PointerEvent, id: string) {
    if (!desktop || event.pointerType !== 'mouse') return;
    clearTimeout(hoverTimer);
    hoverTimer = setTimeout(() => {
      if (openGroup === id) openGroup = null;
    }, 180);
  }

  function groupFocusOut(event: FocusEvent, id: string) {
    if (!desktop || openGroup !== id) return;
    const next = event.relatedTarget;
    if (next instanceof Node && (event.currentTarget as HTMLElement).contains(next)) return;
    openGroup = null;
  }

  async function groupKey(event: KeyboardEvent, id: string) {
    if (event.key !== 'ArrowDown') return;
    event.preventDefault();
    const group = (event.currentTarget as HTMLElement).closest('li');
    openGroup = id;
    await tick();
    group?.querySelector<HTMLElement>('.panel a')?.focus();
  }

  function focusable(): HTMLElement[] {
    if (!header) return [];
    return Array.from(
      header.querySelectorAll<HTMLElement>('a[href], button:not([disabled])')
    ).filter(
      (el) => el.getClientRects().length > 0 && getComputedStyle(el).visibility !== 'hidden'
    );
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      if (menuOpen) {
        event.preventDefault();
        menuOpen = false;
        openGroup = null;
        menuButton?.focus();
      } else if (openGroup) {
        event.preventDefault();
        const group = header?.querySelector<HTMLElement>(`[data-group="${openGroup}"]`);
        const hadFocus = !!group?.contains(document.activeElement);
        openGroup = null;
        if (hadFocus) group?.querySelector<HTMLElement>('button')?.focus();
      }
      return;
    }
    if (event.key !== 'Tab' || !menuOpen) return;
    // Full-screen menu: keep Tab inside the header controls.
    const items = focusable();
    if (!items.length) return;
    const first = items[0];
    const last = items[items.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  function onPointerDown(event: PointerEvent) {
    if (openGroup && header && event.target instanceof Node && !header.contains(event.target))
      openGroup = null;
  }
</script>

<svelte:window onkeydown={onKeydown} onpointerdown={onPointerDown} />
<svelte:head>
  <!-- Without JavaScript the menu button cannot work: show the links as a plain list. -->
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  {@html noscript}
</svelte:head>

<a class="skip" href="#main">Skip to content</a>
<header
  class="site-header"
  class:scrolled
  class:menu-open={menuOpen}
  data-site-header
  data-ready={ready || undefined}
  bind:this={header}
>
  <div class="bar container">
    <a class="brand" href={href('/')} aria-label="Dynodoc home" onclick={closeAll}
      ><img src="/brand/mark.svg?v=2" width="30" height="30" alt="" /><span>dynodoc</span></a
    >
    <button
      type="button"
      class="menu"
      data-site-menu
      disabled={!ready}
      aria-label={menuOpen ? 'Close menu' : 'Open menu'}
      aria-expanded={menuOpen}
      aria-controls="site-nav"
      onclick={toggleMenu}
      >{#if menuOpen}<X size={22} aria-hidden="true" />{:else}<Menu
          size={22}
          aria-hidden="true"
        />{/if}</button
    >
    <nav id="site-nav" class="nav" class:open={menuOpen} data-site-nav aria-label="Main navigation">
      <ul class="list">
        {#each NAV_GROUPS as group (group.id)}
          {@const open = openGroup === group.id}
          <li
            class="group"
            class:open
            data-group={group.id}
            onpointerenter={(event) => hoverIn(event, group.id)}
            onpointerleave={(event) => hoverOut(event, group.id)}
            onfocusout={(event) => groupFocusOut(event, group.id)}
          >
            <button
              type="button"
              class="group-button"
              class:active={groupActive(group.links)}
              data-group-button
              aria-expanded={open}
              aria-controls={`panel-${group.id}`}
              onclick={(event) => toggleGroup(event, group.id)}
              onkeydown={(event) => groupKey(event, group.id)}
              >{group.label}<ChevronDown class="chevron" size={15} aria-hidden="true" /></button
            >
            <ul class="panel" id={`panel-${group.id}`} data-panel>
              {#each group.links as link (link.href)}
                <li>
                  <a href={href(link.href)} aria-current={current(link)} onclick={closeAll}
                    ><span class="item-title">{link.label}</span>{#if link.description}<span
                        class="item-desc">{link.description}</span
                      >{/if}</a
                  >
                </li>
              {/each}
            </ul>
          </li>
        {/each}
        {#each NAV_DIRECT as link (link.href + link.label)}
          <li class="direct">
            <a
              href={link.external ? link.href : href(link.href)}
              aria-current={current(link)}
              onclick={closeAll}
              target={link.external ? '_blank' : undefined}
              rel={link.external ? 'noopener noreferrer' : undefined}
              >{link.label}{#if link.external}<ArrowUpRight size={14} aria-hidden="true" /><span
                  class="sr-only"
                >
                  (opens in a new tab)</span
                >{/if}</a
            >
          </li>
        {/each}
      </ul>
      <div class="actions">
        <ThemeToggle variant={desktop ? 'icon' : 'label'} />
        <a class="signin" href={signInHref(env.PUBLIC_WORKSPACE_URL)} onclick={closeAll}>Sign in</a>
        <a class="cta" href={href('/try')} onclick={closeAll}
          >Try the pre-alpha<ArrowUpRight class="cta-arrow" size={15} aria-hidden="true" /></a
        >
      </div>
    </nav>
  </div>
</header>

<style>
  .skip {
    position: fixed;
    top: -100px;
    left: 16px;
    z-index: 100;
    padding: 12px 16px;
    border: 1px solid var(--color-ink);
    border-radius: 4px;
    background: var(--color-surface-raised);
    color: var(--color-ink);
    font-family: var(--site-font-sans, var(--font-ui));
    font-size: 0.9375rem;
  }
  .skip:focus {
    top: 12px;
  }

  .site-header {
    position: sticky;
    top: 0;
    z-index: 50;
    border-bottom: 1px solid var(--color-border);
    font-family: var(--site-font-sans, var(--font-ui));
    color: var(--color-ink);
  }
  /* The blur lives on a pseudo-element: backdrop-filter on the header itself would make
     it the containing block of the fixed full-screen menu. */
  .site-header::before {
    content: '';
    position: absolute;
    inset: 0;
    z-index: -1;
    background: color-mix(in srgb, var(--color-parchment) 82%, transparent);
    -webkit-backdrop-filter: saturate(1.4) blur(14px);
    backdrop-filter: saturate(1.4) blur(14px);
    transition: background-color 0.2s;
  }
  .site-header.scrolled::before {
    background: color-mix(in srgb, var(--color-parchment) 92%, transparent);
  }
  .bar {
    display: flex;
    align-items: center;
    gap: 24px;
    min-height: var(--site-header-h, 64px);
    width: 100%;
    max-width: calc(var(--site-max, 1200px) + 2 * var(--site-gutter, 24px));
    margin-inline: auto;
    padding-inline: var(--site-gutter, 24px);
  }

  .brand {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    flex: none;
    color: var(--color-ink-strong);
    font-size: 1.3125rem;
    font-weight: 620;
    letter-spacing: -0.04em;
    text-decoration: none;
  }
  .brand img {
    display: block;
    width: 30px;
    height: 30px;
  }

  .nav {
    display: flex;
    flex: 1;
    align-items: center;
    justify-content: space-between;
    gap: 24px;
    min-width: 0;
  }
  .list {
    display: flex;
    align-items: center;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .group {
    position: relative;
  }
  .group-button,
  .direct a {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-height: 40px;
    padding: 0 12px;
    border: 0;
    border-radius: 4px;
    background: none;
    color: var(--color-ink-soft);
    font: inherit;
    font-size: 0.9375rem;
    font-weight: 500;
    text-decoration: none;
    cursor: pointer;
    transition:
      color 0.15s,
      background-color 0.15s;
  }
  .group-button:hover,
  .group.open .group-button,
  .direct a:hover {
    color: var(--color-ink-strong);
    background: var(--color-surface-hover);
  }
  .group-button.active,
  .direct a[aria-current='page'] {
    color: var(--color-ink-strong);
  }
  .group-button :global(.chevron) {
    transition: transform 0.2s;
  }
  .group.open .group-button :global(.chevron) {
    transform: rotate(180deg);
  }

  .panel {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 2;
    width: 340px;
    margin: 0;
    padding: 8px;
    list-style: none;
    border: 1px solid var(--color-border);
    border-radius: 6px;
    background: var(--color-surface-raised);
    box-shadow: var(--shadow-menu);
    visibility: hidden;
    opacity: 0;
    transform: translateY(-4px);
    transition:
      opacity 0.16s ease,
      transform 0.16s ease,
      visibility 0s 0.16s;
  }
  .group.open .panel {
    visibility: visible;
    opacity: 1;
    transform: none;
    transition:
      opacity 0.16s ease,
      transform 0.16s ease,
      visibility 0s;
  }
  /* Before hydration (and without JavaScript) the panels still open on hover/focus. */
  .site-header:not([data-ready]) .group:hover .panel,
  .site-header:not([data-ready]) .group:focus-within .panel {
    visibility: visible;
    opacity: 1;
    transform: none;
  }
  .panel a {
    display: grid;
    gap: 2px;
    padding: 10px 12px;
    border-radius: 4px;
    color: inherit;
    text-decoration: none;
  }
  .panel a:hover,
  .panel a:focus-visible {
    background: var(--color-surface-hover);
  }
  .item-title {
    color: var(--color-ink-strong);
    font-size: 0.9375rem;
    font-weight: 540;
  }
  .panel a[aria-current='page'] .item-title {
    color: var(--color-accent);
  }
  .item-desc {
    color: var(--color-muted);
    font-size: 0.8125rem;
    line-height: 1.4;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }
  .signin {
    display: inline-flex;
    align-items: center;
    min-height: 40px;
    padding: 0 12px;
    border-radius: 4px;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
    font-weight: 500;
    text-decoration: none;
  }
  .signin:hover {
    color: var(--color-ink-strong);
    background: var(--color-surface-hover);
  }
  .cta {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    min-height: 40px;
    padding: 0 16px;
    border: 1px solid var(--color-ink-strong);
    border-radius: 4px;
    background: var(--color-ink-strong);
    color: var(--color-parchment);
    font-size: 0.9375rem;
    font-weight: 500;
    text-decoration: none;
    white-space: nowrap;
    transition:
      background-color 0.18s,
      border-color 0.18s;
  }
  .cta:hover {
    background: var(--color-accent);
    border-color: var(--color-accent);
  }
  .cta :global(.cta-arrow) {
    transition: transform 0.2s;
  }
  .cta:hover :global(.cta-arrow) {
    transform: translate(2px, -2px);
  }

  .menu {
    display: none;
  }

  @media (max-width: 960px) {
    .bar {
      gap: 12px;
    }
    .menu {
      display: inline-grid;
      place-items: center;
      width: 44px;
      height: 44px;
      margin-left: auto;
      border: 1px solid var(--color-border-strong);
      border-radius: 4px;
      background: transparent;
      color: var(--color-ink);
      cursor: pointer;
    }
    .menu:disabled {
      opacity: 0.5;
    }
    .nav {
      display: none;
    }
    /* Full-screen menu behind the bar (z-index -1 inside the header's stacking context). */
    .nav.open {
      position: fixed;
      inset: 0;
      z-index: -1;
      display: flex;
      flex-direction: column;
      align-items: stretch;
      justify-content: flex-start;
      gap: 24px;
      padding: calc(var(--site-header-h, 64px) + 8px) var(--site-gutter, 20px) 28px;
      overflow-y: auto;
      overscroll-behavior: contain;
      background: var(--color-parchment);
    }
    .list {
      flex-direction: column;
      align-items: stretch;
      gap: 0;
    }
    .group,
    .direct {
      border-bottom: 1px solid var(--color-border);
    }
    .group-button,
    .direct a {
      width: 100%;
      min-height: 56px;
      justify-content: space-between;
      padding: 0;
      border-radius: 0;
      background: none !important;
      color: var(--color-ink-strong);
      font-size: 1.25rem;
      font-weight: 520;
      letter-spacing: -0.02em;
    }
    .panel {
      position: static;
      display: none;
      width: auto;
      padding: 0 0 12px;
      border: 0;
      background: none;
      box-shadow: none;
      visibility: visible;
      opacity: 1;
      transform: none;
    }
    .group.open .panel {
      display: block;
    }
    .panel a {
      padding: 10px 0 10px 14px;
      border-left: 1px solid var(--color-border-strong);
      border-radius: 0;
    }
    .actions {
      flex-wrap: wrap;
      gap: 12px;
    }
    .actions :global(.theme-toggle) {
      order: 3;
      width: 100%;
      justify-content: flex-start;
      border-color: var(--color-border);
    }
    .signin,
    .cta {
      flex: 1 1 140px;
      justify-content: center;
      min-height: 48px;
      border: 1px solid var(--color-border-strong);
    }
    .cta {
      border-color: var(--color-ink-strong);
    }
  }
</style>
