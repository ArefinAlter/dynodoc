<script lang="ts">
  import { goto } from '$app/navigation';
  import { page } from '$app/stores';
  import { magicLink, verify } from '$lib/api/auth';
  import { setTokens } from '$lib/stores/session.svelte';
  import { pushToast } from '$lib/stores/toast.svelte';

  let email = $state('');
  let displayName = $state('');
  let magicToken = $state('');
  let busy = $state(false);
  // PoC: the magic-link token comes back in the response, so we can complete login in
  // one screen. Production would email the token and the user would click the link.
  let stage = $state<'request' | 'verify'>('request');

  async function requestLink(event: SubmitEvent) {
    event.preventDefault();
    if (!email.trim()) return;
    busy = true;
    try {
      const res = await magicLink(email.trim(), displayName.trim() || undefined);
      magicToken = res.magic_link_token;
      stage = 'verify';
      pushToast('Magic link issued (PoC: token returned in response).', 'info');
    } catch (e) {
      pushToast(e instanceof Error ? e.message : 'Could not request a link', 'error');
    } finally {
      busy = false;
    }
  }

  async function completeLogin(event: SubmitEvent) {
    event.preventDefault();
    if (!magicToken.trim()) return;
    busy = true;
    try {
      const tokens = await verify(magicToken.trim());
      setTokens(tokens.access_token, tokens.refresh_token);
      const next = $page.url.searchParams.get('next') ?? '/';
      await goto(next);
    } catch (e) {
      pushToast(e instanceof Error ? e.message : 'Verification failed', 'error');
    } finally {
      busy = false;
    }
  }
</script>

<main>
  <div class="eyebrow">dynodoc · researcher</div>
  <h1>Sign in</h1>
  <p class="lede">
    The PoC uses email magic links. Locally the one-time token is returned in the response, so you
    can complete sign-in here without a mail provider.
  </p>

  {#if stage === 'request'}
    <form onsubmit={requestLink}>
      <label>
        <span>Email</span>
        <input type="email" bind:value={email} placeholder="you@example.org" required />
      </label>
      <label>
        <span>Display name (optional)</span>
        <input type="text" bind:value={displayName} placeholder="Your name" />
      </label>
      <button class="primary" type="submit" disabled={busy}>
        {busy ? 'Requesting…' : 'Request magic link'}
      </button>
    </form>
  {:else}
    <form onsubmit={completeLogin}>
      <label>
        <span>Magic-link token</span>
        <input type="text" bind:value={magicToken} required />
      </label>
      <div class="actions">
        <button class="primary" type="submit" disabled={busy}>
          {busy ? 'Verifying…' : 'Complete sign-in'}
        </button>
        <button type="button" class="ghost" onclick={() => (stage = 'request')}>Back</button>
      </div>
    </form>
  {/if}
</main>

<style>
  main {
    max-width: 32rem;
    margin: 6rem auto;
    padding: 0 1.5rem;
  }

  .eyebrow {
    font-family: var(--font-ui);
    color: var(--color-muted);
    font-size: 0.76rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
  }

  h1 {
    font-family: var(--font-display);
    color: var(--color-terracotta);
    font-size: 2.6rem;
    margin: 0.4rem 0 0.6rem;
    font-weight: 500;
  }

  .lede {
    color: var(--color-muted);
    margin-bottom: 2rem;
  }

  form {
    display: grid;
    gap: 1rem;
  }

  label {
    display: grid;
    gap: 0.35rem;
    font-family: var(--font-ui);
    font-size: 0.85rem;
    color: var(--color-muted);
  }

  input {
    border: 1px solid var(--color-border);
    border-radius: 0.6rem;
    padding: 0.7rem 0.85rem;
    background: var(--color-surface);
    color: var(--color-ink);
  }

  .actions {
    display: flex;
    gap: 0.6rem;
  }

  .primary {
    display: inline-flex;
    align-items: center;
    padding: 0.75rem 1rem;
    border: 0;
    border-radius: 0.7rem;
    background: var(--color-ink);
    color: var(--color-surface);
    font-family: var(--font-ui);
    cursor: pointer;
  }

  .primary:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .ghost {
    border: 1px solid var(--color-border);
    border-radius: 0.7rem;
    padding: 0.75rem 1rem;
    background: transparent;
    color: var(--color-muted);
    font-family: var(--font-ui);
    cursor: pointer;
  }
</style>
