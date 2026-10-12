<script lang="ts">
  import Dino from '$lib/art/Dino.svelte';
  import Stego from '$lib/art/Stego.svelte';
  import { Btn, CopyCode, Section } from '$lib/components/site';
  import { GITHUB_URL } from '$lib/components/site/nav';

  const steps = [
    { n: '01', label: 'Create env files and install frontend dependencies', code: 'task setup' },
    { n: '02', label: 'Apply migrations with the operator login', code: 'task migrate' },
    { n: '03', label: 'Start PostgreSQL, the API and the web app', code: 'task dev' }
  ];
</script>

<Section
  id="self-host"
  tone="muted"
  eyebrow="SELF-HOST"
  title="Open source. Run it yourself."
  lede="The application is AGPL-3.0 and the standalone engine is MIT. Run the API and workspace on your own machine against PostgreSQL 16."
  layout="split"
>
  <div class="host">
    <div class="commands">
      <ol class="steps">
        {#each steps as step (step.n)}
          <li data-reveal>
            <span class="num">STEP {step.n}</span>
            <p>{step.label}</p>
            <CopyCode code={step.code} label={step.code} />
          </li>
        {/each}
      </ol>
      <p class="note" data-reveal>
        Between steps, create the separate runtime and operator database logins, as SETUP.md in the
        source archive describes. Private document content you put in the app does not become open
        source.
      </p>
      <div class="links" data-reveal>
        <Btn href="/source" variant="secondary" arrow>Download the source archive</Btn>
        <Btn href={GITHUB_URL} variant="link" external arrow>Standalone engine (MIT)</Btn>
      </div>
    </div>
    <div class="art" aria-hidden="true">
      <Dino name="stego" art={Stego} />
    </div>
  </div>
</Section>

<style>
  .host {
    display: grid;
    gap: clamp(28px, 4vw, 56px);
    align-items: center;
  }
  .steps {
    display: grid;
    gap: 20px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .steps li {
    display: grid;
    gap: 6px;
    min-width: 0;
  }
  .steps p {
    margin: 0;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
  }
  .note {
    margin: 20px 0 0;
    max-width: 60ch;
    color: var(--color-muted);
    font-size: 0.875rem;
    line-height: 1.55;
  }
  .links {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px 24px;
    margin-top: 24px;
  }
  .art {
    min-width: 0;
    color: var(--color-ink);
    --art-accent: var(--color-accent);
  }
  @media (min-width: 960px) {
    .host {
      grid-template-columns: minmax(0, 6fr) minmax(0, 5fr);
    }
  }
  @media (max-width: 959px) {
    .art {
      width: min(80%, 440px);
      margin-inline: auto;
    }
  }
</style>
