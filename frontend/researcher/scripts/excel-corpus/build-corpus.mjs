// Turn the output of run-excel.ps1 into the committed corpus fixture.
//   node scripts/excel-corpus/build-corpus.mjs excel-out.json
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { grid, names } from './cases.mjs';

const source = process.argv[2];
if (!source) throw new Error('Pass the JSON written by run-excel.ps1.');
const text = readFileSync(source, 'utf8');
const results = JSON.parse(text.charCodeAt(0) === 0xfeff ? text.slice(1) : text);

// Functions the reference Excel build lacks (it answers #NAME?) although the engine implements
// them, and names that only HyperFormula knew. They have no Excel truth and are tested by hand.
const WITHOUT_EXCEL_TRUTH =
  /^=(COUNTUNIQUE|TEXTBEFORE|TEXTAFTER|TEXTSPLIT|ISBINARY|MAXPOOL|MEDIANPOOL|COVARIANCEP|SKEWP|VSTACK|HSTACK|TAKE|DROP|CHOOSECOLS|CHOOSEROWS|TOCOL|TOROW|EXPAND|WRAPROWS|ARRAY_CONSTRAIN|ARRAYFORMULA|VERSION|SPLIT)\(/;
// Machine-, locale- or literal-specific results that say nothing about calculation.
const SKIP = new Set([
  '="€5"+0',
  '="5€"+0',
  '=TEXT(B8,"@@")',
  '=9.99999999999999E+99&""',
  '=1.7976931348623157E+308&""',
  '=ROUND(5E-324,2)',
  '=ROUND(1E308,-300)',
  '=1E308*10'
]);
// Excel stops iterative solvers earlier and approximates Bessel functions to about 1e-8.
const tolerance = (f) => (f.startsWith('=BESSEL') || f.startsWith('=XIRR(') ? 1e-6 : undefined);

const encode = (v) => (v === null || v === undefined ? null : v.error ? { e: v.error } : v);
const cases = [];
for (const e of results) {
  if (SKIP.has(e.f)) continue;
  const unavailable =
    WITHOUT_EXCEL_TRUTH.test(e.f) && (e.parseError || (e.value && e.value.error === '#NAME?'));
  if (unavailable) continue;
  let out;
  if (e.parseError) out = { x: 1 };
  else if (e.array) out = { a: e.array.map((row) => row.map(encode)) };
  else out = encode(e.value);
  const row = [e.f, out];
  const t = tolerance(e.f);
  if (t) row.push(t);
  cases.push(row);
}
const target = fileURLToPath(
  new URL('../../src/lib/workspace/formula/fixtures/excel-corpus.json', import.meta.url)
);
writeFileSync(
  target,
  JSON.stringify({
    source:
      'Microsoft Excel 16.0 (build 14326), en-US, 1900 date system; each case is one formula entered in Data!J20 of the grid below',
    options: { dateOrder: 'MDY', now: 46310.5 },
    grid,
    names,
    cases
  })
);
console.log(`wrote ${cases.length} cases to ${target}`);
