//! Explicit origins, no redirects/proxy discovery, bounded bodies and deadlines.
//! Credentials renew on `401` at most once per request: from a refresh token file
//! (atomic rotation) or by signing a project-device challenge with an enrolled key.
//! Access tokens are held in memory only and marked sensitive.
use super::*;
use engine_core::repository::{
    authority_epoch::{Chain, Revocation, Served, SignedTransition},
    device_session::{SessionChallenge, SessionProof, TOKEN_TTL_SECONDS},
};
use reqwest::{header, Method, StatusCode, Url};
use ring::signature::{Ed25519KeyPair, KeyPair};
use serde::de::DeserializeOwned;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

/// Renew this long before a known expiry rather than failing a request.
const RENEW_MARGIN: Duration = Duration::from_secs(120);
/// Longest server-claimed token lifetime honoured for proactive renewal.
const MAX_LIFETIME: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// A run that needs more renewals than this is looping, not expiring.
const MAX_RENEWALS: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Remote {
    pub version: u8,
    /// API base, including a path prefix if required. Never a credential-bearing URL.
    pub url: String,
    pub authority: Uuid,
    /// Obtain this key independently of the server's authority endpoint.
    pub public_key: String,
}

/// Where the bearer comes from and how it is renewed.
pub enum Credential {
    /// One fixed token. An expired token stops the run.
    Static,
    /// Rotate the account refresh token on `401`; both files are replaced atomically.
    Refresh {
        refresh_file: PathBuf,
        access_file: Option<PathBuf>,
    },
    /// Sign a one-use challenge with an enrolled device key for a project token.
    Device {
        key: Ed25519KeyPair,
        scopes: Vec<String>,
    },
}

struct Session {
    header: Option<header::HeaderValue>,
    /// When to renew before the first rejection, if the server stated a lifetime.
    renew_at: Option<Instant>,
    /// Incremented per renewal, so a request that saw an old token does not renew again.
    epoch: u64,
}

pub struct Client {
    pub remote: Remote,
    pub project: Uuid,
    http: reqwest::Client,
    base: Url,
    credential: Credential,
    session: Mutex<Session>,
    renewals: AtomicUsize,
    requests: AtomicUsize,
    transferred: AtomicUsize,
    started: Instant,
    /// Epoch chain served by the remote, fetched once per run and verified locally.
    server: tokio::sync::OnceCell<Served>,
    /// Explicit, independently obtained key accepted across a recorded break.
    break_key: Option<String>,
}

fn bearer(token: &str) -> anyhow::Result<header::HeaderValue> {
    ensure!(
        !token.is_empty() && token.len() <= 16384 && !token.chars().any(char::is_whitespace),
        "invalid bearer token"
    );
    let mut authorization = header::HeaderValue::from_str(&format!("Bearer {token}"))
        .map_err(|_| anyhow!("invalid bearer token"))?;
    authorization.set_sensitive(true);
    Ok(authorization)
}

fn lower_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn is_secret(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}

impl Client {
    /// A fixed bearer, as used by tests and by an externally managed token file.
    pub fn new(remote: Remote, project: Uuid, token: &str, loopback: bool) -> anyhow::Result<Self> {
        Self::with_credential(remote, project, Credential::Static, Some(token), loopback)
    }
    pub fn with_credential(
        remote: Remote,
        project: Uuid,
        credential: Credential,
        initial: Option<&str>,
        loopback: bool,
    ) -> anyhow::Result<Self> {
        ensure!(
            remote.version == 1 && !remote.authority.is_nil() && !project.is_nil(),
            "invalid remote scope/version"
        );
        // Address and public key have the same canonical 32-byte hex encoding.
        Address::try_from(remote.public_key.clone())?;
        let base = Url::parse(&remote.url)?;
        let local = base.host_str().is_some_and(|host| {
            host.trim_matches(['[', ']'])
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
        });
        ensure!(
            base.scheme() == "https" || (loopback && local && base.scheme() == "http"),
            "HTTPS is required (test HTTP needs a literal loopback IP and --allow-loopback-http)"
        );
        ensure!(
            base.username().is_empty()
                && base.password().is_none()
                && base.query().is_none()
                && base.fragment().is_none(),
            "remote URL must not contain credentials, query or fragment"
        );
        ensure!(
            base.path().ends_with('/'),
            "remote API base must end with /"
        );
        let header = match initial {
            Some(token) => Some(bearer(token).map_err(|_| anyhow!("invalid bearer token file"))?),
            None => None,
        };
        ensure!(
            header.is_some() || !matches!(credential, Credential::Static),
            "invalid bearer token file"
        );
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(30))
            .user_agent("dynodoc-engine-cli/0.1")
            .build()?;
        Ok(Self {
            remote,
            project,
            http,
            base,
            credential,
            session: Mutex::new(Session {
                header,
                renew_at: None,
                epoch: 0,
            }),
            renewals: AtomicUsize::new(0),
            requests: AtomicUsize::new(0),
            transferred: AtomicUsize::new(0),
            started: Instant::now(),
            server: tokio::sync::OnceCell::new(),
            break_key: None,
        })
    }
    /// Trust `key` as the successor across a recorded break for this run. Obtain it
    /// independently of the server; the pinned receipt chain must still be extended.
    pub fn accepting_break(mut self, key: String) -> anyhow::Result<Self> {
        Address::try_from(key.clone())?;
        self.break_key = Some(key);
        Ok(self)
    }
    pub fn break_key(&self) -> Option<&str> {
        self.break_key.as_deref()
    }
    fn renewable(&self) -> bool {
        !matches!(self.credential, Credential::Static)
    }
    fn project_url(&self, path: &str) -> anyhow::Result<Url> {
        Ok(self
            .base
            .join(&format!("projects/{}/{path}", self.project))?)
    }
    /// The current bearer, renewing first when it is absent or about to expire.
    async fn current(&self) -> anyhow::Result<(header::HeaderValue, u64)> {
        let mut session = self.session.lock().await;
        let stale =
            session.header.is_none() || session.renew_at.is_some_and(|at| Instant::now() >= at);
        if stale && self.renewable() {
            self.renew(&mut session).await?;
        }
        let header = session
            .header
            .clone()
            .ok_or_else(|| anyhow!("no credential available"))?;
        Ok((header, session.epoch))
    }
    /// A `401` for a token of this epoch: renew unless another request already did.
    async fn rejected(&self, seen: u64) -> anyhow::Result<()> {
        let mut session = self.session.lock().await;
        if session.epoch == seen {
            self.renew(&mut session).await?;
        }
        Ok(())
    }
    async fn renew(&self, session: &mut Session) -> anyhow::Result<()> {
        ensure!(
            self.renewals.fetch_add(1, Ordering::Relaxed) < MAX_RENEWALS,
            "credential renewal limit reached; sign in again"
        );
        let (token, lifetime) = match &self.credential {
            Credential::Static => anyhow::bail!("the access token expired; obtain a fresh token"),
            Credential::Refresh {
                refresh_file,
                access_file,
            } => self.refresh(refresh_file, access_file.as_deref()).await?,
            Credential::Device { key, scopes } => self.device_token(key, scopes).await?,
        };
        // Any rotated secret is already saved. A hostile lifetime can neither
        // overflow `Instant` nor force renewal on every request.
        let renew_at = match lifetime.filter(|l| !l.is_zero()) {
            Some(lifetime) => {
                let lifetime = lifetime.min(MAX_LIFETIME);
                let margin = RENEW_MARGIN.min(lifetime / 4);
                Some(
                    Instant::now()
                        .checked_add(lifetime - margin)
                        .ok_or_else(|| anyhow!("unexpected token lifetime from the server"))?,
                )
            }
            None => None,
        };
        session.header = Some(bearer(&token)?);
        session.renew_at = renew_at;
        session.epoch += 1;
        Ok(())
    }
    /// Rotate the refresh secret. The replacement files are prepared first; the
    /// rotated secret is persisted before anything else can fail.
    async fn refresh(
        &self,
        refresh_file: &Path,
        access_file: Option<&Path>,
    ) -> anyhow::Result<(String, Option<Duration>)> {
        let presented = String::from_utf8(read(refresh_file, 1024)?)?;
        let presented = presented.trim();
        ensure!(is_secret(presented), "invalid refresh token file");
        let refresh_out = crate::private_file::Replacement::prepare(refresh_file)?;
        let access_out = access_file
            .map(crate::private_file::Replacement::prepare)
            .transpose()?;
        #[derive(Deserialize)]
        struct Rotated {
            access_token: String,
            refresh_token: String,
            expires_in: Option<u64>,
        }
        let rotated: Rotated = self
            .anonymous(
                "auth/refresh",
                &json!({ "refresh_token": presented }),
                16 * 1024,
            )
            .await
            .map_err(|e| anyhow!("refresh token rejected or unavailable ({e}); sign in again"))?;
        ensure!(
            is_secret(&rotated.refresh_token),
            "unexpected refresh response; sign in again"
        );
        // The server has already retired the presented secret: persist its
        // successor before anything else can fail.
        refresh_out
            .commit(rotated.refresh_token.as_bytes())
            .context(
                "the server rotated the refresh token but it could not be saved; sign in again",
            )?;
        bearer(&rotated.access_token)?;
        if let Some(access_out) = access_out {
            access_out.commit(rotated.access_token.as_bytes())?;
        }
        Ok((
            rotated.access_token,
            rotated.expires_in.map(Duration::from_secs),
        ))
    }
    /// One challenge/response with the enrolled key. The challenge is verified
    /// against the independently configured installation, project and scopes
    /// before anything is signed.
    async fn device_token(
        &self,
        key: &Ed25519KeyPair,
        scopes: &[String],
    ) -> anyhow::Result<(String, Option<Duration>)> {
        #[derive(Deserialize)]
        struct Issued {
            challenge: SessionChallenge,
            mac: String,
        }
        let issued: Issued = self
            .anonymous(
                &format!("projects/{}/device-sessions/challenge", self.project),
                &json!({"public_key":lower_hex(key.public_key().as_ref()),"scopes":scopes}),
                8 * 1024,
            )
            .await?;
        let proof = SessionProof::sign(
            issued.challenge,
            issued.mac,
            self.remote.authority,
            self.project,
            scopes,
            key,
        )
        .map_err(|_| anyhow!("device-session challenge differs from the configured installation, project, key or scopes; nothing was signed"))?;
        #[derive(Deserialize)]
        struct Granted {
            access_token: String,
            expires_in: u64,
        }
        let session: Granted = self
            .anonymous(
                &format!("projects/{}/device-sessions", self.project),
                &json!(proof),
                16 * 1024,
            )
            .await
            .map_err(|e| anyhow!("device sign-in refused ({e}); check that the key is enrolled and unrevoked, that the account can still access the project, and that the key is granted this project and these scopes in your profile's signing keys"))?;
        ensure!(
            session.expires_in > 0 && session.expires_in <= TOKEN_TTL_SECONDS as u64,
            "unexpected device-session lifetime"
        );
        Ok((
            session.access_token,
            Some(Duration::from_secs(session.expires_in)),
        ))
    }
    /// Count a request against the run's deadline and request budget.
    fn admit(&self) -> anyhow::Result<()> {
        ensure!(
            self.started.elapsed() < Duration::from_secs(1800)
                && self.requests.fetch_add(1, Ordering::Relaxed) < 100_000,
            "transfer run deadline/request limit; verified partial progress is retained"
        );
        Ok(())
    }
    async fn send(
        &self,
        method: Method,
        url: Url,
        body: Option<&[u8]>,
        authorization: Option<header::HeaderValue>,
    ) -> anyhow::Result<reqwest::Response> {
        let mut request = self.http.request(method, url);
        if let Some(authorization) = authorization {
            request = request.header(header::AUTHORIZATION, authorization);
        }
        if let Some(bytes) = body {
            request = request
                .header(header::CONTENT_TYPE, "application/json")
                .body(bytes.to_vec());
        }
        request.send().await.map_err(|_| anyhow!("remote request failed or timed out; retry is safe, fetch after an uncertain ref update"))
    }
    /// Success status, bounded body and JSON decoding, without logging the body.
    async fn finish<T: DeserializeOwned>(
        &self,
        mut response: reqwest::Response,
        label: &str,
        cap: usize,
    ) -> anyhow::Result<T> {
        ensure!(
            response.status().is_success(),
            "remote HTTP {} at {label}; no response body logged",
            response.status().as_u16()
        );
        ensure!(
            response.content_length().is_none_or(|n| n <= cap as u64),
            "remote response exceeds byte limit"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| anyhow!("remote body failed or timed out"))?
        {
            ensure!(
                chunk.len() <= cap.saturating_sub(bytes.len()),
                "remote response exceeds byte limit"
            );
            self.charge(chunk.len())?;
            bytes.extend_from_slice(&chunk);
        }
        Ok(serde_json::from_slice(&bytes)?)
    }
    /// Unauthenticated JSON POST relative to the API base (never carries a bearer).
    /// Deliberately independent of `exchange`: renewal runs inside a request.
    async fn anonymous<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &Value,
        cap: usize,
    ) -> anyhow::Result<T> {
        self.admit()?;
        let encoded = serde_json::to_vec(body)?;
        self.charge(encoded.len())?;
        let response = self
            .send(Method::POST, self.base.join(path)?, Some(&encoded), None)
            .await?;
        self.finish(response, path, cap).await
    }
    #[allow(clippy::too_many_arguments)]
    async fn exchange<T: DeserializeOwned>(
        &self,
        method: Method,
        url: Url,
        label: &str,
        body: Option<&Value>,
        cap: usize,
        missing: bool,
    ) -> anyhow::Result<Option<T>> {
        self.admit()?;
        let encoded = match body {
            Some(body) => {
                let bytes = serde_json::to_vec(body)?;
                self.charge(bytes.len())?;
                Some(bytes)
            }
            None => None,
        };
        let mut renewed = false;
        let response = loop {
            let (header, epoch) = self.current().await?;
            let response = self
                .send(
                    method.clone(),
                    url.clone(),
                    encoded.as_deref(),
                    Some(header),
                )
                .await?;
            // A 401 comes from the credential extractor or from the account lock that
            // opens each transaction, both before the handler writes anything, and
            // ref updates are compare-and-swap. The identical request is retried
            // once with a renewed credential.
            if response.status() == StatusCode::UNAUTHORIZED && !renewed && self.renewable() {
                renewed = true;
                self.rejected(epoch).await?;
                continue;
            }
            break response;
        };
        if missing && response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        Ok(Some(self.finish(response, label, cap).await?))
    }
    async fn request<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
        cap: usize,
        missing: bool,
    ) -> anyhow::Result<Option<T>> {
        let mut url = self.project_url(path)?;
        if !query.is_empty() {
            url.query_pairs_mut()
                .extend_pairs(query.iter().map(|(k, v)| (*k, v)));
        }
        self.exchange(method, url, path, body, cap, missing).await
    }
    fn charge(&self, bytes: usize) -> anyhow::Result<()> {
        ensure!(
            self.transferred.fetch_add(bytes, Ordering::Relaxed) + bytes <= 512 * 1024 * 1024,
            "transfer run exceeds 512 MiB encoded JSON budget; retry reuses accepted work"
        );
        Ok(())
    }
    pub async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
        cap: usize,
    ) -> anyhow::Result<T> {
        self.request(Method::GET, path, query, None, cap, false)
            .await?
            .ok_or_else(|| anyhow!("missing response"))
    }
    /// Like [`Client::get`], but a 404 answers `None`.
    pub async fn get_optional<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
        cap: usize,
    ) -> anyhow::Result<Option<T>> {
        self.request(Method::GET, path, query, None, cap, true)
            .await
    }
    pub async fn published(&self, address: &Address) -> anyhow::Result<Option<SignedCommit>> {
        let result: Option<SignedCommit> = self
            .request(
                Method::GET,
                &format!("history/commits/{}", address.as_str()),
                &[],
                None,
                MAX_COMMIT_BYTES + 1024,
                true,
            )
            .await?;
        if let Some(signed) = &result {
            ensure!(
                signed.verify()? == *address && signed.commit.project == self.project,
                "remote commit address/project mismatch"
            );
        }
        Ok(result)
    }
    pub async fn post(&self, path: &str, body: Value) -> anyhow::Result<Value> {
        Ok(self
            .request(Method::POST, path, &[], Some(&body), 64 * 1024, false)
            .await?
            .unwrap())
    }
    /// The independently configured key anchors trust. It must be a key of the
    /// served epoch chain and reach the active key through dual-signed statements,
    /// with no recorded break and no revocation marker. Fails closed otherwise.
    pub async fn authority(&self) -> anyhow::Result<()> {
        let server = self.server().await?;
        if let Some(new_key) = &self.break_key {
            // NEW_KEY was supplied independently. It must be named by a recorded
            // break, unrevoked from its own epoch, and linked to the active key;
            // whether it follows a journal's pin is decided per branch by `proof`.
            ensure!(
                server.chain.statements().iter().any(|s| {
                    s.transition.kind == engine_core::repository::authority_epoch::Kind::Recovery
                        && &s.transition.new_key == new_key
                }),
                "the remote's chain records no break to that key"
            );
            return server
                .vet(new_key, &server.active_key)
                .map_err(|refusal| anyhow!("{refusal}"));
        }
        server
            .vet(&self.remote.public_key, &server.active_key)
            .map_err(|refusal| match refusal {
                engine_core::repository::authority_epoch::Refusal::UnknownKey => anyhow!(
                    "server authority differs from independently configured remote: the configured key is not part of its epoch chain"
                ),
                other => anyhow!("{other}"),
            })
    }
    /// Every statement of the remote's authority chain (verified), the active key
    /// and the revocation markers. Nothing here is trusted by itself.
    pub async fn server(&self) -> anyhow::Result<&Served> {
        self.server.get_or_try_init(|| self.fetch_served()).await
    }
    async fn fetch_served(&self) -> anyhow::Result<Served> {
        #[derive(Deserialize)]
        struct Item {
            epoch: u32,
            public_key: String,
            statement: Option<SignedTransition>,
            revocation: Option<Revocation>,
        }
        #[derive(Deserialize)]
        struct Page {
            authority: Uuid,
            public_key: String,
            epoch: u32,
            revocation: Option<Revocation>,
            genesis: Option<Item>,
            rotations: Vec<Item>,
            next_epoch: Option<u32>,
        }
        let mut statements = Vec::new();
        let mut revoked = std::collections::BTreeMap::new();
        let mut genesis_key = None;
        let mut from = 1u32;
        let mut active = None;
        // At most 1024 epochs, 64 per page.
        for _ in 0..17 {
            let page: Page = self
                .get(
                    "history/authority",
                    &[("from_epoch", from.to_string()), ("limit", "64".into())],
                    512 * 1024,
                )
                .await?;
            ensure!(
                page.authority == self.remote.authority,
                "server authority differs from independently configured remote"
            );
            if let Some(genesis) = page.genesis {
                ensure!(genesis.epoch == 1, "unexpected genesis epoch");
                genesis_key = Some(genesis.public_key);
                if let Some(marker) = genesis.revocation {
                    ensure!(marker.epoch == 1, "invalid revocation marker");
                    revoked.insert(1, marker);
                }
            }
            for item in page.rotations {
                let statement = item
                    .statement
                    .ok_or_else(|| anyhow!("epoch {} lacks its statement", item.epoch))?;
                ensure!(
                    statement.transition.to_epoch == item.epoch
                        && statement.transition.new_key == item.public_key,
                    "epoch row differs from its statement"
                );
                if let Some(marker) = item.revocation {
                    ensure!(marker.epoch == item.epoch, "invalid revocation marker");
                    revoked.insert(item.epoch, marker);
                }
                statements.push(statement);
            }
            active = Some((page.public_key, page.epoch, page.revocation));
            match page.next_epoch {
                Some(next) if next > from => from = next,
                Some(_) => anyhow::bail!("authority chain page made no progress"),
                None => break,
            }
            ensure!(
                statements.len() < 1024,
                "authority chain exceeds 1024 epochs"
            );
        }
        let (active_key, active_epoch, active_marker) =
            active.ok_or_else(|| anyhow!("authority chain unavailable"))?;
        if let Some(marker) = active_marker {
            ensure!(marker.epoch == active_epoch, "invalid revocation marker");
            revoked.insert(active_epoch, marker);
        }
        match statements.first() {
            Some(first) => ensure!(
                first.transition.from_epoch == 1
                    && genesis_key.as_deref() == Some(first.transition.old_key.as_str()),
                "authority chain does not start at epoch 1"
            ),
            None => ensure!(
                genesis_key.as_deref() == Some(active_key.as_str()),
                "authority genesis key differs from the active key"
            ),
        }
        let chain = Chain::new(self.remote.authority, statements)?;
        Ok(Served::new(chain, active_key, active_epoch, revoked)?)
    }
    pub async fn head(&self, scope: &Scope) -> anyhow::Result<Option<SignedReceipt>> {
        #[derive(Deserialize)]
        struct Entry {
            branch: String,
            head: Head,
        }
        #[derive(Deserialize)]
        struct Refs {
            items: Vec<Entry>,
            receipts: Vec<SignedReceipt>,
        }
        let result: Refs = self
            .get(
                "history/refs",
                &[("branch", scope.branch.clone()), ("limit", "1".into())],
                16384,
            )
            .await?;
        ensure!(
            result.items.len() <= 1 && result.receipts.len() == result.items.len(),
            "invalid ref response"
        );
        if let Some(receipt) = result.receipts.into_iter().next() {
            let entry = &result.items[0];
            // The key is vetted against the verified epoch chain by `proof`.
            receipt.verify()?;
            ensure!(
                receipt.receipt.scope == *scope
                    && self.server().await?.knows(&receipt.receipt.issuer_key),
                "receipt scope or key is not part of the authority chain"
            );
            ensure!(
                entry.branch == scope.branch && entry.head == receipt.receipt.head,
                "ref/receipt mismatch"
            );
            Ok(Some(receipt))
        } else {
            Ok(None)
        }
    }
}
