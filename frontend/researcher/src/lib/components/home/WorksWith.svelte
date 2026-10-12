<script lang="ts">
  import { FileSpreadsheet, FileText, Presentation } from '@lucide/svelte';
  import { Eyebrow, Marquee } from '$lib/components/site';

  // Generic document glyphs only: no vendor logos.
  const editors = [
    { name: 'Word', icon: FileText },
    { name: 'Excel', icon: FileSpreadsheet },
    { name: 'PowerPoint', icon: Presentation },
    { name: 'Google Docs', icon: FileText },
    { name: 'Google Sheets', icon: FileSpreadsheet },
    { name: 'Google Slides', icon: Presentation }
  ];
  const formats = ['DOCX', 'XLSX', 'PPTX', 'CSV', 'Markdown'];
</script>

<section class="works" aria-label="Works with">
  <div class="container row">
    <Eyebrow label="WORKS WITH" />
    <div class="strip">
      <Marquee label="Supported editors and file formats" speed={42}>
        {#each editors as editor (editor.name)}
          <li class="chip">
            <editor.icon size={18} aria-hidden="true" />
            <span>{editor.name}</span>
          </li>
        {/each}
        {#each formats as format (format)}
          <li class="chip chip--format"><span class="mono">{format}</span></li>
        {/each}
      </Marquee>
    </div>
  </div>
</section>

<style>
  .works {
    border-block: 1px solid var(--color-border);
    background: var(--color-surface);
  }
  .row {
    display: grid;
    gap: 14px;
    padding-block: 18px;
    align-items: center;
  }
  .strip {
    min-width: 0;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    flex: none;
    min-height: 36px;
    color: var(--color-ink);
    font-size: 0.9375rem;
    font-weight: 500;
    white-space: nowrap;
  }
  .chip :global(svg) {
    color: var(--color-ink-soft);
  }
  .chip--format {
    padding-inline: 10px;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--site-radius-sm);
    font-size: 0.8125rem;
    letter-spacing: 0.04em;
  }
  @media (min-width: 900px) {
    .row {
      grid-template-columns: auto minmax(0, 1fr);
      gap: 32px;
    }
  }
</style>
