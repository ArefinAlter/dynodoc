<script lang="ts">
  import { Btn, Section, Stat } from '$lib/components/site';
  import { GITHUB_URL } from '$lib/components/site/nav';
  import { production } from '$lib/site/production';
</script>

<Section
  id="proof"
  eyebrow="PROOF"
  title="Facts about the code, not adoption."
  lede="Dynodoc is early software. The test count is from one run of the public engine repository’s CI on 20 October 2026 (UTC), and the schema is the one the hosted workspace has run since {production.date}. None of it counts customers, users or uptime."
>
  <div class="stats">
    <div data-reveal>
      <Stat
        value={1285}
        label="Rust tests passing in the public engine’s CI on Linux, macOS and Windows (1,284 on Intel Macs, where one timing test is excluded)."
      />
    </div>
    <div data-reveal>
      <Stat
        value={6}
        label="editor add-ins, as test installs. No marketplace listing exists yet."
      />
    </div>
    <div data-reveal>
      <Stat
        value={production.schema}
        label="PostgreSQL schema version, deployed on the hosted workspace."
      />
    </div>
  </div>
  <div class="links" data-reveal>
    <Btn href="/roadmap" variant="secondary" arrow>Read the roadmap</Btn>
    <Btn href={GITHUB_URL} variant="link" external arrow>Inspect the open engine</Btn>
  </div>
</Section>

<style>
  .stats {
    display: grid;
    gap: 0;
    border-top: 1px solid var(--color-border-strong);
    border-bottom: 1px solid var(--color-border);
  }
  .stats > div {
    padding: 28px 0;
    border-bottom: 1px solid var(--color-border);
  }
  .stats > div:last-child {
    border-bottom: 0;
  }
  .links {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 12px 24px;
    margin-top: 28px;
  }
  @media (min-width: 820px) {
    .stats {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
    .stats > div {
      padding: 36px clamp(16px, 2.4vw, 32px) 36px 0;
      border-bottom: 0;
      border-right: 1px solid var(--color-border);
    }
    .stats > div + div {
      padding-left: clamp(16px, 2.4vw, 32px);
    }
    .stats > div:last-child {
      border-right: 0;
    }
  }
</style>
