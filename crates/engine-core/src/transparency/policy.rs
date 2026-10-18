//! The client trust policy (design 4.5) and its validating loader.
//!
//! ```json
//! { "version": 1,
//!   "log": { "origin": "...", "vkey": "..." },
//!   "witnesses": [ { "name": "...", "vkey": "...", "operator": "..." } ],
//!   "quorum": { "threshold": 2, "distinct_operators": true },
//!   "freshness": { "max_age_seconds": 86400 },
//!   "on_failure": "warn" }
//! ```
//!
//! The loader refuses a policy that could never be met or that blurs roles: a
//! threshold of zero or above the number of witnesses (or of distinct operators),
//! a log key that is also a witness, repeated witness names or keys, a witness
//! key that is not a cosignature key, a zero freshness window, unknown fields.
//! Quorum intersection (`2 * threshold > n`, design 5.1) is reported by
//! [`Policy::quorum_is_majority`] rather than enforced, because the design lists a
//! one-witness rollout.
use std::collections::HashSet;

use serde::Deserialize;

use super::note::{KeyKind, Origin, VerifierKey};
use super::Error;

/// Why a policy was refused. One variant per rule, so callers and tests can tell
/// them apart.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PolicyError {
    #[error("not a valid policy document: {0}")]
    Json(String),
    #[error("unsupported policy version {0} (only 1 is defined)")]
    Version(u64),
    #[error("log origin is invalid: {0}")]
    Origin(String),
    #[error("log vkey is invalid: {0}")]
    LogKey(String),
    #[error("log vkey must be an Ed25519 note key (type 0x01), not a cosignature key")]
    LogKeyKind,
    #[error("log vkey is named {key} but the log origin is {origin}; the log key is named by its origin")]
    LogKeyName { origin: String, key: String },
    #[error("witness {name}: vkey is invalid: {reason}")]
    WitnessKey { name: String, reason: String },
    #[error("witness {name}: vkey must be a cosignature key (type 0x04)")]
    WitnessKeyKind { name: String },
    #[error("witness {name}: the vkey is named {key}")]
    WitnessName { name: String, key: String },
    #[error("witness name {name} is listed twice")]
    DuplicateWitnessName { name: String },
    #[error("witness {name} repeats another witness's key")]
    DuplicateWitnessKey { name: String },
    #[error("witness {name} uses the log's name or key; log and witness keys must be distinct")]
    WitnessIsLog { name: String },
    #[error("witness {name}: operator must be a non-empty label without control characters")]
    Operator { name: String },
    #[error("quorum threshold must be at least 1")]
    ThresholdZero,
    #[error("quorum threshold {threshold} exceeds the {witnesses} listed witnesses")]
    ThresholdAboveWitnesses { threshold: u64, witnesses: usize },
    #[error("quorum threshold {threshold} exceeds the {operators} distinct operators")]
    ThresholdAboveOperators { threshold: u64, operators: usize },
    #[error("freshness.max_age_seconds must be at least 1")]
    FreshnessZero,
    #[error("on_failure must be \"warn\" or \"reject\", not {0:?}")]
    OnFailure(String),
}

impl From<PolicyError> for Error {
    fn from(error: PolicyError) -> Self {
        Self::Policy(error)
    }
}

/// What a consumer does when the required assurance level is not reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnFailure {
    Warn,
    Reject,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Witness {
    pub name: String,
    pub key: VerifierKey,
    pub operator: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Policy {
    origin: Origin,
    log: VerifierKey,
    witnesses: Vec<Witness>,
    threshold: usize,
    distinct_operators: bool,
    max_age_seconds: u64,
    on_failure: OnFailure,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyWire {
    version: u64,
    log: LogWire,
    witnesses: Vec<WitnessWire>,
    quorum: QuorumWire,
    freshness: FreshnessWire,
    on_failure: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LogWire {
    origin: String,
    vkey: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WitnessWire {
    name: String,
    vkey: String,
    operator: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QuorumWire {
    threshold: u64,
    distinct_operators: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FreshnessWire {
    max_age_seconds: u64,
}

impl Policy {
    /// Parse and validate a policy document.
    pub fn from_json(bytes: &[u8]) -> Result<Self, Error> {
        let wire: PolicyWire =
            serde_json::from_slice(bytes).map_err(|e| PolicyError::Json(e.to_string()))?;
        if wire.version != 1 {
            return Err(PolicyError::Version(wire.version).into());
        }
        let origin =
            Origin::new(&wire.log.origin).map_err(|e| PolicyError::Origin(e.to_string()))?;
        let log =
            VerifierKey::parse(&wire.log.vkey).map_err(|e| PolicyError::LogKey(e.to_string()))?;
        if log.kind() != KeyKind::Log {
            return Err(PolicyError::LogKeyKind.into());
        }
        if log.name() != origin.as_str() {
            return Err(PolicyError::LogKeyName {
                origin: origin.as_str().to_string(),
                key: log.name().to_string(),
            }
            .into());
        }
        let mut witnesses: Vec<Witness> = Vec::with_capacity(wire.witnesses.len());
        let mut names = HashSet::new();
        let mut keys = HashSet::new();
        for w in wire.witnesses {
            let name = w.name;
            let key = VerifierKey::parse(&w.vkey).map_err(|e| PolicyError::WitnessKey {
                name: name.clone(),
                reason: e.to_string(),
            })?;
            if key.kind() != KeyKind::Cosigner {
                return Err(PolicyError::WitnessKeyKind { name }.into());
            }
            if key.name() != name {
                return Err(PolicyError::WitnessName {
                    key: key.name().to_string(),
                    name,
                }
                .into());
            }
            if name == origin.as_str() || key.public() == log.public() {
                return Err(PolicyError::WitnessIsLog { name }.into());
            }
            if !names.insert(name.clone()) {
                return Err(PolicyError::DuplicateWitnessName { name }.into());
            }
            if !keys.insert(*key.public()) {
                return Err(PolicyError::DuplicateWitnessKey { name }.into());
            }
            if w.operator.is_empty() || w.operator.chars().any(char::is_control) {
                return Err(PolicyError::Operator { name }.into());
            }
            witnesses.push(Witness {
                name,
                key,
                operator: w.operator,
            });
        }
        let threshold = wire.quorum.threshold;
        if threshold == 0 {
            return Err(PolicyError::ThresholdZero.into());
        }
        if threshold > witnesses.len() as u64 {
            return Err(PolicyError::ThresholdAboveWitnesses {
                threshold,
                witnesses: witnesses.len(),
            }
            .into());
        }
        let operators = witnesses
            .iter()
            .map(|w| w.operator.as_str())
            .collect::<HashSet<_>>()
            .len();
        if wire.quorum.distinct_operators && threshold > operators as u64 {
            return Err(PolicyError::ThresholdAboveOperators {
                threshold,
                operators,
            }
            .into());
        }
        if wire.freshness.max_age_seconds == 0 {
            return Err(PolicyError::FreshnessZero.into());
        }
        let on_failure = match wire.on_failure.as_str() {
            "warn" => OnFailure::Warn,
            "reject" => OnFailure::Reject,
            other => return Err(PolicyError::OnFailure(other.to_string()).into()),
        };
        Ok(Self {
            origin,
            log,
            witnesses,
            threshold: threshold as usize,
            distinct_operators: wire.quorum.distinct_operators,
            max_age_seconds: wire.freshness.max_age_seconds,
            on_failure,
        })
    }

    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    pub fn log_key(&self) -> &VerifierKey {
        &self.log
    }

    pub fn witnesses(&self) -> &[Witness] {
        &self.witnesses
    }

    pub fn threshold(&self) -> usize {
        self.threshold
    }

    pub fn distinct_operators(&self) -> bool {
        self.distinct_operators
    }

    pub fn max_age_seconds(&self) -> u64 {
        self.max_age_seconds
    }

    pub fn on_failure(&self) -> OnFailure {
        self.on_failure
    }

    /// Whether any two accepted quorums must share a witness (`2 * threshold > n`,
    /// design 5.1), where `n` is the number of distinct operators when operators
    /// are counted once and the number of witnesses otherwise. Without this a fork
    /// needs no dishonest witness in an intersection.
    pub fn quorum_is_majority(&self) -> bool {
        let voters = if self.distinct_operators {
            self.witnesses
                .iter()
                .map(|w| w.operator.as_str())
                .collect::<HashSet<_>>()
                .len()
        } else {
            self.witnesses.len()
        };
        2 * self.threshold > voters
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transparency::note::Signer;
    use serde_json::{json, Value};

    const ORIGIN: &str = "log.dynodoc.example/dynodoc/v1/transparency";

    fn witness(name: &str, operator: &str, seed: u8) -> Value {
        let key = Signer::from_seed(name, KeyKind::Cosigner, &[seed; 32]).unwrap();
        json!({"name": name, "vkey": key.verifier_key().to_string(), "operator": operator})
    }

    fn policy_json() -> Value {
        let log = Signer::from_seed(ORIGIN, KeyKind::Log, &[1; 32]).unwrap();
        json!({
            "version": 1,
            "log": {"origin": ORIGIN, "vkey": log.verifier_key().to_string()},
            "witnesses": [
                witness("w1.verne.example/w", "verne", 2),
                witness("w2.uni-a.example/w", "uni-a", 3),
                witness("w3.customer.example/w", "customer", 4),
            ],
            "quorum": {"threshold": 2, "distinct_operators": true},
            "freshness": {"max_age_seconds": 86400},
            "on_failure": "warn",
        })
    }

    fn load(value: &Value) -> Result<Policy, Error> {
        Policy::from_json(&serde_json::to_vec(value).unwrap())
    }

    fn rejected(value: &Value) -> PolicyError {
        match load(value) {
            Err(Error::Policy(e)) => e,
            other => panic!("expected a policy error, got {other:?}"),
        }
    }

    #[test]
    fn the_design_example_shape_loads() {
        let policy = load(&policy_json()).unwrap();
        assert_eq!(policy.origin().as_str(), ORIGIN);
        assert_eq!(policy.witnesses().len(), 3);
        assert_eq!(policy.threshold(), 2);
        assert!(policy.distinct_operators());
        assert_eq!(policy.max_age_seconds(), 86400);
        assert_eq!(policy.on_failure(), OnFailure::Warn);
        assert!(policy.quorum_is_majority());
        let mut reject = policy_json();
        reject["on_failure"] = json!("reject");
        assert_eq!(load(&reject).unwrap().on_failure(), OnFailure::Reject);
    }

    #[test]
    fn majority_is_reported_not_enforced() {
        let mut p = policy_json();
        p["quorum"]["threshold"] = json!(1);
        let policy = load(&p).unwrap();
        assert!(!policy.quorum_is_majority());
        p["quorum"] = json!({"threshold": 2, "distinct_operators": false});
        p["witnesses"] = json!([
            witness("a/w", "x", 2),
            witness("b/w", "x", 3),
            witness("c/w", "x", 4),
            witness("d/w", "x", 5),
        ]);
        assert!(!load(&p).unwrap().quorum_is_majority());
    }

    #[test]
    fn every_rule_has_its_own_error() {
        let base = policy_json();
        let edit = |f: &dyn Fn(&mut Value)| {
            let mut p = base.clone();
            f(&mut p);
            rejected(&p)
        };
        assert!(matches!(
            edit(&|p| p["version"] = json!(2)),
            PolicyError::Version(2)
        ));
        assert!(matches!(
            edit(&|p| p["log"]["origin"] = json!("https://x/dynodoc/v1/transparency")),
            PolicyError::Origin(_)
        ));
        assert!(matches!(
            edit(&|p| p["log"]["vkey"] = json!("nonsense")),
            PolicyError::LogKey(_)
        ));
        assert!(matches!(
            edit(&|p| p["log"]["origin"] = json!("other.example/dynodoc/v1/transparency")),
            PolicyError::LogKeyName { .. }
        ));
        // A cosignature key where the log key belongs, and a log key as a witness.
        let w1 = base["witnesses"][0]["vkey"].clone();
        assert!(matches!(
            edit(&|p| p["log"]["vkey"] = w1.clone()),
            PolicyError::LogKeyKind | PolicyError::LogKeyName { .. }
        ));
        let log_vkey = base["log"]["vkey"].clone();
        assert!(matches!(
            edit(&|p| p["witnesses"][0]["vkey"] = log_vkey.clone()),
            PolicyError::WitnessKeyKind { .. }
        ));
        assert!(matches!(
            edit(&|p| p["witnesses"][0]["vkey"] = json!("bad")),
            PolicyError::WitnessKey { .. }
        ));
        assert!(matches!(
            edit(&|p| p["witnesses"][0]["name"] = json!("renamed/w")),
            PolicyError::WitnessName { .. }
        ));
        let w2 = base["witnesses"][1].clone();
        assert!(matches!(
            edit(&|p| p["witnesses"][0] = w2.clone()),
            PolicyError::DuplicateWitnessName { .. }
        ));
        // Same key under another name is a second vote from one key.
        let same_key = Signer::from_seed("alias.example/w", KeyKind::Cosigner, &[3; 32]).unwrap();
        assert!(matches!(
            edit(&|p| p["witnesses"][0] = json!({
                "name": "alias.example/w",
                "vkey": same_key.verifier_key().to_string(),
                "operator": "verne",
            })),
            PolicyError::DuplicateWitnessKey { .. }
        ));
        // A witness reusing the log's public key under a cosigner role.
        let log_pub = Signer::from_seed(ORIGIN, KeyKind::Log, &[1; 32])
            .unwrap()
            .verifier_key();
        let as_witness =
            VerifierKey::new("reuse.example/w", KeyKind::Cosigner, *log_pub.public()).unwrap();
        assert!(matches!(
            edit(&|p| p["witnesses"][0] = json!({
                "name": "reuse.example/w",
                "vkey": as_witness.to_string(),
                "operator": "verne",
            })),
            PolicyError::WitnessIsLog { .. }
        ));
        assert!(matches!(
            edit(&|p| p["witnesses"][0]["operator"] = json!("")),
            PolicyError::Operator { .. }
        ));
        assert!(matches!(
            edit(&|p| p["quorum"]["threshold"] = json!(0)),
            PolicyError::ThresholdZero
        ));
        assert!(matches!(
            edit(&|p| p["quorum"]["threshold"] = json!(4)),
            PolicyError::ThresholdAboveWitnesses {
                threshold: 4,
                witnesses: 3
            }
        ));
        assert!(matches!(
            edit(&|p| {
                p["witnesses"][1]["operator"] = json!("verne");
                p["quorum"]["threshold"] = json!(3);
            }),
            PolicyError::ThresholdAboveOperators {
                threshold: 3,
                operators: 2
            }
        ));
        assert!(matches!(
            edit(&|p| p["freshness"]["max_age_seconds"] = json!(0)),
            PolicyError::FreshnessZero
        ));
        assert!(matches!(
            edit(&|p| p["on_failure"] = json!("ignore")),
            PolicyError::OnFailure(_)
        ));
    }

    #[test]
    fn counting_operators_once_is_what_lets_one_org_hold_many_keys() {
        // Three keys, one operator: fine as a list, but a threshold of 2 distinct
        // operators is unreachable and refused; without the flag it is allowed.
        let mut p = policy_json();
        for w in p["witnesses"].as_array_mut().unwrap() {
            w["operator"] = json!("verne");
        }
        assert!(matches!(
            rejected(&p),
            PolicyError::ThresholdAboveOperators { .. }
        ));
        p["quorum"]["distinct_operators"] = json!(false);
        assert!(load(&p).is_ok());
    }

    #[test]
    fn malformed_documents_are_refused_with_a_reason() {
        for bad in [&b""[..], b"[]", b"{}", br#"{"version":1}"#] {
            assert!(matches!(
                Policy::from_json(bad),
                Err(Error::Policy(PolicyError::Json(_)))
            ));
        }
        let mut p = policy_json();
        p["surprise"] = json!(true);
        assert!(matches!(rejected(&p), PolicyError::Json(m) if m.contains("surprise")));
        let mut p = policy_json();
        p["quorum"]["extra"] = json!(1);
        assert!(matches!(rejected(&p), PolicyError::Json(_)));
        let mut p = policy_json();
        p["quorum"]["threshold"] = json!(-1);
        assert!(matches!(rejected(&p), PolicyError::Json(_)));
        let duplicate = serde_json::to_string(&policy_json()).unwrap().replacen(
            "\"version\":1",
            "\"version\":1,\"version\":1",
            1,
        );
        assert!(matches!(
            Policy::from_json(duplicate.as_bytes()),
            Err(Error::Policy(PolicyError::Json(_)))
        ));
    }
}
