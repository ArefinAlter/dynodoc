//! The local person's sign-in to the local engine.
//!
//! There is no web server in front of the local engine, so the app plays the web tier's part: it
//! holds the engine's service credential (in the credential store) and uses the engine's own
//! sign-in route to make a session for the one person of this workspace, exactly as the hosted
//! web tier does for an emailed link. The access token lives in memory; the refresh token is kept
//! in the credential store so a restart needs no new sign-in. Neither is ever shown to the page.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Session {
    access: String,
    refresh: String,
    #[zeroize(skip)]
    expires: Instant,
}

/// What is kept between runs: the refresh token and when the access token ends (unix seconds).
#[derive(Serialize, Deserialize)]
struct Saved {
    refresh: String,
    access: String,
    until: u64,
}

impl Session {
    pub fn access(&self) -> &str {
        &self.access
    }

    /// True when the access token has under a minute left.
    pub fn stale(&self) -> bool {
        Instant::now() + Duration::from_secs(60) >= self.expires
    }

    fn from_tokens(access: &str, refresh: &str, expires_in: u64) -> Self {
        Self {
            access: access.to_string(),
            refresh: refresh.to_string(),
            expires: Instant::now() + Duration::from_secs(expires_in),
        }
    }

    pub fn to_saved_text(&self) -> String {
        let left = self
            .expires
            .saturating_duration_since(Instant::now())
            .as_secs();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        serde_json::to_string(&Saved {
            refresh: self.refresh.clone(),
            access: self.access.clone(),
            until: now + left,
        })
        .unwrap_or_default()
    }

    pub fn from_saved_text(text: &str) -> Option<Self> {
        let saved: Saved = serde_json::from_str(text).ok()?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        Some(Self::from_tokens(
            &saved.access,
            &saved.refresh,
            saved.until.saturating_sub(now),
        ))
    }
}

/// One answer of the engine's sign-in routes.
#[derive(Debug)]
pub enum Refused {
    /// The engine did not answer.
    Unreachable,
    /// The engine said no (a bad or used token, a rate limit).
    No,
}

fn tokens_of(body: &Value) -> Option<Session> {
    Some(Session::from_tokens(
        body.get("access_token")?.as_str()?,
        body.get("refresh_token")?.as_str()?,
        body.get("expires_in")?.as_u64().unwrap_or(3600),
    ))
}

/// Signs in the person of this workspace with the service credential: a one-time link token is
/// made for the address and redeemed at once. The link token never leaves this function.
pub async fn mint(
    http: &reqwest::Client,
    base: &str,
    service_key: &str,
    email: &str,
    display_name: &str,
) -> Result<Session, Refused> {
    let link: Value = http
        .post(format!("{base}/auth/magic-link"))
        .header("x-dynodoc-service-key", service_key)
        .json(&json!({ "email": email, "display_name": display_name }))
        .send()
        .await
        .map_err(|_| Refused::Unreachable)
        .and_then(|r| {
            if r.status().is_success() {
                Ok(r)
            } else {
                Err(Refused::No)
            }
        })?
        .json()
        .await
        .map_err(|_| Refused::Unreachable)?;
    let token = link
        .get("magic_link_token")
        .and_then(Value::as_str)
        .ok_or(Refused::No)?
        .to_string();
    let response = http
        .post(format!("{base}/auth/verify"))
        .json(&json!({ "token": token }))
        .send()
        .await
        .map_err(|_| Refused::Unreachable)?;
    if !response.status().is_success() {
        return Err(Refused::No);
    }
    let body: Value = response.json().await.map_err(|_| Refused::Unreachable)?;
    tokens_of(&body).ok_or(Refused::No)
}

/// Trades the refresh token for a new pair (the engine rotates it).
pub async fn refresh(
    http: &reqwest::Client,
    base: &str,
    session: &Session,
) -> Result<Session, Refused> {
    let response = http
        .post(format!("{base}/auth/refresh"))
        .json(&json!({ "refresh_token": session.refresh }))
        .send()
        .await
        .map_err(|_| Refused::Unreachable)?;
    if !response.status().is_success() {
        return Err(Refused::No);
    }
    let body: Value = response.json().await.map_err(|_| Refused::Unreachable)?;
    tokens_of(&body).ok_or(Refused::No)
}

/// A name and an address a person typed, checked for what the engine accepts.
pub fn valid_profile(name: &str, email: &str) -> Result<(String, String), &'static str> {
    let name = name.trim();
    let email = email.trim().to_lowercase();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err("Enter a name of up to 80 characters.");
    }
    let ok = email.len() <= 254
        && email.matches('@').count() == 1
        && !email.starts_with('@')
        && !email.ends_with('@')
        && email
            .rsplit('@')
            .next()
            .is_some_and(|d| d.contains('.') && !d.starts_with('.') && !d.ends_with('.'))
        && email.bytes().all(|b| b.is_ascii_graphic());
    if !ok {
        return Err(
            "Enter an email address (it stays on this computer unless you sign in to a server).",
        );
    }
    Ok((name.to_string(), email))
}

/// An address for a person who gave none: it is only a key in the local database.
pub fn local_address(random_hex: &str) -> String {
    format!(
        "me-{}@local.dynodoc.invalid",
        &random_hex[..random_hex.len().min(12)]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_session_round_trips_and_goes_stale_near_its_end() {
        let s = Session::from_tokens("access-token-value", "refresh-token-value", 3600);
        assert!(!s.stale());
        let again = Session::from_saved_text(&s.to_saved_text()).unwrap();
        assert_eq!(again.access(), "access-token-value");
        assert!(!again.stale());
        let old = Session::from_tokens("a", "b", 30);
        assert!(old.stale());
        assert!(Session::from_saved_text("not json").is_none());
    }

    #[test]
    fn profiles_are_checked() {
        assert_eq!(
            valid_profile("  Priya Raman ", " Priya@Example.org ").unwrap(),
            ("Priya Raman".to_string(), "priya@example.org".to_string())
        );
        assert!(valid_profile("", "a@b.co").is_err());
        assert!(valid_profile("X", "no-at.example.org").is_err());
        assert!(valid_profile("X", "a@b").is_err());
        assert!(valid_profile("X", "a b@example.org").is_err());
        assert!(valid_profile(&"x".repeat(81), "a@b.co").is_err());
        assert!(local_address("0123456789abcdef").ends_with("@local.dynodoc.invalid"));
    }
}
