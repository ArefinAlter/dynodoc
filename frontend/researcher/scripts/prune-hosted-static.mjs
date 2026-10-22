// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-nocheck -- a command-line tool, exercised by its tests.
// Removes from a finished build the static files that exist only for the hosted service and
// name its address: the add-in packages built for app.dynodoc.online (static/connectors/packages).
// A self-hosted installation serves its own, made for its own address, under /connectors/addins
// (docs/SELF-HOSTED-EDITION.md 7.2), so the image of the self-hosted bundle does not carry the
// hosted ones (infra/selfhosted/images/Dockerfile.web runs this after `pnpm build`). The hosted
// service's build is never pruned and keeps serving them exactly as before.
//
// usage: node scripts/prune-hosted-static.mjs [--build build] [--dry-run] [--json]
//   --build    the build directory (default: ../build next to this script)
//   --dry-run  list what would be removed and remove nothing
//   --json     print { "build", "removed": [...], "dryRun" } and nothing else
// exit codes: 0 done (also when there was nothing to remove); 2 no build directory or a wrong option.
import { existsSync, readdirSync, rmSync, rmdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

/** Relative to <build>/client; the same names as static/connectors/packages in the source. */
export const HOSTED_ADDIN_FILES = [
  'connectors/packages/word.xml',
  'connectors/packages/excel.xml',
  'connectors/packages/powerpoint.xml',
  'connectors/packages/google-docs.zip',
  'connectors/packages/google-sheets.zip',
  'connectors/packages/google-slides.zip'
];
/** A precompressed copy a build may carry beside the file. */
const COPIES = ['', '.br', '.gz'];

/**
 * @param {string} buildDir
 * @param {{ dryRun?: boolean }} [options]
 * @returns {string[]} the paths removed (or that would be), relative to the build directory
 */
export function prune(buildDir, { dryRun = false } = {}) {
  const root = resolve(buildDir);
  const removed = [];
  for (const name of HOSTED_ADDIN_FILES)
    for (const suffix of COPIES) {
      const path = join(root, 'client', ...`${name}${suffix}`.split('/'));
      if (!existsSync(path)) continue;
      removed.push(`client/${name}${suffix}`);
      if (!dryRun) rmSync(path);
    }
  const folder = join(root, 'client', 'connectors', 'packages');
  if (!dryRun && existsSync(folder) && readdirSync(folder).length === 0) rmdirSync(folder);
  return removed;
}

function main() {
  const args = process.argv.slice(2);
  const known = ['--build', '--dry-run', '--json'];
  const unknown = args.filter((arg, index) => arg.startsWith('--') && !known.includes(arg));
  const buildIndex = args.indexOf('--build');
  const value = buildIndex >= 0 ? args[buildIndex + 1] : undefined;
  if (unknown.length || (buildIndex >= 0 && (!value || value.startsWith('--')))) {
    console.error(
      `prune-hosted-static: unknown option or missing value: ${unknown[0] ?? '--build'}`
    );
    return 2;
  }
  const here = dirname(fileURLToPath(import.meta.url));
  const buildDir = value ?? join(here, '..', 'build');
  if (!existsSync(buildDir)) {
    console.error(`prune-hosted-static: no build at ${buildDir}. Run the build first.`);
    return 2;
  }
  const dryRun = args.includes('--dry-run');
  const removed = prune(buildDir, { dryRun });
  if (args.includes('--json'))
    console.log(JSON.stringify({ build: resolve(buildDir), removed, dryRun }, null, 2));
  else {
    console.log(`${dryRun ? 'Would remove' : 'Removed'} ${removed.length} hosted add-in file(s).`);
    for (const path of removed) console.log(`  ${path}`);
  }
  return 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url))
  process.exitCode = main();
