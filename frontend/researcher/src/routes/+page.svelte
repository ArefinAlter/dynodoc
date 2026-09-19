<script lang="ts">
  import { onMount } from 'svelte';
  import { FAQ, structuredDataScript } from '$lib/seo';
  let ready = $state(false);
  onMount(() => {
    ready = true;
  });
  import { env } from '$env/dynamic/public';
  import {
    ArrowRight,
    ArrowUpRight,
    FileText,
    Table2,
    ListChecks,
    Upload,
    History,
    GitBranch,
    ShieldCheck,
    MessageSquare,
    Check,
    ChevronDown,
    FolderOpen,
    MoveUpRight,
    Search,
    FileCheck2
  } from '@lucide/svelte';
  import ResearchJourney from '$lib/components/marketing/ResearchJourney.svelte';
  import SiteHeader from '$lib/components/marketing/SiteHeader.svelte';
  import SiteFooter from '$lib/components/marketing/SiteFooter.svelte';
  import WorkbenchDemo from '$lib/components/marketing/WorkbenchDemo.svelte';
  import VersionFlow from '$lib/components/marketing/VersionFlow.svelte';
  import IntegrationExplorer from '$lib/components/marketing/IntegrationExplorer.svelte';
  const workspace = $derived(
    env.PUBLIC_WORKSPACE_URL
      ? `${env.PUBLIC_WORKSPACE_URL.replace(/\/$/, '')}/workspace`
      : '/workspace'
  );
  let useCase = $state(0);
  const cases = [
    {
      label: 'Design the study',
      title: 'Give the protocol a place to take shape.',
      description:
        'Bring your interview guides, questionnaires and codebooks into one workspace. Invite colleagues to improve the wording while you keep the shared version under control.',
      file: 'Community health · Interview guide',
      kind: 'Document',
      icon: FileText,
      items: ['Research aims', 'Participant experience', 'Questions for team review'],
      note: 'The next revision starts with a conversation.',
      action: 'Comment on the wording',
      person: 'Sam',
      text: 'Let’s leave room for an experience we haven’t anticipated.'
    },
    {
      label: 'Refine in the field',
      title: 'Let fieldwork improve the instrument.',
      description:
        'Try a revised question in a personal draft. Compare it with the team’s version, work through competing edits and share just the changes that are ready.',
      file: 'Community health · Fieldwork revision',
      kind: 'Questionnaire',
      icon: ListChecks,
      items: ['Access to care', 'Travel and transport', 'Follow-up prompts'],
      note: 'A new draft, with the earlier wording still in reach.',
      action: 'Review a proposed question',
      person: 'Maya',
      text: 'This follow-up helped participants describe the travel barrier.'
    },
    {
      label: 'Prepare the data',
      title: 'Keep the reasoning close to the dataset.',
      description:
        'Bring in a response snapshot or spreadsheet. Use formulas, organize the workbook and compare edits, so the team can follow how working data changed.',
      file: 'Community health · Recruitment',
      kind: 'Spreadsheet',
      icon: Table2,
      items: ['Recruitment overview', 'Field sites', 'Cleaning notes'],
      note: 'A working dataset with a history you can inspect.',
      action: 'Check the preparation notes',
      person: 'Rafi',
      text: 'The original response export is retained for reference.'
    },
    {
      label: 'Hand over with context',
      title: 'Pass on the work and its history.',
      description:
        'Share access with the right role, review earlier versions and export files for the next stage. Give a new collaborator a way to understand how the current document took shape.',
      file: 'Community health · Team handover',
      kind: 'Document',
      icon: FolderOpen,
      items: ['Study materials', 'Reviewed changes', 'Source files'],
      note: 'Less detective work for the next person on the project.',
      action: 'Find the change in history',
      person: 'Sam',
      text: 'The latest guide includes the team’s reviewed wording.'
    }
  ];
  const activeCase = $derived(cases[useCase]);
  function reveal(node: HTMLElement) {
    if (
      window.matchMedia('(prefers-reduced-motion: reduce)').matches ||
      !('IntersectionObserver' in window)
    )
      return;
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries)
          if (entry.isIntersecting) {
            entry.target.classList.add('in-view');
            observer.unobserve(entry.target);
          }
      },
      { threshold: 0.08 }
    );
    node.querySelectorAll('[data-reveal]').forEach((el) => observer.observe(el));
    return { destroy: () => observer.disconnect() };
  }
</script>

<svelte:head>
  <!-- Static application metadata, with no user-provided values. -->
  <!-- eslint-disable-next-line svelte/no-at-html-tags -->
  {@html structuredDataScript}
  <title>Dynodoc — A clearer way to work on research, together</title>
  <meta
    name="description"
    content="Write research documents, work with spreadsheets and refine questionnaires in a shared workspace. Explore drafts, review changes and keep the history behind your work."
  />
  <meta property="og:title" content="Dynodoc — Your research, in good company." />
  <meta
    property="og:description"
    content="Familiar editors for documents, spreadsheets and questionnaires. A clearer record of how the work takes shape."
  />
  <meta name="twitter:title" content="Dynodoc — Your research, in good company." />
  <meta
    name="twitter:description"
    content="Familiar editors for documents, spreadsheets and questionnaires. A clearer record of how the work takes shape."
  />
  <meta name="theme-color" content="#faf9f6" />
</svelte:head>
<div class="marketing-page">
  <SiteHeader />
  <main id="main" use:reveal>
    <section class="hero wrap" aria-labelledby="hero-heading">
      <div class="hero-copy">
        <p class="eyebrow"><span></span> THE SHARED WORKSPACE FOR RESEARCH</p>
        <h1 id="hero-heading">Your research,<br />in <span>good company.</span></h1>
      </div>
      <div class="hero-intro">
        <p>
          From the first question to the next discovery, give your team a better place to work
          together.
        </p>
        <p class="hero-detail">
          Write documents. Work with data. Refine questionnaires. Keep the changes, conversations
          and earlier versions within reach.
        </p>
        <div class="hero-actions">
          <a class="primary-link" href={workspace}
            >Bring your research together <ArrowUpRight size={17} /></a
          ><a class="text-link" href="#workspace">Explore the workspace <ArrowRight size={15} /></a>
        </div>
      </div>
    </section>
    <section id="workspace" class="product-band" aria-label="Explore the Dynodoc workspace">
      <div class="wrap product-wrap">
        <WorkbenchDemo />
        <div class="product-benefits">
          <span><FileText size={16} /> Familiar document &amp; spreadsheet tools</span><span
            ><GitBranch size={16} /> Personal drafts, shared progress</span
          ><span><History size={16} /> A history you can return to</span>
        </div>
      </div>
    </section>
    <section class="research-section wrap" aria-labelledby="research-heading">
      <div class="section-heading" data-reveal>
        <div>
          <p class="eyebrow">BUILT AROUND THE WAY RESEARCH HAPPENS</p>
          <h2 id="research-heading">The work evolves.<br />Keep the thinking with it.</h2>
        </div>
        <p>
          Good research takes revision. Dynodoc gives every question, paragraph and cell a place in
          that story, so your team can move forward with the context intact.
        </p>
      </div>
      <div class="research-grid" data-reveal>
        <div class="case-visual" class:green={useCase === 2}>
          <div class="case-topline">
            <span class="small-folder"><FolderOpen size={14} /> Community health study</span><span
              class="case-tag">{activeCase.kind}</span
            >
          </div>
          <div class="case-paper">
            <div class="case-file-icon"><activeCase.icon size={28} /></div>
            <h3>{activeCase.file}</h3>
            <p class="case-description">{activeCase.note}</p>
            <div class="case-blocks">
              {#each activeCase.items as item, i}<div class:highlighted={i === 1}>
                  <span class="block-handle" aria-hidden="true">⠿</span><span>{item}</span
                  >{#if i === 1}<MessageSquare size={14} />{:else}<span class="faint-rule"
                    ></span>{/if}
                </div>{/each}
            </div>
          </div>
          <div class="case-comment">
            <span class="case-person">{activeCase.person[0]}</span>
            <div>
              <b>{activeCase.person}<span>{activeCase.action}</span></b>
              <p>{activeCase.text}</p>
            </div>
          </div>
          <div class="visual-footer"><Check size={13} /> Illustrative research workflow</div>
        </div>
        <div class="case-list" aria-label="Explore research workflows">
          {#each cases as item, i}<div class="case-item" class:active={useCase === i}>
              <button
                disabled={!ready}
                aria-expanded={useCase === i}
                aria-controls={`case-${i}`}
                onclick={() => (useCase = i)}
                ><item.icon size={19} />{item.label}<ArrowUpRight size={17} /></button
              >
              <div id={`case-${i}`} hidden={useCase !== i}>
                <h3>{item.title}</h3>
                <p>{item.description}</p>
              </div>
            </div>{/each}
        </div>
      </div>
    </section>
    <section id="collaboration" class="collaboration-band" aria-labelledby="collab-heading">
      <div class="wrap">
        <div class="section-heading" data-reveal>
          <div>
            <p class="eyebrow">SPACE TO EXPLORE. A WAY BACK TOGETHER.</p>
            <h2 id="collab-heading">Make it your own.<br />Make it better, together.</h2>
          </div>
          <p>
            Try an idea in your own draft. See exactly what changed. Bring selected edits into the
            team version when they’re ready. Everyone gets room to think, and the shared work keeps
            moving.
          </p>
        </div>
        <div data-reveal><VersionFlow /></div>
        <div class="collaboration-notes">
          <article>
            <GitBranch size={21} />
            <h3>A draft for a different direction</h3>
            <p>
              Explore revised wording or another approach without changing the team’s current
              version.
            </p>
          </article>
          <article>
            <Search size={21} />
            <h3>Review the part that changed</h3>
            <p>
              Compare changes to individual paragraphs, questions and cells. Resolve competing edits
              with the context in view.
            </p>
          </article>
          <article>
            <History size={21} />
            <h3>Earlier thinking stays in reach</h3>
            <p>
              Inspect previous versions and restore content as a new change. See who contributed to
              the work.
            </p>
          </article>
        </div>
      </div>
    </section>
    <section class="files-section wrap" aria-labelledby="files-heading">
      <div class="files-copy" data-reveal>
        <p class="eyebrow">START WITH WHAT YOU ALREADY HAVE</p>
        <h2 id="files-heading">Your files have<br />a next chapter.</h2>
        <p>
          That interview guide in Word. The workbook your team knows inside out. Bring them into a
          workspace where the next edit has a history.
        </p>
        <div class="file-formats">
          <span class="word"><FileText size={15} /> DOCX · DOC</span><span class="excel"
            ><Table2 size={15} /> XLSX · CSV</span
          >
        </div>
        <p class="file-detail">
          Upload, review the import and keep working. The original file is retained for reference.
          Export documents and spreadsheets when you’re ready to take them elsewhere.
        </p>
        <a href={workspace} class="text-link">Start with a document <ArrowRight size={16} /></a>
      </div>
      <div class="files-visual" data-reveal>
        <div class="incoming-files">
          <div class="incoming-word"><FileText size={23} /><span>interview-guide.docx</span></div>
          <div class="incoming-excel"><Table2 size={23} /><span>fieldwork-data.xlsx</span></div>
        </div>
        <div class="file-path" aria-hidden="true">
          <span></span><Upload size={19} /><span></span>
        </div>
        <div class="import-result">
          <div class="import-heading">
            <span><FileCheck2 size={18} /> Ready for your next revision</span><Check size={17} />
          </div>
          <div>
            <FileText size={16} /><span>Editable paragraphs &amp; questions</span><span
              class="result-line"
            ></span>
          </div>
          <div>
            <Table2 size={16} /><span>Working cells &amp; formulas</span><span class="result-line"
            ></span>
          </div>
          <div>
            <History size={16} /><span>The original, kept for reference</span><span
              class="result-line"
            ></span>
          </div>
        </div>
        <p>Review conversion warnings and complex formatting after import.</p>
      </div>
    </section>
    <section id="integrations" class="integration-band" aria-labelledby="integration-heading">
      <div class="wrap">
        <div class="section-heading" data-reveal>
          <div>
            <p class="eyebrow">PART OF YOUR RESEARCH TOOLKIT</p>
            <h2 id="integration-heading">Connect the work<br />between your tools.</h2>
          </div>
          <p>
            Keep collecting data where you already do. Bring instruments and response snapshots into
            Dynodoc for the work that comes next: reviewing, refining and making sense of it
            together.
          </p>
        </div>
        <div data-reveal><IntegrationExplorer /></div>
      </div>
    </section>
    <section class="trust-section wrap" aria-labelledby="trust-heading">
      <div data-reveal>
        <p class="eyebrow">THOUGHTFUL COLLABORATION</p>
        <h2 id="trust-heading">Share the work.<br />Choose the access.</h2>
        <p>
          Invite collaborators as editors, reviewers or readers. Keep drafts separate until you’re
          ready to share, and give the team a common place to review the work.
        </p>
        <a class="text-link" href="/privacy">How your data is handled <ArrowRight size={15} /></a>
      </div>
      <div class="access-visual" data-reveal>
        <div class="access-title">
          <ShieldCheck size={22} /><span
            >Community health study<small>People with access</small></span
          >
        </div>
        {#each [{ name: 'Maya', initial: 'M', role: 'Editor', desc: 'Edit and share changes', color: 'blue' }, { name: 'Sam', initial: 'S', role: 'Reviewer', desc: 'Review and discuss the work', color: 'gold' }, { name: 'Rafi', initial: 'R', role: 'Read only', desc: 'Read the shared document', color: 'green' }] as member}<div
            class="access-person"
          >
            <span class="person-icon {member.color}">{member.initial}</span>
            <div><b>{member.name}</b><small>{member.desc}</small></div>
            <span class="role-label">{member.role}</span>
          </div>{/each}
        <p><ShieldCheck size={13} /> An example of document access roles</p>
      </div>
    </section>
    <section class="faq-section wrap" aria-labelledby="faq-heading">
      <div>
        <p class="eyebrow">A FEW THINGS YOU MIGHT BE WONDERING</p>
        <h2 id="faq-heading">Get to know<br />your workspace.</h2>
      </div>
      <div class="faq-list">
        {#each FAQ as entry (entry.question)}
          <details>
            <summary>{entry.question}<ChevronDown size={18} /></summary>
            <p>{entry.answer}</p>
          </details>
        {/each}
      </div>
    </section>
    <section class="product-expansion wrap">
      <div>
        <p class="eyebrow">THE WORK BETWEEN THE FINDINGS</p>
        <h2>Follow the question.<br />Keep the decisions.</h2>
        <p>
          A source file, a revised question, a team discussion, an approved instrument. Give each
          part of the process a clear place in the record.
        </p>
      </div>
      <ResearchJourney />
      <div class="expansion-links">
        <a href="/features">Explore every workspace <ArrowUpRight size={17} /></a><a href="/help"
          >Learn the editing flow <ArrowUpRight size={17} /></a
        >
      </div>
    </section>
    <section class="evidence-expansion wrap">
      <div>
        <p class="eyebrow">DESIGNED FOR RESEARCH PRACTICE</p>
        <h2>Good records make<br />the work easier to follow.</h2>
        <p>
          The FAIR framework describes four foundations for reusable research: findability,
          accessibility, interoperability and reusability. Version-control guidance for researchers
          recommends keeping changes and collaboration in a clear record.
        </p>
        <a href="/why-dynodoc">Read the evidence behind the design <ArrowRight size={17} /></a>
      </div>
      <div class="evidence-note">
        <span>From methods to materials</span>
        <h3>Keep the context<br />with the content.</h3>
        <p>
          Dynodoc is designed around these principles. We have not yet measured its effect on
          research quality or team productivity.
        </p>
        <a href="https://www.nature.com/articles/sdata201618"
          >Wilkinson et al. · Scientific Data, 2016</a
        ><a href="https://journals.plos.org/ploscompbiol/article?id=10.1371/journal.pcbi.1005510"
          >Wilson et al. · PLOS, 2017</a
        >
      </div>
    </section>
    <section class="roadmap-expansion wrap">
      <p class="eyebrow">A LONGER VIEW · PLANNED</p>
      <h2>A workspace that grows<br />with the research.</h2>
      <p>
        Source-grounded learning. Research instrument validation. Statistical checks. Fieldwork and
        geospatial tools. These are planned directions, with no promised release dates.
      </p>
      <a href="/roadmap">Explore what comes next <ArrowUpRight size={17} /></a>
    </section>
    <section class="closing">
      <div class="wrap">
        <div class="closing-symbol" aria-hidden="true">
          <FileText size={32} /><MoveUpRight size={25} /><Table2 size={32} />
        </div>
        <p class="eyebrow">BRING THE TEAM. KEEP THE CONTEXT.</p>
        <h2>Make room for<br />your next good idea.</h2>
        <p>Give your research a shared place to grow.</p>
        <a href={workspace} class="primary-link">Open your workspace <ArrowUpRight size={18} /></a
        ><span class="closing-note">A research pilot for small teams</span>
      </div>
    </section>
  </main>
  <SiteFooter />
</div>

<style>
  .product-expansion,
  .evidence-expansion,
  .roadmap-expansion {
    padding-top: 85px;
    padding-bottom: 85px;
  }
  .product-expansion > div:first-child {
    max-width: 710px;
    margin-bottom: 35px;
  }
  .product-expansion h2,
  .evidence-expansion h2,
  .roadmap-expansion h2 {
    font-size: clamp(33px, 4vw, 49px);
    line-height: 1.12;
    letter-spacing: -1.5px;
    margin: 15px 0 24px;
  }
  .product-expansion > div > p:not(.eyebrow),
  .evidence-expansion p:not(.eyebrow),
  .roadmap-expansion > p:not(.eyebrow) {
    font-size: 18px;
    line-height: 1.85;
    color: #445e77;
  }
  .expansion-links {
    display: flex;
    gap: 28px;
    flex-wrap: wrap;
    margin-top: 25px;
  }
  .expansion-links a,
  .evidence-expansion > div > a,
  .roadmap-expansion > a {
    display: inline-flex;
    align-items: center;
    gap: 9px;
    font-size: 16px;
    color: #245cac;
    text-decoration: none;
  }
  .evidence-expansion {
    display: grid;
    grid-template-columns: 1.2fr 1fr;
    gap: 80px;
    border-top: 1px solid #d7e0e7;
  }
  .evidence-note {
    background: #f1ecdf;
    border: 1px solid #dfd6c1;
    border-radius: 12px;
    padding: 35px;
  }
  .evidence-note > span {
    font-size: 13px;
    color: #5d645f;
  }
  .evidence-note h3 {
    font-size: 30px;
    line-height: 1.25;
    letter-spacing: -0.6px;
  }
  .evidence-note a {
    display: block;
    font-size: 13px;
    margin-top: 12px;
    color: #355873;
  }
  .roadmap-expansion {
    border-top: 1px solid #d7e0e7;
  }
  .roadmap-expansion > p {
    max-width: 740px;
  }
  @media (max-width: 720px) {
    .evidence-expansion {
      grid-template-columns: 1fr;
      gap: 30px;
    }
    .product-expansion,
    .evidence-expansion,
    .roadmap-expansion {
      padding-top: 50px;
      padding-bottom: 50px;
    }
  }

  .marketing-page {
    background: #faf9f6;
    color: #243d54;
    font-family: var(--font-ui);
    --blue: #205bb0;
  }
  .wrap {
    max-width: 1220px;
    padding-left: 48px;
    padding-right: 48px;
    margin: auto;
  }
  section[id] {
    scroll-margin-top: 30px;
  }
  .eyebrow {
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 1.35px;
    line-height: 1.8;
    color: #566a7c;
    margin: 0 0 24px;
  }
  .hero {
    display: grid;
    grid-template-columns: minmax(0, 1.35fr) minmax(0, 1fr);
    gap: 50px;
    padding-top: 65px;
    padding-bottom: 57px;
    align-items: center;
  }
  .hero .eyebrow {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 28px;
  }
  .eyebrow > span {
    display: inline-block;
    width: 6px;
    height: 6px;
    background: #2e71c4;
    border-radius: 50%;
  }
  h1 {
    font-size: clamp(44px, 4.6vw, 66px);
    line-height: 1.06;
    letter-spacing: -3.5px;
    font-weight: 550;
    margin: 0;
    color: #22394e;
  }
  h1 > span {
    color: #205bb0;
    white-space: nowrap;
  }
  .hero-intro {
    padding-top: 26px;
  }
  .hero-intro > p:first-child {
    font-size: 19px;
    line-height: 1.65;
    letter-spacing: -0.3px;
    color: #3e566c;
    margin: 0 0 16px;
    max-width: 370px;
  }
  .hero-intro .hero-detail {
    font-size: 13px;
    line-height: 1.9;
    color: #5d6e7c;
    margin: 0 0 25px;
    max-width: 370px;
  }
  .hero-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 17px;
  }
  .primary-link {
    display: inline-flex;
    justify-content: center;
    align-items: center;
    gap: 17px;
    background: #205bb0;
    color: white;
    padding: 13px 19px;
    text-decoration: none;
    font-size: 13px;
    font-weight: 550;
    border-radius: 5px;
    border: 1px solid #205bb0;
  }
  .primary-link:hover {
    background: #164b96;
  }
  .text-link {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    color: #2059a0;
    text-decoration: none;
    font-size: 13px;
    font-weight: 550;
  }
  .text-link:hover {
    text-decoration: underline;
    text-underline-offset: 5px;
  }
  .hero-actions .text-link {
    font-size: 13px;
  }
  .product-band {
    background: #e8f1fc;
    border-top: 1px solid #dde8f5;
    border-bottom: 1px solid #dce7f4;
  }
  .product-wrap {
    padding-top: 34px;
    padding-bottom: 0;
  }
  .product-benefits {
    display: flex;
    justify-content: space-between;
    gap: 25px;
    border-top: 1px solid #cfdfef;
    margin-top: 29px;
    padding: 25px 0;
    color: #426589;
    font-size: 13px;
  }
  .product-benefits > span {
    display: flex;
    align-items: center;
    gap: 9px;
  }
  .research-section {
    padding-top: 103px;
    padding-bottom: 105px;
  }
  .section-heading {
    display: grid;
    grid-template-columns: 1.25fr 1fr;
    gap: 80px;
    align-items: end;
    margin-bottom: 47px;
  }
  .section-heading .eyebrow {
    margin-bottom: 19px;
  }
  h2 {
    font-size: clamp(32px, 3.5vw, 45px);
    line-height: 1.15;
    font-weight: 550;
    letter-spacing: -1.6px;
    margin: 0;
    color: #253e54;
  }
  .section-heading > p {
    font-size: 15px;
    line-height: 1.85;
    color: #566f84;
    margin: 0 0 3px;
    max-width: 400px;
  }
  .research-grid {
    display: grid;
    grid-template-columns: 1.15fr 1fr;
    gap: 75px;
    align-items: center;
  }
  .case-visual {
    background: #f1ecdf;
    border: 1px solid #e1dacb;
    border-radius: 7px;
    padding: 24px 27px 18px;
    position: relative;
    --visual: #2c64b0;
  }
  .case-visual.green {
    --visual: #157c46;
  }
  .case-topline {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    font-size: 13px;
    color: #686357;
    margin-bottom: 24px;
  }
  .small-folder {
    display: flex;
    gap: 7px;
    align-items: center;
  }
  .case-tag {
    border: 1px solid #d8d0c1;
    background: #faf6ee;
    border-radius: 3px;
    padding: 3px 7px;
    font-size: 13px;
  }
  .case-paper {
    background: #fffefa;
    border: 1px solid #dfd9ce;
    padding: 26px;
    border-radius: 3px;
    box-shadow: 0 10px 20px #7d6f4e08;
    transform: rotate(-1deg);
    min-height: 310px;
  }
  .case-file-icon {
    color: var(--visual);
    margin-bottom: 18px;
  }
  .case-paper h3 {
    font-size: 17px;
    line-height: 1.45;
    max-width: 280px;
    letter-spacing: -0.4px;
    font-weight: 600;
    margin: 0;
    color: #344c65;
  }
  .case-description {
    font-size: 13px;
    line-height: 1.8;
    color: #657078;
    max-width: 280px;
    margin: 8px 0 23px;
  }
  .case-blocks {
    display: grid;
    gap: 8px;
  }
  .case-blocks > div {
    display: flex;
    align-items: center;
    gap: 9px;
    font-size: 13px;
    padding: 10px 8px;
    border: 1px solid #e9e8e2;
    border-radius: 3px;
    color: #5e7080;
  }
  .case-blocks .highlighted {
    color: var(--visual);
    background: #edf3fb;
    border-color: #d3e1f4;
  }
  .green .case-blocks .highlighted {
    background: #edf5ee;
    border-color: #c8dfd0;
  }
  .block-handle {
    color: #46617a;
    font-size: 13px;
    line-height: 1;
  }
  .case-blocks :global(svg) {
    margin-left: auto;
  }
  .faint-rule {
    width: 36px;
    height: 3px;
    background: #ecece7;
    margin-left: auto;
  }
  .case-comment {
    display: flex;
    gap: 12px;
    padding: 15px 18px;
    background: #fffefa;
    border: 1px solid #d6dce4;
    position: relative;
    margin: -16px -8px 0 43px;
    box-shadow: 0 5px 14px #695e4810;
    border-radius: 4px;
    transform: rotate(1deg);
  }
  .case-person {
    background: #e2ebfb;
    color: #3461a0;
    border-radius: 50%;
    width: 25px;
    height: 25px;
    display: grid;
    place-items: center;
    flex-shrink: 0;
    font-size: 13px;
  }
  .case-comment b {
    font-size: 13px;
    font-weight: 600;
    color: #37536e;
  }
  .case-comment b span {
    font-size: 13px;
    font-weight: 400;
    color: #63707c;
    margin-left: 10px;
  }
  .case-comment p {
    font-size: 13px;
    line-height: 1.8;
    margin: 7px 0 0;
    color: #647789;
  }
  .visual-footer {
    display: flex;
    gap: 6px;
    align-items: center;
    color: #676256;
    font-size: 13px;
    margin: 20px 0 0;
  }
  .case-list {
    border-top: 1px solid #dbe1e4;
  }
  .case-item {
    border-bottom: 1px solid #dbe1e4;
    padding: 5px 0;
  }
  .case-item > button {
    display: flex;
    align-items: center;
    gap: 13px;
    padding: 21px 0;
    background: none;
    border: 0;
    text-align: left;
    width: 100%;
    font-size: 15px;
    font-weight: 550;
    color: #5e6e7a;
    cursor: pointer;
  }
  .case-item > button > :global(svg:last-child) {
    margin-left: auto;
    color: #46617a;
  }
  .case-item.active > button {
    color: #205bb0;
  }
  .case-item.active > button > :global(svg:last-child) {
    color: #205bb0;
  }
  .case-item h3 {
    font-size: 19px;
    font-weight: 550;
    letter-spacing: -0.5px;
    line-height: 1.4;
    margin: 0 0 12px;
  }
  .case-item p {
    font-size: 14px;
    line-height: 1.85;
    color: #566f84;
    margin: 0 0 24px;
    max-width: 360px;
  }
  .case-item > div {
    padding-left: 32px;
  }
  .collaboration-band {
    background: #edf3fa;
    border-top: 1px solid #e0e8f0;
    border-bottom: 1px solid #dde7f1;
    padding: 90px 0 62px;
  }
  .collaboration-notes {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 50px;
    padding-top: 37px;
  }
  .collaboration-notes article {
    color: #4e76a3;
  }
  .collaboration-notes h3 {
    color: #345778;
    font-size: 14px;
    font-weight: 600;
    margin: 13px 0 9px;
    line-height: 1.5;
  }
  .collaboration-notes p {
    font-size: 13px;
    line-height: 1.9;
    color: #56687b;
    margin: 0;
    max-width: 270px;
  }
  .files-section {
    display: grid;
    grid-template-columns: 1fr 1.1fr;
    gap: 90px;
    align-items: center;
    padding-top: 100px;
    padding-bottom: 100px;
  }
  .files-copy h2 {
    margin-bottom: 25px;
  }
  .files-copy > p:not(.eyebrow) {
    font-size: 15px;
    color: #526b80;
    line-height: 1.9;
    max-width: 365px;
  }
  .file-formats {
    display: flex;
    gap: 13px;
    margin: 23px 0;
  }
  .file-formats > span {
    display: flex;
    align-items: center;
    gap: 8px;
    border: 1px solid #d8e2ec;
    background: #eff4fc;
    border-radius: 4px;
    padding: 8px 11px;
    font-size: 13px;
    font-weight: 600;
    color: #285c9e;
  }
  .file-formats > span.excel {
    color: #22734a;
    border-color: #d0e1d6;
    background: #eff6f0;
  }
  .files-copy .file-detail {
    font-size: 13px !important;
    max-width: 345px !important;
  }
  .files-copy .text-link {
    margin-top: 15px;
  }
  .files-visual {
    background: #f1f5fa;
    border: 1px solid #e1e7f0;
    padding: 34px 30px 22px;
    border-radius: 7px;
  }
  .incoming-files {
    display: flex;
    justify-content: center;
    gap: 15px;
  }
  .incoming-files > div {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 13px;
    padding: 18px 13px;
    border: 1px solid #c3d6ee;
    background: #e6effd;
    box-shadow: 0 6px 12px #2c4e7806;
    color: #2a61ad;
    border-radius: 4px;
    transform: rotate(-5deg);
  }
  .incoming-files > div.incoming-excel {
    background: #e6f2e9;
    border-color: #c0decc;
    color: #1f7950;
    transform: rotate(5deg);
  }
  .file-path {
    display: flex;
    flex-direction: column;
    align-items: center;
    height: 90px;
    color: #46617a;
    padding: 8px 0;
  }
  .file-path > span {
    height: 23px;
    border-left: 1px dashed #aac1dc;
  }
  .file-path > :global(svg) {
    transform: rotate(180deg);
    padding: 2px;
    box-sizing: content-box;
  }
  .import-result {
    background: #fff;
    border: 1px solid #d0deed;
    border-radius: 5px;
    padding: 0 19px 12px;
    box-shadow: 0 6px 20px #324f7007;
  }
  .import-heading {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 13px;
    color: #396089;
    border-bottom: 1px solid #e4eaf1;
    padding: 18px 0 16px;
    margin-bottom: 6px;
  }
  .import-heading > span {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .import-heading > :global(svg) {
    color: #46617a;
  }
  .import-result > div:not(.import-heading) {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 0;
    font-size: 13px;
    color: #597189;
  }
  .result-line {
    height: 3px;
    width: 30px;
    background: #edf1f6;
    margin-left: auto;
  }
  .files-visual > p {
    font-size: 13px;
    color: #5e6a76;
    text-align: center;
    line-height: 1.8;
    margin: 22px 0 0;
  }
  .integration-band {
    background: #f4f0e6;
    padding: 90px 0 65px;
    border-block: 1px solid #e5e0d4;
  }
  .integration-band .eyebrow {
    color: #626661;
  }
  .trust-section {
    padding-top: 100px;
    padding-bottom: 95px;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 100px;
    align-items: center;
  }
  .trust-section > div > p:not(.eyebrow) {
    font-size: 15px;
    line-height: 1.85;
    color: #566f84;
    max-width: 375px;
    margin: 24px 0;
  }
  .access-visual {
    background: white;
    border: 1px solid #dce4ec;
    border-radius: 7px;
    box-shadow: 0 14px 34px #32496308;
    padding: 29px;
  }
  .access-title {
    display: flex;
    align-items: center;
    gap: 14px;
    color: #426a99;
    padding-bottom: 24px;
    border-bottom: 1px solid #e4eaf0;
    font-size: 13px;
    font-weight: 550;
  }
  .access-title small {
    display: block;
    font-size: 13px;
    color: #62707e;
    margin-top: 4px;
    font-weight: 400;
  }
  .access-person {
    display: flex;
    align-items: center;
    gap: 13px;
    padding: 18px 0;
    border-bottom: 1px solid #edf0f4;
  }
  .person-icon {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    border-radius: 50%;
    font-size: 13px;
  }
  .person-icon.blue {
    background: #e2ebfc;
    color: #3863a3;
  }
  .person-icon.gold {
    background: #f6edda;
    color: #46617a;
  }
  .person-icon.green {
    background: #e3f0e8;
    color: #478266;
  }
  .access-person b {
    display: block;
    font-size: 13px;
    font-weight: 550;
  }
  .access-person small {
    display: block;
    font-size: 13px;
    color: #65707c;
    margin-top: 3px;
  }
  .role-label {
    font-size: 13px;
    margin-left: auto;
    background: #f7f9fc;
    border: 1px solid #e0e7ef;
    border-radius: 4px;
    padding: 5px 8px;
    color: #576e84;
  }
  .access-visual > p {
    display: flex;
    align-items: center;
    gap: 7px;
    color: #46617a;
    font-size: 13px;
    margin: 20px 0 0;
  }
  .faq-section {
    display: grid;
    grid-template-columns: 1fr 1.3fr;
    gap: 80px;
    padding-top: 15px;
    padding-bottom: 100px;
  }
  .faq-section .eyebrow {
    max-width: 210px;
  }
  .faq-section h2 {
    font-size: 38px;
  }
  .faq-list {
    border-top: 1px solid #dce3e8;
  }
  .faq-list details {
    border-bottom: 1px solid #dce3e8;
  }
  .faq-list summary {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 20px;
    list-style: none;
    font-size: 13px;
    font-weight: 550;
    color: #405c76;
    padding: 21px 0;
    cursor: pointer;
  }
  .faq-list summary::-webkit-details-marker {
    display: none;
  }
  .faq-list summary :global(svg) {
    flex-shrink: 0;
    color: #46617a;
  }
  .faq-list details[open] summary :global(svg) {
    transform: rotate(180deg);
  }
  .faq-list p {
    font-size: 14px;
    line-height: 1.85;
    color: #566f84;
    margin: 0 23px 25px 0;
  }
  .closing {
    background: #dfebfb;
    border-block: 1px solid #cedef3;
    text-align: center;
    padding: 65px 0 62px;
  }
  .closing-symbol {
    display: flex;
    gap: 17px;
    justify-content: center;
    align-items: center;
    margin-bottom: 23px;
    color: #46617a;
  }
  .closing-symbol > :global(svg:first-child) {
    color: #2860ac;
    transform: rotate(-8deg);
  }
  .closing-symbol > :global(svg:last-child) {
    color: #217b50;
    transform: rotate(8deg);
  }
  .closing .eyebrow {
    font-size: 13px;
    color: #4d647e;
    margin-bottom: 16px;
  }
  .closing h2 {
    font-size: 49px;
    letter-spacing: -2px;
    color: #254d7e;
    line-height: 1.12;
  }
  .closing p:not(.eyebrow) {
    font-size: 13px;
    color: #4e647d;
    margin: 20px 0 25px;
  }
  .closing-note {
    display: block;
    font-size: 13px;
    color: #4f647a;
    margin-top: 18px;
  }
  @media (prefers-reduced-motion: no-preference) {
    .in-view {
      animation: reveal 0.6s ease-out both;
    }
    .file-path > :global(svg) {
      animation: arrive 1.6s ease-in-out 3;
    }
    .case-item button {
      transition: color 0.2s;
    }
    .primary-link {
      transition: background 0.2s;
    }
    @keyframes reveal {
      from {
        opacity: 0.3;
        transform: translateY(16px);
      }
      to {
        opacity: 1;
        transform: translateY(0);
      }
    }
    @keyframes arrive {
      50% {
        translate: 0 5px;
        opacity: 0.5;
      }
    }
  }
  @media (min-width: 1450px) {
    .hero {
      padding-top: 80px;
      padding-bottom: 70px;
    }
  }
  @media (max-width: 1024px) {
    .hero {
      gap: 32px;
    }
    .hero h1 {
      font-size: 52px;
      letter-spacing: -2.5px;
    }
    .hero-intro > p:first-child {
      font-size: 17px;
    }
    .hero-intro .hero-detail {
      font-size: 13px;
    }
    .section-heading {
      gap: 50px;
    }
    .research-grid {
      gap: 40px;
    }
    .files-section {
      gap: 55px;
    }
    .incoming-files {
      gap: 10px;
    }
    .incoming-files > div {
      padding: 15px 9px;
      font-size: 13px;
      gap: 7px;
    }
    .trust-section {
      gap: 55px;
    }
    .faq-section {
      gap: 45px;
    }
    .collaboration-notes {
      gap: 30px;
    }
  }
  @media (max-width: 800px) {
    .wrap {
      padding-left: 28px;
      padding-right: 28px;
    }
    .hero {
      gap: 30px;
      padding-top: 45px;
      padding-bottom: 40px;
      grid-template-columns: 1.2fr 1fr;
    }
    .hero h1 {
      font-size: 45px;
      letter-spacing: -2px;
    }
    .hero .eyebrow {
      font-size: 13px;
      letter-spacing: 0.9px;
    }
    .hero-intro > p:first-child {
      font-size: 15px;
    }
    .hero-intro .hero-detail {
      font-size: 13px;
    }
    .primary-link {
      font-size: 13px;
      gap: 10px;
      padding: 12px 14px;
    }
    .hero-actions .text-link {
      font-size: 13px;
    }
    .product-benefits {
      gap: 20px;
      font-size: 13px;
    }
    .product-benefits :global(svg) {
      flex-shrink: 0;
    }
    .section-heading {
      gap: 30px;
    }
    .section-heading > p {
      font-size: 13px;
    }
    h2 {
      font-size: 34px;
    }
    .research-section {
      padding-top: 70px;
      padding-bottom: 75px;
    }
    .research-grid {
      gap: 30px;
      grid-template-columns: 1fr 1fr;
    }
    .case-visual {
      padding: 18px 15px 15px;
    }
    .case-topline {
      font-size: 13px;
      gap: 5px;
    }
    .case-tag {
      font-size: 13px;
    }
    .case-paper {
      padding: 22px 17px;
    }
    .case-paper h3 {
      font-size: 15px;
    }
    .case-description {
      font-size: 13px;
    }
    .case-blocks > div {
      font-size: 13px;
      padding: 9px 5px;
    }
    .faint-rule {
      width: 15px;
    }
    .case-comment {
      margin-left: 15px;
      margin-right: -5px;
      padding: 12px;
      gap: 8px;
    }
    .case-comment b span {
      display: block;
      margin: 2px 0;
    }
    .case-item > button {
      font-size: 13px;
      gap: 10px;
      padding: 19px 0;
    }
    .case-item h3 {
      font-size: 17px;
    }
    .case-item p {
      font-size: 13px;
    }
    .collaboration-band {
      padding-top: 65px;
    }
    .collaboration-notes h3 {
      font-size: 13px;
    }
    .collaboration-notes p {
      font-size: 13px;
    }
    .files-section {
      gap: 30px;
      padding-top: 75px;
      padding-bottom: 75px;
    }
    .files-visual {
      padding: 25px 18px 20px;
    }
    .incoming-files {
      flex-direction: column;
      align-items: center;
      gap: 14px;
    }
    .incoming-files > div {
      width: 180px;
      padding: 13px 14px;
      font-size: 13px;
    }
    .file-path {
      height: 70px;
    }
    .file-path > span {
      height: 17px;
    }
    .files-copy > p:not(.eyebrow) {
      font-size: 13px;
    }
    .file-formats {
      gap: 9px;
    }
    .file-formats > span {
      font-size: 13px;
      padding: 7px 8px;
      gap: 5px;
    }
    .import-heading {
      font-size: 13px;
      gap: 5px;
    }
    .import-heading > span {
      gap: 5px;
    }
    .import-result {
      padding: 0 12px 10px;
    }
    .import-result > div:not(.import-heading) {
      font-size: 13px;
      gap: 7px;
    }
    .result-line {
      display: none;
    }
    .integration-band {
      padding-top: 70px;
    }
    .trust-section {
      gap: 40px;
      padding-top: 75px;
      padding-bottom: 70px;
    }
    .trust-section > div > p:not(.eyebrow) {
      font-size: 13px;
    }
    .access-visual {
      padding: 22px;
    }
    .role-label {
      font-size: 13px;
      padding: 4px 6px;
    }
    .access-person {
      gap: 9px;
    }
    .access-person small {
      font-size: 13px;
    }
    .faq-section {
      gap: 38px;
      grid-template-columns: 1fr 1.25fr;
    }
    .faq-section h2 {
      font-size: 32px;
    }
    .faq-list summary {
      font-size: 13px;
    }
    .faq-list p {
      font-size: 13px;
    }
  }
  @media (max-width: 600px) {
    .wrap {
      padding-left: 22px;
      padding-right: 22px;
    }
    .hero {
      grid-template-columns: 1fr;
      gap: 23px;
      padding-top: 36px;
      padding-bottom: 36px;
    }
    .hero h1 {
      font-size: clamp(32px, 9.4vw, 46px);
      letter-spacing: -1.5px;
      max-width: 390px;
      line-height: 1.08;
    }
    .hero .eyebrow {
      font-size: 13px;
      letter-spacing: 1px;
      margin-bottom: 23px;
    }
    .hero-intro {
      padding: 0;
    }
    .hero-intro > p:first-child {
      font-size: 16px;
      max-width: 340px;
      margin-bottom: 12px;
      line-height: 1.7;
    }
    .hero-intro .hero-detail {
      font-size: 13px;
      max-width: 340px;
      margin-bottom: 21px;
    }
    .hero-actions {
      gap: 15px;
    }
    .primary-link {
      font-size: 13px;
      padding: 12px 15px;
    }
    .hero-actions .text-link {
      font-size: 13px;
    }
    .product-wrap {
      padding-top: 22px;
      padding-left: 15px;
      padding-right: 15px;
    }
    .product-benefits {
      margin-top: 20px;
      display: grid;
      gap: 12px;
      padding: 20px 5px;
      font-size: 13px;
    }
    .research-section {
      padding-top: 58px;
      padding-bottom: 60px;
    }
    .section-heading {
      grid-template-columns: 1fr;
      gap: 22px;
      margin-bottom: 30px;
    }
    .eyebrow {
      font-size: 13px;
      letter-spacing: 1px;
      margin-bottom: 17px;
    }
    h2 {
      font-size: 35px;
      line-height: 1.17;
      letter-spacing: -1.3px;
    }
    .section-heading > p {
      font-size: 13px;
      line-height: 1.9;
      max-width: 360px;
    }
    .research-grid {
      display: flex;
      flex-direction: column-reverse;
      gap: 28px;
    }
    .case-list {
      width: 100%;
    }
    .case-item > button {
      font-size: 14px;
      padding: 17px 0;
    }
    .case-item > div {
      padding-left: 32px;
    }
    .case-item h3 {
      font-size: 18px;
    }
    .case-item p {
      font-size: 13px;
      margin-bottom: 20px;
      max-width: 340px;
    }
    .case-visual {
      width: 100%;
      padding: 23px 22px 16px;
    }
    .case-topline {
      font-size: 13px;
    }
    .case-paper {
      padding: 25px;
      min-height: 290px;
    }
    .case-paper h3 {
      font-size: 18px;
    }
    .case-description {
      font-size: 13px;
    }
    .case-blocks > div {
      font-size: 13px;
    }
    .case-comment {
      margin-left: 25px;
      padding: 15px;
    }
    .case-comment b span {
      display: inline;
      margin-left: 6px;
      font-size: 13px;
    }
    .case-comment p {
      font-size: 13px;
    }
    .collaboration-band {
      padding-top: 58px;
      padding-bottom: 40px;
    }
    .collaboration-band .wrap {
      padding-left: 18px;
      padding-right: 18px;
    }
    .collaboration-band .section-heading {
      padding: 0 4px;
    }
    .collaboration-notes {
      grid-template-columns: 1fr;
      gap: 25px;
      padding: 28px 6px 0;
    }
    .collaboration-notes article {
      display: grid;
      grid-template-columns: 24px 1fr;
      gap: 0 13px;
    }
    .collaboration-notes h3 {
      font-size: 13px;
      margin: 0 0 8px;
      align-self: center;
    }
    .collaboration-notes p {
      font-size: 13px;
      grid-column: 2;
      max-width: 330px;
    }
    .files-section {
      grid-template-columns: 1fr;
      gap: 30px;
      padding-top: 60px;
      padding-bottom: 60px;
    }
    .files-copy > p:not(.eyebrow) {
      font-size: 13px;
    }
    .file-formats > span {
      font-size: 13px;
      padding: 8px 11px;
    }
    .files-visual {
      padding: 30px 20px 20px;
    }
    .incoming-files {
      flex-direction: row;
      gap: 11px;
    }
    .incoming-files > div {
      width: auto;
      font-size: 13px;
      padding: 15px 9px;
      gap: 6px;
    }
    .incoming-files :global(svg) {
      width: 19px;
      flex-shrink: 0;
    }
    .import-result {
      padding: 0 17px 12px;
    }
    .import-heading {
      font-size: 13px;
    }
    .import-result > div:not(.import-heading) {
      font-size: 13px;
      padding: 12px 0;
    }
    .files-visual > p {
      font-size: 13px;
    }
    .integration-band {
      padding-top: 57px;
      padding-bottom: 35px;
    }
    .trust-section {
      grid-template-columns: 1fr;
      gap: 30px;
      padding-top: 58px;
      padding-bottom: 62px;
    }
    .access-visual {
      padding: 26px;
    }
    .access-person b {
      font-size: 13px;
    }
    .access-person small {
      font-size: 13px;
    }
    .role-label {
      font-size: 13px;
      padding: 5px 8px;
    }
    .faq-section {
      grid-template-columns: 1fr;
      gap: 30px;
      padding-bottom: 60px;
    }
    .faq-section .eyebrow {
      max-width: none;
      font-size: 13px;
    }
    .faq-section h2 {
      font-size: 34px;
    }
    .faq-list summary {
      font-size: 13px;
      line-height: 1.6;
      padding: 19px 0;
    }
    .faq-list p {
      font-size: 13px;
    }
    .closing {
      padding: 50px 0;
    }
    .closing h2 {
      font-size: 41px;
      letter-spacing: -1.6px;
    }
    .closing p:not(.eyebrow) {
      font-size: 13px;
    }
    .closing-note {
      font-size: 13px;
      line-height: 1.6;
      max-width: 290px;
      margin: 17px auto 0;
    }
  }
  @media (max-width: 360px) {
    .hero h1 {
      font-size: 32px;
    }
    .hero-actions {
      flex-direction: column;
      align-items: flex-start;
    }
    .case-comment b span {
      display: block;
      margin-left: 0;
    }
    .file-formats {
      gap: 7px;
    }
    .incoming-files > div {
      font-size: 13px;
      padding: 13px 8px;
      gap: 4px;
    }
  }
  .hero-intro .hero-detail,
  .section-heading > p,
  .files-copy > p,
  .faq-section p {
    font-size: 17px;
    line-height: 1.8;
  }
</style>
