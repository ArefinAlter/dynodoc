//! Explicit origins, no redirects/proxy discovery, bounded bodies and deadlines.
use super::*;
use reqwest::{header, Method, StatusCode, Url};
use serde::de::DeserializeOwned;
use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

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
pub struct Client {
    pub remote: Remote,
    pub project: Uuid,
    http: reqwest::Client,
    base: Url,
    requests: AtomicUsize,
    transferred: AtomicUsize,
    started: Instant,
}
impl Client {
    pub fn new(remote: Remote, project: Uuid, token: &str, loopback: bool) -> anyhow::Result<Self> {
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
        ensure!(
            !token.is_empty() && token.len() <= 16384 && !token.chars().any(char::is_whitespace),
            "invalid bearer token file"
        );
        let mut authorization = header::HeaderValue::from_str(&format!("Bearer {token}"))?;
        authorization.set_sensitive(true);
        let mut headers = header::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, authorization);
        let http = reqwest::Client::builder()
            .default_headers(headers)
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
            requests: AtomicUsize::new(0),
            transferred: AtomicUsize::new(0),
            started: Instant::now(),
        })
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
        ensure!(
            self.started.elapsed() < Duration::from_secs(1800)
                && self.requests.fetch_add(1, Ordering::Relaxed) < 100_000,
            "transfer run deadline/request limit; verified partial progress is retained"
        );
        let mut url = self
            .base
            .join(&format!("projects/{}/{path}", self.project))?;
        if !query.is_empty() {
            url.query_pairs_mut()
                .extend_pairs(query.iter().map(|(k, v)| (*k, v)));
        }
        let mut request = self.http.request(method, url);
        if let Some(body) = body {
            let bytes = serde_json::to_vec(body)?;
            self.charge(bytes.len())?;
            request = request
                .header(header::CONTENT_TYPE, "application/json")
                .body(bytes);
        }
        let mut response = request.send().await.map_err(|_| anyhow!("remote request failed or timed out; retry is safe, fetch after an uncertain ref update"))?;
        if missing && response.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        ensure!(
            response.status().is_success(),
            "remote HTTP {} at {path}; no response body logged",
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
        Ok(Some(serde_json::from_slice(&bytes)?))
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
    pub async fn authority(&self) -> anyhow::Result<()> {
        #[derive(Deserialize)]
        struct Binding {
            authority: Uuid,
            public_key: String,
        }
        let binding: Binding = self.get("history/authority", &[], 4096).await?;
        ensure!(
            binding.authority == self.remote.authority
                && binding.public_key == self.remote.public_key,
            "server authority differs from independently configured remote"
        );
        Ok(())
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
            Pin::trust_checkpoint(scope, &self.remote.public_key, receipt.clone())?;
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
