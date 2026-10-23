// eslint-disable-next-line @typescript-eslint/ban-ts-comment
// @ts-nocheck -- a command-line tool, exercised by its tests.
// Copies an operator-supplied copy of Office.js into the folder the web server serves it from
// (DYNODOC_OFFICE_JS_DIR, docs/SELF-HOSTED-EDITION.md 7.3). Dynodoc does not ship Office.js:
// whether it may be passed on has not been reviewed, so the operator brings the files (for
// example the dist folder of Microsoft's @microsoft/office-js package, or a copy of the files
// under https://appsforoffice.microsoft.com/lib/1/hosted/) and accepts Microsoft's terms for
// them. This tool copies only what the server would hand out, refuses anything unexpected, and
// does not read the network.
//
// usage: node scripts/stage-office-js.mjs --from <folder with office.js> --to <folder> [--force] [--json]
//   --from   the folder that holds office.js and the files beside it (sub-folders such as en-us
//            are copied too)
//   --to     the folder to fill; it is created if missing, and must be empty unless --force
//   --force  replace files that are already in --to
//   --json   print one JSON document and nothing else
//
// Copied: files whose extension the server serves (js, css, json, map, txt, png, gif, woff,
// woff2), up to 16 MiB each, 2000 files and 6 folder levels. Skipped and listed: everything else,
// hidden files, and links. Refused: a --from without office.js, a --from and --to inside each
// other.
// exit codes: 0 copied; 1 unexpected failure; 2 wrong command line; 3 the source is not usable;
// 4 the target could not be used or written.
import {
  copyFileSync,
  existsSync,
  lstatSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  realpathSync,
  statSync
} from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, extname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { MAX_FILE_BYTES, SERVED_TYPES } from '../src/lib/addins/office-js-files.js';

const MAX_FILES = 2000;
const MAX_DEPTH = 6;

class Failure extends Error {
  constructor(exit, code, message) {
    super(message);
    this.exit = exit;
    this.code = code;
  }
}

const source = readFileSync(fileURLToPath(import.meta.url), 'utf8').split('\n');
const HELP = source
  .slice(
    2,
    source.findIndex((line, index) => index >= 2 && !line.startsWith('//'))
  )
  .map((line) => line.replace(/^\/\/ ?/, ''))
  .join('\n');

/** Every file under `dir`, relative, with why it is skipped (or null). */
function inventory(dir) {
  const files = [];
  const walk = (folder, depth) => {
    for (const entry of readdirSync(folder, { withFileTypes: true })) {
      const path = join(folder, entry.name);
      const name = relative(dir, path).split(sep).join('/');
      if (entry.name.startsWith('.')) files.push({ name, skip: 'hidden' });
      else if (lstatSync(path).isSymbolicLink()) files.push({ name, skip: 'link' });
      else if (entry.isDirectory()) {
        if (depth >= MAX_DEPTH) files.push({ name, skip: 'too deep' });
        else walk(path, depth + 1);
      } else if (!SERVED_TYPES[extname(entry.name).toLowerCase()])
        files.push({ name, skip: 'type not served' });
      else if (statSync(path).size > MAX_FILE_BYTES) files.push({ name, skip: 'too large' });
      else files.push({ name, skip: null });
    }
  };
  walk(dir, 1);
  return files;
}

export function run(argv) {
  const given = {};
  for (let i = 0; i < argv.length; i++) {
    const name = argv[i];
    if (name === '--force' || name === '--json' || name === '--help') given[name] = true;
    else if (name === '--from' || name === '--to') {
      const value = argv[++i];
      if (!value || value.startsWith('--')) throw new Failure(2, 'usage', `${name} needs a value.`);
      if (name in given) throw new Failure(2, 'usage', `${name} was given twice.`);
      given[name] = value;
    } else throw new Failure(2, 'usage', `Unknown option ${name}. Use --help to see the options.`);
  }
  if (given['--help']) return { help: true };
  for (const required of ['--from', '--to'])
    if (!given[required]) throw new Failure(2, 'usage', `${required} is required.`);
  const from = resolve(given['--from']);
  const to = resolve(given['--to']);
  if (!existsSync(from) || !statSync(from).isDirectory())
    throw new Failure(3, 'source', `${from} is not a folder.`);
  const realFrom = realpathSync(from);
  const inside = (a, b) => a === b || a.startsWith(b + sep);
  const realTo = existsSync(to) ? realpathSync(to) : to;
  if (inside(realTo, realFrom) || inside(realFrom, realTo))
    throw new Failure(3, 'source', '--from and --to must be separate folders.');
  const all = inventory(from);
  const copy = all.filter((file) => !file.skip);
  if (!copy.some((file) => file.name === 'office.js'))
    throw new Failure(3, 'source', `${from} holds no office.js (a file of up to 16 MiB).`);
  if (copy.length > MAX_FILES)
    throw new Failure(3, 'source', `${from} holds more than ${MAX_FILES} files to copy.`);
  try {
    if (existsSync(to)) {
      if (!statSync(to).isDirectory()) throw new Failure(4, 'target', `${to} is not a folder.`);
      if (readdirSync(to).length > 0 && !given['--force'])
        throw new Failure(
          4,
          'target',
          `${to} already holds files. Use an empty folder, or --force to replace files.`
        );
    }
    const done = [];
    for (const file of copy) {
      const target = join(to, ...file.name.split('/'));
      mkdirSync(dirname(target), { recursive: true });
      copyFileSync(join(from, ...file.name.split('/')), target);
      done.push({
        name: file.name,
        bytes: statSync(target).size,
        sha256: createHash('sha256').update(readFileSync(target)).digest('hex')
      });
    }
    return {
      ok: true,
      from,
      to,
      copied: done,
      skipped: all.filter((file) => file.skip).map(({ name, skip }) => ({ name, reason: skip })),
      note: 'Dynodoc does not ship Office.js. You are responsible for your right to use and copy these files.'
    };
  } catch (cause) {
    if (cause instanceof Failure) throw cause;
    throw new Failure(4, 'target', `Could not write to ${to}: ${cause.message}`);
  }
}

function main() {
  const argv = process.argv.slice(2);
  try {
    const result = run(argv);
    if (result.help) console.log(HELP);
    else if (argv.includes('--json')) console.log(JSON.stringify(result, null, 2));
    else {
      console.log(`Copied ${result.copied.length} file(s) to ${result.to}.`);
      if (result.skipped.length) console.log(`Skipped ${result.skipped.length} file(s):`);
      for (const file of result.skipped) console.log(`  ${file.name} (${file.reason})`);
      console.log(result.note);
      console.log('Set DYNODOC_OFFICE_JS_DIR to the folder as the server sees it, and restart.');
    }
    return 0;
  } catch (cause) {
    const failure =
      cause instanceof Failure
        ? cause
        : new Failure(1, 'internal', cause instanceof Error ? cause.message : String(cause));
    if (argv.includes('--json'))
      console.log(
        JSON.stringify({
          ok: false,
          exit: failure.exit,
          code: failure.code,
          error: failure.message
        })
      );
    else console.error(`stage-office-js: ${failure.message}`);
    return failure.exit;
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url))
  process.exitCode = main();
