//! Redaction for support bundles. Two layers, both applied to every text that leaves the
//! machine: (1) the literal values of this installation's own secret files, held in memory only
//! and replaced wherever they appear; (2) patterns for credentials of any origin (URL passwords,
//! bearer and PASETO tokens, `password=` style pairs, private-key blocks, long hex keys, and
//! optionally email addresses). Redaction can only hide too much, never too little by design;
//! the bundle also never reads a file from `secrets/` (see `support.rs`).

use regex::{Captures, Regex};
use std::sync::OnceLock;

#[derive(Clone, Debug, Default)]
pub struct Redactor {
    /// (file name, value) of the installation's secrets; values shorter than 8 are not used.
    known: Vec<(String, String)>,
    pub emails: bool,
}

struct Patterns {
    url_password: Regex,
    paseto: Regex,
    jwt: Regex,
    bearer: Regex,
    key_value: Regex,
    private_key: Regex,
    hex_run: Regex,
    email: Regex,
}

fn patterns() -> &'static Patterns {
    static P: OnceLock<Patterns> = OnceLock::new();
    P.get_or_init(|| Patterns {
        url_password: Regex::new(r"([A-Za-z][A-Za-z0-9+.\-]*://[^\s:/@]+):([^\s@/]+)@").expect("regex"),
        paseto: Regex::new(r"\bv[1-4]\.(?:local|public)\.[A-Za-z0-9_\-=.]+").expect("regex"),
        jwt: Regex::new(r"\beyJ[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]{8,}\.[A-Za-z0-9_\-]*").expect("regex"),
        bearer: Regex::new(r"(?i)\b(bearer|basic)\s+[A-Za-z0-9._~+/=\-]{8,}").expect("regex"),
        key_value: Regex::new(
            r#"(?i)\b([a-z0-9_.\-]*(?:password|passwd|secret|token|api[_\-]?key|apikey|private[_\-]?key|passphrase|credential|authorization|cookie)[a-z0-9_.\-]*)(["']?\s*[:=]\s*["']?)([^\s"',;&]+)"#,
        )
        .expect("regex"),
        private_key: Regex::new(r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?(?:-----END [A-Z ]*PRIVATE KEY-----|$)").expect("regex"),
        hex_run: Regex::new(r"\b[0-9a-fA-F]{48,}\b").expect("regex"),
        email: Regex::new(r"[A-Za-z0-9._%+\-]+@[A-Za-z0-9\-]+(?:\.[A-Za-z0-9\-]+)*\.[A-Za-z]{2,}").expect("regex"),
    })
}

impl Redactor {
    pub fn new(emails: bool) -> Self {
        Self { known: Vec::new(), emails }
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
        out = p.private_key.replace_all(&out, "[redacted-private-key]").into_owned();
        out = p.url_password.replace_all(&out, "$1:[redacted]@").into_owned();
        out = p.paseto.replace_all(&out, "[redacted-token]").into_owned();
        out = p.jwt.replace_all(&out, "[redacted-token]").into_owned();
        out = p.bearer.replace_all(&out, "$1 [redacted]").into_owned();
        out = p
            .key_value
            .replace_all(&out, |c: &Captures| {
                let key = &c[1];
                if key.to_ascii_lowercase().ends_with("_file") || &c[3] == "[redacted]" || c[3].starts_with("[redacted") {
                    c[0].to_string()
                } else {
                    format!("{key}{}[redacted]", &c[2])
                }
            })
            .into_owned();
        let before = out.clone();
        out = p
            .hex_run
            .replace_all(&before, |c: &Captures| {
                let whole = c.get(0).expect("match");
                let prefix = &before[..whole.start()];
                if prefix.ends_with("sha256:") {
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
        assert_eq!(r.apply("postgres://dynodoc_runtime:s3cr3tpw@postgres:5432/dynodoc"), "postgres://dynodoc_runtime:[redacted]@postgres:5432/dynodoc");
        assert!(!r.apply("Authorization: Bearer abcdef0123456789abcdef").contains("abcdef0123"));
        assert!(!r.apply("token v4.local.AbCdEf_-0123456789xyz").contains("AbCdEf"));
        assert!(!r.apply("jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.sig").contains("eyJzdWIi"));
    }

    #[test]
    fn key_value_pairs_hide_the_value_but_keep_file_paths() {
        let r = Redactor::new(false);
        let out = r.apply("SMTP_PASSWORD=hunter22 session_secret: abc123def PASETO_LOCAL_KEY_FILE=/run/secrets/paseto_local_key api_key=\"k-1234\" Set-Cookie: dynodoc_session=zzz999");
        assert!(!out.contains("hunter22") && !out.contains("abc123def") && !out.contains("k-1234") && !out.contains("zzz999"), "{out}");
        assert!(out.contains("PASETO_LOCAL_KEY_FILE=/run/secrets/paseto_local_key"), "{out}");
    }

    #[test]
    fn private_key_blocks_and_long_hex_are_hidden() {
        let r = Redactor::new(false);
        let pem = "before\n-----BEGIN PRIVATE KEY-----\nMIGHAgEAMBMG\nabcdef\n-----END PRIVATE KEY-----\nafter";
        let out = r.apply(pem);
        assert!(!out.contains("MIGHAgEA") && out.contains("before") && out.contains("after"), "{out}");
        let truncated = r.apply("-----BEGIN EC PRIVATE KEY-----\nMHcCAQEE");
        assert!(!truncated.contains("MHcCAQEE"));
        assert_eq!(r.apply(&format!("key {HEX64}")), "key [redacted-hex]");
    }

    #[test]
    fn image_digests_are_kept() {
        let r = Redactor::new(false);
        let line = format!("image postgres:16@sha256:{HEX64}");
        assert_eq!(r.apply(&line), line);
    }

    #[test]
    fn emails_are_hidden_only_when_asked() {
        assert_eq!(Redactor::new(true).apply("sign-in for a.b+c@example.org sent"), "sign-in for [email] sent");
        assert_eq!(Redactor::new(false).apply("sign-in for a.b@example.org sent"), "sign-in for a.b@example.org sent");
    }

    #[test]
    fn redaction_is_idempotent() {
        let r = redactor();
        let once = r.apply("postgres://u:pw1234567@h/db password=abc12345 S3ssion-Secret-Value-Seeded-0001 a@b.example");
        assert_eq!(r.apply(&once), once);
    }
}
