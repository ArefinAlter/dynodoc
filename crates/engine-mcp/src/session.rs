//! The credential an HTTP backend presents, and how it is renewed.
//!
//! A [`Session`] hands out the caller's own bearer token and nothing else: this crate never
//! accepts a token from an MCP client, never puts one in a tool argument, a log line or an
//! error, and sends it only to the API origin it was configured with (no redirects are
//! followed). How the token is obtained (for a local agent key: the device-session challenge
//! of docs/PROJECT-SCOPED-DEVICE-SESSIONS.md) is the implementor's business, so this crate
//! needs no key file, no signing code and no database.

use std::{fmt, future::Future, sync::Arc};

/// A bearer token and the generation of the session that issued it.
#[derive(Clone)]
pub struct Token {
    secret: Arc<str>,
    generation: u64,
}

impl Token {
    /// `generation` increases with every renewal, so that a call whose request was
    /// rejected can tell whether another call has already replaced the token.
    pub fn new(secret: impl Into<String>, generation: u64) -> Self {
        Self {
            secret: Arc::from(secret.into()),
            generation,
        }
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The secret, for the `Authorization` header and nothing else.
    pub(crate) fn expose(&self) -> &str {
        &self.secret
    }
}

impl fmt::Debug for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Token(generation {}, <redacted>)", self.generation)
    }
}

/// Why no token could be had. Carries no secret and no server text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionError {
    /// The service refused the credential (revoked key, withdrawn grant, wrong scope), or
    /// a fresh token was rejected at once. Retrying will not help.
    Refused,
    /// The service could not be reached or answered nonsense. Retrying may help.
    Unavailable,
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SessionError::Refused => "the credential was refused",
            SessionError::Unavailable => "the service could not be reached",
        })
    }
}

impl std::error::Error for SessionError {}

/// A source of bearer tokens for one project.
pub trait Session: Send + Sync + 'static {
    /// The token to use now: the current one, or a new one when there is none or it is
    /// about to expire.
    fn token(&self) -> impl Future<Output = Result<Token, SessionError>> + Send;

    /// The API answered `401` to a request made with `rejected`. Replace the token, unless
    /// another call already has (compare generations). The caller retries once.
    fn rejected(&self, rejected: &Token) -> impl Future<Output = Result<(), SessionError>> + Send;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_token_never_shows_in_debug_output() {
        let token = Token::new("v4.local.SECRET-VALUE", 3);
        let shown = format!("{token:?} {:?}", Some(&token));
        assert!(!shown.contains("SECRET"), "{shown}");
        assert!(shown.contains("generation 3"));
    }
}
