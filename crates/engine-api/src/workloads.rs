//! Process-local admission bounds. These are not fleet-wide quotas or memory proofs.
use crate::error::ApiError;
use axum::{
    body::Body,
    extract::{Request, State},
    http::Method,
    middleware::Next,
    response::{IntoResponse, Response},
};
use futures::StreamExt;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::Semaphore;
use uuid::Uuid;

const WINDOW: Duration = Duration::from_secs(60);
const MAX_KEYS: usize = 16_384;
#[derive(Clone)]
pub struct Workloads {
    requests: Arc<Semaphore>,
    streams: Arc<Semaphore>,
    expensive: Arc<Semaphore>,
    counters: Arc<Mutex<HashMap<(bool, Uuid), Counter>>>,
}
struct Counter {
    since: Instant,
    used: u32,
    active: u32,
}
impl Default for Workloads {
    fn default() -> Self {
        Self {
            requests: Arc::new(Semaphore::new(64)),
            streams: Arc::new(Semaphore::new(256)),
            expensive: Arc::new(Semaphore::new(8)),
            counters: Default::default(),
        }
    }
}
pub(crate) struct ReadPermit {
    counters: Arc<Mutex<HashMap<(bool, Uuid), Counter>>>,
    keys: Vec<(bool, Uuid)>,
    _permit: tokio::sync::OwnedSemaphorePermit,
}
impl Drop for ReadPermit {
    fn drop(&mut self) {
        if let Ok(mut counters) = self.counters.lock() {
            for key in &self.keys {
                if let Some(c) = counters.get_mut(key) {
                    c.active -= 1;
                }
            }
        }
    }
}
fn limited() -> ApiError {
    ApiError::RateLimited { retry_after: 60 }
}
impl Workloads {
    /// Call only after authenticating and checking current resource membership.
    pub(crate) fn enter(&self, actor: Uuid, project: Option<Uuid>) -> Result<ReadPermit, ApiError> {
        let permit = self
            .expensive
            .clone()
            .try_acquire_owned()
            .map_err(|_| limited())?;
        let mut counters = self
            .counters
            .lock()
            .map_err(|_| ApiError::Internal("workload lock poisoned".into()))?;
        let now = Instant::now();
        // Bound registry size even when arbitrary authenticated accounts are used.
        if counters.len() >= MAX_KEYS - 2 {
            counters.retain(|_, c| c.active > 0 || now.duration_since(c.since) < WINDOW);
        }
        let mut keys = vec![(false, actor)];
        if let Some(project) = project {
            keys.push((true, project));
        }
        let missing = keys.iter().filter(|k| !counters.contains_key(k)).count();
        if counters.len() + missing > MAX_KEYS {
            return Err(limited());
        }
        for key in &keys {
            if let Some(c) = counters.get_mut(key) {
                if now.duration_since(c.since) >= WINDOW {
                    c.since = now;
                    c.used = 0;
                }
                let (rate, parallel) = if key.0 { (600, 4) } else { (120, 2) };
                if c.used >= rate || c.active >= parallel {
                    return Err(limited());
                }
            }
        }
        for key in &keys {
            let c = counters.entry(*key).or_insert(Counter {
                since: now,
                used: 0,
                active: 0,
            });
            c.used += 1;
            c.active += 1;
        }
        Ok(ReadPermit {
            counters: self.counters.clone(),
            keys,
            _permit: permit,
        })
    }
    pub(crate) async fn document(
        &self,
        db: &mut sqlx::PgConnection,
        actor: Uuid,
        document: Uuid,
    ) -> Result<ReadPermit, ApiError> {
        let project: Option<Uuid> = sqlx::query_scalar("select document_project($1)")
            .bind(document)
            .fetch_one(db)
            .await?;
        self.enter(actor, Some(project.unwrap_or(document)))
    }
}
/// Bound active response bodies, including slow readers. SSE uses a separate pool,
/// so idle subscriptions do not starve edits. GET setup has a cooperative deadline;
/// writes are not cancelled after an ambiguous commit. Database deadlines are separate.
pub(crate) async fn admission(
    State(budgets): State<Workloads>,
    request: Request,
    next: Next,
) -> Response {
    if request.uri().path() == "/healthz" {
        return next.run(request).await;
    }
    let stream = request.uri().path().starts_with("/documents/")
        && request.uri().path().ends_with("/stream");
    let permits = if stream {
        &budgets.streams
    } else {
        &budgets.requests
    };
    let Ok(permit) = permits.clone().try_acquire_owned() else {
        return limited().into_response();
    };
    let read = request.method() == Method::GET || request.method() == Method::HEAD;
    let response = if read {
        match tokio::time::timeout(Duration::from_secs(30), next.run(request)).await {
            Ok(response) => response,
            Err(_) => ApiError::Deadline.into_response(),
        }
    } else {
        next.run(request).await
    };
    let (parts, body) = response.into_parts();
    let body = futures::stream::unfold(
        (body.into_data_stream(), permit),
        |(mut body, permit)| async move { body.next().await.map(|chunk| (chunk, (body, permit))) },
    );
    Response::from_parts(parts, Body::from_stream(body.fuse()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{middleware, routing::get, Router};
    use tower::ServiceExt;
    #[test]
    fn actor_project_limits_and_expiry_release() {
        let b = Workloads::default();
        let a = Uuid::new_v4();
        let p = Uuid::new_v4();
        let first = b.enter(a, Some(p)).unwrap();
        let second = b.enter(a, Some(p)).unwrap();
        assert!(b.enter(a, None).is_err());
        let third = b.enter(Uuid::new_v4(), Some(p)).unwrap();
        let fourth = b.enter(Uuid::new_v4(), Some(p)).unwrap();
        assert!(b.enter(Uuid::new_v4(), Some(p)).is_err());
        drop((first, second, third, fourth));
        for _ in 2..120 {
            drop(b.enter(a, Some(p)).unwrap());
        }
        assert!(b.enter(a, None).is_err());
        b.counters
            .lock()
            .unwrap()
            .get_mut(&(false, a))
            .unwrap()
            .since -= WINDOW;
        assert!(b.enter(a, Some(p)).is_ok());
    }
    #[test]
    fn registry_stays_bounded_and_does_not_evict_active_guards() {
        let b = Workloads::default();
        {
            let mut m = b.counters.lock().unwrap();
            for _ in 0..MAX_KEYS {
                m.insert(
                    (false, Uuid::new_v4()),
                    Counter {
                        since: Instant::now(),
                        used: 1,
                        active: 0,
                    },
                );
            }
        }
        assert!(b.enter(Uuid::new_v4(), None).is_err());
        for c in b.counters.lock().unwrap().values_mut() {
            c.since -= WINDOW;
        }
        assert!(b.enter(Uuid::new_v4(), None).is_ok());
        assert!(b.counters.lock().unwrap().len() <= MAX_KEYS);
    }
    #[tokio::test]
    async fn response_drop_releases_admission_and_health_and_streams_are_independent() {
        let b = Workloads::default();
        let router = Router::new()
            .route("/read", get(|| async { "ok" }))
            .route("/healthz", get(|| async { "ok" }))
            .route("/documents/{id}/stream", get(|| async { "stream" }))
            .layer(middleware::from_fn_with_state(b.clone(), admission));
        let request = |path| Request::builder().uri(path).body(Body::empty()).unwrap();
        let mut held = Vec::new();
        for _ in 0..64 {
            held.push(router.clone().oneshot(request("/read")).await.unwrap());
        }
        assert_eq!(
            router
                .clone()
                .oneshot(request("/read"))
                .await
                .unwrap()
                .status(),
            429
        );
        assert_eq!(
            router
                .clone()
                .oneshot(request("/healthz"))
                .await
                .unwrap()
                .status(),
            200
        );
        assert_eq!(
            router
                .clone()
                .oneshot(request("/documents/x/stream"))
                .await
                .unwrap()
                .status(),
            200
        );
        drop(held.pop());
        assert_eq!(
            router.oneshot(request("/read")).await.unwrap().status(),
            200
        );
    }
    #[tokio::test]
    async fn completed_body_can_be_polled_again_and_releases_its_slot() {
        let b = Workloads::default();
        let router = Router::new()
            .route("/read", get(|| async { "ok" }))
            .layer(middleware::from_fn_with_state(b.clone(), admission));
        let response = router
            .oneshot(Request::builder().uri("/read").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let mut body = response.into_body().into_data_stream();
        assert!(body.next().await.unwrap().is_ok());
        assert!(body.next().await.is_none());
        // HTTP clients may poll again after completion; unfold alone panics here.
        assert!(body.next().await.is_none());
        assert_eq!(b.requests.available_permits(), 64);
    }
}
