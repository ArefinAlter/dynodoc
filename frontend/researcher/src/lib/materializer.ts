/**
 * TS port of the Rust materializer fold (`crates/engine-core/src/materializer.rs`).
 *
 * This MUST apply events the same way the engine does, or the optimistic UI lies to the
 * user. The rules below mirror `Materializer::fold_into` event-for-event. The unit tests
 * in `materializer.test.ts` lock each event type against the Rust rules.
 *
 * PoC reduction of the doc's watch-out: a full cross-language fuzz parity harness
 * (generate random sequences, run through both Rust and TS, assert equal output) is
 * deferred — see SETUP.md. The per-event tests here are the PoC substitute.
 *
 * Divergences from the Rust fold are intentional and minimal:
 * - The Rust fold returns `Err` for a missing-node / missing-suggestion reference; here
 *   we skip such events (the UI must never throw on a server-canonical stream — the
 *   server already validated ordering, so this only guards a corrupt/partial replay).
 */
import type { DocumentState, EventPayload, EventRow, MaterializedNode } from './api/types';

/** A fresh, empty folded state. */
export function emptyState(): DocumentState {
  return { nodes: {}, comments: [], suggestions: {}, removed_choices: [] };
}

/**
 * `object_or_value` from the Rust fold: an object passes through; anything else is
 * wrapped under a `"value"` key so `current_fields` is always a JSON object.
 */
function objectOrValue(value: unknown): Record<string, unknown> {
  if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
    return value as Record<string, unknown>;
  }
  return { value: value ?? null };
}

/**
 * Fold one event's payload into `state`, mutating it in place. Mirrors the Rust match
 * arm-for-arm. Returns `true` if the event changed state, `false` if it was a no-op
 * (e.g. a dangling reference that the Rust fold would have rejected).
 */
export function applyPayload(state: DocumentState, payload: EventPayload): boolean {
  switch (payload.type) {
    case 'NodeCreated': {
      const node: MaterializedNode = {
        id: payload.node_id,
        parent_id: payload.parent_id ?? null,
        type: payload.node_type,
        pos: payload.pos,
        current_fields: objectOrValue(payload.fields),
        var_name: payload.var_name ?? null,
        deleted: false
      };
      state.nodes[payload.node_id] = node;
      return true;
    }
    case 'FieldEdited': {
      const node = state.nodes[payload.node_id];
      if (!node) return false;
      node.current_fields = { ...node.current_fields, [payload.field]: payload.value };
      if (payload.field === 'var_name' || payload.field === 'varName') {
        node.var_name = typeof payload.value === 'string' ? payload.value : null;
      }
      return true;
    }
    case 'NodeMoved': {
      const node = state.nodes[payload.node_id];
      if (!node) return false;
      node.parent_id = payload.new_parent_id ?? null;
      node.pos = payload.new_pos;
      return true;
    }
    case 'NodeRestored': {
      const node = state.nodes[payload.node_id];
      if (!node) return false;
      node.deleted = false;
      state.removed_choices = state.removed_choices.filter((id) => id !== payload.node_id);
      return true;
    }
    case 'NodeDeleted': {
      const node = state.nodes[payload.node_id];
      if (!node) return false;
      node.deleted = true;
      return true;
    }
    case 'ChoiceAdded': {
      // `or_insert`: only inserts if the choice node is absent.
      if (!state.nodes[payload.choice_id]) {
        state.nodes[payload.choice_id] = {
          id: payload.choice_id,
          parent_id: payload.node_id,
          type: 'choice',
          pos: payload.choice_id,
          current_fields: objectOrValue(payload.fields),
          var_name: null,
          deleted: false
        };
      }
      state.removed_choices = state.removed_choices.filter((id) => id !== payload.choice_id);
      return true;
    }
    case 'ChoiceRemoved': {
      if (!state.removed_choices.includes(payload.choice_id)) {
        state.removed_choices.push(payload.choice_id);
      }
      return true;
    }
    case 'CommentAdded': {
      state.comments.push({ node_id: payload.node_id ?? null, body: payload.body });
      return true;
    }
    case 'SuggestionProposed': {
      state.suggestions[payload.suggestion_id] = {
        target_node_id: payload.target_node_id ?? null,
        detail: payload.detail ?? null,
        accepted: false,
        rejected_reason: null
      };
      return true;
    }
    case 'SuggestionAccepted': {
      const s = state.suggestions[payload.suggestion_id];
      if (!s) return false;
      s.accepted = true;
      s.rejected_reason = null;
      return true;
    }
    case 'SuggestionRejected': {
      const s = state.suggestions[payload.suggestion_id];
      if (!s) return false;
      s.accepted = false;
      s.rejected_reason = payload.reason ?? 'rejected';
      return true;
    }
    case 'Deployed':
      return false;
    default:
      return false;
  }
}

/** Apply a full event row (uses `event.payload`). */
export function applyEvent(state: DocumentState, event: EventRow): boolean {
  return applyPayload(state, event.payload);
}

/** Fold a sequence of events from a base state (clones the base; does not mutate it). */
export function fold(base: DocumentState, events: EventRow[]): DocumentState {
  const state = structuredClone(base);
  for (const event of events) applyEvent(state, event);
  return state;
}
