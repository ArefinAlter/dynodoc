/**
 * Adapter: fold a flat {@link DocumentState} node tree into the existing
 * {@link EditorDocument} view model (sections → items → choices) so the scaffold
 * components render real data unchanged.
 *
 * Tree shape (PoC questionnaire kinds, docs/04 §D.1):
 * - `form` / `section` nodes (and any top-level node with item children) → sections.
 * - `item` (+ `paragraph` / `clause`) nodes under a section → items.
 * - `choice` nodes under an item → choices (excluding `removed_choices`).
 *
 * Field mapping (`current_fields`):
 *   text     ← label | text | title | value
 *   hint     ← hint
 *   type     ← type | node.type
 *   varName  ← node.var_name | var_name field
 *   relevance← relevance
 *   choice label ← label | text | value
 *
 * Deleted nodes are dropped. Items that have no resolvable section parent are gathered
 * under a synthetic "Ungrouped" section so nothing silently disappears.
 */
import type {
  CommentState,
  DocumentDetail,
  DocumentState,
  MaterializedNode,
  SuggestionState
} from './api/types';
import type {
  ActivityEvent,
  ConflictCard,
  EditorChoice,
  EditorDocument,
  EditorItem,
  EditorSection
} from './types/editor';

const SECTION_KINDS = new Set(['form', 'section']);
const ITEM_KINDS = new Set(['item', 'paragraph', 'clause']);

function fieldString(fields: Record<string, unknown>, key: string): string | undefined {
  const v = fields[key];
  if (typeof v === 'string') return v;
  if (typeof v === 'number' || typeof v === 'boolean') return String(v);
  return undefined;
}

function nodeText(node: MaterializedNode): string {
  const f = node.current_fields;
  return (
    fieldString(f, 'label') ??
    fieldString(f, 'text') ??
    fieldString(f, 'title') ??
    fieldString(f, 'value') ??
    ''
  );
}

function byPos(a: MaterializedNode, b: MaterializedNode): number {
  if (a.pos < b.pos) return -1;
  if (a.pos > b.pos) return 1;
  return a.id < b.id ? -1 : 1;
}

/** Live (non-deleted) nodes whose parent is `parentId`, in sibling order. */
function children(
  state: DocumentState,
  parentId: string | null,
  kinds: Set<string>
): MaterializedNode[] {
  return Object.values(state.nodes)
    .filter((n) => !n.deleted && (n.parent_id ?? null) === parentId && kinds.has(n.type))
    .sort(byPos);
}

function toChoice(node: MaterializedNode): EditorChoice {
  return { id: node.id, label: nodeText(node) || node.id };
}

function toItem(
  state: DocumentState,
  node: MaterializedNode,
  number: string,
  counts: { comments: Map<string, number>; suggestions: Map<string, number> }
): EditorItem {
  const f = node.current_fields;
  const choices = children(state, node.id, new Set(['choice']))
    .filter((c) => !state.removed_choices.includes(c.id))
    .map(toChoice);
  return {
    id: node.id,
    number,
    type: fieldString(f, 'type') ?? node.type,
    text: nodeText(node),
    hint: fieldString(f, 'hint'),
    varName: node.var_name ?? fieldString(f, 'var_name') ?? undefined,
    relevance: fieldString(f, 'relevance'),
    choices: choices.length ? choices : undefined,
    commentsCount: counts.comments.get(node.id) ?? 0,
    suggestionsCount: counts.suggestions.get(node.id) ?? 0
  };
}

/** Tally per-node comment + suggestion counts for the gutter badges. */
function tallyCounts(comments: CommentState[], suggestions: Record<string, SuggestionState>) {
  const commentCounts = new Map<string, number>();
  for (const c of comments) {
    if (!c.node_id) continue;
    commentCounts.set(c.node_id, (commentCounts.get(c.node_id) ?? 0) + 1);
  }
  const suggestionCounts = new Map<string, number>();
  for (const s of Object.values(suggestions)) {
    if (s.accepted || s.rejected_reason || !s.target_node_id) continue;
    suggestionCounts.set(s.target_node_id, (suggestionCounts.get(s.target_node_id) ?? 0) + 1);
  }
  return { comments: commentCounts, suggestions: suggestionCounts };
}

/** Build the editor view model from a full `GET /documents/{id}` response. */
export function toEditorDocument(detail: DocumentDetail): EditorDocument {
  const state = detail.state;
  const counts = tallyCounts(state.comments, state.suggestions);

  const sectionNodes = Object.values(state.nodes)
    .filter((n) => !n.deleted && SECTION_KINDS.has(n.type))
    .sort(byPos);

  const sections: EditorSection[] = sectionNodes.map((sec, sIdx) => {
    const itemNodes = children(state, sec.id, ITEM_KINDS);
    const f = sec.current_fields;
    return {
      id: sec.id,
      number: String(sIdx + 1),
      title: nodeText(sec) || fieldString(f, 'name') || 'Section',
      meta: fieldString(f, 'meta'),
      items: itemNodes.map((node, iIdx) => toItem(state, node, `${sIdx + 1}.${iIdx + 1}`, counts))
    };
  });

  // Items with no section parent (top-level or orphaned) → synthetic section.
  const sectionIds = new Set(sectionNodes.map((s) => s.id));
  const orphans = Object.values(state.nodes)
    .filter(
      (n) =>
        !n.deleted &&
        ITEM_KINDS.has(n.type) &&
        (n.parent_id === null || !sectionIds.has(n.parent_id))
    )
    .sort(byPos);
  if (orphans.length) {
    const n = sections.length + 1;
    sections.push({
      id: '__ungrouped__',
      number: String(n),
      title: 'Ungrouped',
      meta: 'Items without a section',
      items: orphans.map((node, iIdx) => toItem(state, node, `${n}.${iIdx + 1}`, counts))
    });
  }

  return {
    id: detail.id,
    title: detail.title,
    overline: 'dynodoc · live document',
    organization: detail.created_by ? `Owner ${shortId(detail.created_by)}` : 'dynodoc',
    version: `${detail.status} · seq-driven`,
    status: capitalize(detail.status),
    languages: detail.languages.length ? detail.languages : ['en'],
    collaborators: [],
    sections,
    activity: [],
    conflict: emptyConflict()
  };
}

/** A short, human-friendly prefix of a UUID/ULID for display. */
function shortId(id: string): string {
  return id.slice(0, 8);
}

function capitalize(s: string): string {
  return s.length ? s[0].toUpperCase() + s.slice(1) : s;
}

/** The conflict card the scaffold's `ConflictModal` needs; empty until merge (stage 21). */
export function emptyConflict(): ConflictCard {
  return { id: '', itemId: '', itemLabel: '', remaining: 0, options: [] };
}

/** Map raw event rows into the scaffold's `ActivityEvent` timeline shape. */
export function toActivity(
  events: { actor_id: string; type: string; seq: number }[]
): ActivityEvent[] {
  return events
    .slice()
    .reverse()
    .map((e) => ({
      id: `seq-${e.seq}`,
      time: `seq ${e.seq}`,
      actor: shortId(e.actor_id),
      action: humanizeEventType(e.type),
      target: ''
    }));
}

function humanizeEventType(type: string): string {
  switch (type) {
    case 'NodeCreated':
      return 'created a node';
    case 'NodeMoved':
      return 'moved a node';
    case 'NodeDeleted':
      return 'deleted a node';
    case 'FieldEdited':
      return 'edited a field';
    case 'ChoiceAdded':
      return 'added a choice';
    case 'ChoiceRemoved':
      return 'removed a choice';
    case 'CommentAdded':
      return 'added a comment';
    case 'SuggestionProposed':
      return 'proposed a suggestion';
    case 'SuggestionAccepted':
      return 'accepted a suggestion';
    case 'SuggestionRejected':
      return 'rejected a suggestion';
    case 'Deployed':
      return 'deployed';
    default:
      return type;
  }
}
