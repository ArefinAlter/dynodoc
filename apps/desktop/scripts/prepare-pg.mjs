#!/usr/bin/env node
// Prepares the PostgreSQL tree the app bundles and starts for the workspace on the person's computer
// (decision 019): src-tauri/resources/pg. The programs are the embedded PostgreSQL binaries of the
// zonky.io project (Maven Central, PostgreSQL 16.15), checked against the SHA-256 Maven Central itself
// publishes, then cut down to what the app needs:
//
//   bin    postgres, initdb, pg_ctl and the libraries they load (Windows: the Visual C++ runtime beside
//          them, taken from this computer's System32 or the folder named by --vcruntime)
//   lib    the plpgsql, pgcrypto, pg_trgm and snowball modules and the UTF-8 converters
//   share  the catalogue files, time zones, text-search data, and only the three extensions the engine's
//          migrations create (plpgsql, pgcrypto, pg_trgm); no translations, no contrib, no docs
//
//   node scripts/prepare-pg.mjs                              this computer's platform
//   node scripts/prepare-pg.mjs --platform linux
//   node scripts/prepare-pg.mjs --jar path/to/the.jar        use a jar you downloaded (still checked)
//   node scripts/prepare-pg.mjs --vcruntime C:/Windows/System32
//
// Needs `tar` (Windows 10 and later have one that reads zip and xz).

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const VERSION = '16.15.0';
const SOURCES = {
  windows: {
    artifact: 'embedded-postgres-binaries-windows-amd64',
    sha256: '51c7812dc1af47c9a2ccb64fe74efb88c515cff2da347ce72aab92b4cc8e1191',
    txz: 'postgres-windows-x86_64.txz',
  },
  linux: {
    artifact: 'embedded-postgres-binaries-linux-amd64',
    sha256: '653abc065c682b85d3da50168fb95dc524bd85426cdec9ce3695a550ef431df2',
    txz: 'postgres-linux-x86_64.txz',
  },
};

const here = dirname(fileURLToPath(import.meta.url));
const out = resolve(here, '..', 'src-tauri', 'resources', 'pg');

function fail(message) {
  console.error(`prepare-pg: ${message}`);
  process.exit(1);
}

const args = process.argv.slice(2);
let platform = process.platform === 'win32' ? 'windows' : 'linux';
let jar = null;
let vcruntime = process.env.SystemRoot ? join(process.env.SystemRoot, 'System32') : null;
for (let i = 0; i < args.length; i += 1) {
  if (args[i] === '--platform') platform = args[(i += 1)];
  else if (args[i] === '--jar') jar = resolve(args[(i += 1)]);
  else if (args[i] === '--vcruntime') vcruntime = resolve(args[(i += 1)]);
  else fail(`unknown argument: ${args[i]}`);
}
const src = SOURCES[platform];
if (!src) fail('--platform is windows or linux');

const work = mkdtempSync(join(tmpdir(), 'dynodoc-pg-'));
try {
  if (!jar) {
    const url = `https://repo1.maven.org/maven2/io/zonky/test/postgres/${src.artifact}/${VERSION}/${src.artifact}-${VERSION}.jar`;
    console.log(`prepare-pg: downloading ${url}`);
    const response = await fetch(url);
    if (!response.ok) fail(`${url} answered ${response.status}`);
    jar = join(work, 'pg.jar');
    writeFileSync(jar, Buffer.from(await response.arrayBuffer()));
  }
  const digest = createHash('sha256').update(readFileSync(jar)).digest('hex');
  if (digest !== src.sha256) fail(`the jar's SHA-256 is ${digest}, not the ${src.sha256} Maven Central publishes for ${VERSION}`);
  console.log(`prepare-pg: ${jar} matches the published SHA-256`);

  const tar = (a, cwd) => {
    // Windows' own bsdtar (reads zip and xz); the tar that Git for Windows puts first on the PATH takes D:\... for a host name.
    const program = process.platform === 'win32' && process.env.SystemRoot ? join(process.env.SystemRoot, 'System32', 'tar.exe') : 'tar';
    const r = spawnSync(program, a, { cwd, stdio: 'inherit' });
    if (r.status !== 0) fail(`tar ${a.join(' ')} failed`);
  };
  tar(['-xf', jar, src.txz], work);
  const tree = join(work, 'tree');
  mkdirSync(tree);
  tar(['-xf', join(work, src.txz), '-C', tree], work);

  rmSync(out, { recursive: true, force: true });
  mkdirSync(join(out, 'bin'), { recursive: true });
  mkdirSync(join(out, 'lib'), { recursive: true });
  const keep = (from, to, accept) => {
    for (const name of readdirSync(from)) if (accept(name)) cpSync(join(from, name), join(to, name), { recursive: true });
  };
  if (platform === 'windows') {
    const DLLS = new Set(['icudt67.dll', 'icuin67.dll', 'icuuc67.dll', 'libcrypto-3-x64.dll', 'libiconv-2.dll', 'libintl-9.dll', 'liblz4.dll', 'libpq.dll', 'libssl-3-x64.dll', 'libwinpthread-1.dll', 'libxml2.dll', 'libzstd.dll', 'zlib1.dll']);
    keep(join(tree, 'bin'), join(out, 'bin'), (n) => ['postgres.exe', 'initdb.exe', 'pg_ctl.exe'].includes(n) || DLLS.has(n));
    for (const name of ['vcruntime140.dll', 'vcruntime140_1.dll', 'msvcp140.dll']) {
      if (!vcruntime || !existsSync(join(vcruntime, name))) fail(`${name} was not found in ${vcruntime}; pass --vcruntime <folder with the Visual C++ runtime>`);
      cpSync(join(vcruntime, name), join(out, 'bin', name));
    }
    keep(join(tree, 'lib'), join(out, 'lib'), (n) => /^(plpgsql|pgcrypto|pg_trgm|dict_snowball)\.dll$/.test(n) || /^utf8_and_.*\.dll$/.test(n));
  } else {
    keep(join(tree, 'bin'), join(out, 'bin'), (n) => ['postgres', 'initdb', 'pg_ctl'].includes(n));
    // The shared libraries the three programs load sit in lib/ on Linux, beside the modules.
    keep(join(tree, 'lib'), join(out, 'lib'), (n) => /\.so(\.|$)/.test(n) && !/^(libecpg|libpgtypes|libpq\.a)/.test(n) && !/(wx|curl|xslt)/.test(n));
  }
  mkdirSync(join(out, 'share'), { recursive: true });
  keep(join(tree, 'share'), join(out, 'share'), (n) => !['locale', 'contrib', 'doc', 'man', 'extension'].includes(n));
  mkdirSync(join(out, 'share', 'extension'), { recursive: true });
  keep(join(tree, 'share', 'extension'), join(out, 'share', 'extension'), (n) => /^(plpgsql|pgcrypto|pg_trgm)/.test(n));

  const bytes = (dir) => readdirSync(dir, { withFileTypes: true }).reduce((n, e) => n + (e.isDirectory() ? bytes(join(dir, e.name)) : statSync(join(dir, e.name)).size), 0);
  console.log(`prepare-pg: ${out} (${(bytes(out) / 1048576).toFixed(1)} MB)`);
} finally {
  rmSync(work, { recursive: true, force: true });
}
