<script lang="ts">
  import { beginPairing, finishPairing, type Pairing } from '$lib/provenance/pairing';
  import type { Host } from '$lib/provenance/protocol';
  let {
    host,
    origin,
    disabled = false,
    onConnected
  }: {
    host: Host;
    origin: string;
    disabled?: boolean;
    onConnected: (value: { documentId: string; token: string }) => Promise<void>;
  } = $props();
  let pairing = $state<Pairing | null>(null),
    error = $state(''),
    busy = $state(false);
  let boundOrigin = '';
  async function run(action: () => Promise<void>) {
    busy = true;
    error = '';
    try {
      await action();
    } catch (e) {
      error = e instanceof Error ? e.message : 'Sign-in failed';
    } finally {
      busy = false;
    }
  }
</script>

<section aria-label="Connect to Dynodoc">
  <p>
    Sign in with your Dynodoc account in your browser, then choose which Dynodoc file to connect. No
    password or key is typed here.
  </p>
  <button
    class:primary={!pairing}
    disabled={disabled || busy}
    onclick={() =>
      void run(async () => {
        boundOrigin = origin;
        pairing = await beginPairing(origin, host);
      })}>Sign in to Dynodoc</button
  >
  {#if pairing}
    <p>
      <a href={pairing.url} target="_blank" rel="noreferrer"
        >Open browser to sign in and choose a file</a
      >
    </p>
    <p>
      Compare code <strong>{pairing.challenge.slice(0, 12)}</strong> in that browser. The link expires
      in ten minutes. Return here after approval.
    </p>
    <button
      class="primary"
      disabled={disabled || busy}
      onclick={() =>
        void run(async () => {
          const value = await finishPairing(boundOrigin, host, pairing!);
          if (boundOrigin !== origin)
            throw new Error('Workspace address changed. Start sign-in again.');
          await onConnected(value);
          pairing = null;
        })}>I approved this connection</button
    >
  {/if}
  {#if error}<p role="alert">{error}</p>{/if}
</section>

<style>
  section {
    margin: 16px 0;
  }
  p {
    line-height: 1.5;
  }
  button {
    font: inherit;
    padding: 8px 14px;
    border: 1px solid var(--color-border-strong);
    border-radius: 5px;
    background: var(--color-surface);
    color: inherit;
    cursor: pointer;
  }
  button.primary {
    background: var(--color-accent-solid);
    border-color: var(--color-accent-solid);
    color: var(--color-on-accent);
    font-weight: 600;
  }
  button.primary:hover:not(:disabled) {
    background: var(--color-accent-solid-hover);
  }
  button:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }
  button:focus-visible,
  a:focus-visible {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }
  a {
    color: var(--color-accent);
  }
</style>
