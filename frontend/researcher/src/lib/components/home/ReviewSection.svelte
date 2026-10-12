<script lang="ts">
  import { ReviewMock } from '$lib/components/mock';
  import { DiffCard, Eyebrow, Section, type DiffLine } from '$lib/components/site';

  const points = [
    {
      title: 'Word-level diffs',
      text: 'Changed words are marked inside the paragraph, cell or slide, so a reviewer reads the edit instead of the whole page.'
    },
    {
      title: 'Approvals with rules',
      text: 'A project can require one to five approvals before a merge. Owners and managers can override, and the audit log records that they did.'
    },
    {
      title: 'Policy floors',
      text: 'A project sets the minimum protection for every file inside it. A file can tighten the rules and never weaken them.'
    }
  ];

  const examples: { path: string; meta: string; lines: DiffLine[] }[] = [
    {
      path: 'Board update.docx · Summary, paragraph 1',
      meta: 'Maya Chen · 2 min ago',
      lines: [
        { kind: 'del', text: 'Revenue grew sharply in Q3.' },
        { kind: 'add', text: 'Revenue rose 12% in Q3.' }
      ]
    },
    {
      path: 'Q3 budget.xlsx · Sheet1!D4',
      meta: 'Sam Okoye · yesterday',
      lines: [
        { kind: 'del', text: '=C4-B4' },
        { kind: 'add', text: '=C4/B4-1' }
      ]
    },
    {
      path: 'Launch plan.pptx · Slide 1 title',
      meta: 'Priya Nair · 3 days ago',
      lines: [
        { kind: 'del', text: 'Launch plan' },
        { kind: 'add', text: 'Launch plan: October beta' }
      ]
    }
  ];
</script>

<Section
  id="review"
  eyebrow="REVIEW"
  title="Review the change, not the file."
  lede="A change request shows which paragraphs, cells and slides moved. Reviewers approve or ask for changes, and the merge follows the project’s rules."
  layout="split"
>
  <div class="review">
    <div class="mock" data-reveal>
      <ReviewMock
        label="A change request titled Tighten the Q3 summary, showing word-level edits to two paragraphs and two of two approvals before the Merge button."
      />
    </div>
    <ul class="points">
      {#each points as point (point.title)}
        <li data-reveal>
          <h3 class="h-3">{point.title}</h3>
          <p>{point.text}</p>
        </li>
      {/each}
    </ul>
  </div>

  <div class="changes">
    <div data-reveal><Eyebrow label="WHAT CHANGED, IN ANY FORMAT" /></div>
    <div class="cards">
      {#each examples as example (example.path)}
        <div data-reveal>
          <DiffCard path={example.path} meta={example.meta} lines={example.lines} typing />
        </div>
      {/each}
    </div>
  </div>
</Section>

<style>
  .review {
    display: grid;
    gap: clamp(28px, 4vw, 56px);
    align-items: start;
  }
  .mock {
    min-width: 0;
  }
  .points {
    display: grid;
    gap: 24px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .points li {
    display: grid;
    gap: 8px;
    padding-top: 18px;
    border-top: 1px solid var(--color-border-strong);
  }
  .points h3,
  .points p {
    margin: 0;
  }
  .points p {
    color: var(--color-ink-soft);
    line-height: 1.6;
  }
  .changes {
    margin-top: clamp(40px, 5vw, 72px);
    display: grid;
    gap: 16px;
  }
  .cards {
    display: grid;
    gap: 16px;
  }
  .cards > div {
    min-width: 0;
  }
  @media (min-width: 900px) {
    .review {
      grid-template-columns: minmax(0, 7fr) minmax(0, 4fr);
    }
    .cards {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
  }
</style>
