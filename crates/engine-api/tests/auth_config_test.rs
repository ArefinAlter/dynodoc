//! Startup must fail closed before database connection/migration in every environment.
use std::process::Command;

#[test]
fn missing_or_placeholder_credentials_prevent_startup_without_production_flag() {
    for environment in [None, Some("preview"), Some("production")] {
        for service_key in [
            None,
            Some(""),
            Some("short"),
            Some("                                "),
        ] {
            let mut command = Command::new(env!("CARGO_BIN_EXE_engine-api"));
            command
                .env_remove("APP_ENV")
                .env_remove("WEB_SERVICE_KEY")
                .env("PASETO_LOCAL_KEY", "07".repeat(32))
                .env("DATABASE_URL", "intentionally-invalid");
            if let Some(environment) = environment {
                command.env("APP_ENV", environment);
            }
            if let Some(service_key) = service_key {
                command.env("WEB_SERVICE_KEY", service_key);
            }
            let output = command.output().unwrap();
            assert!(!output.status.success());
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("WEB_SERVICE_KEY"), "{stderr}");
            assert!(!stderr.contains("connect to Postgres"), "{stderr}");
        }
    }
    let output = Command::new(env!("CARGO_BIN_EXE_engine-api"))
        .env_remove("APP_ENV")
        .env("WEB_SERVICE_KEY", "isolated-configuration-test-service-key")
        .env("PASETO_LOCAL_KEY", "00".repeat(32))
        .env("DATABASE_URL", "intentionally-invalid")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("random PASETO_LOCAL_KEY"));
}

#[test]
fn api_rejects_migration_credentials_even_with_valid_auth_configuration() {
    let output = Command::new(env!("CARGO_BIN_EXE_engine-api"))
        .env("MIGRATION_DATABASE_URL", "never-use-this-owner-url")
        .env("DATABASE_URL", "intentionally-invalid")
        .env("PASETO_LOCAL_KEY", "07".repeat(32))
        .env("WEB_SERVICE_KEY", "isolated-configuration-test-service-key")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("MIGRATION_DATABASE_URL must not be present"));
    assert!(!stderr.contains("never-use-this-owner-url"));
}

const KEY: &str = "0707070707070707070707070707070707070707070707070707070707070707";
const SERVICE: &str = "isolated-configuration-test-service-key";

/// A secret file only its owner can read (what the API requires on Unix).
fn secret_file(text: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("dynodoc-config-test-{}", uuid::Uuid::new_v4()));
    std::fs::write(&path, format!("{text}\n")).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    path
}

fn clean_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_engine-api"));
    for name in [
        "PASETO_LOCAL_KEY",
        "PASETO_LOCAL_KEY_FILE",
        "WEB_SERVICE_KEY",
        "WEB_SERVICE_KEY_FILE",
        "DATABASE_URL",
        "DATABASE_URL_FILE",
        "MIGRATION_DATABASE_URL",
        "MIGRATION_DATABASE_URL_FILE",
    ] {
        command.env_remove(name);
    }
    command
}

#[test]
fn file_secrets_are_read_and_every_problem_is_reported_together() {
    let (key, service, url) = (
        secret_file(KEY),
        secret_file(SERVICE),
        secret_file("intentionally-invalid"),
    );
    let output = clean_command()
        .env("PASETO_LOCAL_KEY_FILE", &key)
        .env("WEB_SERVICE_KEY_FILE", &service)
        .env("DATABASE_URL_FILE", &url)
        .env("RETENTION_AUTH_DAYS", "0")
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The two valid file secrets raise nothing; the two real problems are both listed.
    assert!(
        stderr.contains("invalid configuration (2 problems)"),
        "{stderr}"
    );
    assert!(
        stderr.contains("DATABASE_URL is not a valid PostgreSQL connection URL"),
        "{stderr}"
    );
    assert!(
        stderr.contains("RETENTION_AUTH_DAYS must be a whole number from 1 to 365"),
        "{stderr}"
    );
    for secret in [KEY, SERVICE, "intentionally-invalid"] {
        assert!(!stderr.contains(secret), "{stderr}");
    }
    assert!(!stderr.contains("connect to Postgres"), "{stderr}");
    for path in [key, service, url] {
        let _ = std::fs::remove_file(path);
    }
}

#[test]
fn a_secret_given_both_ways_or_by_a_missing_file_stops_startup() {
    let service = secret_file(SERVICE);
    let absent = std::env::temp_dir().join(format!("dynodoc-absent-{}", uuid::Uuid::new_v4()));
    let output = clean_command()
        .env("PASETO_LOCAL_KEY", KEY)
        .env("PASETO_LOCAL_KEY_FILE", &service)
        .env("WEB_SERVICE_KEY_FILE", &absent)
        .env(
            "DATABASE_URL",
            "postgres://runtime:do-not-print-this@127.0.0.1:1/dynodoc",
        )
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("invalid configuration (2 problems)"),
        "{stderr}"
    );
    assert!(
        stderr.contains("PASETO_LOCAL_KEY and PASETO_LOCAL_KEY_FILE are both set"),
        "{stderr}"
    );
    assert!(stderr.contains("WEB_SERVICE_KEY_FILE ("), "{stderr}");
    for secret in [KEY, SERVICE, "do-not-print-this"] {
        assert!(!stderr.contains(secret), "{stderr}");
    }
    let _ = std::fs::remove_file(service);
}

#[test]
fn the_migration_credential_is_refused_in_file_form_too() {
    let output = clean_command()
        .env("MIGRATION_DATABASE_URL_FILE", "/run/secrets/never-read")
        .env("DATABASE_URL", "intentionally-invalid")
        .env("PASETO_LOCAL_KEY", KEY)
        .env("WEB_SERVICE_KEY", SERVICE)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("MIGRATION_DATABASE_URL must not be present"),
        "{stderr}"
    );
}
