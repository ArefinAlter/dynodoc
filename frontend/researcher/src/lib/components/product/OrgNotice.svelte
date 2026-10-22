<script lang="ts">
  // The notice an organisation running its own installation configures (organisation name,
  // plain text, a support link). It stands where the hosted service shows its operator facts
  // and its privacy and terms links. Plain text only: line breaks are kept, nothing is parsed.
  // `compact` folds it into a one-line summary for the footer of the workspace pages.
  import { page } from '$app/stores';
  let { compact = false }: { compact?: boolean } = $props();
  const notice = $derived($page.data.site?.notice);
</script>

{#if notice}
  {#if compact}
    <details class="org-notice compact">
      <summary>{notice.organisation || 'About this installation'}</summary>
      {#if notice.text}<p class="text">{notice.text}</p>{/if}
      {#if notice.supportUrl}<p>
          <a href={notice.supportUrl} rel="noopener noreferrer">Contact or support</a>
        </p>{/if}
    </details>
  {:else}
    <aside class="org-notice" aria-label="About this installation">
      {#if notice.organisation}<p class="org">{notice.organisation}</p>{/if}
      {#if notice.text}<p class="text">{notice.text}</p>{/if}
      {#if notice.supportUrl}<p>
          <a href={notice.supportUrl} rel="noopener noreferrer">Contact or support</a>
        </p>{/if}
    </aside>
  {/if}
{/if}

<style>
  .org-notice {
    margin: 24px auto;
    max-width: 460px;
    padding: 0 8px;
    text-align: center;
    font-size: 13px;
    line-height: 1.6;
    color: var(--color-ink-soft, inherit);
  }
  .org-notice p {
    margin: 4px 0;
  }
  .org {
    font-weight: 600;
    color: var(--color-ink, inherit);
  }
  .text {
    white-space: pre-line;
    text-align: left;
  }
  a {
    color: inherit;
  }
  .compact {
    margin: 0;
    max-width: 520px;
    padding: 0;
    font-size: 12px;
  }
  .compact summary {
    cursor: pointer;
  }
  @media (pointer: coarse) {
    .compact summary {
      min-height: 44px;
      display: flex;
      align-items: center;
      justify-content: center;
    }
  }
</style>
