// Builds the spec of the parity workbook (src/lib/workspace/formula/fixtures/formula-parity.xlsx):
// realistic sheets whose cached values are produced by Excel.
//   node scripts/excel-corpus/parity-spec.mjs spec.json
//   powershell -File scripts/excel-corpus/make-parity-xlsx.ps1 -SpecFile spec.json -Out formula-parity.xlsx
import fs from 'node:fs';

const sheets = [];
const names = [];
const add = (name) => {
  const s = { name, cells: [] };
  sheets.push(s);
  const put = (a1, v) => s.cells.push({ a: a1, v });
  const f = (a1, formula) => s.cells.push({ a: a1, f: formula });
  return { s, put, f };
};
const col = (c) => String.fromCharCode(65 + c);

// ---------------------------------------------------------------- Budget
{
  const { put, f } = add('Budget');
  const months = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun'];
  put('A1', 'Category');
  months.forEach((m, i) => put(`${col(i + 1)}1`, m));
  put('H1', 'Total');
  put('I1', 'Share');
  put('J1', 'Status');
  put('K1', 'Average');
  const cats = [
    'Rent',
    'Salaries',
    'Travel',
    'Software',
    'Marketing',
    'Supplies',
    'Training',
    'Legal'
  ];
  const base = [1200, 8400, 650, 420, 900, 180, 300, 250];
  cats.forEach((cat, r) => {
    const row = r + 2;
    put(`A${row}`, cat);
    months.forEach((_, c) =>
      put(
        `${col(c + 1)}${row}`,
        Math.round(base[r] * (1 + ((c * 7 + r * 3) % 11) / 40) * 100) / 100
      )
    );
    f(`H${row}`, `=SUM(B${row}:G${row})`);
    f(`I${row}`, `=H${row}/$H$10`);
    f(`J${row}`, `=IF(H${row}>5000,"High",IF(H${row}>1000,"Medium","Low"))`);
    f(`K${row}`, `=ROUND(AVERAGE(B${row}:G${row}),2)`);
  });
  put('A10', 'Total');
  for (let c = 1; c <= 7; c++) f(`${col(c)}10`, `=SUM(${col(c)}2:${col(c)}9)`);
  f('I10', '=SUM(I2:I9)');
  f('K10', '=AVERAGE(K2:K9)');
  put('A12', 'Largest category');
  f('B12', '=INDEX(A2:A9,MATCH(MAX(H2:H9),H2:H9,0))');
  put('A13', 'Smallest category');
  f('B13', '=INDEX(A2:A9,MATCH(MIN(H2:H9),H2:H9,0))');
  put('A14', 'Categories above average');
  f('B14', '=COUNTIF(H2:H9,">"&AVERAGE(H2:H9))');
  put('A15', 'High-cost total');
  f('B15', '=SUMIF(J2:J9,"High",H2:H9)');
  put('A16', 'Months over 12000');
  f('B16', '=SUMPRODUCT(--(B10:G10>12000))');
  put('A17', 'Growth Jan to Jun');
  f('B17', '=(G10-B10)/B10');
  put('A18', 'Summary');
  f('B18', '="Total: "&TEXT(H10,"#,##0.00")&" across "&COUNTA(A2:A9)&" categories"');
  put('A19', 'Quarter 1');
  f('B19', '=SUM(B10:D10)');
  put('A20', 'Quarter 2');
  f('B20', '=SUM(E10:G10)');
  put('A21', 'Change');
  f('B21', '=IFERROR(B20/B19-1,0)');
  names.push({ name: 'GrandTotal', ref: '=Budget!$H$10' });
  names.push({ name: 'MonthlyTotals', ref: '=Budget!$B$10:$G$10' });
  put('A22', 'Named total');
  f('B22', '=GrandTotal');
  put('A23', 'Peak month total');
  f('B23', '=MAX(MonthlyTotals)');
  put('A24', 'Months');
  f('B24', '=COUNT(MonthlyTotals)');
}

// ---------------------------------------------------------------- Rates
{
  const { put } = add('Rates');
  put('A1', 'Grade');
  put('B1', 'Base');
  put('C1', 'Bonus rate');
  [
    ['A', 40000, 0.05],
    ['B', 52000, 0.08],
    ['C', 64000, 0.1],
    ['D', 80000, 0.12],
    ['E', 99000, 0.15]
  ].forEach((r, i) => r.forEach((v, c) => put(`${col(c)}${i + 2}`, v)));
  names.push({ name: 'TaxRate', ref: '=Rates!$F$1' });
  put('E1', 'Tax');
  put('F1', 0.2);
}

// ---------------------------------------------------------------- Staff
{
  const { put, f } = add('Staff');
  [
    'Name',
    'Dept',
    'Grade',
    'Hired',
    'Base',
    'Bonus',
    'Gross',
    'Net',
    'Tenure',
    'Initials',
    'Code'
  ].forEach((h, c) => put(`${col(c)}1`, h));
  const people = [
    ['Maya Chen', 'Ops', 'B', 43831],
    ['Sam Okoye', 'Finance', 'C', 44197],
    ['Priya Nair', 'Ops', 'A', 44562],
    ['Luca Rossi', 'Sales', 'D', 43101],
    ['Ana Costa', 'Finance', 'B', 44927],
    ['Jonas Weber', 'Sales', 'E', 42370],
    ['Mei Tanaka', 'Ops', 'C', 44287],
    ['Omar Haddad', 'Sales', 'B', 45292],
    ['Sofia Petrov', 'Finance', 'D', 43466],
    ['Noah Kim', 'Ops', 'A', 45658]
  ];
  people.forEach((p, i) => {
    const r = i + 2;
    put(`A${r}`, p[0]);
    put(`B${r}`, p[1]);
    put(`C${r}`, p[2]);
    put(`D${r}`, p[3]);
    f(`E${r}`, `=VLOOKUP(C${r},Rates!$A$2:$C$6,2,FALSE)`);
    f(`F${r}`, `=E${r}*VLOOKUP(C${r},Rates!$A$2:$C$6,3,FALSE)`);
    f(`G${r}`, `=E${r}+F${r}`);
    f(`H${r}`, `=ROUND(G${r}*(1-TaxRate),0)`);
    f(`I${r}`, `=DATEDIF(D${r},DATE(2026,1,1),"Y")`);
    f(`J${r}`, `=LEFT(A${r},1)&MID(A${r},FIND(" ",A${r})+1,1)`);
    f(`K${r}`, `=UPPER(LEFT(B${r},3))&"-"&TEXT(ROW()-1,"000")`);
  });
  put('A13', 'Headcount');
  f('B13', '=COUNTA(A2:A11)');
  put('A14', 'Payroll');
  f('B14', '=SUM(G2:G11)');
  put('A15', 'Average net');
  f('B15', '=AVERAGE(H2:H11)');
  put('A16', 'Ops payroll');
  f('B16', '=SUMIF(B2:B11,"Ops",G2:G11)');
  put('A17', 'Sales headcount');
  f('B17', '=COUNTIF(B2:B11,"Sales")');
  put('A18', 'Finance average gross');
  f('B18', '=AVERAGEIF(B2:B11,"Finance",G2:G11)');
  put('A19', 'Senior ops');
  f('B19', '=COUNTIFS(B2:B11,"Ops",I2:I11,">=3")');
  put('A20', 'Highest paid');
  f('B20', '=INDEX(A2:A11,MATCH(MAX(G2:G11),G2:G11,0))');
  put('A21', 'Earliest hire');
  f('B21', '=TEXT(MIN(D2:D11),"yyyy-mm-dd")');
  put('A22', 'Latest hire');
  f('B22', '=TEXT(MAX(D2:D11),"d mmm yyyy")');
  put('A23', 'Median gross');
  f('B23', '=MEDIAN(G2:G11)');
  put('A24', 'Top 3 sum');
  f('B24', '=LARGE(G2:G11,1)+LARGE(G2:G11,2)+LARGE(G2:G11,3)');
  put('A25', 'Rank of Maya');
  f('B25', '=RANK(G2,G2:G11)');
  put('A26', 'Net above 60k');
  f('B26', '=SUMPRODUCT((H2:H11>60000)*H2:H11)');
  put('A27', 'Distinct depts');
  f('B27', '=SUMPRODUCT(1/COUNTIF(B2:B11,B2:B11))');
  put('A28', 'Full report');
  f('B28', '=B20&" earns "&TEXT(MAX(G2:G11),"$#,##0")');
  put('A29', 'Weekday of first hire');
  f(
    'B29',
    '=TEXT(WEEKDAY(D2),"0")&"/"&CHOOSE(WEEKDAY(D2,2),"Mon","Tue","Wed","Thu","Fri","Sat","Sun")'
  );
  put('A30', 'Months since first hire');
  f('B30', '=DATEDIF(MIN(D2:D11),DATE(2026,1,1),"M")');
  put('A31', 'End of month');
  f('B31', '=EOMONTH(D2,0)-D2');
  put('A32', 'Working days');
  f('B32', '=NETWORKDAYS(D2,D2+30)');
  put('A33', 'Sorted lookup');
  f('B33', '=LOOKUP(60000,Rates!B2:B6,Rates!A2:A6)');
}

// ---------------------------------------------------------------- Loan
{
  const { put, f } = add('Loan');
  put('A1', 'Principal');
  put('B1', 250000);
  put('A2', 'Annual rate');
  put('B2', 0.045);
  put('A3', 'Years');
  put('B3', 15);
  put('A4', 'Monthly payment');
  f('B4', '=-PMT(B2/12,B3*12,B1)');
  put('A5', 'Total paid');
  f('B5', '=B4*B3*12');
  put('A6', 'Total interest');
  f('B6', '=B5-B1');
  put('A7', 'First interest');
  f('B7', '=-IPMT(B2/12,1,B3*12,B1)');
  put('A8', 'First principal');
  f('B8', '=-PPMT(B2/12,1,B3*12,B1)');
  put('A9', 'Future value of savings');
  f('B9', '=FV(0.03/12,120,-200)');
  put('A10', 'Present value');
  f('B10', '=PV(0.05,10,-1000)');
  put('A11', 'Periods to payoff');
  f('B11', '=NPER(B2/12,-1500,B1)');
  put('A12', 'NPV');
  f('B12', '=NPV(0.08,B15:B19)');
  put('A14', 'Cash flows');
  [-5000, 1200, 1500, 1800, 2100].forEach((v, i) => put(`B${15 + i}`, v));
  put('A20', 'IRR');
  f('B20', '=IRR(B15:B19)');
  put('A21', 'Straight-line depreciation');
  f('B21', '=SLN(10000,1000,5)');
  put('A22', 'Effective rate');
  f('B22', '=EFFECT(0.06,12)');
  // amortisation table with relative references
  put('D1', 'Month');
  put('E1', 'Opening');
  put('F1', 'Interest');
  put('G1', 'Principal');
  put('H1', 'Closing');
  for (let m = 1; m <= 12; m++) {
    const r = m + 1;
    put(`D${r}`, m);
    if (m === 1) f(`E${r}`, '=$B$1');
    else f(`E${r}`, `=H${r - 1}`);
    f(`F${r}`, `=E${r}*$B$2/12`);
    f(`G${r}`, `=$B$4-F${r}`);
    f(`H${r}`, `=E${r}-G${r}`);
  }
  put('D14', 'Year-end balance');
  f('E14', '=H13');
  put('D15', 'Interest in year 1');
  f('E15', '=SUM(F2:F13)');
}

// ---------------------------------------------------------------- Stats
{
  const { put, f } = add('Stats');
  put('A1', 'x');
  put('B1', 'y');
  put('C1', 'group');
  const xs = [2, 4, 5, 7, 9, 10, 12, 13, 15, 18, 20, 21];
  xs.forEach((x, i) => {
    const r = i + 2;
    put(`A${r}`, x);
    put(`B${r}`, Math.round((3 + 1.7 * x + ((i * 37) % 9) - 4) * 10) / 10);
    put(`C${r}`, ['a', 'b', 'c'][i % 3]);
  });
  const stats = [
    ['Mean x', '=AVERAGE(A2:A13)'],
    ['Median x', '=MEDIAN(A2:A13)'],
    ['Stdev x', '=STDEV(A2:A13)'],
    ['Variance x', '=VAR(A2:A13)'],
    ['Pop stdev', '=STDEVP(A2:A13)'],
    ['Min', '=MIN(B2:B13)'],
    ['Max', '=MAX(B2:B13)'],
    ['Range', '=MAX(B2:B13)-MIN(B2:B13)'],
    ['Correlation', '=CORREL(A2:A13,B2:B13)'],
    ['Slope', '=SLOPE(B2:B13,A2:A13)'],
    ['Intercept', '=INTERCEPT(B2:B13,A2:A13)'],
    ['R squared', '=RSQ(B2:B13,A2:A13)'],
    ['Forecast at 25', '=FORECAST(25,B2:B13,A2:A13)'],
    ['90th percentile', '=PERCENTILE(B2:B13,0.9)'],
    ['Quartile 1', '=QUARTILE(B2:B13,1)'],
    ['Quartile 3', '=QUARTILE(B2:B13,3)'],
    ['Count above mean', '=COUNTIF(B2:B13,">"&AVERAGE(B2:B13))'],
    ['Sum group a', '=SUMIF(C2:C13,"a",B2:B13)'],
    ['Average group b', '=AVERAGEIF(C2:C13,"b",B2:B13)'],
    ['Count group c', '=COUNTIF(C2:C13,"c")'],
    ['Skew', '=SKEW(B2:B13)'],
    ['Kurtosis', '=KURT(B2:B13)'],
    ['Geometric mean', '=GEOMEAN(A2:A13)'],
    ['Mode-like', '=SMALL(A2:A13,1)'],
    ['Z score of 10', '=STANDARDIZE(10,AVERAGE(A2:A13),STDEV(A2:A13))'],
    ['Normal cdf', '=NORM.DIST(10,AVERAGE(A2:A13),STDEV(A2:A13),TRUE)'],
    ['Std error', '=STDEV(B2:B13)/SQRT(COUNT(B2:B13))'],
    ['Rounded mean', '=ROUND(AVERAGE(B2:B13),1)'],
    ['Ceiling', '=CEILING(AVERAGE(B2:B13),5)'],
    ['Floor', '=FLOOR(AVERAGE(B2:B13),5)'],
    ['Integer part', '=INT(AVERAGE(B2:B13))'],
    ['Sign', '=SIGN(SLOPE(B2:B13,A2:A13))'],
    ['Abs dev', '=AVEDEV(B2:B13)'],
    ['Dev sq', '=DEVSQ(B2:B13)'],
    ['Harmonic', '=HARMEAN(A2:A13)'],
    ['Trim mean', '=TRIMMEAN(B2:B13,0.2)']
  ];
  stats.forEach(([label, formula], i) => {
    put(`E${i + 1}`, label);
    f(`F${i + 1}`, formula);
  });
}

fs.writeFileSync(process.argv[2] ?? 'parity-spec.json', JSON.stringify({ sheets, names }, null, 1));
console.log(
  'cells',
  sheets.reduce((n, s) => n + s.cells.length, 0),
  'formulas',
  sheets.reduce((n, s) => n + s.cells.filter((c) => c.f).length, 0)
);
