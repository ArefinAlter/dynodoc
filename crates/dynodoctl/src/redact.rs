//! Redaction for support bundles. Two layers, both applied to every text that leaves the
//! machine: (1) the literal values of this installation's own secret files, held in memory only
//! and replaced wherever they appear; (2) patterns for credentials of any origin (URL user
//! information, query parameters such as `code=`, bearer and PASETO tokens, `name=value` pairs
//! whose NAME says it is sensitive, private-key blocks, long hex keys, and optionally email
//! addresses). Redaction can only hide too much, never too little by design; the bundle also
//! never reads a file from `secrets/` (see `support.rs`).
//!
//! Limits, stated plainly (docs/SELF-HOSTED-OPERATIONS.md, "support-bundle"): a secret that has
//! no sensitive name next to it and is not one of this installation's own secret values is not
//! recognised; email addresses are recognised in their ASCII, `%40`-encoded and Unicode-letter
//! forms but not in obfuscated forms such as "name at example dot org". Read a bundle before you
//! send it.

use regex::Regex;
use std::sync::OnceLock;

#[derive(Clone, Debug, Default)]
pub struct Redactor {
    /// (file name, value) of the installation's secrets; values shorter than 8 are not used.
    known: Vec<(String, String)>,
    pub emails: bool,
}

struct Patterns {
    url_userinfo: Regex,
    query_secret: Regex,
    paseto: Regex,
    jwt: Regex,
    bearer: Regex,
    name_separator: Regex,
    name_then_blob: Regex,
    private_key: Regex,
    hex_run: Regex,
    email: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        // The authority runs to the first `/`, `?` or `#`; the user information is everything
        // before the LAST `@` in it, so a password that contains `@` is hidden whole.
        url_userinfo: Regex::new(r"([A-Za-z][A-Za-z0-9+.\-]*://)([^\s/?#]*)@").expect("regex"),
        query_secret: Regex::new(
            r"(?i)([?&;](?:code|state|token|access_token|id_token|refresh_token|key|api_key|apikey|sig|signature|password|passwd|pwd|secret|client_secret|session|sid|auth|jwt|assertion)=)[^&\s#<>]*",
        )
        .expect("regex"),
        paseto: Regex::new(r"\bv[1-4]\.(?:local|public)\.[A-Za-z0-9_\-=.]+").expect("regex"),
        jwt: Regex::new(r"\beyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]*")
            .expect("regex"),
        // A bearer token of any length.
        bearer: Regex::new(r"(?i)\bbearer\s+[^\s]+").expect("regex"),
        // A name and its separator; the value is read by hand (`read_value`).
        name_separator: Regex::new(r#"([A-Za-z0-9_.\-]+)(["']?[ \t]*[:=][ \t]*)"#).expect("regex"),
        name_then_blob: Regex::new(
            r"(?i)\b(secret|token|password|passwd|pwd|key|credential)s?\b[ \t]+([A-Za-z0-9+/_\-]{20,}={0,2})",
        )
        .expect("regex"),
        private_key: Regex::new(
            r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?(?:-----END [A-Z ]*PRIVATE KEY-----|$)",
        )
        .expect("regex"),
        hex_run: Regex::new(r"\b[0-9a-fA-F]{48,}\b").expect("regex"),
        // ASCII `@`, `%40` and the full-width `＠`, with letters and digits of any script.
        email: Regex::new(
            r"(?i)[\p{L}\p{N}._%+\-]+(?:@|%40|＠)[\p{L}\p{N}\-]+(?:\.[\p{L}\p{N}\-]+)*\.\p{L}{2,}",
        )
        .expect("regex"),
    })
}

/// Words that make a name sensitive, matched against the name's parts (split at `_`, `.`, `-` and
/// lower-to-upper case changes; a trailing `s` is ignored): `smtp_password`, `sessionToken`,
/// `Set-Cookie`, `X-Api-Key` are sensitive; `secrets-init` (a service name) holds a structure and
/// is handled by `is_structure`.
const STRONG: [&str; 10] = [
    "password",
    "passwd",
    "pwd",
    "passphrase",
    "secret",
    "token",
    "credential",
    "bearer",
    "authorization",
    "cookie",
];
/// Weaker words: a short number or a boolean after one of these is left alone (`max_sessions=5`,
/// `status_code=200`).
const WEAK: [&str; 13] = [
    "key",
    "apikey",
    "session",
    "sess",
    "code",
    "signature",
    "sig",
    "auth",
    "private",
    "pass",
    "cred",
    "jwt",
    "otp",
];

fn name_parts(name: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut previous_lower = false;
    for c in name.chars() {
        if matches!(c, '_' | '.' | '-') {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
            }
            previous_lower = false;
            continue;
        }
        if c.is_ascii_uppercase() && previous_lower && !current.is_empty() {
            parts.push(std::mem::take(&mut current));
        }
        previous_lower = c.is_ascii_lowercase();
        current.push(c.to_ascii_lowercase());
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Sensitivity {
    No,
    Weak,
    Strong,
}

fn sensitivity(name: &str) -> Sensitivity {
    let mut result = Sensitivity::No;
    for part in name_parts(name) {
        let singular = part.strip_suffix('s').unwrap_or(&part);
        for candidate in [part.as_str(), singular] {
            if STRONG.contains(&candidate) {
                return Sensitivity::Strong;
            }
            if WEAK.contains(&candidate) {
                result = Sensitivity::Weak;
            }
        }
        // Compact forms such as `apikey`, `privatekey`, `accesskey`, `sessionid`.
        let compact = part.as_str();
        if [
            "apikey",
            "privatekey",
            "accesskey",
            "secretkey",
            "authtoken",
        ]
        .iter()
        .any(|w| compact.contains(w))
        {
            return Sensitivity::Strong;
        }
    }
    result
}

/// A path, a duration or a size is not a secret even when the name says "token".
fn harmless_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [
        "_file", "_path", "_dir", "_seconds", "_ttl", "_minutes", "_hours", "_days", "_bytes",
        "_size", "_count",
    ]
    .iter()
    .any(|s| lower.ends_with(s))
}

/// A YAML/JSON flow opening such as `{condition: ok}` is structure, not a value.
fn is_structure(value: &str) -> bool {
    (value.starts_with('{') || value.starts_with('[')) && value.contains([':', '='])
}

/// The value that starts at `start`: a quoted string to its closing quote (backslash escapes
/// honoured), anything else to the end of the line. Returns the end offset and the quote used.
fn read_value(text: &str, start: usize) -> (usize, Option<char>) {
    let rest = &text[start..];
    let line_end = rest.find(['\n', '\r']).map_or(text.len(), |i| start + i);
    match rest.chars().next() {
        Some(q @ ('"' | '\'')) => {
            let mut escaped = false;
            for (i, c) in rest.char_indices().skip(1) {
                if start + i >= line_end {
                    break;
                }
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == q {
                    return (start + i + c.len_utf8(), Some(q));
                }
            }
            // No closing quote on the line: hide to the end of the line.
            (line_end, Some(q))
        }
        _ => (line_end, None),
    }
}

/// Hide the value of every `name=value` / `name: value` whose name is sensitive.
fn redact_pairs(text: &str) -> String {
    let p = patterns();
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    let mut search = 0;
    while search <= text.len() {
        let Some(m) = p.name_separator.captures_at(text, search) else {
            break;
        };
        let whole = m.get(0).expect("match");
        let name = &m[1];
        let value_start = whole.end();
        search = value_start.max(whole.start() + 1);
        let level = sensitivity(name);
        if level == Sensitivity::No || harmless_name(name) {
            continue;
        }
        let (end, quote) = read_value(text, value_start);
        let raw = &text[value_start..end];
        let inner = match quote {
            Some(_) => raw.trim_matches(|c| c == '"' || c == '\'').trim(),
            None => raw.trim(),
        };
        let numeric_or_flag = !inner.is_empty()
            && inner.len() <= 6
            && (inner.chars().all(|c| c.is_ascii_digit())
                || ["true", "false", "yes", "no", "on", "off", "none", "null"]
                    .contains(&inner.to_ascii_lowercase().as_str()));
        if inner.is_empty()
            || inner.starts_with("[redacted")
            || (quote.is_none() && is_structure(inner))
            || (level == Sensitivity::Weak && numeric_or_flag)
        {
            continue;
        }
        out.push_str(&text[copied..value_start]);
        match quote {
            Some(q) => {
                out.push(q);
                out.push_str("[redacted]");
                if raw.len() > 1 && raw.ends_with(q) {
                    out.push(q);
                }
            }
            None => out.push_str("[redacted]"),
        }
        copied = end;
        search = end;
    }
    out.push_str(&text[copied..]);
    out
}

impl Redactor {
    pub fn new(emails: bool) -> Self {
        Self {
            known: Vec::new(),
            emails,
        }
    }

    /// Add a secret value to hide wherever it appears (kept in memory, never written anywhere).
    pub fn add_secret(&mut self, name: &str, value: &str) {
        let value = value.trim();
        if value.len() >= 8 {
            self.known.push((name.to_string(), value.to_string()));
        }
    }

    pub fn known_count(&self) -> usize {
        self.known.len()
    }

    pub fn apply(&self, text: &str) -> String {
        let p = patterns();
        let mut out = text.to_string();
        // Longest values first, so a secret that contains another is replaced whole.
        let mut known: Vec<&(String, String)> = self.known.iter().collect();
        known.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
        for (name, value) in known {
            out = out.replace(value.as_str(), &format!("[redacted:{name}]"));
        }
        out = p
            .private_key
            .replace_all(&out, "[redacted-private-key]")
            .into_owned();
        out = p
            .url_userinfo
            .replace_all(&out, |c: &regex::Captures| {
                // `user:password@` keeps the user name; anything else (`token@`, `:pw@`) is hidden whole.
                match c[2].split_once(':') {
                    Some((user, _)) => format!("{}{user}:[redacted]@", &c[1]),
                    None => format!("{}[redacted]@", &c[1]),
                }
            })
            .into_owned();
        out = p
            .query_secret
            .replace_all(&out, "$1[redacted]")
            .into_owned();
        out = p.paseto.replace_all(&out, "[redacted-token]").into_owned();
        out = p.jwt.replace_all(&out, "[redacted-token]").into_owned();
        out = p.bearer.replace_all(&out, "Bearer [redacted]").into_owned();
        out = redact_pairs(&out);
        out = p
            .name_then_blob
            .replace_all(&out, "$1 [redacted]")
            .into_owned();
        let before = out.clone();
        out = p
            .hex_run
            .replace_all(&before, |c: &regex::Captures| {
                let whole = c.get(0).expect("match");
                let prefix = &before[..whole.start()];
                // Only an image reference (`name@sha256:<digest>`) is a digest to keep; any other
                // name ending in `sha256:` is not exempt.
                if prefix.ends_with("@sha256:") {
                    whole.as_str().to_string()
                } else {
                    "[redacted-hex]".to_string()
                }
            })
            .into_owned();
        if self.emails {
            out = p.email.replace_all(&out, "[email]").into_owned();
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEX64: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    fn redactor() -> Redactor {
        let mut r = Redactor::new(true);
        r.add_secret("session_secret", "S3ssion-Secret-Value-Seeded-0001");
        r.add_secret("postgres_password", "pgpass-seeded-0002-abcdef");
        r
    }

    #[test]
    fn known_secret_values_are_hidden_wherever_they_appear() {
        let text = "boot S3ssion-Secret-Value-Seeded-0001 and again S3ssion-Secret-Value-Seeded-0001, db pgpass-seeded-0002-abcdef";
        let out = redactor().apply(text);
        assert!(!out.contains("Seeded"), "{out}");
        assert!(out.contains("[redacted:session_secret]"));
        assert!(out.contains("[redacted:postgres_password]"));
    }

    #[test]
    fn short_values_are_not_used_as_scrub_strings() {
        let mut r = Redactor::new(false);
        r.add_secret("tiny", "abc");
        assert_eq!(r.known_count(), 0);
        assert_eq!(r.apply("abc"), "abc");
    }

    #[test]
    fn credentials_in_urls_and_headers_are_hidden() {
        let r = Redactor::new(false);
        assert_eq!(
            r.apply("postgres://dynodoc_runtime:s3cr3tpw@postgres:5432/dynodoc"),
            "postgres://dynodoc_runtime:[redacted]@postgres:5432/dynodoc"
        );
        assert!(!r
            .apply("Authorization: Bearer abcdef0123456789abcdef")
            .contains("abcdef0123"));
        assert!(!r
            .apply("token v4.local.AbCdEf_-0123456789xyz")
            .contains("AbCdEf"));
        assert!(!r
            .apply("jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.sig")
            .contains("eyJzdWIi"));
    }

    #[test]
    fn key_value_pairs_hide_the_value_but_keep_file_paths() {
        let r = Redactor::new(false);
        let out = r.apply("SMTP_PASSWORD=hunter22\nsession_secret: abc123def\nPASETO_LOCAL_KEY_FILE=/run/secrets/paseto_local_key\napi_key=\"k-1234\"\nSet-Cookie: dynodoc_session=zzz999");
        assert!(
            !out.contains("hunter22")
                && !out.contains("abc123def")
                && !out.contains("k-1234")
                && !out.contains("zzz999"),
            "{out}"
        );
        assert!(
            out.contains("PASETO_LOCAL_KEY_FILE=/run/secrets/paseto_local_key"),
            "{out}"
        );
    }

    #[test]
    fn compose_structure_survives_and_a_secret_on_the_same_line_does_not() {
        let r = Redactor::new(false);
        let yaml = "depends_on:\n  secrets-init:\n    condition: service_completed_successfully\n  postgres:\n    condition: service_healthy\nenvironment:\n  TOKEN_TTL_SECONDS: \"3600\"\n  SESSION_SECRET_FILE: /run/secrets/session_secret\n  SESSION_SECRET: abc12345def\n";
        let out = r.apply(yaml);
        assert!(
            out.contains("secrets-init:\n    condition: service_completed_successfully"),
            "{out}"
        );
        assert!(out.contains("TOKEN_TTL_SECONDS: \"3600\""), "{out}");
        assert!(
            out.contains("SESSION_SECRET_FILE: /run/secrets/session_secret"),
            "{out}"
        );
        assert!(
            !out.contains("abc12345def") && out.contains("SESSION_SECRET: [redacted]"),
            "{out}"
        );
        assert!(
            r.apply("secrets-init: {condition: service_completed_successfully}")
                .contains("{condition"),
            "a flow opening is structure"
        );
    }

    #[test]
    fn private_key_blocks_and_long_hex_are_hidden() {
        let r = Redactor::new(false);
        let pem = "before\n-----BEGIN PRIVATE KEY-----\nMIGHAgEAMBMG\nabcdef\n-----END PRIVATE KEY-----\nafter";
        let out = r.apply(pem);
        assert!(
            !out.contains("MIGHAgEA") && out.contains("before") && out.contains("after"),
            "{out}"
        );
        let truncated = r.apply("-----BEGIN EC PRIVATE KEY-----\nMHcCAQEE");
        assert!(!truncated.contains("MHcCAQEE"));
        // After a sensitive name or alone, a long hex value is hidden either way.
        assert_eq!(r.apply(&format!("key {HEX64}")), "key [redacted]");
        assert_eq!(r.apply(&format!("hash {HEX64}")), "hash [redacted-hex]");
    }

    #[test]
    fn image_digests_are_kept_but_a_name_that_merely_ends_in_sha256_is_not_exempt() {
        let r = Redactor::new(false);
        let line = format!("image postgres:16@sha256:{HEX64}");
        assert_eq!(r.apply(&line), line);
        // M5: any name ending in `sha256:` used to be exempt (`x_sha256:` included).
        for name in ["x_sha256:", "key_sha256:", "sha256:", "token-sha256:"] {
            let out = r.apply(&format!("{name}{HEX64}"));
            assert!(!out.contains(HEX64), "{name}: {out}");
        }
    }

    #[test]
    fn emails_are_hidden_only_when_asked() {
        assert_eq!(
            Redactor::new(true).apply("sign-in for a.b+c@example.org sent"),
            "sign-in for [email] sent"
        );
        assert_eq!(
            Redactor::new(false).apply("sign-in for a.b@example.org sent"),
            "sign-in for a.b@example.org sent"
        );
    }

    #[test]
    fn redaction_is_idempotent() {
        let r = redactor();
        let once = r.apply("postgres://u:pw1234567@h/db password=abc12345 S3ssion-Secret-Value-Seeded-0001 a@b.example");
        assert_eq!(r.apply(&once), once);
    }

    /// One fixture with every shape the review named (M5). Each secret is a distinct marker so
    /// the assertion says which shape leaked.
    const FIXTURE: &str = r#"
redis://:REDISPW@cache:6379/0
postgres://app:p@ss:w0rd-ATSIGN@db.internal:5432/app
https://ghp_TOKENONLYUSERINFO@github.com/org/repo.git
smtp.password="TWO WORDS PW"
password=two words unquoted UNQUOTEDREST
PWD=PWDMARKER
passwd: PASSWDMARKER
client_secret = 'SINGLE QUOTED CS'
{"password": "JSONPW", "token": "JSONTOKEN", "user": "keep-user"}
api.key=APIKEYMARKER
X-Api-Key: XAPIKEYMARKER
sessionToken: CAMELTOKEN
Cookie: sid=COOKIEA; theme=dark; csrf=COOKIEB
Set-Cookie: dynodoc_session=SETCOOKIE; Path=/; HttpOnly
Authorization: Bearer BEARERSHORT
Authorization: Basic QkFTSUNNQVJLRVI=
curl -H "Authorization: Bearer b" https://x
GET /callback?code=OAUTHCODE&state=OAUTHSTATE&keep=1 HTTP/1.1
GET /x?access_token=ACCESSTOK&other=2
private_key: PRIVMARKER
signature=SIGMARKER
Credential=CREDMARKER
the token ABCDEFGHIJKLMNOPQRSTUVWXYZ0123 was issued
secret 0123456789abcdef0123456789abcdef0123
key=0123456789abcdef0123456789abcdef
mail to alice@example.org and bob%40example.org and ｃａｒｏｌ@münchen.de and dave＠example.org
level=info msg="starting" token=INLINETOKEN rest=ok
"#;

    #[test]
    fn every_shape_the_review_named_is_hidden() {
        let out = Redactor::new(true).apply(FIXTURE);
        for marker in [
            "REDISPW",
            "ATSIGN",
            "w0rd",
            "TOKENONLYUSERINFO",
            "TWO WORDS",
            "UNQUOTEDREST",
            "PWDMARKER",
            "PASSWDMARKER",
            "SINGLE QUOTED",
            "JSONPW",
            "JSONTOKEN",
            "APIKEYMARKER",
            "XAPIKEYMARKER",
            "CAMELTOKEN",
            "COOKIEA",
            "COOKIEB",
            "SETCOOKIE",
            "BEARERSHORT",
            "QkFTSUNN",
            "OAUTHCODE",
            "OAUTHSTATE",
            "ACCESSTOK",
            "PRIVMARKER",
            "SIGMARKER",
            "CREDMARKER",
            "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123",
            "0123456789abcdef0123456789abcdef",
            "alice@example.org",
            "bob%40example.org",
            "münchen",
            "dave＠example.org",
            "INLINETOKEN",
        ] {
            assert!(!out.contains(marker), "`{marker}` leaked:\n{out}");
        }
        // Structure and harmless text survive.
        assert!(out.contains("redis://:[redacted]@cache:6379/0"), "{out}");
        assert!(
            out.contains("postgres://app:[redacted]@db.internal:5432/app"),
            "{out}"
        );
        assert!(
            out.contains("https://[redacted]@github.com/org/repo.git"),
            "{out}"
        );
        assert!(
            out.contains("\"user\": \"keep-user\"") || out.contains("keep-user"),
            "{out}"
        );
        assert!(out.contains("keep=1") && out.contains("other=2"), "{out}");
        assert!(out.contains("level=info msg=\"starting\""), "{out}");
        // A bearer token of one character is hidden too, wherever it is written.
        assert_eq!(
            Redactor::new(false).apply("sent bearer b to x and BEARER 0 here"),
            "sent Bearer [redacted] to x and Bearer [redacted] here"
        );
    }

    #[test]
    fn a_json_document_stays_valid_json_after_redaction() {
        let out = Redactor::new(false)
            .apply(r#"{"Env":{"PASSWORD":"pw one","Mode":"x"},"Cookie":"a=1; b=2"}"#);
        let parsed: serde_json::Value =
            serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out}"));
        assert_eq!(parsed["Env"]["PASSWORD"], "[redacted]");
        assert_eq!(parsed["Cookie"], "[redacted]");
    }

    #[test]
    fn harmless_numbers_and_flags_after_weak_names_survive_but_not_after_strong_ones() {
        let r = Redactor::new(false);
        let out = r.apply("max_sessions=5\nstatus_code=200\nkey=true\npassword=12345\ntoken=1\n");
        assert!(
            out.contains("max_sessions=5")
                && out.contains("status_code=200")
                && out.contains("key=true"),
            "{out}"
        );
        assert!(
            !out.contains("12345") && out.contains("token=[redacted]"),
            "{out}"
        );
    }

    #[test]
    fn names_are_split_into_parts() {
        assert_eq!(name_parts("smtp_password"), vec!["smtp", "password"]);
        assert_eq!(name_parts("sessionToken"), vec!["session", "token"]);
        assert_eq!(name_parts("Set-Cookie"), vec!["set", "cookie"]);
        assert_eq!(name_parts("X-Api-Key"), vec!["x", "api", "key"]);
        assert!(sensitivity("secrets-init") == Sensitivity::Strong);
        assert!(
            sensitivity("monkey") == Sensitivity::No,
            "`monkey` has no part `key`"
        );
        assert!(sensitivity("hostname") == Sensitivity::No);
    }
}
