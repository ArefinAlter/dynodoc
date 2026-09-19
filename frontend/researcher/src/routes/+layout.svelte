<script lang="ts">
  import '../app.css';
  import { page } from '$app/stores';
  import { env } from '$env/dynamic/public';
  import { observeVitals } from '$lib/telemetry';
  import { onMount } from 'svelte';
  import { loadSession } from '$lib/stores/session.svelte';
  onMount(() => {
    void loadSession();
    void observeVitals().catch(() => {});
  });
  import { goto } from '$app/navigation';
  import { setUnauthorizedHandler } from '$lib/api/client';
  import ToastHost from '$lib/components/ToastHost.svelte';

  let { children } = $props();

  // When a 401 can't be recovered (refresh failed), the API client clears the session
  // and calls this handler; route the user to login.
  setUnauthorizedHandler(() => {
    void goto('/login');
  });
</script>

<svelte:head>
  {#if ['/', '/privacy', '/terms'].includes($page.url.pathname)}
    <link rel="canonical" href={`https://dynodoc.online${$page.url.pathname}`} />
    <meta property="og:url" content={`https://dynodoc.online${$page.url.pathname}`} />
    <meta property="og:site_name" content="Dynodoc" />
    <meta property="og:type" content="website" />
    <meta property="og:image" content="https://dynodoc.online/brand/social.png" />
    <meta property="og:image:width" content="1200" /><meta
      property="og:image:height"
      content="630"
    />
    <meta name="twitter:card" content="summary_large_image" />
    <meta name="twitter:image" content="https://dynodoc.online/brand/social.png" />
    {#if env.PUBLIC_GOOGLE_SITE_VERIFICATION}<meta
        name="google-site-verification"
        content={env.PUBLIC_GOOGLE_SITE_VERIFICATION}
      />{/if}
  {:else}<meta name="robots" content="noindex,nofollow" />{/if}
</svelte:head>
{@render children()}
<ToastHost />
