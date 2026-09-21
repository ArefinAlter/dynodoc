import type { DocxImport } from './docx-import';

/** A separate worker per conversion can be terminated without affecting editing. */
export function convertDocx(buffer: ArrayBuffer, signal?: AbortSignal): Promise<DocxImport> {
  return new Promise((resolve, reject) => {
    if (signal?.aborted) {
      reject(new Error('Import cancelled.'));
      return;
    }
    const worker = new Worker(new URL('./docx-import.worker.ts', import.meta.url), {
      type: 'module'
    });
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
    worker.onmessage = (e: MessageEvent<{ result?: DocxImport; error?: string }>) => {
      finish();
      if (e.data.result) resolve(e.data.result);
      else reject(new Error(e.data.error || 'Import failed.'));
    };
    worker.onerror = () => {
      finish();
      reject(
        new Error('Could not convert this Word document. The original has not been modified.')
      );
    };
    worker.postMessage(buffer, [buffer]);
  });
}
