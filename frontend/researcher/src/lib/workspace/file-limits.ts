/** Temporary tester limit, measured on original file bytes (decimal MB). */
export const MAX_FILE_BYTES = 3_000_000;
export const FILE_LIMIT_MESSAGE = 'Files are limited to 3 MB (3,000,000 bytes) during testing.';
export function checkFileSize(size: number) {
  if (!Number.isSafeInteger(size) || size < 0 || size > MAX_FILE_BYTES)
    throw new Error(FILE_LIMIT_MESSAGE);
}
