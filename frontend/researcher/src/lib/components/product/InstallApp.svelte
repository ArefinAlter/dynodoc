<script lang="ts">
  import { onMount } from 'svelte';
  import { pwa } from '$lib/pwa/client.svelte';

  // Browsers and the layout start this once; calling start() again is a no-op.
  onMount(() => pwa.start());
  let declined = $state(false);
  async function install() {
    declined = !(await pwa.install());
  }
</script>

<section class="repo-panel" id="install-app" aria-labelledby="install-app-title">
  <div class="repo-panel-body repo-form">
    <h2 id="install-app-title">Install the app</h2>
    {#if pwa.guide?.kind === 'installed'}
      <p class="repo-muted">Dynodoc is running as an installed app on this device.</p>
    {:else if pwa.guide?.kind === 'prompt'}
      <p class="repo-muted">
        Open Dynodoc in its own window, with its own icon, without the browser's tabs and address
        bar. You still sign in the same way, and your documents stay on the server.
      </p>
      <button type="button" class="repo-primary" onclick={install}>Install app</button>
      {#if declined}<p class="repo-muted repo-small" role="status">
          Not installed. You can try again whenever you like.
        </p>{/if}
    {:else if pwa.guide?.kind === 'steps'}
      <p class="repo-muted">{pwa.guide.intro}</p>
      <ol class="install-steps">
        {#each pwa.guide.steps as step (step)}<li>{step}</li>{/each}
      </ol>
    {:else if pwa.guide?.kind === 'unavailable'}
      <p class="repo-muted">{pwa.guide.text}</p>
    {/if}
    <p class="repo-muted repo-small">
      The app needs a connection. When you are offline it shows a short notice instead of a
      document; nothing from your account is stored on the device for offline use.
    </p>
  </div>
</section>

<style>
  .install-steps {
    margin: 0 0 12px;
    padding-left: 1.25rem;
  }
  .install-steps li {
    margin-bottom: 4px;
  }
</style>
