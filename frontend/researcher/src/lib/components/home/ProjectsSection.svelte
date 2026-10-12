<script lang="ts">
  import { RepoMock } from '$lib/components/mock';
  import { Btn, Section } from '$lib/components/site';

  const points = [
    {
      title: 'Projects',
      text: 'Related Word, Excel, PowerPoint, CSV and Markdown files live together with one set of members and one set of rules.'
    },
    {
      title: 'Drafts',
      text: 'Each person can work in a private draft of a file and share selected changes when they are ready. Project-wide branches exist in the engine, and the workspace lists them read-only under Branches.'
    },
    {
      title: 'Named versions',
      text: 'Name a milestone such as Board pack v3 and return to it. Restoring creates a new change; newer work is not erased.'
    },
    {
      title: 'Recovery',
      text: 'Revert a past change or replay it onto the current version. Either one is a new commit that points back at the change it came from.'
    }
  ];
</script>

<Section
  id="projects"
  eyebrow="PROJECTS"
  title="A repository for documents."
  lede="Group related files into a project, give people roles, and let every file keep its own history, drafts and change requests."
  layout="split"
>
  <div class="projects">
    <div class="mock" data-reveal>
      <RepoMock
        label="A project named board-pack listing Board update.docx, Q3 budget.xlsx, Launch plan.pptx and README.md with their latest changes, plus a format breakdown and two open change requests."
      />
    </div>
    <ul class="points">
      {#each points as point, index (point.title)}
        <li data-reveal>
          <span class="num">{String(index + 1).padStart(2, '0')}</span>
          <div>
            <h3 class="h-3">{point.title}</h3>
            <p>{point.text}</p>
          </div>
        </li>
      {/each}
    </ul>
  </div>
  <div class="more" data-reveal>
    <Btn href="/why-dynodoc" variant="link" arrow>Why provenance matters</Btn>
  </div>
</Section>

<style>
  .projects {
    display: grid;
    gap: clamp(28px, 4vw, 56px);
    align-items: start;
  }
  .mock {
    min-width: 0;
  }
  .points {
    display: grid;
    gap: 0;
    margin: 0;
    padding: 0;
    list-style: none;
    border-top: 1px solid var(--color-border-strong);
  }
  .points li {
    display: grid;
    grid-template-columns: 2.5rem minmax(0, 1fr);
    gap: 4px 12px;
    padding: 18px 0;
    border-bottom: 1px solid var(--color-border);
  }
  .points h3,
  .points p {
    margin: 0;
  }
  .points h3 {
    margin-bottom: 6px;
  }
  .points p {
    color: var(--color-ink-soft);
    line-height: 1.6;
  }
  .more {
    margin-top: 24px;
  }
  @media (min-width: 960px) {
    .projects {
      grid-template-columns: minmax(0, 7fr) minmax(0, 5fr);
    }
    .mock {
      position: sticky;
      top: calc(var(--site-header-h, 64px) + 24px);
    }
  }
</style>
