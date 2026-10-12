<script lang="ts">
  import { SyncMock } from '$lib/components/mock';
  import { Btn, Section } from '$lib/components/site';

  const steps = [
    {
      n: '01',
      title: 'Capture in your editor',
      text: 'An add-in in Word, Excel, PowerPoint, Docs, Sheets or Slides records edits to existing paragraphs, cells and slide text against stable identities. Capture runs while its pane is open; there is no background agent.'
    },
    {
      n: '02',
      title: 'Review as a change request',
      text: 'Push the captured changes to a project. Reviewers see what moved, approve or ask for changes, and the project’s rules set how many approvals a merge needs.'
    },
    {
      n: '03',
      title: 'Merge and prove',
      text: 'A merge records its parents, resolutions and resulting state in an append-only, hash-chained log. Pull the result back into the file and the history stays verifiable.'
    }
  ];
</script>

<Section
  id="how-it-works"
  eyebrow="HOW IT WORKS"
  title="From an edit to a provable history."
  lede="Dynodoc sits beside the editor you already use. It records changes against stable identities, routes them through review, and keeps the outcome in a log anyone with access can verify."
  layout="split"
>
  <ol class="steps">
    {#each steps as step (step.n)}
      <li class="step" data-reveal>
        <span class="num">STEP {step.n}</span>
        <h3 class="h-3">{step.title}</h3>
        <p>{step.text}</p>
      </li>
    {/each}
  </ol>

  <figure class="sync" data-reveal>
    <SyncMock
      label="Diagram: a Word file on a laptop and a Google Doc each exchange captured changes with one Dynodoc project, where review and merge happen."
    />
    <figcaption>
      Your editors stay the place you write. The project holds the review and the history between
      your copies.
    </figcaption>
  </figure>

  <div class="more" data-reveal>
    <Btn href="/features" variant="link" arrow>See every feature</Btn>
  </div>
</Section>

<style>
  .steps {
    display: grid;
    gap: 0;
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: 1px solid var(--color-border-strong);
  }
  .step {
    display: grid;
    gap: 10px;
    align-content: start;
    padding: 24px 0;
    border-bottom: 1px solid var(--color-border);
  }
  .step p {
    margin: 0;
    max-width: 44ch;
    color: var(--color-ink-soft);
    line-height: 1.6;
  }
  .sync {
    margin: clamp(32px, 4vw, 56px) 0 0;
    display: grid;
    gap: 14px;
  }
  .sync figcaption {
    color: var(--color-muted);
    font-size: 0.875rem;
    max-width: 70ch;
  }
  .more {
    margin-top: 24px;
  }
  @media (min-width: 820px) {
    .steps {
      grid-template-columns: repeat(3, minmax(0, 1fr));
      border-bottom: 1px solid var(--color-border);
    }
    .step {
      padding: 28px clamp(16px, 2.4vw, 32px) 32px 0;
      border-bottom: 0;
      border-right: 1px solid var(--color-border);
    }
    .step + .step {
      padding-left: clamp(16px, 2.4vw, 32px);
    }
    .step:last-child {
      border-right: 0;
      padding-right: 0;
    }
  }
</style>
