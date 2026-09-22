import { DOMParser, type Element } from '@xmldom/xmldom';
import { strToU8, unzipSync, zipSync } from 'fflate';
import { xmlText } from './bounded-zip';
import { chartData, sheetCharts, type SheetChart } from './sheet-analysis';
import { columnName } from './sheet-model';
import { parseAddress, rangeAddress } from './sheet-operations';
import type { WorkbookName } from './workbook-names';
const chartNs = 'http://schemas.openxmlformats.org/drawingml/2006/chart';
const drawingNs = 'http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing';
const mainNs = 'http://schemas.openxmlformats.org/drawingml/2006/main';
const relNs = 'http://schemas.openxmlformats.org/package/2006/relationships';
const officeRel = 'http://schemas.openxmlformats.org/officeDocument/2006/relationships';
const esc = (v: unknown) =>
  String(v ?? '').replace(
    /[&<>"']/g,
    (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&apos;' })[c]!
  );
const xml = (body: string) => '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>' + body;
function chartXml(chart: SheetChart, sheet: string, values: unknown[][]) {
  const data = chartData(chart, values),
    range = parseAddress(chart.range)!;
  const ref = (c: number, first: number, last = first) =>
    `'${sheet.replace(/'/g, "''")}'!$${columnName(c)}$${first + 1}${last === first ? '' : `:$${columnName(c)}$${last + 1}`}`;
  const cache = (items: (string | number)[], numeric = false) =>
    `<c:${numeric ? 'numCache' : 'strCache'}>${numeric ? '<c:formatCode>General</c:formatCode>' : ''}<c:ptCount val="${items.length}"/>${items.map((v, i) => `<c:pt idx="${i}"><c:v>${esc(v)}</c:v></c:pt>`).join('')}</c:${numeric ? 'numCache' : 'strCache'}>`;
  const series = (chart.type === 'pie' ? data.series.slice(0, 1) : data.series)
    .map(
      (s, i) =>
        `<c:ser><c:idx val="${i}"/><c:order val="${i}"/><c:tx><c:strRef><c:f>${esc(ref(range.c1 + i + 1, range.r1))}</c:f>${cache([s.name])}</c:strRef></c:tx><c:cat><c:strRef><c:f>${esc(ref(range.c1, range.r1 + 1, range.r1 + data.labels.length))}</c:f>${cache(data.labels)}</c:strRef></c:cat><c:val><c:numRef><c:f>${esc(ref(range.c1 + i + 1, range.r1 + 1, range.r1 + data.labels.length))}</c:f>${cache(s.values, true)}</c:numRef></c:val></c:ser>`
    )
    .join('');
  const kind = chart.type === 'pie' ? 'pie' : chart.type === 'line' ? 'line' : 'bar';
  const axes =
    kind === 'pie'
      ? ''
      : `<c:catAx><c:axId val="1"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:axPos val="${chart.type === 'bar' ? 'l' : 'b'}"/><c:tickLblPos val="nextTo"/><c:crossAx val="2"/><c:crosses val="autoZero"/><c:auto val="1"/><c:lblAlgn val="ctr"/><c:lblOffset val="100"/></c:catAx><c:valAx><c:axId val="2"/><c:scaling><c:orientation val="minMax"/></c:scaling><c:axPos val="${chart.type === 'bar' ? 'b' : 'l'}"/><c:majorGridlines/><c:numFmt formatCode="General" sourceLinked="1"/><c:tickLblPos val="nextTo"/><c:crossAx val="1"/><c:crosses val="autoZero"/><c:crossBetween val="between"/></c:valAx>`;
  return xml(
    `<c:chartSpace xmlns:c="${chartNs}" xmlns:a="${mainNs}" xmlns:r="${officeRel}"><c:lang val="en-US"/><c:chart><c:title><c:tx><c:rich><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>${esc(chart.title)}</a:t></a:r></a:p></c:rich></c:tx><c:overlay val="0"/></c:title><c:autoTitleDeleted val="0"/><c:plotArea><c:layout/><c:${kind}Chart>${kind === 'bar' ? `<c:barDir val="${chart.type === 'bar' ? 'bar' : 'col'}"/><c:grouping val="clustered"/>` : kind === 'line' ? '<c:grouping val="standard"/>' : ''}<c:varyColors val="${kind === 'pie' ? 1 : 0}"/>${series}${kind === 'pie' ? '' : '<c:axId val="1"/><c:axId val="2"/>'}</c:${kind}Chart>${axes}</c:plotArea><c:legend><c:legendPos val="b"/><c:overlay val="0"/></c:legend><c:plotVisOnly val="1"/><c:dispBlanksAs val="gap"/></c:chart></c:chartSpace>`
  );
}

/** ExcelJS does not write charts. Add standard DrawingML parts with live cell references. */
export function addWorkbookParts(
  buffer: ArrayBuffer,
  sheets: { name: string; charts: SheetChart[]; values: unknown[][] }[],
  names: WorkbookName[]
) {
  const parts = unzipSync(new Uint8Array(buffer));
  let types = xmlText(parts['[Content_Types].xml']);
  let chartIndex = 0;
  sheets.forEach((sheet, index) => {
    if (!sheet.charts.length) return;
    const drawing = index + 1,
      links: string[] = [],
      anchors: string[] = [];
    sheet.charts.forEach((chart, i) => {
      if (!parseAddress(chart.range)) return;
      const n = ++chartIndex;
      parts[`xl/charts/chart${n}.xml`] = strToU8(chartXml(chart, sheet.name, sheet.values));
      links.push(
        `<Relationship Id="rId${i + 1}" Type="${officeRel}/chart" Target="../charts/chart${n}.xml"/>`
      );
      anchors.push(
        `<xdr:absoluteAnchor><xdr:pos x="${Math.round(chart.x * 9525)}" y="${Math.round(chart.y * 9525)}"/><xdr:ext cx="${Math.round(chart.width * 9525)}" cy="${Math.round(chart.height * 9525)}"/><xdr:graphicFrame macro=""><xdr:nvGraphicFramePr><xdr:cNvPr id="${i + 1}" name="${esc(chart.title)}"/><xdr:cNvGraphicFramePr/></xdr:nvGraphicFramePr><xdr:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/></xdr:xfrm><a:graphic><a:graphicData uri="${chartNs}"><c:chart xmlns:c="${chartNs}" xmlns:r="${officeRel}" r:id="rId${i + 1}"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:absoluteAnchor>`
      );
      types = types.replace(
        '</Types>',
        `<Override PartName="/xl/charts/chart${n}.xml" ContentType="application/vnd.openxmlformats-officedocument.drawingml.chart+xml"/></Types>`
      );
    });
    parts[`xl/drawings/drawing${drawing}.xml`] = strToU8(
      xml(`<xdr:wsDr xmlns:xdr="${drawingNs}" xmlns:a="${mainNs}">${anchors.join('')}</xdr:wsDr>`)
    );
    parts[`xl/drawings/_rels/drawing${drawing}.xml.rels`] = strToU8(
      xml(`<Relationships xmlns="${relNs}">${links.join('')}</Relationships>`)
    );
    const path = `xl/worksheets/sheet${index + 1}.xml`,
      rels = `xl/worksheets/_rels/sheet${index + 1}.xml.rels`;
    const relationship = `<Relationship Id="rIdDynodocDrawing" Type="${officeRel}/drawing" Target="../drawings/drawing${drawing}.xml"/>`;
    parts[rels] = strToU8(
      parts[rels]
        ? xmlText(parts[rels]).replace('</Relationships>', relationship + '</Relationships>')
        : xml(`<Relationships xmlns="${relNs}">${relationship}</Relationships>`)
    );
    // drawing follows pageSetup/headerFooter but precedes legacyDrawing (cell notes).
    let content = xmlText(parts[path]);
    const marker = content.includes('<legacyDrawing')
      ? '<legacyDrawing'
      : content.includes('<tableParts')
        ? '<tableParts'
        : '</worksheet>';
    content = content.replace(marker, `<drawing r:id="rIdDynodocDrawing"/>${marker}`);
    parts[path] = strToU8(content);
    types = types.replace(
      '</Types>',
      `<Override PartName="/xl/drawings/drawing${drawing}.xml" ContentType="application/vnd.openxmlformats-officedocument.drawing+xml"/></Types>`
    );
  });
  if (names.length) {
    const path = 'xl/workbook.xml',
      entries = names
        .map(
          (n) =>
            `<definedName name="${esc(n.name)}">${esc(n.expression.replace(/^=/, ''))}</definedName>`
        )
        .join('');
    let content = xmlText(parts[path]);
    content = content.includes('</definedNames>')
      ? content.replace('</definedNames>', entries + '</definedNames>')
      : content.replace('</sheets>', `</sheets><definedNames>${entries}</definedNames>`);
    parts[path] = strToU8(content);
  }
  parts['[Content_Types].xml'] = strToU8(types);
  return zipSync(parts, { level: 6 });
}
const elements = (node: Element, name: string) =>
  Array.from(node.getElementsByTagNameNS('*', name));
const first = (node: Element, name: string) => elements(node, name)[0];
function doc(bytes: Uint8Array | undefined) {
  const source = xmlText(bytes);
  return source ? new DOMParser().parseFromString(source, 'application/xml').documentElement : null;
}
function resolve(base: string, target: string) {
  const stack = target.startsWith('/') ? [] : base.split('/').slice(0, -1);
  for (const part of target.split('/')) {
    if (part === '..') stack.pop();
    else if (part && part !== '.') stack.push(part);
  }
  return stack.join('/');
}
function relations(parts: Map<string, Uint8Array>, path: string) {
  const relPath = path.replace(/([^/]+)$/, '_rels/$1.rels'),
    root = doc(parts.get(relPath));
  return new Map(
    (root ? elements(root, 'Relationship') : [])
      .filter((r) => r.getAttribute('TargetMode') !== 'External')
      .map((r) => [r.getAttribute('Id'), resolve(path, r.getAttribute('Target') || '')])
  );
}
export function readWorkbookNames(parts: Map<string, Uint8Array>): WorkbookName[] {
  const root = doc(parts.get('xl/workbook.xml'));
  return (root ? elements(root, 'definedName') : [])
    .filter((n) => !n.hasAttribute('localSheetId') && !n.getAttribute('name')?.startsWith('_xlnm.'))
    .slice(0, 100)
    .map((n) => ({ name: n.getAttribute('name') || '', expression: '=' + (n.textContent || '') }));
}
/** Import rectangular, same-sheet column/bar/line/pie charts. Unsupported charts stay in the original. */
export function readWorkbookCharts(
  parts: Map<string, Uint8Array>,
  sheetName: string,
  index: number
): SheetChart[] {
  const workbook = doc(parts.get('xl/workbook.xml')),
    wbRels = relations(parts, 'xl/workbook.xml');
  const node = workbook
    ? elements(workbook, 'sheet').find((s) => s.getAttribute('name') === sheetName)
    : null;
  const path = node ? wbRels.get(node.getAttribute('r:id')) : undefined;
  if (!path) return [];
  const root = doc(parts.get(path)),
    sheetRels = relations(parts, path),
    result: SheetChart[] = [];
  for (const drawing of root ? elements(root, 'drawing') : []) {
    const drawingPath = sheetRels.get(drawing.getAttribute('r:id'));
    if (!drawingPath) continue;
    const drawingRoot = doc(parts.get(drawingPath)),
      drawingRels = relations(parts, drawingPath);
    if (!drawingRoot) continue;
    for (const anchor of Array.from(drawingRoot.childNodes).filter(
      (n): n is Element => n.nodeType === 1
    )) {
      const reference = first(anchor, 'chart'),
        chartPath = reference ? drawingRels.get(reference.getAttribute('r:id')) : undefined;
      const chart = chartPath ? doc(parts.get(chartPath)) : null;
      if (!chart) continue;
      const plot = first(chart, 'plotArea');
      if (!plot) continue;
      const type = first(plot, 'pieChart')
        ? 'pie'
        : first(plot, 'lineChart')
          ? 'line'
          : first(plot, 'barChart')
            ? first(plot, 'barDir')?.getAttribute('val') === 'bar'
              ? 'bar'
              : 'column'
            : null;
      if (!type) continue;
      const series = elements(plot, 'ser');
      if (!series.length || series.length > 8) continue;
      const refs = series.flatMap((s) =>
        [first(s, 'cat'), first(s, 'val')].map((n) => (n ? first(n, 'f')?.textContent || '' : ''))
      );
      const ranges = refs.map((f) => {
        const match = /^(?:'((?:[^']|'')+)'|([^!]+))!([A-Z$0-9:]+)$/i.exec(f);
        if (!match || (match[1]?.replace(/''/g, "'") || match[2]) !== sheetName) return null;
        return parseAddress(match[3].replace(/\$/g, ''));
      });
      if (ranges.some((r) => !r)) continue;
      const category = ranges[0]!;
      if (category.r1 < 1 || category.c1 !== category.c2 || category.r2 - category.r1 >= 200)
        continue;
      if (
        ranges.some(
          (r, i) =>
            r!.r1 !== category.r1 ||
            r!.r2 !== category.r2 ||
            r!.c1 !== r!.c2 ||
            r!.c1 !== (i % 2 === 0 ? category.c1 : category.c1 + Math.floor(i / 2) + 1)
        )
      )
        continue;
      const pos = first(anchor, 'pos'),
        extent = first(anchor, 'ext'),
        from = first(anchor, 'from');
      const px = (n: Element | undefined, key: string, fallback: number) =>
        n?.getAttribute(key) ? Number(n.getAttribute(key)) / 9525 : fallback;
      const title = first(chart, 'title');
      result.push({
        id: `chart-${index}-${result.length}`,
        title:
          (title
            ? elements(title, 't')
                .map((t) => t.textContent)
                .join('')
            : '') || 'Chart',
        type,
        range: rangeAddress({ ...category, r1: category.r1 - 1, c2: category.c1 + series.length }),
        x: pos ? px(pos, 'x', 160) : Number((from && first(from, 'col')?.textContent) || 2) * 64,
        y: pos ? px(pos, 'y', 60) : Number((from && first(from, 'row')?.textContent) || 3) * 20,
        width: px(extent, 'cx', 560),
        height: px(extent, 'cy', 320)
      });
    }
  }
  return sheetCharts(result);
}
