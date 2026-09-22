import type { DocxImport } from './docx-import';
import type { ImportedSheet } from './xlsx-import';

/** A separate worker per conversion can be terminated without affecting editing. */
export function convertDocx(buffer: ArrayBuffer, signal?: AbortSignal): Promise<DocxImport> {
  return convertOffice(
    buffer,
    () => new Worker(new URL('./docx-import.worker.ts', import.meta.url), { type: 'module' }),
    signal
  );
}

export function convertWorkbook(
  buffer: ArrayBuffer,
  signal?: AbortSignal
): Promise<ImportedSheet[]> {
  return convertOffice(
    buffer,
    () => new Worker(new URL('./xlsx-import.worker.ts', import.meta.url), { type: 'module' }),
    signal
  );
}

function convertOffice<T>(
  buffer: ArrayBuffer,
  createWorker: () => Worker,
  signal?: AbortSignal
): Promise<T> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(new Error('Import cancelled.'));
      return;
    }
    const worker = createWorker();
    const finish = () => {
      clearTimeout(timer);
      signal?.removeEventListener('abort', abort);
      worker.terminate();
    };
    const abort = () => {
      finish();
      reject(new Error('Import cancelled.'));
    };
    const timer = setTimeout(() => {
      finish();
      reject(
        new Error('This file took too long to convert. Try splitting it into smaller documents.')
      );
    }, 45000);
    signal?.addEventListener('abort', abort, { once: true });
    worker.onmessage = (e: MessageEvent<{ result?: T; error?: string }>) => {
      finish();
      if (e.data.result) resolve(e.data.result);
      else reject(new Error(e.data.error || 'Import failed.'));
    };
    worker.onerror = () => {
      finish();
      reject(new Error('Could not convert this Office file. The original has not been modified.'));
    };
    worker.postMessage(buffer, [buffer]);
  });
}
