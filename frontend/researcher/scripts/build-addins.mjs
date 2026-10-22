// Offline builder for one installation's add-in files (docs/SELF-HOSTED-EDITION.md 7.2, P0-7).
// It writes the Office manifests (standard and catalog-compatible), the Google editor add-on
// packages and a SHA256SUMS file into a directory, for installations that distribute them by
// hand or have no route to the web server. It uses the same code as the web server's
// /connectors/addins routes (src/lib/addins), so a file built here is byte for byte the file the
// running server would serve for the same settings. It needs only Node.js 20 or later and the
// files in this package: no network, no other dependency, nothing is sent to Microsoft or Google.
//
// usage: node scripts/build-addins.mjs --origin https://dynodoc.example.org --out ./addins
//          [--brand "Acme Docs"] [--app-version 1.0.0] [--support-url https://help.example.org]
//          [--provider "Acme Research Ltd."] [--display-suffix "(staging)"]
//          [--force] [--json] [--allow-loopback-http]
//
//   --origin               required. The address people open Dynodoc at: https, a host name and
//                          an optional port, no path or trailing slash. Anything else is refused.
//   --out                  required. The directory to write to. It is created if missing. A
//                          directory that already holds files is refused unless --force.
//   --brand                the product name in the add-in names (default: Dynodoc; at most 60 characters)
//   --app-version          the version the manifest version follows, as major.minor.patch
//                          (default: the version in package.json)
//   --support-url          an https address for the "support" link in the manifests
//                          (default: the origin itself)
//   --provider             the provider shown in the add-in details (default: the brand)
//   --display-suffix       text added to each add-in name, to tell staging from production
//   --force                write into a directory that already holds files: the generated names are
//                          replaced, stale generated files from an earlier build are removed, any
//                          other file is left alone
//   --json                 print one JSON document on standard output and nothing else
//   --allow-loopback-http  accept http://localhost, http://127.x.x.x and http://[::1] (test
//                          stacks only; Office loads real add-ins over HTTPS only)
//
// exit codes: 0 built; 1 unexpected failure; 2 wrong command line (unknown or missing option);
// 3 an input was refused (unsafe origin, the hosted service's address, a bad name, version or
// link); 4 the output directory could not be used or written.
//
// --json output, on success:
//   { "ok": true, "origin", "brand", "appVersion", "manifestVersion", "supportUrl", "out",
//     "google": { "available": true } | { "available": false, "reason": "..." },
//     "files": [ { "name", "group": "office"|"google"|"checksums", "host", "variant", "id",
//                  "bytes", "sha256" } ] }
// and on failure: { "ok": false, "exit": 2|3|4|1, "code": "usage"|"invalid-input"|"output"|"internal",
//                   "error": "..." }.
// The same text goes to standard error without --json, and nothing is written to standard
// output in that case except the summary on success.
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync
} from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { buildAddinBundle } from '../src/lib/addins/bundle.js';
import { HOSTED } from '../src/lib/server/config/hosted.js';

const OPTIONS = {
  '--origin': 'value',
  '--out': 'value',
  '--brand': 'value',
  '--app-version': 'value',
  '--support-url': 'value',
  '--provider': 'value',
  '--display-suffix': 'value',
  '--force': 'flag',
  '--json': 'flag',
  '--allow-loopback-http': 'flag',
  '--help': 'flag'
};
const source = readFileSync(fileURLToPath(import.meta.url), 'utf8').split('\n');
const HELP = source
  .slice(
    2,
    source.findIndex((line, index) => index >= 2 && !line.startsWith('//'))
  )
  .map((line) => line.replace(/^\/\/ ?/, ''))
  .join('\n');

class Failure extends Error {
  /** @param {number} exit @param {string} code @param {string} message */
  constructor(exit, code, message) {
    super(message);
    this.exit = exit;
    this.code = code;
  }
}

/** @param {string[]} argv */
function parse(argv) {
  /** @type {Record<string, string | true>} */
  const given = {};
  for (let i = 0; i < argv.length; i++) {
    const name = argv[i];
    const kind = OPTIONS[/** @type {keyof typeof OPTIONS} */ (name)];
    if (!kind)
      throw new Failure(2, 'usage', `Unknown option ${name}. Use --help to see the options.`);
    if (name in given) throw new Failure(2, 'usage', `${name} was given twice.`);
    if (kind === 'flag') given[name] = true;
    else {
      const value = argv[++i];
      if (value === undefined || value.startsWith('--'))
        throw new Failure(2, 'usage', `${name} needs a value.`);
      given[name] = value;
    }
  }
  return given;
}

/** The version in package.json, one folder above this script. */
function packageVersion() {
  const path = fileURLToPath(new URL('../package.json', import.meta.url));
  try {
    return JSON.parse(readFileSync(path, 'utf8')).version;
  } catch {
    throw new Failure(2, 'usage', 'No package.json beside the script: give --app-version.');
  }
}

/** @param {string[]} argv */
export function run(argv) {
  const given = parse(argv);
  if (given['--help']) return { help: true };
  for (const required of ['--origin', '--out'])
    if (typeof given[required] !== 'string')
      throw new Failure(2, 'usage', `${required} is required. Use --help to see the options.`);
  const value = (/** @type {string} */ name) =>
    typeof given[name] === 'string' ? /** @type {string} */ (given[name]) : undefined;
  let bundle;
  try {
    bundle = buildAddinBundle({
      origin: /** @type {string} */ (value('--origin')),
      brand: value('--brand') ?? 'Dynodoc',
      appVersion: value('--app-version') ?? packageVersion(),
      supportUrl: value('--support-url'),
      provider: value('--provider'),
      displaySuffix: value('--display-suffix'),
      allowLoopbackHttp: given['--allow-loopback-http'] === true,
      // An installation's files never name the hosted service.
      forbidDomain: HOSTED.domain
    });
  } catch (cause) {
    if (cause instanceof RangeError) throw new Failure(3, 'invalid-input', cause.message);
    throw cause;
  }
  const out = resolve(/** @type {string} */ (value('--out')));
  const names = new Set([
    ...bundle.files.map((file) => file.name),
    // Names an earlier build may have written and this one does not.
    'google-docs.zip',
    'google-sheets.zip',
    'google-slides.zip'
  ]);
  try {
    if (existsSync(out)) {
      if (!statSync(out).isDirectory())
        throw new Failure(4, 'output', `${out} exists and is not a directory.`);
      if (readdirSync(out).length > 0 && given['--force'] !== true)
        throw new Failure(
          4,
          'output',
          `${out} already holds files. Use an empty directory, or --force to replace the files this tool writes.`
        );
    } else mkdirSync(out, { recursive: true });
    // Everything is computed before the first file is written. A leftover from an earlier build
    // that this build does not produce (the Google packages for an address that no longer has
    // them) is removed so the directory never mixes two builds.
    for (const name of names)
      if (!bundle.files.some((file) => file.name === name) && existsSync(join(out, name)))
        rmSync(join(out, name));
    for (const file of bundle.files) writeFileSync(join(out, file.name), file.bytes);
  } catch (cause) {
    if (cause instanceof Failure) throw cause;
    throw new Failure(
      4,
      'output',
      `Could not write to ${out}: ${cause instanceof Error ? cause.message : String(cause)}`
    );
  }
  return {
    ok: true,
    origin: bundle.origin,
    brand: bundle.brand,
    appVersion: bundle.appVersion,
    manifestVersion: bundle.manifestVersion,
    supportUrl: bundle.supportUrl,
    out,
    google: bundle.google,
    files: bundle.files.map((file) => ({
      name: file.name,
      group: file.group,
      host: file.host ?? null,
      variant: file.variant ?? null,
      id: file.id ?? null,
      bytes: file.bytes.length,
      sha256: file.sha256
    }))
  };
}

function main() {
  const argv = process.argv.slice(2);
  const json = argv.includes('--json');
  try {
    const result = run(argv);
    if ('help' in result) {
      console.log(HELP);
      return 0;
    }
    if (json) console.log(JSON.stringify(result, null, 2));
    else {
      console.log(`Add-in files for ${result.origin} written to ${result.out}`);
      for (const file of result.files) console.log(`  ${file.sha256.slice(0, 12)}  ${file.name}`);
      if (!result.google.available) console.log(`No Google packages: ${result.google.reason}`);
    }
    return 0;
  } catch (cause) {
    const failure =
      cause instanceof Failure
        ? cause
        : new Failure(1, 'internal', cause instanceof Error ? cause.message : String(cause));
    if (json)
      console.log(
        JSON.stringify(
          { ok: false, exit: failure.exit, code: failure.code, error: failure.message },
          null,
          2
        )
      );
    else console.error(`build-addins: ${failure.message}`);
    return failure.exit;
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url))
  process.exitCode = main();
