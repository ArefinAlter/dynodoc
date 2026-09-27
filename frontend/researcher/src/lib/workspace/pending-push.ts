/**
 * A file chosen on the workspace home that belongs to an existing document. It is
 * handed to that document's editor in memory, so the upload dialog opens pre-filled.
 */
let pending: { documentId: string; file: File } | null = null;
export function queuePush(documentId: string, file: File) {
  pending = { documentId, file };
}
export function takePush(documentId: string): File | null {
  if (pending?.documentId !== documentId) return null;
  const file = pending.file;
  pending = null;
  return file;
}
