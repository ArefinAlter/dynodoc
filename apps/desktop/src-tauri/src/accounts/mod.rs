//! Servers the person signed in to from the app: the hosted Dynodoc, or one they run themselves
//! (decision 020).
//!
//! The standalone app works on a workspace of its own and needs no server. A server is an addition:
//! the person adds its address, signs in with the system browser (the server's own sign-in page,
//! then a one-time code handed back to this app on a loopback address, with PKCE), and from then on
//! the page can read that server's workspace and link local projects to it. This module keeps:
//!
//! - the list of servers (`accounts.json` in the config folder: an id, the address, who the
//!   server says you are; no secret);
//! - one session per server in the operating system's credential store (the server's
//!   `dynodoc_session` cookie, which the server renews on every request: the renewed value is stored
//!   again; a cleared one signs the person out here);
//! - the requests made for the page (`call`): fixed route shapes only, never an address the page
//!   names, the cookie never reaches the page.
//!
//! Nothing here prints a cookie, a code, a state or a verifier.

#[cfg(desktop)]
mod loopback;

use crate::remote::{Method, Reply, Target};
use serde_json::Value;
use tauri::AppHandle;

/// An account id: `srv_` and lower-case hex.
pub fn id_ok(id: &str) -> bool {
    id.len() >= 5
        && id.len() <= 40
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

#[cfg(desktop)]
pub use imp::{AccountView, Accounts, FlowView};

/// One request to a server the person signed in to, for the page.
pub async fn call(
    app: &AppHandle,
    id: &str,
    target: Target,
    method: Method,
    path: &str,
    body: Option<Value>,
    if_match: Option<&str>,
) -> Result<Reply, String> {
    #[cfg(desktop)]
    {
        use tauri::Manager;
        let state = app.state::<crate::AppState>();
        state
            .accounts
            .call(id, target, method, path, body.as_ref(), if_match)
            .await
    }
    #[cfg(not(desktop))]
    {
        let _ = (app, id, target, method, path, body, if_match);
        Err("Sign in to that server first.".to_string())
    }
}

#[cfg(desktop)]
mod imp {
    use super::loopback::{self, Request};
    use crate::local::appdb::now_ms;
    use crate::local::secrets::{random_hex, Secret, Store};
    use crate::remote::{self, Method, Reply, Target};
    use crate::server::{parse_server_address, ServerOrigin};
    use crate::signin::{self, Attempt, Finish, Moment, Pending, Refusal};
    use serde::{Deserialize, Serialize};
    use serde_json::{json, Value};
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};
    use std::time::Instant;
    use tokio::net::TcpListener;
    use tokio::sync::watch;

    const FILE: &str = "accounts.json";
    const MAX_ACCOUNTS: usize = 8;
    const MAX_FILE_BYTES: u64 = 64 * 1024;
    /// Longest a name or an email from a server is kept.
    const MAX_TEXT: usize = 200;

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    pub struct Account {
        pub id: String,
        /// `https://host[:port]`, as `ServerOrigin::as_text` writes it.
        pub server: String,
        pub added_at: i64,
        #[serde(default)]
        pub name: Option<String>,
        #[serde(default)]
        pub email: Option<String>,
    }

    /// What the page is told about an account.
    #[derive(Debug, Clone, Serialize)]
    pub struct AccountView {
        pub id: String,
        pub server: String,
        pub host: String,
        pub cloud: bool,
        pub signed_in: bool,
        pub name: Option<String>,
        pub email: Option<String>,
        pub added_at: i64,
    }

    /// The sign-in that may be waiting for the browser.
    #[derive(Debug, Clone, Serialize)]
    pub struct FlowView {
        /// `none`, `waiting`, `done` or `failed`.
        pub state: &'static str,
        pub account_id: Option<String>,
        /// Four characters the server's page shows too.
        pub pairing: Option<String>,
        pub message: String,
    }

    impl FlowView {
        fn none() -> Self {
            Self {
                state: "none",
                account_id: None,
                pairing: None,
                message: String::new(),
            }
        }
    }

    struct Flow {
        view: FlowView,
        cancel: Option<watch::Sender<bool>>,
        started: Option<Instant>,
    }

    /// A sign-in that waits for the `dynodoc://signin` link instead of the loopback address (for a
    /// server that is older than the loopback hand-over).
    struct LinkAttempt {
        id: String,
        origin: ServerOrigin,
        pending: Pending,
        pairing: String,
    }

    struct Inner {
        dir: PathBuf,
        store: Box<dyn Store>,
        list: Mutex<Vec<Account>>,
        flow: Mutex<Flow>,
        link: Mutex<Option<Arc<LinkAttempt>>>,
        /// Writes of a renewed session go one at a time.
        write: Mutex<()>,
    }

    #[derive(Clone)]
    pub struct Accounts(Arc<Inner>);

    fn secret_name(id: &str) -> String {
        format!("account:{id}")
    }

    fn clean(text: &str) -> Option<String> {
        let t: String = text.chars().filter(|c| !c.is_control()).collect();
        let t = t.trim();
        (!t.is_empty()).then(|| t.chars().take(MAX_TEXT).collect())
    }

    fn load(dir: &Path) -> Vec<Account> {
        let path = dir.join(FILE);
        let Ok(meta) = std::fs::metadata(&path) else {
            return Vec::new();
        };
        if !meta.is_file() || meta.len() > MAX_FILE_BYTES {
            return Vec::new();
        }
        let Ok(bytes) = std::fs::read(&path) else {
            return Vec::new();
        };
        let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
            return Vec::new();
        };
        let list: Vec<Account> =
            serde_json::from_value(value["accounts"].clone()).unwrap_or_default();
        // Only entries that are still what they should be: a good id and a server address that
        // parses to itself.
        list.into_iter()
            .filter(|a| {
                super::id_ok(&a.id)
                    && parse_server_address(&a.server).is_ok_and(|o| o.as_text() == a.server)
            })
            .take(MAX_ACCOUNTS)
            .collect()
    }

    fn host_of(a: &Account) -> String {
        a.server
            .split("://")
            .nth(1)
            .unwrap_or(&a.server)
            .to_string()
    }

    impl Accounts {
        pub fn new(dir: &Path, store: impl Store + 'static) -> Self {
            Self(Arc::new(Inner {
                dir: dir.to_path_buf(),
                store: Box::new(store),
                list: Mutex::new(load(dir)),
                flow: Mutex::new(Flow {
                    view: FlowView::none(),
                    cancel: None,
                    started: None,
                }),
                link: Mutex::new(None),
                write: Mutex::new(()),
            }))
        }

        fn save(&self, list: &[Account]) -> Result<(), String> {
            const CANNOT: &str = "The list of servers could not be saved.";
            std::fs::create_dir_all(&self.0.dir).map_err(|_| CANNOT.to_string())?;
            let text = serde_json::to_string_pretty(&json!({ "version": 1, "accounts": list }))
                .map_err(|_| CANNOT.to_string())?;
            let path = self.0.dir.join(FILE);
            let temporary = path.with_extension("json.tmp");
            std::fs::write(&temporary, text).map_err(|_| CANNOT.to_string())?;
            std::fs::rename(&temporary, &path).map_err(|_| CANNOT.to_string())
        }

        /// Where the sessions are kept, in words.
        pub fn store_describe(&self) -> &'static str {
            self.0.store.describe()
        }

        pub fn find(&self, id: &str) -> Option<Account> {
            self.0
                .list
                .lock()
                .ok()?
                .iter()
                .find(|a| a.id == id)
                .cloned()
        }

        fn origin_of(account: &Account) -> Result<ServerOrigin, String> {
            parse_server_address(&account.server).map_err(|e| e.to_string())
        }

        fn cookie(&self, id: &str) -> Option<Secret> {
            self.0
                .store
                .get(&secret_name(id))
                .ok()
                .flatten()
                .filter(|s| !s.is_empty())
        }

        fn view_of(&self, a: &Account) -> AccountView {
            let cloud = Self::origin_of(a).is_ok_and(|o| o.is_cloud());
            AccountView {
                id: a.id.clone(),
                server: a.server.clone(),
                host: host_of(a),
                cloud,
                signed_in: self.cookie(&a.id).is_some(),
                name: a.name.clone(),
                email: a.email.clone(),
                added_at: a.added_at,
            }
        }

        pub fn views(&self) -> Vec<AccountView> {
            let list = self.0.list.lock().map(|l| l.clone()).unwrap_or_default();
            list.iter().map(|a| self.view_of(a)).collect()
        }

        pub fn view(&self, id: &str) -> Option<AccountView> {
            self.find(id).map(|a| self.view_of(&a))
        }

        /// Adds a server by its address (nothing is sent to it yet). An address that is already
        /// there gives the account it already has.
        pub fn add(&self, address: &str) -> Result<AccountView, String> {
            let origin = parse_server_address(address).map_err(|e| e.to_string())?;
            let text = origin.as_text();
            let mut list = self
                .0
                .list
                .lock()
                .map_err(|_| "The list of servers is busy.".to_string())?;
            if let Some(found) = list.iter().find(|a| a.server == text) {
                let found = found.clone();
                drop(list);
                return Ok(self.view_of(&found));
            }
            if list.len() >= MAX_ACCOUNTS {
                return Err(format!(
                    "At most {MAX_ACCOUNTS} servers can be added. Remove one first."
                ));
            }
            let random = random_hex(4)
                .map_err(|_| "The system could not make a random value.".to_string())?;
            let account = Account {
                id: format!("srv_{}", random.as_str()),
                server: text,
                added_at: now_ms(),
                name: None,
                email: None,
            };
            list.push(account.clone());
            self.save(&list)?;
            drop(list);
            Ok(self.view_of(&account))
        }

        /// Forgets a server and its session on this computer. (To end the session at the server
        /// as well, sign out first.)
        pub fn remove(&self, id: &str) -> Result<(), String> {
            self.cancel_flow_for(id);
            self.drop_cookie(id);
            let mut list = self
                .0
                .list
                .lock()
                .map_err(|_| "The list of servers is busy.".to_string())?;
            list.retain(|a| a.id != id);
            self.save(&list)
        }

        fn put_cookie(&self, id: &str, value: &str) -> Result<(), String> {
            let _guard = self
                .0
                .write
                .lock()
                .map_err(|_| "The credential store is busy.".to_string())?;
            self.0
                .store
                .set(&secret_name(id), value)
                .map_err(|e| e.to_string())
        }

        fn drop_cookie(&self, id: &str) {
            if let Ok(_guard) = self.0.write.lock() {
                let _ = self.0.store.delete(&secret_name(id));
            }
        }

        /// What a response's `Set-Cookie` does to the stored session: replaces it, or ends it.
        fn apply_set_cookie(&self, id: &str, set_cookie: &str) {
            let Ok(parsed) = tauri::webview::Cookie::parse(set_cookie.to_string()) else {
                return;
            };
            if parsed.name() != remote::SESSION_COOKIE {
                return;
            }
            let cleared = parsed.value().is_empty()
                || parsed.max_age().is_some_and(|age| age.whole_seconds() <= 0);
            if cleared {
                self.drop_cookie(id);
            } else {
                let _ = self.put_cookie(id, parsed.value());
            }
        }

        /// One request, for the page. The route must be one of the fixed shapes (`remote::allowed`,
        /// checked by the caller); the address is the server's, never the page's.
        pub async fn call(
            &self,
            id: &str,
            target: Target,
            method: Method,
            path: &str,
            body: Option<&Value>,
            if_match: Option<&str>,
        ) -> Result<Reply, String> {
            let account = self
                .find(id)
                .ok_or_else(|| "That server is not in your list.".to_string())?;
            let origin = Self::origin_of(&account)?;
            let url = remote::address(&origin, target, path)
                .ok_or_else(|| "That request is not available from the client.".to_string())?;
            let Some(cookie) = self.cookie(id) else {
                return Err(format!(
                    "You are not signed in to {}. Sign in under Settings, Accounts.",
                    host_of(&account)
                ));
            };
            let (reply, renewed) = remote::send(
                &origin,
                url,
                target,
                method,
                Some(cookie.as_str()),
                body,
                if_match,
            )
            .await
            .map_err(|f| f.to_string())?;
            if let Some(set) = renewed {
                self.apply_set_cookie(id, &set);
            }
            Ok(reply)
        }

        /// Asks the server who the session is (and whether it still is one); keeps the name and the
        /// email the server gives.
        pub async fn check(&self, id: &str) -> Result<AccountView, String> {
            let account = self
                .find(id)
                .ok_or_else(|| "That server is not in your list.".to_string())?;
            if self.cookie(id).is_none() {
                return Ok(self.view_of(&account));
            }
            let reply = self
                .call(id, Target::Api, Method::Get, "profile", None, None)
                .await?;
            match reply.status {
                200 => {
                    let name = reply.body["name"].as_str().and_then(clean);
                    let email = reply.body["email"].as_str().and_then(clean);
                    self.set_identity(id, name, email);
                }
                401 => self.drop_cookie(id),
                _ => {}
            }
            self.view(id)
                .ok_or_else(|| "That server is not in your list.".to_string())
        }

        fn set_identity(&self, id: &str, name: Option<String>, email: Option<String>) {
            if let Ok(mut list) = self.0.list.lock() {
                if let Some(a) = list.iter_mut().find(|a| a.id == id) {
                    if a.name != name || a.email != email {
                        a.name = name;
                        a.email = email;
                        let snapshot = list.clone();
                        let _ = self.save(&snapshot);
                    }
                }
            }
        }

        /// Ends the session at the server, and here whatever the server says. True when the server
        /// confirmed.
        pub async fn sign_out(&self, id: &str) -> Result<(AccountView, bool), String> {
            let account = self
                .find(id)
                .ok_or_else(|| "That server is not in your list.".to_string())?;
            let origin = Self::origin_of(&account)?;
            let mut confirmed = false;
            if let (Some(cookie), Some(url)) = (self.cookie(id), origin.join("/auth/logout")) {
                if let Ok((reply, _)) = remote::send(
                    &origin,
                    url,
                    Target::Api,
                    Method::Post,
                    Some(cookie.as_str()),
                    Some(&json!({})),
                    None,
                )
                .await
                {
                    confirmed = reply.status == 200;
                }
            }
            self.drop_cookie(id);
            self.set_identity(id, None, None);
            Ok((self.view_of(&account), confirmed))
        }

        // ---- the browser sign-in ----------------------------------------------------------------------

        pub fn flow(&self) -> FlowView {
            self.0
                .flow
                .lock()
                .map(|f| f.view.clone())
                .unwrap_or_else(|_| FlowView::none())
        }

        fn set_flow(&self, view: FlowView) {
            if let Ok(mut f) = self.0.flow.lock() {
                f.view = view;
            }
        }

        pub fn cancel_flow(&self) {
            if let Ok(mut link) = self.0.link.lock() {
                *link = None;
            }
            if let Ok(mut f) = self.0.flow.lock() {
                if let Some(tx) = f.cancel.take() {
                    let _ = tx.send(true);
                }
                if f.view.state == "waiting" {
                    f.view = FlowView::none();
                }
            }
        }

        fn cancel_flow_for(&self, id: &str) {
            if self.flow().account_id.as_deref() == Some(id) {
                self.cancel_flow();
            }
        }

        /// Starts a sign-in to `id`: a listener on this computer, then the server's sign-in page in
        /// the system browser through `open`. Returns the pairing code.
        pub async fn begin(
            &self,
            id: &str,
            via_link: bool,
            open: impl FnOnce(&str) -> Result<(), String>,
        ) -> Result<String, String> {
            let account = self
                .find(id)
                .ok_or_else(|| "That server is not in your list.".to_string())?;
            let origin = Self::origin_of(&account)?;
            {
                // One start a second at most, and at most one sign-in waiting.
                let mut f = self
                    .0
                    .flow
                    .lock()
                    .map_err(|_| "The sign-in is busy.".to_string())?;
                if f.started
                    .is_some_and(|t| t.elapsed() < std::time::Duration::from_secs(1))
                {
                    return Err("Wait a moment before starting again.".to_string());
                }
                f.started = Some(Instant::now());
                if let Some(tx) = f.cancel.take() {
                    let _ = tx.send(true);
                }
            }
            let (attempt, request) =
                Attempt::new(&origin.as_text(), Moment::now(), signin::os_random).ok_or_else(
                    || {
                        "The system could not make a random value, so no sign-in was started."
                            .to_string()
                    },
                )?;
            let pairing = signin::pairing_code(&request.state);
            if via_link {
                return self.begin_link(id, origin, attempt, request, pairing, open);
            }
            let listener = TcpListener::bind(("127.0.0.1", 0)).await.map_err(|_| {
                "The app could not listen on this computer for the browser's answer.".to_string()
            })?;
            let port = listener
                .local_addr()
                .map_err(|_| "The app could not listen on this computer.".to_string())?
                .port();
            let url = origin.app_signin_url_loopback(
                &request.challenge,
                &request.state,
                signin::PLATFORM,
                port,
            );
            let pending = Pending::default();
            pending.start(attempt);
            let (cancel_tx, cancel_rx) = watch::channel(false);
            if let Ok(mut f) = self.0.flow.lock() {
                f.cancel = Some(cancel_tx);
                f.view = FlowView {
                    state: "waiting",
                    account_id: Some(id.to_string()),
                    pairing: Some(pairing.clone()),
                    message: String::new(),
                };
            }
            if let Err(message) = open(url.as_str()) {
                self.cancel_flow();
                return Err(message);
            }
            let me = self.clone();
            let id = id.to_string();
            let shown = pairing.clone();
            tokio::spawn(async move {
                me.run_flow(id, origin, listener, port, pending, pairing, cancel_rx)
                    .await;
            });
            Ok(shown)
        }

        /// The other way, for a server without the loopback hand-over: the server's page ends with a
        /// `dynodoc://signin?code=..&state=..` link, which the system gives to this app (so the app
        /// must be installed with its link registered). `complete_link` takes it from there.
        fn begin_link(
            &self,
            id: &str,
            origin: ServerOrigin,
            attempt: Attempt,
            request: signin::Request,
            pairing: String,
            open: impl FnOnce(&str) -> Result<(), String>,
        ) -> Result<String, String> {
            let url = origin.app_signin_url(&request.challenge, &request.state, signin::PLATFORM);
            let pending = Pending::default();
            pending.start(attempt);
            let mine = Arc::new(LinkAttempt {
                id: id.to_string(),
                origin,
                pending,
                pairing: pairing.clone(),
            });
            if let Ok(mut link) = self.0.link.lock() {
                *link = Some(mine.clone());
            }
            if let Ok(mut f) = self.0.flow.lock() {
                f.cancel = None;
                f.view = FlowView {
                    state: "waiting",
                    account_id: Some(id.to_string()),
                    pairing: Some(pairing.clone()),
                    message: String::new(),
                };
            }
            if let Err(message) = open(url.as_str()) {
                self.cancel_flow();
                return Err(message);
            }
            // After ten minutes the attempt is over (the pending attempt refuses it by itself).
            let me = self.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(signin::ATTEMPT_TTL).await;
                let still =
                    me.0.link
                        .lock()
                        .ok()
                        .and_then(|l| l.clone())
                        .is_some_and(|l| Arc::ptr_eq(&l, &mine));
                if still {
                    if let Ok(mut link) = me.0.link.lock() {
                        *link = None;
                    }
                    me.set_flow(FlowView {
                        state: "failed",
                        account_id: Some(mine.id.clone()),
                        pairing: Some(mine.pairing.clone()),
                        message: "The sign-in took too long. Start again.".to_string(),
                    });
                }
            });
            Ok(pairing)
        }

        /// A `dynodoc://signin` link came back. True when it is the one a waiting link sign-in is
        /// for (it is then finished in the background); false when none is waiting or the state is
        /// not that attempt's, so that nothing changes.
        pub fn complete_link(&self, code: &str, state: &str) -> bool {
            let Some(attempt) = self.0.link.lock().ok().and_then(|l| l.clone()) else {
                return false;
            };
            match attempt
                .pending
                .complete(Moment::now(), &attempt.origin.as_text(), code, state)
            {
                Ok(finish) => {
                    if let Ok(mut link) = self.0.link.lock() {
                        *link = None;
                    }
                    let me = self.clone();
                    tauri::async_runtime::spawn(async move {
                        let outcome = me.finish(&attempt.id, &attempt.origin, &finish).await;
                        let (state, message) = match outcome {
                            Ok(()) => ("done", String::new()),
                            Err(m) => ("failed", m),
                        };
                        me.set_flow(FlowView {
                            state,
                            account_id: Some(attempt.id.clone()),
                            pairing: Some(attempt.pairing.clone()),
                            message,
                        });
                    });
                    true
                }
                Err(Refusal::StateMismatch) | Err(Refusal::NothingPending) => false,
                Err(_) => {
                    if let Ok(mut link) = self.0.link.lock() {
                        *link = None;
                    }
                    self.set_flow(FlowView {
                        state: "failed",
                        account_id: Some(attempt.id.clone()),
                        pairing: Some(attempt.pairing.clone()),
                        message: "The sign-in took too long. Start again.".to_string(),
                    });
                    true
                }
            }
        }

        #[allow(clippy::too_many_arguments)]
        async fn run_flow(
            &self,
            id: String,
            origin: ServerOrigin,
            listener: TcpListener,
            port: u16,
            pending: Pending,
            pairing: String,
            mut cancel: watch::Receiver<bool>,
        ) {
            let deadline = tokio::time::Instant::now() + signin::ATTEMPT_TTL;
            let say = |me: &Self, state: &'static str, message: &str| {
                me.set_flow(FlowView {
                    state,
                    account_id: Some(id.clone()),
                    pairing: Some(pairing.clone()),
                    message: message.to_string(),
                });
            };
            loop {
                let accepted = tokio::select! {
                    _ = cancel.changed() => return,
                    _ = tokio::time::sleep_until(deadline) => {
                        say(self, "failed", "The sign-in took too long. Start again.");
                        return;
                    }
                    a = listener.accept() => a,
                };
                let Ok((mut stream, peer)) = accepted else {
                    continue;
                };
                if !peer.ip().is_loopback() {
                    continue;
                }
                let Some(head) = loopback::read_head(&mut stream).await else {
                    continue;
                };
                let request = loopback::parse(&head, port);
                let Request::SignIn { code, state } = &request else {
                    loopback::write(&mut stream, &loopback::answer(&request, true)).await;
                    continue;
                };
                match pending.complete(Moment::now(), &origin.as_text(), code, state) {
                    Ok(finish) => {
                        loopback::write(&mut stream, &loopback::answer(&request, true)).await;
                        match self.finish(&id, &origin, &finish).await {
                            Ok(()) => say(self, "done", ""),
                            Err(message) => say(self, "failed", &message),
                        }
                        // The browser follows the redirect to the page that says it can be closed.
                        let until = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
                        while let Ok(Ok((mut s, _))) =
                            tokio::time::timeout_at(until, listener.accept()).await
                        {
                            if let Some(h) = loopback::read_head(&mut s).await {
                                let r = loopback::parse(&h, port);
                                let answer = if r == Request::Done {
                                    loopback::answer(&r, true)
                                } else {
                                    loopback::answer(&Request::Refuse(404), true)
                                };
                                loopback::write(&mut s, &answer).await;
                            }
                        }
                        return;
                    }
                    Err(Refusal::StateMismatch) => {
                        // Not this app's request: it changes nothing and cannot cancel the person's own.
                        loopback::write(&mut stream, &loopback::answer(&request, false)).await;
                    }
                    Err(_) => {
                        loopback::write(&mut stream, &loopback::answer(&request, false)).await;
                        say(self, "failed", "The sign-in took too long. Start again.");
                        return;
                    }
                }
            }
        }

        /// Trades the code for a session, keeps it, and learns who it is for.
        async fn finish(
            &self,
            id: &str,
            origin: &ServerOrigin,
            finish: &Finish,
        ) -> Result<(), String> {
            const REFUSED: &str = "The server did not accept the sign-in. Start again.";
            let http = remote::client().ok_or_else(|| REFUSED.to_string())?;
            let url = origin
                .join("/auth/app-signin/redeem")
                .ok_or_else(|| REFUSED.to_string())?;
            let body = finish.redeem_body();
            let response = http
                .post(url)
                .header("origin", origin.as_text())
                .header("content-type", "application/json")
                .header("accept", "application/json")
                .body(body.expose().to_string())
                .send()
                .await
                .map_err(|_| {
                    "The server could not be reached. Check the connection and start again."
                        .to_string()
                })?;
            if !response.status().is_success() {
                return Err(REFUSED.to_string());
            }
            let set = remote::session_set_cookie(response.headers())
                .ok_or_else(|| REFUSED.to_string())?;
            let parsed = tauri::webview::Cookie::parse(set).map_err(|_| REFUSED.to_string())?;
            if parsed.value().is_empty() {
                return Err(REFUSED.to_string());
            }
            self.put_cookie(id, parsed.value())?;
            // Who the server says this is (best effort: a failure here does not undo the sign-in).
            let _ = self.check(id).await;
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use crate::local::secrets::FileStore;

        fn accounts(name: &str) -> (Accounts, PathBuf) {
            let dir = std::env::temp_dir()
                .join(format!("dynodoc-accounts-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            (Accounts::new(&dir, FileStore::new(&dir.join("s"))), dir)
        }

        #[test]
        fn a_server_is_added_once_whatever_the_spelling_and_kept_across_a_restart() {
            let (a, dir) = accounts("add");
            let first = a.add("Dynodoc.Example.org").unwrap();
            assert_eq!(first.server, "https://dynodoc.example.org");
            assert!(!first.signed_in && !first.cloud);
            assert_eq!(a.add("https://dynodoc.example.org/").unwrap().id, first.id);
            assert_eq!(a.views().len(), 1);
            let again = Accounts::new(&dir, FileStore::new(&dir.join("s")));
            assert_eq!(again.views().len(), 1);
            assert_eq!(
                again.find(&first.id).unwrap().server,
                "https://dynodoc.example.org"
            );
            assert!(
                a.add("http://dynodoc.example.org").is_err(),
                "plain http only to this computer"
            );
            assert!(a.add("https://user@evil.example").is_err());
            assert!(a.add("javascript:alert(1)").is_err());
            assert!(a.add("https://dynodoc.example.org/path").is_err());
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn a_session_is_in_the_store_and_never_in_the_list() {
            let (a, dir) = accounts("session");
            let v = a.add("https://app.dynodoc.online").unwrap();
            assert!(v.cloud);
            a.put_cookie(&v.id, "CANARY-session-value").unwrap();
            assert!(a.view(&v.id).unwrap().signed_in);
            let file = std::fs::read_to_string(dir.join(FILE)).unwrap();
            assert!(!file.contains("CANARY"), "the list holds no session");
            let leaked = serde_json::to_string(&a.views()).unwrap();
            assert!(!leaked.contains("CANARY"), "the page is told no session");
            // A renewed cookie replaces it; a cleared one signs the person out here.
            a.apply_set_cookie(
                &v.id,
                "dynodoc_session=NEW-value; Path=/; HttpOnly; Max-Age=2592000",
            );
            assert_eq!(a.cookie(&v.id).unwrap().as_str(), "NEW-value");
            a.apply_set_cookie(&v.id, "other=1; Path=/");
            assert_eq!(
                a.cookie(&v.id).unwrap().as_str(),
                "NEW-value",
                "only the session cookie counts"
            );
            a.apply_set_cookie(&v.id, "dynodoc_session=; Path=/; Max-Age=0");
            assert!(a.cookie(&v.id).is_none());
            assert!(!a.view(&v.id).unwrap().signed_in);
            a.put_cookie(&v.id, "again").unwrap();
            a.remove(&v.id).unwrap();
            assert!(a.find(&v.id).is_none() && a.cookie(&v.id).is_none());
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[tokio::test]
        async fn a_link_sign_in_takes_only_the_link_with_its_own_state() {
            let (a, dir) = accounts("link");
            let v = a.add("http://127.0.0.1:1").unwrap();
            let seen = std::sync::Mutex::new(String::new());
            let pairing = a
                .begin(&v.id, true, |url| {
                    *seen.lock().unwrap() = url.to_string();
                    Ok(())
                })
                .await
                .unwrap();
            assert_eq!(pairing.len(), 4);
            let url = url::Url::parse(&seen.lock().unwrap()).unwrap();
            let keys: Vec<String> = url.query_pairs().map(|(k, _)| k.to_string()).collect();
            assert_eq!(
                keys,
                ["challenge", "state", "platform"],
                "no loopback port, exactly the three values"
            );
            let state = url
                .query_pairs()
                .find(|(k, _)| k == "state")
                .unwrap()
                .1
                .to_string();
            let code = "ab".repeat(32);
            assert_eq!(a.flow().state, "waiting");
            // No link, a link for another attempt, and a link for nothing at all change nothing.
            assert!(!a.complete_link(&code, "not-this-attempts-state-0123456789"));
            assert_eq!(a.flow().state, "waiting");
            // Its own state is taken (the trade then fails: nothing listens on that port).
            assert!(a.complete_link(&code, &state));
            assert!(
                !a.complete_link(&code, &state),
                "an attempt is used up by its link"
            );
            for _ in 0..50 {
                if a.flow().state != "waiting" {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            assert_eq!(a.flow().state, "failed");
            assert!(!a.view(&v.id).unwrap().signed_in);
            let _ = std::fs::remove_dir_all(&dir);
        }

        #[test]
        fn a_list_file_that_was_edited_into_something_odd_gives_only_the_good_entries() {
            let (_, dir) = accounts("odd");
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join(FILE),
                r#"{"version":1,"accounts":[
                    {"id":"srv_0123abcd","server":"https://dynodoc.example.org","added_at":1},
                    {"id":"../x","server":"https://dynodoc.example.org","added_at":1},
                    {"id":"srv_ffff0000","server":"http://evil.example","added_at":1},
                    {"id":"srv_aaaa1111","server":"https://Dynodoc.Example.org","added_at":1}]}"#,
            )
            .unwrap();
            let b = Accounts::new(&dir, FileStore::new(&dir.join("s")));
            let ids: Vec<String> = b.views().into_iter().map(|v| v.id).collect();
            assert_eq!(ids, vec!["srv_0123abcd".to_string()]);
            let _ = std::fs::remove_dir_all(&dir);
        }
    }
}
