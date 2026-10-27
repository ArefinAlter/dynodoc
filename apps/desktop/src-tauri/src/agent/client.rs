//! Sending one turn to a provider, from this process only.
//!
//! TLS, timeouts, retries with backoff on 429 and 5xx (before any text has streamed), a size limit on
//! what is sent and on what is read, and cancellation. The key goes into one header and nowhere
//! else: an error never holds a header, a body or an address with a query, and what a provider says
//! is cut to a few hundred characters with the key taken out of it.

#[path = "../../../../../shared/local_agent.rs"]
pub mod local_agent;

use super::sse::SseDecoder;
use super::types::{AgentError, ApiKey, Endpoint, Turn};
use super::wire::{self, Built};
use futures_util::StreamExt;
use std::sync::OnceLock;
use std::time::Duration;
use tokio::sync::watch;

/// The largest request body the app sends.
pub const MAX_REQUEST_BYTES: usize = 2 * 1024 * 1024;
/// The most text one turn may stream before the app stops reading.
pub const MAX_TEXT_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub connect: Duration,
    /// How long the stream may stay silent.
    pub idle: Duration,
    pub total: Duration,
    pub retries: u32,
    pub base_backoff: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            connect: Duration::from_secs(10),
            idle: Duration::from_secs(60),
            total: Duration::from_secs(240),
            retries: 3,
            base_backoff: Duration::from_millis(600),
        }
    }
}

fn http(loopback: bool, connect: Duration) -> Option<reqwest::Client> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut b = reqwest::Client::builder()
        .user_agent(format!("Dynodoc/{} (agent)", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(connect);
    if loopback {
        b = b.no_proxy();
    }
    b.build().ok()
}

fn shared(loopback: bool) -> Option<&'static reqwest::Client> {
    static NORMAL: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    static LOCAL: OnceLock<Option<reqwest::Client>> = OnceLock::new();
    let cell = if loopback { &LOCAL } else { &NORMAL };
    cell.get_or_init(|| http(loopback, Duration::from_secs(10)))
        .as_ref()
}

/// How long to wait before attempt `n` (0 is the first retry): the provider's own `retry-after` when
/// it gave one (at most 30 s), otherwise doubling from the base with a little jitter.
pub fn backoff(n: u32, retry_after: Option<Duration>, base: Duration, jitter: u64) -> Duration {
    if let Some(r) = retry_after {
        return r.min(Duration::from_secs(30));
    }
    let doubled = base.saturating_mul(1u32 << n.min(6));
    (doubled + Duration::from_millis(jitter % 250)).min(Duration::from_secs(20))
}

fn jitter() -> u64 {
    let mut b = [0u8; 8];
    let _ = getrandom::fill(&mut b);
    u64::from_le_bytes(b)
}

/// The provider's own words for an error, shortened and without the key.
pub fn provider_message(body: &str, key: Option<&str>) -> String {
    let parsed: Option<serde_json::Value> = serde_json::from_str(body).ok();
    let text = parsed
        .as_ref()
        .and_then(|v| {
            v["error"]["message"]
                .as_str()
                .or_else(|| v["message"].as_str())
                .or_else(|| v["error"].as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.lines().next().unwrap_or("").to_string());
    let mut out: String = text.chars().filter(|c| !c.is_control()).take(300).collect();
    if let Some(k) = key {
        if k.len() >= 6 {
            out = out.replace(k, "[key]");
        }
    }
    out
}

pub fn classify(
    status: u16,
    body: &str,
    retry_after: Option<Duration>,
    key: Option<&str>,
) -> AgentError {
    let message = provider_message(body, key);
    match status {
        429 => AgentError::RateLimited {
            retry_after_ms: retry_after.map(|d| d.as_millis() as u64),
        },
        401 | 403 => AgentError::Rejected(
            format!("the key was not accepted ({status}). {message}")
                .trim()
                .to_string(),
        ),
        404 => AgentError::Rejected(
            format!("the model or the address was not found ({status}). {message}")
                .trim()
                .to_string(),
        ),
        400 | 402 | 413 | 422 => AgentError::Rejected(format!("{message} ({status})")),
        s => AgentError::Provider { status: s, message },
    }
}

fn retryable(status: u16) -> bool {
    matches!(status, 429 | 500 | 502 | 503 | 504 | 529)
}

fn retry_after_of(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|s| *s >= 0.0)
        .map(Duration::from_secs_f64)
}

/// One turn. `on_text` receives the text as it streams. Returns the turn, or what went wrong.
pub async fn send_turn(
    ep: &Endpoint,
    key: Option<&ApiKey>,
    built: &Built,
    mut cancel: watch::Receiver<bool>,
    limits: Limits,
    on_text: &mut (dyn FnMut(&str) + Send),
) -> Result<Turn, AgentError> {
    if local_agent::agent_for_url(&built.url).is_some() {
        if *cancel.borrow() {
            return Err(AgentError::Cancelled);
        }
        let mut v = tokio::select! {
            v = local_agent::complete(&built.url, &built.body, limits.total) => v.map_err(|e| AgentError::BadSetup(e.to_string()))?,
            _ = cancel.changed() => return Err(AgentError::Cancelled),
        };
        if v["choices"][0]["message"]["content"]
            .as_str()
            .is_some_and(|s| s.len() > MAX_TEXT_BYTES)
        {
            return Err(AgentError::TooLarge);
        }
        v["choices"][0]["delta"] = v["choices"][0]["message"].take();
        let mut reader = wire::reader(super::types::Style::OpenAi);
        reader.feed(
            &super::sse::SseEvent {
                event: None,
                data: v.to_string(),
            },
            &mut |text| on_text(text),
        )?;
        return reader.finish();
    }
    let body = serde_json::to_vec(&built.body)
        .map_err(|_| AgentError::BadSetup("The request could not be made.".into()))?;
    if body.len() > MAX_REQUEST_BYTES {
        return Err(AgentError::TooLarge);
    }
    if ep.kind.needs_key() && key.is_none() {
        return Err(AgentError::NoKey);
    }
    let loopback = super::types::is_loopback_host(ep.base.host_str().unwrap_or(""));
    let client = if limits.connect == Duration::from_secs(10) {
        shared(loopback).cloned()
    } else {
        http(loopback, limits.connect)
    }
    .ok_or_else(|| AgentError::BadSetup("The network client could not start.".into()))?;
    let key_text = key.map(ApiKey::expose);
    let started = tokio::time::Instant::now();
    let mut attempt = 0;
    loop {
        if *cancel.borrow() {
            return Err(AgentError::Cancelled);
        }
        let mut req = client.post(&built.url).body(body.clone());
        for (k, v) in &built.headers {
            req = req.header(*k, v);
        }
        if let (Some((header, prefix)), Some(k)) = (built.key_header, key) {
            req = req.header(header, format!("{prefix}{}", k.expose()));
        }
        let sent = tokio::select! {
            r = req.send() => r,
            _ = cancel.changed() => return Err(AgentError::Cancelled),
            _ = tokio::time::sleep(limits.total.saturating_sub(started.elapsed())) => return Err(AgentError::TimedOut),
        };
        let response = match sent {
            Ok(r) => r,
            Err(e) => {
                if attempt < limits.retries && (e.is_connect() || e.is_timeout()) {
                    let wait = backoff(attempt, None, limits.base_backoff, jitter());
                    attempt += 1;
                    tokio::select! { _ = tokio::time::sleep(wait) => {}, _ = cancel.changed() => return Err(AgentError::Cancelled) }
                    continue;
                }
                return Err(if e.is_timeout() {
                    AgentError::TimedOut
                } else {
                    AgentError::Unreachable
                });
            }
        };
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let retry_after = retry_after_of(response.headers());
            let text = tokio::time::timeout(Duration::from_secs(10), response.text())
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default();
            if retryable(status) && attempt < limits.retries {
                let wait = backoff(attempt, retry_after, limits.base_backoff, jitter());
                attempt += 1;
                tokio::select! { _ = tokio::time::sleep(wait) => {}, _ = cancel.changed() => return Err(AgentError::Cancelled) }
                continue;
            }
            return Err(classify(status, &text, retry_after, key_text));
        }
        // The stream.
        let mut decoder = SseDecoder::default();
        let mut reader = wire::reader(ep.kind.style());
        let mut stream = response.bytes_stream();
        let mut text_bytes = 0usize;
        let mut counted = |t: &str, on_text: &mut (dyn FnMut(&str) + Send)| -> bool {
            text_bytes += t.len();
            on_text(t);
            text_bytes <= MAX_TEXT_BYTES
        };
        let mut too_large = false;
        loop {
            let next = tokio::select! {
                n = tokio::time::timeout(limits.idle, stream.next()) => n,
                _ = cancel.changed() => return Err(AgentError::Cancelled),
            };
            if started.elapsed() > limits.total {
                return Err(AgentError::TimedOut);
            }
            let chunk = match next {
                Err(_) => return Err(AgentError::TimedOut),
                Ok(None) => break,
                Ok(Some(Err(_))) => return Err(AgentError::Unreachable),
                Ok(Some(Ok(c))) => c,
            };
            let events = decoder.push(&chunk).map_err(|_| AgentError::TooLarge)?;
            for ev in events {
                reader.feed(&ev, &mut |t| {
                    if !counted(t, on_text) {
                        too_large = true;
                    }
                })?;
            }
            if too_large {
                return Err(AgentError::TooLarge);
            }
        }
        for ev in decoder.finish() {
            reader.feed(&ev, &mut |t| on_text(t))?;
        }
        return reader.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backoff_follows_retry_after_and_otherwise_doubles() {
        let base = Duration::from_millis(500);
        assert_eq!(
            backoff(0, Some(Duration::from_secs(3)), base, 99),
            Duration::from_secs(3)
        );
        assert_eq!(
            backoff(0, Some(Duration::from_secs(900)), base, 0),
            Duration::from_secs(30)
        );
        assert_eq!(backoff(0, None, base, 0), Duration::from_millis(500));
        assert_eq!(backoff(2, None, base, 0), Duration::from_millis(2000));
        assert!(backoff(30, None, base, 0) <= Duration::from_secs(20));
    }

    #[test]
    fn errors_say_what_the_provider_said_without_the_key() {
        let key = "sk-CANARY-1234567890";
        let body = format!("{{\"error\":{{\"message\":\"Incorrect API key provided: {key}\"}}}}");
        let e = classify(401, &body, None, Some(key));
        let shown = e.to_string();
        assert!(
            shown.contains("not accepted") && !shown.contains("CANARY"),
            "{shown}"
        );
        assert!(matches!(
            classify(429, "{}", Some(Duration::from_secs(2)), None),
            AgentError::RateLimited {
                retry_after_ms: Some(2000)
            }
        ));
        assert!(matches!(
            classify(503, "<html>down</html>", None, None),
            AgentError::Provider { status: 503, .. }
        ));
        assert!(provider_message(&"x".repeat(2000), None).len() <= 300);
    }
}
