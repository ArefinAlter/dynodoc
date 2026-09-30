<script lang="ts">
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { session } from '$lib/stores/session.svelte';
  import { rememberNext, safeNext, takeRememberedNext } from '$lib/auth-next';
  import { ArrowRight, ArrowLeft, Mail, CheckCircle2 } from '@lucide/svelte';
  import ThemeToggle from '$lib/components/ThemeToggle.svelte';
  // A protected page sent the visitor here; return there once they have signed in.
  const next = $derived(safeNext($page.url.searchParams.get('next')));
  let email = $state(''),
    name = $state(''),
    message = $state(''),
    error = $state(''),
    busy = $state(false),
    sent = $state(false),
    devToken = $state('');
  let options = $state({
    google: false,
    email: false,
    development: false,
    destination: '/workspace'
  });
  onMount(async () => {
    if (next) rememberNext(next);
    const res = await fetch('/auth/options');
    if (res.ok) options = await res.json();
  });
  function destination() {
    if ($page.url.searchParams.get('next') === 'admin' || options.destination === '/admin')
      return '/admin';
    const remembered = takeRememberedNext();
    return next || remembered || '/workspace';
  }
  async function requestLink(event: SubmitEvent) {
    event.preventDefault();
    busy = true;
    error = '';
    try {
      const res = await fetch('/auth/email', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ email, display_name: name })
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error);
      devToken = data.development_token || '';
      sent = true;
      message = 'Check your inbox. Your sign-in link is valid for 15 minutes.';
    } catch (e) {
      error = e instanceof Error ? e.message : 'Please try again.';
    } finally {
      busy = false;
    }
  }
  async function verify(token: string) {
    busy = true;
    error = '';
    try {
      const res = await fetch('/auth/verify', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ token })
      });
      const data = await res.json();
      if (!res.ok) throw new Error(data.error);
      session.authenticated = true;
      await goto(destination());
    } catch (e) {
      error = e instanceof Error ? e.message : 'Please request another link.';
    } finally {
      busy = false;
    }
  }
</script>

<svelte:head><title>Sign in · Dynodoc</title><meta name="robots" content="noindex" /></svelte:head>
<main class="auth-page">
  <a href="https://dynodoc.online" class="back"><ArrowLeft size={16} /> Back to Dynodoc</a>
  <div class="theme-corner"><ThemeToggle /></div>
  <section class="card">
    <a class="brand" href="https://dynodoc.online"
      ><img src="/brand/mark.svg?v=2" width="52" height="52" alt="Dynodoc" /></a
    >
    <p class="eyebrow">YOUR DOCUMENTS, WITH A HISTORY</p>
    <h1>Welcome to your workspace.</h1>
    <p class="intro">A little less file chaos.<br />A clear history for every document.</p>
    {#if $page.url.searchParams.get('token')}
      <div class="notice">
        <Mail size={22} />
        <p>Your secure sign-in link is ready.</p>
      </div>
      <button
        class="primary"
        disabled={busy}
        onclick={() => verify($page.url.searchParams.get('token') || '')}
        >Continue to workspace <ArrowRight size={18} /></button
      >
    {:else if sent}
      <div class="notice">
        <CheckCircle2 size={26} />
        <p>{message}</p>
      </div>
      {#if devToken}<button class="primary" disabled={busy} onclick={() => verify(devToken)}
          >Continue in local development <ArrowRight size={18} /></button
        >{/if}
      <button
        class="text-button"
        onclick={() => {
          sent = false;
          devToken = '';
        }}>Use a different email</button
      >
    {:else}
      {#if next}<p class="setup">Sign in to open your workspace.</p>{/if}
      <a
        class:disabled={!options.google}
        class="google"
        href={options.google
          ? `/auth/google${next ? `?next=${encodeURIComponent(next)}` : ''}`
          : undefined}
        aria-disabled={!options.google}><span class="g">G</span> Continue with Google</a
      >
      {#if !options.google}<p class="setup">Google sign-in is awaiting administrator setup.</p>{/if}
      <div class="divider"><span></span>or use your email<span></span></div>
      <form onsubmit={requestLink}>
        <label
          >Your name<input
            autocomplete="name"
            bind:value={name}
            placeholder="How should we call you?"
            maxlength="100"
          /></label
        >
        <label
          >Work email<input
            type="email"
            autocomplete="email"
            bind:value={email}
            placeholder="you@company.com"
            required
          /></label
        >
        <button class="primary" disabled={busy || (!options.email && !options.development)}
          >{busy ? 'Sending your link…' : 'Email me a sign-in link'}<ArrowRight size={18} /></button
        >
      </form>
      {#if !options.email && !options.development}<p class="setup">
          Email sign-in is awaiting administrator setup.
        </p>{/if}
      <p class="fine">No password to remember. You choose who can collaborate on your documents.</p>
    {/if}
    {#if error}<p role="alert" class="error">{error}</p>{/if}
  </section>
  <p class="footer">
    Made for the people behind every revision.<br /><a href="/privacy">Privacy policy</a> ·
    <a href="/terms">Terms &amp; conditions</a>
  </p>
</main>

<style>
  .auth-page {
    min-height: 100vh;
    background: var(--color-parchment);
    font-family: var(--font-ui);
    padding: 32px;
  }
  .back {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--color-ink-soft);
    text-decoration: none;
    font-size: 13px;
  }
  .card {
    margin: 40px auto 24px;
    max-width: 460px;
    background: var(--color-surface-raised);
    padding: 40px;
    border: 1px solid var(--color-border-soft);
    border-radius: 20px;
    box-shadow: var(--shadow-panel);
  }
  .brand {
    font: 700 42px var(--font-ui);
    color: var(--color-accent-strong);
    text-decoration: none;
  }
  .eyebrow {
    font-size: 13px;
    letter-spacing: 2px;
    color: var(--color-ink-soft);
    margin: 24px 0 12px;
  }
  h1 {
    font: 600 34px/1.15 var(--font-ui);
    letter-spacing: -1px;
    color: var(--color-ink-strong);
    margin: 0 0 12px;
  }
  .intro {
    font-size: 14px;
    color: var(--color-ink-soft);
    line-height: 1.7;
    margin-bottom: 28px;
  }
  .google,
  .primary {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    width: 100%;
    min-height: 46px;
    border: 1px solid var(--color-border);
    border-radius: 8px;
    text-decoration: none;
    font-size: 14px;
    font-weight: 500;
    color: var(--color-ink);
    background: var(--color-surface-raised);
    cursor: pointer;
  }
  .g {
    font: bold 20px Arial;
    color: var(--color-ink-soft);
  }
  .primary {
    background: var(--color-accent-solid);
    color: var(--color-on-accent);
    border-color: var(--color-accent-solid);
    margin-top: 10px;
    justify-content: space-between;
    padding: 0 16px;
  }
  .disabled,
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .divider {
    display: flex;
    align-items: center;
    gap: 12px;
    color: var(--color-ink-soft);
    font-size: 13px;
    margin: 22px 0;
  }
  .divider span {
    height: 1px;
    background: var(--color-border-soft);
    flex: 1;
  }
  form {
    display: grid;
    gap: 16px;
  }
  label {
    font-size: 13px;
    font-weight: 500;
    color: var(--color-ink);
    display: grid;
    gap: 6px;
  }
  input {
    border: 1px solid var(--color-border);
    border-radius: 7px;
    padding: 11px 12px;
    font-size: 14px;
    width: 100%;
    background: var(--color-surface-raised);
    color: var(--color-ink);
  }
  input:focus {
    outline: 2px solid var(--color-accent-border);
    outline-offset: 2px;
  }
  .fine,
  .setup {
    font-size: 13px;
    color: var(--color-ink-soft);
    line-height: 1.6;
  }
  .fine {
    margin-top: 20px;
  }
  .setup {
    margin: 8px 0;
  }
  .footer {
    text-align: center;
    font-size: 13px;
    color: var(--color-ink-soft);
  }
  .notice {
    display: flex;
    gap: 12px;
    align-items: center;
    color: var(--color-success);
    background: var(--color-success-bg);
    border-radius: 10px;
    padding: 14px;
    font-size: 14px;
    margin: 20px 0;
  }
  .text-button {
    border: 0;
    background: none;
    color: var(--color-success);
    text-decoration: underline;
    font-size: 13px;
    margin: 16px 0;
    cursor: pointer;
  }
  .error {
    color: var(--color-danger);
    background: var(--color-danger-bg);
    padding: 12px;
    border-radius: 8px;
    font-size: 13px;
  }
  @media (max-width: 600px) {
    .auth-page {
      padding: 20px;
    }
    .card {
      padding: 28px 22px;
      margin-top: 28px;
    }
  }
  .theme-corner {
    position: absolute;
    top: 18px;
    right: 18px;
  }
</style>
