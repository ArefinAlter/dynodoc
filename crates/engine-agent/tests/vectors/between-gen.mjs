// Generates tests/vectors/between.json by RUNNING the real `between()` of the web editor.
//
//   node crates/engine-agent/tests/vectors/between-gen.mjs [--check]
//
// The function is not copied: the script transpiles
// frontend/researcher/src/lib/workspace/changes.ts with the TypeScript compiler (no bundler),
// evaluates the result in a sandbox whose imports are inert stubs (`between` uses none of
// them) and calls the exported function. Output is deterministic (fixed seed, no Math.random),
// so `--check` can compare a fresh run with the committed file.
//
// TypeScript is looked up in this order: $TYPESCRIPT_DIR, then
// frontend/researcher/node_modules/typescript under the repository root, then (for a git
// worktree without node_modules) the main checkout's copy named by $DYNODOC_MAIN_CHECKOUT.
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import vm from 'node:vm';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../../..');
const source = 'frontend/researcher/src/lib/workspace/changes.ts';
const out = path.join(here, 'between.json');

function findTypescript() {
  const candidates = [
    process.env.TYPESCRIPT_DIR,
    path.join(root, 'frontend/researcher/node_modules/typescript'),
    process.env.DYNODOC_MAIN_CHECKOUT &&
      path.join(process.env.DYNODOC_MAIN_CHECKOUT, 'frontend/researcher/node_modules/typescript')
  ].filter(Boolean);
  for (const dir of candidates) {
    if (existsSync(path.join(dir, 'package.json'))) return dir;
  }
  throw new Error(
    'TypeScript not found. Set TYPESCRIPT_DIR (or DYNODOC_MAIN_CHECKOUT) to a directory that ' +
      'holds node_modules/typescript for frontend/researcher.'
  );
}

const tsDir = findTypescript();
const require = createRequire(path.join(tsDir, 'package.json'));
const ts = require(path.join(tsDir, 'lib/typescript.js'));
const text = readFileSync(path.join(root, source), 'utf8');

// The exact text of the function, stored so a test can detect that the source has drifted.
const fn = /export function between[\s\S]*?\n}\n/.exec(text);
if (!fn) throw new Error('export function between not found in ' + source);

const js = ts.transpileModule(text, {
  compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2022 }
}).outputText;
const inert = () =>
  new Proxy(function () {}, { get: (_t, k) => (k === '__esModule' ? true : inert()), apply: inert });
const mod = { exports: {} };
vm.runInNewContext(js, { module: mod, exports: mod.exports, require: inert });
const between = mod.exports.between;
if (typeof between !== 'function') throw new Error('between was not exported');

// xorshift64* over BigInt: deterministic and independent of the JavaScript engine.
let state = 0x9e3779b97f4a7c15n;
const mask = (1n << 64n) - 1n;
function next() {
  state ^= state >> 12n;
  state ^= (state << 25n) & mask;
  state ^= state >> 27n;
  return Number(((state * 0x2545f4914f6cdd1dn) & mask) >> 32n);
}
const below = (n) => next() % n;
const pick = (xs) => xs[below(xs.length)];

const ascii = Array.from({ length: 95 }, (_, i) => String.fromCharCode(32 + i));
const alphabets = [
  ascii,
  ['a', 'b'],
  ['a', 'b', 'c', 'd'],
  ['0', '1', '2'],
  ['y', 'z', '{', '|', '}', '~'],
  [' ', '!', '"', '#'],
  ['O', 'P', 'a', 'z'],
  [...'0123456789'],
  ['a', 'b', 'é', '中', '😀', '\u0000', '\u007f', '\u0080']
];
function key(alphabet, max) {
  const n = below(max + 1);
  let s = '';
  for (let i = 0; i < n; i++) s += pick(alphabet);
  return s;
}

const cases = [];
let skipped = 0;
function add(left, right) {
  const k = between(left, right ?? undefined);
  if (!k.isWellFormed()) {
    skipped++;
    return;
  }
  cases.push([left, right, k]);
}

// Hand-picked edge cases.
const fixed = [
  ['', null],
  ['', ''],
  ['', 'a'],
  ['', '!'],
  [' ', null],
  ['a', null],
  ['a', 'a'],
  ['a', 'b'],
  ['b', 'a'],
  ['a', 'a0'],
  ['a', 'a '],
  ['a', 'a~'],
  ['a}', 'a~'],
  ['}', null],
  ['}}', null],
  ['~', null],
  ['~~', null],
  ['0', '1'],
  ['00000010', '00000020'],
  ['00000019', '00000020'],
  ['00000099', null],
  ['W', null],
  ['a0', 'a1'],
  ['a0', 'a0P'],
  ['\u0000', null],
  ['a', '\u0000'],
  ['é', null],
  ['中', null],
  ['😀', null],
  ['a', '😀'],
  ['😀', '😁'],
  ['x'.repeat(198), null],
  ['x'.repeat(199), null],
  ['x'.repeat(200), null],
  ['x'.repeat(201), null],
  ['x'.repeat(199), 'x'.repeat(199) + 'y'],
  ['x'.repeat(200), 'x'.repeat(199) + 'y'],
  ['x'.repeat(200), 'x'.repeat(200) + '!'],
  ['}'.repeat(199), null],
  ['}'.repeat(200), null],
  ['~'.repeat(199), null],
  ['~'.repeat(200), null],
  ['a'.repeat(200), 'b'],
  ['', 'x'.repeat(200)],
  [' '.repeat(199), 'x'],
  ['', ' '.repeat(5) + '!']
];
for (const [l, r] of fixed) add(l, r);

// Random pairs: arbitrary order, then ordered (left < right) pairs.
while (cases.length < 5000) {
  const a = pick(alphabets);
  add(key(a, 12), below(8) === 0 ? null : key(a, 12));
}
while (cases.length < 10000) {
  const a = pick(alphabets);
  let l = key(a, 12);
  let r = key(a, 12);
  if (l > r) [l, r] = [r, l];
  if (l === r) r = null;
  add(l, r);
}

// Chains: each step feeds on the previous result, so one wrong character anywhere diverges
// every later key. Only every `every`-th key is stored.
const chains = [];
function chain(mode, left0, right0, steps, every) {
  let left = left0;
  let right = right0;
  const keys = [];
  for (let i = 1; i <= steps; i++) {
    const k = between(left, right ?? undefined);
    if (mode === 'left_chases') left = k;
    else right = k;
    if (i % every === 0) keys.push(k);
  }
  chains.push({ mode, left: left0, right: right0, steps, every, keys });
}
chain('left_chases', '', null, 1600, 20);
chain('left_chases', '00000010', '00000020', 1600, 20);
chain('left_chases', 'a', 'b', 1600, 20);
chain('right_chases', '', 'm', 1600, 20);
chain('right_chases', 'a', 'b', 1600, 20);

const body = {
  description:
    'Position keys: the output of between() of frontend/researcher/src/lib/workspace/changes.ts, ' +
    'produced by running that function (see between-gen.mjs). cases are [left, right, key] with ' +
    'right null for "no right neighbour". chains feed each result back (mode left_chases: ' +
    'left = key; right_chases: right = key) and store every `every`-th key.',
  source,
  between_source: fn[0],
  typescript: ts.version,
  generator: 'crates/engine-agent/tests/vectors/between-gen.mjs',
  skipped_not_well_formed: skipped
};
const lines = [
  '{',
  ...Object.entries(body).map(([k, v]) => `  ${JSON.stringify(k)}: ${JSON.stringify(v)},`),
  '  "cases": [',
  cases.map((c) => '    ' + JSON.stringify(c)).join(',\n'),
  '  ],',
  '  "chains": [',
  chains.map((c) => '    ' + JSON.stringify(c)).join(',\n'),
  '  ]',
  '}',
  ''
];
const text_out = lines.join('\n');
if (process.argv.includes('--check')) {
  const have = readFileSync(out, 'utf8').replace(/\r\n/g, '\n');
  if (have !== text_out) {
    console.error('between.json is out of date; run the generator without --check');
    process.exit(1);
  }
  console.log('between.json is current (' + cases.length + ' cases, ' + chains.length + ' chains)');
} else {
  writeFileSync(out, text_out);
  console.log('wrote ' + out + ': ' + cases.length + ' cases, ' + chains.length + ' chains');
}
