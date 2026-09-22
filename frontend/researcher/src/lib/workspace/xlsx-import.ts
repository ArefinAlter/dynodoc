import { readWorkbookCharts, readWorkbookNames } from './xlsx-charts';
import type { SheetChart } from './sheet-analysis';
import type { WorkbookName } from './workbook-names';
import { pageSetup, type PageSetup } from './office';
import ExcelJS from 'exceljs';
import { readOfficeZip } from './bounded-zip';
import {
  readCell,
  readSheetLayout,
  readConditionalFormats,
  type CellFormat,
  type SheetLayout
} from './sheet-model';

export interface ImportedSheet {
  charts?: SheetChart[];
  names?: WorkbookName[];
  page?: PageSetup;
  printArea?: string;
  protected?: boolean;
  name: string;
  rows: string[][];
  formats?: CellFormat[][];
  heights?: number[];
  hidden?: boolean[];
  layout?: SheetLayout;
}

/** Runs in a terminable worker in the browser; no workbook objects cross threads. */
export async function readWorkbook(data: ArrayBuffer): Promise<ImportedSheet[]> {
  const parts = readOfficeZip(new Uint8Array(data), () => true);
  const book = new ExcelJS.Workbook();
  await book.xlsx.load(data);
  if (
    book.worksheets.reduce((total, sheet) => total + sheet.rowCount, 0) > 3000 ||
    book.worksheets.some((sheet) => sheet.columnCount > 100)
  )
    throw new Error('This workspace supports up to 3,000 rows and 100 columns per workbook.');
  return book.worksheets.map((sheet, index) => {
    const rows: string[][] = [],
      formats: CellFormat[][] = [],
      heights: number[] = [],
      hidden: boolean[] = [];
    // Iterate explicit row numbers: eachRow omits trailing empty rows even with
    // includeEmpty, which otherwise shifts saved merge anchors and row metadata.
    for (let r = 1; r <= sheet.rowCount; r++) {
      const row = sheet.getRow(r),
        values: string[] = [],
        styles: CellFormat[] = [];
      for (let c = 1; c <= sheet.columnCount; c++) {
        const cell = row.getCell(c),
          parsed = readCell(cell);
        values.push(cell.isMerged && cell.master.address !== cell.address ? '' : parsed.value);
        styles.push(parsed.format);
      }
      rows.push(values);
      formats.push(styles);
      heights.push(row.height ? (row.height * 4) / 3 : 28);
      hidden.push(row.hidden || false);
    }
    readConditionalFormats(sheet, formats);
    return {
      name: sheet.name,
      charts: readWorkbookCharts(parts, sheet.name, index),
      names: index === 0 ? readWorkbookNames(parts) : undefined,
      page: pageSetup({
        size: !sheet.pageSetup.paperSize
          ? 'Letter'
          : sheet.pageSetup.paperSize === 5
            ? 'Legal'
            : 'A4',
        landscape: sheet.pageSetup.orientation === 'landscape',
        margin: (sheet.pageSetup.margins?.top || 0.75) * 25.4,
        header: sheet.headerFooter?.oddHeader || '',
        footer: (sheet.headerFooter?.oddFooter || '').replace(/&P\s*\/\s*&N/g, ''),
        pageNumbers: (sheet.headerFooter?.oddFooter || '').includes('&P')
      }),
      printArea: sheet.pageSetup.printArea,
      protected: !!(sheet.model as unknown as { sheetProtection?: { sheet?: boolean } })
        .sheetProtection?.sheet,
      rows,
      formats,
      heights,
      hidden,
      layout: readSheetLayout(
        sheet,
        rows.map((_, i) => String(i))
      )
    };
  });
}
