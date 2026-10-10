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

use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use engine_shared::{DocumentId, Event};
use futures::stream::{self, StreamExt};
use serde::Deserialize;
use tokio::sync::broadcast;
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

    /// Check and subscribe under the same lock so concurrent requests cannot
    /// oversubscribe between a receiver-count check and acquiring the receiver.
    fn subscribe(&self, document_id: DocumentId) -> Option<broadcast::Receiver<Arc<Event>>> {
        let mut map = self.inner.lock().expect("subscriptions mutex poisoned");
        let sender = map
            .entry(document_id.0)
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0);
        (sender.receiver_count() < MAX_SUBSCRIBERS_PER_DOCUMENT).then(|| sender.subscribe())
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
    headers: HeaderMap,
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

    // Subscribe before pinning the backfill head; no append can slip through the gap.
    let receiver = state
        .subscriptions
        .subscribe(document_id)
        .ok_or(ApiError::RateLimited { retry_after: 5 })?;
    let since = match params.since_seq {
        Some(since) => since,
        None => match headers.get("last-event-id") {
            Some(value) => value
                .to_str()
                .ok()
                .and_then(|v| v.parse::<i64>().ok())
                .ok_or_else(|| ApiError::BadRequest {
                    reason: "Invalid Last-Event-ID".into(),
                })?,
            None => 0,
        },
    };
    let head: i64 =
        sqlx::query_scalar("select coalesce(max(seq),0) from event where document_id=$1")
            .bind(document_id.0)
            .fetch_one(&state.pool)
            .await?;
    if since < 0 || since > head {
        return Err(ApiError::BadRequest {
            reason: "Resume position is outside document history".into(),
        });
    }
    // Hold only one page while catching up. DB failures and broadcast lag terminate
    // the stream so the client can resume from its last delivered sequence.
    let events = stream::unfold(
        (
            state.pool.clone(),
            since,
            VecDeque::<Event>::new(),
            receiver,
        ),
        move |(pool, mut cursor, mut pending, mut receiver)| async move {
            loop {
                if let Some(event) = pending.pop_front() {
                    cursor = event.seq;
                    return Some((to_sse_event(event), (pool, cursor, pending, receiver)));
                }
                if cursor < head {
                    match engine_core::log::read_range_page(
                        &pool,
                        document_id,
                        cursor + 1,
                        head,
                        100,
                    )
                    .await
                    {
                        Ok(page) if !page.is_empty() => pending.extend(page),
                        _ => return None,
                    }
                } else {
                    match receiver.recv().await {
                        Ok(event) if event.seq > cursor => {
                            // A missing sequence must be recovered from storage, never skipped.
                            if Some(event.seq) != cursor.checked_add(1) {
                                return None;
                            }
                            cursor = event.seq;
                            return Some((
                                to_sse_event((*event).clone()),
                                (pool, cursor, pending, receiver),
                            ));
                        }
                        Ok(_) => {}
                        Err(_) => return None,
                    }
                }
            }
        },
    );

    let pool = state.pool.clone();
    // Flush an idle subscription immediately through HTTP proxies. Otherwise a
    // caught-up browser can wait for the 30-second heartbeat before seeing headers;
    // concurrent requests for the same URL can wait behind that first response.
    // This is an SSE comment, so it cannot advance the replay cursor.
    let ready = stream::iter([Ok::<_, Infallible>(
        SseEvent::default().comment("connected"),
    )]);
    let stream = ready.chain(events).take_while(move |_| {
        let pool = pool.clone();
        async move {
            if crate::auth::store::ensure_bound_session(
                &pool,
                auth.identity_id,
                auth.session_generation,
                auth.session_id,
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
