<script lang="ts">
  import { onMount } from 'svelte';
  import AppSigninCard from '$lib/components/AppSigninCard.svelte';
  import type { ActionData, PageData } from './$types';

  let { data, form }: { data: PageData; form: ActionData } = $props();
  let cancelled = $state(false);

  // The app link only exists once the form has been submitted and answered. Navigating to it from
  // script may be refused by the browser (it asks for a recent tap); the button below is the same
  // link and always works.
  onMount(() => {
    if (form && 'url' in form && form.url) window.location.assign(form.url);
  });

  // The two refusals of the confirmation step say what failed in terms that help a person and
  // nobody else: a browser that does not report the click (WebKit sends no `Sec-Fetch-User`; some
  // older ones send no `Sec-Fetch-*` at all) cannot be used for this step, and neither can a request
  // that was not the person's own click. They look the same from the server, so the words cover both.
  const PROBLEMS: Record<string, string> = {
    browser:
      'This browser does not support the confirmation step used for signing in the app, so no sign-in was prepared. Use Chrome, Edge or Firefox, then start again from the app.',
    blocked:
      'This request could not be confirmed as your own click on this page, so no sign-in was prepared. Press Continue yourself; if that does not work this browser does not support the confirmation step (use Chrome, Edge or Firefox), or go back to the app and start again.',
    'signed-out': 'Your sign-in here has ended. Sign in again, then start again from the app.',
    busy: 'Too many sign-in attempts. Wait a minute, then try again.',
    failed: 'The sign-in could not be prepared. Please try again.',
    invalid: 'This sign-in link is not valid. Go back to the app and start again.'
  };
</script>

<svelte:head>
  <!-- A reverse proxy that replaces the Referrer-Policy header with `no-referrer` (both shipped ones
       do) would make the browser send `Origin: null` with the Continue form, which the form check
       refuses. The page's own policy wins over the header, so the form carries its real Origin. -->
  <meta name="referrer" content="same-origin" />
</svelte:head>

{#if data.phase === 'invalid'}
  <AppSigninCard title="This sign-in link is not valid">
    <p>Go back to the Dynodoc app and choose <strong>Sign in with your browser</strong> again.</p>
    <p class="fine">Nothing was signed in and nothing was shared.</p>
  </AppSigninCard>
{:else if data.phase === 'no-method'}
  <AppSigninCard title="No sign-in method is turned on">
    <p>
      This installation has no way to sign in at the moment (emailed links and Google are both off).
      Ask your administrator to turn one on, then try again from the app.
    </p>
  </AppSigninCard>
{:else if cancelled}
  <AppSigninCard title="Cancelled">
    <p>Nothing was shared with the app. You can close this tab.</p>
    <div class="actions"><a class="button" href="/workspace">Go to my workspace</a></div>
  </AppSigninCard>
{:else if form && 'url' in form && form.url}
  <AppSigninCard title="Opening the app">
    <p class="notice" role="status">
      Your {data.device} should now open the Dynodoc app and finish signing in.
    </p>
    {#if form.loopback}
      <p class="fine">
        This page hands the sign-in to the Dynodoc app that is waiting on this computer. If nothing
        happened, check that the app is still open on its sign-in screen, then press the button.
      </p>
    {:else}
      <p class="fine">
        If nothing happened, make sure the app is installed, then press the button.
      </p>
    {/if}
    <div class="actions"><a class="button" href={form.url}>Open the Dynodoc app</a></div>
    <p class="fine">You can close this tab once the app shows your workspace.</p>
  </AppSigninCard>
{:else}
  <AppSigninCard title="Sign in to the Dynodoc app">
    <p class="who">You are signed in here as <strong>{data.email}</strong>.</p>
    <p>
      You are signing in the Dynodoc app on your <strong>{data.device}</strong> as this account.
    </p>
    <p class="who" data-testid="pairing">
      Pairing code <strong>{data.pairing}</strong>. The app shows the same four characters. If it
      does not, press Cancel: this request is not the one you started.
    </p>
    <p class="fine">
      Only continue if you just chose <em>Sign in with your browser</em> in the app. If you did not, press
      Cancel: someone else may have sent you here.
    </p>
    {#if form && 'problem' in form && form.problem}
      <p role="alert" class="error">{PROBLEMS[form.problem] ?? PROBLEMS.failed}</p>
    {/if}
    <!-- A real form, submitted by a real click: the server accepts nothing else (see issueForForm). -->
    <form method="POST" class="actions">
      <button class="primary" type="submit">Continue</button>
      <button type="button" onclick={() => (cancelled = true)}>Cancel</button>
    </form>
  </AppSigninCard>
{/if}
