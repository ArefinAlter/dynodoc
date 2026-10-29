<script lang="ts">
  import { env } from '$env/dynamic/public';
  import { Btn, Section, scrollable, spotlight } from '$lib/components/site';
  import { workspaceHref } from '$lib/components/site/nav';
  import { pageMeta } from '$lib/seo';
  import { onMount } from 'svelte';
  import DownloadCards from '$lib/components/site/DownloadCards.svelte';
  import { PLATFORMS, RELEASE, SAMPLE_WORKSPACE_URL } from '$lib/site/release';
  import { detectOS, type DetectedOS } from '$lib/site/os';

  const meta = pageMeta('/try');
  const title = meta?.title ?? 'Download Dynodoc · Dynodoc';
  const description =
    meta?.description ??
    'Download the Dynodoc workflow manager: Windows 0.1 alpha 1 or Linux 0.0.1c. Manage repositories, reviews, history and sync beside your document editors.';
  const workspace = $derived(workspaceHref(env.PUBLIC_WORKSPACE_URL));

  // The visitor's platform, after mount only: the server-rendered page has equal cards.
  let os = $state<DetectedOS>('');
  onMount(() => {
    os = detectOS();
  });
  const heroPlatform = $derived(PLATFORMS.find((p) => p.os === os));
</script>

<svelte:head>
  <title>{title}</title>
  <meta name="description" content={description} />
  <meta property="og:title" content={title} />
  <meta property="og:description" content={description} />
  <meta name="twitter:title" content={title} />
  <meta name="twitter:description" content={description} />
</svelte:head>

<main id="main" use:scrollable>
  <header class="hero dotgrid dotgrid--fade spotlight" use:spotlight>
    <div class="container hero__copy">
      <span class="badge">Windows alpha · {RELEASE.tag}</span>
      <h1 class="h-1">Try Dynodoc</h1>
      <p class="lede">
        The desktop app manages repositories, changes, history, reviews and sync. Edit in Word,
        Excel, PowerPoint, Google Docs, Sheets or Slides through their Dynodoc add-in or extension.
        Open the full platform in your browser.
      </p>
      <p class="lede">
        Windows 0.1 alpha 1 is the latest development release; Linux remains on 0.0.1c pre-alpha.
        The desktop manager keeps repositories and their history in a database on your computer,
        with no server account needed for local work. Connect to a server when you want to sync, or
        try a sample workspace in your browser.
      </p>
      <div class="actions">
        <Btn
          href={heroPlatform ? heroPlatform.primary.url : '#download'}
          size="lg"
          data-dl-hero
          arrow
          >{heroPlatform ? `Download for ${heroPlatform.name}` : 'Download the desktop app'}</Btn
        >
        <Btn href={SAMPLE_WORKSPACE_URL} variant="secondary" size="lg">Start a sample workspace</Btn
        >
      </div>
    </div>
  </header>

  <Section
    id="browser"
    title="In your browser"
    lede="The hosted pre-alpha server makes you a workspace of your own, already filled with a sample organisation."
  >
    <div class="cols" data-reveal>
      <div class="card">
        <h3>A sample workspace</h3>
        <ul>
          <li>
            Three projects (a team handbook with its budget and review deck, a field guide and a
            grant application) with their history, drafts, change requests, reviews and issues.
          </li>
          <li>
            The colleagues in it are invented and use example.org addresses. Nothing is emailed.
          </li>
          <li>
            It is yours alone and is removed 24 hours after you start it. Do not put anything
            private or confidential in it.
          </li>
          <li>No account or address is asked for. Six workspaces per address per hour.</li>
        </ul>
        <Btn href={SAMPLE_WORKSPACE_URL} arrow>Start a sample workspace</Btn>
      </div>
      <div class="card">
        <h3>Your own account</h3>
        <p>
          Sign in with an emailed link or with Google and create your own projects. The hosted
          workspace is a test deployment with no paid plan; the
          <a href="/terms">terms</a> and <a href="/privacy">privacy policy</a> are drafts.
        </p>
        <Btn href={workspace} variant="secondary" arrow>Open the workspace</Btn>
      </div>
    </div>
  </Section>

  <Section
    id="download"
    title="Desktop workflow manager"
    lede="Manage document repositories, history, drafts, change requests, reviews and synchronization. Keep editing in Office or Google editors through the Dynodoc add-ins and extensions, and open the full web workspace in your browser. Windows alpha also connects to supported installed coding-agent sign-ins alongside API-key providers. macOS is not built yet."
  >
    <div data-reveal>
      <DownloadCards {os} />
    </div>
    <ul class="facts" data-reveal>
      <li>
        <strong>Choose what connects.</strong> Keep repositories local or connect a server for sync. Agent
        requests use the provider or supported installed agent you select. API keys stay in the operating
        system's credential store; installed agents manage their own sign-in. The Windows installer is
        unsigned. Each platform also offers engine-cli as a separate download.
      </li>
      <li>
        <strong>No auto-update, no telemetry of its own.</strong> Newer releases appear on the
        <a href={RELEASE.page}>release page</a>, with notes on what works and what does not.
      </li>
    </ul>
  </Section>
</main>

<style>
  .hero {
    border-bottom: 1px solid var(--color-border);
  }
  .hero__copy {
    display: grid;
    gap: 22px;
    justify-items: start;
    padding-block: clamp(48px, 8vw, 104px) clamp(40px, 6vw, 80px);
  }
  .hero__copy .lede {
    max-width: 62ch;
  }
  .badge {
    display: inline-flex;
    padding: 2px 10px;
    border: 1px solid var(--color-border-strong);
    border-radius: 999px;
    color: var(--color-ink-soft);
    font-family: var(--site-font-mono);
    font-size: 0.75rem;
    letter-spacing: 0.04em;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }
  .cols {
    display: grid;
    gap: 20px;
  }
  @media (min-width: 860px) {
    .cols {
      grid-template-columns: 1.4fr 1fr;
    }
  }
  .card {
    display: grid;
    gap: 14px;
    align-content: start;
    justify-items: start;
    padding: 24px;
    border: 1px solid var(--color-border);
    border-radius: var(--site-radius);
    background: var(--color-surface-raised);
  }
  .card h3 {
    margin: 0;
    font-size: 1.125rem;
  }
  .card ul {
    display: grid;
    gap: 8px;
    margin: 0;
    padding-left: 18px;
    color: var(--color-ink-soft);
  }
  .card p {
    margin: 0;
    color: var(--color-ink-soft);
  }
  .facts {
    display: grid;
    gap: 10px;
    max-width: 980px;
    margin: 28px auto 0;
    padding-left: 18px;
    color: var(--color-ink-soft);
  }
</style>
