<script lang="ts">
  import AppSigninCard from '$lib/components/AppSigninCard.svelte';
  import type { PageData } from './$types';

  let { data }: { data: PageData } = $props();
  let step = $state<'ask' | 'working' | 'handed' | 'cancelled'>('ask');
  let error = $state('');
  let link = $state('');

  // Continue is a real action by the person: nothing is made until it is pressed, and the
  // request goes to this site only (the server refuses any other origin).
  async function carryOn() {
    if (data.phase !== 'ready') return;
    step = 'working';
    error = '';
    try {
      const response = await fetch('/auth/app-signin', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(data.request)
      });
      const answer = await response.json().catch(() => ({}));
      if (!response.ok || typeof answer.url !== 'string') throw new Error(answer.error);
      link = answer.url;
      step = 'handed';
      // The app takes over from here; if nothing happens, the button below does the same.
      window.location.assign(link);
    } catch (e) {
      step = 'ask';
      error = e instanceof Error && e.message ? e.message : 'Please try again.';
    }
  }
</script>

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
{:else if step === 'cancelled'}
  <AppSigninCard title="Cancelled">
    <p>Nothing was shared with the app. You can close this tab.</p>
    <div class="actions"><a class="button" href="/workspace">Go to my workspace</a></div>
  </AppSigninCard>
{:else if step === 'handed'}
  <AppSigninCard title="Opening the app">
    <p class="notice" role="status">
      Your {data.device} should now open the Dynodoc app and finish signing in.
    </p>
    <p class="fine">If nothing happened, make sure the app is installed, then press the button.</p>
    <div class="actions"><a class="button" href={link}>Open the Dynodoc app</a></div>
    <p class="fine">You can close this tab once the app shows your workspace.</p>
  </AppSigninCard>
{:else}
  <AppSigninCard title="Sign in to the Dynodoc app">
    <p class="who">You are signed in here as <strong>{data.email}</strong>.</p>
    <p>
      You are signing in the Dynodoc app on your <strong>{data.device}</strong> as this account.
    </p>
    <p class="fine">
      Only continue if you just chose <em>Sign in with your browser</em> in the app. If you did not, press
      Cancel: someone else may have sent you here.
    </p>
    {#if error}<p role="alert" class="error">{error}</p>{/if}
    <div class="actions">
      <button class="primary" onclick={carryOn} disabled={step === 'working'}
        >{step === 'working' ? 'Preparing…' : 'Continue'}</button
      >
      <button onclick={() => (step = 'cancelled')} disabled={step === 'working'}>Cancel</button>
    </div>
  </AppSigninCard>
{/if}
