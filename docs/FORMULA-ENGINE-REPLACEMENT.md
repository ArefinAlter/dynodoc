# Formula engine replacement: HyperFormula out, in-house engine in

Date: 14 October 2026. Status: implemented in the workspace; production release pending.
Owner decision: 14 October 2026 (decision 015, resolving the open point of decision 013).

## Result in one paragraph

`hyperformula` (GPL-3.0-only, or a commercial licence) shipped in the browser bundle of a
closed-source product, so it was removed. No candidate library gave what the workspace
needs (A1 formulas with reference rebasing on structural edits, names, correct Excel
semantics, a function set at least as wide as before), so the spreadsheet calculation is now
an in-house, dependency-free engine in `frontend/researcher/src/lib/workspace/formula/`.
It covers all 408 user-facing HyperFormula functions plus 50 more (458 names), reproduces
2,020 Excel-evaluated formula cases and 243/243 formulas of an Excel-written workbook, and
matches HyperFormula's reference rebasing on 8,358 random structural edits without a
difference. Compared with HyperFormula it builds a 100,000-cell workbook 4 to 5 times faster and ships
130 KB minified (44 KB gzip) instead of 776 KB (164 KB gzip). Nothing was bought and no
library licence has to be honoured for calculation.

## What the application used HyperFormula for

Enumerated from the source and the tests before the change.

| Need | Where | HyperFormula API | Now |
| --- | --- | --- | --- |
| Calculate every formula of a workbook, incrementally | `workbook-calculator.ts` | `buildFromSheets`, `batch`, `setCellContents`, `getSheetValues`, `destroy` | `FormulaEngine` with the same method names |
| Validate and add a workbook name | `workbook-names.ts` | `addNamedExpression` | same; errors are readable messages |
| Rebase references on insert/delete row or column, rename and delete sheet | `sheet-structure.ts` | `addRows`, `removeRows`, `addColumns`, `removeColumns`, `renameSheet`, `removeSheet`, `getSheetSerialized`, `getAllNamedExpressionsSerialized` | same names |
| Sort, remove duplicates | `sheet-analysis.ts` | `setRowOrder`, `removeRows`, `getSheetValues`, `getSheetSerialized` | same names |
| Relative fill | `sheet-operations.ts` | `copy`, `paste`, `getCellSerialized` | `copyCell`, `getCellSerialized` |
| Function list in the Insert Function dialog | `SheetEditor.svelte` | `getRegisteredFunctionNames('enGB')` | `FormulaEngine.getRegisteredFunctionNames()`, generated from the registry |
| XLSX export value caches for charts | `xlsx-export.ts` | via `WorkbookCalculator` | unchanged |

Functions exercised by application code, tests and the example workbooks: `SUM` (unit tests,
e2e, `fieldwork-tracker.xlsx`) and the operators `+ - * /`. The XLSX importer carries any
Excel formula through unchanged, including `_xlfn.` prefixed names, so the practical
requirement is "the functions people really use in Excel and Sheets", not only `SUM`.
`licenseKey: 'gpl-v3'` appeared at six call sites and is gone.

## Requirements

1. Behaviour of everything the app and its tests rely on: A1 references, ranges, cross-sheet
   references, absolute and relative addressing, workbook and sheet names, operators, percent,
   booleans, dates as 1900-system serials, text/number coercion, error values, cycle
   detection, incremental recalculation with correct ordering.
2. A function set at least as wide as HyperFormula's, and the common Excel functions it
   lacked. Unknown functions produce `#NAME?`; nothing throws.
3. Reference rebasing on structural edits identical to before, keeping node IDs as
   `sheet-structure.ts` does.
4. No `eval`/`new Function`, bounded depth, work and memory, no prototype pollution through
   names, fuzzed for crashes.
5. Permissive licence for the engine and every transitive dependency; CSP stays
   `script-src 'self'` (no WebAssembly).
6. The function list shown to users comes from the registry.

## Candidates, with evidence

All licences were read from package metadata and the `LICENSE` files of the installed tree.
"Excel corpus" is the 2,013 cases of the corpus below that Excel answers with a value or an
error (first cell of array results); every candidate was given the same grid, names and
formulas through a thin adapter. The corpus was written by this project, so absolute
percentages favour what this project implemented; the ranking, not the number, is the point.

| Candidate | Licence (whole tree) | Last release | Rebasing on row/column edits | Functions | Excel corpus | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| `hyperformula` 3.4.0 | GPL-3.0-only or commercial; deps chevrotain 6.5 (Apache-2.0), tiny-emitter (MIT), regexp-to-ast (MIT) | Oct 2026 | yes | 408 | 68.2% | Removed: licence. Needs `FALSE()` with parentheses, lacks `CONCAT`, `LOOKUP`, `INDIRECT`, `XMATCH`, `AVERAGEIFS`, `LET`, counts empty strings as values |
| `fast-formula-parser` 1.0.19 | MIT; deps chevrotain 7 (Apache-2.0), jstat (MIT), bessel (Apache-2.0), bahttext (MIT), regexp-to-ast (MIT) | Oct 2023 | none (parser and evaluator only) | 280 | 61.1% | Rejected: no `MAX`, `MIN`, `COUNTA`, `MATCH`, `CHOOSE`, `SUBSTITUTE`, `VALUE`, `SUBTOTAL`, `XLOOKUP`, `OFFSET`, `INDIRECT`; rejects `1E3` and `.5`; no rebasing; unmaintained |
| `hot-formula-parser` 4.0.0 | MIT; deps @handsontable/formulajs (MIT), jstat (MIT), bessel (Apache-2.0), tiny-emitter (MIT) | Jun 2022 | none | 382 | 42.0% | Rejected: no sheet-qualified references, no arrays, unmaintained |
| `xlsx-calc` 0.9.2 | MIT, no dependencies | Aug 2024 | none | 49 | 20.8% | Rejected: tiny function set |
| `@formulajs/formulajs` 4.6.1 | MIT; deps jstat (MIT), bessel (Apache-2.0) | Jul 2026 | not applicable (functions only, no parser or graph) | 398 | n/a | Not used: a function library; its semantics still differ from Excel in places and it adds a 2.4 MB tree for functions that are cheap to write and verify |
| `@borgar/fx` 5.1.3 | MIT, no dependencies | Sep 2026 | tokeniser/parser only | n/a | n/a | Not used: parsing is the easy part and the engine needs its own spans |
| `formulon` 6.25.6 | MIT; decimal.js (MIT) | Jun 2025 | none | Salesforce formula language | n/a | Rejected: not an A1 spreadsheet language |
| `@ironcalc/wasm` 0.8.4 | MIT or Apache-2.0 | Aug 2026 | in the Rust engine | young, version 0.x | not run | Rejected: WebAssembly needs `'wasm-unsafe-eval'` in the CSP, which is `script-src 'self'` |
| `@univerjs/engine-formula` 1.0.4 | Apache-2.0 | Oct 2026 | inside the Univer platform | large | not run | Rejected: 40 MB, coupled to the Univer runtime and its dependency injection |
| In-house engine | none needed | this change | yes | 458 | 98.4% (see below) | **Chosen** |

Function-name coverage of the 389 distinct functions that appear in the corpus and that the
reference Excel build accepts (the "commonly used" set):

| Engine | Registered names | Covers the common set |
| --- | --- | --- |
| In-house engine | 458 | 389 / 389 (100%) |
| `hyperformula` | 408 | 354 / 389 (91%) |
| `@formulajs/formulajs` | 398 | 312 / 389 (80%) |
| `hot-formula-parser` | 382 | 305 / 389 (78%) |
| `fast-formula-parser` | 280 | 261 / 389 (67%) |
| `xlsx-calc` | 49 | 45 / 389 (12%) |

Choosing evidence in short: only HyperFormula rebases references, and it is the one that
cannot be shipped. Every permissive candidate is either a parser without a graph or an
evaluator with a smaller function set and no rebasing, so rebasing and incremental
recalculation would have been written here anyway; writing the evaluator as well removed an
adapter layer, a dependency tree with an Apache-2.0 notice obligation, and bug-for-bug
surprises.

## Decision

An in-house engine, written to Excel semantics and verified against Microsoft Excel itself.
Choosing Excel as the oracle rather than HyperFormula was deliberate: the files users import
and export are Excel files, and HyperFormula deviates from Excel in many places (see the list
below). Where the old engine's behaviour was something the editor depends on (rebasing,
cycle values, error object shape, sheet-ID stability), it was kept and tested against
HyperFormula directly.

## What was built

`src/lib/workspace/formula/`:

| Module | Content |
| --- | --- |
| `parser.ts` | Tokeniser and Pratt parser to an AST that keeps source spans; `A1`, `$A$1`, `A1:B2`, `A:A`, `1:3`, `Sheet!A1`, `'My Sheet'!A1`, `Sheet!Name`, intersection (space), union in parentheses, `@`, array literals, error literals, number literals truncated to 15 digits as Excel does |
| `evaluator.ts` | Operators with array broadcasting, lazy `IF`-style functions, references as values |
| `engine.ts` | Workbook model, dependency index, iterative Tarjan cycle detection, incremental recalculation, spill, names, structural edits by splicing reference text |
| `values.ts`, `dates.ts`, `numfmt.ts`, `criteria.ts` | Coercion and comparison, 1900 date system and text dates, `TEXT()` number formats, COUNTIF-style criteria |
| `fn-*.ts` | The function library: math, aggregate and statistics, text, logic and information, lookup and arrays, dates, financial, distributions, complex numbers, Bessel, database, `CONVERT` |
| `functions.ts` | Registry; the UI function list is generated from it |

Design choices worth knowing:

- **Formula text is kept.** A formula whose references do not change is returned exactly as
  typed; changed references are spliced in canonical form. HyperFormula re-printed every
  formula, which made structural edits rewrite unrelated cells.
- **Recalculation** finds dirty cells through a reference index (single cells in a map,
  ranges bucketed by row block), then evaluates them in dependency order with an explicit
  stack, so a 6,000-cell chain does not recurse. Volatile functions are recalculated on each
  batch. Dynamic references (`OFFSET`, `INDIRECT`) fall back to on-demand evaluation.
- **Dynamic arrays.** Array results spill into free cells; a blocked spill is `#SPILL!`.
  When arrays are present every change recalculates the sheet so results never go stale.
- **No third-party runtime dependency.**

## Evidence

| Check | Result |
| --- | --- |
| Excel corpus: 2,020 committed formula cases evaluated by Microsoft Excel 16.0 (en-US), compared with the engine, in `formula-engine.test.ts` | all pass (numeric tolerance 1e-9; 1e-6 for Bessel and `XIRR`) |
| Full scratch corpus incl. functions this Excel build lacks, 2,055 cases | 2,005 pass; 27 are functions Excel's build does not have or HyperFormula-only names (no Excel truth), 23 are listed under "Excel quirks" below |
| Parity workbook written by Excel with 243 formulas (names, lookups, `SUMPRODUCT`, `DATEDIF`, `TEXT`, financial, statistics, cross-sheet, running totals) recalculated and compared with Excel's cached values | 243 / 243 match (0% mismatch) |
| Example workbooks `static/examples/*.xlsx` | 6 / 6 formulas match their cached values |
| Structural edits: random workbooks (3 sheets, names, absolute/relative/cross-sheet references) with random insert/delete row/column, rename, remove sheet, sort, copy/paste, compared with HyperFormula on formulas, names and values | 8,358 operations over six random seeds: no difference in any formula, name or value |
| Incremental recalculation equals a fresh build after random edits (including spills and cycles) | passes, unit test |
| Fuzzing | 12,000 random formulas and junk strings through the engine and 2,000 through the parser in a unit test: no exceptions, always a value or an error value |
| Unit and workflow tests | `formula-engine.test.ts` mirrors the authenticated Excel flows of `e2e/office.spec.ts`: names, chart data, pivot refresh after an edit, data tools, conditional formatting following formula values, relative fill, text-typed numbers |

### Excel quirks the engine does not copy

- Bessel functions: Excel's results differ from the true values from the ninth digit
  (`BESSELJ(0,0)` is `1.00000000283141`). The engine returns the accurate values. 12 cases.
- Complex numbers: `IMSQRT("-4")` and `IMPOWER("1+i",2)` carry Excel's `1.2251e-16` real part
  where the exact computation gives `1.2246e-16`; two last-digit differences. 4 cases.
- `XIRR`: the engine returns the exact root; Excel stops at its tolerance (difference 5e-9).
- Excel refuses to accept a few literals (`1E308*10`, `5E-324`), prints
  `9.99999999999999E+99` as `1E+100` and answers `TEXT("42","@@")` with one copy. 6 cases.

## Behaviour

### Error values

`#DIV/0!`, `#N/A`, `#NAME?`, `#NUM!`, `#NULL!`, `#REF!`, `#VALUE!`, and the engine values
`#CYCLE!` (circular reference), `#ERROR!` (syntax error or wrong argument count; the message
says which), `#SPILL!` and `#CALC!`. Errors are `FormulaError` objects with `value`
(`'#DIV/0!'`), `type` and `message`, the shape the grid already read. `#LIC!` no longer
exists. Errors propagate left to right as in Excel; `IFERROR`, `IFNA` and `ISERROR` catch them.

### Date system and locale

- 1900 date system with Excel's compatibility quirks: serial 60 is 1900-02-29 and serial 0
  is "January 0, 1900". The 1904 system is not supported.
- Text that looks like a date or time is converted when a function or operator needs a
  number (`="2026-01-15"+1`, `YEAR("15 Jan 2026")`), in the forms Excel accepts. All-numeric
  `a/b/yyyy` is day-first by default (`dateOrder: 'DMY'`, HyperFormula's default and the
  convention of the project's users) and month-first with `dateOrder: 'MDY'`; the Excel
  corpus runs month-first. Month and day names are English.
- Function names are English; decimal point `.`, argument separator `,`, array column
  separator `,` and row separator `;`.
- Cell constants are typed as before: `12`, `50%`, `$5`, `true` and error literals become
  values; `'text` is text; `1,000` and ISO dates stay text.

### Circular references

Cycles are found with an iterative Tarjan search over the dependency graph. Exactly the
cells that are part of a cycle, and everything that depends on them, show `#CYCLE!`, the
same value HyperFormula produced, independent of edit order. `IFERROR` around a cycle
recovers, as before.

### Arrays, intersection and names

- Operators and most functions lift over arrays with Excel broadcasting; `=B2:B4*2` spills.
  `@` forces implicit intersection. HyperFormula answered `#VALUE!` here.
- Names exist at workbook scope and sheet scope (`addNamedExpression(name, expr, sheetId)`,
  sheet scope wins). A name created through the editor must be sheet-qualified and absolute
  (`='Sheet 1'!$B$2:$B$4`), as before. Imported names with relative references are read as
  absolute instead of failing the whole workbook.
- Names are looked up in a `Map` and sheets by a linear scan, so `__proto__`, `constructor`
  and similar are ordinary identifiers.

### Safety bounds

Formula length 8,192, expression nesting 128, name nesting 32, array results at most
1,000,000 cells, text results 32,767 characters, evaluation work 150,000,000 range-cell and
array-cell units per recalculation (then `#ERROR!`, "Calculation budget exceeded"), no
`eval` or `new Function`, no network, no timers.

## Performance and size

Workbook: 99,600 cells on two sheets, 51,600 formulas (sums, averages, `IF`, `COUNTIF`,
`SUMPRODUCT`, `INDEX/MATCH`, `VLOOKUP` over 3,000 rows, a running total chain, 200
`SUMIFS` over 3,000 rows). Best of three in one Node 24 process on a development laptop
under load; other work was running, so read the ratio, not the milliseconds.

| | In-house engine | HyperFormula 3.4 |
| --- | --- | --- |
| Build and calculate | 1.5 to 2.1 s | 7.1 to 7.6 s (3.5 to 5 times slower) |
| Same workbook without the `SUMIFS` block (99,000 cells, isolated run) | 0.8 s | 4.0 s |
| Edit one input cell (recalculate dependents, incl. the 200 `SUMIFS`) | 0.5 to 0.6 s | 1.0 to 2.0 s |
| Edit the head of the 3,000-cell running total | 0.5 to 0.6 s | 1.0 to 1.7 s |
| Batch of 100 edits | 0.5 to 0.65 s | not measured |
| Retained heap, 99,000 cells | 72 MB | 75 MB |

| Bundle contribution (esbuild, minified, browser) | Minified | gzip | brotli |
| --- | --- | --- | --- |
| HyperFormula 3.4 (ES build) | 776 KB | 164 KB | 135 KB |
| In-house engine | 130 KB | 44 KB | 38 KB |
| Change | -646 KB | -120 KB | -97 KB |

## Known differences

### From Excel

- 1904 date system, R1C1 notation, structured table references, external workbook
  references, 3-D references (`Sheet1:Sheet3!A1`), `LET`, `LAMBDA` and the dynamic-array
  helpers beyond `FILTER`, `SORT`, `SORTBY`, `UNIQUE`, `SEQUENCE`, `TAKE`, `DROP`, `VSTACK`,
  `HSTACK`, `CHOOSECOLS`, `CHOOSEROWS`, `TOCOL`, `TOROW`, `EXPAND`, `WRAPROWS`,
  `WRAPCOLS`, `TEXTSPLIT`, `TEXTBEFORE`, `TEXTAFTER`, `XLOOKUP` and `XMATCH` give `#NAME?`.
  Also unsupported: `CELL`, `INFO`, `GETPIVOTDATA`, `WEBSERVICE`, `RTD`, DBCS functions.
- `TEXTBEFORE`, `TEXTAFTER`, `TEXTSPLIT`, `TAKE`, `DROP`, `VSTACK`, `HSTACK`, `CHOOSECOLS`,
  `CHOOSEROWS`, `TOCOL`, `TOROW`, `EXPAND`, `WRAPROWS`, `WRAPCOLS` follow Microsoft's
  documentation; the reference Excel build (16.0.14326) does not have them, so they could not
  be checked against Excel.
- `SUBTOTAL(101..111)` and `AGGREGATE` cannot skip hidden rows (the engine has no row
  visibility); they do skip nested `SUBTOTAL`/`AGGREGATE` results and, for `AGGREGATE`,
  errors.
- `TEXT()` implements sections, conditions, `0 # ?`, thousands and scaling commas, percent,
  `E+`, fractions, `[h]`/`[m]`/`[s]`, `AM/PM`, quoted text, `\` escapes and `[$-409]`;
  colours and `*` fill are ignored.
- Text comparison uses an accent-sensitive, case-insensitive ICU collator; ordering of rare
  punctuation can differ from Excel's collation.
- `CONVERT` covers the common mass, length, time, temperature, volume, area, pressure,
  force, energy, power, speed and information units, not all of Excel's.
- Text functions count UTF-16 code units, like Excel; byte-count variants (`LENB`,
  `LEFTB`) are not provided.

### From HyperFormula (what users may notice)

| Before | Now |
| --- | --- |
| Cells holding an empty string counted as values (`COUNTA`, `ISBLANK`) | Blank is blank, as in Excel |
| `=A4` with `A4` empty displayed the formula text | Shows `0`, as Excel does |
| `TRUE`/`FALSE` needed parentheses | Both forms work |
| `=A1:A4` was `#VALUE!`, `SUM(A1:A4*2)` was `#VALUE!` | Array evaluation; results spill |
| Date-looking text such as `05/01/2026` in a cell was turned into a serial number and displayed as `46027` | Stays text; converted only when a formula needs a number |
| Wrong argument count was `#N/A` | `#ERROR!` with a message |
| Every published number, constants included, was rounded to 11 significant digits: `12345678901234` was shown as `12345678901000`, `=1/3` as `0.33333333333` | Numbers leave the engine at 15 significant digits (what Excel keeps and compares at), so `=0.1+0.2` is `0.3` and long integers stay intact; the grid shows whole numbers in full and fractions the way Excel's General format does (`0.333333333`) |
| Every addition and subtraction was snapped toward zero | Only the final operation of a formula is snapped, as Excel does (`=0.1+0.2-0.3` is 0 but `=(0.1+0.2-0.3)*1` is 5.55E-17) |
| 1,000+ cells with formulas re-printed in canonical case and spacing on structural edits | Original text kept unless a reference changes |
| Relative names failed the whole workbook at build | Treated as absolute |

### Function names dropped

None. All 408 HyperFormula names resolve, including its alias spellings (`COVARIANCEP`,
`COVARIANCES`, `SKEWP`, `WEIBULLDIST`, `TINV2T`, `TDISTRT`, `TDIST2T`, `FINVRT`, `FDISTRT`,
`CHIDISTRT`, `CHIINVRT`, `LOGNORMINV`, `POISSONDIST`), `ARRAYFORMULA`, `SPLIT`,
`INTERVAL`, `VERSION`, `MAXPOOL`, `MEDIANPOOL`, `ISBINARY` and the engineering, complex and
Bessel families. Fifty names are new, among them `CONCAT`, `AVERAGEIFS`, `LOOKUP`,
`INDIRECT`, `RANK`, `MODE`, `PERCENTRANK`, `FORECAST`, `FREQUENCY`, `AGGREGATE`, `CONVERT`,
`SORTBY`, `XMATCH`, `NUMBERVALUE` and the dynamic-array helpers.

## Repeating the evidence

`frontend/researcher/scripts/excel-corpus/` holds the inputs and tools; they need Microsoft
Excel on Windows and are not part of any build.

1. `cases.mjs` is the grid, the names and the 2,055 formulas.
2. `run-excel.ps1` evaluates them through COM (`Formula2`, spill ranges read back).
3. `build-corpus.mjs` writes `src/lib/workspace/formula/fixtures/excel-corpus.json`.
4. `parity-spec.mjs` and `make-parity-xlsx.ps1` rebuild `fixtures/formula-parity.xlsx`.

`pnpm test:unit run src/lib/workspace/formula-engine.test.ts` runs the corpus, the parity
workbook, the structural, graph, fuzz and bounds tests.

## Residual risks

- The corpus was written by the project and checked on one Excel build and locale; formulas
  outside it are covered by the semantics read from Microsoft's documentation, not by
  evidence.
- Spill and dynamic arrays recalculate the whole sheet on each change; a workbook with many
  array formulas and 100,000 formulas will feel it.
- The engine keeps `ExcelJS` conversions of cached results out of scope: exports still set
  `fullCalcOnLoad`, so Excel recomputes on open.
