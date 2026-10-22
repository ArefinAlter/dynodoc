//! `engine-cli auth issue-link | list-links | revoke-link` as the real process, against a real
//! database (docs/decisions/018-operator-issued-sign-in-links.md): the link is printed once, on
//! standard output alone, and appears nowhere else; the command works with the owner credential and
//! not with the runtime login; every argument is checked before the database is touched.
use std::process::{Command, Output};

use ring::digest::{digest, SHA256};
use sqlx::PgPool;

const ORIGIN: &str = "https://docs.example.org";

/// The owner URL of the disposable database `sqlx::test` made: `DATABASE_URL` with the database swapped.
fn owner_url(pool: &PgPool) -> String {
    let base = std::env::var("DATABASE_URL").expect("DATABASE_URL");
    let server = base.rsplit_once('/').unwrap().0;
    format!(
        "{server}/{}",
        pool.connect_options().get_database().unwrap()
    )
}

fn run(owner: Option<&str>, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_engine-cli"));
    command
        .arg("auth")
        .args(args)
        // The most talkative setting there is: nothing may reach it.
        .env("RUST_LOG", "trace")
        .env_remove("DATABASE_URL")
        .env_remove("ORIGIN")
        .env_remove("ADMIN_EMAILS");
    match owner {
        Some(url) => command.env("MIGRATION_DATABASE_URL", url),
        None => command.env_remove("MIGRATION_DATABASE_URL"),
    };
    command.output().unwrap()
}

fn out(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}
fn err(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// The token in a printed link, after checking the link's whole shape.
fn token_of(link: &str) -> String {
    let token = link
        .trim()
        .strip_prefix(&format!("{ORIGIN}/login?token="))
        .unwrap_or_else(|| panic!("not a sign-in link: {link:?}"));
    assert_eq!(token.len(), 64);
    assert!(token
        .bytes()
        .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')));
    token.to_string()
}

fn sha(text: &str) -> Vec<u8> {
    digest(&SHA256, text.as_bytes()).as_ref().to_vec()
}

async fn account(pool: &PgPool, email: &str) {
    sqlx::query("insert into identity(email) values($1)")
        .bind(email)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_link_is_printed_once_on_standard_output_and_nowhere_else(pool: PgPool) {
    let url = owner_url(&pool);
    let o = run(
        Some(&url),
        &[
            "issue-link",
            "--email",
            "Admin@Example.org",
            "--origin",
            ORIGIN,
            "--create",
            "--admin-emails",
            "admin@example.org",
            "--role",
            "admin",
            "--ttl",
            "20m",
            "--operator",
            "alice",
            "--reason",
            "first administrator",
        ],
    );
    assert!(o.status.success(), "{}", err(&o));
    // Standard output is the link and a newline: nothing else.
    let stdout = out(&o);
    assert_eq!(stdout.lines().count(), 1, "{stdout:?}");
    assert!(stdout.ends_with('\n'));
    let token = token_of(&stdout);
    // The explanation is on standard error, and the token is not in it, nor in the trace-level log.
    let stderr = err(&o);
    assert!(
        stderr.contains("admin@example.org") && stderr.contains("administrator"),
        "{stderr}"
    );
    assert!(stderr.contains("The account was created"), "{stderr}");
    assert!(!stderr.contains(&token), "the token is on standard error");
    assert_eq!(
        format!("{stdout}{stderr}").matches(&token).count(),
        1,
        "printed once"
    );
    // The database holds only the hash; the audit row names the operator and the reason.
    let stored: Vec<u8> = sqlx::query_scalar("select token_hash from operator_sign_in_link")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, sha(&token));
    let (operator, reason, ttl, created): (String, String, i64, bool) = sqlx::query_as(
        "select detail->>'operator', detail->>'reason', (detail->>'ttl_seconds')::bigint, (detail->>'created_account')::bool
         from operation_audit where action='auth.operator_link.issued'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        (operator.as_str(), reason.as_str(), ttl, created),
        ("alice", "first administrator", 1200, true)
    );
    let table: String =
        sqlx::query_scalar("select string_agg(t::text, ' ') from operator_sign_in_link t")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!table.contains(&token));
    // The command line never held the token (it was not an argument), and neither does the listing.
    let listed = run(Some(&url), &["list-links"]);
    assert!(listed.status.success(), "{}", err(&listed));
    let listing = format!("{}{}", out(&listed), err(&listed));
    assert!(listing.contains(&hex::encode(&stored[..4])), "{listing}");
    assert!(
        listing.contains("admin@example.org") && listing.contains("alice"),
        "{listing}"
    );
    assert!(!listing.contains(&token));
}

#[sqlx::test(migrations = "../../migrations")]
async fn json_mode_is_one_object_that_holds_the_link(pool: PgPool) {
    let url = owner_url(&pool);
    account(&pool, "jo@example.org").await;
    let o = run(
        Some(&url),
        &[
            "issue-link",
            "--email",
            "jo@example.org",
            "--origin",
            ORIGIN,
            "--json",
            "--operator",
            "bob",
        ],
    );
    assert!(o.status.success(), "{}", err(&o));
    let value: serde_json::Value = serde_json::from_str(out(&o).trim()).unwrap();
    assert_eq!(out(&o).lines().count(), 1);
    let token = token_of(value["link"].as_str().unwrap());
    assert_eq!(value["ok"], true);
    assert_eq!(value["ttl_seconds"], 900);
    assert_eq!(value["created_account"], false);
    assert_eq!(value["token_prefix"], hex::encode(&sha(&token)[..4]));
    assert!(!err(&o).contains(&token));
}

#[sqlx::test(migrations = "../../migrations")]
async fn creating_an_account_the_rule_does_not_allow_stops_with_exit_3_and_changes_nothing(
    pool: PgPool,
) {
    let url = owner_url(&pool);
    account(&pool, "admin@example.org").await;
    let with = |email: &str, extra: &[&str]| -> Vec<String> {
        let mut args: Vec<String> = [
            "issue-link",
            "--origin",
            ORIGIN,
            "--create",
            "--admin-emails",
            "admin@example.org",
            "--operator",
            "carol",
            "--email",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        args.push(email.to_string());
        args.extend(extra.iter().map(|s| s.to_string()));
        args
    };
    let go = |args: Vec<String>| {
        run(
            Some(&url),
            &args.iter().map(String::as_str).collect::<Vec<_>>(),
        )
    };
    let o = go(with("new@example.org", &[]));
    assert_eq!(o.status.code(), Some(3), "{}", err(&o));
    assert!(out(&o).is_empty(), "no link, no output");
    assert!(
        err(&o).contains("--confirm-create new@example.org"),
        "{}",
        err(&o)
    );
    let o = go(with("new@example.org", &["--json"]));
    assert_eq!(o.status.code(), Some(3));
    let value: serde_json::Value = serde_json::from_str(out(&o).trim()).unwrap();
    assert_eq!(value["needs_confirmation"], "administrator_exists");
    let accounts: i64 = sqlx::query_scalar("select count(*) from identity")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(accounts, 1);
    // A confirmation that is not this address is the same stop.
    let o = go(with(
        "newer@example.org",
        &["--confirm-create", "new@example.org"],
    ));
    assert_eq!(o.status.code(), Some(3), "{}", err(&o));
    // The second confirmation: the address typed again.
    let o = go(with(
        "new@example.org",
        &["--confirm-create", "new@example.org"],
    ));
    assert!(o.status.success(), "{}", err(&o));
    token_of(&out(&o));
    // Other refusals are plain failures (exit 1) with a reason, and no link.
    for bad in [
        vec!["--email", "ghost@example.org"],
        vec!["--email", "admin@example.org", "--role", "user"],
    ] {
        let o = run(
            Some(&url),
            &[
                &[
                    "issue-link",
                    "--origin",
                    ORIGIN,
                    "--admin-emails",
                    "admin@example.org",
                    "--operator",
                    "carol",
                ][..],
                &bad,
            ]
            .concat(),
        );
        assert_eq!(o.status.code(), Some(1), "{bad:?}: {}", err(&o));
        assert!(out(&o).is_empty());
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn bad_arguments_are_refused_before_the_database_is_used(pool: PgPool) {
    // A URL that cannot connect: if the arguments were checked after connecting, the error would
    // be about the connection instead.
    let nowhere = "postgres://nobody:nothing@127.0.0.1:1/none";
    let _ = pool;
    for (args, wanted) in [
        (
            vec![
                "--email",
                "a@example.org",
                "--origin",
                ORIGIN,
                "--ttl",
                "5d",
            ],
            "lifetime",
        ),
        (
            vec![
                "--email",
                "a@example.org",
                "--origin",
                ORIGIN,
                "--ttl",
                "59s",
            ],
            "lifetime",
        ),
        (
            vec![
                "--email",
                "a@example.org",
                "--origin",
                ORIGIN,
                "--ttl",
                "25h",
            ],
            "lifetime",
        ),
        (
            vec![
                "--email",
                "a@example.org",
                "--origin",
                ORIGIN,
                "--ttl",
                "0m",
            ],
            "lifetime",
        ),
        (
            vec!["--email", "a@example.org", "--origin", ORIGIN, "--ttl=-5m"],
            "lifetime",
        ),
        (
            vec![
                "--email",
                "a@example.org",
                "--origin",
                "http://docs.example.org",
            ],
            "origin",
        ),
        (
            vec![
                "--email",
                "a@example.org",
                "--origin",
                "https://docs.example.org/x",
            ],
            "origin",
        ),
        (
            vec![
                "--email",
                "a@example.org",
                "--origin",
                "https://u@docs.example.org",
            ],
            "origin",
        ),
        (vec!["--email", "a@example.org"], "web address"),
        (
            vec![
                "--email",
                "a@example.org",
                "--origin",
                ORIGIN,
                "--role",
                "root",
            ],
            "root",
        ),
    ] {
        let o = run(Some(nowhere), &[&["issue-link"][..], &args].concat());
        assert!(!o.status.success(), "{args:?}");
        assert!(out(&o).is_empty(), "{args:?}");
        let message = err(&o).to_lowercase();
        assert!(message.contains(wanted), "{args:?}: {message}");
        assert!(
            !message.contains("could not connect"),
            "{args:?}: {message}"
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn without_the_owner_credential_it_does_nothing(pool: PgPool) {
    let o = run(
        None,
        &["issue-link", "--email", "a@example.org", "--origin", ORIGIN],
    );
    assert!(!o.status.success());
    assert!(out(&o).is_empty());
    assert!(
        err(&o).contains("MIGRATION_DATABASE_URL is required"),
        "{}",
        err(&o)
    );
    // DATABASE_URL (the API's own login) is not a substitute.
    let o = Command::new(env!("CARGO_BIN_EXE_engine-cli"))
        .args([
            "auth",
            "issue-link",
            "--email",
            "a@example.org",
            "--origin",
            ORIGIN,
        ])
        .env_remove("MIGRATION_DATABASE_URL")
        .env("DATABASE_URL", owner_url(&pool))
        .output()
        .unwrap();
    assert!(!o.status.success());
    assert!(out(&o).is_empty());
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_runtime_login_cannot_issue_a_link_even_through_the_tool(pool: PgPool) {
    let role = format!("operator_cli_{}", uuid::Uuid::new_v4().simple());
    let password = uuid::Uuid::new_v4().simple().to_string();
    sqlx::raw_sql(&format!(
        "create role {role} login password '{password}' nosuperuser nocreatedb nocreaterole noreplication nobypassrls"
    ))
    .execute(&pool)
    .await
    .unwrap();
    engine_core::database::grant_runtime(&pool, &role)
        .await
        .unwrap();
    account(&pool, "rt@example.org").await;
    let options = pool.connect_options();
    let runtime_url = format!(
        "postgres://{role}:{password}@{}:{}/{}",
        options.get_host(),
        options.get_port(),
        options.get_database().unwrap()
    );
    let o = run(
        Some(&runtime_url),
        &[
            "issue-link",
            "--email",
            "rt@example.org",
            "--origin",
            ORIGIN,
        ],
    );
    assert!(!o.status.success(), "{}", out(&o));
    assert!(out(&o).is_empty(), "no link was printed");
    assert!(
        err(&o).to_lowercase().contains("permission denied"),
        "{}",
        err(&o)
    );
    let links: i64 = sqlx::query_scalar("select count(*) from operator_sign_in_link")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(links, 0);
    sqlx::raw_sql(&format!("drop owned by {role}; drop role {role}"))
        .execute(&pool)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn list_and_revoke_work_from_the_hash_prefix_alone(pool: PgPool) {
    let url = owner_url(&pool);
    account(&pool, "lr@example.org").await;
    let issue = |who: &str| {
        run(
            Some(&url),
            &[
                "issue-link",
                "--email",
                "lr@example.org",
                "--origin",
                ORIGIN,
                "--operator",
                who,
            ],
        )
    };
    let first = issue("dana");
    let token = token_of(&out(&first));
    let prefix = hex::encode(&sha(&token)[..4]);
    // The second link ends the first, and says so.
    let second = issue("dana");
    assert!(err(&second).contains("1 earlier link"), "{}", err(&second));
    let live = out(&run(Some(&url), &["list-links"]));
    assert_eq!(live.lines().count(), 1, "{live}");
    assert!(!live.contains(&prefix) || prefix == hex::encode(&sha(&token_of(&out(&second)))[..4]));
    let all = out(&run(Some(&url), &["list-links", "--all"]));
    assert_eq!(all.lines().count(), 2, "{all}");
    assert!(all.contains(&prefix) && all.contains("revoked"), "{all}");
    // Revoke the live one by prefix; short prefixes and unknown ones are refused.
    let live_prefix = live.split_whitespace().next().unwrap().to_string();
    for bad in ["abc", "zzzzzzzz", "ffffffffff"] {
        let o = run(Some(&url), &["revoke-link", bad]);
        assert!(!o.status.success(), "{bad}");
    }
    let o = run(
        Some(&url),
        &["revoke-link", &live_prefix, "--operator", "erin"],
    );
    assert!(o.status.success(), "{}", err(&o));
    assert!(out(&run(Some(&url), &["list-links"])).contains("No outstanding"));
    let revoked_by: String = sqlx::query_scalar(
        "select detail->>'operator' from operation_audit where action='auth.operator_link.revoked'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(revoked_by, "erin");
}

#[sqlx::test(migrations = "../../migrations")]
async fn the_web_address_may_come_from_the_environment(pool: PgPool) {
    let url = owner_url(&pool);
    account(&pool, "env@example.org").await;
    let o = Command::new(env!("CARGO_BIN_EXE_engine-cli"))
        .args([
            "auth",
            "issue-link",
            "--email",
            "env@example.org",
            "--operator",
            "frank",
        ])
        .env("MIGRATION_DATABASE_URL", &url)
        .env("ORIGIN", ORIGIN)
        .env("RUST_LOG", "off")
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", err(&o));
    token_of(&out(&o));
}
