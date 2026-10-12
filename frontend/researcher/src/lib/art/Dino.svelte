<script lang="ts">
  // Pencil + halftone dinosaur plate with an in-view "draw" animation.
  //
  // The art itself is generated (see scripts/gen-dino-art.mjs) and uses only
  // `currentColor` plus `var(--art-accent, #1f7a4d)`, so dark mode is just a
  // matter of setting `color` (and optionally `--art-accent`) on a parent.
  //
  // Progressive enhancement: server-rendered and no-JS output is the finished
  // art. Only after hydration do we add `armed` (hides the layers) and, once the
  // plate scrolls into view, `data-in` (plays the animation). Nothing is armed
  // for `animate={false}` or `prefers-reduced-motion: reduce`.
  import { onMount, type Component } from 'svelte';
  import type { DinoName } from './index';

  type Plate = Component<{
    class?: string;
    role?: 'img';
    'aria-hidden'?: 'true';
    'aria-label'?: string;
  }>;

  type Props = {
    name: DinoName;
    /** The plate component, imported by the page (`import Rex from '$lib/art/Rex.svelte'`) so each route bundles only the art it shows. */
    art: Plate;
    class?: string;
    animate?: boolean;
    /** When set the plate becomes an accessible image; otherwise it is decorative. */
    label?: string;
  };

  let { name, art: Art, class: klass = '', animate = true, label }: Props = $props();

  let host: HTMLElement | undefined = $state();
  let armed = $state(false);
  let inView = $state(false);

  onMount(() => {
    if (!animate || typeof window === 'undefined') return;
    if (window.matchMedia?.('(prefers-reduced-motion: reduce)').matches) return;
    if (!('IntersectionObserver' in window) || !host) return;
    armed = true;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) {
          inView = true;
          io.disconnect();
        }
      },
      { threshold: 0.2, rootMargin: '0px 0px -6% 0px' }
    );
    io.observe(host);
    return () => io.disconnect();
  });

  const live = $derived(armed && animate);
</script>

<div
  class="dino {klass}"
  class:armed={live}
  data-art={name}
  data-in={live && inView ? '' : undefined}
  bind:this={host}
>
  {#if label}
    <Art class="dino-svg" role="img" aria-label={label} />
  {:else}
    <Art class="dino-svg" aria-hidden="true" />
  {/if}
</div>

<style>
  .dino {
    display: block;
    width: 100%;
    color: inherit;
  }
  .dino :global(.dino-svg) {
    display: block;
    width: 100%;
    height: auto;
    overflow: visible;
    transform-origin: 50% 100%;
  }

  /* ---- armed: hidden until scrolled into view ------------------------------ */
  .dino.armed :global(.ol) {
    stroke-dasharray: 1 2;
    stroke-dashoffset: 1.02;
  }
  .dino.armed :global(.t),
  .dino.armed :global(.ht),
  .dino.armed :global(.ow),
  .dino.armed :global(.m) {
    opacity: 0;
    transform-box: fill-box;
    transform-origin: 50% 70%;
    transform: scale(0.97);
  }

  /* ---- in view: draw, then fade layers in, accent last -------------------- */
  .dino.armed[data-in] :global(.ol) {
    animation: dino-draw 1.1s cubic-bezier(0.45, 0.05, 0.2, 1) both;
  }
  .dino.armed[data-in] :global(.ol:nth-of-type(3n + 2)) {
    animation-delay: 0.08s;
  }
  .dino.armed[data-in] :global(.ol:nth-of-type(3n + 3)) {
    animation-delay: 0.16s;
  }
  .dino.armed[data-in] :global(.t),
  .dino.armed[data-in] :global(.ht),
  .dino.armed[data-in] :global(.ow),
  .dino.armed[data-in] :global(.m) {
    animation: dino-in 0.7s ease-out both;
  }
  .dino.armed[data-in] :global(.ht) {
    animation-delay: 0.55s;
  }
  .dino.armed[data-in] :global(.t1) {
    animation-delay: 0.6s;
  }
  .dino.armed[data-in] :global(.t2) {
    animation-delay: 0.72s;
  }
  .dino.armed[data-in] :global(.t3) {
    animation-delay: 0.84s;
  }
  .dino.armed[data-in] :global(.ow) {
    animation-delay: 0.8s;
  }
  .dino.armed[data-in] :global(.m) {
    animation-delay: 1s;
  }
  /* accent last */
  .dino.armed[data-in] :global(.a),
  .dino.armed[data-in] :global(.ol.a) {
    animation-delay: 1.15s;
  }

  /* ---- hover / focus: a small wobble ---------------------------------------- */
  .dino.armed:hover :global(.dino-svg),
  .dino.armed:focus-within :global(.dino-svg) {
    animation: dino-wobble 0.9s ease-in-out;
  }

  /* ---- per-plate micro-motions (after the intro, only when armed) ---------- */
  .dino.armed[data-in] :global(.part-page) {
    transform-box: fill-box;
    transform-origin: 50% 0%;
    animation: dino-bob 4.2s ease-in-out 1.6s infinite alternate;
  }
  .dino.armed[data-in][data-art='ptero'] :global(.part-page) {
    animation-name: dino-swing;
    animation-duration: 3.6s;
  }
  .dino.armed[data-in] :global(.part-wingL),
  .dino.armed[data-in] :global(.part-wingR) {
    transform-box: fill-box;
    animation: dino-flap 1.9s ease-in-out 1.4s infinite alternate;
  }
  .dino.armed[data-in] :global(.part-wingR) {
    transform-origin: 0% 100%;
  }
  .dino.armed[data-in] :global(.part-wingL) {
    transform-origin: 100% 100%;
  }
  .dino.armed[data-in][data-art='egg'] :global(.part-baby) {
    transform-box: fill-box;
    transform-origin: 50% 100%;
    animation: dino-peek 3.4s ease-in-out 1.5s infinite alternate;
  }
  .dino.armed[data-in][data-art='egg'] :global(.part-cap) {
    transform-box: fill-box;
    transform-origin: 50% 50%;
    animation: dino-bob 3.8s ease-in-out 1.2s infinite alternate;
  }
  .dino.armed[data-in][data-art='egg'] :global(.part-egg) {
    transform-box: fill-box;
    transform-origin: 50% 100%;
    animation: dino-rock 5s ease-in-out 1.8s infinite alternate;
  }
  .dino.armed[data-in][data-art='trike'] :global(.part-head) {
    transform-box: fill-box;
    transform-origin: 85% 75%;
    animation: dino-nod 3.2s ease-in-out 1.6s infinite alternate;
  }
  .dino.armed[data-in][data-art='stego'] :global(.part-plates) {
    transform-box: fill-box;
    transform-origin: 50% 100%;
    animation: dino-breathe 4.6s ease-in-out 1.6s infinite alternate;
  }

  @keyframes -global-dino-draw {
    to {
      stroke-dashoffset: 0;
    }
  }
  @keyframes -global-dino-in {
    to {
      opacity: 1;
      transform: scale(1);
    }
  }
  @keyframes -global-dino-wobble {
    0% {
      transform: rotate(0deg);
    }
    30% {
      transform: rotate(-1.2deg);
    }
    65% {
      transform: rotate(0.8deg);
    }
    100% {
      transform: rotate(0deg);
    }
  }
  @keyframes -global-dino-bob {
    from {
      transform: translateY(0);
    }
    to {
      transform: translateY(3px);
    }
  }
  @keyframes -global-dino-swing {
    from {
      transform: rotate(-2.2deg);
    }
    to {
      transform: rotate(2.2deg);
    }
  }
  @keyframes -global-dino-flap {
    from {
      transform: scaleY(0.93);
    }
    to {
      transform: scaleY(1.05);
    }
  }
  @keyframes -global-dino-peek {
    from {
      transform: translateY(2px) rotate(-1.5deg);
    }
    to {
      transform: translateY(-3px) rotate(1.5deg);
    }
  }
  @keyframes -global-dino-rock {
    from {
      transform: rotate(-0.8deg);
    }
    to {
      transform: rotate(0.8deg);
    }
  }
  @keyframes -global-dino-nod {
    from {
      transform: rotate(-1.4deg);
    }
    to {
      transform: rotate(1.4deg);
    }
  }
  @keyframes -global-dino-breathe {
    from {
      transform: scaleY(1);
    }
    to {
      transform: scaleY(1.025);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .dino :global(*),
    .dino :global(.dino-svg) {
      animation: none !important;
      transition: none !important;
    }
    .dino.armed :global(.ol) {
      stroke-dasharray: none;
      stroke-dashoffset: 0;
    }
    .dino.armed :global(.t),
    .dino.armed :global(.ht),
    .dino.armed :global(.ow),
    .dino.armed :global(.m) {
      opacity: 1;
      transform: none;
    }
  }
</style>
