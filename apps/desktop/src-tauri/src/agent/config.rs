//! What the person has allowed the agent, and how much it may use.
//!
//! Kept by the app (not by the page) in `agent.json` in the config folder:
//! - **consent**, per provider entry and per host: a provider is called only after the person saw
//!   what would be sent and allowed that host (off by default; a changed address asks again);
//! - **offline**: one switch that stops every model call;
//! - **caps** on tokens per thread and per 24 hours and on estimated cost per 24 hours (a cap of
//!   zero means no model call at all);
//! - **prices**, the local table the cost estimate uses (per model, in millionths of a dollar per
//!   million tokens), which the person edits. It starts empty: a price the app made up would be a
//!   claim about somebody's pricing.

use super::types::AgentError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FILE: &str = "agent.json";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Consent {
    pub host: String,
    pub at: i64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct Price {
    /// Millionths of a dollar per million input tokens.
    pub input: u64,
    pub output: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConfig {
    pub offline: bool,
    pub consent: BTreeMap<String, Consent>,
    pub thread_token_cap: u64,
    pub day_token_cap: u64,
    /// Millionths of a dollar.
    pub day_cost_cap: u64,
    pub prices: BTreeMap<String, Price>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            offline: false,
            consent: BTreeMap::new(),
            thread_token_cap: 200_000,
            day_token_cap: 1_000_000,
            day_cost_cap: 5_000_000,
            prices: BTreeMap::new(),
        }
    }
}

pub fn path(config_dir: &Path) -> PathBuf {
    config_dir.join(FILE)
}

pub fn load(config_dir: &Path) -> AgentConfig {
    std::fs::read_to_string(path(config_dir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(config_dir: &Path, c: &AgentConfig) -> Result<(), String> {
    std::fs::create_dir_all(config_dir)
        .map_err(|_| "The agent's settings could not be saved.".to_string())?;
    let text = serde_json::to_string_pretty(c)
        .map_err(|_| "The agent's settings could not be saved.".to_string())?;
    let tmp = path(config_dir).with_extension("json.tmp");
    std::fs::write(&tmp, text)
        .map_err(|_| "The agent's settings could not be saved.".to_string())?;
    std::fs::rename(&tmp, path(config_dir))
        .map_err(|_| "The agent's settings could not be saved.".to_string())
}

/// The estimated cost of a call in millionths of a dollar; `None` when no price is set for the model.
pub fn cost(c: &AgentConfig, model: &str, input: u64, output: u64) -> Option<u64> {
    let p = c.prices.get(model)?;
    Some((input.saturating_mul(p.input) + output.saturating_mul(p.output)) / 1_000_000)
}

/// Whether a call may be made now. Order matters: offline, consent, then caps.
pub fn allow(c: &AgentConfig, provider_id: &str, host: &str) -> Result<(), AgentError> {
    if c.offline {
        return Err(AgentError::Offline);
    }
    match c.consent.get(provider_id) {
        Some(k) if k.host == host => {}
        _ => return Err(AgentError::NoConsent),
    }
    if c.thread_token_cap == 0 || c.day_token_cap == 0 || c.day_cost_cap == 0 {
        return Err(AgentError::CapReached(
            "A cap in Settings, Providers is zero, which means no model is called.".into(),
        ));
    }
    Ok(())
}

/// Whether more may be used given what has been used (tokens in a thread, tokens and cost in 24 hours).
pub fn within_caps(
    c: &AgentConfig,
    thread_tokens: u64,
    day_tokens: u64,
    day_cost: u64,
) -> Result<(), AgentError> {
    if thread_tokens >= c.thread_token_cap {
        return Err(AgentError::CapReached(format!("This thread has used its {} tokens. Raise the cap in Settings, Providers, or start a new thread.", c.thread_token_cap)));
    }
    if day_tokens >= c.day_token_cap {
        return Err(AgentError::CapReached(format!(
            "The last 24 hours have used the cap of {} tokens. Raise it in Settings, Providers.",
            c.day_token_cap
        )));
    }
    if day_cost >= c.day_cost_cap {
        return Err(AgentError::CapReached("The estimated cost of the last 24 hours has reached its cap. Raise it in Settings, Providers.".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed() -> AgentConfig {
        let mut c = AgentConfig::default();
        c.consent.insert(
            "lp1".into(),
            Consent {
                host: "api.example.org".into(),
                at: 1,
            },
        );
        c
    }

    #[test]
    fn nothing_is_called_without_consent_for_this_host_and_never_when_offline() {
        let mut c = AgentConfig::default();
        assert_eq!(
            allow(&c, "lp1", "api.example.org"),
            Err(AgentError::NoConsent)
        );
        c = allowed();
        assert_eq!(allow(&c, "lp1", "api.example.org"), Ok(()));
        assert_eq!(
            allow(&c, "lp1", "other.example.org"),
            Err(AgentError::NoConsent),
            "a changed address asks again"
        );
        assert_eq!(
            allow(&c, "lp2", "api.example.org"),
            Err(AgentError::NoConsent)
        );
        c.offline = true;
        assert_eq!(
            allow(&c, "lp1", "api.example.org"),
            Err(AgentError::Offline)
        );
    }

    #[test]
    fn a_cap_of_zero_means_no_call_and_a_reached_cap_stops() {
        for zero in 0..3 {
            let mut c = allowed();
            match zero {
                0 => c.thread_token_cap = 0,
                1 => c.day_token_cap = 0,
                _ => c.day_cost_cap = 0,
            }
            assert!(
                matches!(
                    allow(&c, "lp1", "api.example.org"),
                    Err(AgentError::CapReached(_))
                ),
                "{zero}"
            );
        }
        let c = allowed();
        assert!(within_caps(&c, 10, 10, 0).is_ok());
        assert!(within_caps(&c, c.thread_token_cap, 0, 0).is_err());
        assert!(within_caps(&c, 0, c.day_token_cap, 0).is_err());
        assert!(within_caps(&c, 0, 0, c.day_cost_cap).is_err());
    }

    #[test]
    fn cost_needs_a_price_and_is_in_millionths_of_a_dollar() {
        let mut c = AgentConfig::default();
        assert_eq!(cost(&c, "m", 1000, 1000), None);
        c.prices.insert(
            "m".into(),
            Price {
                input: 3_000_000,
                output: 15_000_000,
            },
        );
        // $3 and $15 per million tokens: 2,000 in and 1,000 out cost $0.006 + $0.015.
        assert_eq!(cost(&c, "m", 2000, 1000), Some(21_000));
    }

    #[test]
    fn the_settings_round_trip_through_a_file() {
        let dir = std::env::temp_dir().join(format!("dynodoc-agentcfg-{}", std::process::id()));
        let mut c = allowed();
        c.offline = true;
        save(&dir, &c).unwrap();
        assert_eq!(load(&dir), c);
        assert_eq!(load(&dir.join("nowhere")), AgentConfig::default());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
