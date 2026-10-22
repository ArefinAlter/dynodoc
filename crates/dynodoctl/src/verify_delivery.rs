//! `dynodoctl verify-delivery`: look at a running installation from the operator's side of the
//! network, the way a browser would, and say whether it is delivered as intended: HTTPS with a
//! valid chain (or the evaluation warning), the redirect, the headers the bundle promises, the
//! sign-in page, `/healthz`, no marketing pages, no references to outside hosts in what is served,
//! the certificate horizon and, when SMTP is configured, the mail check. Add-in manifests are P0-7
//! and are reported as "not built".

use crate::configure;
use crate::ctx::Ctx;
use crate::error::{exit, CliError, Result};
use crate::net::{Net, Target};
use crate::origin::Origin;
use crate::preflight::{Check, Level};
use regex::Regex;
use serde::Serialize;

#[derive(Clone, Debug, Default)]
pub struct Options {
    /// The address people use. Taken from `.env` of the install directory when omitted.
    pub origin: Option<String>,
    /// Connect here (host or host:port) instead of resolving the origin's name; the certificate
    /// name, SNI and Host header stay those of the origin.
    pub connect: Option<String>,
    pub http_port: Option<u16>,
    pub tls_mode: Option<String>,
    pub mail_to: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeliveryReport {
    pub schema_version: u32,
    pub origin: String,
    pub tls_mode: String,
    pub checks: Vec<Check>,
    /// `passed`, `warnings` or `failed`.
    pub overall: String,
}

impl DeliveryReport {
    pub fn exit_code(&self) -> i32 {
        match self.overall.as_str() {
            "failed" => exit::FAILED,
            "warnings" => exit::WARNINGS,
            _ => exit::OK,
        }
    }
    pub fn render(&self) -> String {
        let mut o = format!(
            "Delivery check of {} (certificate mode: {})\n\n",
            self.origin,
            if self.tls_mode.is_empty() {
                "unknown"
            } else {
                &self.tls_mode
            }
        );
        o.push_str(&crate::preflight::render(&self.checks));
        o.push_str(&format!("\nResult: {}\n", self.overall.to_uppercase()));
        o
    }
}

/// Origins that may appear in what is served without being a third party the browser talks to:
/// identifiers that are never fetched. Anything else outside the installation's own origin is
/// reported (docs/SELF-HOSTED-OPERATIONS.md, "verify-delivery"). Deliberately short.
const INERT: [&str; 1] = ["www.w3.org"];

/// Known, documented, optional: reported as a warning, not a failure. The host and, when given,
/// the first path segment after it.
const KNOWN: [(&str, &str, &str); 4] = [
    (
        "appsforoffice.microsoft.com",
        "",
        "the Office.js CDN (docs/SELF-HOSTED-EDITION.md 7.3)",
    ),
    ("accounts.google.com", "", "optional Google sign-in"),
    ("resend.com", "", "the optional Resend mail transport"),
    (
        "github.com",
        "ArefinAlter",
        "a link to the public engine repository",
    ),
];

/// An absolute `http(s)` URL, or a protocol-relative `//host/...` one (taken as `https`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UrlParts {
    pub scheme: String,
    pub host: String,
    pub port: u16,
    /// Path without query and fragment, at least `/`.
    pub path: String,
    /// `user@` was present in the authority.
    pub userinfo: bool,
}

impl UrlParts {
    /// `scheme://host[:port]` with the default port left out.
    pub fn origin(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        let default = if self.scheme == "https" { 443 } else { 80 };
        if self.port == default {
            format!("{}://{host}", self.scheme)
        } else {
            format!("{}://{host}:{}", self.scheme, self.port)
        }
    }

    /// Exactly the installation's own origin: https, the same host, the same port, no user name.
    pub fn is_own(&self, origin: &Origin) -> bool {
        self.scheme == "https"
            && self.host == origin.host
            && self.port == origin.port
            && !self.userinfo
    }
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    text.get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &text[prefix.len()..])
}

/// Split an absolute or protocol-relative URL the way a browser does for the parts that decide
/// WHERE it goes: the authority ends at the first `/`, backslash, `?` or `#`; a user name before
/// `@` is not the host; a bracketed IPv6 literal keeps its colons. `None` for other schemes and
/// for text that is not a URL.
pub fn split_url(reference: &str) -> Option<UrlParts> {
    let reference = reference.trim();
    let (scheme, rest) = if let Some(r) = strip_prefix_ci(reference, "https://") {
        ("https", r)
    } else if let Some(r) = strip_prefix_ci(reference, "http://") {
        ("http", r)
    } else if let Some(r) = reference.strip_prefix("//") {
        ("https", r)
    } else {
        return None;
    };
    let end = rest.find(['/', '\\', '?', '#']).unwrap_or(rest.len());
    let (authority, tail) = rest.split_at(end);
    if authority.chars().any(|c| c.is_control() || c == ' ') {
        return None;
    }
    let (userinfo, hostport) = match authority.rsplit_once('@') {
        Some((_, h)) => (true, h),
        None => (false, authority),
    };
    let (host, port_text) = if let Some(inner) = hostport.strip_prefix('[') {
        let (h, after) = inner.split_once(']')?;
        (h.to_string(), after.strip_prefix(':'))
    } else {
        match hostport.rsplit_once(':') {
            Some((h, p)) => (h.to_string(), Some(p)),
            None => (hostport.to_string(), None),
        }
    };
    if host.is_empty()
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':'))
    {
        return None;
    }
    let port = match port_text {
        Some(p) if !p.is_empty() => p.parse::<u16>().ok()?,
        _ if scheme == "https" => 443,
        _ => 80,
    };
    let path = tail.split(['?', '#']).next().unwrap_or("");
    Some(UrlParts {
        scheme: scheme.to_string(),
        host: host.to_ascii_lowercase(),
        port,
        path: if path.is_empty() {
            "/".to_string()
        } else {
            path.to_string()
        },
        userinfo,
    })
}

/// Every absolute `http(s)` URL written in a text, as `(origin, first path segment)`.
pub fn urls_in(text: &str) -> Vec<(String, String)> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(
            r#"(?i)\bhttps?://(?:\[[0-9a-f:.]+\][^\s"'`<>\\)\]}{,;|^*]*|[^\s"'`<>\\)\]}{,;|^*]+)"#,
        )
        .expect("regex")
    });
    re.find_iter(text)
        .filter_map(|m| split_url(m.as_str()))
        .map(|u| {
            let segment = u
                .path
                .trim_start_matches('/')
                .split('/')
                .next()
                .unwrap_or("")
                .to_string();
            (u.origin(), segment)
        })
        .collect()
}

/// `--connect`: `host`, `host:port`, `[v6]`, `[v6]:port` or a bare IPv6 literal (`::1`).
pub fn parse_connect(text: &str, default_port: u16) -> (String, u16) {
    let text = text.trim();
    if let Some(inner) = text.strip_prefix('[') {
        if let Some((host, after)) = inner.split_once(']') {
            let port = after
                .strip_prefix(':')
                .and_then(|p| p.parse().ok())
                .unwrap_or(default_port);
            return (host.to_string(), port);
        }
    }
    // Two or more colons and no brackets: an IPv6 address, not host:port.
    if text.matches(':').count() >= 2 {
        return (text.to_string(), default_port);
    }
    match text.rsplit_once(':') {
        Some((h, p)) if p.parse::<u16>().is_ok() => {
            (h.to_string(), p.parse().unwrap_or(default_port))
        }
        _ => (text.to_string(), default_port),
    }
}

/// `src`/`href`/`action` values of the tags a browser loads by itself.
pub fn loaded_references(html: &str) -> Vec<(String, String)> {
    static TAG: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static ATTR: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let tag = TAG.get_or_init(|| {
        Regex::new(r"(?is)<(script|link|img|iframe|form|source|video|audio|embed|object)\b([^>]*)>")
            .expect("regex")
    });
    let attr = ATTR.get_or_init(|| {
        Regex::new(r#"(?is)\b(src|href|action|data)\s*=\s*["']([^"']+)["']"#).expect("regex")
    });
    let mut out = Vec::new();
    for t in tag.captures_iter(html) {
        let name = t[1].to_ascii_lowercase();
        for a in attr.captures_iter(&t[2]) {
            let key = a[1].to_ascii_lowercase();
            let relevant = match name.as_str() {
                "form" => key == "action",
                "link" | "script" | "img" | "iframe" | "source" | "video" | "audio" | "embed" => {
                    key == "src" || key == "href"
                }
                _ => key == "data" || key == "src",
            };
            if relevant {
                out.push((name.clone(), a[2].to_string()));
            }
        }
    }
    out
}

/// Files read from the origin while looking for references to outside hosts. Every file costs a
/// TLS handshake, so the number is bounded; files the sign-in page loads eagerly come first.
const MAX_CRAWL: usize = 60;

/// The files still to read, the ones already queued, and references to other hosts found so far.
/// "Lazy" files are reached only through `import(...)` of another script: route modules the
/// browser loads when it opens that route. "Eager" files are loaded by the sign-in page itself.
struct Crawl {
    origin: Origin,
    eager: std::collections::VecDeque<String>,
    lazy: std::collections::VecDeque<String>,
    seen: std::collections::HashSet<String>,
    lazy_files: std::collections::HashSet<String>,
    /// `(origin, what loads it)` for references to anything but the installation's own origin.
    external: Vec<(String, String)>,
}

impl Crawl {
    fn new(origin: &Origin) -> Self {
        Self {
            origin: origin.clone(),
            eager: Default::default(),
            lazy: Default::default(),
            seen: Default::default(),
            lazy_files: Default::default(),
            external: Vec::new(),
        }
    }

    fn enqueue(&mut self, from: &str, reference: &str, tag: &str, lazy: bool) {
        let looks_absolute = {
            let r = reference.trim();
            r.starts_with("//")
                || strip_prefix_ci(r, "https://").is_some()
                || strip_prefix_ci(r, "http://").is_some()
        };
        let own_path;
        let path = if looks_absolute {
            match split_url(reference) {
                // The installation's own origin written out in full: crawl it like any other
                // reference (it used to be skipped, so a file named that way was never read).
                Some(u) if u.is_own(&self.origin) => {
                    own_path = resolve_path("/", &u.path);
                    match own_path {
                        Some(p) => p,
                        None => return,
                    }
                }
                Some(u) => {
                    self.external
                        .push((u.origin(), format!("<{tag}> loads {reference}")));
                    return;
                }
                None => {
                    self.external
                        .push((reference.to_string(), format!("<{tag}> loads {reference}")));
                    return;
                }
            }
        } else {
            let Some(path) = resolve_path(from, reference) else {
                return;
            };
            path
        };
        if !is_static_asset(&path) {
            return;
        }
        if self.seen.insert(path.clone()) {
            if lazy {
                self.lazy_files.insert(path.clone());
                self.lazy.push_back(path);
            } else {
                self.eager.push_back(path);
            }
        } else if !lazy {
            // Found eagerly after it was queued as lazy: it is eager.
            self.lazy_files.remove(&path);
        }
    }

    fn next(&mut self) -> Option<String> {
        self.eager.pop_front().or_else(|| self.lazy.pop_front())
    }

    fn is_lazy(&self, path: &str) -> bool {
        self.lazy_files.contains(path)
    }
}

fn is_static_asset(path: &str) -> bool {
    [".js", ".mjs", ".css"].iter().any(|e| path.ends_with(e))
}

/// Resolve a reference found in the file at `from` to an absolute path on this origin
/// (`./x`, `../x`, `x`, `/x`); None for another scheme, a data URL or a fragment.
pub fn resolve_path(from: &str, reference: &str) -> Option<String> {
    let reference = reference.split(['?', '#']).next().unwrap_or("");
    if reference.is_empty()
        || reference.contains("://")
        || reference.starts_with("//")
        || reference.starts_with("data:")
        || reference.starts_with("mailto:")
    {
        return None;
    }
    let mut parts: Vec<&str> = if reference.starts_with('/') {
        Vec::new()
    } else {
        let dir = from
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .rsplit_once('/')
            .map(|(d, _)| d)
            .unwrap_or("");
        dir.split('/').filter(|p| !p.is_empty()).collect()
    };
    for piece in reference.split('/') {
        match piece {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            other => parts.push(other),
        }
    }
    Some(format!("/{}", parts.join("/")))
}

/// Module specifiers a script or page names, each with whether it is a dynamic `import("...")`:
/// `import("./x.js")` (lazy), `from "./x.js"` and `import "./x.js"` (static).
pub fn script_imports(text: &str) -> Vec<(String, bool)> {
    static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r#"(?:(import\s*\(\s*)|from\s*|import\s*)["']([^"']+\.m?js)["']"#)
            .expect("regex")
    });
    re.captures_iter(text)
        .map(|c| (c[2].to_string(), c.get(1).is_some()))
        .collect()
}

/// `max-age` of a Strict-Transport-Security value. The directive name is case-insensitive
/// (RFC 6797), and the value may be quoted.
fn max_age(hsts: &str) -> Option<u64> {
    hsts.split(';').find_map(|p| {
        let p = p.trim();
        let (name, value) = p.split_once('=')?;
        name.trim()
            .eq_ignore_ascii_case("max-age")
            .then(|| value.trim().trim_matches('"').parse().ok())
            .flatten()
    })
}

pub fn run(ctx: &Ctx<'_>, net: &dyn Net, opts: &Options) -> Result<DeliveryReport> {
    let installed = ctx.layout.is_installed();
    let env = if installed { ctx.env().ok() } else { None };
    let origin_text = opts
        .origin
        .clone()
        .or_else(|| {
            env.as_ref()
                .and_then(|e| e.get("ORIGIN").map(str::to_string))
        })
        .ok_or_else(|| {
            CliError::usage(
                "which installation? the origin is not known",
                "pass --origin https://docs.example.org, or run from the install directory",
            )
        })?;
    let origin = Origin::parse(&origin_text)?;
    let mode = opts
        .tls_mode
        .clone()
        .or_else(|| {
            env.as_ref()
                .and_then(|e| e.get("DYNODOC_TLS_MODE").map(str::to_string))
        })
        .unwrap_or_default();
    let http_port = opts
        .http_port
        .or_else(|| {
            env.as_ref()
                .and_then(|e| e.get("DYNODOC_HTTP_PORT")?.parse().ok())
        })
        .unwrap_or(80);
    let (connect_host, connect_port) = match &opts.connect {
        Some(c) => parse_connect(c, origin.port),
        None => (origin.host.clone(), origin.port),
    };
    let https = |host_header: String| Target {
        connect_host: connect_host.clone(),
        port: connect_port,
        server_name: origin.host.clone(),
        host_header,
    };
    let target = https(origin.host_header());
    let now = (ctx.now)();
    let mut checks: Vec<Check> = Vec::new();
    let mut add = |id: &str, level: Level, title: &str, detail: String| {
        checks.push(Check::new(id, level, title, detail))
    };

    // 1. TLS and the first page.
    let (login, facts) = match net.https_get(&target, "/login") {
        Ok(v) => v,
        Err(e) => {
            add("reachable", Level::Fail, "HTTPS is reachable at the origin", format!("{e} - check the name, the port, DYNODOC_BIND and any firewall; `dynodoctl status` shows the proxy"));
            return Ok(finish(&origin, &mode, checks));
        }
    };
    add(
        "reachable",
        Level::Pass,
        "HTTPS is reachable at the origin",
        format!("{} on {}:{}", facts.protocol, connect_host, connect_port),
    );
    let proto_ok = facts.protocol.contains("1_2")
        || facts.protocol.contains("1_3")
        || facts.protocol.contains("1.2")
        || facts.protocol.contains("1.3");
    // What this client negotiated. The TLS library offers 1.2 and 1.3 only, so whether the server
    // would ALSO accept 1.0 or 1.1 is not tested, and the check does not claim it.
    add(
        "tls-version",
        if proto_ok { Level::Pass } else { Level::Fail },
        "This client negotiated TLS 1.2 or newer",
        format!(
            "{} (whether the server also accepts TLS 1.0 or 1.1 is not tested)",
            facts.protocol
        ),
    );
    let leaf = facts.leaf();
    // M4: only the operator's own statement of the certificate mode relaxes a check. What the
    // server says about itself (its certificate's subject, a header) never does.
    let evaluation = mode == "evaluation";
    match &facts.trust {
        Ok(()) => add("tls-chain", Level::Pass, "The certificate chain is valid for this name", leaf.as_ref().map(|c| format!("issued by {}", c.issuer)).unwrap_or_default()),
        Err(why) if evaluation => add("tls-chain", Level::Warn, "Evaluation certificate", format!("{why}. Expected in evaluation mode; browsers and Office add-ins reject it. Use `provided` or `acme` for real use.")),
        Err(why) => add(
            "tls-chain",
            Level::Fail,
            "The certificate chain is valid for this name",
            if leaf.as_ref().map(|c| c.is_evaluation()).unwrap_or(false) {
                format!("{why} (it is the bundle's evaluation certificate, but the certificate mode is `{}`)", if mode.is_empty() { "not given" } else { &mode })
            } else {
                why.to_string()
            },
        ),
    }
    match &leaf {
        Some(c) => {
            let days = c.days_left(now);
            let (level, note) = if days < 0 {
                (
                    Level::Fail,
                    format!("expired {} days ago ({}): renew it", -days, c.not_after),
                )
            } else if days < 7 {
                (
                    Level::Fail,
                    format!("expires in {days} days ({})", c.not_after),
                )
            } else if days < 30 && !c.is_evaluation() {
                (
                    Level::Warn,
                    format!("expires in {days} days ({}): renew it soon", c.not_after),
                )
            } else {
                (Level::Pass, format!("{days} days left ({})", c.not_after))
            };
            add("tls-expiry", level, "Certificate expiry horizon", note);
        }
        None => add(
            "tls-expiry",
            Level::Warn,
            "Certificate expiry horizon",
            "the certificate could not be read".to_string(),
        ),
    }

    // 2. The redirect from plain HTTP.
    let http_host = if http_port == 80 {
        origin.host.clone()
    } else {
        format!("{}:{http_port}", origin.host)
    };
    let http_target = Target {
        connect_host: connect_host.clone(),
        port: http_port,
        server_name: origin.host.clone(),
        host_header: http_host,
    };
    match net.http_get(&http_target, "/") {
        // The Location is parsed, not matched as text: `https://docs.example.org.evil.example/`
        // and `https://docs.example.org@evil.example/` both start with the origin.
        Ok(r)
            if matches!(r.status, 301 | 302 | 307 | 308)
                && r.header("location")
                    // `//host/` from an http page stays on http: only an explicit https:// counts.
                    .filter(|l| strip_prefix_ci(l.trim(), "https://").is_some())
                    .and_then(split_url)
                    .map(|u| u.is_own(&origin))
                    .unwrap_or(false) =>
        {
            add(
                "redirect",
                Level::Pass,
                "Plain HTTP redirects to the origin",
                format!("{} -> {}", r.status, r.header("location").unwrap_or("")),
            )
        }
        Ok(r) => add(
            "redirect",
            Level::Fail,
            "Plain HTTP redirects to the origin",
            format!(
                "answered {} with location {:?}",
                r.status,
                r.header("location")
            ),
        ),
        Err(e) => add(
            "redirect",
            Level::Warn,
            "Plain HTTP redirects to the origin",
            format!(
                "port {http_port} did not answer ({e}); people who type http:// will get nothing"
            ),
        ),
    }

    // 3. Headers of the sign-in page.
    add(
        "signin",
        if login.status == 200 {
            Level::Pass
        } else {
            Level::Fail
        },
        "The sign-in page answers 200",
        format!("status {}", login.status),
    );
    let csp_count = login.header_count("content-security-policy");
    let csp = login.header("content-security-policy").unwrap_or("");
    add(
        "csp",
        if csp_count == 1
            && csp.contains("script-src")
            && csp.contains(&format!("connect-src 'self' {}", origin.url()))
        {
            Level::Pass
        } else {
            Level::Fail
        },
        "Exactly one Content-Security-Policy, naming this origin in connect-src",
        format!("{csp_count} header(s)"),
    );
    match login.header("strict-transport-security") {
        Some(h) => {
            let age = max_age(h).unwrap_or(0);
            let (level, detail) = if evaluation {
                (
                    Level::Pass,
                    format!("{h} (short on purpose in evaluation mode)"),
                )
            } else if age >= 15_552_000 {
                (Level::Pass, h.to_string())
            } else {
                (Level::Warn, format!("{h}: shorter than 180 days"))
            };
            add("hsts", level, "Strict-Transport-Security is sent", detail);
        }
        None => add(
            "hsts",
            Level::Fail,
            "Strict-Transport-Security is sent",
            "the header is missing".to_string(),
        ),
    }
    for (id, name, want) in [
        ("nosniff", "x-content-type-options", "nosniff"),
        ("referrer", "referrer-policy", "no-referrer"),
        ("frame", "x-frame-options", "sameorigin"),
    ] {
        let got = login.header(name).unwrap_or("");
        add(
            id,
            if got.eq_ignore_ascii_case(want) {
                Level::Pass
            } else {
                Level::Fail
            },
            &format!("{name}: {want}"),
            if got.is_empty() {
                "missing".into()
            } else {
                got.to_string()
            },
        );
    }
    let server = login.header("server").unwrap_or("");
    add(
        "banner",
        if !server.contains('/') && login.header("x-powered-by").is_none() {
            Level::Pass
        } else {
            Level::Warn
        },
        "No server version or framework banner",
        format!("Server: {server}"),
    );
    add(
        "robots",
        if login
            .header("x-robots-tag")
            .map(|v| v.contains("noindex"))
            .unwrap_or(false)
        {
            Level::Pass
        } else {
            Level::Warn
        },
        "Nothing is offered to search engines",
        login
            .header("x-robots-tag")
            .unwrap_or("missing")
            .to_string(),
    );

    // 4. Health, host check, API privacy.
    match net.https_get(&target, "/healthz") {
        Ok((r, _)) if r.status == 200 && r.text().contains("\"status\":\"ok\"") => add(
            "healthz",
            Level::Pass,
            "/healthz reports the web server alive",
            String::new(),
        ),
        Ok((r, _)) => add(
            "healthz",
            Level::Fail,
            "/healthz reports the web server alive",
            format!(
                "status {} {}",
                r.status,
                r.text().chars().take(80).collect::<String>()
            ),
        ),
        Err(e) => add(
            "healthz",
            Level::Fail,
            "/healthz reports the web server alive",
            e,
        ),
    }
    match net.https_get(&https("dynodoc-verify-delivery.invalid".into()), "/") {
        Ok((r, _)) if r.status == 421 => add(
            "host-check",
            Level::Pass,
            "Another host name gets 421",
            String::new(),
        ),
        Ok((r, _)) => add(
            "host-check",
            Level::Fail,
            "Another host name gets 421",
            format!(
                "status {}: the installation answers names it does not own",
                r.status
            ),
        ),
        Err(e) => add("host-check", Level::Warn, "Another host name gets 421", e),
    }
    match net.https_get(&target, "/api/engine/documents") {
        Ok((r, _)) if r.status == 401 => add(
            "api-private",
            Level::Pass,
            "The API is not open to anonymous requests",
            "401".to_string(),
        ),
        Ok((r, _)) => add(
            "api-private",
            Level::Fail,
            "The API is not open to anonymous requests",
            format!("status {}", r.status),
        ),
        Err(e) => add(
            "api-private",
            Level::Fail,
            "The API is not open to anonymous requests",
            e,
        ),
    }

    // 5. No marketing pages.
    let mut visible = Vec::new();
    for p in [
        "/features",
        "/privacy",
        "/terms",
        "/pricing",
        "/sitemap.xml",
        "/llms.txt",
    ] {
        match net.https_get(&target, p) {
            Ok((r, _)) if r.status == 404 => {}
            Ok((r, _)) => visible.push(format!("{p} ({})", r.status)),
            Err(e) => visible.push(format!("{p} ({e})")),
        }
    }
    add(
        "marketing",
        if visible.is_empty() {
            Level::Pass
        } else {
            Level::Fail
        },
        "Marketing, legal and sitemap pages answer 404",
        visible.join(", "),
    );
    match net.https_get(&target, "/") {
        Ok((r, _))
            if matches!(r.status, 302 | 303 | 307)
                && r.header("location")
                    .map(|l| l == "/login" || l == format!("{}/login", origin.url()))
                    .unwrap_or(false) =>
        {
            add(
                "front-page",
                Level::Pass,
                "The front page sends a visitor to sign-in",
                String::new(),
            )
        }
        Ok((r, _)) => add(
            "front-page",
            Level::Fail,
            "The front page sends a visitor to sign-in",
            format!("status {} location {:?}", r.status, r.header("location")),
        ),
        Err(e) => add(
            "front-page",
            Level::Fail,
            "The front page sends a visitor to sign-in",
            e,
        ),
    }

    // 6. Nothing served names an outside host.
    // Breadth first from the sign-in page: its tags, its inline `import("...")` calls, and the
    // static imports of every script found, up to MAX_CRAWL files of this origin.
    // (path, text, lazy)
    let mut texts: Vec<(String, String, bool)> = vec![("/login".to_string(), login.text(), false)];
    let mut crawl = Crawl::new(&origin);
    for (tag, reference) in loaded_references(&login.text()) {
        crawl.enqueue("/login", &reference, &tag, false);
    }
    for (reference, _) in script_imports(&login.text()) {
        // The page's own inline `import("...")` loads the application: eager.
        crawl.enqueue("/login", &reference, "script", false);
    }
    // Files that could not be read are not "clean": they are reported, and make the result a warning.
    let mut unreadable: Vec<String> = Vec::new();
    while texts.len() <= MAX_CRAWL {
        let Some(path) = crawl.next() else {
            break;
        };
        match net.https_get(&target, &path) {
            Ok((r, _)) if r.status == 200 => {
                let body = r.text();
                let lazy_parent = crawl.is_lazy(&path);
                if path.ends_with(".js") || path.ends_with(".mjs") {
                    for (reference, dynamic) in script_imports(&body) {
                        crawl.enqueue(&path, &reference, "script", dynamic || lazy_parent);
                    }
                }
                texts.push((path, body, lazy_parent));
            }
            Ok((r, _)) => unreadable.push(format!("{path} (status {})", r.status)),
            Err(e) => unreadable.push(format!("{path} ({e})")),
        }
    }
    let skipped = crawl.eager.len() + crawl.lazy.len();
    // A file found eagerly after it was first read as lazy is eager.
    for t in texts.iter_mut() {
        t.2 = t.2 && crawl.is_lazy(&t.0);
    }
    // M4: every absolute http(s) origin in what is served, whatever its name, except the
    // installation's own, the operator's own support link, and the short INERT list. Reported once
    // per distinct origin.
    let support_origin = env
        .as_ref()
        .and_then(|e| e.get("DYNODOC_SUPPORT_URL"))
        .and_then(split_url)
        .map(|u| u.origin());
    let mut outside: std::collections::BTreeMap<String, (String, bool)> = Default::default();
    let mut known: std::collections::BTreeMap<String, String> = Default::default();
    let mut note = |origin_text: String, where_: String, lazy: bool| {
        let entry = outside.entry(origin_text).or_insert((where_, lazy));
        // Found eagerly anywhere means eager.
        entry.1 = entry.1 && lazy;
    };
    for (origin_text, how) in &crawl.external {
        let parts = split_url(origin_text);
        if parts.as_ref().is_some_and(|u| {
            INERT.contains(&u.host.as_str())
                || support_origin.as_deref() == Some(u.origin().as_str())
        }) {
            continue;
        }
        note(origin_text.clone(), how.clone(), false);
    }
    for (path, text, lazy) in &texts {
        for (origin_text, segment) in urls_in(text) {
            let Some(parts) = split_url(&origin_text) else {
                continue;
            };
            if parts.is_own(&origin)
                || INERT.contains(&parts.host.as_str())
                || support_origin.as_deref() == Some(origin_text.as_str())
            {
                continue;
            }
            if let Some((_, _, why)) = KNOWN.iter().find(|(host, first, _)| {
                (parts.host == *host || parts.host.ends_with(&format!(".{host}")))
                    && (first.is_empty() || first.eq_ignore_ascii_case(&segment))
            }) {
                known
                    .entry(origin_text.clone())
                    .or_insert_with(|| format!("{origin_text} in {path} ({why})"));
                continue;
            }
            note(origin_text, format!("in {path}"), *lazy);
        }
    }
    let eager_outside: Vec<String> = outside
        .iter()
        .filter(|(_, (_, lazy))| !lazy)
        .map(|(o, (w, _))| format!("{o} ({w})"))
        .collect();
    let dormant: Vec<String> = outside
        .iter()
        .filter(|(_, (_, lazy))| *lazy)
        .map(|(o, (w, _))| format!("{o} ({w})"))
        .collect();
    let incomplete = skipped > 0 || !unreadable.is_empty();
    let summary = format!(
        "{} files read ({} bytes){}{}",
        texts.len(),
        texts.iter().map(|(_, t, _)| t.len()).sum::<usize>(),
        if skipped > 0 {
            format!(", {skipped} more NOT read (limit {MAX_CRAWL}): the check is incomplete")
        } else {
            String::new()
        },
        if unreadable.is_empty() {
            String::new()
        } else {
            format!(
                ", {} file(s) could not be read: {}",
                unreadable.len(),
                unreadable
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("; ")
            )
        }
    );
    add(
        "outbound-references",
        if !eager_outside.is_empty() {
            Level::Fail
        } else if incomplete {
            Level::Warn
        } else {
            Level::Pass
        },
        "What is served names no origin but its own",
        if eager_outside.is_empty() {
            summary
        } else {
            format!("{}; {summary}", eager_outside.join("; "))
        },
    );
    if !dormant.is_empty() {
        let shown: Vec<&str> = dormant.iter().take(5).map(String::as_str).collect();
        add(
            "dormant-references",
            Level::Warn,
            "Route modules that are not loaded at sign-in name another origin",
            format!(
                "{} origin(s): {}. They are loaded only when the browser opens that route; the marketing routes answer 404 here. Known open item: acceptance criterion 6 of Phase 0 (task P0-6, a build without the (site) routes)",
                dormant.len(),
                shown.join("; ")
            ),
        );
    }
    if !known.is_empty() {
        add(
            "known-references",
            Level::Warn,
            "Documented optional references are present",
            known.values().cloned().collect::<Vec<_>>().join("; "),
        );
    }

    // 7. Mail, from the server's side.
    let mail = env
        .as_ref()
        .map(|e| e.get("DYNODOC_MAIL_TRANSPORT") == Some("smtp"))
        .unwrap_or(false);
    if !installed {
        add("mail", Level::Skip, "Mail server check", "run this on the server, in the install directory, to check the mail server from the web server's network position".to_string());
    } else if !mail {
        add(
            "mail",
            Level::Skip,
            "Mail server check",
            "the mail transport is not SMTP".to_string(),
        );
    } else {
        let to = opts.mail_to.clone().or_else(|| {
            env.as_ref()
                .and_then(|e| e.get("ADMIN_EMAILS"))
                .and_then(|a| a.split(',').next())
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
        });
        match to {
            Some(addr) => match configure::check_mail(ctx, &addr, false) {
                Ok(m) if m.ok => add(
                    "mail",
                    Level::Pass,
                    "The mail server accepts the connection, login, sender and recipient",
                    "nothing was sent".to_string(),
                ),
                Ok(m) => add(
                    "mail",
                    Level::Fail,
                    "The mail server accepts the connection, login, sender and recipient",
                    m.message,
                ),
                Err(e) => add("mail", Level::Fail, "The mail server check ran", e.what),
            },
            None => add(
                "mail",
                Level::Skip,
                "Mail server check",
                "no recipient: pass --mail-to".to_string(),
            ),
        }
    }

    // 8. Add-in manifests (P0-7).
    add(
        "addin-manifests",
        Level::Skip,
        "Add-in manifest validation",
        "NOT BUILT: the manifest generator and validator are task P0-7".to_string(),
    );
    Ok(finish(&origin, &mode, checks))
}

fn finish(origin: &Origin, mode: &str, checks: Vec<Check>) -> DeliveryReport {
    let overall = if checks.iter().any(|c| c.level == Level::Fail) {
        "failed"
    } else if checks.iter().any(|c| c.level == Level::Warn) {
        "warnings"
    } else {
        "passed"
    };
    DeliveryReport {
        schema_version: 1,
        origin: origin.url(),
        tls_mode: mode.to_string(),
        checks,
        overall: overall.into(),
    }
}

#[cfg(test)]
#[path = "tests/verify_delivery.rs"]
mod tests;
