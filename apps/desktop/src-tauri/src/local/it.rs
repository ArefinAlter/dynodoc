//! An end-to-end run of the local workspace against the real programs: PostgreSQL, the engine's
//! migrations, `engine-api`, the local sign-in, the client's own state, and a clean shutdown.
//! Loopback only, in a scratch folder, with a credential-store name of its own.
//!
//! Needs the programs: set `DYNODOC_PROGRAMS` to a folder with `engine-api`, `engine-cli` and a
//! `pg` tree (a debug build reads it), then
//! `cargo test local::it -- --ignored --nocapture --test-threads=1`.

use super::*;
use crate::remote::Method;
use serde_json::json;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dynodoc-it-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn wait_for(local: &Local, phase: &str, limit: u64) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(limit);
    loop {
        let s = local.status();
        if s.phase == phase {
            return;
        }
        assert!(s.phase != "failed", "failed: {:?}", s.detail);
        assert!(tokio::time::Instant::now() < deadline, "timed out at {} / {}", s.phase, s.step);
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs DYNODOC_PROGRAMS"]
async fn a_new_workspace_starts_signs_in_works_and_closes_cleanly() {
    let dir = scratch("one");
    let exe_dir = std::path::PathBuf::from(std::env::var("DYNODOC_PROGRAMS").expect("DYNODOC_PROGRAMS"));
    let local = Local::new(&dir, None, &exe_dir, false);
    local.start();
    wait_for(&local, "profile", 300).await;
    assert!(local.status().first_run);

    let profile = local.create_profile("Priya Raman", "").await.unwrap();
    assert!(profile.email.ends_with("@local.dynodoc.invalid"));
    assert!(local.is_ready());

    // The engine answers as the local person.
    let me = local.api(Method::Get, "profile", None, None).await.unwrap();
    assert_eq!(me.status, 200);
    assert_eq!(me.body["name"], "Priya Raman");
    let created = local
        .api(Method::Post, "projects", Some(&json!({ "name": "Riverside survey", "description": "" })), None)
        .await
        .unwrap();
    assert_eq!(created.status, 200, "{:?}", created.body);
    let list = local.api(Method::Get, "projects", None, None).await.unwrap();
    assert_eq!(list.body["items"].as_array().map(Vec::len), Some(1));

    // The client's own state.
    let put = local
        .svc(Method::Put, "state/working", Some(&json!({ "value": [{ "id": "w1" }] })))
        .await;
    assert_eq!(put.status, 200);
    let got = local.svc(Method::Get, "state/working", None).await;
    assert_eq!(got.body["value"][0]["id"], "w1");

    // Closing stops the engine and the database; the data directory stays.
    local.shutdown().await;
    assert!(local.api(Method::Get, "profile", None, None).await.is_err());
    assert!(dir.join("workspace").join("pgdata").join("PG_VERSION").exists());

    // A second start finds the cluster, needs no first run, and still knows the person and project.
    let again = Local::new(&dir, None, &exe_dir, false);
    again.start();
    wait_for(&again, "ready", 120).await;
    assert!(!again.status().first_run);
    let list = again.api(Method::Get, "projects", None, None).await.unwrap();
    assert_eq!(list.body["items"][0]["name"], "Riverside survey");
    again.shutdown().await;
    let _ = std::fs::remove_dir_all(&dir);
    // The secrets of the scratch workspace leave the credential store with it.
    let store = secrets::OsStore::new(&secrets::service_for(&dir, false));
    let _ = store.delete(super::SECRETS_ENTRY);
    let _ = store.delete(super::SESSION_ENTRY);
}
