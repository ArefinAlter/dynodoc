<script lang="ts">
  import { page } from '$app/stores';
  import ProfileView from '$lib/components/product/ProfileView.svelte';
</script>

{#key $page.params.id}<ProfileView id={$page.params.id} />{/key}
