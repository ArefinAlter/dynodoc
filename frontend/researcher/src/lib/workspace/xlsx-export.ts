import { byPosition } from './api';
import type { DocumentState } from '$lib/api/types';
import { rowsForSheet } from './files';
import { writeCell, sheetLayout, cellFormat, workbookInput } from './sheet-model';
import { sheetCharts, type SheetChart } from './sheet-analysis';
import { workbookNames } from './workbook-names';
import { WorkbookCalculator } from './workbook-calculator';
import { pageSetup } from './office';
import { parseAddress } from './sheet-operations';
import { addWorkbookParts } from './xlsx-charts';
export async function writeWorkbook(state: DocumentState) {
  const { default: ExcelJS } = await import('exceljs');
  const book = new ExcelJS.Workbook();
  book.calcProperties.fullCalcOnLoad = true;
  const sheets = Object.values(state.nodes)
    .filter((n) => !n.deleted && n.current_fields.kind === 'sheet')
    .sort((a, b) => byPosition(a, b));
  const calculator = new WorkbookCalculator();
  const computed = calculator.calculate(workbookInput(state), workbookNames(state));
  calculator.destroy();
  const chartSheets: { name: string; charts: SheetChart[]; values: unknown[][] }[] = [];
  for (const [i, s] of sheets.entries()) {
    const label = String(s.current_fields.label || `Sheet ${i + 1}`)
      .replace(/[\\/*?:[\]]/g, '')
      .slice(0, 31);
    let worksheet;
    try {
      worksheet = book.addWorksheet(label);
    } catch {
      worksheet = book.addWorksheet(`Sheet ${i + 1}`);
    }
    const rows = rowsForSheet(state, s.id),
      layout = sheetLayout(s.current_fields.sheet_layout);
    rows.forEach((row, r) => {
      const count = Math.min(100, Number(s.current_fields.columns) || 12);
      for (let c = 0; c < count; c++)
        writeCell(
          worksheet.getCell(r + 1, c + 1),
          String(row.current_fields[`cell_${c}`] ?? ''),
          cellFormat(row, c)
        );
      if (row.current_fields.row_height)
        worksheet.getRow(r + 1).height = (Number(row.current_fields.row_height) * 3) / 4;
      worksheet.getRow(r + 1).hidden = row.current_fields.hidden === true;
    });
    for (let c = 0; c < (Number(s.current_fields.columns) || 12); c++) {
      worksheet.getColumn(c + 1).width = ((layout.widths[String(c)] || 64) - 5) / 7;
      worksheet.getColumn(c + 1).hidden = layout.hiddenColumns.includes(c);
    }
    if (layout.frozenRows || layout.frozenColumns)
      worksheet.views = [
        { state: 'frozen', xSplit: layout.frozenColumns, ySplit: layout.frozenRows }
      ];
    for (const merge of layout.merges) {
      const from = rows.findIndex((r) => r.id === merge.fromRow),
        to = rows.findIndex((r) => r.id === merge.toRow);
      if (from >= 0 && to >= from)
        worksheet.mergeCells(from + 1, merge.fromCol + 1, to + 1, merge.toCol + 1);
    }
    const area = String(s.current_fields.print_area || '');
    const setup = pageSetup(s.current_fields.page_setup);
    worksheet.pageSetup = {
      paperSize: setup.size === 'Letter' ? undefined : setup.size === 'Legal' ? 5 : 9,
      orientation: setup.landscape ? 'landscape' : 'portrait',
      fitToPage: true,
      fitToWidth: 1,
      fitToHeight: 0,
      printArea: area || undefined,
      margins: {
        top: setup.margin / 25.4,
        bottom: setup.margin / 25.4,
        left: setup.margin / 25.4,
        right: setup.margin / 25.4,
        header: 0.25,
        footer: 0.25
      }
    };
    worksheet.headerFooter = {
      oddHeader: setup.header,
      oddFooter: setup.footer + (setup.pageNumbers ? ' &P / &N' : '')
    };
    if (s.current_fields.sheet_protected === true)
      await worksheet.protect('', {
        selectLockedCells: true,
        selectUnlockedCells: true,
        spinCount: 0
      });
    const table = parseAddress(String(s.current_fields.table_range || ''));
    if (table)
      worksheet.autoFilter = {
        from: { row: table.r1 + 1, column: table.c1 + 1 },
        to: { row: table.r2 + 1, column: table.c2 + 1 }
      };
    chartSheets.push({
      name: worksheet.name,
      charts: sheetCharts(s.current_fields.charts),
      values: computed[String(s.current_fields.label || s.id)] || []
    });
  }
  const bytes = addWorkbookParts(
    (await book.xlsx.writeBuffer()) as ArrayBuffer,
    chartSheets,
    workbookNames(state)
  );
  return bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
}
