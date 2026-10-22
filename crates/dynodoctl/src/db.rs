//! Everything the tool does to PostgreSQL and to `engine-cli`, always through the containers of
//! the bundle: owner credentials are read inside a container from its own secret files and never
//! pass through this process.

use crate::compose::{check_ident, short, Compose, PG_CONNECT};
use crate::error::{CliError, Result};
use crate::runner::Output;
use std::path::Path;

/// The live database.
pub const LIVE_DB: &str = "dynodoc";

#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DbFacts {
    pub migrations_applied: i64,
    pub latest_migration: i64,
    pub events: i64,
    pub documents: i64,
    pub identities: i64,
    pub server_version: String,
}

/// One query for everything a backup, a status and a restore comparison need.
const FACTS_SQL: &str = "select (select count(*) from _sqlx_migrations where success), (select coalesce(max(version),0) from _sqlx_migrations where success), (select count(*) from event), (select count(*) from document), (select count(*) from identity), current_setting('server_version')";

pub fn parse_facts(text: &str) -> Option<DbFacts> {
    let line = text.lines().find(|l| l.matches('|').count() >= 5)?;
    let mut parts = line.trim().splitn(6, '|');
    let mut int = || parts.next().and_then(|p| p.trim().parse::<i64>().ok());
    let applied = int()?;
    let latest = int()?;
    let events = int()?;
    let documents = int()?;
    let identities = int()?;
    let server_version = parts.next()?.trim().to_string();
    Some(DbFacts { migrations_applied: applied, latest_migration: latest, events, documents, identities, server_version })
}

pub fn read_facts(c: &Compose<'_>, db: &str) -> Result<DbFacts> {
    let out = c.psql(db, FACTS_SQL)?;
    if !out.ok() {
        return Err(CliError::failed(
            format!("could not read the database `{db}`: {}", short(&out.text())),
            "check that the stack is running (`dynodoctl status`) and that the database has been initialised",
        ));
    }
    parse_facts(&out.stdout_text()).ok_or_else(|| CliError::failed(format!("unexpected answer from the database: {}", short(&out.text())), "run `dynodoctl status` and report this output"))
}

/// Has `init` ever run on this database?
pub fn has_schema(c: &Compose<'_>, db: &str) -> Result<bool> {
    let out = c.psql(db, "select to_regclass('public._sqlx_migrations') is not null")?;
    Ok(out.ok() && out.stdout_text().trim() == "t")
}

pub fn size_bytes(c: &Compose<'_>, db: &str) -> Result<u64> {
    check_ident(db)?;
    let out = c.psql("postgres", &format!("select pg_database_size('{db}')"))?;
    out.stdout_text().trim().parse::<u64>().map_err(|_| CliError::failed(format!("could not read the size of `{db}`: {}", short(&out.text())), "check `dynodoctl status`"))
}

pub fn create_database(c: &Compose<'_>, name: &str) -> Result<()> {
    check_ident(name)?;
    let out = c.psql("postgres", &format!("create database {name}"))?;
    if out.ok() {
        Ok(())
    } else {
        Err(CliError::failed(format!("could not create the scratch database `{name}`: {}", short(&out.text())), "check the free disk space of the database volume and that PostgreSQL is healthy"))
    }
}

/// Drop a database even if sessions remain (PostgreSQL 13+; the bundle ships 16).
pub fn drop_database(c: &Compose<'_>, name: &str) -> Result<()> {
    check_ident(name)?;
    if name == LIVE_DB {
        return Err(CliError::failed("internal: refusing to drop the live database", "this is a bug in dynodoctl; please report it"));
    }
    let out = c.psql("postgres", &format!("drop database if exists {name} with (force)"))?;
    if out.ok() {
        Ok(())
    } else {
        Err(CliError::failed(format!("could not remove the database `{name}`: {}", short(&out.text())), format!("remove it by hand: docker compose exec postgres sh -c 'PGPASSWORD=$(cat /run/secrets/postgres_password) dropdb -h 127.0.0.1 -U dynodoc {name}'")))
    }
}

pub fn list_scratch_databases(c: &Compose<'_>) -> Result<Vec<String>> {
    let out = c.psql("postgres", "select datname from pg_database where datname like 'dynodoc!_restore!_%' escape '!' or datname like 'dynodoc!_restoring!_%' escape '!' order by 1")?;
    if !out.ok() {
        return Ok(Vec::new());
    }
    Ok(out.stdout_text().lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
}

/// `pg_restore` of a custom-format dump into `db`, read from the file on standard input.
pub fn restore_dump(c: &Compose<'_>, db: &str, dump: &Path) -> Result<Output> {
    check_ident(db)?;
    c.pg_sh_io(&format!("pg_restore {PG_CONNECT} -d {db} --no-owner --no-acl --exit-on-error"), Some(dump.to_path_buf()), None)
}

/// Number of entries `pg_restore --list` finds in a dump (proves it is a readable custom-format dump).
pub fn toc_entries(c: &Compose<'_>, dump: &Path) -> Result<u64> {
    let out = c.pg_sh_io("pg_restore --list", Some(dump.to_path_buf()), None)?;
    if !out.ok() {
        return Err(CliError::failed(format!("the dump cannot be read by pg_restore: {}", short(&out.text())), "do not rely on this backup; take a new one"));
    }
    Ok(out.stdout_text().lines().filter(|l| !l.trim().is_empty() && !l.starts_with(';')).count() as u64)
}

/// Ids of the documents in `db`, in a stable order.
pub fn document_ids(c: &Compose<'_>, db: &str) -> Result<Vec<String>> {
    let out = c.psql(db, "select id from document order by id")?;
    if !out.ok() {
        return Err(CliError::failed(format!("could not list the documents: {}", short(&out.text())), "check `dynodoctl status`"));
    }
    let ids: Vec<String> = out.stdout_text().lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    if let Some(bad) = ids.iter().find(|id| !id.chars().all(|ch| ch.is_ascii_hexdigit() || ch == '-')) {
        return Err(CliError::failed(format!("unexpected document id `{bad}`"), "run `dynodoctl status` and report this"));
    }
    Ok(ids)
}

/// engine-cli against database `db` with the owner login in `MIGRATION_DATABASE_URL` and the
/// runtime login in `DATABASE_URL`, exactly as the bundle's own jobs set them (jobs/init.sh,
/// jobs/engine-cli.sh), except that the database name is a parameter.
const ENGINE_SCRIPT: &str = r#"set -eu
DB=$1; shift
OWNER_PW=$(cat /run/secrets/postgres_password)
RUNTIME_PW=$(cat /run/secrets/runtime_database_password)
export MIGRATION_DATABASE_URL="postgres://dynodoc:${OWNER_PW}@postgres:5432/${DB}"
export DATABASE_URL="postgres://dynodoc_runtime:${RUNTIME_PW}@postgres:5432/${DB}"
exec engine-cli "$@"
"#;

/// Verify every document one by one (ids arrive on standard input; a command line would be too
/// long for Windows with a thousand documents).
const VERIFY_SCRIPT: &str = r#"set -eu
DB=$1
OWNER_PW=$(cat /run/secrets/postgres_password)
RUNTIME_PW=$(cat /run/secrets/runtime_database_password)
export MIGRATION_DATABASE_URL="postgres://dynodoc:${OWNER_PW}@postgres:5432/${DB}"
export DATABASE_URL="postgres://dynodoc_runtime:${RUNTIME_PW}@postgres:5432/${DB}"
n=0
while read -r id; do
  [ -n "$id" ] || continue
  engine-cli verify "$id"
  n=$((n+1))
done
echo "verified $n documents"
"#;

/// Start the API image against `db` and wait for `/healthz`. It runs in a one-off container on the
/// bundle's private network and publishes nothing.
const SMOKE_SCRIPT: &str = r#"set -u
DB=$1
URL=$(cat /run/secrets/database_url)
export DATABASE_URL="${URL%/*}/${DB}"
unset DATABASE_URL_FILE
engine-api >/tmp/api.log 2>&1 &
pid=$!
i=0; ok=0
while [ "$i" -lt 60 ]; do
  if bash -c 'exec 3<>/dev/tcp/127.0.0.1/8080 && printf "GET /healthz HTTP/1.0\r\nHost: api\r\n\r\n" >&3 && head -n 1 <&3 | grep -q " 200 "' 2>/dev/null; then ok=1; break; fi
  kill -0 "$pid" 2>/dev/null || break
  i=$((i+1)); sleep 1
done
kill "$pid" 2>/dev/null || true
wait "$pid" 2>/dev/null || true
if [ "$ok" = 1 ]; then echo "api smoke: /healthz answered 200 after ${i}s"; exit 0; fi
echo "api smoke: /healthz did not answer 200"; tail -n 5 /tmp/api.log | cut -c1-300; exit 1
"#;

pub fn engine_cli(c: &Compose<'_>, db: &str, args: &[&str]) -> Result<Output> {
    check_ident(db)?;
    let mut all = vec![db];
    all.extend_from_slice(args);
    c.run_script("database-admin", ENGINE_SCRIPT, &all, None)
}

pub fn verify_documents(c: &Compose<'_>, db: &str, ids: &[String]) -> Result<Output> {
    check_ident(db)?;
    let mut input = ids.join("\n");
    input.push('\n');
    c.run_script("database-admin", VERIFY_SCRIPT, &[db], Some(input.into_bytes()))
}

pub fn api_smoke(c: &Compose<'_>, db: &str) -> Result<Output> {
    check_ident(db)?;
    c.run_script("api", SMOKE_SCRIPT, &[db], None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::{Ctx, Layout, ScriptedConfirm};
    use crate::runner::FakeRunner;

    #[test]
    fn facts_parse_from_one_psql_line() {
        let f = parse_facts("52|52|36|2|9|16.4 (Debian 16.4-1.pgdg120+2)\n").unwrap();
        assert_eq!((f.migrations_applied, f.latest_migration, f.events, f.documents, f.identities), (52, 52, 36, 2, 9));
        assert!(f.server_version.starts_with("16.4"));
        assert!(parse_facts("ERROR: relation does not exist").is_none());
        assert!(parse_facts("1|2|x|4|5|16").is_none());
    }

    fn with_ctx<T>(runner: &FakeRunner, f: impl FnOnce(&Compose<'_>) -> T) -> T {
        let dir = tempfile::tempdir().unwrap();
        let confirm = ScriptedConfirm::new(&[]);
        let ctx = Ctx::new(runner, Layout::new(dir.path()), &confirm);
        f(&Compose::new(&ctx))
    }

    #[test]
    fn the_live_database_can_never_be_dropped_by_the_scratch_helper() {
        let runner = FakeRunner::new();
        let err = with_ctx(&runner, |c| drop_database(c, LIVE_DB)).unwrap_err();
        assert!(err.what.contains("refusing"));
        assert!(runner.calls().is_empty());
    }

    #[test]
    fn drop_uses_force_and_if_exists() {
        let runner = FakeRunner::new();
        with_ctx(&runner, |c| drop_database(c, "dynodoc_restore_1")).unwrap();
        assert!(runner.ran("drop database if exists dynodoc_restore_1 with (force)"));
    }

    #[test]
    fn restore_uses_no_owner_no_acl_and_stops_on_error() {
        let runner = FakeRunner::new();
        with_ctx(&runner, |c| restore_dump(c, "dynodoc_restore_1", Path::new("/tmp/x.dump"))).unwrap();
        let line = runner.lines().pop().unwrap();
        assert!(line.contains("pg_restore -h 127.0.0.1 -U dynodoc -d dynodoc_restore_1 --no-owner --no-acl --exit-on-error"));
        assert_eq!(runner.calls()[0].stdin_file.as_deref(), Some(Path::new("/tmp/x.dump")));
    }

    #[test]
    fn toc_counts_entries_and_ignores_comment_lines() {
        let runner = FakeRunner::new();
        runner.on("pg_restore --list", 0, "; Archive created at ...\n; dbname: dynodoc\n1; 2615 2200 SCHEMA - public x\n2; 1259 1 TABLE public event x\n");
        assert_eq!(with_ctx(&runner, |c| toc_entries(c, Path::new("/tmp/x.dump"))).unwrap(), 2);
        let bad = FakeRunner::new();
        bad.on("pg_restore --list", 1, "");
        assert!(with_ctx(&bad, |c| toc_entries(c, Path::new("/tmp/x.dump"))).is_err());
    }

    #[test]
    fn document_ids_must_look_like_uuids() {
        let runner = FakeRunner::new();
        runner.on("select id from document", 0, "0190a1b2-0000-7000-8000-000000000001\n0190a1b2-0000-7000-8000-000000000002\n");
        assert_eq!(with_ctx(&runner, |c| document_ids(c, "dynodoc")).unwrap().len(), 2);
        let evil = FakeRunner::new();
        evil.on("select id from document", 0, "x; rm -rf /\n");
        assert!(with_ctx(&evil, |c| document_ids(c, "dynodoc")).is_err());
    }

    #[test]
    fn engine_cli_runs_in_a_one_off_container_with_the_database_as_first_argument() {
        let runner = FakeRunner::new();
        with_ctx(&runner, |c| engine_cli(c, "dynodoc_restore_1", &["migrate"])).unwrap();
        let line = runner.lines().pop().unwrap();
        assert!(line.contains("--profile maintenance run --rm --no-deps -T --entrypoint /bin/sh database-admin -c"));
        assert!(line.ends_with(" sh dynodoc_restore_1 migrate"));
        assert!(!line.contains("postgres_password") || line.contains("cat /run/secrets"), "passwords are read inside the container");
    }

    #[test]
    fn verify_sends_ids_on_standard_input() {
        let runner = FakeRunner::new();
        with_ctx(&runner, |c| verify_documents(c, "dynodoc", &["a-1".to_string(), "b-2".to_string()])).unwrap();
        assert_eq!(runner.calls()[0].stdin_bytes.as_deref(), Some(&b"a-1\nb-2\n"[..]));
    }
}
