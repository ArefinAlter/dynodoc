<script lang="ts">
  import {
    ArrowRight,
    ArrowUpRight,
    Check,
    CheckCheck,
    FileText,
    Table2,
    GitCompareArrows,
    History,
    MessageSquare,
    ShieldCheck,
    Upload,
    Users,
    ChevronRight,
    Sparkles,
    Menu,
    X,
    Download,
    Layers
  } from '@lucide/svelte';
  import { env } from '$env/dynamic/public';
  const workspace = $derived(
    env.PUBLIC_WORKSPACE_URL
      ? env.PUBLIC_WORKSPACE_URL.replace(/\/$/, '') + '/workspace'
      : '/workspace'
  );
  let step = $state(1),
    menu = $state(false);
  const steps = [
    {
      title: 'Bring your work',
      copy: 'Start with a document, a questionnaire, or a table. Give the team one place to work.'
    },
    {
      title: 'Think together',
      copy: 'Write, leave comments, and suggest a better question. Keep discussion next to the work.'
    },
    {
      title: 'See what changed',
      copy: 'Compare versions, review overlapping edits, and keep the reasoning behind every decision.'
    },
    {
      title: 'Keep moving',
      copy: 'Save an agreed version and take your work back to the tools your study needs.'
    }
  ];
</script>

<svelte:head
  ><title>Dynodoc — Good research is a team effort.</title><meta
    name="description"
    content="A shared workspace for research documents, questionnaires, and tables. Write together, review changes, and keep a clear history."
  /></svelte:head
>
<div class="landing">
  <header class="nav-wrap">
    <nav class="nav" aria-label="Main navigation">
      <a href="/" class="wordmark"><span class="logo-mark">d<span></span></span>dynodoc</a>
      <div class:mobile-open={menu} class="nav-links">
        <a href="#why" onclick={() => (menu = false)}>Why Dynodoc</a><a
          href="#workflow"
          onclick={() => (menu = false)}>How it works</a
        ><a href="#formats" onclick={() => (menu = false)}>Your tools</a>
      </div>
      <div class="nav-actions">
        <a class="signin" href={workspace}>Sign in</a><a class="nav-cta" href={workspace}
          >Open workspace <ArrowUpRight size={14} /></a
        ><button class="mobile-toggle" aria-label="Toggle navigation" onclick={() => (menu = !menu)}
          >{#if menu}<X size={20} />{:else}<Menu size={20} />{/if}</button
        >
      </div>
    </nav>
  </header>
  <main>
    <section class="hero">
      <div class="hero-copy">
        <div class="eyebrow"><span></span> A SHARED HOME FOR YOUR RESEARCH</div>
        <h1>Good research.<br />Better <em>together.</em></h1>
        <p>
          The document. The questionnaire. The spreadsheet.<br class="desktop" /> Bring your team’s
          work together—with every change<br class="desktop" /> understood, and every voice in the conversation.
        </p>
        <div class="hero-actions">
          <a class="button solid" href={workspace}>Start your workspace <ArrowRight size={18} /></a
          ><a class="watch" href="#workflow">See how it works <span>↓</span></a>
        </div>
        <div class="hero-foot">
          <span><Check size={14} /> Familiar editing</span><span
            ><Check size={14} /> Clear version history</span
          ><span><Check size={14} /> Your team, in sync</span>
        </div>
      </div>
      <div class="hero-visual" aria-label="Illustration of a shared research document">
        <div class="orbit orbit-one"></div>
        <div class="orbit orbit-two"></div>
        <div class="small-label">LESS “FINAL_FINAL_V3”. MORE PROGRESS.</div>
        <div class="floating-note">
          <span class="note-icon"><MessageSquare size={15} /></span>
          <div><b>A clearer question?</b><small>Keep the conversation in context.</small></div>
        </div>
        <div class="mock-app">
          <div class="mock-title">
            <span class="file-icon"><FileText size={18} /></span>
            <div>
              <b>Understanding everyday livelihoods</b><small
                >Team version <span>·</span> All changes saved</small
              >
            </div>
            <div class="avatars"><i>AS</i><i>RM</i><i>+8</i></div>
          </div>
          <div class="mock-menu">
            File <span>Edit</span><span>View</span><span>Insert</span><span>Format</span><span
              class="mock-share"><Users size={11} /> Share</span
            >
          </div>
          <div class="mock-toolbar">
            <span>100%⌄</span><span>Normal text⌄</span><span class="font-name">Arial⌄</span><b>B</b
            ><em>I</em><u>U</u><span class="toolbar-rule"></span><span>≡</span><span>☷</span>
          </div>
          <div class="mock-body">
            <div class="mock-page">
              <p class="paper-label">HOUSEHOLD WELLBEING STUDY</p>
              <h3>A better picture<br />of everyday life.</h3>
              <p class="paper-intro">
                Understanding how households work, live, and make decisions helps us ask better
                questions.
              </p>
              <h4>01 &nbsp; Livelihoods & resources</h4>
              <p class="question">
                What is your household’s <span class="suggestion">main source of income?</span>
              </p>
              <div class="answer-line"></div>
              <p class="question">How has this changed over the past year?</p>
              <div class="answer-line short"></div>
              <div class="cursor-marker"><span></span>Rafi is editing</div>
            </div>
            <div class="mock-comment">
              <div class="comment-top"><i>AS</i><b>Ayesha</b><span>just now</span></div>
              <p>“Main source” might be clearer than “primary livelihood”.</p>
              <div class="comment-actions">
                <span><Check size={11} /> Accepted</span><small>View change</small>
              </div>
            </div>
          </div>
        </div>
        <div class="version-float">
          <span><History size={17} /></span>
          <div><b>Every version has a story.</b><small>And you can always find it.</small></div>
          <CheckCheck size={17} />
        </div>
      </div>
    </section>
    <div class="research-strip">
      <span>FOR THE WORK BEHIND THE FINDINGS</span>
      <div>
        <span>Social research</span><span>Field studies</span><span>Public health</span><span
          >Policy research</span
        ><span>Academic teams</span>
      </div>
    </div>
    <section id="why" class="why section">
      <div class="section-heading">
        <div>
          <p class="eyebrow">ROOM FOR THE WHOLE PROCESS</p>
          <h2>Less chasing files.<br /><em>More finding answers.</em></h2>
        </div>
        <p>
          Research is collaborative. Your workspace should be, too. Keep the writing, the questions,
          and the decisions connected.
        </p>
      </div>
      <div class="feature-grid">
        <article>
          <div class="feature-icon"><FileText size={23} /></div>
          <h3>Feels like the tools you know.</h3>
          <p>
            A document when you’re writing. A table when you’re organizing. Familiar controls keep
            your attention on the research.
          </p>
          <div class="mini-format">
            <span>Heading 1⌄</span><b>B</b><em>I</em><u>U</u><span class="mini-highlight">Aa</span>
          </div>
        </article>
        <article>
          <div class="feature-icon"><GitCompareArrows size={23} /></div>
          <h3>Changes you can understand.</h3>
          <p>
            See what changed and who changed it. Try a new direction in a personal draft, then
            review it with your team.
          </p>
          <div class="mini-diff">
            <span class="deleted">primary livelihood</span><ArrowRight size={12} /><span
              class="added">main source of income</span
            >
          </div>
        </article>
        <article>
          <div class="feature-icon"><Users size={23} /></div>
          <h3>One team. One shared picture.</h3>
          <p>
            Share with an editor, invite a reviewer, or give someone a read-only view. Suggestions
            stay connected to the work.
          </p>
          <div class="mini-people">
            <i>AS</i><i>RM</i><i>NK</i><span>Different perspectives. Better questions.</span>
          </div>
        </article>
      </div>
    </section>
    <section id="workflow" class="workflow-wrap">
      <div class="workflow section">
        <div class="workflow-copy">
          <p class="eyebrow">FROM FIRST DRAFT TO AGREED VERSION</p>
          <h2>A natural flow.<br /><em>A clearer trail.</em></h2>
          <p class="workflow-lede">
            Explore an example of how a research team moves its work forward.
          </p>
          <div class="steps" role="tablist" aria-label="Research workflow">
            {#each steps as item, i}<button
                role="tab"
                aria-selected={step === i}
                aria-controls="workflow-demo"
                class:active={step === i}
                onclick={() => (step = i)}
                ><span class="step-no">0{i + 1}</span>
                <div>
                  <b>{item.title}</b>{#if step === i}<p>{item.copy}</p>{/if}
                </div>
                <ChevronRight size={17} /></button
              >{/each}
          </div>
        </div>
        <div class="workflow-demo" id="workflow-demo" role="tabpanel" tabindex="0">
          <div class="demo-head">
            <span class="status-dot"></span> Household wellbeing study
            <span>Example workspace</span>
          </div>
          <div class="demo-content">
            {#key step}{#if step === 0}<div class="demo-screen">
                  <div class="drop-zone">
                    <Upload size={34} />
                    <h3>Your next study starts here.</h3>
                    <p>Bring a document or table into the workspace.</p>
                    <div class="file-chips">
                      <span><FileText size={15} /> interview-guide.docx</span><span
                        ><Table2 size={15} /> questionnaire.xlsx</span
                      >
                    </div>
                  </div>
                  <div class="demo-file">
                    <FileText size={20} />
                    <div><b>Interview guide</b><small>Ready for your team</small></div>
                    <Check size={16} />
                  </div>
                </div>{:else if step === 1}<div class="demo-screen">
                  <div class="draft-badge"><span></span> Team version</div>
                  <h3>Let’s ask a better question.</h3>
                  <p class="demo-prose">
                    What challenges have you experienced<br />in accessing
                    <mark>healthcare in your area</mark>?
                  </p>
                  <div class="suggest-card">
                    <div><i>NK</i><b>Nadia suggested a change</b></div>
                    <p>Could we add “in the past six months” to make the recall period clearer?</p>
                    <div class="accepted">
                      <Check size={14} /> A specific suggestion, ready for review
                    </div>
                  </div>
                  <div class="demo-bottom">
                    <Users size={15} /> The discussion stays with the question.
                  </div>
                </div>{:else if step === 2}<div class="demo-screen">
                  <div class="draft-badge"><GitCompareArrows size={13} /> Compare versions</div>
                  <h3>Small changes. Full context.</h3>
                  <div class="compare">
                    <div>
                      <small>PREVIOUS VERSION</small>
                      <p>How often do you use health services?</p>
                    </div>
                    <div class="after">
                      <small>PROPOSED CHANGE</small>
                      <p>In the past six months, how many times have you used health services?</p>
                    </div>
                  </div>
                  <div class="demo-bottom">
                    <ShieldCheck size={15} /> Your previous versions remain in the history.
                  </div>
                </div>{:else}<div class="demo-screen">
                  <div class="finished"><CheckCheck size={30} /></div>
                  <h3>A version everyone can stand behind.</h3>
                  <p class="demo-prose">
                    Name this milestone. Keep the decisions.<br />Take your work into the next part
                    of the study.
                  </p>
                  <div class="version-row">
                    <History size={20} />
                    <div>
                      <b>Ready for field review</b><small
                        >Named version · Team-approved wording</small
                      >
                    </div>
                    <Check size={17} />
                  </div>
                  <div class="demo-bottom">
                    <Download size={15} /> Download a copy when you need it.
                  </div>
                </div>{/if}{/key}
          </div>
          <div class="demo-pagination">
            {#each steps.map((_, i) => i) as i}<button
                aria-label={`Show step ${i + 1}`}
                class:chosen={step === i}
                onclick={() => (step = i)}
              ></button>{/each}
          </div>
        </div>
      </div>
    </section>
    <section id="formats" class="formats section">
      <div class="format-copy">
        <p class="eyebrow">PART OF YOUR RESEARCH TOOLKIT</p>
        <h2>Your work goes<br /><em>where you need it.</em></h2>
        <p>
          Start with the files your team already uses. Keep document editing, tables, and
          questionnaire specifications close together.
        </p>
        <a href={workspace} class="inline-link">Explore the workspace <ArrowUpRight size={16} /></a>
      </div>
      <div class="format-board">
        <div class="format-row">
          <span class="format-icon blue"><FileText size={23} /></span>
          <div>
            <b>Documents</b>
            <p>Interview guides, protocols, field notes</p>
          </div>
          <span class="extensions">DOCX</span>
        </div>
        <div class="format-row">
          <span class="format-icon green"><Table2 size={23} /></span>
          <div>
            <b>Spreadsheets & tables</b>
            <p>Codebooks, question banks, study tables</p>
          </div>
          <span class="extensions">XLSX · CSV</span>
        </div>
        <div class="format-row">
          <span class="format-icon amber"><Layers size={23} /></span>
          <div>
            <b>Research instruments</b>
            <p>Structured questions, choices, and logic</p>
          </div>
          <span class="extensions">XLSForm</span>
        </div>
        <p class="format-note">
          Conversion support varies by format. Review the import report before using a converted
          instrument in the field.
        </p>
      </div>
    </section>
    <section class="closing section">
      <div class="closing-grain"></div>
      <span class="closing-icon"><Sparkles size={24} /></span>
      <h2>Make room for<br />your team’s best thinking.</h2>
      <p>A calmer place for the messy, meaningful work of research.</p>
      <a class="button light" href={workspace}>Open your workspace <ArrowRight size={17} /></a>
      <div class="closing-note">
        <ShieldCheck size={13} /> Private documents. Thoughtful collaboration.
      </div>
    </section>
  </main>
  <footer class="footer section">
    <a class="wordmark" href="/"><span class="logo-mark">d<span></span></span>dynodoc</a>
    <p>Made for the people behind the research.</p>
    <div>
      <a href="#workflow">How it works</a><a href={workspace}
        >Workspace <ArrowUpRight size={13} /></a
      >
    </div>
    <small
      >© {new Date().getFullYear()} Dynodoc · <a href="/source">Source code · AGPL-3.0</a></small
    >
  </footer>
</div>

<style>
  .landing {
    --green: #224f43;
    --ink: #1c332d;
    --muted: #74817c;
    background: #fdfdfa;
    color: var(--ink);
    font-family: var(--font-ui);
    font-size: 14px;
    overflow: hidden;
  }
  .landing a {
    text-decoration: none;
    color: inherit;
  }
  .nav-wrap {
    border-bottom: 1px solid #e7ebe3;
    background: #fdfdfaea;
    position: sticky;
    top: 0;
    z-index: 20;
    backdrop-filter: blur(15px);
  }
  .nav {
    max-width: 1240px;
    margin: auto;
    height: 84px;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 30px;
    padding: 0 40px;
  }
  .wordmark {
    display: flex;
    align-items: center;
    gap: 9px;
    font-size: 23px;
    font-weight: 600;
    letter-spacing: -0.9px;
  }
  .logo-mark {
    display: inline-flex;
    position: relative;
    font: bold 36px/1 Georgia;
    align-items: center;
    justify-content: center;
    color: var(--green);
    width: 28px;
  }
  .logo-mark span {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: #cfaa6d;
    position: absolute;
    right: -1px;
    bottom: 2px;
  }
  .nav-links {
    display: flex;
    gap: 30px;
    font-size: 12px;
    color: #52645c;
  }
  .nav-actions {
    display: flex;
    align-items: center;
    gap: 23px;
    font-size: 12px;
  }
  .nav-cta {
    display: flex;
    gap: 15px;
    align-items: center;
    color: white !important;
    background: var(--green);
    padding: 11px 17px;
    border-radius: 7px;
  }
  .nav a:hover {
    opacity: 0.8;
  }
  .mobile-toggle {
    display: none;
    background: none;
    border: 0;
    color: inherit;
  }
  .hero {
    max-width: 1320px;
    margin: auto;
    display: grid;
    grid-template-columns: 0.96fr 1.04fr;
    gap: 20px;
    padding: 100px 70px 94px;
    min-height: 650px;
    align-items: center;
  }
  .eyebrow {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 1.75px;
    color: #708278;
    margin: 0 0 24px;
  }
  .eyebrow > span,
  .status-dot {
    width: 6px;
    height: 6px;
    background: #61937a;
    border-radius: 100%;
    display: inline-block;
    box-shadow: 0 0 0 4px #61937a10;
  }
  h1 {
    font:
      500 clamp(48px, 5vw, 70px)/1.09 Georgia,
      serif;
    letter-spacing: -2.8px;
    margin: 0 0 28px;
  }
  h1 em,
  h2 em {
    font-weight: 400;
    color: #69836e;
  }
  h1 em {
    color: #627d65;
  }
  .hero-copy > p {
    font-size: 14px;
    line-height: 1.9;
    color: #78847d;
    margin: 0 0 30px;
  }
  .hero-actions {
    display: flex;
    align-items: center;
    gap: 23px;
  }
  .button {
    display: inline-flex;
    align-items: center;
    gap: 20px;
    border-radius: 7px;
    padding: 14px 19px;
    font-size: 12px;
    font-weight: 500;
    transition:
      transform 0.2s,
      box-shadow 0.2s;
  }
  .button:hover {
    transform: translateY(-2px);
    box-shadow: 0 6px 20px #193b2920;
  }
  .solid {
    background: var(--green);
    color: white !important;
  }
  .watch {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 12px;
    color: #52675a !important;
  }
  .watch > span {
    width: 24px;
    height: 24px;
    display: grid;
    place-items: center;
    border: 1px solid #dfe6d9;
    border-radius: 50%;
  }
  .hero-foot {
    display: flex;
    gap: 17px;
    margin-top: 24px;
    font-size: 9px;
    color: #78897f;
  }
  .hero-foot > span {
    display: flex;
    gap: 5px;
    align-items: center;
  }
  .hero-visual {
    position: relative;
    min-height: 405px;
    display: flex;
    align-items: center;
  }
  .orbit {
    position: absolute;
    z-index: 0;
    border: 1px solid #d9e1d780;
    border-radius: 50%;
    width: 430px;
    height: 430px;
    top: -10px;
    left: 50px;
  }
  .orbit-two {
    width: 510px;
    height: 510px;
    top: -50px;
    left: 10px;
    border-style: dashed;
    opacity: 0.5;
  }
  .small-label {
    position: absolute;
    top: -14px;
    left: 92px;
    font-size: 7px;
    letter-spacing: 1.7px;
    color: #91a28d;
    transform: rotate(-3deg);
  }
  .mock-app {
    position: relative;
    width: 600px;
    flex-shrink: 0;
    height: 375px;
    background: white;
    border: 1px solid #dce3df;
    border-radius: 10px;
    box-shadow:
      0 14px 60px #224f4310,
      0 2px 5px #224f430a;
    transform: rotate(-2deg);
    overflow: hidden;
  }
  .mock-title {
    height: 54px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 16px;
  }
  .file-icon {
    color: #53846e;
  }
  .mock-title b {
    display: block;
    font-size: 10px;
    font-weight: 500;
  }
  .mock-title small {
    display: block;
    font-size: 7px;
    color: #8d9a93;
    margin-top: 3px;
  }
  .mock-title small span {
    padding: 0 4px;
  }
  .avatars {
    margin-left: auto;
    display: flex;
    padding-right: 2px;
  }
  .avatars i,
  .mini-people i,
  .comment-top i,
  .suggest-card i {
    display: grid;
    place-items: center;
    border-radius: 50%;
    font-style: normal;
    width: 26px;
    height: 26px;
    border: 2px solid white;
    background: #e9daca;
    font-size: 8px;
    margin-left: -5px;
    color: #78553a;
  }
  .avatars i:nth-child(2),
  .mini-people i:nth-child(2) {
    background: #dce6de;
    color: #426854;
  }
  .avatars i:last-child,
  .mini-people i:last-of-type {
    background: #ebe7f1;
    color: #77608d;
  }
  .mock-menu {
    height: 21px;
    font-size: 8px;
    padding: 0 48px;
    display: flex;
    gap: 14px;
    position: relative;
  }
  .mock-share {
    position: absolute;
    right: 20px;
    top: -13px;
    padding: 6px 10px;
    background: #eaf1e7;
    border-radius: 5px;
    display: flex;
    align-items: center;
    gap: 4px;
    color: #56704f;
  }
  .mock-toolbar {
    height: 32px;
    background: #f3f6f4;
    display: flex;
    align-items: center;
    padding: 0 20px;
    gap: 18px;
    font: 8px Arial;
    color: #61716a;
    border-top: 1px solid #e9eeeb;
    border-bottom: 1px solid #e9eeeb;
  }
  .toolbar-rule {
    height: 16px;
    width: 1px;
    background: #dce4df;
  }
  .mock-body {
    height: 268px;
    background: #f5f7f5;
    padding: 20px 95px 0 35px;
    position: relative;
  }
  .mock-page {
    background: white;
    min-height: 310px;
    max-width: 365px;
    padding: 30px 34px;
    position: relative;
    border: 1px solid #e9ece8;
    box-shadow: 0 2px 6px #102b1304;
  }
  .paper-label {
    font: 6px Arial !important;
    letter-spacing: 1px;
    color: #a0aba2;
    margin: 0 0 14px !important;
  }
  .mock-page h3 {
    font: 21px/1.2 Georgia;
    margin: 0 0 13px;
    color: #314639;
  }
  .paper-intro {
    font: 8px/1.8 Arial;
    color: #839086;
    max-width: 250px;
  }
  .mock-page h4 {
    font: 600 8px Arial;
    margin: 20px 0 14px;
    color: #516958;
  }
  .question {
    font: 8px/1.6 Arial;
    color: #566358;
    margin: 12px 0;
  }
  .suggestion {
    background: #e7f2d5;
    border-bottom: 1px solid #7eab68;
    padding: 2px;
  }
  .answer-line {
    height: 1px;
    background: #e7eae5;
    width: 92%;
    margin-top: 18px;
  }
  .answer-line.short {
    width: 70%;
  }
  .cursor-marker {
    position: absolute;
    right: 15px;
    bottom: 47px;
    background: #ba906b;
    color: white;
    font: 6px Arial;
    padding: 3px 5px;
    border-radius: 3px;
  }
  .cursor-marker span {
    position: absolute;
    left: -1px;
    bottom: 0;
    height: 22px;
    border-left: 1px solid #ba906b;
  }
  .mock-comment {
    position: absolute;
    right: 16px;
    top: 106px;
    background: white;
    border: 1px solid #dee5db;
    box-shadow: 0 4px 15px #1e453808;
    border-radius: 6px;
    width: 164px;
    padding: 11px;
  }
  .comment-top {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .comment-top i {
    width: 19px;
    height: 19px;
    margin: 0;
    font-size: 6px;
  }
  .comment-top b {
    font-size: 7px;
  }
  .comment-top > span {
    font-size: 6px;
    color: #a1aaa5;
    margin-left: auto;
  }
  .mock-comment p {
    font: 8px/1.6 Arial;
    color: #66796b;
    margin: 8px 0;
  }
  .comment-actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .comment-actions > span {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 6px;
    color: #668154;
  }
  .comment-actions small {
    font-size: 6px;
    color: #929c96;
  }
  .floating-note {
    position: absolute;
    top: -8px;
    right: -35px;
    z-index: 2;
    background: white;
    border: 1px solid #e7e6dc;
    border-radius: 8px;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 16px;
    transform: rotate(3deg);
    box-shadow: 0 8px 28px #28483009;
    animation: float 7s ease-in-out infinite;
  }
  .floating-note b,
  .version-float b {
    display: block;
    font-size: 9px;
    font-weight: 500;
  }
  .floating-note small,
  .version-float small {
    font-size: 7px;
    color: #87968a;
  }
  .note-icon {
    background: #f4eedf;
    color: #b59756;
    display: grid;
    place-items: center;
    width: 29px;
    height: 29px;
    border-radius: 7px;
  }
  .version-float {
    position: absolute;
    bottom: -7px;
    left: 65px;
    z-index: 3;
    display: flex;
    align-items: center;
    gap: 12px;
    background: white;
    padding: 13px 17px;
    border: 1px solid #e0e7db;
    border-radius: 8px;
    box-shadow: 0 6px 20px #224f4310;
    transform: rotate(2deg);
    animation: float 7s ease-in-out infinite reverse;
  }
  .version-float > span {
    color: #7e9a63;
    background: #eff4e7;
    padding: 7px;
    border-radius: 7px;
    display: flex;
  }
  .version-float > :global(svg) {
    margin-left: 25px;
    color: #8ca274;
  }
  .research-strip {
    max-width: 1180px;
    margin: 0 auto;
    border-top: 1px solid #e6eae0;
    border-bottom: 1px solid #e6eae0;
    padding: 25px 0 28px;
    text-align: center;
  }
  .research-strip > span {
    font-size: 8px;
    letter-spacing: 2px;
    color: #9ba499;
  }
  .research-strip > div {
    display: flex;
    justify-content: center;
    gap: 65px;
    margin-top: 16px;
    font: 17px Georgia;
    color: #7d8a7a;
  }
  .section {
    max-width: 1180px;
    margin-left: auto;
    margin-right: auto;
  }
  .why {
    padding: 96px 0;
  }
  .section-heading {
    display: flex;
    align-items: end;
    justify-content: space-between;
    gap: 50px;
    margin-bottom: 42px;
  }
  h2 {
    font: 400 42px/1.18 Georgia;
    letter-spacing: -1.4px;
    margin: 0;
  }
  .section-heading > p {
    max-width: 310px;
    color: #859081;
    font-size: 12px;
    line-height: 1.9;
    margin: 0 0 4px;
  }
  .section-heading .eyebrow {
    margin-bottom: 18px;
  }
  .feature-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 20px;
  }
  .feature-grid article {
    padding: 30px 27px 24px;
    border: 1px solid #e5e9df;
    border-radius: 10px;
    background: #fff;
  }
  .feature-icon {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 44px;
    height: 44px;
    border-radius: 10px;
    background: #f0f4eb;
    color: #739068;
    margin-bottom: 21px;
  }
  .feature-grid h3 {
    font: 500 19px Georgia;
    margin: 0 0 12px;
  }
  .feature-grid p {
    font-size: 11px;
    line-height: 1.9;
    color: #899183;
    margin: 0 0 23px;
  }
  .mini-format,
  .mini-diff,
  .mini-people {
    min-height: 37px;
    border-top: 1px solid #eff0e9;
    padding-top: 18px;
    display: flex;
    align-items: center;
    gap: 13px;
    font-size: 11px;
    color: #72806f;
  }
  .mini-format > span:first-child {
    font-size: 9px;
    margin-right: 15px;
  }
  .mini-highlight {
    background: #f4e5a6;
    padding: 0 4px;
    border-radius: 3px;
  }
  .mini-diff {
    gap: 7px;
    font-size: 8px;
  }
  .deleted {
    color: #ad8581;
    text-decoration: line-through;
    background: #f7efed;
    padding: 4px;
  }
  .added {
    color: #628461;
    background: #edf4e8;
    padding: 4px;
  }
  .mini-people {
    gap: 0;
  }
  .mini-people i {
    width: 24px;
    height: 24px;
    font-size: 7px;
  }
  .mini-people i:first-child {
    margin: 0;
  }
  .mini-people > span {
    font-size: 8px;
    margin-left: 9px;
    color: #92998b;
  }
  .workflow-wrap {
    background: #f1f4ec;
    border-top: 1px solid #e9eddf;
    border-bottom: 1px solid #e9eddf;
  }
  .workflow {
    padding: 80px 0;
    display: grid;
    grid-template-columns: 1fr 1.12fr;
    gap: 100px;
    align-items: center;
  }
  .workflow-lede {
    font-size: 12px;
    color: #82927d;
    line-height: 1.9;
    max-width: 330px;
    margin: 22px 0;
  }
  .steps {
    display: grid;
    gap: 5px;
  }
  .steps button {
    display: flex;
    align-items: flex-start;
    width: 100%;
    text-align: left;
    background: none;
    border: 0;
    border-radius: 8px;
    padding: 17px;
    gap: 15px;
    color: #89987e;
    cursor: pointer;
  }
  .step-no {
    font-size: 10px;
    letter-spacing: 1px;
    line-height: 20px;
    opacity: 0.7;
  }
  .steps b {
    font-size: 12px;
    font-weight: 500;
  }
  .steps button > :global(svg) {
    margin-left: auto;
    opacity: 0.5;
    margin-top: 2px;
  }
  .steps p {
    font-size: 11px;
    line-height: 1.8;
    max-width: 250px;
    margin: 7px 0 0;
    color: #89987e;
  }
  .steps button.active {
    background: #fff;
    color: #46623d;
    box-shadow: 0 3px 12px #30482703;
  }
  .workflow-demo {
    border-radius: 12px;
    border: 1px solid #dfe5d7;
    background: #fff;
    box-shadow: 0 18px 50px #264b2209;
    min-height: 396px;
    overflow: hidden;
  }
  .demo-head {
    height: 49px;
    border-bottom: 1px solid #edf0e8;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 19px;
    font-size: 9px;
    color: #60745c;
  }
  .demo-head > span:last-child {
    margin-left: auto;
    font-size: 8px;
    color: #a3ad9d;
  }
  .demo-content {
    padding: 30px;
  }
  .demo-screen {
    animation: appear 0.4s ease;
    min-height: 270px;
  }
  .draft-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 8px;
    background: #f0f5e9;
    color: #76915f;
    padding: 5px 8px;
    border-radius: 5px;
    margin-bottom: 10px;
  }
  .draft-badge > span {
    height: 4px;
    width: 4px;
    border-radius: 100%;
    background: #7c9c67;
  }
  .demo-screen h3 {
    font: 400 24px/1.3 Georgia;
    margin: 9px 0 14px;
    color: #3f5738;
    letter-spacing: -0.4px;
  }
  .demo-prose {
    font: 12px/1.9 Georgia;
    color: #78856c;
  }
  .demo-prose mark {
    background: #ecf2dd;
    color: inherit;
  }
  .suggest-card {
    margin-top: 21px;
    background: #fcfdf9;
    border: 1px solid #e5ebdb;
    border-radius: 8px;
    padding: 14px;
  }
  .suggest-card > div:first-child {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .suggest-card i {
    margin: 0;
    width: 22px;
    height: 22px;
    font-size: 7px;
    background: #e9e4cf;
    color: #8f7f46;
  }
  .suggest-card b {
    font-size: 9px;
    color: #6d7e5f;
    font-weight: 500;
  }
  .suggest-card > p {
    font-size: 10px;
    line-height: 1.8;
    color: #829374;
  }
  .accepted {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 8px;
    color: #7c9866;
    margin-top: 13px;
  }
  .demo-bottom {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 8px;
    color: #98a38c;
    margin-top: 25px;
  }
  .demo-pagination {
    display: flex;
    gap: 5px;
    justify-content: center;
    padding-bottom: 19px;
  }
  .demo-pagination button {
    height: 5px;
    width: 5px;
    border: 0;
    border-radius: 20px;
    background: #dee5d6;
    padding: 0;
    cursor: pointer;
  }
  .demo-pagination .chosen {
    width: 18px;
    background: #839d6e;
  }
  .drop-zone {
    text-align: center;
    border: 1px dashed #cbd9bf;
    border-radius: 9px;
    background: #f8faf4;
    padding: 25px 10px;
    color: #8ba777;
  }
  .drop-zone > :global(svg) {
    margin: auto;
  }
  .drop-zone h3 {
    font-size: 21px;
  }
  .drop-zone p {
    font-size: 10px;
    color: #9aa88d;
  }
  .file-chips {
    display: flex;
    justify-content: center;
    gap: 7px;
    margin-top: 20px;
  }
  .file-chips > span {
    display: flex;
    align-items: center;
    gap: 5px;
    padding: 6px;
    border: 1px solid #e2e9d8;
    background: white;
    border-radius: 5px;
    font-size: 8px;
  }
  .demo-file,
  .version-row {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 18px;
    border: 1px solid #e8eddf;
    border-radius: 6px;
    padding: 12px;
    color: #7d9768;
  }
  .demo-file b,
  .version-row b {
    font-size: 10px;
    display: block;
    color: #627c4f;
  }
  .demo-file small,
  .version-row small {
    display: block;
    font-size: 8px;
    color: #9aa88e;
  }
  .demo-file > :global(svg):last-child,
  .version-row > :global(svg):last-child {
    margin-left: auto;
  }
  .compare {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    margin-top: 23px;
  }
  .compare > div {
    background: #f7f4ee;
    border-radius: 7px;
    padding: 17px;
    min-height: 131px;
  }
  .compare .after {
    background: #f0f5e8;
  }
  .compare small {
    font-size: 7px;
    letter-spacing: 0.8px;
    color: #a49b87;
  }
  .compare p {
    font: 13px/1.7 Georgia;
    color: #827660;
    margin-bottom: 0;
  }
  .compare .after p {
    color: #6b8055;
  }
  .finished {
    background: #f0f5e8;
    color: #819d60;
    display: grid;
    place-items: center;
    width: 51px;
    height: 51px;
    border-radius: 11px;
    margin: 0 0 17px;
  }
  .formats {
    padding: 90px 0;
    display: grid;
    grid-template-columns: 1fr 1.2fr;
    gap: 100px;
    align-items: center;
  }
  .format-copy > p:not(.eyebrow) {
    font-size: 12px;
    line-height: 1.9;
    color: #8a9483;
    max-width: 335px;
    margin: 24px 0;
  }
  .inline-link {
    font-size: 11px;
    display: inline-flex;
    align-items: center;
    gap: 10px;
    color: #597247 !important;
  }
  .format-board {
    border: 1px solid #e4e8db;
    border-radius: 12px;
    background: #fff;
    padding: 8px 25px 13px;
  }
  .format-row {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 24px 0;
    border-bottom: 1px solid #eef0e8;
  }
  .format-icon {
    width: 43px;
    height: 46px;
    border-radius: 9px;
    display: grid;
    place-items: center;
  }
  .blue {
    background: #edf2fa;
    color: #7c9cbf;
  }
  .green {
    background: #ecf4e9;
    color: #80a06d;
  }
  .amber {
    background: #f7f1e4;
    color: #b5a064;
  }
  .format-row b {
    font-size: 12px;
    font-weight: 500;
    color: #657650;
  }
  .format-row p {
    font-size: 9px;
    color: #9ca58e;
    margin: 3px 0 0;
  }
  .extensions {
    font-size: 8px;
    letter-spacing: 0.5px;
    margin-left: auto;
    color: #9ca58e;
  }
  .format-note {
    font-size: 9px;
    color: #a2a893;
    line-height: 1.8;
    margin: 16px 0 0;
  }
  .closing {
    position: relative;
    background: #244e40;
    border-radius: 14px;
    text-align: center;
    padding: 56px 20px;
    color: #fff;
    overflow: hidden;
  }
  .closing-icon {
    position: relative;
    color: #b5c78e;
  }
  .closing h2 {
    position: relative;
    font-size: 45px;
    letter-spacing: -1.5px;
    margin: 16px 0;
    color: #f7f5e7;
  }
  .closing > p {
    position: relative;
    font-size: 12px;
    color: #a8bdae;
    margin: 16px 0 26px;
  }
  .light {
    position: relative;
    background: #f8f7ee;
    color: #3e5d43 !important;
  }
  .closing-note {
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    font-size: 9px;
    color: #8ba990;
    margin-top: 19px;
  }
  .closing-grain {
    position: absolute;
    width: 800px;
    height: 800px;
    top: -100px;
    left: 50%;
    transform: translateX(-50%);
    background: repeating-radial-gradient(
      circle,
      transparent 0px,
      transparent 55px,
      #9abc8410 56px,
      #9abc8410 57px
    );
    opacity: 0.75;
  }
  .footer {
    padding: 40px 0 35px;
    display: grid;
    grid-template-columns: 1fr auto;
    align-items: center;
    gap: 6px;
  }
  .footer .wordmark {
    font-size: 21px;
  }
  .footer > p {
    font-size: 10px;
    color: #99a18d;
    margin: 5px 0;
    grid-row: 2;
  }
  .footer > div {
    display: flex;
    gap: 25px;
    grid-column: 2;
    grid-row: 1/3;
    font-size: 10px;
    color: #7d8f70;
  }
  .footer > div a {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .footer > small {
    grid-column: 1/-1;
    border-top: 1px solid #e8ebdf;
    margin-top: 24px;
    padding-top: 22px;
    font-size: 8px;
    color: #a7ad9b;
  }
  @keyframes float {
    50% {
      translate: 0 -7px;
    }
  }
  @keyframes appear {
    from {
      opacity: 0;
      transform: translateY(5px);
    }
    to {
      opacity: 1;
      transform: none;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    * {
      animation: none !important;
      transition: none !important;
      scroll-behavior: auto !important;
    }
  }
  @media (min-width: 1400px) {
    .hero {
      padding-left: 40px;
      padding-right: 40px;
    }
  }
  @media (max-width: 1250px) {
    .section,
    .research-strip {
      margin-left: 50px;
      margin-right: 50px;
    }
    .hero {
      padding: 85px 50px;
    }
    .hero-visual {
      transform: scale(0.86);
      transform-origin: left center;
    }
    .hero {
      gap: 0;
    }
    .research-strip > div {
      gap: 35px;
    }
    .workflow,
    .formats {
      gap: 50px;
    }
  }
  @media (max-width: 960px) {
    .nav {
      padding: 0 25px;
      height: 72px;
    }
    .nav-links {
      gap: 18px;
    }
    .nav-actions {
      gap: 15px;
    }
    .hero {
      padding: 70px 35px;
      grid-template-columns: 1fr;
      gap: 60px;
    }
    .hero-copy {
      text-align: center;
    }
    .hero-copy .eyebrow,
    .hero-actions,
    .hero-foot {
      justify-content: center;
    }
    .hero-visual {
      transform: none;
      max-width: 550px;
      margin: auto;
      min-height: 400px;
      width: 100%;
    }
    .mock-app {
      width: 100%;
    }
    .floating-note {
      right: -12px;
    }
    .hero-copy > p {
      font-size: 15px;
    }
    .research-strip > div {
      gap: 24px;
      flex-wrap: wrap;
      font-size: 16px;
    }
    .section,
    .research-strip {
      margin-left: 35px;
      margin-right: 35px;
    }
    .why {
      padding: 70px 0;
    }
    .feature-grid {
      gap: 12px;
    }
    .feature-grid article {
      padding: 23px 18px;
    }
    .feature-grid h3 {
      font-size: 18px;
    }
    .mini-diff {
      flex-wrap: wrap;
    }
    .workflow {
      gap: 35px;
      grid-template-columns: 0.9fr 1.1fr;
    }
    .demo-content {
      padding: 20px;
    }
    .formats {
      gap: 35px;
      grid-template-columns: 1fr 1.2fr;
    }
    .format-board {
      padding: 8px 17px 13px;
    }
    .format-row {
      gap: 10px;
    }
    .extensions {
      font-size: 7px;
    }
    .format-row p {
      font-size: 8px;
    }
    h2 {
      font-size: 37px;
    }
  }
  @media (max-width: 680px) {
    .nav {
      gap: 14px;
      padding: 0 20px;
      height: 65px;
    }
    .nav-links {
      display: none;
      position: absolute;
      top: 65px;
      left: 0;
      right: 0;
      background: #fdfdf9;
      border-bottom: 1px solid #e5eadf;
      padding: 22px;
      flex-direction: column;
    }
    .nav-links.mobile-open {
      display: flex;
    }
    .nav-cta {
      display: none;
    }
    .mobile-toggle {
      display: flex;
    }
    .nav-actions {
      gap: 18px;
    }
    .wordmark {
      font-size: 21px;
    }
    .hero {
      padding: 55px 23px 48px;
      gap: 42px;
      min-height: 0;
    }
    .hero .eyebrow {
      font-size: 7px;
      letter-spacing: 1.3px;
      margin-bottom: 21px;
    }
    h1 {
      font-size: 51px;
      letter-spacing: -2px;
      margin-bottom: 22px;
    }
    .hero-copy > p {
      font-size: 12px;
    }
    .hero-actions {
      gap: 19px;
    }
    .button {
      font-size: 11px;
      padding: 13px 16px;
      gap: 12px;
    }
    .watch {
      font-size: 11px;
      gap: 7px;
    }
    .hero-foot {
      font-size: 7px;
      gap: 10px;
      margin-top: 20px;
    }
    .hero-foot :global(svg) {
      width: 11px;
    }
    .hero-visual {
      min-height: 290px;
      max-width: 400px;
      padding-top: 15px;
    }
    .mock-app {
      height: 280px;
      transform: rotate(-2deg);
    }
    .mock-title {
      height: 46px;
      padding: 0 11px;
      gap: 6px;
    }
    .mock-title b {
      font-size: 8px;
    }
    .mock-title small {
      font-size: 6px;
    }
    .avatars i {
      width: 22px;
      height: 22px;
      font-size: 6px;
    }
    .mock-menu {
      font-size: 7px;
      padding-left: 34px;
      gap: 12px;
    }
    .mock-toolbar {
      font-size: 7px;
      padding: 0 15px;
      gap: 13px;
      height: 28px;
    }
    .font-name {
      display: none;
    }
    .mock-body {
      padding: 15px 60px 0 15px;
    }
    .mock-page {
      padding: 22px;
      max-width: 280px;
    }
    .mock-page h3 {
      font-size: 18px;
    }
    .mock-comment {
      width: 120px;
      right: 9px;
      top: 85px;
      padding: 9px;
    }
    .mock-comment p {
      font-size: 6px;
    }
    .comment-top > span {
      display: none;
    }
    .paper-intro {
      font-size: 7px;
    }
    .mock-page h4,
    .question {
      font-size: 7px;
    }
    .floating-note {
      top: 0;
      right: -2px;
      padding: 8px 10px;
      gap: 6px;
    }
    .floating-note b,
    .version-float b {
      font-size: 7px;
    }
    .floating-note small,
    .version-float small {
      font-size: 6px;
    }
    .floating-note .note-icon {
      width: 23px;
      height: 23px;
    }
    .small-label {
      display: none;
    }
    .version-float {
      left: 20px;
      bottom: -14px;
      padding: 8px 12px;
      gap: 7px;
    }
    .version-float > span {
      padding: 5px;
    }
    .version-float > :global(svg) {
      margin-left: 10px;
    }
    .orbit {
      width: 300px;
      height: 300px;
      left: 20px;
    }
    .orbit-two {
      width: 340px;
      height: 340px;
      left: 0;
      top: -25px;
    }
    .research-strip {
      padding: 20px 0;
      margin: 0 23px;
    }
    .research-strip > span {
      font-size: 6px;
      letter-spacing: 1.3px;
    }
    .research-strip > div {
      font-size: 12px;
      gap: 12px 20px;
      margin-top: 14px;
    }
    .section {
      margin-left: 23px;
      margin-right: 23px;
    }
    .why {
      padding: 55px 0;
    }
    .section-heading {
      display: block;
      margin-bottom: 25px;
    }
    h2 {
      font-size: 34px;
      letter-spacing: -1px;
    }
    .eyebrow {
      font-size: 7px;
      letter-spacing: 1.5px;
      margin-bottom: 18px;
    }
    .section-heading > p {
      font-size: 11px;
      margin-top: 20px;
      max-width: none;
    }
    .feature-grid {
      grid-template-columns: 1fr;
      gap: 13px;
    }
    .feature-grid article {
      padding: 23px;
    }
    .feature-icon {
      width: 36px;
      height: 36px;
      margin-bottom: 15px;
    }
    .feature-grid h3 {
      font-size: 21px;
    }
    .feature-grid p {
      font-size: 12px;
      margin-bottom: 18px;
    }
    .mini-format,
    .mini-diff,
    .mini-people {
      padding-top: 13px;
    }
    .workflow {
      padding: 50px 0;
      grid-template-columns: 1fr;
      gap: 28px;
    }
    .workflow-lede {
      max-width: none;
      font-size: 11px;
    }
    .steps {
      grid-template-columns: 1fr 1fr;
      gap: 5px;
    }
    .steps button {
      padding: 13px 10px;
      gap: 8px;
      align-items: center;
    }
    .steps button p {
      display: none;
    }
    .steps b {
      font-size: 10px;
    }
    .steps button > :global(svg) {
      width: 13px;
    }
    .step-no {
      font-size: 8px;
    }
    .workflow-demo {
      min-height: 365px;
    }
    .demo-content {
      padding: 25px;
    }
    .demo-screen h3 {
      font-size: 24px;
    }
    .demo-head {
      font-size: 8px;
      padding: 0 15px;
    }
    .demo-head > span:last-child {
      font-size: 7px;
    }
    .formats {
      padding: 50px 0;
      grid-template-columns: 1fr;
      gap: 25px;
    }
    .format-copy > p:not(.eyebrow) {
      font-size: 11px;
      max-width: none;
      margin: 20px 0;
    }
    .format-row {
      padding: 20px 0;
      gap: 12px;
    }
    .format-row p {
      font-size: 9px;
    }
    .extensions {
      font-size: 8px;
    }
    .closing {
      padding: 40px 20px;
    }
    .closing h2 {
      font-size: 35px;
    }
    .closing > p {
      font-size: 10px;
    }
    .footer {
      padding: 30px 0;
    }
    .footer > p {
      font-size: 8px;
      max-width: 175px;
    }
    .footer > div {
      font-size: 9px;
      gap: 12px;
    }
    .footer > small {
      font-size: 7px;
    }
    .desktop {
      display: none;
    }
  }
</style>
