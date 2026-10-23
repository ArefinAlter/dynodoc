<script lang="ts">
  import Dino from '$lib/art/Dino.svelte';
  import Trike from '$lib/art/Trike.svelte';
  import InfoHero from '$lib/components/marketing/InfoHero.svelte';
  import { Btn, CopyCode, Section, scrollable } from '$lib/components/site';
  import { GITHUB_URL } from '$lib/components/site/nav';
  import { pageMeta } from '$lib/seo';

  const meta = pageMeta('/open-source');
  const title = meta?.title ?? 'Open source · Dynodoc';
  const description =
    meta?.description ??
    'The Dynodoc engine, API and command-line tools are MIT licensed. How to build them from source, the platforms they are tested on and what is not published yet.';

  // Facts: docs/PLATFORM-SUPPORT.md and the engine README. The run is the engine repository's
  // "engine-platforms" workflow; its jobs ran on 20 October 2026 (UTC).
  const REPO = GITHUB_URL;
  const RUN = `${REPO}/actions/runs/39493540615`;
  const SUPPORT_DOC = `${REPO}/blob/main/docs/PLATFORM-SUPPORT.md`;

  const open = [
    {
      title: 'Open: engine, API and command-line tools',
      text: 'The event-log, versioning and merge engine, the HTTP API and the command-line tools are published under the MIT licence in the public repository. It is early software, version 0.1, and no package has been published to crates.io.'
    },
    {
      title: 'Not open: the workspace and this website',
      text: 'The web workspace, the editor add-ins and this website are proprietary and live in a private repository. The public repository has no browser editors, no production configuration, no credentials and no user documents.'
    },
    {
      title: 'Your documents stay yours',
      text: 'Private document content you put into the hosted app does not become open source. Open source here describes the code, not the content.'
    }
  ];

  type Tier = 1 | 2;
  const platforms: { name: string; tier: Tier; tests: string; store: string }[] = [
    {
      name: 'Linux x86-64',
      tier: 1,
      tests: 'Main CI workflow',
      store: 'Secret Service (GNOME Keyring)'
    },
    {
      name: 'Linux Arm (aarch64)',
      tier: 1,
      tests: '1,285',
      store: 'Secret Service (GNOME Keyring)'
    },
    {
      name: 'macOS, Apple silicon',
      tier: 1,
      tests: '1,285',
      store: 'Keychain (a temporary keychain)'
    },
    {
      name: 'macOS, Intel',
      tier: 1,
      tests: '1,284',
      store: 'Keychain (a temporary keychain)'
    },
    { name: 'Windows x86-64', tier: 1, tests: '1,285', store: 'DPAPI' },
    { name: 'Windows on Arm (aarch64)', tier: 2, tests: '1,285', store: 'DPAPI' }
  ];

  const needs: { target: string; need: string }[] = [
    {
      target: 'Linux x86-64 and Arm',
      need: 'glibc 2.34 or newer, which covers Ubuntu 22.04, Debian 12 and RHEL 9.'
    },
    { target: 'macOS, Apple silicon and Intel', need: 'macOS 11 or newer. System libraries only.' },
    {
      target: 'Windows x86-64 and Arm',
      need: 'No Visual C++ redistributable. Windows 10 or newer is Rust’s own minimum for the target; it was not tested on a Windows 10 computer.'
    }
  ];

  const limits = [
    'Windows on Arm is tier 2. It has no native PostgreSQL, so its job runs an x64 build of PostgreSQL 16 under emulation while the engine under test is native Arm. That job takes about 40 minutes.',
    'Windows x86-64 uses the PostgreSQL 17 that the runner image ships; every other platform uses PostgreSQL 16. The tests pass on 17.',
    'One latency test, which asserts that a server-sent event arrives within 100 milliseconds, runs in its own non-blocking step on Windows x86-64, Windows on Arm and Intel Macs. It failed on both Windows platforms every time it was recorded. It is not weakened, and it still blocks on the other platforms.',
    'Four tests are not run on any platform in the main run: a compression measurement, planning a hundred thousand commits, a million-leaf property test, and the real key store test, which has its own step.',
    'Tests run on the debug profile. The release binaries are only smoke tested: the version, the help text and the refusal to start without configuration.',
    'Not covered anywhere: real hardware and devices, 32-bit systems, big-endian systems, RISC-V, musl, Linux distributions other than Ubuntu, macOS older than 15 and Windows 10. The macOS 11 and glibc 2.34 floors are build settings, not test results.',
    'On Windows, the files that hold secrets for the API are not checked for permissions, and the hidden passphrase prompt is not tested in CI.',
    'On Linux the operating system key store needs a running Secret Service, which headless servers usually lack. A passphrase file is the tested alternative.'
  ];
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
  <InfoHero
    eyebrow="OPEN SOURCE · 23 OCTOBER 2026"
    lede="The engine, the HTTP API and the command-line tools are open source under the MIT licence. This page says how to build them from source, which platforms they are tested on, what that testing covers and what is not published yet."
  >
    {#snippet heading()}Open source, and <span class="hl">what that covers</span>.{/snippet}
    {#snippet actions()}
      <Btn href={REPO} external arrow>Open the repository</Btn>
      <Btn href="/licenses" variant="secondary">Read the licences</Btn>
    {/snippet}
    {#snippet art()}
      <Dino name="trike" art={Trike} />
    {/snippet}
  </InfoHero>

  <Section
    id="covers"
    eyebrow="WHAT IS OPEN"
    title="Which parts you can read and run."
    lede="The public repository is the core that records and verifies history. It is not the hosted application."
    layout="split"
  >
    <ul class="cards" data-reveal>
      {#each open as item (item.title)}
        <li class="card card--raised">
          <h3 class="h-3">{item.title}</h3>
          <p>{item.text}</p>
        </li>
      {/each}
    </ul>
  </Section>

  <Section
    id="build"
    eyebrow="BUILD FROM SOURCE"
    title="Build and test it yourself."
    lede="You need Rust at the version the repository pins. To run the tests you also need PostgreSQL 16, and the repository uses Docker and Task to start one."
    layout="split"
    tone="muted"
  >
    <ol class="steps">
      <li data-reveal>
        <span class="num">STEP 01 · get the source</span>
        <CopyCode
          code="git clone https://github.com/ArefinAlter/dynodoc-engine.git"
          label="clone command"
        />
      </li>
      <li data-reveal>
        <span class="num">STEP 02 · build everything</span>
        <CopyCode code="cargo build --workspace --locked" label="build command" />
      </li>
      <li data-reveal>
        <span class="num">STEP 03 · check the command-line tool</span>
        <CopyCode code="cargo run --locked -p engine-cli -- --version" label="version command" />
      </li>
      <li data-reveal>
        <span class="num">STEP 04 · run the tests (needs PostgreSQL 16)</span>
        <CopyCode code="cargo test --workspace --locked" label="test command" />
      </li>
    </ol>
    <p class="after">
      The tests create databases and roles, so point them at a throwaway PostgreSQL, never at one
      that holds data you keep. Running the API itself needs a few settings and a restricted
      database login. The repository README gives those steps, from <code>task up</code> to
      <code>task run</code>.
      <a class="link-sweep" href="{REPO}#local-development">Read the README</a>
    </p>
  </Section>

  <Section
    id="platforms"
    eyebrow="PLATFORMS"
    title="Where it is built and tested."
    lede="Linux, macOS and Windows, on Intel or AMD processors and on Arm. Every job builds the whole workspace and runs the whole test suite against PostgreSQL."
  >
    <div class="table-wrap" data-reveal>
      <table>
        <caption class="sr-only">Platforms, support tier and test results</caption>
        <thead>
          <tr>
            <th scope="col">Platform</th>
            <th scope="col">Tier</th>
            <th scope="col">Tests passed</th>
            <th scope="col">Real key store tested</th>
          </tr>
        </thead>
        <tbody>
          {#each platforms as row (row.name)}
            <tr>
              <th scope="row">{row.name}</th>
              <td
                ><span class="chip chip--tier{row.tier}"
                  >Tier {row.tier}{row.tier === 2 ? ', not blocking' : ''}</span
                ></td
              >
              <td>{row.tests}</td>
              <td>{row.store}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
    <p class="after small">
      Tier 1: a failing build or test on that platform fails the workflow. Tier 2: built and tested,
      but a failure is reported without blocking, and nothing stronger is promised. Not supported:
      32-bit systems, RISC-V, big-endian systems, musl, and Linux distributions older than the glibc
      floor below.
    </p>
  </Section>

  <Section
    id="checks"
    eyebrow="WHAT THE CHECKS PROVE"
    title="One run, with its date."
    lede="A row above is true for the commit its run names. The figures here come from one run of the engine repository’s platform workflow."
    layout="split"
    tone="muted"
  >
    <div class="facts" data-reveal>
      <p class="fact">
        <span class="num num--lg">1,285</span>
        <span
          >Rust tests passed on Linux Arm, Apple-silicon Macs, Windows and Windows on Arm. Intel
          Macs passed 1,284. The run started on 20 October 2026 (UTC) and all six jobs succeeded.</span
        >
      </p>
      <p>
        Each job built the workspace with a locked dependency list, ran the whole test suite against
        PostgreSQL, regenerated the OpenAPI file and compared it with the checked-in copy, and used
        the operating system’s real key store to protect a signing key, open it again and remove the
        stored secret. On Linux the key store job ran on both processors. On macOS it used a
        temporary keychain, and the login keychain was checked to stay clean.
      </p>
      <p class="links">
        <Btn href={RUN} variant="link" external arrow>The run, job by job</Btn>
        <Btn href={SUPPORT_DOC} variant="link" external arrow>The support record</Btn>
      </p>
    </div>
  </Section>

  <Section
    id="prebuilt"
    eyebrow="PREBUILT TOOLS"
    title="No release is published yet."
    lede="Prebuilt command-line tools will be published as GitHub releases; none is published yet. Until one is, build from source."
    layout="split"
  >
    <div class="stack" data-reveal>
      <p>
        The project’s release workflow has built <code>engine-cli</code> and <code>engine-api</code>
        natively for all six platforms, run each binary it built, unpacked each archive into a clean folder
        and run it again from there. Three of the archives were also downloaded and run outside CI, one
        on a Windows PC and two in Linux containers. The step that publishes a release has never run,
        because running it would publish a release in the public repository. This section describes how
        a release will be checked, and it is not a download page.
      </p>
      <h3 class="h-3">How a release will be verified</h3>
      <p>
        A release will have one archive for each of the six targets, holding
        <code>engine-cli</code>, <code>engine-api</code>, the licence and a short README, and one
        <code>SHA256SUMS</code> file. Compare the checksum before you run anything.
      </p>
      <div class="commands">
        <p class="num">Linux</p>
        <CopyCode
          code="sha256sum --check --ignore-missing SHA256SUMS"
          label="Linux checksum command"
        />
        <p class="num">macOS</p>
        <CopyCode
          code="shasum -a 256 -c SHA256SUMS --ignore-missing"
          label="macOS checksum command"
        />
        <p class="num">Windows (PowerShell)</p>
        <CopyCode
          code="Get-FileHash .\archive.zip -Algorithm SHA256"
          prompt="PS&gt;"
          label="Windows checksum command"
        />
      </div>
      <p>
        On Windows, replace <code>archive.zip</code> with the name of the downloaded archive and
        compare the result with the line for it in <code>SHA256SUMS</code>. A checksum shows that a
        file is the one the release workflow built. It does not show who wrote the source.
      </p>
      <div class="callout" role="note">
        <p>
          <strong>The binaries will not be signed or notarised.</strong> The project has no
          code-signing certificate. macOS Gatekeeper blocks a downloaded unsigned binary: once the
          checksum matches, run <code>xattr -d com.apple.quarantine engine-cli engine-api</code>, or
          open it once with a right click and Open. Windows SmartScreen shows an unknown-publisher
          warning: once the checksum matches, choose More info, then Run anyway. Linux shows no
          warning.
        </p>
      </div>
      <h3 class="h-3">What they need from the system</h3>
      <div class="table-wrap">
        <table>
          <caption class="sr-only">System requirements of the prebuilt tools</caption>
          <thead>
            <tr><th scope="col">Target</th><th scope="col">Needs</th></tr>
          </thead>
          <tbody>
            {#each needs as row (row.target)}
              <tr><th scope="row">{row.target}</th><td>{row.need}</td></tr>
            {/each}
          </tbody>
        </table>
      </div>
    </div>
  </Section>

  <Section
    id="container"
    eyebrow="CONTAINER IMAGE"
    title="Built for amd64 and arm64."
    lede="A container image of the API and the command-line tool is built and started in CI. It is not published yet."
    layout="split"
    tone="muted"
  >
    <div class="stack" data-reveal>
      <p>
        The repository’s <code>Dockerfile</code> puts <code>engine-api</code> and
        <code>engine-cli</code> on Debian 12 (<code>bookworm-slim</code>) and runs them as a
        non-root user. CI builds it natively for <code>linux/amd64</code> and
        <code>linux/arm64</code>, on a runner of each processor. Each job checks the image’s
        architecture, runs <code>engine-cli --version</code>, checks that <code>engine-api</code>
        refuses to start without configuration, and checks the user id. Both passed.
      </p>
      <p>
        The images are not signed and carry no software bill of materials. The step that pushes them
        to a registry has not run, so there is no image to pull.
      </p>
    </div>
  </Section>

  <Section
    id="limits"
    eyebrow="KNOWN LIMITS"
    title="What these checks do not cover."
    lede="The support record lists these too. They are repeated here so nobody has to find them."
  >
    <ul class="limits" data-reveal>
      {#each limits as limit (limit)}
        <li>{limit}</li>
      {/each}
    </ul>
  </Section>

  <Section
    tone="ink"
    align="center"
    eyebrow="READ IT, RUN IT, TELL US"
    title="The code is public."
    lede="Found a problem? A security issue goes through private reporting. Anything else goes to the issue tracker."
  >
    <div class="closing" data-reveal>
      <Btn href={REPO} size="lg" external arrow>Open the repository</Btn>
      <Btn href="/security#report" size="lg" variant="secondary">Report a vulnerability</Btn>
    </div>
  </Section>
</main>

<style>
  .cards {
    display: grid;
    gap: 16px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .cards p {
    margin: 10px 0 0;
    color: var(--color-ink-soft);
  }

  .steps {
    display: grid;
    gap: 22px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .steps li {
    display: grid;
    gap: 8px;
    min-width: 0;
  }
  .steps .num {
    color: var(--color-muted);
    font-size: 0.75rem;
  }
  .after {
    max-width: 68ch;
    margin: 24px 0 0;
    color: var(--color-ink-soft);
  }
  .link-sweep {
    display: inline-block;
    margin-left: 6px;
    color: var(--color-accent-strong);
  }

  .stack {
    display: grid;
    gap: 18px;
    min-width: 0;
  }
  .stack p {
    margin: 0;
    max-width: 68ch;
    color: var(--color-ink-soft);
  }
  .stack h3 {
    margin: 10px 0 0;
  }
  .commands {
    display: grid;
    gap: 8px;
    min-width: 0;
  }
  .commands .num {
    margin: 8px 0 0;
    color: var(--color-muted);
    font-size: 0.75rem;
    text-transform: uppercase;
  }
  .facts {
    display: grid;
    gap: 18px;
    min-width: 0;
  }
  .facts p {
    margin: 0;
    max-width: 68ch;
    color: var(--color-ink-soft);
  }
  .fact {
    display: grid;
    gap: 6px;
    color: var(--color-ink);
  }
  .links {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 28px;
  }

  .table-wrap {
    overflow: auto;
    border: 1px solid var(--color-border);
    border-radius: var(--site-radius);
    background: var(--color-surface-raised);
  }
  table {
    width: 100%;
    min-width: 560px;
    border-collapse: collapse;
    font-size: 0.9375rem;
    line-height: 1.55;
  }
  th,
  td {
    padding: 14px 16px;
    border-bottom: 1px solid var(--color-border);
    text-align: left;
    vertical-align: top;
  }
  thead th {
    background: var(--color-surface-muted);
    color: var(--color-ink);
    font-family: var(--site-font-mono);
    font-size: 0.75rem;
    font-weight: 500;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  tbody th {
    color: var(--color-ink-strong);
    font-weight: 560;
  }
  td {
    color: var(--color-ink-soft);
  }
  tbody tr:last-child > * {
    border-bottom: 0;
  }
  .chip {
    display: inline-block;
    padding: 1px 9px;
    border: 1px solid var(--color-border-strong);
    border-radius: 999px;
    color: var(--color-ink-soft);
    font-family: var(--site-font-mono);
    font-size: 0.6875rem;
    font-weight: 500;
    letter-spacing: 0.06em;
    line-height: 1.7;
    text-transform: uppercase;
    white-space: nowrap;
  }
  .chip--tier1 {
    border-color: var(--color-accent-border);
    background: var(--color-accent-bg);
    color: var(--color-accent-strong);
  }
  .chip--tier2 {
    border-color: var(--color-warn-border);
    background: var(--color-warn-bg);
    color: var(--color-warn-ink);
  }

  .limits {
    display: grid;
    gap: 12px;
    margin: 0;
    padding: 0 0 0 1.1em;
    max-width: 78ch;
    color: var(--color-ink-soft);
    font-size: 0.9375rem;
    line-height: 1.55;
  }
  .limits li::marker {
    color: var(--color-muted);
  }
  .closing {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: 12px;
  }

  @media (min-width: 720px) {
    .cards {
      grid-template-columns: repeat(3, minmax(0, 1fr));
    }
  }
</style>
