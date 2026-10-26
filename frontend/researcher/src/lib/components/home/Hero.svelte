<script lang="ts">
  import Dino from '$lib/art/Dino.svelte';
  import Rex from '$lib/art/Rex.svelte';
  import { AddinMock } from '$lib/components/mock';
  import { Btn, spotlight } from '$lib/components/site';
  import { parallax } from './parallax';

  let { workspace }: { workspace: string } = $props();
</script>

<section
  class="hero dotgrid dotgrid--fade-b spotlight"
  aria-labelledby="hero-title"
  use:spotlight
  use:parallax
>
  <div class="container grid">
    <div class="copy">
      <h1 id="hero-title" class="title">
        Version control for <span class="hl">documents, spreadsheets and presentations</span>
      </h1>
      <p class="lede">
        Dynodoc records edits to paragraph text, cell values and slide text in Word, Docs, Excel,
        Sheets, PowerPoint and Slides, routes them through review, and keeps the merged result in a
        hash-chained history that can be checked.
      </p>
      <div class="actions">
        <Btn href="/try" size="lg" arrow>Try the pre-alpha</Btn>
        <Btn href={workspace} variant="secondary" size="lg">Open workspace</Btn>
      </div>
      <p class="note">
        Pre-alpha 0.0.1a: a sample workspace in your browser, or the desktop app for Windows and
        Linux. Add-ins ship as test installs; marketplace listings are not live. The engine, API and
        command-line tools are open source (MIT).
      </p>
    </div>

    <div class="stage">
      <div class="plate"><Dino name="rex" art={Rex} /></div>
      <div class="win"><AddinMock host="word" /></div>
      <p class="cap">Illustration with sample content, not a screenshot of a real file.</p>
    </div>
  </div>
</section>

<style>
  .hero {
    padding-block: clamp(32px, 5vw, 72px) clamp(40px, 6vw, 88px);
    overflow: clip;
  }
  .grid {
    display: grid;
    gap: clamp(36px, 5vw, 56px);
    align-items: center;
  }
  .copy {
    display: grid;
    justify-items: start;
    gap: 22px;
    min-width: 0;
  }
  .title {
    font-size: clamp(2.375rem, 1.1rem + 3.4vw, 3.75rem);
    line-height: 1.05;
    letter-spacing: -0.04em;
    font-weight: 520;
  }
  .lede {
    max-width: 46ch;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
    margin-top: 6px;
  }
  .note {
    margin: 0;
    max-width: 52ch;
    color: var(--color-muted);
    font-size: 0.875rem;
    line-height: 1.55;
  }

  .stage {
    position: relative;
    min-width: 0;
  }
  .plate {
    position: relative;
    z-index: 0;
    width: min(76%, 460px);
    margin: 0 0 -2px auto;
    color: var(--color-ink);
    --art-accent: var(--color-accent);
  }
  .win {
    position: relative;
    z-index: 1;
  }
  .cap {
    margin: 10px 0 0;
    color: var(--color-muted);
    font-size: 0.8125rem;
    text-align: right;
  }

  @media (min-width: 1000px) {
    .grid {
      grid-template-columns: minmax(0, 5fr) minmax(0, 6fr);
      gap: clamp(32px, 4vw, 72px);
    }
  }
  @media (max-width: 999px) {
    .plate {
      width: min(62%, 360px);
      margin-right: 4%;
    }
    .title {
      max-width: 20ch;
    }
  }

  /* Entrance and pointer parallax: transform only, off under reduced motion. */
  @media (prefers-reduced-motion: no-preference) {
    .copy > :global(*) {
      animation: rise 0.75s var(--site-ease) both;
    }
    .copy > :global(:nth-child(2)) {
      animation-delay: 70ms;
    }
    .copy > :global(:nth-child(3)) {
      animation-delay: 140ms;
    }
    .copy > :global(:nth-child(4)) {
      animation-delay: 210ms;
    }
    .copy > :global(:nth-child(5)) {
      animation-delay: 280ms;
    }
    .win {
      animation: rise-lg 0.95s var(--site-ease) 0.2s both;
    }
    @keyframes rise {
      from {
        transform: translateY(14px);
      }
    }
    @keyframes rise-lg {
      from {
        transform: translateY(28px) scale(0.985);
      }
    }
  }
  @media (prefers-reduced-motion: no-preference) and (hover: hover) and (pointer: fine) {
    .plate {
      translate: calc(var(--px, 0) * 12px) calc(var(--py, 0) * 7px);
      transition: translate 0.7s var(--site-ease);
    }
    .win {
      translate: calc(var(--px, 0) * -6px) calc(var(--py, 0) * -4px);
      transition: translate 0.7s var(--site-ease);
    }
  }
</style>
