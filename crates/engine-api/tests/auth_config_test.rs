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
