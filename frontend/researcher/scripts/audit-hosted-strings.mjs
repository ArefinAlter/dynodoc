#!/usr/bin/env node
// Hosted-string audit of a built web server (acceptance criterion 6 of Phase 0 in
// docs/SELF-HOSTED-EDITION.md: "no file in the shipped images contains dynodoc.online outside
// of third-party notices"). It scans every file under the build directory for the hosted
// service's hostnames, the operator's facts, analytics and third-party hosts, and says for
// each hit whether a self-hosted installation can reach that file:
//
//   shared-client / shared-server   loaded by pages or code that run in every mode (a leak)
//   marketing-client / -server      loaded only by the (site) routes and the search-engine
//                                   routes, which answer 404 in self-hosted mode
//   static-asset                    a file served as it is (add-in packages, security.txt, ...)
//   sourcemap                       a source map, which repeats the source of its file
//   notice                          a third-party notice (font licences)
//
// Reachability comes from the build's own route table (server/manifest.js) and the import
// graph of its JavaScript. A file that is shipped but unreachable is still a hit: the
// criterion is about the image, and only a build that leaves the marketing routes out can
// make it pass. `--check` compares the result with scripts/hosted-strings-known.json and
// fails on anything not listed there, so the list can only shrink.
//
// usage: node scripts/audit-hosted-strings.mjs [--build build] [--json out.json] [--check] [--strict]
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { dirname, join, posix, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { unzipSync, strFromU8 } from 'fflate';

/** What to look for. `exception` marks a hit that is documented elsewhere and allowed. */
export const PATTERNS = [
  { id: 'hosted-domain', re: /dynodoc\.online/gi },
  {
    id: 'operator',
    re: /Verne\b|vernesystems|Dhaka|Bangladesh|\+880[ -]?1870|8\/A, Sher-e-Bangla/g
  },
  {
    id: 'mail-service',
    re: /api\.resend\.com|resend\.com/gi,
    exception: 'Optional mail transport: the server contacts it only when a key is configured.'
  },
  {
    id: 'analytics',
    re: /google-analytics|googletagmanager|plausible\.io|posthog|mixpanel|segment\.(?:io|com)|sentry\.io|vercel-insights|_vercel\/insights|vercel\.live|cloudflareinsights|hotjar|clarity\.ms/gi
  },
  {
    id: 'google',
    re: /accounts\.google\.com|[a-z]+\.googleapis\.com|gstatic\.com/gi,
    exception: 'Optional Google sign-in and Forms import: contacted only when configured.'
  },
  { id: 'cdn', re: /cdn\.jsdelivr|unpkg\.com|cdnjs\.|fonts\.googleapis/gi },
  {
    id: 'office-cdn',
    re: /appsforoffice\.microsoft\.com|onecdn\.static\.microsoft|ajax\.aspnetcdn\.com/gi,
    exception: 'The Office.js CDN is a known exception (docs/SELF-HOSTED-EDITION.md section 7.3).'
  },
  {
    id: 'repository',
    re: /github\.com\/ArefinAlter/g,
    exception: "A link to the public engine repository; following it is the reader's choice."
  }
];

/** Hosts that appear in code but are namespaces or documentation, not something contacted. */
const QUIET_HOSTS =
  /^(www\.w3\.org|w3\.org|schemas\.(?:openxmlformats|microsoft)\.org|purl\.org|svelte\.dev|kit\.svelte\.dev|developer\.mozilla\.org|reactjs\.org|github\.com\/sveltejs|ns\.adobe\.com|xmlns\.com|www\.ecma-international\.org|tc39\.es|example\.(?:com|org|test)|localhost|127\.0\.0\.1|json-schema\.org|www\.iana\.org|datatracker\.ietf\.org|html\.spec\.whatwg\.org)$/;

const TEXT = /\.(?:js|mjs|css|html|txt|xml|json|svg|md|csv|webmanifest|map|rels)$/i;
const ARCHIVE = /\.(?:zip|xlsx|docx|pptx)$/i;
const COMPRESSED_COPY = /\.(?:br|gz)$/i;
const NOTICE = /(?:^|\/)fonts\/(?:[^/]*OFL[^/]*\.txt|README\.md)$/i;

function* walk(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) yield* walk(path);
    else yield path;
  }
}

/** Relative imports of a JavaScript file, resolved to paths inside the build. */
export function importsOf(file, source) {
  const found = new Set();
  const re = /(?:\bfrom|\bimport)\s*\(?\s*["'](\.{1,2}\/[^"']+?\.js)["']/g;
  for (const match of source.matchAll(re))
    found.add(posix.normalize(posix.join(posix.dirname(file), match[1])));
  return found;
}

/** Everything reachable from `roots`, never stepping onto a path in `blocked`. */
export function closure(roots, graph, blocked = new Set()) {
  const seen = new Set();
  const queue = [...roots].filter((path) => !blocked.has(path));
  while (queue.length) {
    const path = queue.pop();
    if (seen.has(path)) continue;
    seen.add(path);
    for (const next of graph.get(path) ?? [])
      if (!blocked.has(next) && !seen.has(next)) queue.push(next);
  }
  return seen;
}

/**
 * Which files serve marketing and which serve every mode, from the build's route table.
 * The table's functions are memoised wrappers, so the files they load are read from the text
 * of server/manifest.js: nodes in order, and each route's endpoint. Returns null when the
 * build has no route table (then every JavaScript file counts as shared).
 */
async function routeFiles(buildDir) {
  const manifestPath = join(buildDir, 'server', 'manifest.js');
  if (!existsSync(manifestPath)) return null;
  const text = readFileSync(manifestPath, 'utf8');
  const { manifest } = await import(pathToFileURL(resolve(manifestPath)).href);
  const table = manifest._;
  const serverFile = (path) => posix.normalize(posix.join('server', path));
  const nodesText = /\bnodes:\s*\[([\s\S]*?)\],\s*remotes\b/.exec(text)?.[1] ?? '';
  const nodeFiles = [...nodesText.matchAll(/import\(\s*["']([^"']+)["']\s*\)/g)].map((m) =>
    serverFile(m[1])
  );
  const endpoints = new Map(
    [
      ...text.matchAll(
        /\bid:\s*"([^"]+)"[\s\S]*?\bendpoint:\s*(?:null|__memo\(\(\)\s*=>\s*import\(\s*["']([^"']+)["']\s*\)\))/g
      )
    ].map((m) => [m[1], m[2] ? serverFile(m[2]) : null])
  );
  const marketingRoute = (id) =>
    id.startsWith('/(site)') ||
    ['/sitemap.xml', '/llms.txt', '/llms-full.txt', '/source'].includes(id);
  const marketingNodes = new Set();
  const sharedNodes = new Set();
  const marketingFiles = new Set();
  const sharedFiles = new Set();
  for (const route of table.routes) {
    const marketing = marketingRoute(route.id);
    const page = route.page;
    const indexes = page ? [page.leaf, ...page.layouts, ...page.errors] : [];
    for (const index of indexes)
      if (typeof index === 'number') (marketing ? marketingNodes : sharedNodes).add(index);
    const endpoint = endpoints.get(route.id);
    if (endpoint) (marketing ? marketingFiles : sharedFiles).add(endpoint);
  }
  // A node that serves both kinds of route (the root layout) is shared.
  for (const index of sharedNodes) marketingNodes.delete(index);
  return {
    clientNodes: { marketing: [...marketingNodes], shared: [...sharedNodes] },
    serverNodes: {
      marketing: [...marketingNodes].map((i) => nodeFiles[i]).filter(Boolean),
      shared: [...sharedNodes].map((i) => nodeFiles[i]).filter(Boolean)
    },
    marketingFiles,
    sharedFiles
  };
}

/** Scan a build. Returns { files, hits, urls }. */
export async function scanBuild(buildDir) {
  const root = resolve(buildDir);
  const rel = (path) => relative(root, path).split(sep).join('/');
  const files = [];
  const graph = new Map();
  for (const path of walk(root)) {
    const name = rel(path);
    if (COMPRESSED_COPY.test(name)) continue;
    files.push({ path: name, size: statSync(path).size });
    if (name.endsWith('.js')) graph.set(name, importsOf(name, readFileSync(path, 'utf8')));
  }

  // Reachability of the JavaScript.
  const routes = await routeFiles(root);
  const reach = { shared: new Set(graph.keys()), marketing: new Set() };
  if (routes) {
    const clientNode = (index) =>
      files.find((f) =>
        new RegExp(`^client/_app/immutable/nodes/${index}\\.[^/]+\\.js$`).test(f.path)
      )?.path;
    const marketingClient = routes.clientNodes.marketing.map(clientNode).filter(Boolean);
    const marketingNodeFiles = new Set([
      ...marketingClient,
      ...routes.serverNodes.marketing,
      ...routes.marketingFiles
    ]);
    const entries = files
      .map((f) => f.path)
      .filter(
        (p) =>
          /^client\/_app\/immutable\/entry\/(?:start|app)\.[^/]+\.js$/.test(p) ||
          p === 'server/index.js' ||
          p === 'server/manifest.js' ||
          p === 'index.js' ||
          p === 'handler.js'
      );
    const sharedRoots = [
      ...entries,
      ...routes.clientNodes.shared.map(clientNode).filter(Boolean),
      ...routes.serverNodes.shared,
      ...routes.sharedFiles
    ];
    // The entry points name every node (to load it on demand); only the route table says
    // which nodes serve marketing, so those edges are not followed from the entry points.
    reach.shared = closure(sharedRoots, graph, marketingNodeFiles);
    reach.marketing = new Set(
      [...closure(marketingNodeFiles, graph)].filter((p) => !reach.shared.has(p))
    );
  }

  const classOf = (name) => {
    if (NOTICE.test(name)) return 'notice';
    if (name.endsWith('.map')) return 'sourcemap';
    const inClient = name.startsWith('client/_app/immutable/');
    const inServer = name.startsWith('server/');
    if (!name.endsWith('.js') || !(inClient || inServer)) {
      if (name.startsWith('client/') && !inClient) return 'static-asset';
      return inClient ? 'client-resource' : inServer ? 'server-resource' : 'runtime';
    }
    const side = inClient ? 'client' : 'server';
    if (reach.shared.has(name)) return `shared-${side}`;
    if (reach.marketing.has(name)) return `marketing-${side}`;
    return `unreachable-${side}`;
  };

  const hits = [];
  const urls = new Map();
  const scan = (name, text, container) => {
    const found = {};
    for (const { id, re } of PATTERNS) {
      const matches = [...text.matchAll(re)];
      if (matches.length) found[id] = matches.length;
    }
    for (const match of text.matchAll(/https?:\/\/([a-z0-9.-]+\.[a-z]{2,})(?::\d+)?/gi)) {
      const host = match[1].toLowerCase();
      if (QUIET_HOSTS.test(host)) continue;
      const entry = urls.get(host) ?? { count: 0, files: new Set() };
      entry.count++;
      entry.files.add(container ?? name);
      urls.set(host, entry);
    }
    if (Object.keys(found).length)
      hits.push({
        file: container ?? name,
        inside: container ? name : undefined,
        class: classOf(container ?? name),
        found
      });
  };
  for (const file of files) {
    const path = join(root, ...file.path.split('/'));
    if (TEXT.test(file.path)) scan(file.path, readFileSync(path, 'utf8'));
    else if (ARCHIVE.test(file.path)) {
      try {
        for (const [inner, bytes] of Object.entries(
          unzipSync(new Uint8Array(readFileSync(path)))
        )) {
          if (TEXT.test(inner) || /\.gs$/.test(inner) || !inner.includes('.'))
            scan(inner, strFromU8(bytes), file.path);
        }
      } catch {
        // Not a readable archive: reported as a file, not scanned.
      }
    }
  }
  // One row per file, with the archive entries folded in.
  const rows = new Map();
  for (const hit of hits) {
    const row = rows.get(hit.file) ?? { file: hit.file, class: hit.class, found: {} };
    for (const [id, count] of Object.entries(hit.found))
      row.found[id] = (row.found[id] ?? 0) + count;
    rows.set(hit.file, row);
  }
  return {
    files: files.length,
    routeTable: routes !== null,
    hits: [...rows.values()].sort(
      (a, b) => a.class.localeCompare(b.class) || a.file.localeCompare(b.file)
    ),
    hosts: [...urls.entries()]
      .map(([host, v]) => ({ host, mentions: v.count, files: v.files.size }))
      .sort((a, b) => b.mentions - a.mentions)
  };
}

/**
 * Compare a scan with the list of known remaining occurrences. A hit is allowed when its
 * class is listed, every pattern it contains is listed for that class, and (for a static
 * asset or resource) its path matches one of the listed paths. Returns the rest.
 */
export function violations(scan, known) {
  const out = [];
  for (const hit of scan.hits) {
    if (hit.class === 'notice') continue;
    const rule = known.classes[hit.class];
    const ids = Object.keys(hit.found).filter(
      (id) => !PATTERNS.find((p) => p.id === id)?.exception
    );
    if (!ids.length) continue;
    if (!rule) {
      out.push({ ...hit, why: `class ${hit.class} is not allowed to name these` });
      continue;
    }
    const extra = ids.filter((id) => !rule.patterns.includes(id));
    if (extra.length)
      out.push({ ...hit, why: `${extra.join(', ')} is not allowed in ${hit.class}` });
    else if (rule.paths && !rule.paths.some((pattern) => new RegExp(pattern).test(hit.file)))
      out.push({ ...hit, why: `${hit.file} is not a listed ${hit.class}` });
  }
  return out;
}

function report(scan) {
  const lines = [];
  const byClass = new Map();
  for (const hit of scan.hits) byClass.set(hit.class, [...(byClass.get(hit.class) ?? []), hit]);
  lines.push(
    `${scan.files} files scanned${scan.routeTable ? '' : ' (no route table: every script counts as shared)'}; ${scan.hits.length} contain a hosted string.`
  );
  for (const [cls, rows] of [...byClass.entries()].sort()) {
    lines.push('', `${cls} (${rows.length})`);
    for (const row of rows)
      lines.push(
        `  ${row.file}  ${Object.entries(row.found)
          .map(([id, n]) => `${id}x${n}`)
          .join(' ')}`
      );
  }
  lines.push('', 'Other hosts named in the build (no hit above, for information):');
  for (const { host, mentions, files } of scan.hosts.slice(0, 25))
    lines.push(`  ${host}  ${mentions} mentions in ${files} files`);
  return lines.join('\n');
}

async function main() {
  const args = process.argv.slice(2);
  const option = (name, fallback) =>
    args.includes(name) ? args[args.indexOf(name) + 1] : fallback;
  const here = dirname(fileURLToPath(import.meta.url));
  const buildDir = option('--build', join(here, '..', 'build'));
  if (!existsSync(buildDir)) {
    console.error(`No build at ${buildDir}. Run the build first.`);
    process.exit(2);
  }
  const scan = await scanBuild(buildDir);
  console.log(report(scan));
  if (args.includes('--json')) {
    const { writeFileSync } = await import('node:fs');
    writeFileSync(option('--json'), JSON.stringify(scan, null, 2) + '\n');
  }
  if (args.includes('--strict')) {
    // The acceptance criterion as written: nothing but third-party notices.
    const bad = scan.hits.filter(
      (hit) => hit.class !== 'notice' && Object.keys(hit.found).some((id) => id === 'hosted-domain')
    );
    console.log(
      `\nstrict: ${bad.length} file(s) outside third-party notices contain dynodoc.online.`
    );
    if (bad.length) process.exit(1);
  }
  if (args.includes('--check')) {
    const known = JSON.parse(readFileSync(join(here, 'hosted-strings-known.json'), 'utf8'));
    const bad = violations(scan, known);
    if (bad.length) {
      console.log('\nNot in scripts/hosted-strings-known.json:');
      for (const row of bad) console.log(`  ${row.file}  ${row.why}`);
      process.exit(1);
    }
    console.log('\ncheck: every occurrence is in the known list.');
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
