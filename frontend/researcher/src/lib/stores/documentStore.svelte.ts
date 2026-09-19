/**
 * One reactive store per open document (Svelte 5 runes).
 *
 * `load()` GETs `/documents/{id}` → seeds `state` (a {@link DocumentState}) and `seq`,
 * then `subscribe()` opens the authenticated SSE stream with `?since_seq=`. Incoming
 * events apply incrementally through the {@link applyEvent} reducer — no refetch.
 *
 * Optimistic updates: write ops apply to `state` immediately, then reconcile. On error
 * we roll back the optimistic mutation and surface a toast. On success the SSE stream
 * delivers the canonical event; we dedupe against our own already-applied events by seq
 * so the echo is a no-op.
 */
import { DocumentStream, type ConnectionStatus } from '$lib/api/sse';
import {
  acceptProposal,
  getDocument,
  listEvents,
  listConflicts,
  opComment,
  opFieldEdit,
  opNodeCreate,
  rejectProposal,
  resolveConflict,
  opSuggest
} from '$lib/api/client';
import type { ConflictView, DocumentDetail, DocumentState, EventRow } from '$lib/api/types';
import { emptyState, applyEvent } from '$lib/materializer';
import { session } from './session.svelte';
import { pushToast } from './toast.svelte';

/** Event types that can create or clear a four-card conflict (trigger a refetch). */
const CONFLICT_AFFECTING = new Set([
  'SuggestionProposed',
  'SuggestionAccepted',
  'SuggestionRejected',
  'FieldEdited'
]);

export class DocumentStore {
  detail = $state<DocumentDetail | null>(null);
  state = $state<DocumentState>(emptyState());
  seq = $state(0);
  connection = $state<ConnectionStatus>('reconnecting');
  loading = $state(true);
  error = $state<string | null>(null);
  /** Live four-card conflicts, refetched on load + whenever a suggestion/field event lands. */
  conflicts = $state<ConflictView[]>([]);

  private stream: DocumentStream | null = null;
  /** Highest seq we've folded — the dedupe watermark for SSE echoes. */
  private appliedSeq = 0;
  private pending = new Map<number, EventRow>();
  private catchingUp = false;

  constructor(public documentId: string) {}

  /** Fetch the snapshot, then open the live stream. */
  async load(): Promise<void> {
    this.loading = true;
    this.error = null;
    try {
      const detail = await getDocument(this.documentId);
      this.detail = detail;
      this.state = detail.state;
      // `latest_seq` is the seq the server folded `state` through (same tail read), so
      // resuming the stream from it delivers only *new* events — no re-fold (which would
      // double-count `CommentAdded`, a push) and no missed window.
      this.seq = detail.latest_seq;
      this.appliedSeq = this.seq;
      this.subscribe();
      void this.loadConflicts();
    } catch (e) {
      this.error = e instanceof Error ? e.message : 'failed to load document';
    } finally {
      this.loading = false;
    }
  }

  private subscribe(): void {
    this.stream?.stop();
    this.stream = new DocumentStream(this.documentId, () => session.accessToken, {
      onEvent: (event) => this.ingest(event),
      onStatus: (status) => (this.connection = status)
    });
    this.stream.start(this.seq);
  }

  /** Apply a canonical event from the stream, deduping our own echoes by seq. */
  private ingest(event: EventRow): void {
    if (event.seq <= this.appliedSeq) return;
    this.pending.set(event.seq, event);
    let changedConflicts = false;
    for (;;) {
      const next = this.pending.get(this.appliedSeq + 1);
      if (!next) break;
      this.pending.delete(next.seq);
      applyEvent(this.state, next);
      this.appliedSeq = next.seq;
      this.seq = next.seq;
      changedConflicts ||= CONFLICT_AFFECTING.has(next.type);
    }
    if (changedConflicts) void this.loadConflicts();
    if (this.pending.size) void this.catchUp();
  }

  private async catchUp(): Promise<void> {
    if (this.catchingUp) return;
    this.catchingUp = true;
    try {
      while (this.pending.size) {
        const page = await listEvents(this.documentId, {
          from_seq: this.appliedSeq + 1,
          limit: 1000
        });
        if (!page.items.length) break;
        for (const event of page.items) this.ingest(event);
      }
    } catch (e) {
      this.toastError(e, 'Could not synchronize. Reopen this document to retry.');
    } finally {
      this.catchingUp = false;
    }
  }

  receive(events: EventRow[]): void {
    for (const event of events) this.ingest(event);
  }

  /** Refetch the live conflicts derived from current state. Best-effort (never throws). */
  async loadConflicts(): Promise<void> {
    try {
      const { items } = await listConflicts(this.documentId);
      this.conflicts = items;
    } catch {
      // A transient failure leaves the prior list; the next event refetches.
    }
  }

  /**
   * Resolve a four-card conflict (author action): accept one competing proposal or
   * write a custom value. Refetches conflicts on success so the modal updates.
   */
  async resolveConflict(
    conflictId: string,
    body: { accept_suggestion_id?: string; custom_value?: unknown }
  ): Promise<void> {
    try {
      const { events } = await resolveConflict(this.documentId, conflictId, body);
      // Fold the canonical events the server appended (the field edit + lifecycle).
      this.receive(events);
      await this.loadConflicts();
    } catch (e) {
      this.toastError(e, 'Resolve failed');
      throw e;
    }
  }

  /** Tear down the live stream (call on unmount). */
  dispose(): void {
    this.stream?.stop();
    this.stream = null;
  }

  // --- write ops (optimistic) -----------------------------------------------------

  /**
   * Edit a field. In `direct` mode this POSTs `/ops/field-edit` and optimistically
   * sets the field; in `suggest` mode it POSTs `/ops/suggest` (wrapping a `FieldEdited`
   * in `detail`) and optimistically records the pending suggestion.
   */
  async editField(
    nodeId: string,
    field: string,
    value: unknown,
    mode: 'direct' | 'suggest'
  ): Promise<void> {
    if (mode === 'suggest') {
      return this.suggestFieldEdit(nodeId, field, value);
    }
    try {
      const { event } = await opFieldEdit(this.documentId, {
        node_id: nodeId,
        field,
        value,
        base_seq: this.seq
      });
      this.ingest(event);
    } catch (e) {
      this.toastError(e, 'Your edit was not saved.');
      throw e;
    }
  }

  private async suggestFieldEdit(nodeId: string, field: string, value: unknown): Promise<void> {
    const detail = { type: 'FieldEdited' as const, node_id: nodeId, field, value };
    const { event } = await opSuggest(this.documentId, { target_node_id: nodeId, detail });
    this.ingest(event);
  }

  /** Create a node (section/item/choice). Optimistically folds the resulting event. */
  async createNode(body: {
    node_type: string;
    parent_id?: string;
    pos: string;
    fields?: Record<string, unknown>;
    var_name?: string;
  }): Promise<EventRow> {
    try {
      const { event } = await opNodeCreate(this.documentId, body);
      this.ingest(event);
      return event;
    } catch (e) {
      this.toastError(e, 'Create failed');
      throw e;
    }
  }

  /** Add a comment, optionally anchored to a node. */
  async comment(body: string, nodeId?: string): Promise<void> {
    const { event } = await opComment(this.documentId, { node_id: nodeId, body });
    this.ingest(event);
  }

  async accept(suggestionId: string): Promise<void> {
    const { event } = await acceptProposal(this.documentId, suggestionId);
    this.ingest(event);
  }

  async reject(suggestionId: string, reason?: string): Promise<void> {
    const { event } = await rejectProposal(this.documentId, suggestionId, reason);
    this.ingest(event);
  }

  private toastError(e: unknown, fallback: string): void {
    pushToast(e instanceof Error ? e.message : fallback, 'error');
  }
}
