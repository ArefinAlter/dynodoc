import type { Node } from '@tiptap/pm/model';

/**
 * Where a caret goes when the whole document is replaced by a newer copy of itself, as
 * happens when another person's change arrives while someone is typing. Replacing the
 * document drops the selection to the end, so the rest of what the person types would land
 * in the last paragraph. This treats the difference as one contiguous change: a caret before
 * it stays, a caret after it moves by the change in length, and a caret inside it goes to
 * the end of the replacement.
 */
export function mapPositionAcrossReplace(before: Node, after: Node, position: number): number {
  const limit = after.content.size;
  const start = before.content.findDiffStart(after.content);
  if (start === null) return Math.min(position, limit);
  const end = before.content.findDiffEnd(after.content);
  if (!end) return Math.min(position, limit);
  let { a, b } = end;
  // The two scans can overlap when text is inserted next to identical text.
  const overlap = start - Math.min(a, b);
  if (overlap > 0) {
    a += overlap;
    b += overlap;
  }
  if (position <= start) return Math.min(position, limit);
  if (position >= a) return Math.min(position + (b - a), limit);
  return Math.min(b, limit);
}
