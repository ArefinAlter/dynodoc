/**
 * Hand-written response types for the stage-18 REST + SSE surface.
 *
 * Stage 18 annotated every response body as `serde_json::Value`, so the committed
 * `openapi.yaml` only types REQUEST bodies. The generated `openapi.d.ts` therefore
 * gives typed requests + `unknown` responses. These types mirror the exact Rust serde
 * shapes (see `crates/engine-shared/src/rows.rs`, `crates/engine-core/src/materializer.rs`,
 * and `crates/engine-api/src/{documents,ops}/routes.rs`).
 *
 * Wire-form notes that bite:
 * - Enum columns serialize as their lowercase storage repr: `DocumentStatus` is
 *   `"draft" | "deployed" | "archived"`, node `type` is `"form" | "section" | "item"
 *   | "choice" | "clause" | "paragraph"`.
 * - The semantic `EventType` / event-payload `type` tag is PascalCase
 *   (`"FieldEdited"`, `"NodeCreated"`, …) — it is NOT lowercased.
 * - `DocumentDetail` is the `Document` row flattened + a `state: DocumentState`.
 * - `DocumentState.nodes` / `.suggestions` are JSON objects keyed by id (Rust
 *   BTreeMap), `.removed_choices` is an array (HashSet), `.comments` is an array.
 * - Op responses are wrapped: `{ "event": Event }`.
 */

// --- enums ----------------------------------------------------------------------

export type DocumentStatus = 'draft' | 'deployed' | 'archived';

export type NodeType = 'form' | 'section' | 'item' | 'choice' | 'clause' | 'paragraph';

/** The semantic event vocabulary (PascalCase on the wire — see `enums.rs`). */
export type EventTypeName =
  | 'NodeCreated'
  | 'NodeMoved'
  | 'NodeDeleted'
  | 'FieldEdited'
  | 'ChoiceAdded'
  | 'ChoiceRemoved'
  | 'CommentAdded'
  | 'SuggestionProposed'
  | 'SuggestionAccepted'
  | 'SuggestionRejected'
  | 'Deployed';

// --- document rows --------------------------------------------------------------

/** A row of `document` (mirrors `engine_shared::Document`). */
export interface Document {
  id: string;
  title: string;
  languages: string[];
  status: DocumentStatus;
  settings: unknown;
  deployed_snapshot_id: string | null;
  created_by: string | null;
  created_at: string;
  updated_at: string;
}

/** One node in the materialized current state (`MaterializedNode`). */
export interface MaterializedNode {
  id: string;
  parent_id: string | null;
  type: NodeType;
  pos: string;
  current_fields: Record<string, unknown>;
  var_name: string | null;
  deleted: boolean;
}

/** A reviewer comment carried alongside the tree (`CommentState`). */
export interface CommentState {
  node_id: string | null;
  body: string;
}

/** Suggestion workflow state (`SuggestionState`). `detail` is the wrapped op. */
export interface SuggestionState {
  target_node_id: string | null;
  detail: unknown;
  accepted: boolean;
  rejected_reason: string | null;
}

/** The folded current state (`DocumentState`). Maps serialize as JSON objects. */
export interface DocumentState {
  nodes: Record<string, MaterializedNode>;
  comments: CommentState[];
  suggestions: Record<string, SuggestionState>;
  removed_choices: string[];
}

/** `GET /documents/{id}` — `Document` flattened + `state` + the seq it folded through. */
export type DocumentDetail = Document & {
  state: DocumentState;
  /** Event seq `state` was folded through; resume the SSE stream from here. */
  latest_seq: number;
};

// --- events ---------------------------------------------------------------------

/**
 * The internally-tagged event payload (`engine_shared::EventPayload`). The `type`
 * tag is PascalCase and lives inside the JSON. This is what the materializer folds
 * and what a suggestion's `detail` wraps.
 */
export type EventPayload =
  | {
      type: 'NodeCreated';
      node_id: string;
      node_type: NodeType;
      parent_id?: string | null;
      pos: string;
      fields?: unknown;
      var_name?: string | null;
    }
  | { type: 'NodeMoved'; node_id: string; new_parent_id?: string | null; new_pos: string }
  | { type: 'NodeDeleted'; node_id: string }
  | { type: 'FieldEdited'; node_id: string; field: string; value: unknown }
  | { type: 'ChoiceAdded'; node_id: string; choice_id: string; fields?: unknown }
  | { type: 'ChoiceRemoved'; node_id: string; choice_id: string }
  | { type: 'CommentAdded'; node_id?: string | null; body: string }
  | {
      type: 'SuggestionProposed';
      suggestion_id: string;
      target_node_id?: string | null;
      detail?: unknown;
    }
  | { type: 'SuggestionAccepted'; suggestion_id: string }
  | { type: 'SuggestionRejected'; suggestion_id: string; reason?: string | null }
  | { type: 'Deployed'; snapshot_id?: string | null };

/**
 * A row of `event` (mirrors `engine_shared::Event`). This is the SSE frame body and
 * each `/events` page item. `payload` is the internally-tagged {@link EventPayload};
 * `type` is its PascalCase discriminant. The 32-byte hash columns serialize as JSON
 * arrays of byte values (`Vec<u8>`).
 */
export interface EventRow {
  id: string;
  document_id: string;
  seq: number;
  type: EventTypeName;
  actor_id: string;
  target_node_id: string | null;
  payload: EventPayload;
  content_hash: number[];
  prev_chain_hash: number[];
  chain_hash: number[];
  created_at: string;
}

// --- envelopes ------------------------------------------------------------------

/** A cursor-paginated list response (`Page<T>`). */
export interface Page<T> {
  items: T[];
  next_cursor?: string | null;
}

/** The wrapper every write op returns: `{ event }`. */
export interface OpResponse {
  event: EventRow;
}

// --- audit ----------------------------------------------------------------------

/** `GET /documents/{id}/verify` — hash-chain verification result. */
export interface VerifyResult {
  /** True if the whole chain recomputes correctly. */
  ok: boolean;
  /** The seq of the first inconsistency, if any. */
  broken_at_seq?: number;
  /** Human-readable detail when broken. */
  detail?: string;
}
