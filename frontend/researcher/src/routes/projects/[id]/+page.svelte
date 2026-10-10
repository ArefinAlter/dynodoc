<script lang="ts">
  import { page } from '$app/stores';
  import ProjectRepository from '$lib/components/product/ProjectRepository.svelte';
</script>

{#key $page.url.pathname}<ProjectRepository />{/key}
