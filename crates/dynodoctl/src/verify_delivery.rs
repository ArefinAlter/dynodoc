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
        let mut o = format!("Delivery check of {} (certificate mode: {})\n\n", self.origin, if self.tls_mode.is_empty() { "unknown" } else { &self.tls_mode });
        o.push_str(&crate::preflight::render(&self.checks));
        o.push_str(&format!("\nResult: {}\n", self.overall.to_uppercase()));
        o
    }
}

/// Hosts that must never appear in what an installation serves. Same ideas as
/// `frontend/researcher/scripts/audit-hosted-strings.mjs`.
const FORBIDDEN: [(&str, &str); 4] = [
    ("dynodoc.online", "the hosted service's name"),
    ("google-analytics", "analytics"),
    ("googletagmanager", "analytics"),
    ("plausible.io", "analytics"),
];
const FORBIDDEN_MORE: [(&str, &str); 8] = [
    ("sentry.io", "analytics"),
    ("vercel-insights", "analytics"),
    ("cdn.jsdelivr", "a public CDN"),
    ("unpkg.com", "a public CDN"),
    ("cdnjs.", "a public CDN"),
    ("fonts.googleapis", "a font CDN"),
    ("posthog", "analytics"),
    ("hotjar", "analytics"),
];
/// Known, documented, optional: reported as a warning, not a failure.
const KNOWN: [(&str, &str); 4] = [
    ("appsforoffice.microsoft.com", "the Office.js CDN (docs/SELF-HOSTED-EDITION.md 7.3)"),
    ("accounts.google.com", "optional Google sign-in"),
    ("resend.com", "the optional Resend mail transport"),
    ("github.com/ArefinAlter", "a link to the public engine repository"),
];

fn find_hosts(text: &str, set: &[(&str, &str)]) -> Vec<String> {
    let lower = text.to_ascii_lowercase();
    set.iter().filter(|(needle, _)| lower.contains(&needle.to_ascii_lowercase())).map(|(n, why)| format!("{n} ({why})")).collect()
}

/// `src`/`href`/`action` values of the tags a browser loads by itself.
pub fn loaded_references(html: &str) -> Vec<(String, String)> {
    static TAG: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static ATTR: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let tag = TAG.get_or_init(|| Regex::new(r"(?is)<(script|link|img|iframe|form|source|video|audio|embed|object)\b([^>]*)>").expect("regex"));
    let attr = ATTR.get_or_init(|| Regex::new(r#"(?is)\b(src|href|action|data)\s*=\s*["']([^"']+)["']"#).expect("regex"));
    let mut out = Vec::new();
    for t in tag.captures_iter(html) {
        let name = t[1].to_ascii_lowercase();
        for a in attr.captures_iter(&t[2]) {
            let key = a[1].to_ascii_lowercase();
            let relevant = match name.as_str() {
                "form" => key == "action",
                "link" | "script" | "img" | "iframe" | "source" | "video" | "audio" | "embed" => key == "src" || key == "href",
                _ => key == "data" || key == "src",
            };
            if relevant {
                out.push((name.clone(), a[2].to_string()));
            }
        }
    }
    out
}

fn host_of(reference: &str) -> Option<String> {
    let rest = reference.strip_prefix("https://").or_else(|| reference.strip_prefix("http://")).or_else(|| reference.strip_prefix("//"))?;
    Some(rest.split(['/', '?', '#', ':']).next().unwrap_or("").to_ascii_lowercase())
}

fn max_age(hsts: &str) -> Option<u64> {
    hsts.split(';').find_map(|p| p.trim().strip_prefix("max-age=").and_then(|n| n.trim().parse().ok()))
}

pub fn run(ctx: &Ctx<'_>, net: &dyn Net, opts: &Options) -> Result<DeliveryReport> {
    let installed = ctx.layout.is_installed();
    let env = if installed { ctx.env().ok() } else { None };
    let origin_text = opts
        .origin
        .clone()
        .or_else(|| env.as_ref().and_then(|e| e.get("ORIGIN").map(str::to_string)))
        .ok_or_else(|| CliError::usage("which installation? the origin is not known", "pass --origin https://docs.example.org, or run from the install directory"))?;
    let origin = Origin::parse(&origin_text)?;
    let mode = opts.tls_mode.clone().or_else(|| env.as_ref().and_then(|e| e.get("DYNODOC_TLS_MODE").map(str::to_string))).unwrap_or_default();
    let http_port = opts.http_port.or_else(|| env.as_ref().and_then(|e| e.get("DYNODOC_HTTP_PORT")?.parse().ok())).unwrap_or(80);
    let (connect_host, connect_port) = match &opts.connect {
        Some(c) => match c.rsplit_once(':') {
            Some((h, p)) if p.parse::<u16>().is_ok() => (h.to_string(), p.parse().unwrap_or(origin.port)),
            _ => (c.clone(), origin.port),
        },
        None => (origin.host.clone(), origin.port),
    };
    let https = |host_header: String| Target { connect_host: connect_host.clone(), port: connect_port, server_name: origin.host.clone(), host_header };
    let target = https(origin.host_header());
    let now = (ctx.now)();
    let mut checks: Vec<Check> = Vec::new();
    let mut add = |id: &str, level: Level, title: &str, detail: String| checks.push(Check::new(id, level, title, detail));

    // 1. TLS and the first page.
    let (login, facts) = match net.https_get(&target, "/login") {
        Ok(v) => v,
        Err(e) => {
            add("reachable", Level::Fail, "HTTPS is reachable at the origin", format!("{e} - check the name, the port, DYNODOC_BIND and any firewall; `dynodoctl status` shows the proxy"));
            return Ok(finish(&origin, &mode, checks));
        }
    };
    add("reachable", Level::Pass, "HTTPS is reachable at the origin", format!("{} on {}:{}", facts.protocol, connect_host, connect_port));
    let proto_ok = facts.protocol.contains("1_2") || facts.protocol.contains("1_3") || facts.protocol.contains("1.2") || facts.protocol.contains("1.3");
    add("tls-version", if proto_ok { Level::Pass } else { Level::Fail }, "TLS 1.2 or newer", facts.protocol.clone());
    let leaf = facts.leaf();
    let evaluation = mode == "evaluation" || leaf.as_ref().map(|c| c.is_evaluation()).unwrap_or(false) || login.header("x-dynodoc-tls-mode").map(|v| v.starts_with("evaluation")).unwrap_or(false);
    match &facts.trust {
        Ok(()) => add("tls-chain", Level::Pass, "The certificate chain is valid for this name", leaf.as_ref().map(|c| format!("issued by {}", c.issuer)).unwrap_or_default()),
        Err(why) if evaluation => add("tls-chain", Level::Warn, "Evaluation certificate", format!("{why}. Expected in evaluation mode; browsers and Office add-ins reject it. Use `provided` or `acme` for real use.")),
        Err(why) => add("tls-chain", Level::Fail, "The certificate chain is valid for this name", format!("{why}")),
    }
    match &leaf {
        Some(c) => {
            let days = c.days_left(now);
            let (level, note) = if days < 0 {
                (Level::Fail, format!("expired {} days ago ({}): renew it", -days, c.not_after))
            } else if days < 7 {
                (Level::Fail, format!("expires in {days} days ({})", c.not_after))
            } else if days < 30 && !c.is_evaluation() {
                (Level::Warn, format!("expires in {days} days ({}): renew it soon", c.not_after))
            } else {
                (Level::Pass, format!("{days} days left ({})", c.not_after))
            };
            add("tls-expiry", level, "Certificate expiry horizon", note);
        }
        None => add("tls-expiry", Level::Warn, "Certificate expiry horizon", "the certificate could not be read".to_string()),
    }

    // 2. The redirect from plain HTTP.
    let http_host = if http_port == 80 { origin.host.clone() } else { format!("{}:{http_port}", origin.host) };
    let http_target = Target { connect_host: connect_host.clone(), port: http_port, server_name: origin.host.clone(), host_header: http_host };
    match net.http_get(&http_target, "/") {
        Ok(r) if matches!(r.status, 301 | 302 | 307 | 308) && r.header("location").map(|l| l.starts_with(&origin.url())).unwrap_or(false) => {
            add("redirect", Level::Pass, "Plain HTTP redirects to the origin", format!("{} -> {}", r.status, r.header("location").unwrap_or("")))
        }
        Ok(r) => add("redirect", Level::Fail, "Plain HTTP redirects to the origin", format!("answered {} with location {:?}", r.status, r.header("location"))),
        Err(e) => add("redirect", Level::Warn, "Plain HTTP redirects to the origin", format!("port {http_port} did not answer ({e}); people who type http:// will get nothing")),
    }

    // 3. Headers of the sign-in page.
    add("signin", if login.status == 200 { Level::Pass } else { Level::Fail }, "The sign-in page answers 200", format!("status {}", login.status));
    let csp_count = login.header_count("content-security-policy");
    let csp = login.header("content-security-policy").unwrap_or("");
    add(
        "csp",
        if csp_count == 1 && csp.contains("script-src") && csp.contains(&format!("connect-src 'self' {}", origin.url())) { Level::Pass } else { Level::Fail },
        "Exactly one Content-Security-Policy, naming this origin in connect-src",
        format!("{csp_count} header(s)"),
    );
    match login.header("strict-transport-security") {
        Some(h) => {
            let age = max_age(h).unwrap_or(0);
            let (level, detail) = if evaluation { (Level::Pass, format!("{h} (short on purpose in evaluation mode)")) } else if age >= 15_552_000 { (Level::Pass, h.to_string()) } else { (Level::Warn, format!("{h}: shorter than 180 days")) };
            add("hsts", level, "Strict-Transport-Security is sent", detail);
        }
        None => add("hsts", Level::Fail, "Strict-Transport-Security is sent", "the header is missing".to_string()),
    }
    for (id, name, want) in [("nosniff", "x-content-type-options", "nosniff"), ("referrer", "referrer-policy", "no-referrer"), ("frame", "x-frame-options", "sameorigin")] {
        let got = login.header(name).unwrap_or("");
        add(id, if got.eq_ignore_ascii_case(want) { Level::Pass } else { Level::Fail }, &format!("{name}: {want}"), if got.is_empty() { "missing".into() } else { got.to_string() });
    }
    let server = login.header("server").unwrap_or("");
    add("banner", if !server.contains('/') && login.header("x-powered-by").is_none() { Level::Pass } else { Level::Warn }, "No server version or framework banner", format!("Server: {server}"));
    add("robots", if login.header("x-robots-tag").map(|v| v.contains("noindex")).unwrap_or(false) { Level::Pass } else { Level::Warn }, "Nothing is offered to search engines", login.header("x-robots-tag").unwrap_or("missing").to_string());

    // 4. Health, host check, API privacy.
    match net.https_get(&target, "/healthz") {
        Ok((r, _)) if r.status == 200 && r.text().contains("\"status\":\"ok\"") => add("healthz", Level::Pass, "/healthz reports the web server alive", String::new()),
        Ok((r, _)) => add("healthz", Level::Fail, "/healthz reports the web server alive", format!("status {} {}", r.status, r.text().chars().take(80).collect::<String>())),
        Err(e) => add("healthz", Level::Fail, "/healthz reports the web server alive", e),
    }
    match net.https_get(&https("dynodoc-verify-delivery.invalid".into()), "/") {
        Ok((r, _)) if r.status == 421 => add("host-check", Level::Pass, "Another host name gets 421", String::new()),
        Ok((r, _)) => add("host-check", Level::Fail, "Another host name gets 421", format!("status {}: the installation answers names it does not own", r.status)),
        Err(e) => add("host-check", Level::Warn, "Another host name gets 421", e),
    }
    match net.https_get(&target, "/api/engine/documents") {
        Ok((r, _)) if r.status == 401 => add("api-private", Level::Pass, "The API is not open to anonymous requests", "401".to_string()),
        Ok((r, _)) => add("api-private", Level::Fail, "The API is not open to anonymous requests", format!("status {}", r.status)),
        Err(e) => add("api-private", Level::Fail, "The API is not open to anonymous requests", e),
    }

    // 5. No marketing pages.
    let mut visible = Vec::new();
    for p in ["/features", "/privacy", "/terms", "/pricing", "/sitemap.xml", "/llms.txt"] {
        match net.https_get(&target, p) {
            Ok((r, _)) if r.status == 404 => {}
            Ok((r, _)) => visible.push(format!("{p} ({})", r.status)),
            Err(e) => visible.push(format!("{p} ({e})")),
        }
    }
    add("marketing", if visible.is_empty() { Level::Pass } else { Level::Fail }, "Marketing, legal and sitemap pages answer 404", visible.join(", "));
    match net.https_get(&target, "/") {
        Ok((r, _)) if matches!(r.status, 302 | 303 | 307) && r.header("location").map(|l| l == "/login" || l == format!("{}/login", origin.url())).unwrap_or(false) => {
            add("front-page", Level::Pass, "The front page sends a visitor to sign-in", String::new())
        }
        Ok((r, _)) => add("front-page", Level::Fail, "The front page sends a visitor to sign-in", format!("status {} location {:?}", r.status, r.header("location"))),
        Err(e) => add("front-page", Level::Fail, "The front page sends a visitor to sign-in", e),
    }

    // 6. Nothing served names an outside host.
    let mut texts: Vec<(String, String)> = vec![("/login".to_string(), login.text())];
    let mut external_loads: Vec<String> = Vec::new();
    let mut fetched = 0;
    for (tag, reference) in loaded_references(&login.text()) {
        if let Some(host) = host_of(&reference) {
            if host != origin.host {
                external_loads.push(format!("<{tag}> loads {reference}"));
            }
            continue;
        }
        let path = if reference.starts_with('/') { reference.clone() } else { continue };
        let static_asset = path.split('?').next().map(|p| p.ends_with(".js") || p.ends_with(".mjs") || p.ends_with(".css")).unwrap_or(false);
        if static_asset && fetched < 40 {
            fetched += 1;
            if let Ok((r, _)) = net.https_get(&target, &path) {
                if r.status == 200 {
                    texts.push((path, r.text()));
                }
            }
        }
    }
    let mut forbidden: Vec<String> = Vec::new();
    let mut known: Vec<String> = Vec::new();
    for (path, text) in &texts {
        for hit in find_hosts(text, &FORBIDDEN).into_iter().chain(find_hosts(text, &FORBIDDEN_MORE)) {
            forbidden.push(format!("{path}: {hit}"));
        }
        for hit in find_hosts(text, &KNOWN) {
            known.push(format!("{path}: {hit}"));
        }
    }
    forbidden.extend(external_loads);
    add(
        "outbound-references",
        if forbidden.is_empty() { Level::Pass } else { Level::Fail },
        "What is served names no outside host",
        if forbidden.is_empty() { format!("{} files read ({} bytes)", texts.len(), texts.iter().map(|(_, t)| t.len()).sum::<usize>()) } else { forbidden.join("; ") },
    );
    if !known.is_empty() {
        add("known-references", Level::Warn, "Documented optional references are present", known.join("; "));
    }

    // 7. Mail, from the server's side.
    let mail = env.as_ref().map(|e| e.get("DYNODOC_MAIL_TRANSPORT") == Some("smtp")).unwrap_or(false);
    if !installed {
        add("mail", Level::Skip, "Mail server check", "run this on the server, in the install directory, to check the mail server from the web server's network position".to_string());
    } else if !mail {
        add("mail", Level::Skip, "Mail server check", "the mail transport is not SMTP".to_string());
    } else {
        let to = opts.mail_to.clone().or_else(|| env.as_ref().and_then(|e| e.get("ADMIN_EMAILS")).and_then(|a| a.split(',').next()).map(|a| a.trim().to_string()).filter(|a| !a.is_empty()));
        match to {
            Some(addr) => match configure::check_mail(ctx, &addr, false) {
                Ok(m) if m.ok => add("mail", Level::Pass, "The mail server accepts the connection, login, sender and recipient", "nothing was sent".to_string()),
                Ok(m) => add("mail", Level::Fail, "The mail server accepts the connection, login, sender and recipient", m.message),
                Err(e) => add("mail", Level::Fail, "The mail server check ran", e.what),
            },
            None => add("mail", Level::Skip, "Mail server check", "no recipient: pass --mail-to".to_string()),
        }
    }

    // 8. Add-in manifests (P0-7).
    add("addin-manifests", Level::Skip, "Add-in manifest validation", "NOT BUILT: the manifest generator and validator are task P0-7".to_string());
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
    DeliveryReport { schema_version: 1, origin: origin.url(), tls_mode: mode.to_string(), checks, overall: overall.into() }
}

#[cfg(test)]
#[path = "tests/verify_delivery.rs"]
mod tests;
