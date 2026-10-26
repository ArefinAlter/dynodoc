//! The local workspace: a PostgreSQL cluster and the Dynodoc engine, started, supervised and
//! stopped by this app, so that one installer is a working product with no server, no account and
//! no network (decision 019).
//!
//! Start-up, in order: the workspace folder and its log; the secrets (generated once, kept in the
//! operating system's credential store); the cluster (created on the first launch, adopted if an
//! earlier run left it running, recovered from its write-ahead log if that run was killed); the
//! roles and databases; the engine's migrations; the engine on a loopback port; the app's own
//! tables; the person's sign-in. Every step is reported to the page in plain words.
//!
//! The page never reaches any of this: it asks the app, through typed commands, and the app
//! speaks to the engine over loopback with a bearer token the page never sees.

pub mod appdb;
pub mod applog;
pub mod engine;
pub mod folders;
pub mod layout;
pub mod pg;
pub mod procs;
pub mod secrets;
pub mod session;
pub mod svc;

#[cfg(test)]
mod it;

use crate::remote::{Failure, Method, Reply, MAX_ANSWER_BYTES};
use layout::{Layout, Missing, Programs};
use secrets::{Preferred, Store};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use session::Session;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;
use tokio_postgres::Client;
use zeroize::Zeroizing;

const SECRETS_ENTRY: &str = "local-secrets";
const SESSION_ENTRY: &str = "local-session";
pub const WORKSPACE_SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Profile {
    pub name: String,
    pub email: String,
}

/// What the page is told about the local workspace.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    /// `starting`, `profile` (waiting for a name), `ready`, `failed` or `closing`.
    pub phase: &'static str,
    /// What is happening, in a sentence.
    pub step: String,
    /// For `failed`: what went wrong and what to do.
    pub detail: Option<String>,
    pub profile: Option<Profile>,
    pub first_run: bool,
    /// Where the person's data lives (shown in Settings).
    pub folder: String,
    /// Where the secrets are kept (shown in Settings).
    pub secrets_in: String,
    pub engine_restarts: u32,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    schema: u32,
    profile: Option<Profile>,
    created_ms: i64,
}

#[derive(Serialize, Deserialize)]
struct SecretBundle {
    owner: String,
    runtime: String,
    paseto: String,
    service: String,
}

struct Inner {
    layout: Layout,
    programs: Result<Programs, Missing>,
    secrets: Preferred,
    status: Mutex<Status>,
    base: engine::Base,
    http: reqwest::Client,
    session: tokio::sync::Mutex<Option<Session>>,
    app: tokio::sync::Mutex<Option<Client>>,
    pg_port: AtomicU16,
    keys: Mutex<Option<Arc<engine::Keys>>>,
    shutdown: watch::Sender<bool>,
    started: AtomicBool,
    closing: AtomicBool,
    identity: Mutex<Option<String>>,
}

/// Holds the app-database connection while one operation uses it.
pub(crate) struct AppGuard<'a>(tokio::sync::MutexGuard<'a, Option<Client>>);

impl std::ops::Deref for AppGuard<'_> {
    type Target = Client;
    fn deref(&self) -> &Client {
        self.0.as_ref().expect("a connection")
    }
}

#[derive(Clone)]
pub struct Local {
    inner: Arc<Inner>,
}

fn http_client() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60))
        .no_proxy()
        .build()
        .expect("an HTTP client")
}

impl Local {
    /// `ordinary` is true for the per-user data folder and false for a portable or test folder
    /// (which then gets a credential-store service name of its own, see `secrets::service_for`).
    pub fn new(data_dir: &Path, resource_dir: Option<&Path>, exe_dir: &Path, ordinary: bool) -> Self {
        let layout = Layout::new(data_dir);
        let secrets = Preferred::open(&data_dir.join("secrets"), &secrets::service_for(data_dir, ordinary));
        let status = Status {
            phase: "starting",
            step: "Starting".into(),
            detail: None,
            profile: None,
            first_run: false,
            folder: layout.root.display().to_string(),
            secrets_in: secrets.describe().to_string(),
            engine_restarts: 0,
        };
        let (shutdown, _) = watch::channel(false);
        Self {
            inner: Arc::new(Inner {
                programs: Programs::discover(resource_dir, exe_dir),
                layout,
                secrets,
                status: Mutex::new(status),
                base: engine::Base::default(),
                http: http_client(),
                session: tokio::sync::Mutex::new(None),
                app: tokio::sync::Mutex::new(None),
                pg_port: AtomicU16::new(0),
                keys: Mutex::new(None),
                shutdown,
                started: AtomicBool::new(false),
                closing: AtomicBool::new(false),
                identity: Mutex::new(None),
            }),
        }
    }

    pub fn status(&self) -> Status {
        self.inner.status.lock().map(|s| s.clone()).unwrap_or_else(|e| e.into_inner().clone())
    }

    fn set(&self, phase: &'static str, step: &str) {
        if let Ok(mut s) = self.inner.status.lock() {
            s.phase = phase;
            s.step = step.to_string();
            if phase != "failed" {
                s.detail = None;
            }
        }
    }

    fn fail(&self, detail: &str) {
        applog::note(&format!("failed: {detail}"));
        if let Ok(mut s) = self.inner.status.lock() {
            s.phase = "failed";
            s.step = "The local workspace did not start".into();
            s.detail = Some(detail.to_string());
        }
    }

    pub fn layout(&self) -> &Layout {
        &self.inner.layout
    }

    pub fn is_ready(&self) -> bool {
        self.inner.status.lock().map(|s| s.phase == "ready").unwrap_or(false)
    }

    /// Begins the start-up in the background (once). The page polls `status`.
    pub fn start(&self) {
        if self.inner.started.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = self.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(message) = me.bring_up().await {
                me.fail(&message);
            }
        });
    }

    fn load_saved(&self) -> Option<Saved> {
        let text = std::fs::read_to_string(&self.inner.layout.state_file).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn write_saved(&self, saved: &Saved) -> Result<(), String> {
        let text = serde_json::to_string_pretty(saved).map_err(|_| "The workspace could not be saved.")?;
        let path = &self.inner.layout.state_file;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, text).map_err(|_| "The workspace could not be saved.")?;
        std::fs::rename(&temporary, path).map_err(|_| "The workspace could not be saved.".to_string())
    }

    fn secret_bundle(&self, create: bool) -> Result<SecretBundle, String> {
        match self.inner.secrets.get(SECRETS_ENTRY) {
            Ok(Some(text)) => {
                if let Ok(bundle) = serde_json::from_str::<SecretBundle>(&text) {
                    return Ok(bundle);
                }
            }
            Ok(None) => {}
            Err(e) => return Err(e.to_string()),
        }
        if !create {
            return Err("The keys of this workspace are not in the credential store any more. Restore the workspace from a backup, or choose Reset in Settings to start a new one.".into());
        }
        let bundle = SecretBundle {
            owner: secrets::random_hex(24).map_err(|e| e.to_string())?.to_string(),
            runtime: secrets::random_hex(24).map_err(|e| e.to_string())?.to_string(),
            paseto: secrets::random_hex(32).map_err(|e| e.to_string())?.to_string(),
            service: secrets::random_hex(24).map_err(|e| e.to_string())?.to_string(),
        };
        let text = Zeroizing::new(serde_json::to_string(&bundle).map_err(|_| "No keys could be made.")?);
        self.inner.secrets.set(SECRETS_ENTRY, &text).map_err(|e| e.to_string())?;
        Ok(bundle)
    }

    async fn bring_up(&self) -> Result<(), String> {
        let inner = &self.inner;
        self.set("starting", "Preparing the workspace folder");
        inner.layout.ensure().map_err(|_| "The workspace folder could not be created.".to_string())?;
        applog::open(inner.layout.logs.join("app.log"));
        applog::note("starting the local workspace");
        let programs = inner.programs.clone().map_err(|m| m.to_string())?;

        let had_cluster = inner.layout.has_cluster();
        let bundle = self.secret_bundle(!had_cluster)?;
        let keys = Arc::new(engine::Keys {
            owner_password: Zeroizing::new(bundle.owner),
            runtime_password: Zeroizing::new(bundle.runtime),
            paseto: Zeroizing::new(bundle.paseto),
            service: Zeroizing::new(bundle.service),
        });
        if let Ok(mut slot) = inner.keys.lock() {
            *slot = Some(keys.clone());
        }
        let database = pg::Pg::new(programs.clone(), inner.layout.clone());

        if !had_cluster {
            if let Ok(mut s) = inner.status.lock() {
                s.first_run = true;
            }
            self.set("starting", "Creating your local database (first launch only; this can take a minute)");
            database.init_cluster(&keys.owner_password).await.map_err(|e| e.to_string())?;
        }

        self.set("starting", "Starting the local database");
        let port = match database.running_port().await {
            Some(port) => {
                applog::note("adopting a database an earlier run left running");
                port
            }
            None => {
                let port = pg::free_port().map_err(|_| "No local port is free.".to_string())?;
                database.start(port).await.map_err(|e| e.to_string())?;
                port
            }
        };
        inner.pg_port.store(port, Ordering::SeqCst);
        pg::Pg::wait_ready(port, &keys.owner_password, Duration::from_secs(60))
            .await
            .map_err(|e| e.to_string())?;
        pg::Pg::provision(port, &keys.owner_password, &keys.runtime_password)
            .await
            .map_err(|e| e.to_string())?;

        self.set("starting", "Bringing the workspace up to date");
        engine::migrate(&programs, port, &keys).await?;

        self.set("starting", "Starting the engine");
        let launch = engine::Launch {
            programs: programs.clone(),
            layout: inner.layout.clone(),
            pg_port: port,
            keys: keys.clone(),
        };
        let first = engine::spawn(&launch).await?;
        inner.base.set(Some(format!("http://127.0.0.1:{}", first.port)));
        let me = self.clone();
        let base = inner.base.clone();
        let shutdown = inner.shutdown.subscribe();
        tauri::async_runtime::spawn(engine::supervise(launch, first, base, shutdown, move |event| {
            if let Ok(mut s) = me.inner.status.lock() {
                match event {
                    engine::Event::Up => {}
                    engine::Event::Down { .. } => s.engine_restarts += 1,
                    engine::Event::GaveUp(message) => {
                        s.phase = "failed";
                        s.step = "The engine stopped".into();
                        s.detail = Some(message);
                    }
                }
            }
        }));

        self.set("starting", "Opening the workspace's own data");
        let mut client = pg::Pg::connect(port, pg::OWNER, &keys.owner_password, pg::APP_DB)
            .await
            .map_err(|e| e.to_string())?;
        appdb::ensure_schema(&mut client).await.map_err(|e| e.0)?;
        *inner.app.lock().await = Some(client);

        let saved = self.load_saved();
        match saved.and_then(|s| s.profile) {
            Some(profile) => {
                self.set("starting", "Signing in on this computer");
                self.ensure_session(Some(&profile)).await?;
                if let Ok(mut s) = inner.status.lock() {
                    s.profile = Some(profile);
                }
                self.set("ready", "Ready");
            }
            None => self.set("profile", "Waiting for your name"),
        }
        applog::note("the local workspace is ready");
        Ok(())
    }

    fn keys(&self) -> Option<Arc<engine::Keys>> {
        self.inner.keys.lock().ok().and_then(|k| k.clone())
    }

    /// Makes (or renews) the sign-in of the local person.
    async fn ensure_session(&self, profile: Option<&Profile>) -> Result<(), String> {
        let base = self.inner.base.get().ok_or("The engine is not running.")?;
        let keys = self.keys().ok_or("The workspace is not open.")?;
        let mut slot = self.inner.session.lock().await;
        if let Some(s) = slot.as_ref() {
            if !s.stale() {
                return Ok(());
            }
        }
        // The saved refresh token first (a restart), then the service credential.
        let saved = self
            .inner
            .secrets
            .get(SESSION_ENTRY)
            .ok()
            .flatten()
            .and_then(|t| Session::from_saved_text(&t));
        let candidate = slot.take().or(saved);
        if let Some(old) = candidate {
            if !old.stale() {
                *slot = Some(old);
                return Ok(());
            }
            if let Ok(next) = session::refresh(&self.inner.http, &base, &old).await {
                self.remember(&next);
                *slot = Some(next);
                return Ok(());
            }
        }
        let profile = match profile {
            Some(p) => p.clone(),
            None => self.load_saved().and_then(|s| s.profile).ok_or("There is no profile yet.")?,
        };
        let next = session::mint(&self.inner.http, &base, &keys.service, &profile.email, &profile.name)
            .await
            .map_err(|_| "The engine did not sign this computer in.".to_string())?;
        self.remember(&next);
        *slot = Some(next);
        Ok(())
    }

    fn remember(&self, session: &Session) {
        let _ = self.inner.secrets.set(SESSION_ENTRY, &session.to_saved_text());
    }

    /// The first thing a person does: say who they are. Nothing leaves the computer.
    pub async fn create_profile(&self, name: &str, email: &str) -> Result<Profile, String> {
        if self.status().phase != "profile" {
            return Err("This workspace already has a profile.".into());
        }
        let email = if email.trim().is_empty() {
            session::local_address(&secrets::random_hex(8).map_err(|e| e.to_string())?)
        } else {
            email.to_string()
        };
        let (name, email) = session::valid_profile(name, &email).map_err(str::to_string)?;
        let profile = Profile { name, email };
        self.ensure_session(Some(&profile)).await?;
        self.write_saved(&Saved {
            schema: WORKSPACE_SCHEMA,
            profile: Some(profile.clone()),
            created_ms: appdb::now_ms(),
        })?;
        if let Ok(mut s) = self.inner.status.lock() {
            s.profile = Some(profile.clone());
        }
        self.set("ready", "Ready");
        Ok(profile)
    }

    /// One request to the local engine as the local person. `allowed` was checked by the caller.
    pub async fn api(
        &self,
        method: Method,
        path_and_query: &str,
        body: Option<&Value>,
        if_match: Option<&str>,
    ) -> Result<Reply, Failure> {
        for attempt in 0..2 {
            self.ensure_session(None).await.map_err(|_| Failure::Unreachable)?;
            let base = self.inner.base.get().ok_or(Failure::Unreachable)?;
            let token = {
                let slot = self.inner.session.lock().await;
                slot.as_ref().map(|s| s.access().to_string()).ok_or(Failure::Unreachable)?
            };
            let reply = self.send(&base, method, path_and_query, &token, body, if_match).await?;
            if reply.status == 401 && attempt == 0 {
                // The token ended (or the engine restarted): make a new one and try once more.
                *self.inner.session.lock().await = None;
                let _ = self.inner.secrets.delete(SESSION_ENTRY);
                continue;
            }
            return Ok(reply);
        }
        Err(Failure::Unreachable)
    }

    async fn send(
        &self,
        base: &str,
        method: Method,
        path_and_query: &str,
        token: &str,
        body: Option<&Value>,
        if_match: Option<&str>,
    ) -> Result<Reply, Failure> {
        let m = match method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Put => reqwest::Method::PUT,
            Method::Patch => reqwest::Method::PATCH,
            Method::Delete => reqwest::Method::DELETE,
        };
        let mut request = self
            .inner
            .http
            .request(m, format!("{base}/{path_and_query}"))
            .bearer_auth(token)
            .header("accept", "application/json");
        if path_and_query.starts_with("documents/") {
            request = request.header("x-dynodoc-editor", "office-v5");
        }
        if let Some(tag) = if_match {
            request = request.header("if-match", tag);
        }
        if let Some(value) = body {
            request = request
                .header("content-type", "application/json")
                .body(serde_json::to_vec(value).map_err(|_| Failure::NotJson)?);
        }
        let mut response = request.send().await.map_err(|_| Failure::Unreachable)?;
        let status = response.status();
        if status.is_redirection() {
            return Err(Failure::Redirected);
        }
        let mut bytes: Vec<u8> = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Unreachable)? {
            if bytes.len() + chunk.len() > MAX_ANSWER_BYTES {
                return Err(Failure::TooLarge);
            }
            bytes.extend_from_slice(&chunk);
        }
        let body = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).map_err(|_| Failure::NotJson)?
        };
        Ok(Reply {
            status: status.as_u16(),
            body,
        })
    }

    /// The identity id of the local person (for notes), cached after the first read.
    pub async fn identity(&self) -> Option<String> {
        if let Some(id) = self.inner.identity.lock().ok().and_then(|g| g.clone()) {
            return Some(id);
        }
        let reply = self.api(Method::Get, "auth/me", None, None).await.ok()?;
        let id = reply.body.get("identity_id")?.as_str()?.to_string();
        if let Ok(mut g) = self.inner.identity.lock() {
            *g = Some(id.clone());
        }
        Some(id)
    }

    /// The connection to the app's own database (opened again if it was lost).
    pub(crate) async fn app(&self) -> Result<AppGuard<'_>, String> {
        let mut slot = self.inner.app.lock().await;
        let closed = slot.as_ref().map(Client::is_closed).unwrap_or(true);
        if closed {
            let keys = self.keys().ok_or("The workspace is not open.")?;
            let port = self.inner.pg_port.load(Ordering::SeqCst);
            let client = pg::Pg::connect(port, pg::OWNER, &keys.owner_password, pg::APP_DB)
                .await
                .map_err(|e| e.to_string())?;
            *slot = Some(client);
        }
        Ok(AppGuard(slot))
    }

    /// After a failed start: start again (a database that is already running is adopted).
    pub fn retry(&self) -> bool {
        if self.status().phase != "failed" {
            return false;
        }
        self.inner.started.store(false, Ordering::SeqCst);
        self.set("starting", "Starting again");
        self.start();
        true
    }

    pub fn is_closing(&self) -> bool {
        self.inner.closing.load(Ordering::SeqCst)
    }

    /// Stops the engine, then the database, cleanly. Safe to call twice.
    pub async fn shutdown(&self) {
        if self.inner.closing.swap(true, Ordering::SeqCst) {
            return;
        }
        self.set("closing", "Closing the workspace");
        let _ = self.inner.shutdown.send(true);
        // Give the engine a moment to release its connections.
        tokio::time::sleep(Duration::from_millis(300)).await;
        *self.inner.app.lock().await = None;
        if let Ok(programs) = self.inner.programs.clone() {
            let database = pg::Pg::new(programs, self.inner.layout.clone());
            if database.stop().await.is_err() {
                applog::note("the database did not stop cleanly");
            }
        }
        applog::note("closed");
    }

    pub fn data_folder(&self) -> PathBuf {
        self.inner.layout.root.clone()
    }

    /// Ends this run's database for a restore, and lets the next start bring a restored one up.
    pub(crate) fn pg_port(&self) -> u16 {
        self.inner.pg_port.load(Ordering::SeqCst)
    }

    pub(crate) fn programs(&self) -> Result<Programs, String> {
        self.inner.programs.clone().map_err(|m| m.to_string())
    }

    pub(crate) fn secrets(&self) -> &Preferred {
        &self.inner.secrets
    }

    pub(crate) fn keys_for_backup(&self) -> Option<Arc<engine::Keys>> {
        self.keys()
    }

    pub(crate) fn json_status(&self) -> Value {
        serde_json::to_value(self.status()).unwrap_or_else(|_| json!({}))
    }
}
