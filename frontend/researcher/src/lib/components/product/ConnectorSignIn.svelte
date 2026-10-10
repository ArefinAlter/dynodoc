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
  <button
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
