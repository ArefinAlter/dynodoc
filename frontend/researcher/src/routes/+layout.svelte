<script lang="ts">
  import '../app.css';
  import { goto } from '$app/navigation';
  import { setUnauthorizedHandler } from '$lib/api/client';
  import ToastHost from '$lib/components/ToastHost.svelte';

  let { children } = $props();

  // When a 401 can't be recovered (refresh failed), the API client clears the session
  // and calls this handler; route the user to login.
  setUnauthorizedHandler(() => {
    void goto('/login');
  });
</script>

{@render children()}
<ToastHost />
