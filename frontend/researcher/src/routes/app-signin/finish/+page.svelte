<script lang="ts">
  import { onMount } from 'svelte';
  import { parseFinishFragment } from '$lib/app-signin';
  import AppSigninCard from '$lib/components/AppSigninCard.svelte';

  // The Dynodoc app opens this page in its own window with the one-time code, its state and
  // its verifier in the address FRAGMENT (`#code=..&state=..&verifier=..`). A fragment is never
  // sent to a server, a log or a Referer header, so nothing secret is in this page's HTML, and
  // the script below removes it from the address bar and the history before doing anything else.
  //
  // The app can open this page again while it is still showing (a second try after a failed
  // one). A browser then changes only the fragment and runs no page load, so the same work is
  // done on `hashchange` too.
  let failed = $state(false);
  let latest = 0;

  async function finishSignIn() {
    const mine = ++latest;
    const finish = parseFinishFragment(window.location.hash);
    // SvelteKit's own replaceState cannot be used this early in the first page; this keeps its
    // history state and drops only the fragment.
    window.history.replaceState(window.history.state, '', window.location.pathname);
    failed = false;
    if (!finish) {
      failed = true;
      return;
    }
    try {
      const response = await fetch('/auth/app-signin/redeem', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(finish)
      });
      if (!response.ok) throw new Error('refused');
      // A whole navigation, so the new session is read from the first request.
      window.location.replace('/workspace');
    } catch {
      // Only the newest attempt may speak.
      if (mine === latest) failed = true;
    }
  }

  onMount(() => {
    void finishSignIn();
    const again = () => {
      if (window.location.hash) void finishSignIn();
    };
    window.addEventListener('hashchange', again);
    return () => window.removeEventListener('hashchange', again);
  });
</script>

{#if failed}
  <AppSigninCard title="Signing in did not finish">
    <p role="alert" class="error">
      This sign-in could not be completed. It may have expired or already been used.
    </p>
    <p>Go back to the Dynodoc app and choose <strong>Sign in with your browser</strong> again.</p>
  </AppSigninCard>
{:else}
  <AppSigninCard title="Finishing sign-in">
    <p class="notice" role="status">Signing you in…</p>
  </AppSigninCard>
{/if}
