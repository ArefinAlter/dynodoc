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
  name: string;
  rows: string[][];
  formats?: CellFormat[][];
  heights?: number[];
  hidden?: boolean[];
  layout?: SheetLayout;
}

/** Runs in a terminable worker in the browser; no workbook objects cross threads. */
export async function readWorkbook(data: ArrayBuffer): Promise<ImportedSheet[]> {
  readOfficeZip(new Uint8Array(data), () => true);
  const book = new ExcelJS.Workbook();
  await book.xlsx.load(data);
  if (
    book.worksheets.reduce((total, sheet) => total + sheet.rowCount, 0) > 3000 ||
    book.worksheets.some((sheet) => sheet.columnCount > 100)
  )
    throw new Error('This workspace supports up to 3,000 rows and 100 columns per workbook.');
  return book.worksheets.map((sheet) => {
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
