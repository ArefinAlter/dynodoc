//! `ORIGIN` is exactly what people type: `https://host[:port]`, no path.

use crate::error::{CliError, Result};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    /// The port was written in the origin (so it must also be the published port).
    pub explicit_port: bool,
}

impl Origin {
    pub fn parse(text: &str) -> Result<Origin> {
        let bad = |why: &str| {
            CliError::usage(
                format!("`{text}` is not a usable origin: {why}"),
                "write exactly what people will type in the browser, for example https://docs.example.org or https://docs.example.org:8443 (no path, no trailing slash)",
            )
        };
        let (scheme, rest) = text.split_once("://").ok_or_else(|| bad("it has no https://"))?;
        if scheme != "https" {
            return Err(bad("the scheme must be https"));
        }
        if rest.is_empty() || rest.contains('/') || rest.contains('?') || rest.contains('#') || rest.contains('@') || rest.contains(' ') {
            return Err(bad("it must be a host name with an optional port and nothing else"));
        }
        let (host, port_text) = if let Some(inner) = rest.strip_prefix('[') {
            let (h, after) = inner.split_once(']').ok_or_else(|| bad("unclosed ["))?;
            (h.to_string(), after.strip_prefix(':').map(str::to_string))
        } else {
            match rest.rsplit_once(':') {
                Some((h, p)) => (h.to_string(), Some(p.to_string())),
                None => (rest.to_string(), None),
            }
        };
        if host.is_empty() {
            return Err(bad("the host name is empty"));
        }
        let (port, explicit) = match port_text {
            Some(p) => (p.parse::<u16>().ok().filter(|p| *p != 0).ok_or_else(|| bad("the port is not a number from 1 to 65535"))?, true),
            None => (443, false),
        };
        Ok(Origin { scheme: scheme.to_string(), host: host.to_ascii_lowercase(), port, explicit_port: explicit })
    }

    /// The `Host` header and server name as a browser would send them.
    pub fn host_header(&self) -> String {
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        if self.port == 443 {
            host
        } else {
            format!("{host}:{}", self.port)
        }
    }

    pub fn is_loopback_name(&self) -> bool {
        self.host == "localhost" || self.host == "127.0.0.1" || self.host == "::1" || self.host.ends_with(".localhost")
    }

    pub fn url(&self) -> String {
        format!("https://{}", self.host_header())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_host_and_optional_port() {
        let a = Origin::parse("https://Docs.Example.org").unwrap();
        assert_eq!((a.host.as_str(), a.port, a.explicit_port), ("docs.example.org", 443, false));
        let b = Origin::parse("https://localhost:8443").unwrap();
        assert_eq!((b.port, b.explicit_port), (8443, true));
        assert_eq!(b.host_header(), "localhost:8443");
        assert_eq!(a.host_header(), "docs.example.org");
        let c = Origin::parse("https://[::1]:9443").unwrap();
        assert_eq!((c.host.as_str(), c.port), ("::1", 9443));
        assert_eq!(c.host_header(), "[::1]:9443");
    }

    #[test]
    fn refuses_everything_else() {
        for bad in ["http://docs.example.org", "docs.example.org", "https://", "https://x/path", "https://x/", "https://x:0", "https://x:99999", "https://x:abc", "https://u@x", "https://x y", "https://x?q=1", "https://[::1"] {
            let e = Origin::parse(bad).unwrap_err();
            assert_eq!(e.code(), crate::error::exit::USAGE, "{bad}");
            assert!(!e.todo.is_empty());
        }
    }

    #[test]
    fn loopback_names_are_recognised() {
        assert!(Origin::parse("https://localhost:8443").unwrap().is_loopback_name());
        assert!(Origin::parse("https://127.0.0.1").unwrap().is_loopback_name());
        assert!(!Origin::parse("https://docs.example.org").unwrap().is_loopback_name());
    }
}
