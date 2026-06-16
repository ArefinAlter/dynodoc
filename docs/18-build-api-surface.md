# 07 · Web API Surface (REST + Server-Sent Events)

### Claude Code prompt to start this stage

> Wire the engine internals to an HTTP API. This is the surface every frontend talks to. In `crates/engine-api`:
>
> 1. Set up Axum with `tower-http` middleware: `CompressionLayer` (gzip), `CorsLayer` (restrict to known origins), `TraceLayer` (OpenTelemetry integration), and a custom `RateLimitLayer` reading from Redis.
> 2. Implement the REST endpoints (all return JSON; all require auth except where noted as `[public]`):
>
> **Documents**
>    - `GET /documents` `[public]` — list public documents (paginated, filter by status).
>    - `GET /documents/:id` `[public]` — full document metadata + current materialized state.
>    - `GET /documents/:id/node/:node_id` `[public]` — single node state.
>    - `GET /documents/:id/events?from_seq=&to_seq=` `[T1+]` — raw event stream (paginated, max 1000 per page).
>    - `POST /documents` `[Sponsor]` — create a new document.
>
> **Operations** (the write path)
>    - `POST /documents/:id/ops/node-create` `[T2+]` — create a node (often via a suggestion wrapper at T2).
>    - `POST /documents/:id/ops/field-edit` `[T2+]` — edit a field.
>    - `POST /documents/:id/ops/comment` `[T2+]` — add a comment.
>    - `POST /documents/:id/ops/vote` `[T1+]` — cast a vote.
>    - `POST /documents/:id/ops/suggest` `[T2+]` — propose a wrapped event.
>    - `POST /documents/:id/proposals/:event_id/accept` `[Sponsor]` — accept a proposal.
>    - `POST /documents/:id/proposals/:event_id/reject` `[Sponsor]` — reject a proposal.
>    - `POST /documents/:id/deploy` `[Sponsor]` — mark a deployed checkpoint.
>
> **Subscriptions**
>    - `GET /documents/:id/stream` — Server-Sent Events stream of new events. Optional `?since_seq=` query for resume. Send keepalive pings every 30s. Limit to 100 concurrent subscribers per document; reject with 503 above that.
>
> **Audit**
>    - `GET /documents/:id/verify` `[public]` — runs `EventLog::verify_chain` and returns OK or the seq of the first inconsistency. Cacheable for 60 seconds.
>    - `GET /documents/:id/snapshot/:snapshot_id` `[public]` — full snapshot with merkle_root.
>    - `GET /documents/:id/merkle-proof/:event_id` `[public]` — inclusion proof for an event in the latest snapshot.
>
> 3. **OpenAPI spec**: generate `openapi.yaml` automatically from the route handlers using `utoipa`. Commit it to the repo. Wire CI to fail if the generated spec drifts from the committed one.
>
> 4. **Error handling**: a single `ApiError` enum with variants `NotFound`, `Unauthorized`, `Forbidden`, `BadRequest{reason}`, `Conflict{reason}`, `RateLimited{retry_after}`, `Internal`. All errors return JSON `{ error: "...", code: "...", details: {...} }`. Log internal errors with full context to traces; never leak internals to clients.
>
> 5. **Pagination**: cursor-based, not offset. All list endpoints accept `?cursor=...&limit=...` and return `{ items: [...], next_cursor: ... }`. Cursor is opaque (base64-encoded `(last_seq, last_id)`).
>
> 6. **Idempotency**: every write endpoint accepts an optional `Idempotency-Key` header. If the same key arrives within 24 hours, return the cached response without re-executing. Store keys in Redis with the response body.
>
> 7. Tests:
>    - Integration tests using `axum::test` + a real Postgres test database.
>    - Test SSE: subscribe, append an event from another connection, verify the subscriber receives it within 100ms.
>    - Test idempotency: same key returns the same response twice without duplicating events.

### Why SSE for the realtime channel

WebSockets are bidirectional, which we don't need. The client→server direction is just HTTP POST. The server→client direction is a stream of events. SSE handles this natively:

- Auto-reconnect with `Last-Event-ID` header (resumable from a sequence number).
- Plain HTTP/2 multiplexing; no separate upgrade.
- Works through almost every proxy and CDN unmodified.
- Trivially auth'd (token in the URL or header at connect time).

Axum has `Sse` response type that handles backpressure correctly:

```rust
// engine-api/src/handlers/stream.rs
pub async fn document_stream(
    State(state): State<AppState>,
    Path(document_id): Path<DocumentId>,
    Query(params): Query<StreamParams>,
    auth: AuthContext,
) -> impl IntoResponse {
    let stream = state.subscriptions
        .subscribe(document_id, params.since_seq)
        .await
        .map(|event| {
            Event::default()
                .id(event.id.0.clone())
                .event("event")
                .json_data(&event)
                .unwrap()
        });
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(30)))
}
```

The subscription mechanism is a tokio broadcast channel per document, with new appended events fanned out to subscribers. Slow subscribers are dropped (use `try_send`, not `send`); the SSE auto-reconnect with `Last-Event-ID` gives them a clean recovery path.

### Cursor pagination, not offset

Offset pagination breaks under concurrent writes. With a moving cursor (last_seen_seq), a client can safely page through results that are growing in real time without missing or duplicating items. Cursors are stable references to positions in the log.

```rust
// engine-api/src/pagination.rs
#[derive(Serialize, Deserialize)]
pub struct Cursor { pub last_seq: i64, pub last_id: String }

pub fn encode(c: &Cursor) -> String {
    URL_SAFE_NO_PAD.encode(serde_json::to_vec(c).unwrap())
}
pub fn decode(s: &str) -> Option<Cursor> {
    let bytes = URL_SAFE_NO_PAD.decode(s).ok()?;
    serde_json::from_slice(&bytes).ok()
}
```

### Watch out for

**SSE through CDNs.** Make sure your CDN (Cloudflare, etc.) doesn't buffer SSE responses. Set `Cache-Control: no-cache, no-transform` and `X-Accel-Buffering: no` on SSE responses.

**Rate-limiting writes vs reads.** Write endpoints get strict per-identity rate limits. Read endpoints get IP-based rate limits and aggressive CDN caching. Don't apply identity-rate-limits to GETs; you'll throttle legitimate scrollers.

**OpenAPI generation drift.** It's easy for the handler to add a query param that doesn't appear in the spec, breaking generated TypeScript clients. CI must fail if the generated spec doesn't match the committed one.

**The 100-concurrent-subscriber limit per document is intentional.** A document in active consultation might have 10,000 readers; almost none of them need an SSE connection. They can poll. Reserve SSE for participants actively contributing.

### Verification checklist

- [ ] Integration tests pass for every endpoint.
- [ ] OpenAPI spec is generated and committed; CI catches drift.
- [ ] SSE delivery latency under 100ms in test.
- [ ] Idempotency cache works (same key, two POSTs → one event).
- [ ] Auth middleware enforces tier requirements.
- [ ] Rate limits enforced on writes.
