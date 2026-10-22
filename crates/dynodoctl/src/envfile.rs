//! The bundle's `.env`: read, change a setting in place, keep comments, never show a secret.

use crate::error::{CliError, Result};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Line {
    /// A comment, a blank line or anything that is not KEY=VALUE.
    Raw(String),
    Pair { key: String, value: String },
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvFile {
    lines: Vec<Line>,
}

fn valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl EnvFile {
    pub fn parse(text: &str) -> Self {
        let lines = text
            .lines()
            .map(|raw| {
                let trimmed = raw.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    return Line::Raw(raw.to_string());
                }
                match trimmed.split_once('=') {
                    Some((key, value)) if valid_key(key.trim()) => {
                        let mut value = value.trim().to_string();
                        if value.len() >= 2
                            && ((value.starts_with('"') && value.ends_with('"')) || (value.starts_with('\'') && value.ends_with('\'')))
                        {
                            value = value[1..value.len() - 1].to_string();
                        }
                        Line::Pair { key: key.trim().to_string(), value }
                    }
                    _ => Line::Raw(raw.to_string()),
                }
            })
            .collect();
        Self { lines }
    }

    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| {
            CliError::failed(
                format!("cannot read {}: {e}", path.display()),
                "run `dynodoctl install` first, or pass --dir with the bundle directory that holds .env",
            )
        })?;
        Ok(Self::parse(&text))
    }

    /// The value of the last uncommented `KEY=`.
    pub fn get(&self, key: &str) -> Option<&str> {
        self.lines
            .iter()
            .rev()
            .find_map(|l| match l {
                Line::Pair { key: k, value } if k == key => Some(value.as_str()),
                _ => None,
            })
    }

    pub fn has(&self, key: &str) -> bool {
        self.get(key).is_some()
    }

    /// Change `KEY` where it is set, or append it. Values may not contain a line break.
    pub fn set(&mut self, key: &str, value: &str) -> Result<()> {
        if !valid_key(key) {
            return Err(CliError::usage(format!("`{key}` is not a valid setting name"), "use letters, digits and underscores"));
        }
        if value.contains('\n') || value.contains('\r') {
            return Err(CliError::usage(format!("the value for {key} contains a line break"), "give the value on one line"));
        }
        let mut found = false;
        for line in &mut self.lines {
            if let Line::Pair { key: k, value: v } = line {
                if k == key {
                    *v = value.to_string();
                    found = true;
                }
            }
        }
        if !found {
            self.lines.push(Line::Pair { key: key.to_string(), value: value.to_string() });
        }
        Ok(())
    }

    /// Remove every uncommented `KEY=`; returns whether one existed.
    pub fn unset(&mut self, key: &str) -> bool {
        let before = self.lines.len();
        self.lines.retain(|l| !matches!(l, Line::Pair { key: k, .. } if k == key));
        self.lines.len() != before
    }

    /// Append a comment line (kept above later settings).
    pub fn comment(&mut self, text: &str) {
        self.lines.push(Line::Raw(format!("# {text}")));
    }

    pub fn blank(&mut self) {
        self.lines.push(Line::Raw(String::new()));
    }

    pub fn pairs(&self) -> Vec<(&str, &str)> {
        self.lines
            .iter()
            .filter_map(|l| match l {
                Line::Pair { key, value } => Some((key.as_str(), value.as_str())),
                _ => None,
            })
            .collect()
    }

    /// The file text. Line endings are LF.
    pub fn render(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                Line::Raw(text) => out.push_str(text),
                Line::Pair { key, value } => {
                    out.push_str(key);
                    out.push('=');
                    out.push_str(value);
                }
            }
            out.push('\n');
        }
        out
    }

    /// The file text with every secret-looking value replaced. For support bundles and `configure show`.
    pub fn render_redacted(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            match line {
                Line::Raw(text) => out.push_str(text),
                Line::Pair { key, value } => {
                    out.push_str(key);
                    out.push('=');
                    out.push_str(&display_value(key, value));
                }
            }
            out.push('\n');
        }
        out
    }
}

/// Is this setting's value a secret (a name that says so; `*_FILE` settings hold only a path)?
pub fn is_secret_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    if upper.ends_with("_FILE") || upper.ends_with("_FILE_PATH") {
        return false;
    }
    ["PASSWORD", "SECRET", "TOKEN", "API_KEY", "PRIVATE", "PASSPHRASE", "CREDENTIAL"].iter().any(|w| upper.contains(w))
        || upper.ends_with("_KEY")
        || upper == "DATABASE_URL"
        || upper == "MIGRATION_DATABASE_URL"
}

/// What may be shown for a setting.
pub fn display_value(key: &str, value: &str) -> String {
    if is_secret_key(key) && !value.is_empty() {
        "[hidden]".to_string()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# comment\nDYNODOC_MODE=selfhosted\nORIGIN=https://docs.example.org\n# ADMIN_EMAILS=\nSESSION_SECRET_FILE=/run/secrets/session_secret\n";

    #[test]
    fn parse_and_render_keep_comments_and_order() {
        let env = EnvFile::parse(SAMPLE);
        assert_eq!(env.render(), SAMPLE);
        assert_eq!(env.get("ORIGIN"), Some("https://docs.example.org"));
        assert!(!env.has("ADMIN_EMAILS"), "a commented setting is not set");
    }

    #[test]
    fn set_replaces_in_place_or_appends() {
        let mut env = EnvFile::parse(SAMPLE);
        env.set("ORIGIN", "https://new.example.org").unwrap();
        env.set("ADMIN_EMAILS", "a@example.org").unwrap();
        let text = env.render();
        assert!(text.contains("ORIGIN=https://new.example.org\n"));
        assert!(text.ends_with("ADMIN_EMAILS=a@example.org\n"));
        assert_eq!(text.matches("ORIGIN=").count(), 1);
    }

    #[test]
    fn set_refuses_line_breaks_and_bad_names() {
        let mut env = EnvFile::default();
        assert!(env.set("A", "x\nB=y").is_err());
        assert!(env.set("1A", "x").is_err());
        assert!(env.set("A B", "x").is_err());
    }

    #[test]
    fn unset_removes_pairs_only() {
        let mut env = EnvFile::parse(SAMPLE);
        assert!(env.unset("ORIGIN"));
        assert!(!env.unset("ORIGIN"));
        assert!(env.render().contains("# ADMIN_EMAILS="));
    }

    #[test]
    fn quotes_around_values_are_removed() {
        let env = EnvFile::parse("A=\"x y\"\nB='z'\n");
        assert_eq!(env.get("A"), Some("x y"));
        assert_eq!(env.get("B"), Some("z"));
    }

    #[test]
    fn secrets_are_recognised_by_name_and_never_displayed() {
        for key in ["SMTP_PASSWORD", "SESSION_SECRET", "PASETO_LOCAL_KEY", "RESEND_API_KEY", "DATABASE_URL", "WEB_SERVICE_KEY"] {
            assert!(is_secret_key(key), "{key}");
            assert_eq!(display_value(key, "hunter2"), "[hidden]");
        }
        for key in ["SMTP_PASSWORD_FILE", "SESSION_SECRET_FILE", "ORIGIN", "SMTP_HOST", "DYNODOC_TLS_MODE"] {
            assert!(!is_secret_key(key), "{key}");
        }
        assert_eq!(display_value("ORIGIN", "https://x"), "https://x");
    }

    #[test]
    fn redacted_render_hides_a_secret_that_was_pasted_into_env() {
        let env = EnvFile::parse("ORIGIN=https://x\nSMTP_PASSWORD=hunter2\nSESSION_SECRET_FILE=/run/secrets/s\n");
        let text = env.render_redacted();
        assert!(!text.contains("hunter2"));
        assert!(text.contains("SMTP_PASSWORD=[hidden]"));
        assert!(text.contains("SESSION_SECRET_FILE=/run/secrets/s"));
    }
}
