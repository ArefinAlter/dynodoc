//! Server-Sent Events: the realtime server→client channel (docs/18, "Why SSE").
//!
//! Clients POST changes (the write path); the server streams new events back over SSE.
//! SSE fits because the only server→client need is an append-ordered event stream, and
//! SSE gives auto-reconnect with `Last-Event-ID` (resume from a seq) for free, over
//! plain HTTP, through proxies and CDNs.
//!
//! ## Subscription registry
//!
//! One [`tokio::sync::broadcast`] channel per document, created lazily on first
//! subscribe and reused. The write path calls [`Subscriptions::publish`] after a
//! committed append; every live subscriber's SSE stream fans the event out. Broadcast
//! is the right primitive: `send` never blocks the publisher, and a subscriber that
//! falls behind the channel's ring buffer surfaces a `Lagged` error rather than
//! back-pressuring the writer — that subscriber simply reconnects (with
//! `Last-Event-ID`) and catches up from the DB. Slow readers degrade themselves, never
//! the system (docs/18 "use try_send, not send").
//!
//! ## Limits and proxy hygiene
//!
//! At most [`MAX_SUBSCRIBERS_PER_DOCUMENT`] concurrent subscribers per document; past
//! that, subscribe returns `503` (a document may have thousands of readers, but only
//! active participants need a live socket — the rest poll). Responses set
//! `Cache-Control: no-cache, no-transform` and `X-Accel-Buffering: no` so a CDN/proxy
//! does not buffer the stream.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use engine_shared::{DocumentId, Event};
use futures::stream::{self, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

use crate::auth::AuthContext;
use crate::error::ApiError;
use crate::{documents::store as doc_store, AppState};

/// Per-document concurrent subscriber cap (docs/18). Above this, subscribe is `503`.
pub const MAX_SUBSCRIBERS_PER_DOCUMENT: usize = 100;

/// Broadcast ring-buffer depth. A subscriber that falls more than this many events
/// behind lags and reconnects; sized generously so only genuinely stuck clients drop.
const CHANNEL_CAPACITY: usize = 256;

/// Keepalive ping interval (docs/18: every 30s) to hold the connection through idle
/// proxies.
const KEEPALIVE_SECS: u64 = 30;

/// The per-document broadcast registry, cloned into [`AppState`].
#[derive(Clone, Default)]
pub struct Subscriptions {
    inner: Arc<Mutex<HashMap<Uuid, broadcast::Sender<Arc<Event>>>>>,
}

impl Subscriptions {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get (or lazily create) the broadcast sender for a document.
    fn sender(&self, document_id: DocumentId) -> broadcast::Sender<Arc<Event>> {
        let mut map = self.inner.lock().expect("subscriptions mutex poisoned");
        map.entry(document_id.0)
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
            .clone()
    }

    /// Fan a freshly committed event out to all live subscribers. Non-blocking: if
    /// there are no subscribers (or all have lagged away), this is a no-op. The write
    /// path calls this after `commit()`.
    pub fn publish(&self, document_id: DocumentId, event: Event) {
        let sender = self.sender(document_id);
        // `send` errors only when there are zero receivers — not an error for us.
        let _ = sender.send(Arc::new(event));
    }

    /// Current live subscriber count for a document (the receiver count).
    fn subscriber_count(&self, document_id: DocumentId) -> usize {
        let map = self.inner.lock().expect("subscriptions mutex poisoned");
        map.get(&document_id.0)
            .map(|s| s.receiver_count())
            .unwrap_or(0)
    }
}

/// `GET /documents/:id/stream` query: optional resume point.
#[derive(Debug, Deserialize)]
pub struct StreamParams {
    /// Resume after this seq (the `Last-Event-ID` a reconnecting client replays).
    #[serde(default)]
    pub since_seq: Option<i64>,
}

pub fn router() -> Router<AppState> {
    Router::new().route("/documents/:id/stream", get(document_stream))
}

/// Subscribe to a document's live event stream. Authenticated (a live socket is for
/// participants). Emits any events missed since `since_seq` first, then live appends.
async fn document_stream(
    State(state): State<AppState>,
    Path(document_id): Path<Uuid>,
    Query(params): Query<StreamParams>,
    auth: AuthContext,
) -> Result<Response, ApiError> {
    let document_id = DocumentId(document_id);
    crate::ops::apply::require_role(
        &state.pool,
        document_id,
        engine_shared::IdentityId(auth.identity_id),
    )
    .await?;

    // The document must exist (a stream to nothing is a 404, not an empty socket).
    if !doc_store::document_exists(&state.pool, document_id).await? {
        return Err(ApiError::NotFound);
    }

    // Enforce the concurrent-subscriber cap before we hand out a receiver.
    if state.subscriptions.subscriber_count(document_id) >= MAX_SUBSCRIBERS_PER_DOCUMENT {
        return Err(ApiError::RateLimited { retry_after: 5 });
    }

    // Subscribe *before* reading the backfill so no append slips through the gap
    // between the catch-up read and the live subscription.
    let receiver = state.subscriptions.sender(document_id).subscribe();
    let since = params.since_seq.unwrap_or(0);

    // Backfill: events strictly after `since_seq` already in the log. A reconnecting
    // client resumes exactly where it left off.
    let backfill = engine_core::log::read_range(&state.pool, document_id, since + 1, i64::MAX)
        .await
        .map_err(ApiError::from)?;
    let last_backfilled = backfill.last().map(|e| e.seq).unwrap_or(since);

    let backfill_stream = stream::iter(backfill.into_iter().map(to_sse_event));

    // Live stream: drop events at or before the last backfilled seq (the subscribe
    // happened before the backfill read, so the first live events may overlap).
    let live_stream = BroadcastStream::new(receiver)
        .take_while(|result| futures::future::ready(result.is_ok()))
        .filter_map(move |result| {
            let value = match result {
                Ok(event) if event.seq > last_backfilled => Some(to_sse_event((*event).clone())),
                // A lagged subscriber missed events; the SSE client reconnects with
                // Last-Event-ID and catches up from the backfill. Drop, don't error.
                _ => None,
            };
            async move { value }
        });

    let pool = state.pool.clone();
    // Flush an idle subscription immediately through HTTP proxies. Otherwise a
    // caught-up browser can wait for the 30-second heartbeat before seeing headers;
    // concurrent requests for the same URL can wait behind that first response.
    // This is an SSE comment, so it cannot advance the replay cursor.
    let ready = stream::iter([Ok::<_, Infallible>(
        SseEvent::default().comment("connected"),
    )]);
    let stream = ready
        .chain(backfill_stream)
        .chain(live_stream)
        .take_while(move |_| {
            let pool = pool.clone();
            async move {
                if crate::auth::store::ensure_session(
                    &pool,
                    auth.identity_id,
                    auth.session_generation,
                )
                .await
                .is_err()
                {
                    return false;
                }
                crate::auth::store::role_for(&pool, document_id, auth.identity_id)
                    .await
                    .ok()
                    .flatten()
                    .is_some()
            }
        });

    let sse = Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(KEEPALIVE_SECS))
            .text("keepalive"),
    );

    // Proxy hygiene: stop any CDN/reverse-proxy from buffering the stream.
    let mut response = sse.into_response();
    let headers = response.headers_mut();
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-cache, no-transform"),
    );
    headers.insert(
        header::HeaderName::from_static("x-accel-buffering"),
        header::HeaderValue::from_static("no"),
    );
    Ok(response)
}

/// Render one event as an SSE frame: `id:` is the seq (the `Last-Event-ID` a client
/// replays as `since_seq`), `event:` is the type, data is the JSON-serialized event.
fn to_sse_event(event: Event) -> Result<SseEvent, Infallible> {
    let id = event.seq.to_string();
    let event_type = event.event_type.clone();
    let frame = SseEvent::default()
        .id(id)
        .event(event_type)
        .json_data(&event)
        .unwrap_or_else(|_| SseEvent::default().comment("serialization error"));
    Ok(frame)
}
