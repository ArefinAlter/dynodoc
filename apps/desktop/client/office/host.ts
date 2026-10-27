// The page's side of the Office codec: starts a worker for each job, with a time limit, and gives
// the client one small object to call. It lives in its own script (office-host.js) because the
// launcher's policy keeps cross-context messaging out of launcher.js; the worker has no network
// and no access to the app, and its answer is plain data.

interface Job {
  op: 'import' | 'export' | 'plan' | 'origin';
  [key: string]: unknown;
}

const LIMIT_MS = 90_000;
let counter = 0;

function work(job: Job, bytes?: ArrayBuffer): Promise<{ result?: unknown; bytes?: Uint8Array }> {
  return new Promise((resolve, reject) => {
    const id = ++counter;
    const worker = new Worker('office-worker.js');
    const finish = () => {
      clearTimeout(timer);
      worker.terminate();
    };
    const timer = setTimeout(() => {
      finish();
      reject(new Error('This file took too long to convert. Try a smaller file.'));
    }, LIMIT_MS);
    worker.onmessage = (e: MessageEvent<{ id: number; ok: boolean; result?: unknown; bytes?: ArrayBuffer; error?: string }>) => {
      finish();
      if (!e.data.ok) reject(new Error(e.data.error || 'The file could not be converted.'));
      else resolve({ result: e.data.result, bytes: e.data.bytes ? new Uint8Array(e.data.bytes) : undefined });
    };
    worker.onerror = () => {
      finish();
      reject(new Error('This file could not be converted. The original has not been changed.'));
    };
    worker.postMessage({ ...job, id, bytes }, bytes ? [bytes] : []);
  });
}

const copy = (b: Uint8Array): ArrayBuffer => b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength) as ArrayBuffer;

(window as unknown as { dynodocOffice: unknown }).dynodocOffice = {
  /** A file's content as the operations that make it a document in the engine. */
  importFile: async (bytes: Uint8Array, name: string) => (await work({ op: 'import', name }, copy(bytes))).result,
  /** The bytes of a .docx, .xlsx or .pptx made from a document's state. */
  exportFile: async (kind: string, state: unknown, title: string, origin?: { documentId: string; seq: number }) =>
    (await work({ op: 'export', kind, state, title, origin })).bytes as Uint8Array,
  /** What a saved copy changed, against the state the copy started from. */
  planPush: async (bytes: Uint8Array, name: string, base: unknown, title: string) =>
    (await work({ op: 'plan', name, base, title }, copy(bytes))).result,
  /** The document and revision a file was exported from, if it says so. */
  originOf: async (bytes: Uint8Array) => (await work({ op: 'origin' }, copy(bytes))).result,
};
