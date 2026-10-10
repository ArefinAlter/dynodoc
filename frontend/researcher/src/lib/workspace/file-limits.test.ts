import { expect, it } from 'vitest';
import { checkFileSize, MAX_FILE_BYTES } from './file-limits';
import { importFile } from './files';
import { preparePush } from './reviews';

it('allows exactly three million bytes and rejects oversize or invalid sizes', () => {
  expect(() => checkFileSize(MAX_FILE_BYTES)).not.toThrow();
  for (const size of [MAX_FILE_BYTES + 1, -1, NaN, Infinity]) {
    expect(() => checkFileSize(size)).toThrow('3 MB');
  }
});

it('rejects oversize originals before import or push parsing', async () => {
  const file = new File([new Uint8Array(MAX_FILE_BYTES + 1)], 'large.docx');
  await expect(importFile(file)).rejects.toThrow('3 MB');
  for (const kind of ['document', 'spreadsheet', 'presentation'] as const) {
    await expect(preparePush('unused', 'Large file', file, kind)).rejects.toThrow('3 MB');
  }
});
