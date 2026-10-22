<script lang="ts">
  import { onMount } from 'svelte';
  import { parseFinishFragment } from '$lib/app-signin';
  import AppSigninCard from '$lib/components/AppSigninCard.svelte';
  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();

  // The Dynodoc app opens this page in its own window and then, once the page says it is ready,
  // hands it the one-time code, its state and its verifier as the address FRAGMENT
  // (`#code=..&state=..&verifier=..`), by setting `location.hash` from the shell (a script it
  // runs in this page; see apps/desktop/src-tauri/src/finish_hash.js). So the secrets are in no
  // address that was ever requested, logged by the web view or kept in its history; a fragment is
  // never sent to a server or in a Referer header either, so nothing secret is in this page's HTML.
  //
  // What makes it safe to act on a fragment (decision 017, "a crafted finish link"):
  //  - only in the app's own window: the server saw its User-Agent token (`data.app`) and the
  //    shell's bridge object exists here (`__TAURI_INTERNALS__`); any other browser is told to use
  //    the app and the page never reads a fragment;
  //  - a fragment that is already in the address when the page starts is ignored and removed: only
  //    a CHANGE of the fragment after the page has said it is ready is acted on, which is how the
  //    shell hands it over (and how the app hands over a second try while the page still shows);
  //  - the fragment is removed from the address bar and the history before anything else happens.
  // With no fragment the page waits a few seconds for one, then says it did not finish; a fragment
  // that is not three valid values fails at once.
  const WAIT_MS = 6000;
  let phase = $state<'waiting' | 'working' | 'failed' | 'not-app'>('waiting');
  // Outside the app the page is the "open the app" card from the first byte of HTML.
  const view = $derived(data.app ? phase : 'not-app');
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
    const bridge = (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
    // A fragment present at load is never acted on, whoever put it there.
    if (window.location.hash)
      window.history.replaceState(window.history.state, '', window.location.pathname);
    // Never inside a frame: a plain link whose target is the frame could feed it a fragment, and in
    // the app's own window the frame would carry the app's token and bridge. (The headers already
    // refuse to be framed; this is the same rule where a header does not reach.)
    const framed = window.top !== window;
    if (!data.app || !bridge || framed) {
      phase = 'not-app';
      return;
    }
    const giveUp = setTimeout(() => {
      if (phase === 'waiting') phase = 'failed';
    }, WAIT_MS);
    const again = () => {
      if (window.location.hash) void finishSignIn();
    };
    window.addEventListener('hashchange', again);
    // Ready: from now on a change of the fragment is the shell handing something over.
    document.documentElement.setAttribute('data-finish-ready', '1');
    return () => {
      clearTimeout(giveUp);
      window.removeEventListener('hashchange', again);
      document.documentElement.removeAttribute('data-finish-ready');
    };
  });
</script>

{#if view === 'not-app'}
  <AppSigninCard title="Open this from the Dynodoc app">
    <p>
      This page finishes signing in the Dynodoc app and works only inside it. Open the app and
      choose
      <strong>Sign in with your browser</strong>.
    </p>
    <p class="fine">Nothing was signed in.</p>
  </AppSigninCard>
{:else if view === 'failed'}
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
