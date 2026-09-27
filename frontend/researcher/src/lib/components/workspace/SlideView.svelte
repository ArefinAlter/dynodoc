<script lang="ts">
  import SlideShape from './SlideShape.svelte';
  import { themes, type Slide, type ThemeName } from '$lib/workspace/presentation';
  let { slide, theme, ratio }: { slide: Slide; theme: ThemeName; ratio: number } = $props();
</script>

<div
  class="slide"
  style:aspect-ratio={ratio}
  style:background-color={slide.background || themes[theme].background}
>
  {#each slide.shapes as shape (shape.id)}<SlideShape {shape} {theme} />{/each}
</div>

<style>
  .slide {
    position: relative;
    width: 100%;
    container-type: size;
    overflow: hidden;
    user-select: none;
  }
</style>
