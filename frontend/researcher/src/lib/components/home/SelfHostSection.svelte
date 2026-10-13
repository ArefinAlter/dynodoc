<script lang="ts">
  import Dino from '$lib/art/Dino.svelte';
  import Stego from '$lib/art/Stego.svelte';
  import { Btn, CopyCode, Section } from '$lib/components/site';
  import { GITHUB_URL } from '$lib/components/site/nav';

  // Commands from the engine repository's README and Taskfile.yml. The workspace and website
  // live in a private repository and have no public setup.
  const steps = [
    { n: '01', label: 'Start PostgreSQL 16 on loopback port 5548', code: 'task up' },
    { n: '02', label: 'Apply migrations with the operator login', code: 'task migrate' },
    {
      n: '03',
      label: 'Grant the restricted runtime login its allowlist',
      code: 'task db:grant-runtime'
    },
    { n: '04', label: 'Run the HTTP API on loopback port 8080', code: 'task run' }
  ];
</script>

<Section
  id="self-host"
  tone="muted"
  eyebrow="OPEN ENGINE"
  title="The engine is open source. Run it yourself."
  lede="The engine, API and command-line tools are open source under the MIT licence. Run the API on your own machine against PostgreSQL 16. The workspace and website are not published as open source."
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
        First copy the two example env files, set the keys and create the restricted runtime login,
        as the engine README describes. The API listens on loopback only and never applies
        migrations itself. The commands need Docker, Rust and Task.
      </p>
      <div class="links" data-reveal>
        <Btn href={GITHUB_URL} variant="secondary" external arrow>Engine on GitHub (MIT)</Btn>
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
