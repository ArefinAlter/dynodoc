// The web workspace converts in a worker it starts itself; this code already runs in one, so the
// conversion is a direct call.
import { readDocx } from '../../../../../frontend/researcher/src/lib/workspace/docx-import';
import { readWorkbook } from '../../../../../frontend/researcher/src/lib/workspace/xlsx-import';

export async function convertDocx(buffer: ArrayBuffer) {
  return readDocx(new Uint8Array(buffer));
}

export async function convertWorkbook(buffer: ArrayBuffer) {
  return readWorkbook(buffer);
}
