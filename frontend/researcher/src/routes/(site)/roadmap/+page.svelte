<script lang="ts">
  import ProductPage from '$lib/components/marketing/ProductPage.svelte';
  import {
    BookOpen,
    Bot,
    ListChecks,
    Library,
    ChartNoAxesCombined,
    Map,
    ClipboardCheck,
    Waypoints
  } from '@lucide/svelte';
  const ideas = [
    {
      icon: Waypoints,
      title: 'Portable, efficient document histories',
      body: 'Structurally shared snapshots, portable revision ancestry and incremental editor operations, measured against real document workloads.'
    },
    {
      icon: ListChecks,
      title: 'Higher-fidelity Word and Excel exchange',
      body: 'Expand complex page layout, native equations, references, formatting revisions, charts and workbook compatibility, with published round-trip tests.'
    },
    {
      icon: ClipboardCheck,
      title: 'Precise attribution and offline collaboration',
      body: 'Stable identities inside tables and nested content, attributed text ranges and an evaluated convergence layer for disconnected editing.'
    },
    {
      icon: BookOpen,
      title: 'Learning grounded in your sources',
      body: 'Retrieve relevant passages across a study’s materials, build reading guides and explore questions with source references. Inspired by source-based research notebooks.'
    },
    {
      icon: Bot,
      title: 'Agentic Document Environment',
      body: 'Assistants that propose document operations for human review, with clear sources, permissions and recorded acceptance.'
    },
    {
      icon: ListChecks,
      title: 'Research instrument validation',
      body: 'Inspect wording, scales, branching and consistency inside the workspace. Separate automated checks from methodological judgement.'
    },
    {
      icon: Library,
      title: 'Research instrument gallery',
      body: 'Discover reusable instruments with provenance, licensing, adaptation notes and version histories.'
    },
    {
      icon: ChartNoAxesCombined,
      title: 'Statistical validation and tests',
      body: 'Explore dataset quality, missingness, reliability and appropriate statistical tests with explicit assumptions and reproducible outputs.'
    },
    {
      icon: ClipboardCheck,
      title: 'Survey field implementation',
      body: 'Connect approved instruments to deployment and field monitoring, preserving the version associated with collected data.'
    },
    {
      icon: Map,
      title: 'Geospatial research tools',
      body: 'Work with geographic attributes, coverage, boundaries and spatial quality checks while respecting location privacy.'
    },
    {
      icon: Waypoints,
      title: 'Deeper collaboration and interoperability',
      body: 'Expand editor fidelity, connected-tool synchronization and accessible collaboration after dedicated testing.'
    }
  ];
</script>

<ProductPage
  title="The document version-control roadmap"
  description="The core priorities are faithful editing, efficient history and dependable collaboration. Research and learning tools build on that foundation. These are planned directions, with no promised release dates."
  eyebrow="Product direction · Planned features"
>
  <section class="callout">
    <h2>Planned, not currently available</h2>
    <p>
      The capabilities below describe the direction of the product. They are not enabled services or
      commitments. The current release focuses on authoring, imports, review, access and version
      history. <a href="/features">See what works today.</a>
    </p>
  </section>
  <section class="cards">
    {#each ideas as idea}<article>
        <idea.icon size={29} />
        <p><span class="badge">Planned</span></p>
        <h2>{idea.title}</h2>
        <p>{idea.body}</p>
      </article>{/each}
  </section>
  <section>
    <h2>The principle that carries forward</h2>
    <p>
      New tools should help people understand and review their work. Suggestions should have
      sources, edits should be reviewable, and accepted changes should join the document’s history.
      Statistical or AI assistance will need its own evaluation before it can support research
      decisions.
    </p>
  </section>
</ProductPage>
