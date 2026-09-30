<script lang="ts">
  import { onMount } from 'svelte';
  import 'katex/dist/katex.min.css';
  import '$lib/workspace/fonts.css';
  import { printCss, previewContent } from '$lib/workspace/print';
  import { pageSetup, documentFonts } from '$lib/workspace/office';
  let host: HTMLElement;
  let status = $state('Preparing pages…'),
    ready = $state(false),
    started = false;
  onMount(() => {
    const receive = async (event: MessageEvent) => {
      if (
        event.source !== window.parent ||
        event.origin !== location.origin ||
        event.data?.type !== 'dynodoc-print' ||
        started
      )
        return;
      started = true;
      try {
        const { Previewer } = await import('pagedjs');
        const settings = pageSetup(event.data.page);
        const html = typeof event.data.html === 'string' ? event.data.html : '';
        if (html.length > 24 * 1024 * 1024)
          throw new Error(
            'This document is too large for browser print preview. Download Word instead.'
          );
        document.documentElement.lang = settings.language;
        const faces = [
          ['Noto Sans Bengali', 'বাংলা'],
          ['Noto Naskh Arabic', 'العربية'],
          ['Noto Sans Devanagari', 'हिन्दी'],
          ['Noto Sans Hebrew', 'עברית']
        ];
        await Promise.all(
          faces.map(([name, sample]) => document.fonts.load(`11pt "${name}"`, sample))
        );
        if (/^[0-9a-f-]{36}$/i.test(String(event.data.documentId)))
          await Promise.all(
            documentFonts(event.data.fonts).map(async (font) => {
              const face = new FontFace(
                font.family,
                `url(/api/engine/documents/${event.data.documentId}/uploads/${font.uploadId})`
              );
              await face.load();
              document.fonts.add(face);
            })
          );
        await document.fonts.ready;
        const result = await new Previewer().preview(
          previewContent(html),
          [{ 'dynodoc-print.css': printCss(settings) }],
          host
        );
        status = `${result.total} ${result.total === 1 ? 'page' : 'pages'}`;
        ready = true;
      } catch (e) {
        status = e instanceof Error ? e.message : 'Could not paginate this document.';
      }
    };
    window.addEventListener('message', receive);
    window.parent.postMessage({ type: 'dynodoc-print-ready' }, location.origin);
    return () => window.removeEventListener('message', receive);
  });
</script>

<svelte:head
  ><title>Print preview · Dynodoc</title><meta
    name="robots"
    content="noindex,nofollow"
  /></svelte:head
>
<div class="preview-tools">
  <span role="status">{status}</span><button disabled={!ready} onclick={() => window.print()}
    >Print / Save as PDF</button
  >
</div>
<main bind:this={host} aria-label="Paginated document"></main>

<style>
  @media print {
    :global(body) {
      background: white !important;
    }
    .preview-tools {
      display: none !important;
    }
    :global(.pagedjs_page) {
      margin: 0 !important;
      box-shadow: none !important;
      break-after: page;
    }
  }
  /* The print preview stays in day colours whatever the interface mode: it shows
     paper as it will print, so these literals intentionally do not use the theme tokens. */
  :global(html) {
    color-scheme: light;
  }
  /* `html body` outranks the themed `body` background from app.css in any load order. */
  :global(html body) {
    margin: 0;
    background: #e8edf3;
  }
  .preview-tools {
    position: sticky;
    top: 0;
    z-index: 5;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 15px;
    padding: 12px 20px;
    background: #faf9f6;
    border-bottom: 1px solid #b9cadc;
    font: 15px Arial;
    color: #284967;
  }
  button {
    padding: 10px 16px;
    border: 1px solid #b9cadc;
    border-radius: 5px;
    color: #254f86;
    background: #e4eefe;
    font: inherit;
    cursor: pointer;
  }
  main {
    overflow: auto;
  }
</style>
