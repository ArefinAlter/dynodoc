/**
 * A fetch-based SSE reader for `GET /documents/{id}/stream`.
 *
 * The native `EventSource` cannot set an `Authorization` header, and the stream route is
 * authenticated, so we read the `text/event-stream` body with `fetch` + a streaming
 * reader instead. We parse `id:` / `event:` / `data:` frames per the SSE wire format and
 * resume with `?since_seq=` (the server's backfill replays anything missed).
 *
 * Reconnect: on a dropped/errored connection we retry with backoff, resuming from the
 * last seq we saw. The caller is notified of connection state transitions so the UI can
 * show `connected | reconnecting | offline`.
 */
import { API_BASE_URL } from './config';
import type { EventRow } from './types';

export type ConnectionStatus = 'connected' | 'reconnecting' | 'offline';

export interface SseHandlers {
  onEvent: (event: EventRow) => void;
  onStatus?: (status: ConnectionStatus) => void;
}

const MAX_BACKOFF_MS = 15_000;

export class DocumentStream {
  private controller: AbortController | null = null;
  private closed = false;
  private backoff = 1_000;

  constructor(
    private documentId: string,
    private getToken: () => string | null,
    private handlers: SseHandlers
  ) {}

  /** Open the stream, resuming after `sinceSeq`. */
  start(sinceSeq: number): void {
    this.closed = false;
    void this.connect(sinceSeq);
  }

  /** Permanently stop the stream. */
  stop(): void {
    this.closed = true;
    this.controller?.abort();
    this.controller = null;
  }

  private async connect(sinceSeq: number): Promise<void> {
    let lastSeq = sinceSeq;
    this.controller = new AbortController();
    const token = this.getToken();
    const headers: Record<string, string> = { accept: 'text/event-stream' };
    if (token) headers['Authorization'] = `Bearer ${token}`;

    try {
      const res = await fetch(
        `${API_BASE_URL}/documents/${this.documentId}/stream?since_seq=${lastSeq}`,
        { headers, signal: this.controller.signal }
      );
      if (!res.ok || !res.body) {
        throw new Error(`stream failed (${res.status})`);
      }

      this.handlers.onStatus?.('connected');
      this.backoff = 1_000;

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = '';

      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;
        buffer += decoder.decode(value, { stream: true });

        // Frames are separated by a blank line.
        let sep: number;
        while ((sep = buffer.indexOf('\n\n')) !== -1) {
          const frame = buffer.slice(0, sep);
          buffer = buffer.slice(sep + 2);
          const seq = this.handleFrame(frame);
          if (seq !== null) lastSeq = seq;
        }
      }
      // Server closed the stream cleanly; treat as a reconnect.
      throw new Error('stream closed');
    } catch (err) {
      if (this.closed) return;
      if (err instanceof DOMException && err.name === 'AbortError') return;
      this.handlers.onStatus?.(navigator?.onLine === false ? 'offline' : 'reconnecting');
      const wait = this.backoff;
      this.backoff = Math.min(this.backoff * 2, MAX_BACKOFF_MS);
      await delay(wait);
      if (!this.closed) void this.connect(lastSeq);
    }
  }

  /** Parse one SSE frame; returns the event seq if it carried a JSON event. */
  private handleFrame(frame: string): number | null {
    let data = '';
    for (const line of frame.split('\n')) {
      if (line.startsWith('data:')) data += line.slice(5).trimStart();
      // `id:` and `event:` lines are ignored — the seq is also inside the JSON body.
    }
    if (!data) return null;
    try {
      const event = JSON.parse(data) as EventRow;
      if (typeof event.seq !== 'number') return null;
      this.handlers.onEvent(event);
      return event.seq;
    } catch {
      // Keepalive comments / malformed frames: ignore.
      return null;
    }
  }
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
