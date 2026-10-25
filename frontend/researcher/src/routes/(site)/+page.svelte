<script lang="ts">
  import { env } from '$env/dynamic/public';
  import { Faq, Section } from '$lib/components/site';
  import { workspaceHref } from '$lib/components/site/nav';
  import ClosingCta from '$lib/components/home/ClosingCta.svelte';
  import Editors from '$lib/components/home/Editors.svelte';
  import Hero from '$lib/components/home/Hero.svelte';
  import HistorySection from '$lib/components/home/HistorySection.svelte';
  import HowItWorks from '$lib/components/home/HowItWorks.svelte';
  import ProjectsSection from '$lib/components/home/ProjectsSection.svelte';
  import ProofSection from '$lib/components/home/ProofSection.svelte';
  import ReviewSection from '$lib/components/home/ReviewSection.svelte';
  import SelfHostSection from '$lib/components/home/SelfHostSection.svelte';
  import WorksWith from '$lib/components/home/WorksWith.svelte';
  import { FAQ, SITE_DESCRIPTION, SITE_NAME, SITE_TAGLINE, structuredDataScript } from '$lib/seo';

  const workspace = $derived(workspaceHref(env.PUBLIC_WORKSPACE_URL));
  const title = `${SITE_NAME} — ${SITE_TAGLINE}`;
</script>

<svelte:head>
  <!-- Static application metadata, with no user-provided values. -->
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  {@html structuredDataScript}
  <title>{title}</title>
  <meta name="description" content={SITE_DESCRIPTION} />
  <meta property="og:title" content={title} />
  <meta property="og:description" content={SITE_DESCRIPTION} />
  <meta name="twitter:title" content={title} />
  <meta name="twitter:description" content={SITE_DESCRIPTION} />
</svelte:head>

<main id="main">
  <Hero {workspace} />
  <WorksWith />
  <HowItWorks />
  <Editors />
  <ReviewSection />
  <HistorySection />
  <ProjectsSection />
  <ProofSection />
  <SelfHostSection />
  <Section
    id="faq"
    title="Common questions"
    lede="Including what Dynodoc does not do."
    layout="split"
  >
    <div data-reveal>
      <Faq items={FAQ} exclusive />
    </div>
  </Section>
  <ClosingCta {workspace} />
</main>
