<script lang="ts">
  import Dino from '$lib/art/Dino.svelte';
  import Egg from '$lib/art/Egg.svelte';
  import { Btn } from '$lib/components/site';

  let { workspace }: { workspace: string } = $props();

  const links = [
    { href: '/features', label: 'Features' },
    { href: '/add-ins', label: 'Add-ins' },
    { href: '/security', label: 'Security' },
    { href: '/why-dynodoc', label: 'Why provenance' },
    { href: '/roadmap', label: 'Roadmap' }
  ];
</script>

<section id="start" class="closing tone-ink" aria-labelledby="start-title">
  <div class="container grid">
    <div class="copy">
      <h2 id="start-title" class="h-1 title">Open the hosted workspace</h2>
      <p class="lede">
        Create a project, add a Word, Excel or PowerPoint file, and open the first change request.
        The hosted workspace is a test deployment with no paid plan.
      </p>
      <div class="actions">
        <Btn href={workspace} size="lg" arrow>Open workspace</Btn>
        <Btn href="/add-ins" variant="secondary" size="lg">Get the add-ins</Btn>
      </div>
      <nav aria-label="More about Dynodoc">
        <ul class="links">
          {#each links as link (link.href)}
            <li><a class="link-sweep" href={link.href}>{link.label}</a></li>
          {/each}
        </ul>
      </nav>
    </div>
    <div class="egg" aria-hidden="true">
      <Dino name="egg" art={Egg} />
    </div>
  </div>
</section>

<style>
  .closing {
    border-block: 1px solid var(--color-border);
    overflow: clip;
  }
  .grid {
    display: grid;
    gap: 40px;
    align-items: center;
    padding-block: var(--site-section-y, 96px);
  }
  .copy {
    display: grid;
    justify-items: start;
    gap: 20px;
    min-width: 0;
  }
  .title {
    max-width: 14ch;
  }
  .lede {
    max-width: 48ch;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin-top: 8px;
  }
  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 24px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .links a {
    display: inline-block;
    padding-block: 6px;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
  }
  .links a:hover {
    color: var(--color-ink-strong);
  }
  .egg {
    width: min(70%, 340px);
    justify-self: center;
    color: var(--color-ink);
    --art-accent: var(--color-accent);
  }
  @media (min-width: 900px) {
    .grid {
      grid-template-columns: minmax(0, 7fr) minmax(0, 4fr);
      gap: clamp(32px, 6vw, 96px);
    }
    .egg {
      width: 100%;
      max-width: 400px;
      justify-self: end;
    }
  }
</style>
