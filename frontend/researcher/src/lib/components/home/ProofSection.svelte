<script lang="ts">
  import { Btn, Section, Stat } from '$lib/components/site';
  import { GITHUB_URL } from '$lib/components/site/nav';
</script>

<Section
  id="proof"
  eyebrow="PROOF"
  title="Facts about the code, not adoption."
  lede="Dynodoc is early software. These numbers come from the repository and its CI as of 12 October 2026; none of them counts customers, users or uptime."
>
  <div class="stats">
    <div data-reveal>
      <Stat value={360} label="Rust tests passing in CI on the latest synced revision." />
    </div>
    <div data-reveal>
      <Stat
        value={6}
        label="editor add-ins, as test installs. No marketplace listing exists yet."
      />
    </div>
    <div data-reveal>
      <Stat value={38} label="PostgreSQL schema version, deployed on the hosted workspace." />
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
