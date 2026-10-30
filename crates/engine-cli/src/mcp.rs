//! `engine-cli mcp`: the MCP server for AI agents, over standard input and output. Read-only
//! unless `--write` is given.
//!
//! The protocol, the tools and the HTTP calls are the `engine-mcp` library
//! (docs/AI-AGENT-CONNECTORS.md section 11.2). This module supplies what that library
//! deliberately lacks: the credential. It loads the agent's key file like every other
//! command (`signing::load`, so encrypted `dynodoc-key-v1` files work, with the passphrase
//! from `--passphrase-file` or `DYNODOC_KEY_PASSPHRASE_FILE`), and obtains a project device
//! session with the `content:read` scope through the challenge protocol of
//! docs/PROJECT-SCOPED-DEVICE-SESSIONS.md (Path A of the connector design, section 4.3).
//! The one-hour token is held in memory only and renewed shortly before it expires; nothing
//! is written to disk and nothing but the token's owner (this process) ever sees it.
//!
//! **Writing is opt-in.** Without `--write` the session asks for `content:read` and for
//! nothing else, and the server lists six read tools. With it the session also asks for
//! `content:propose` and `content:submit` (the service refuses a write scope without the
//! read scope beside it), and the server lists `propose` and `submit` too. If the service
//! refuses that set because the key was not granted the write scopes, the session falls back
//! to `content:read` alone, says so on standard error, and marks its tokens read-only: the
//! write tools then answer with a plain refusal and the read tools keep working. Nothing here
//! can merge or approve: an agent only proposes, and a person reviews.
//!
//! Standard output carries protocol messages and nothing else. Logs go to standard error
//! (`main` points the subscriber there for this verb), the passphrase prompt writes to
//! standard error and appears only when standard input is a terminal, which it never is
//! under an MCP client, and nothing in this module prints.
#![deny(clippy::print_stdout, clippy::print_stderr, clippy::dbg_macro)]

use std::{
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{ensure, Context};
use clap::Args;
use engine_core::repository::device_session::{SessionChallenge, SessionProof, TOKEN_TTL_SECONDS};
use engine_mcp::{
    parse_base, BackendConfig, HttpBackend, Profile, Server, Session, SessionError, Token,
};
use reqwest::{Method, StatusCode, Url};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::{de::DeserializeOwned, Deserialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::sync::http::Remote;

/// The scope of an agent key's read session (protocol spec 14.2). The service refuses it
/// for a key that was not enrolled as an agent key and granted this project.
const SCOPES: [&str; 1] = ["content:read"];
/// With `--write`: the write scopes, each held beside the read scope (a preview of a proposal
/// shows the text it changes, so the service refuses a write scope without `content:read`).
const WRITE_SCOPES: [&str; 3] = ["content:read", "content:propose", "content:submit"];
/// Renew this long before the stated expiry (a quarter of the lifetime at most).
const RENEW_MARGIN: Duration = Duration::from_secs(120);
/// A token the service rejects this soon after issuing it is rejected for a reason a new
/// one will not fix (a revoked grant, a key of the wrong class): do not ask again.
const MIN_RENEW_GAP: Duration = Duration::from_secs(3);

#[derive(Args)]
pub struct Options {
    /// Independently prepared JSON with version, url, authority and public_key: the file
    /// `sync --remote` reads.
    #[arg(long)]
    remote: PathBuf,
    /// The project this server reads. The key must be enrolled for an AI agent and granted
    /// this project with the content:read scope in your profile (signing keys).
    #[arg(long)]
    project: Uuid,
    /// Enrolled Ed25519 agent key file: unencrypted DER PKCS#8 or an encrypted
    /// dynodoc-key-v1 file (passphrase from --passphrase-file, the
    /// DYNODOC_KEY_PASSPHRASE_FILE variable, or an OS keystore binding; there is no prompt
    /// when standard input is the MCP stream). Obtains and renews one-hour tokens in
    /// memory. On Unix a key readable by group or others is refused.
    #[arg(long)]
    device_key: PathBuf,
    /// Synthetic local tests only. HTTPS is required for other addresses.
    #[arg(long)]
    allow_loopback_http: bool,
    /// Also offer the tools propose and submit. The session then asks for content:propose and
    /// content:submit besides content:read, so the key must have been granted them in your
    /// profile (signing keys) and the project's write_mode must not be none. An agent only
    /// proposes: a person reviews and merges, and nothing it sends is live before that. Off
    /// unless given; the variable DYNODOC_WRITE set to on, yes, true or 1 has the same effect.
    #[arg(long)]
    write: bool,
}

/// `DYNODOC_WRITE` as an operator would set it: on, yes, true or 1 turn writing on. Anything
/// else (off, no, false, 0, empty, a word nobody expects) leaves it off, so a mistake errs
/// toward the read-only server. The Claude Code plugin passes this variable through.
fn write_from_environment() -> bool {
    write_wanted(std::env::var("DYNODOC_WRITE").ok().as_deref())
}

fn write_wanted(value: Option<&str>) -> bool {
    value.is_some_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "on" | "yes" | "true"
        )
    })
}

fn read_capped(path: &Path, cap: usize) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(cap as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= cap, "input exceeds byte limit");
    Ok(bytes)
}

pub async fn run(options: Options) -> anyhow::Result<()> {
    let write = options.write || write_from_environment();
    ensure!(!write, "MCP write tools require a trusted per-request human confirmation surface; this CLI currently supports read-only access");
    let remote: Remote = serde_json::from_slice(
        &read_capped(&options.remote, 8192).context("cannot read the --remote file")?,
    )
    .context("the --remote file is not valid")?;
    ensure!(
        remote.version == 1 && !remote.authority.is_nil() && !options.project.is_nil(),
        "invalid remote scope/version"
    );
    let base = parse_base(&remote.url, options.allow_loopback_http)?;
    // Before anything is served: a wrong path or passphrase should stop the launch with a
    // message on stderr, not surface as a tool error on every call.
    let key = crate::signing::load(&options.device_key)?;
    let owned = |scopes: &[&str]| scopes.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let mut session = DeviceSession::new(
        base.clone(),
        options.project,
        remote.authority,
        key,
        owned(if write { &WRITE_SCOPES } else { &SCOPES }),
    )?;
    if write {
        session = session.with_read_fallback(owned(&SCOPES));
    }
    let backend = HttpBackend::new(BackendConfig::new(base, options.project), session)?;
    let (profile, mode) = if write {
        (Profile::full(), "read and propose")
    } else {
        (Profile::read_only(), "read-only")
    };
    tracing::info!(project = %options.project, mode, "dynodoc MCP server ready on stdio");
    Server::new(backend, profile)
        .serve_stdio()
        .await
        .context("standard input/output failed")?;
    tracing::info!("standard input closed; stopping");
    Ok(())
}

fn lower_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

struct State {
    /// The service refused the write scopes and the session runs on the read scope alone.
    read_only: bool,
    token: Option<Token>,
    /// When to replace the token before it is refused.
    renew_at: Option<Instant>,
    issued_at: Option<Instant>,
    /// Incremented per issue, so that a call whose request was rejected can tell whether
    /// another call has already replaced the token.
    generation: u64,
}

/// A project device session kept in memory for one agent key.
struct DeviceSession {
    http: reqwest::Client,
    base: Url,
    project: Uuid,
    authority: Uuid,
    key: Ed25519KeyPair,
    scopes: Vec<String>,
    /// What to ask for instead when the service refuses `scopes` (the read scope, for a
    /// session that asked for the write scopes); `None` for a session with nothing to fall
    /// back to.
    fallback: Option<Vec<String>>,
    /// See [`MIN_RENEW_GAP`]; a field so that the tests need not wait for it.
    renew_gap: Duration,
    /// Held across the network round trip on purpose: concurrent callers wait for one
    /// renewal instead of each asking for a session.
    state: tokio::sync::Mutex<State>,
}

impl DeviceSession {
    fn new(
        base: Url,
        project: Uuid,
        authority: Uuid,
        key: Ed25519KeyPair,
        scopes: Vec<String>,
    ) -> anyhow::Result<Self> {
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent("dynodoc-engine-cli-mcp/0.1")
            .build()?;
        Ok(Self {
            http,
            base,
            project,
            authority,
            key,
            scopes,
            fallback: None,
            renew_gap: MIN_RENEW_GAP,
            state: tokio::sync::Mutex::new(State {
                read_only: false,
                token: None,
                renew_at: None,
                issued_at: None,
                generation: 0,
            }),
        })
    }

    /// Ask for `scopes` instead when the service refuses the ones this session wants.
    fn with_read_fallback(mut self, scopes: Vec<String>) -> Self {
        self.fallback = Some(scopes);
        self
    }

    /// An unauthenticated JSON `POST` below the API base. A refusal by the service (any
    /// `4xx` but a timeout or a rate limit) is final; anything else may pass.
    async fn post<T: DeserializeOwned>(&self, path: &str, body: &Value) -> Result<T, SessionError> {
        let unavailable = |what: &str| {
            tracing::warn!(what, "device-session request failed");
            SessionError::Unavailable
        };
        let url = self
            .base
            .join(path)
            .map_err(|_| unavailable("bad request URL"))?;
        let mut response = self
            .http
            .request(Method::POST, url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(serde_json::to_vec(body).map_err(|_| unavailable("encode"))?)
            .send()
            .await
            .map_err(|_| unavailable("send"))?;
        let status = response.status();
        if !status.is_success() {
            tracing::warn!(status = status.as_u16(), "device sign-in refused or failed");
            return Err(
                if status.is_client_error()
                    && status != StatusCode::TOO_MANY_REQUESTS
                    && status != StatusCode::REQUEST_TIMEOUT
                {
                    SessionError::Refused
                } else {
                    SessionError::Unavailable
                },
            );
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| unavailable("read"))? {
            if bytes.len() + chunk.len() > 16 * 1024 {
                return Err(unavailable("response too large"));
            }
            bytes.extend_from_slice(&chunk);
        }
        serde_json::from_slice(&bytes).map_err(|_| unavailable("response is not valid"))
    }

    /// One challenge and its answer. The challenge is checked against the independently
    /// configured installation, project, key and scopes before anything is signed.
    async fn obtain(&self, scopes: &[String]) -> Result<(String, Duration), SessionError> {
        #[derive(Deserialize)]
        struct Issued {
            challenge: SessionChallenge,
            mac: String,
        }
        #[derive(Deserialize)]
        struct Granted {
            access_token: String,
            expires_in: u64,
        }
        let issued: Issued = self
            .post(
                &format!("projects/{}/device-sessions/challenge", self.project),
                &json!({
                    "public_key": lower_hex(self.key.public_key().as_ref()),
                    "scopes": scopes,
                }),
            )
            .await?;
        let proof = SessionProof::sign(
            issued.challenge,
            issued.mac,
            self.authority,
            self.project,
            scopes,
            &self.key,
        )
        .map_err(|_| {
            tracing::error!(
                "the device-session challenge differs from the configured installation, \
                 project, key or scopes; nothing was signed"
            );
            SessionError::Refused
        })?;
        let granted: Granted = self
            .post(
                &format!("projects/{}/device-sessions", self.project),
                &json!(proof),
            )
            .await?;
        if granted.expires_in == 0 || granted.expires_in > TOKEN_TTL_SECONDS as u64 {
            tracing::error!("unexpected device-session lifetime");
            return Err(SessionError::Unavailable);
        }
        Ok((
            granted.access_token,
            Duration::from_secs(granted.expires_in),
        ))
    }

    async fn renew(&self, state: &mut State) -> Result<(), SessionError> {
        let fallback = self.fallback.as_ref();
        let (secret, lifetime) = match fallback.filter(|_| state.read_only) {
            Some(read) => self.obtain(read).await?,
            None => match (self.obtain(&self.scopes).await, fallback) {
                // The service will not issue the write scopes to this key (not granted, or a
                // project that does not allow them): run on the read scope and say so.
                (Err(SessionError::Refused), Some(read)) => {
                    tracing::warn!(
                        "no session with the write scopes; serving read-only. Grant the key \
                         content:propose and content:submit, then restart"
                    );
                    state.read_only = true;
                    self.obtain(read).await?
                }
                (result, _) => result?,
            },
        };
        let margin = RENEW_MARGIN.min(lifetime / 4);
        let now = Instant::now();
        state.generation += 1;
        let mut token = Token::new(secret, state.generation);
        // A token that was not asked to carry the write scopes does not.
        if state.read_only || !self.scopes.iter().any(|s| s == "content:propose") {
            token = token.without_writes();
        }
        state.token = Some(token);
        state.issued_at = Some(now);
        state.renew_at = now.checked_add(lifetime - margin);
        tracing::info!(generation = state.generation, "device session issued");
        Ok(())
    }
}

impl Session for DeviceSession {
    async fn token(&self) -> Result<Token, SessionError> {
        let mut state = self.state.lock().await;
        let stale = state.token.is_none() || state.renew_at.is_some_and(|at| Instant::now() >= at);
        if stale {
            self.renew(&mut state).await?;
        }
        state.token.clone().ok_or(SessionError::Unavailable)
    }

    async fn rejected(&self, rejected: &Token) -> Result<(), SessionError> {
        let mut state = self.state.lock().await;
        if state.generation != rejected.generation() {
            return Ok(()); // another call already replaced it
        }
        if state
            .issued_at
            .is_some_and(|at| at.elapsed() < self.renew_gap)
        {
            tracing::warn!("a new device session was rejected at once; not asking for another");
            return Err(SessionError::Refused);
        }
        self.renew(&mut state).await
    }
}

#[cfg(test)]
mod tests;
