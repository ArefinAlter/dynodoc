// The Office codec: reads and writes Word, Excel and PowerPoint files for the desktop client.
//
// This is the web workspace's own code (frontend/researcher/src/lib/workspace: the importers, the
// exporters and the push planner), bundled for a dedicated worker, so the desktop app and the
// hosted workspace make the same blocks from the same files and write the same files back. It does
// pure computation: bytes in, operations or bytes out. It has no network (the page's CSP allows
// none) and no access to the app; the page asks it to work and the page talks to the app.
//
//   import   {name, bytes}                      -> {kind, ops, warnings, filename, rootId}
//   export   {kind, state, title, origin?}      -> bytes of a .docx, .xlsx or .pptx
//   plan     {name, bytes, base, title}         -> {origin, plan}: what a saved copy changed
//   origin   {bytes}                            -> the document and revision a copy was exported from
//
// Built by client/build.mjs into ui/office-worker.js (committed, like launcher.js).

import { importFile } from '../../../../frontend/researcher/src/lib/workspace/files';
import { readDocx } from '../../../../frontend/researcher/src/lib/workspace/docx-import';
import { writeDocx } from '../../../../frontend/researcher/src/lib/workspace/docx-export';
import { writeWorkbook } from '../../../../frontend/researcher/src/lib/workspace/xlsx-export';
import { writePptx } from '../../../../frontend/researcher/src/lib/workspace/pptx-export';
import { planPush, readOrigin } from '../../../../frontend/researcher/src/lib/workspace/file-push';
import { planSheetPush, planSlidePush, pushState } from '../../../../frontend/researcher/src/lib/workspace/structured-push';

type Json = Record<string, unknown>;

interface Request {
  id: number;
  op: 'import' | 'export' | 'plan' | 'origin';
  name?: string;
  bytes?: ArrayBuffer;
  kind?: 'document' | 'spreadsheet' | 'presentation';
  state?: Json;
  title?: string;
  origin?: { documentId: string; seq: number };
  base?: Json;
}

const ctx = self as unknown as {
  onmessage: ((e: MessageEvent<Request>) => void) | null;
  postMessage(message: unknown, transfer?: Transferable[]): void;
};

const extensionOf = (name: string) => name.split('.').pop()?.toLowerCase() ?? '';

async function bytesOf(out: unknown): Promise<Uint8Array> {
  if (out instanceof Uint8Array) return out;
  if (out instanceof ArrayBuffer) return new Uint8Array(out);
  if (out && typeof (out as Blob).arrayBuffer === 'function') return new Uint8Array(await (out as Blob).arrayBuffer());
  throw new Error('The exporter returned something this app cannot write.');
}

async function run(r: Request): Promise<{ result?: unknown; bytes?: Uint8Array }> {
  if (r.op === 'import') {
    const name = r.name ?? 'file';
    if (extensionOf(name) === 'doc') throw new Error('Legacy .doc files cannot be read here. Open the file in Word, choose Save As, and pick .docx.');
    const file = new File([r.bytes as ArrayBuffer], name);
    return { result: await importFile(file) };
  }
  if (r.op === 'origin') return { result: readOrigin(new Uint8Array(r.bytes as ArrayBuffer)) };
  if (r.op === 'export') {
    const state = r.state as never;
    const title = r.title ?? 'Document';
    if (r.kind === 'spreadsheet') return { bytes: await bytesOf(await writeWorkbook(state, r.origin)) };
    if (r.kind === 'presentation') return { bytes: await bytesOf(await writePptx(state, title, r.origin)) };
    return { bytes: await bytesOf(await writeDocx(state, title, r.origin)) };
  }
  // plan: what a saved copy of a file changed, against the revision it started from
  const base = r.base as never;
  const bytes = new Uint8Array(r.bytes as ArrayBuffer);
  const ext = extensionOf(r.name ?? '');
  let plan;
  if (ext === 'docx') {
    const incoming = readDocx(bytes);
    const exported = await bytesOf(await writeDocx(base, r.title ?? 'Document'));
    plan = planPush(base, incoming, readDocx(exported));
  } else if (ext === 'xlsx' || ext === 'pptx') {
    const imported = await importFile(new File([r.bytes as ArrayBuffer], r.name ?? 'file'));
    const incoming = pushState(imported, ext === 'xlsx' ? 'spreadsheet' : 'presentation');
    plan = ext === 'xlsx' ? planSheetPush(base, incoming) : planSlidePush(base, incoming);
  } else throw new Error('Choose a Word, Excel or PowerPoint file.');
  const { state: _state, ...light } = plan as unknown as Json & { state?: unknown };
  return { result: { origin: readOrigin(bytes), plan: light } };
}

ctx.onmessage = (e) => {
  const r = e.data;
  run(r).then(
    (out) => {
      if (out.bytes) {
        const buffer = out.bytes.buffer.slice(out.bytes.byteOffset, out.bytes.byteOffset + out.bytes.byteLength) as ArrayBuffer;
        ctx.postMessage({ id: r.id, ok: true, bytes: buffer }, [buffer]);
      } else ctx.postMessage({ id: r.id, ok: true, result: out.result });
    },
    (error: unknown) => ctx.postMessage({ id: r.id, ok: false, error: error instanceof Error ? error.message : String(error) }),
  );
};
