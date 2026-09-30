/**
 * Before an uploaded file becomes a new document, compare its text with documents
 * the uploader can already open. The server fingerprints the text the same way it
 * fingerprints stored documents (engine_core::similarity), so a renamed or edited
 * copy is recognised by content rather than by its file name.
 */
import type { EventPayload } from '$lib/api/types';
import { checkSimilarity, type Relation, type SimilarDocument } from './collaboration';
import { textOf, type TextBlock } from './files';

/** The comparable text of each block an import would create, as the server reads it. */
export function blockTexts(ops: EventPayload[]): string[] {
  const out: string[] = [];
  for (const op of ops) {
    if (op.type !== 'NodeCreated' || !op.parent_id) continue;
    const fields = (op.fields || {}) as Record<string, unknown>;
    const label =
      typeof fields.label === 'string' && fields.label.trim()
        ? fields.label
        : fields.content && typeof fields.content === 'object'
          ? textOf(fields.content as TextBlock)
          : '';
    if (label.trim()) out.push(label);
    const cells = Object.entries(fields)
      .filter(([key, value]) => /^cell_\d+$/.test(key) && value !== null && value !== '')
      .sort(([a], [b]) => Number(a.slice(5)) - Number(b.slice(5)))
      .map(([, value]) => String(value));
    if (cells.length) out.push(cells.join(' | '));
    if (typeof fields.notes === 'string' && fields.notes.trim()) out.push(fields.notes);
  }
  return out;
}

const COPIES: Relation[] = ['identical', 'copy', 'edited_copy', 'excerpt', 'extended'];
/** A match strong enough to suggest sending the file as changes. */
export function isCopy(match: SimilarDocument) {
  return match.embedded_id || COPIES.includes(match.comparison.relation);
}
export function isSameName(match: SimilarDocument) {
  return !isCopy(match) && match.comparison.title_match;
}

/** Documents the uploader can open that resemble the file, strongest first. */
export async function similarDocuments(
  file: File,
  ops: EventPayload[]
): Promise<SimilarDocument[]> {
  let origin: string | undefined;
  if (/\.(docx|xlsx|pptx)$/i.test(file.name)) {
    const { readOrigin } = await import('./file-push');
    origin = readOrigin(new Uint8Array(await file.arrayBuffer())).documentId;
  }
  const { matches } = await checkSimilarity(blockTexts(ops), file.name, origin);
  return matches.filter((m) => isCopy(m) || isSameName(m) || m.comparison.relation === 'overlap');
}
