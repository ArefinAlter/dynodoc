//! Startup configuration: every secret and setting the API binary reads, validated
//! together so a misconfigured deployment gets one error that lists all problems.
//!
//! Secrets come from `NAME` or `NAME_FILE` ([`crate::secrets`]). No message built here
//! contains a secret value, or any part of one.
use crate::{
    retention::RetentionConfig,
    secrets::{self, Lookup, Secret},
    AuthConfig,
};
use sqlx::postgres::PgConnectOptions;
use std::str::FromStr;

/// Everything `main` needs before it touches the database.
pub struct StartupConfig {
    pub auth: AuthConfig,
    /// The runtime login's connection URL (holds its password).
    pub database_url: Secret,
    pub retention: RetentionConfig,
}

impl StartupConfig {
    /// Load from the process environment.
    pub fn from_env() -> anyhow::Result<Self> {
        Self::load(&|name| std::env::var_os(name))
    }

    /// Load from any environment. Fails with every problem found, one per line.
    pub fn load(lookup: Lookup) -> anyhow::Result<Self> {
        let mut problems = Vec::new();
        let paseto_key = match secret(lookup, "PASETO_LOCAL_KEY", &mut problems) {
            Ok(Some(value)) => parse_paseto_key(&value)
                .map_err(|problem| problems.push(problem))
                .ok(),
            Ok(None) => {
                problems.push(
                    "PASETO_LOCAL_KEY is not set (32 bytes, hex-encoded; or set PASETO_LOCAL_KEY_FILE)"
                        .into(),
                );
                None
            }
            Err(()) => None,
        };
        let service_key = match secret(lookup, "WEB_SERVICE_KEY", &mut problems) {
            Ok(Some(value)) if value.expose().len() >= 32 && !value.expose().trim().is_empty() => {
                Some(value)
            }
            Ok(_) => {
                problems.push(
                    "WEB_SERVICE_KEY of at least 32 characters is required in every environment"
                        .into(),
                );
                None
            }
            Err(()) => None,
        };
        // The API must never hold owner credentials, in either form.
        if lookup("MIGRATION_DATABASE_URL").is_some()
            || lookup("MIGRATION_DATABASE_URL_FILE").is_some()
        {
            problems.push(
                "MIGRATION_DATABASE_URL must not be present in the API environment (nor MIGRATION_DATABASE_URL_FILE)"
                    .into(),
            );
        }
        let database_url = match secret(lookup, "DATABASE_URL", &mut problems) {
            Ok(Some(url)) if PgConnectOptions::from_str(url.expose()).is_ok() => Some(url),
            Ok(Some(_)) => {
                problems.push("DATABASE_URL is not a valid PostgreSQL connection URL".into());
                None
            }
            Ok(None) => {
                problems.push(
                    "DATABASE_URL is not set (see .env / SETUP.md; or set DATABASE_URL_FILE)"
                        .into(),
                );
                None
            }
            Err(()) => None,
        };
        let retention = RetentionConfig::load(lookup, &mut problems);

        let text = |name: &str| lookup(name).and_then(|v| v.into_string().ok());
        match (paseto_key, service_key, database_url) {
            (Some(paseto_key), Some(service_key), Some(database_url)) if problems.is_empty() => {
                Ok(Self {
                    auth: AuthConfig {
                        paseto_key,
                        service_key: Some(service_key.expose().to_string()),
                        token_ttl_seconds: text("TOKEN_TTL_SECONDS")
                            .and_then(|s| s.parse().ok())
                            .unwrap_or(86_400),
                        public_base_url: text("PUBLIC_API_BASE_URL")
                            .unwrap_or_else(|| "http://127.0.0.1:8080".to_string()),
                    },
                    database_url,
                    retention,
                })
            }
            _ => anyhow::bail!(
                "invalid configuration ({} problem{}):\n  - {}",
                problems.len(),
                if problems.len() == 1 { "" } else { "s" },
                problems.join("\n  - ")
            ),
        }
    }
}

/// Read a secret. A read problem is recorded here and reported as `Err(())`.
fn secret(lookup: Lookup, name: &str, problems: &mut Vec<String>) -> Result<Option<Secret>, ()> {
    secrets::read(lookup, name).map_err(|error| problems.push(error.to_string()))
}

/// 32 random bytes as 64 hex characters (surrounding whitespace is ignored).
fn parse_paseto_key(secret: &Secret) -> Result<[u8; 32], String> {
    let bytes = hex::decode(secret.expose().trim())
        .map_err(|_| "PASETO_LOCAL_KEY must be hex-encoded".to_string())?;
    let key: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| "PASETO_LOCAL_KEY must be 32 bytes (64 hex chars)".to_string())?;
    if key == [0u8; 32] {
        return Err("A random PASETO_LOCAL_KEY is required in every environment".into());
    }
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::HashMap, ffi::OsString};

    const KEY: &str = "0707070707070707070707070707070707070707070707070707070707070707";
    const SERVICE: &str = "isolated-configuration-test-service-key";
    const URL: &str = "postgres://runtime:p4ssw0rd-do-not-leak@127.0.0.1:5432/dynodoc";

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let map: HashMap<String, OsString> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), OsString::from(v)))
            .collect();
        move |name| map.get(name).cloned()
    }

    fn valid<'a>() -> Vec<(&'a str, &'a str)> {
        vec![
            ("PASETO_LOCAL_KEY", KEY),
            ("WEB_SERVICE_KEY", SERVICE),
            ("DATABASE_URL", URL),
        ]
    }

    fn error_of(pairs: &[(&str, &str)]) -> String {
        match StartupConfig::load(&env(pairs)) {
            Ok(_) => panic!("configuration was accepted"),
            Err(error) => error.to_string(),
        }
    }

    #[test]
    fn plain_variables_load_with_documented_defaults() {
        let config = StartupConfig::load(&env(&valid())).unwrap();
        assert_eq!(config.auth.paseto_key, [7u8; 32]);
        assert_eq!(config.auth.service_key.as_deref(), Some(SERVICE));
        assert_eq!(config.auth.token_ttl_seconds, 86_400);
        assert_eq!(config.auth.public_base_url, "http://127.0.0.1:8080");
        assert_eq!(config.database_url.expose(), URL);
        assert_eq!(config.retention, RetentionConfig::default());
    }

    #[test]
    fn plain_values_are_used_without_trimming_except_the_hex_key() {
        let padded_key = format!(" {KEY}\n");
        let padded_service = format!(" {SERVICE}\n");
        let mut pairs = valid();
        pairs[0].1 = &padded_key;
        pairs[1].1 = &padded_service;
        pairs.push(("TOKEN_TTL_SECONDS", "3600"));
        pairs.push(("PUBLIC_API_BASE_URL", "https://dynodoc.example.test"));
        let config = StartupConfig::load(&env(&pairs)).unwrap();
        assert_eq!(config.auth.paseto_key, [7u8; 32]);
        assert_eq!(
            config.auth.service_key.as_deref(),
            Some(padded_service.as_str())
        );
        assert_eq!(config.auth.token_ttl_seconds, 3600);
        assert_eq!(config.auth.public_base_url, "https://dynodoc.example.test");
    }

    #[test]
    fn every_problem_is_listed_once_in_one_error() {
        let message = error_of(&[
            ("PASETO_LOCAL_KEY", "zz"),
            ("WEB_SERVICE_KEY", "short"),
            ("DATABASE_URL", "intentionally-invalid"),
            ("MIGRATION_DATABASE_URL", "never-use-this-owner-url"),
            ("RETENTION_AUTH_DAYS", "0"),
        ]);
        assert!(
            message.starts_with("invalid configuration (5 problems):"),
            "{message}"
        );
        for expected in [
            "PASETO_LOCAL_KEY must be hex-encoded",
            "WEB_SERVICE_KEY of at least 32 characters is required",
            "DATABASE_URL is not a valid PostgreSQL connection URL",
            "MIGRATION_DATABASE_URL must not be present",
            "RETENTION_AUTH_DAYS must be a whole number from 1 to 365",
        ] {
            assert_eq!(
                message.matches(expected).count(),
                1,
                "{expected}\n{message}"
            );
        }
        assert!(!message.contains("never-use-this-owner-url"), "{message}");
        assert!(!message.contains("short"), "{message}");
    }

    #[test]
    fn missing_secrets_are_each_named() {
        let message = error_of(&[]);
        for name in ["PASETO_LOCAL_KEY", "WEB_SERVICE_KEY", "DATABASE_URL"] {
            assert!(
                message.contains(&format!("{name} is not set"))
                    || message.contains(&format!("{name} of at least")),
                "{name}\n{message}"
            );
        }
    }

    #[test]
    fn key_shape_errors_keep_their_wording_and_never_echo_the_key() {
        let zero = "00".repeat(32);
        let short = "07".repeat(31);
        let odd = format!("{KEY}0");
        for (value, expected) in [
            (zero.as_str(), "A random PASETO_LOCAL_KEY is required"),
            (
                short.as_str(),
                "PASETO_LOCAL_KEY must be 32 bytes (64 hex chars)",
            ),
            (odd.as_str(), "PASETO_LOCAL_KEY must be hex-encoded"),
            (
                "not-hex-at-all-0123456789abcdef",
                "PASETO_LOCAL_KEY must be hex-encoded",
            ),
        ] {
            let mut pairs = valid();
            pairs[0].1 = value;
            let message = error_of(&pairs);
            assert!(message.contains(expected), "{message}");
            assert!(!message.contains(value), "{message}");
        }
        for value in ["", "short", "                                "] {
            let mut pairs = valid();
            pairs[1].1 = value;
            assert!(error_of(&pairs).contains("WEB_SERVICE_KEY of at least 32 characters"));
        }
    }

    #[test]
    fn migration_credentials_are_refused_in_both_forms() {
        for name in ["MIGRATION_DATABASE_URL", "MIGRATION_DATABASE_URL_FILE"] {
            let mut pairs = valid();
            pairs.push((name, ""));
            assert!(error_of(&pairs).contains("MIGRATION_DATABASE_URL must not be present"));
        }
    }

    #[test]
    fn file_variants_load_and_conflicts_are_errors() {
        let dir = std::env::temp_dir();
        let write = |text: &str| {
            let path = dir.join(format!("dynodoc-config-{}", uuid::Uuid::new_v4()));
            std::fs::write(&path, format!("{text}\n")).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
            }
            path
        };
        let (key, service, url) = (write(KEY), write(SERVICE), write(URL));
        let pairs = [
            ("PASETO_LOCAL_KEY_FILE", key.to_str().unwrap()),
            ("WEB_SERVICE_KEY_FILE", service.to_str().unwrap()),
            ("DATABASE_URL_FILE", url.to_str().unwrap()),
        ];
        let config = StartupConfig::load(&env(&pairs)).unwrap();
        assert_eq!(config.auth.paseto_key, [7u8; 32]);
        assert_eq!(config.auth.service_key.as_deref(), Some(SERVICE));
        assert_eq!(config.database_url.expose(), URL);

        let mut both = pairs.to_vec();
        both.push(("WEB_SERVICE_KEY", SERVICE));
        let message = error_of(&both);
        assert!(
            message.contains("WEB_SERVICE_KEY and WEB_SERVICE_KEY_FILE are both set"),
            "{message}"
        );
        assert_eq!(
            message.matches("WEB_SERVICE_KEY of at least").count(),
            0,
            "{message}"
        );

        let missing = dir.join(format!("dynodoc-absent-{}", uuid::Uuid::new_v4()));
        let mut unreadable = pairs.to_vec();
        unreadable[2].1 = missing.to_str().unwrap();
        let message = error_of(&unreadable);
        assert!(message.contains("DATABASE_URL_FILE ("), "{message}");
        assert!(
            message.starts_with("invalid configuration (1 problem):"),
            "{message}"
        );
        for path in [key, service, url] {
            let _ = std::fs::remove_file(path);
        }
    }
}
