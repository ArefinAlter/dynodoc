<script lang="ts">
  import { onMount } from 'svelte';
  import type { PageSetup } from '$lib/workspace/office';
  let {
    html,
    page,
    fonts,
    documentId,
    description = 'Preview the current tab. Download Word to export all tabs.',
    onClose
  }: {
    html: string;
    page: PageSetup;
    description?: string;
    fonts?: unknown;
    documentId?: string;
    onClose: () => void;
  } = $props();
  let dialog: HTMLDialogElement, frame: HTMLIFrameElement;
  onMount(() => {
    dialog.showModal();
    const receive = (e: MessageEvent) => {
      if (
        e.source === frame.contentWindow &&
        e.origin === location.origin &&
        e.data?.type === 'dynodoc-print-ready'
      )
        frame.contentWindow?.postMessage(
          {
            type: 'dynodoc-print',
            html,
            page: JSON.parse(JSON.stringify(page)),
            fonts: JSON.parse(JSON.stringify(fonts || [])),
            documentId
          },
          location.origin
        );
    };
    window.addEventListener('message', receive);
    return () => window.removeEventListener('message', receive);
  });
</script>

<dialog bind:this={dialog} onclose={onClose} aria-label="Print preview">
  <header>
    <strong>Print preview</strong><span>{description}</span><button onclick={onClose}
      >Close preview</button
    >
  </header>
  <iframe
    bind:this={frame}
    src="/print"
    title="Document pages"
    sandbox="allow-scripts allow-same-origin allow-modals"
  ></iframe>
</dialog>

<style>
  dialog {
    width: calc(100vw - 32px);
    max-width: 1180px;
    height: calc(100vh - 32px);
    padding: 0;
    border: 1px solid var(--color-border-strong);
    border-radius: 10px;
    color: var(--color-ink);
    background: var(--color-surface-raised);
  }
  dialog::backdrop {
    background: var(--color-overlay);
  }
  header {
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 14px 20px;
    background: var(--color-surface);
  }
  header span {
    font-size: 13px;
    flex: 1;
  }
  button {
    padding: 9px 13px;
    border: 1px solid var(--color-border-strong);
    border-radius: 5px;
    background: var(--color-surface-raised);
    color: inherit;
    cursor: pointer;
  }
  iframe {
    width: 100%;
    height: calc(100% - 66px);
    border: 0;
  }
  @media (max-width: 600px) {
    header span {
      display: none;
    }
  }
</style>
