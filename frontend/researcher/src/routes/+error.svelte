<script lang="ts">
  // The error page for every host. On the public website it uses the marketing header, footer
  // and styling, so a mistyped address leads somewhere. Anywhere else (the workspace and admin
  // hosts, self-hosted installations) it stays a short, plain page in the application styling.
  // site.css only styles content inside a `.site` wrapper, so importing it here is safe.
  import '$lib/styles/site.css';
  import { page } from '$app/stores';
  import Dino from '$lib/art/Dino.svelte';
  import Tracks from '$lib/art/Tracks.svelte';
  import InfoHero from '$lib/components/marketing/InfoHero.svelte';
  import SiteFooter from '$lib/components/marketing/SiteFooter.svelte';
  import SiteHeader from '$lib/components/marketing/SiteHeader.svelte';
  import { Btn } from '$lib/components/site';

  const site = $derived(
    $page.data?.site as
      | { indexable?: boolean; homeUrl?: string; helpUrl?: string; workspaceUrl?: string }
      | undefined
  );
  const marketing = $derived(Boolean(site?.indexable));
  const notFound = $derived($page.status === 404);
  const heading = $derived(notFound ? 'Page not found' : 'Something went wrong');
  const message = $derived(
    notFound
      ? 'There is nothing at this address. The page may have moved, or the link may be mistyped.'
      : ($page.error?.message ??
          'The page could not be shown. Try again in a moment, or go back to the start.')
  );
</script>

<svelte:head>
  <title>{heading} · Dynodoc</title>
</svelte:head>

{#if marketing}
  <div class="site">
    <SiteHeader />
    <main id="main">
      <InfoHero title={heading} lede={message}>
        {#snippet actions()}
          <Btn href="/" arrow>Back to the home page</Btn>
          <Btn href="/try" variant="secondary">Try the pre-alpha</Btn>
        {/snippet}
        {#snippet art()}
          <Dino name="tracks" art={Tracks} />
        {/snippet}
      </InfoHero>
      <section class="container more" aria-label="Pages that exist">
        <p>
          Looking for something specific? Try the <a href="/features">features</a>, the
          <a href="/help">help guide</a>, the <a href="/roadmap">roadmap</a> or
          <a href="/support">support</a>.
        </p>
      </section>
    </main>
    <SiteFooter />
  </div>
{:else}
  <main id="main" class="plain">
    <h1>{heading}</h1>
    <p>{message}</p>
    <p>
      <a href={site?.workspaceUrl ? `${site.workspaceUrl}/projects` : '/'}>Go to your projects</a>
      {#if site?.helpUrl}
        <span aria-hidden="true">·</span> <a href={site.helpUrl}>Help</a>
      {/if}
    </p>
  </main>
{/if}

<style>
  .more {
    padding-block: clamp(32px, 5vw, 56px);
    color: var(--color-ink-soft);
  }
  .more p {
    margin: 0;
  }
  .plain {
    max-width: 38rem;
    margin: 0 auto;
    padding: 12vh 20px;
    color: var(--color-ink);
  }
  .plain h1 {
    margin: 0 0 12px;
    font-size: 1.75rem;
    line-height: 1.2;
  }
  .plain p {
    margin: 0 0 12px;
    color: var(--color-ink-soft);
  }
  .plain a {
    color: var(--color-accent);
  }
</style>
