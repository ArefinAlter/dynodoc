<script lang="ts">
  // Public marketing pages: Paper & Ink design system (docs/SITE-DESIGN.md). The
  // workspace, admin, login and shared-view routes live outside this group and keep the
  // application styling.
  import '$lib/styles/site.css';
  import SiteHeader from '$lib/components/marketing/SiteHeader.svelte';
  import SiteFooter from '$lib/components/marketing/SiteFooter.svelte';
  import { theme } from '$lib/theme.svelte';

  let { children } = $props();
</script>

<svelte:head>
  <!-- Browser chrome follows the paper in day and night. -->
  <meta name="theme-color" content={theme.resolved === 'dark' ? '#12120f' : '#f6f3ea'} />
</svelte:head>

<div class="site">
  <SiteHeader />
  {@render children()}
  <SiteFooter />
</div>
