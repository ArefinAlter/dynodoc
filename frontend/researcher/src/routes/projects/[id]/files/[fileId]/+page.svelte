<script lang="ts">
  import { page } from '$app/stores';
  import ProjectFile from '$lib/components/product/ProjectFile.svelte';
</script>

{#key $page.url.pathname}<ProjectFile />{/key}
