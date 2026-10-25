<script lang="ts">
  import { AuditMock, TimelineMock } from '$lib/components/mock';
  import { Btn, Section } from '$lib/components/site';

  const points = [
    {
      title: 'Append-only',
      text: 'Events are never updated or deleted. Restoring or reverting adds a new commit that points back at its source; history is not reset.'
    },
    {
      title: 'Hash-chained',
      text: 'Each event’s hash covers the one before it, so changing the past breaks every hash after it. Anyone with access can walk the chain and check it.'
    },
    {
      title: 'Signed commits',
      text: 'Project commits in the open-source engine and CLI carry an Ed25519 signature from the author’s device key. Revoked keys stop signing; older signatures still verify.'
    }
  ];
</script>

<Section
  id="history"
  tone="muted"
  title="An append-only, hash-chained history"
  lede="Who changed what, and when, is recorded as typed events in a log that anyone with access can check."
  layout="split"
>
  <ul class="points">
    {#each points as point (point.title)}
      <li data-reveal>
        <h3 class="h-3">{point.title}</h3>
        <p>{point.text}</p>
      </li>
    {/each}
  </ul>

  <div class="mocks">
    <div data-reveal>
      <TimelineMock
        label="History of Board update.docx: four revisions by Maya Chen, Sam Okoye and Priya Nair, each signed and linked to the previous hash."
      />
    </div>
    <div data-reveal>
      <AuditMock
        label="Audit trail for the board-pack project: five events with sequence numbers, actors, signatures and a verified chain."
      />
    </div>
  </div>
  <p class="caption" data-reveal>
    Illustrative project with synthetic names. The hash chain runs in the hosted workspace today;
    signed project commits are in the engine and CLI and are not yet the hosted default.
  </p>

  <aside class="limits callout" data-reveal>
    <p>
      <strong>The log is tamper-evident, not tamper-proof.</strong> A database-only rewrite by an administrator
      is detectable only against a hash or backup you kept independently. Outside witnesses are planned,
      not shipped.
    </p>
    <Btn href="/security" variant="link" arrow>How data is protected</Btn>
  </aside>
</Section>

<style>
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
    align-content: start;
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
  .mocks {
    display: grid;
    gap: 20px;
    margin-top: clamp(32px, 4vw, 56px);
    align-items: start;
  }
  .mocks > div {
    min-width: 0;
  }
  .caption {
    margin: 16px 0 0;
    color: var(--color-muted);
    font-size: 0.875rem;
    max-width: 78ch;
  }
  .limits {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 12px 24px;
    margin-top: 28px;
  }
  .limits p {
    margin: 0;
    max-width: 70ch;
    line-height: 1.55;
  }
  @media (min-width: 900px) {
    .points {
      grid-template-columns: repeat(3, minmax(0, 1fr));
      gap: 32px;
    }
  }
  @media (min-width: 1040px) {
    .mocks {
      grid-template-columns: minmax(0, 5fr) minmax(0, 7fr);
    }
  }
</style>
