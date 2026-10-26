//! The agent with the person's own model provider (bring your own key).
//!
//! Model requests are made by this process alone, straight to the provider the person chose, over
//! TLS, with their own key (`keys`), after they allowed that host once (`config`), inside caps they
//! set, and never while the offline switch is on. The page has no network and never sees a key. The
//! agent calls tools from a closed table (`tools`); the page runs them (they read what it already
//! holds of the workspace) and answers; anything with a consequence comes back as a card a person
//! presses. No tool approves, merges, commits or reaches outside the table.

pub mod client;
pub mod config;
pub mod keys;
pub mod run;
pub mod sse;
pub mod tools;
pub mod types;
pub mod wire;

#[cfg(test)]
mod testserver;

use crate::local::appdb::now_ms;
use crate::local::Local;
use config::AgentConfig;
use keys::KeyStore;
use run::ToolReply;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio::sync::{mpsc, watch};

pub struct RunHandle {
    pub cancel: watch::Sender<bool>,
    pub replies: mpsc::Sender<(String, ToolReply)>,
}

pub struct Agent {
    pub keys: KeyStore,
    config_dir: PathBuf,
    cfg: Mutex<AgentConfig>,
    pub runs: Mutex<HashMap<String, RunHandle>>,
}

impl Agent {
    pub fn new(config_dir: &Path, service: &str) -> Self {
        Self {
            keys: KeyStore::open(service, config_dir.join("provider-keys.enc")),
            config_dir: config_dir.to_path_buf(),
            cfg: Mutex::new(config::load(config_dir)),
            runs: Mutex::new(HashMap::new()),
        }
    }

    pub fn config(&self) -> AgentConfig {
        self.cfg.lock().map(|c| c.clone()).unwrap_or_default()
    }

    /// Changes the settings and saves them.
    pub fn update(&self, f: impl FnOnce(&mut AgentConfig)) -> Result<AgentConfig, String> {
        let mut guard = self.cfg.lock().map_err(|_| "The agent's settings are busy.".to_string())?;
        let mut next = guard.clone();
        f(&mut next);
        config::save(&self.config_dir, &next)?;
        *guard = next.clone();
        Ok(next)
    }
}

impl Local {
    /// Records one turn's use (tokens and the estimated cost, if a price is set).
    pub async fn ledger_add(&self, thread: &str, provider: &str, model: &str, input: u64, output: u64, cost: Option<u64>) {
        if let Ok(app) = self.app().await {
            let _ = app
                .execute(
                    "insert into usage_ledger (at, thread, provider, model, tokens_in, tokens_out, cost_micros) values ($1,$2,$3,$4,$5,$6,$7)",
                    &[&now_ms(), &thread, &provider, &model, &(input as i64), &(output as i64), &(cost.unwrap_or(0) as i64)],
                )
                .await;
        }
    }

    /// Tokens used in a thread, tokens and estimated cost used in the last 24 hours.
    pub async fn ledger_totals(&self, thread: &str) -> (u64, u64, u64) {
        let Ok(app) = self.app().await else { return (0, 0, 0) };
        let since = now_ms() - 86_400_000;
        let t: i64 = app
            .query_one("select coalesce(sum(tokens_in + tokens_out), 0)::bigint from usage_ledger where thread = $1", &[&thread])
            .await
            .map(|r| r.get(0))
            .unwrap_or(0);
        let d = app
            .query_one("select coalesce(sum(tokens_in + tokens_out), 0)::bigint, coalesce(sum(cost_micros), 0)::bigint from usage_ledger where at >= $1", &[&since])
            .await
            .map(|r| (r.get::<_, i64>(0), r.get::<_, i64>(1)))
            .unwrap_or((0, 0));
        (t.max(0) as u64, d.0.max(0) as u64, d.1.max(0) as u64)
    }
}
