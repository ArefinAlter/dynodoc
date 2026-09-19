<script lang="ts">
  import ProductPage from '$lib/components/marketing/ProductPage.svelte';
  import ResearchJourney from '$lib/components/marketing/ResearchJourney.svelte';
</script>

<ProductPage
  title="The reasoning deserves a record."
  description="A questionnaire is more than a file. It carries choices about wording, context, measurement and people. Dynodoc gives those choices somewhere to live."
>
  <section>
    <h2>Research changes before it becomes a finding.</h2>
    <p>
      A fieldworker spots an ambiguous phrase. A colleague proposes a different recall period. The
      protocol changes after review. Dynodoc is designed to keep the document, the proposed revision
      and the decision connected, so the next person can understand what changed.
    </p>
    <ResearchJourney />
  </section>
  <section>
    <h2>Grounded in research practice</h2>
    <div class="cards">
      <article>
        <span class="badge">Scientific Data · 2016</span>
        <h3>Four principles for reusable research</h3>
        <p>
          The FAIR framework organizes guidance around findability, accessibility, interoperability
          and reusability. Its authors apply the principles to research workflows and tools as well
          as datasets. Dynodoc’s use of structured content and retained history is a design response
          to that need; it is not FAIR certification.
        </p>
        <a href="https://www.nature.com/articles/sdata201618"
          >Wilkinson et al., The FAIR Guiding Principles</a
        >
      </article>
      <article>
        <span class="badge">PLOS Computational Biology · 2017</span>
        <h3>Version control belongs in the workflow</h3>
        <p>
          Wilson and colleagues recommend practical habits for organizing data, recording changes,
          collaborating and preparing manuscripts. Dynodoc brings selected version-control ideas
          into graphical document and questionnaire editing.
        </p>
        <a href="https://journals.plos.org/ploscompbiol/article?id=10.1371/journal.pcbi.1005510"
          >Wilson et al., Good enough practices in scientific computing</a
        >
      </article>
    </div>
  </section>
  <section class="callout">
    <h2>Why transparent records matter</h2>
    <p>
      A 2015 project repeated <strong>100 studies</strong> from three psychology journals. Its
      report found statistically significant results in
      <strong>97% of the original studies and 36% of replications</strong>. This is a result for
      that selected study set and significance criterion, not a universal reproducibility rate. The
      Association for Psychological Science reported the findings for a project that included 40
      studies from its journal.
    </p>
    <p>
      <a
        href="https://www.psychologicalscience.org/publications/observer/obsonline/report-demonstrates-need-for-improved-reproducibility-in-psychological-science.html"
        >APS report on the Open Science Collaboration (2015)</a
      >
      · <a href="https://doi.org/10.1126/science.aac4716">Original Science paper</a>
    </p>
    <p>
      These findings motivate transparent methods and materials. They do not measure Dynodoc’s
      impact, and version history alone cannot establish research validity.
    </p>
  </section>
  <section>
    <h2>Where Dynodoc is designed to help</h2>
    <div class="cards">
      <article>
        <h3>Before fieldwork</h3>
        <p>
          Compare alternative wording. Keep questionnaire logic and translations with the
          instrument. Name the version your team approves.
        </p>
      </article>
      <article>
        <h3>Across a research team</h3>
        <p>
          Give colleagues a role that fits their contribution. Review proposed changes before
          combining them with the shared document.
        </p>
      </article>
      <article>
        <h3>At a handover</h3>
        <p>
          Pass on the source file, working copy and revision record. Let the next researcher recover
          context from the work itself.
        </p>
      </article>
      <article>
        <h3>When work is shared</h3>
        <p>
          Publish a deliberate read-only copy. Export editable files for other tools. Keep private
          review conversations inside the workspace.
        </p>
      </article>
    </div>
  </section>
  <a class="cta" href="/features">Explore the current features</a>
</ProductPage>
