<script lang="ts">
  import { onMount } from 'svelte';
  import { parseFinishFragment } from '$lib/app-signin';
  import AppSigninCard from '$lib/components/AppSigninCard.svelte';

  // The Dynodoc app opens this page in its own window and then, once the page has loaded, hands
  // it the one-time code, its state and its verifier as the address FRAGMENT
  // (`#code=..&state=..&verifier=..`), by setting `location.hash` from the shell. So the secrets are
  // in no address that was ever requested, logged by the web view or kept in its history before
  // this page runs; a fragment is never sent to a server or in a Referer header either, so nothing
  // secret is in this page's HTML. The script below removes the fragment from the address bar and
  // the history before doing anything else with it.
  //
  // Two cases are handled alike: the fragment is already there when the page starts (a browser or a
  // test that opened `/app-signin/finish#...` directly), or it arrives later as a change of the
  // fragment (the shell's way, and the app opening this page a second time while it is showing,
  // which is no page load). With no fragment at all the page waits a few seconds for one, then says
  // it did not finish; a fragment that is not three valid values fails at once.
  const WAIT_MS = 6000;
  let phase = $state<'waiting' | 'working' | 'failed'>('waiting');
  let latest = 0;

  async function finishSignIn() {
    const mine = ++latest;
    const hadFragment = window.location.hash !== '';
    const finish = parseFinishFragment(window.location.hash);
    // SvelteKit's own replaceState cannot be used this early in the first page; this keeps its
    // history state and drops only the fragment.
    if (hadFragment)
      window.history.replaceState(window.history.state, '', window.location.pathname);
    if (!finish) {
      if (hadFragment) phase = 'failed';
      return;
    }
    phase = 'working';
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
      if (mine === latest) phase = 'failed';
    }
  }

  onMount(() => {
    void finishSignIn();
    const giveUp = setTimeout(() => {
      if (phase === 'waiting') phase = 'failed';
    }, WAIT_MS);
    const again = () => {
      if (window.location.hash) void finishSignIn();
    };
    window.addEventListener('hashchange', again);
    return () => {
      clearTimeout(giveUp);
      window.removeEventListener('hashchange', again);
    };
  });
</script>

{#if phase === 'failed'}
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
