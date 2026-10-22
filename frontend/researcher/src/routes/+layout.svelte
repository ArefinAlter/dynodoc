<script lang="ts">
  import '../app.css';
  import '$lib/styles/touch.css';
  import { page } from '$app/stores';
  import { env } from '$env/dynamic/public';
  import { observeVitals } from '$lib/telemetry';
  import { onMount } from 'svelte';
  import { loadSession } from '$lib/stores/session.svelte';
  import { pwa } from '$lib/pwa/client.svelte';
  import { trackKeyboard } from '$lib/viewport';
  import { theme, setThemeCookieDomain } from '$lib/theme.svelte';
  import PwaUpdate from '$lib/components/PwaUpdate.svelte';
  onMount(() => {
    if (window.location.pathname === '/print') return;
    void loadSession();
    void observeVitals(data.site.telemetry).catch(() => {});
    pwa.start();
    return trackKeyboard();
  });
  import { beforeNavigate, goto } from '$app/navigation';
  import { isOfficePane } from '$lib/security-policy';
  // A document's CSP cannot change during client navigation. Cross the Office
  // boundary with a fresh response so Microsoft permissions never follow a pane
  // into account/workspace pages, and entering a pane gets its scoped policy.
  beforeNavigate(({ from, to, willUnload, cancel }) => {
    if (
      from &&
      to &&
      !willUnload &&
      isOfficePane(from.url.pathname) !== isOfficePane(to.url.pathname)
    ) {
      cancel();
      window.location.assign(to.url.href);
    }
  });
  import { setUnauthorizedHandler } from '$lib/api/client';
  import { requiresSignIn } from '$lib/auth-next';
  import HelpGuide from '$lib/components/product/HelpGuide.svelte';
  import ToastHost from '$lib/components/ToastHost.svelte';

  let { children, data } = $props();
  // Canonical URLs and the like exist only where the server says the site is indexable.
  const seo = $derived(data.seo);
  const canonical = $derived(
    seo && seo.paths.includes($page.url.pathname)
      ? seo.origin + ($page.url.pathname === '/' ? '/' : $page.url.pathname)
      : null
  );
  $effect.pre(() => setThemeCookieDomain(data.site.themeCookieDomain));

  // Browser bar colour. The public site sets its own; here the OS decides unless the person
  // picked day or night mode in the app, in which case both entries follow that choice.
  const bar = { light: '#f8f9fc', dark: '#0e141b' };
  const barByDay = $derived(theme.preference === 'dark' ? bar.dark : bar.light);
  const barByNight = $derived(theme.preference === 'light' ? bar.light : bar.dark);

  // When a 401 can't be recovered (refresh failed), the API client clears the session
  // and calls this handler; route the user to login.
  setUnauthorizedHandler(() => {
    const here = window.location.pathname + window.location.search;
    void goto(
      requiresSignIn(window.location.pathname)
        ? `/login?next=${encodeURIComponent(here)}`
        : '/login'
    );
  });
</script>

<svelte:head>
  {#if !$page.route.id?.startsWith('/(site)')}
    <meta name="theme-color" content={barByDay} media="(prefers-color-scheme: light)" />
    <meta name="theme-color" content={barByNight} media="(prefers-color-scheme: dark)" />
  {/if}
  {#if seo && canonical}
    <link rel="canonical" href={canonical} />
    <!-- Allow full snippets and large image previews, so search and answer
         engines can quote these pages properly rather than truncating them. -->
    <meta
      name="robots"
      content="index, follow, max-image-preview:large, max-snippet:-1, max-video-preview:-1"
    />
    <meta property="og:url" content={canonical} />
    <meta property="og:site_name" content="Dynodoc" />
    <meta property="og:type" content="website" />
    <meta property="og:locale" content="en" />
    <meta property="og:image" content={seo.image} />
    <meta property="og:image:width" content="1200" /><meta
      property="og:image:height"
      content="630"
    />
    <meta property="og:image:alt" content="Dynodoc — version control for documents" />
    <meta name="twitter:card" content="summary_large_image" />
    <meta name="twitter:image" content={seo.image} />
    <link rel="alternate" type="text/plain" href={`${seo.origin}/llms.txt`} title="llms.txt" />
    {#if env.PUBLIC_GOOGLE_SITE_VERIFICATION}<meta
        name="google-site-verification"
        content={env.PUBLIC_GOOGLE_SITE_VERIFICATION}
      />{/if}
  {:else}<meta name="robots" content="noindex,nofollow" />{/if}
</svelte:head>
{@render children()}
<ToastHost />
<PwaUpdate />
{#if $page.url.pathname !== '/print'}<HelpGuide />{/if}
