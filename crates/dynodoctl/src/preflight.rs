//! Checks before an installation or an upgrade: Docker, Compose, ports, disk, memory, rights,
//! clock, and the DNS name when certificates are automatic. Read-only.

use crate::compose::docker;
use crate::ctx::Ctx;
use crate::origin::Origin;
use crate::util::{free_bytes, human_bytes};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::net::{IpAddr, TcpListener, ToSocketAddrs};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Pass,
    Warn,
    Fail,
    Skip,
}

#[derive(Clone, Debug, Serialize)]
pub struct Check {
    pub id: String,
    pub level: Level,
    pub title: String,
    pub detail: String,
}

impl Check {
    pub fn new(id: &str, level: Level, title: &str, detail: impl Into<String>) -> Self {
        Self { id: id.into(), level, title: title.into(), detail: detail.into() }
    }
}

pub fn render(checks: &[Check]) -> String {
    let mut out = String::new();
    for c in checks {
        let mark = match c.level {
            Level::Pass => "pass",
            Level::Warn => "WARN",
            Level::Fail => "FAIL",
            Level::Skip => "skip",
        };
        out.push_str(&format!("  {mark}  {}", c.title));
        if !c.detail.is_empty() {
            out.push_str(&format!(" - {}", c.detail));
        }
        out.push('\n');
    }
    out
}

pub fn failed(checks: &[Check]) -> Vec<&Check> {
    checks.iter().filter(|c| c.level == Level::Fail).collect()
}

/// `v5.1.4`, `2.24.0`, `2.24.0-desktop.1` -> (5, 1, 4)
pub fn parse_version(text: &str) -> Option<(u32, u32, u32)> {
    let t = text.trim().trim_start_matches('v');
    let mut it = t.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty());
    Some((it.next()?.parse().ok()?, it.next().unwrap_or("0").parse().ok()?, it.next().unwrap_or("0").parse().ok()?))
}

/// Seconds the host clock is ahead of the Docker daemon's clock (negative: behind).
pub fn skew_seconds(daemon_time: &str, host_now: DateTime<Utc>) -> Option<i64> {
    let parsed = DateTime::parse_from_rfc3339(daemon_time.trim()).ok()?;
    Some((host_now - parsed.with_timezone(&Utc)).num_seconds())
}

pub struct Input<'a> {
    pub dir: &'a Path,
    pub bind: String,
    pub http_port: u16,
    pub https_port: u16,
    pub tls_mode: Option<String>,
    pub origin: Option<Origin>,
    /// The installation's own proxy already holds the ports.
    pub stack_running: bool,
    pub needs_images: bool,
}

pub fn run(ctx: &Ctx<'_>, input: &Input<'_>) -> Vec<Check> {
    let mut checks = Vec::new();

    // Docker daemon and Compose.
    let server = docker(ctx, &["version", "--format", "{{.Server.Version}}"]);
    match server {
        Ok(o) if o.ok() && !o.stdout_text().trim().is_empty() => checks.push(Check::new("docker", Level::Pass, "Docker daemon reachable", format!("version {}", o.stdout_text().trim()))),
        Ok(o) => checks.push(Check::new("docker", Level::Fail, "Docker daemon is not reachable", format!("{} - start Docker, and make sure your user may use it (docker group on Linux)", crate::compose::short(&o.text())))),
        Err(e) => checks.push(Check::new("docker", Level::Fail, "Docker is not installed", e.todo)),
    }
    let compose = docker(ctx, &["compose", "version", "--short"]);
    match compose {
        Ok(o) if o.ok() => match parse_version(&o.stdout_text()) {
            Some(v) if v >= (2, 24, 0) => checks.push(Check::new("compose", Level::Pass, "Docker Compose is new enough", format!("{} (needs 2.24 or newer)", o.stdout_text().trim()))),
            Some(_) => checks.push(Check::new("compose", Level::Fail, "Docker Compose is too old", format!("{} - the bundle needs v2.24 or newer: update Docker", o.stdout_text().trim()))),
            None => checks.push(Check::new("compose", Level::Warn, "Could not read the Compose version", o.stdout_text().trim().to_string())),
        },
        _ => checks.push(Check::new("compose", Level::Fail, "Docker Compose v2 is not available", "install the Compose plugin (`docker compose version` must work)")),
    }

    // Memory and clock, from the daemon (on Docker Desktop that is the VM, which is what counts).
    if let Ok(o) = docker(ctx, &["info", "--format", "{{.MemTotal}}|{{.SystemTime}}"]) {
        if o.ok() {
            let text = o.stdout_text();
            let (mem, time) = text.trim().split_once('|').unwrap_or((text.trim(), ""));
            match mem.trim().parse::<u64>() {
                Ok(m) if m >= 4 << 30 => checks.push(Check::new("memory", Level::Pass, "Memory for containers", human_bytes(m))),
                Ok(m) if m >= 2 << 30 => checks.push(Check::new("memory", Level::Warn, "Little memory for containers", format!("{} (about 4 GiB is recommended; the service limits add up to 5.25 GiB)", human_bytes(m)))),
                Ok(m) => checks.push(Check::new("memory", Level::Fail, "Not enough memory for containers", format!("{} - give Docker at least 2 GiB (4 GiB recommended)", human_bytes(m)))),
                Err(_) => checks.push(Check::new("memory", Level::Skip, "Memory for containers", "Docker did not report it")),
            }
            match skew_seconds(time, (ctx.now)()) {
                Some(s) if s.abs() <= 60 => checks.push(Check::new("clock", Level::Pass, "Clock agrees with Docker's", format!("{s} s apart"))),
                Some(s) if s.abs() <= 300 => checks.push(Check::new("clock", Level::Warn, "Clock differs from Docker's", format!("{s} s apart - keep clocks in step with NTP; tokens and certificates depend on it"))),
                Some(s) => checks.push(Check::new("clock", Level::Fail, "Clock differs a lot from Docker's", format!("{s} s apart - fix the time source (NTP) before installing"))),
                None => checks.push(Check::new("clock", Level::Skip, "Clock agrees with Docker's", "Docker did not report its time")),
            }
        }
    }

    // Disk.
    let probe = if input.dir.exists() { input.dir.to_path_buf() } else { input.dir.parent().map(Path::to_path_buf).unwrap_or_else(|| input.dir.to_path_buf()) };
    match free_bytes(&probe) {
        Some(f) if f >= 10 << 30 => checks.push(Check::new("disk", Level::Pass, "Free disk space", human_bytes(f))),
        Some(f) if f >= 2 << 30 => checks.push(Check::new("disk", Level::Warn, "Little free disk space", format!("{} at {} (10 GiB is recommended for a trial)", human_bytes(f), probe.display()))),
        Some(f) => checks.push(Check::new("disk", Level::Fail, "Not enough free disk space", format!("{} at {} - free some space", human_bytes(f), probe.display()))),
        None => checks.push(Check::new("disk", Level::Skip, "Free disk space", "not available on this platform")),
    }

    // Rights: can we write here?
    let test = input.dir.join(".dynodoctl-write-test");
    let _ = std::fs::create_dir_all(input.dir);
    match std::fs::write(&test, b"x") {
        Ok(()) => {
            let _ = std::fs::remove_file(&test);
            checks.push(Check::new("permissions", Level::Pass, "The install directory is writable", input.dir.display().to_string()));
        }
        Err(e) => checks.push(Check::new("permissions", Level::Fail, "The install directory is not writable", format!("{}: {e} - run as the user who owns it", input.dir.display()))),
    }

    // Ports.
    match input.bind.parse::<IpAddr>() {
        Ok(ip) => {
            for (name, port) in [("HTTPS", input.https_port), ("HTTP", input.http_port)] {
                let id = format!("port-{port}");
                if input.stack_running {
                    checks.push(Check::new(&id, Level::Skip, &format!("{name} port {port}"), "held by this installation's own proxy"));
                    continue;
                }
                match TcpListener::bind((ip, port)) {
                    Ok(_) => checks.push(Check::new(&id, Level::Pass, &format!("{name} port {port} on {ip} is free"), "")),
                    Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => checks.push(Check::new(&id, Level::Fail, &format!("{name} port {port} on {ip} is in use"), "another program holds it (IIS, WSL, another web server): choose --http-port / --https-port, and put the port in the origin")),
                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => checks.push(Check::new(&id, Level::Fail, &format!("{name} port {port} needs privileges"), "ports below 1024 need extra rights on some systems: choose --http-port 8080 --https-port 8443 and put :8443 in the origin")),
                    Err(e) => checks.push(Check::new(&id, Level::Warn, &format!("{name} port {port} on {ip} could not be tested"), e.to_string())),
                }
            }
        }
        Err(_) => checks.push(Check::new("bind", Level::Fail, "DYNODOC_BIND is not an address", format!("`{}` - use 127.0.0.1, 0.0.0.0 or one IP address", input.bind))),
    }

    // The name people will use.
    if let Some(origin) = &input.origin {
        let resolves = (origin.host.as_str(), origin.port).to_socket_addrs().map(|a| a.collect::<Vec<_>>()).unwrap_or_default();
        let acme = input.tls_mode.as_deref() == Some("acme");
        if resolves.is_empty() {
            let level = if acme { Level::Fail } else if origin.is_loopback_name() { Level::Pass } else { Level::Warn };
            checks.push(Check::new("dns", level, &format!("The name {} resolves", origin.host), if acme { "automatic certificates need a public DNS name that points at this machine".to_string() } else { "people will not reach it by this name until DNS points at this machine".to_string() }));
        } else {
            let all_private = resolves.iter().all(|a| match a.ip() {
                IpAddr::V4(v) => v.is_loopback() || v.is_private() || v.is_link_local(),
                IpAddr::V6(v) => v.is_loopback(),
            });
            if acme && all_private {
                checks.push(Check::new("dns", Level::Fail, &format!("{} resolves only to private addresses", origin.host), "the certificate authority cannot reach a private address; automatic certificates need a public name"));
            } else {
                checks.push(Check::new("dns", Level::Pass, &format!("The name {} resolves", origin.host), resolves.iter().map(|a| a.ip().to_string()).collect::<Vec<_>>().join(", ")));
            }
        }
    }
    if input.needs_images {
        checks.push(Check::new("images", Level::Skip, "Images", "checked when they are pulled or looked up"));
    }
    checks
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::{Layout, ScriptedConfirm};
    use crate::runner::FakeRunner;
    use chrono::TimeZone;

    fn input(dir: &Path) -> Input<'_> {
        Input { dir, bind: "127.0.0.1".into(), http_port: 0, https_port: 0, tls_mode: Some("evaluation".into()), origin: None, stack_running: false, needs_images: false }
    }

    fn run_with(runner: &FakeRunner, dir: &Path, edit: impl FnOnce(&mut Input<'_>)) -> Vec<Check> {
        let confirm = ScriptedConfirm::new(&[]);
        let mut ctx = Ctx::new(runner, Layout::new(dir), &confirm);
        ctx.now = || Utc.with_ymd_and_hms(2026, 10, 22, 12, 0, 0).unwrap();
        let mut i = input(dir);
        edit(&mut i);
        run(&ctx, &i)
    }

    fn healthy() -> FakeRunner {
        let r = FakeRunner::new();
        r.on("version --format {{.Server.Version}}", 0, "29.5.3\n");
        r.on("compose version --short", 0, "v5.1.4\n");
        r.on("info --format", 0, "8589934592|2026-10-22T12:00:10Z\n");
        r
    }

    fn level(checks: &[Check], id: &str) -> Level {
        checks.iter().find(|c| c.id == id).unwrap_or_else(|| panic!("no check {id}")).level
    }

    #[test]
    fn versions_parse_in_the_forms_docker_prints() {
        assert_eq!(parse_version("v5.1.4"), Some((5, 1, 4)));
        assert_eq!(parse_version("2.24.0-desktop.1"), Some((2, 24, 0)));
        assert_eq!(parse_version("2.24"), Some((2, 24, 0)));
        assert_eq!(parse_version("nonsense"), None);
    }

    #[test]
    fn a_healthy_host_passes_docker_compose_memory_clock_and_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let checks = run_with(&healthy(), dir.path(), |_| {});
        for id in ["docker", "compose", "memory", "clock", "permissions"] {
            assert_eq!(level(&checks, id), Level::Pass, "{id}: {checks:?}");
        }
        assert!(failed(&checks).is_empty() || failed(&checks).iter().all(|c| c.id == "disk" || c.id.starts_with("port")), "{:?}", failed(&checks));
    }

    #[test]
    fn old_compose_missing_docker_low_memory_and_clock_skew_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let old = FakeRunner::new();
        old.on("version --format {{.Server.Version}}", 1, "Cannot connect to the Docker daemon");
        old.on("compose version --short", 0, "v2.20.1\n");
        old.on("info --format", 0, "1073741824|2026-10-22T11:50:00Z\n");
        let checks = run_with(&old, dir.path(), |_| {});
        assert_eq!(level(&checks, "docker"), Level::Fail);
        assert_eq!(level(&checks, "compose"), Level::Fail);
        assert_eq!(level(&checks, "memory"), Level::Fail);
        assert_eq!(level(&checks, "clock"), Level::Fail);
    }

    #[test]
    fn a_port_in_use_is_a_failure_that_names_the_fix() {
        let dir = tempfile::tempdir().unwrap();
        let held = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = held.local_addr().unwrap().port();
        let checks = run_with(&healthy(), dir.path(), |i| i.https_port = port);
        let c = checks.iter().find(|c| c.id == format!("port-{port}")).unwrap();
        assert_eq!(c.level, Level::Fail);
        assert!(c.detail.contains("--https-port"));
    }

    #[test]
    fn ports_held_by_our_own_proxy_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let held = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = held.local_addr().unwrap().port();
        let checks = run_with(&healthy(), dir.path(), |i| {
            i.https_port = port;
            i.stack_running = true;
        });
        assert_eq!(level(&checks, &format!("port-{port}")), Level::Skip);
    }

    #[test]
    fn a_bad_bind_address_fails() {
        let dir = tempfile::tempdir().unwrap();
        let checks = run_with(&healthy(), dir.path(), |i| i.bind = "not-an-ip".into());
        assert_eq!(level(&checks, "bind"), Level::Fail);
    }

    #[test]
    fn acme_needs_a_public_name_other_modes_only_warn() {
        let dir = tempfile::tempdir().unwrap();
        let acme = run_with(&healthy(), dir.path(), |i| {
            i.tls_mode = Some("acme".into());
            i.origin = Some(Origin::parse("https://localhost").unwrap());
        });
        assert_eq!(level(&acme, "dns"), Level::Fail);
        let eval = run_with(&healthy(), dir.path(), |i| i.origin = Some(Origin::parse("https://localhost").unwrap()));
        assert_eq!(level(&eval, "dns"), Level::Pass);
        let unresolvable = run_with(&healthy(), dir.path(), |i| i.origin = Some(Origin::parse("https://no-such-host.invalid").unwrap()));
        assert_eq!(level(&unresolvable, "dns"), Level::Warn);
    }

    #[test]
    fn clock_skew_is_measured_in_the_right_direction() {
        let now = Utc.with_ymd_and_hms(2026, 10, 22, 12, 0, 0).unwrap();
        assert_eq!(skew_seconds("2026-10-22T11:59:00Z", now), Some(60));
        assert_eq!(skew_seconds("2026-10-22T12:01:00+00:00", now), Some(-60));
        assert_eq!(skew_seconds("garbage", now), None);
    }

    #[test]
    fn render_lists_every_check_with_its_level() {
        let text = render(&[Check::new("a", Level::Pass, "Fine", ""), Check::new("b", Level::Fail, "Broken", "do this")]);
        assert!(text.contains("pass  Fine\n") && text.contains("FAIL  Broken - do this\n"));
    }
}
