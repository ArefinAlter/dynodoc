<script lang="ts">
  import { onMount } from 'svelte';
  import { Search } from '@lucide/svelte';
  import { env } from '$env/dynamic/public';
  import { Btn, Eyebrow, reveal, scrollable, spotlight } from '$lib/components/site';
  import { workspaceHref } from '$lib/components/site/nav';
  import { ReviewMock, TimelineMock } from '$lib/components/mock';
  import Dino from '$lib/art/Dino.svelte';
  import Egg from '$lib/art/Egg.svelte';
  import { pageMeta } from '$lib/seo';
  import { guide, versionVocabulary } from '$lib/workspace/guide';

  const meta = pageMeta('/help');
  const title = meta?.title ?? 'Help · Dynodoc';
  const description =
    meta?.description ??
    'Create a project, capture changes from an editor, review and merge a pull request, restore earlier content and verify history.';
  const workspace = $derived(workspaceHref(env.PUBLIC_WORKSPACE_URL));

  const topics = [
    {
      id: 'first-project',
      title: 'Create your first project',
      keywords: 'new project upload file add file invite people settings'
    },
    {
      id: 'capture',
      title: 'Capture a change from an editor',
      keywords: 'add-in word excel powerpoint google docs sheets slides push edited file pane'
    },
    {
      id: 'review',
      title: 'Review a pull request',
      keywords: 'change request approve request changes comment conflict compare'
    },
    {
      id: 'merge',
      title: 'Merge, or send it back',
      keywords: 'merge decline approvals override apply preview'
    },
    {
      id: 'recover',
      title: 'Restore, revert and recover',
      keywords: 'history earlier revision restore revert cherry-pick download recover undo'
    },
    {
      id: 'versions',
      title: 'Named versions and private drafts',
      keywords: 'named version milestone draft branch private'
    },
    {
      id: 'roles',
      title: 'Roles and review rules',
      keywords: 'owner manager editor reviewer contributor viewer permission policy protect main'
    },
    {
      id: 'verify',
      title: 'Verify the history',
      keywords: 'hash chain integrity audit verify tamper'
    },
    {
      id: 'web-editors',
      title: 'The web editors',
      keywords: 'web editor tour documents spreadsheets presentations share public link trash'
    },
    {
      id: 'words',
      title: 'Version-control words, in plain language',
      keywords: 'git commit branch push pull merge conflict blame vocabulary'
    },
    {
      id: 'limits',
      title: 'Limits and when something goes wrong',
      keywords: 'limit size 3 mb recovery copies unsupported troubleshoot support'
    }
  ];

  const roles = [
    ['Owner', 'Owns the project. A manager cannot promote someone to owner or remove an owner.'],
    ['Manager', 'Shares the project, sets review rules and manages files.'],
    ['Editor', 'Edits the team version and merges changes.'],
    ['Reviewer', 'Approves or requests changes, comments and suggests.'],
    ['Contributor', 'Comments, suggests and sends changes for review.'],
    ['Viewer', 'Reads and downloads.']
  ];

  let query = $state('');
  let ready = $state(false);
  let active = $state('');
  const needle = $derived(query.trim().toLowerCase());
  const shown = $derived(
    needle
      ? topics.filter((t) => `${t.title} ${t.keywords}`.toLowerCase().includes(needle))
      : topics
  );

  // Scrollspy: highlight the topic nearest the top of the reading column. Without
  // JavaScript the table of contents is a plain list of anchors.
  onMount(() => {
    ready = true;
    if (!('IntersectionObserver' in window)) return;
    const ids = topics.map((t) => t.id);
    const visible = new Set<string>();
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) visible.add(entry.target.id);
          else visible.delete(entry.target.id);
        }
        const first = ids.find((id) => visible.has(id));
        if (first) active = first;
      },
      { rootMargin: '-96px 0px -60% 0px' }
    );
    for (const id of ids) {
      const el = document.getElementById(id);
      if (el) observer.observe(el);
    }
    const hash = () => {
      const id = decodeURIComponent(location.hash.slice(1));
      if (ids.includes(id)) active = id;
    };
    hash();
    window.addEventListener('hashchange', hash);
    return () => {
      observer.disconnect();
      window.removeEventListener('hashchange', hash);
    };
  });
</script>

<svelte:head>
  <title>{title}</title>
  <meta name="description" content={description} />
  <meta property="og:title" content={title} />
  <meta property="og:description" content={description} />
  <meta name="twitter:title" content={title} />
  <meta name="twitter:description" content={description} />
</svelte:head>

<main id="main" use:scrollable>
  <header class="hero dotgrid dotgrid--fade spotlight" use:spotlight>
    <div class="container hero__grid">
      <div class="hero__copy">
        <Eyebrow label="HELP" />
        <h1 class="h-1">From your first project to <span class="hl">a verified history</span>.</h1>
        <p class="lede">
          How to create a project, bring changes in from an editor, review and merge them, recover
          earlier content and check that the history is intact.
        </p>
        <div class="actions">
          <Btn href="#first-project" arrow>Start with the first project</Btn>
          <Btn href="/support" variant="secondary">Get help</Btn>
        </div>
      </div>
      <div class="hero__art" aria-hidden="true">
        <Dino name="egg" art={Egg} />
      </div>
    </div>
  </header>

  <div class="container help">
    <aside>
      <nav aria-label="On this page">
        <p class="toc-title">On this page</p>
        {#if ready}
          <label class="find">
            <span class="sr-only">Find a topic</span>
            <Search size={15} aria-hidden="true" />
            <input type="search" placeholder="Find a topic" bind:value={query} autocomplete="off" />
          </label>
        {/if}
        <ol>
          {#each shown as topic (topic.id)}
            <li>
              <a
                href={`#${topic.id}`}
                class:active={active === topic.id}
                aria-current={active === topic.id ? 'location' : undefined}>{topic.title}</a
              >
            </li>
          {/each}
        </ol>
        {#if !shown.length}<p class="none small" role="status">No topic matches “{query}”.</p>{/if}
      </nav>
      <div class="related">
        <a href="/add-ins">Editors &amp; add-ins</a>
        <a href="/compatibility">Compatibility</a>
        <a href="/support">Support</a>
      </div>
    </aside>

    <div class="col">
      <article class="prose" use:reveal>
        <section id="first-project">
          <h2>Create your first project</h2>
          <p>
            A project is a private place for related files, the people who work on them and the
            rules for changing them.
          </p>
          <ol>
            <li>
              Open <strong>Projects</strong> and choose <strong>New project</strong>. Give it a
              name. A project is invite-only: nobody sees it until you add them.
            </li>
            <li>
              Choose <strong>Add file</strong>, then <strong>Upload a file</strong>. DOCX, XLSX and
              PPTX are the main formats. While Dynodoc is in testing, an original can be at most 3
              MB (3,000,000 bytes). Dynodoc keeps your original and builds a reviewable copy in
              which every paragraph, row and shape has a stable identity.
            </li>
            <li>
              Open <strong>Settings</strong> to add people and choose a role for each. The roles are described
              below.
            </li>
          </ol>
        </section>

        <section id="capture">
          <h2>Capture a change from an editor</h2>
          <p>
            There are two ways to bring an edit into a project. Either way, nothing reaches the team
            version until someone reviews and merges it.
          </p>
          <h3>With an add-in</h3>
          <p>
            Install the add-in for your editor (see <a href="/add-ins">Editors &amp; add-ins</a>),
            choose <strong>Sign in to Dynodoc</strong>, approve one file in the browser and turn
            capture on. Edit existing text or values, then push the changes for review. Capture
            works only while the pane or sidebar is open, and covers existing paragraph text, cell
            values and shape text.
          </p>
          <h3>With an edited file</h3>
          <p>
            Edit a copy in any editor. Open the file in the project and choose
            <strong>Push an edited file</strong>. Files downloaded from Dynodoc carry hidden markers
            that identify each block exactly; other files are matched by content. The differences
            become a draft that you can submit for review. Use this route for formatting and
            structural changes that the add-ins do not capture.
          </p>
        </section>

        <section id="review">
          <h2>Review a pull request</h2>
          <p>
            A change request appears under <strong>Pull requests</strong>: a named set of changes
            proposed against a specific revision of one file.
          </p>
          <ul>
            <li>
              Open a request to see each changed block before and after, with formatting and
              structural differences.
            </li>
            <li>
              If the team version changed the same content after the request was based, the overlap
              is shown as a conflict to resolve before merging.
            </li>
            <li>
              Comment on a change. Reviewers choose <strong>Approve</strong>, or
              <strong>Request changes</strong> and say what should change.
            </li>
            <li>
              The project’s review rules set how many approvals are needed. The request shows how
              many it has.
            </li>
          </ul>
          <figure data-reveal>
            <ReviewMock />
            <figcaption class="small">Illustration with sample content.</figcaption>
          </figure>
        </section>

        <section id="merge">
          <h2>Merge, or send it back</h2>
          <p>
            Someone with merge permission chooses <strong>Merge</strong>. The merge adds events to
            the file’s history that record who proposed the change, who reviewed it and who merged
            it. Earlier events are not rewritten.
          </p>
          <ul>
            <li>
              You can merge selected changes and leave the rest of the request open for later.
            </li>
            <li>
              Choose <strong>Decline</strong>, with a note, to close a request without applying it.
            </li>
            <li>
              If a request lacks the approvals the rules require, an owner or manager can merge
              without them. The override is recorded in the audit log.
            </li>
            <li>
              In a connected editor, choose <strong>Preview team changes</strong> and then
              <strong>Apply preview</strong> to bring the merged change into your file. If the file changed
              after the preview, nothing is overwritten.
            </li>
          </ul>
        </section>

        <section id="recover">
          <h2>Restore, revert and recover</h2>
          <p>
            History keeps every recorded change, so older content can come back without losing newer
            work. Open a file’s <strong>History</strong> tab and choose
            <strong>Recover an earlier revision</strong>. Pick one of three operations and preview
            it before anything is created.
          </p>
          <dl>
            <dt>Restore earlier content</dt>
            <dd>
              Bring selected blocks back to how they were at that revision. A block that did not
              exist then is proposed for deletion.
            </dd>
            <dt>Revert changes in a range</dt>
            <dd>Undo the net effect of the changes between two revisions.</dd>
            <dt>Cherry-pick changes in a range</dt>
            <dd>
              Take the net effect of the changes between two revisions and apply it on top of the
              current version.
            </dd>
          </dl>
          <p>
            Recovery creates a private draft based on the current team version. Inspect it and
            choose
            <strong>Send for review</strong>; it then follows the same approval and merge rules as
            any other change. Overlaps with newer work are shown as conflicts. You can also download
            the checkpoint as JSON or as a native DOCX, XLSX or PPTX; a native download holds the
            supported content, so keep the original for full fidelity.
          </p>
        </section>

        <section id="versions">
          <h2>Named versions and private drafts</h2>
          <h3>Named versions</h3>
          <p>
            A named version marks a milestone such as “Approved for release”. In a web editor, open
            the version history panel, enter a <strong>Version name</strong> and an optional note,
            and choose <strong>Save named version</strong>. Restoring a named version adds new
            changes; it never removes the work that came after it.
          </p>
          <h3>Private drafts</h3>
          <p>
            A draft is a private branch of one file. Under <strong>Your drafts</strong> you can compare
            it with main, keep editing and open a pull request when it is ready. A draft is visible only
            to you until you submit it. Project-wide branches published from the command line are listed
            read-only under Branches; creating and merging them in the browser is not available yet.
          </p>
        </section>

        <section id="roles">
          <h2>Roles and review rules</h2>
          <div class="table-wrap" data-reveal>
            <table>
              <caption class="sr-only">Project roles</caption>
              <thead><tr><th scope="col">Role</th><th scope="col">What it can do</th></tr></thead>
              <tbody>
                {#each roles as [role, text] (role)}
                  <tr><th scope="row">{role}</th><td>{text}</td></tr>
                {/each}
              </tbody>
            </table>
          </div>
          <p>
            Under <strong>Settings</strong>, review rules decide whether main is protected and how
            many approvals a change needs, from none to five. When main is protected, changes go
            through pull requests. Approvals can become stale if the author edits the request
            afterwards.
          </p>
          <p>
            A project’s rules are the minimum for every file in it. A file can add stricter rules,
            but a rule that would weaken the project’s is rejected. Owning a file does not give
            review authority over the project.
          </p>
        </section>

        <section id="verify">
          <h2>Verify the history</h2>
          <p>
            Every recorded change goes into an append-only history in which each entry carries a
            hash that depends on the entry before it. Open a file’s <strong>History</strong> tab and
            choose
            <strong>Verify hash chain</strong> to recompute it. The result says that the chain is intact
            through a revision, or names the first revision where it does not verify.
          </p>
          <p>
            This shows that recorded events were not altered or removed after they were written. It
            does not show who typed a change, and it is not an independent timestamp. Add-ins record
            what they observe at snapshot times, not every keystroke.
          </p>
          <figure data-reveal>
            <TimelineMock />
            <figcaption class="small">Illustration with sample content.</figcaption>
          </figure>
        </section>

        <section id="web-editors">
          <h2>The web editors</h2>
          <p>
            The web editors stay available for quick edits and for documents that began there. This
            is the same short tour the <strong>Help &amp; tutorials</strong> button replays in the
            workspace. Formats and limits are on the
            <a href="/compatibility">compatibility page</a>.
          </p>
          <ol class="tour">
            {#each guide as topic (topic.title)}
              <li>
                <h3>{topic.title}</h3>
                <p><strong>{topic.action}.</strong> {topic.body}</p>
              </li>
            {/each}
          </ol>
        </section>

        <section id="words">
          <h2>Version-control words, in plain language</h2>
          <p>
            You do not need to know Git. This table maps familiar version-control words to what
            Dynodoc calls them. Each file has one central history; Dynodoc is not a Git repository.
          </p>
          <div class="table-wrap" data-reveal>
            <table>
              <caption class="sr-only">Version-control words in Dynodoc</caption>
              <thead>
                <tr><th scope="col">Version-control idea</th><th scope="col">In Dynodoc</th></tr>
              </thead>
              <tbody>
                {#each versionVocabulary as [term, label] (term)}
                  <tr><th scope="row">{term}</th><td>{label}</td></tr>
                {/each}
              </tbody>
            </table>
          </div>
        </section>

        <section id="limits">
          <h2>Limits and when something goes wrong</h2>
          <ul>
            <li>
              Originals are limited to 3 MB (3,000,000 bytes) while Dynodoc is in testing. Workbook
              imports are limited to 3,000 rows and 100 columns.
            </li>
            <li>
              Add-ins capture existing text and values only, up to 2,000 paragraphs, cells or shapes
              per file. Use <strong>Push an edited file</strong> for anything else.
            </li>
            <li>
              Always read the conversion notes after an import and keep your original. Complex
              layouts and unsupported features can differ from the source.
            </li>
            <li>
              If a save is interrupted in a web editor, return to the document and choose
              <strong>Review recovered work</strong>. To keep recovery copies, choose
              <strong>File</strong>, then <strong>Keep recovery copies on this browser</strong>, on
              a trusted device. Recovery is local to that browser and cleared when you sign out.
            </li>
          </ul>
          <p>
            More detail is on the <a href="/compatibility">compatibility page</a>. If something is
            not covered, see <a href="/support">Support</a>.
          </p>
        </section>
      </article>
      <div class="cta"><Btn href={workspace} arrow>Open your projects</Btn></div>
    </div>
  </div>
</main>

<style>
  .hero {
    border-bottom: 1px solid var(--color-border);
  }
  .hero__grid {
    display: grid;
    gap: clamp(28px, 5vw, 64px);
    align-items: center;
    padding-block: clamp(48px, 8vw, 96px) clamp(36px, 5vw, 64px);
  }
  .hero__copy {
    display: grid;
    gap: 22px;
    justify-items: start;
  }
  .hero__copy h1 {
    max-width: 18ch;
  }
  .hero__art {
    max-width: 260px;
    color: var(--color-ink-soft);
    --art-accent: var(--color-accent);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 12px;
  }

  .help {
    display: grid;
    grid-template-columns: 1fr;
    gap: 32px;
    align-items: start;
    padding-block: clamp(28px, 4vw, 56px) var(--site-section-y, 96px);
  }
  aside {
    min-width: 0;
  }
  .toc-title {
    margin: 0 0 12px;
    color: var(--color-muted);
    font-family: var(--site-font-mono);
    font-size: 0.75rem;
    font-weight: 500;
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }
  .find {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 14px;
    padding: 0 10px;
    border: 1px solid var(--color-border-strong);
    border-radius: var(--site-radius-sm);
    background: var(--color-surface-raised);
    color: var(--color-muted);
  }
  .find:focus-within {
    outline: 2px solid var(--color-focus);
    outline-offset: 2px;
  }
  .find input {
    flex: 1;
    min-width: 0;
    height: 40px;
    border: 0;
    background: transparent;
    color: var(--color-ink);
    font: inherit;
    font-size: 0.9375rem;
  }
  .find input:focus {
    outline: none;
  }
  .find input::placeholder {
    color: var(--color-muted);
  }
  nav ol {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
    counter-reset: toc;
  }
  nav ol li {
    counter-increment: toc;
  }
  nav ol a {
    display: flex;
    gap: 10px;
    padding: 4px 10px 4px 12px;
    border-left: 1px solid var(--color-border);
    color: var(--color-ink-soft);
    font-size: 0.875rem;
    line-height: 1.45;
    text-decoration: none;
    transition:
      color 0.15s,
      border-color 0.15s;
  }
  nav ol a::before {
    content: counter(toc, decimal-leading-zero);
    content: counter(toc, decimal-leading-zero) / '';
    flex: none;
    color: var(--color-muted);
    font-family: var(--site-font-mono);
    font-size: 0.75rem;
    line-height: 1.9;
  }
  nav ol a:hover {
    color: var(--color-ink-strong);
    border-left-color: var(--color-ink-soft);
  }
  nav ol a.active {
    padding-left: 11px;
    border-left: 2px solid var(--color-accent);
    color: var(--color-ink-strong);
    font-weight: 540;
  }
  .none {
    margin: 10px 0 0;
  }
  .related {
    display: grid;
    gap: 6px;
    margin-top: 24px;
    padding-top: 20px;
    border-top: 1px solid var(--color-border);
  }
  .related a {
    display: inline-flex;
    align-items: center;
    min-height: 28px;
    color: var(--color-accent-strong);
    font-size: 0.875rem;
    text-underline-offset: 3px;
  }

  .col {
    min-width: 0;
    max-width: 68ch;
  }
  .cta {
    margin-top: 40px;
  }
  article {
    min-width: 0;
    counter-reset: topic;
  }
  article section {
    counter-increment: topic;
    margin-bottom: clamp(32px, 4vw, 52px);
    padding-bottom: clamp(32px, 4vw, 52px);
    border-bottom: 1px solid var(--color-border);
    scroll-margin-top: calc(var(--site-header-h, 64px) + 20px);
  }
  article section:last-child {
    margin-bottom: 0;
    border-bottom: 0;
  }
  article section > h2:first-child {
    margin-top: 0;
  }
  article section > h2:first-child::before {
    content: counter(topic, decimal-leading-zero);
    content: counter(topic, decimal-leading-zero) / '';
    display: block;
    margin-bottom: 8px;
    color: var(--color-muted);
    font-family: var(--site-font-mono);
    font-size: 0.75rem;
    font-weight: 500;
    letter-spacing: 0.08em;
  }
  article dl {
    display: grid;
    gap: 0;
    margin: 0 0 1.1em;
    border-top: 1px solid var(--color-border);
  }
  article dt {
    padding-top: 12px;
  }
  article dd {
    margin: 0;
    padding-bottom: 12px;
    border-bottom: 1px solid var(--color-border);
  }
  figure {
    margin: 1.6em 0 0;
  }
  figcaption {
    margin-top: 10px;
  }
  .tour {
    display: grid;
    gap: 14px;
    padding-left: 0;
    list-style: none;
    counter-reset: tour;
  }
  .tour li {
    counter-increment: tour;
    margin: 0;
    padding: 16px 18px;
    border: 1px solid var(--color-border);
    border-radius: var(--site-radius);
    background: var(--color-surface);
  }
  .tour h3 {
    margin: 0 0 6px;
    font-size: 1.0625rem;
  }
  .tour h3::before {
    content: counter(tour, decimal-leading-zero) ' ';
    content: counter(tour, decimal-leading-zero) ' ' / '';
    color: var(--color-muted);
    font-family: var(--site-font-mono);
    font-size: 0.75rem;
    letter-spacing: 0.06em;
  }
  .tour p {
    margin: 0;
  }

  @media (min-width: 900px) {
    .hero__grid {
      grid-template-columns: minmax(0, 8fr) minmax(0, 3fr);
    }
    .help {
      grid-template-columns: 240px minmax(0, 68ch);
      gap: clamp(40px, 6vw, 88px);
    }
  }
  @media (min-width: 900px) and (min-height: 640px) {
    aside {
      position: sticky;
      top: calc(var(--site-header-h, 64px) + 24px);
    }
  }
  @media (max-width: 899px) {
    aside {
      padding-bottom: 24px;
      border-bottom: 1px solid var(--color-border);
    }
    nav ol {
      grid-template-columns: repeat(2, minmax(0, 1fr));
      gap: 2px 16px;
    }
  }
  @media (max-width: 480px) {
    nav ol {
      grid-template-columns: 1fr;
    }
  }
</style>
