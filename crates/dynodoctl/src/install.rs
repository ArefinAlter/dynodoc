//! `dynodoctl install`: preflight, settings and secrets (through the bundle's own generator),
//! the TLS mode, images, `docker compose up --wait`, and the next steps. Idempotent: an existing
//! `.env` and `secrets/` are never rewritten, a conflicting setting is refused, and `--dry-run`
//! changes nothing.

use crate::compose::{docker, short, Compose};
use crate::ctx::Ctx;
use crate::envfile::EnvFile;
use crate::error::{CliError, Result};
use crate::generator::{self, Choice, GenerateArgs};
use crate::origin::Origin;
use crate::preflight::{self, Check, Level};
use crate::redact::Redactor;
use crate::report::{render_steps, Recorder, Step};
use crate::util::write_private;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const TLS_MODES: [&str; 3] = ["evaluation", "provided", "acme"];

#[derive(Clone, Debug)]
pub struct Options {
    /// A bundle directory to copy compose.yml, jobs/ and so on from when `--dir` is empty.
    pub from: Option<PathBuf>,
    pub origin: Option<String>,
    pub tls: Option<String>,
    pub admin_emails: Option<String>,
    pub org_name: Option<String>,
    pub brand_name: Option<String>,
    pub support_url: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_from: Option<String>,
    pub smtp_user: Option<String>,
    pub smtp_port: Option<String>,
    pub smtp_security: Option<String>,
    pub smtp_ca_file: Option<String>,
    /// The mail server password, read by the caller from a file or standard input (never an argument).
    pub smtp_password: Option<String>,
    pub with_converter: bool,
    pub cert: Option<PathBuf>,
    pub key: Option<PathBuf>,
    pub acme_email: Option<String>,
    pub acme_ca: Option<String>,
    pub acme_accept_tos: bool,
    pub bind: Option<String>,
    pub http_port: Option<u16>,
    pub https_port: Option<u16>,
    pub registry: Option<String>,
    pub version: Option<String>,
    /// A `compose.release.generated.yml` from the release (images pinned by digest).
    pub overlay: Option<PathBuf>,
    pub generator: Choice,
    pub generator_script: Option<PathBuf>,
    /// Build the images from the source checkout (`compose.build.yml`).
    pub build: bool,
    pub no_start: bool,
    pub dry_run: bool,
    pub wait_timeout: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            from: None,
            origin: None,
            tls: None,
            admin_emails: None,
            org_name: None,
            brand_name: None,
            support_url: None,
            smtp_host: None,
            smtp_from: None,
            smtp_user: None,
            smtp_port: None,
            smtp_security: None,
            smtp_ca_file: None,
            smtp_password: None,
            with_converter: false,
            cert: None,
            key: None,
            acme_email: None,
            acme_ca: None,
            acme_accept_tos: false,
            bind: None,
            http_port: None,
            https_port: None,
            registry: None,
            version: None,
            overlay: None,
            generator: Choice::Auto,
            generator_script: None,
            build: false,
            no_start: false,
            dry_run: false,
            wait_timeout: 300,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct InstallReport {
    pub dry_run: bool,
    pub directory: PathBuf,
    pub plan: Vec<String>,
    pub preflight: Vec<Check>,
    pub steps: Vec<Step>,
    pub generated_secrets: bool,
    pub started: bool,
    pub warnings: Vec<String>,
    pub next_steps: Vec<String>,
    pub ok: bool,
}

impl InstallReport {
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Install {} in {}\n\nPreflight\n",
            if self.dry_run {
                "(dry run, nothing changed)"
            } else {
                ""
            },
            self.directory.display()
        ));
        out.push_str(&preflight::render(&self.preflight));
        out.push_str("\nPlan\n");
        for (i, p) in self.plan.iter().enumerate() {
            out.push_str(&format!("  {}. {p}\n", i + 1));
        }
        if !self.steps.is_empty() {
            out.push_str("\nDone\n");
            out.push_str(&render_steps(&self.steps));
        }
        for w in &self.warnings {
            out.push_str(&format!("\nWARNING: {w}\n"));
        }
        if !self.next_steps.is_empty() {
            out.push_str("\nNext steps\n");
            for n in &self.next_steps {
                out.push_str(&format!("  - {n}\n"));
            }
        }
        out
    }
}

/// Check the options against each other. Pure.
pub fn validate(
    opts: &Options,
    installed: Option<&EnvFile>,
    tls_files_present: bool,
) -> Result<()> {
    let tls = opts
        .tls
        .as_deref()
        .or_else(|| installed.and_then(|e| e.get("DYNODOC_TLS_MODE")));
    match tls {
        None => {
            return Err(CliError::usage(
                "the certificate mode is not chosen",
                "choose one with --tls: `evaluation` (self-signed, to try it out), `provided` (your own certificate: --cert and --key) or `acme` (automatic public certificate); there is no default on purpose",
            ))
        }
        Some(mode) if !TLS_MODES.contains(&mode) => {
            return Err(CliError::usage(format!("`{mode}` is not a certificate mode"), "use evaluation, provided or acme"));
        }
        _ => {}
    }
    if installed.is_none() && opts.origin.is_none() {
        return Err(CliError::usage("the origin is not given", "pass --origin with exactly what people will type in the browser, for example https://docs.example.org"));
    }
    if tls == Some("provided") && !tls_files_present && (opts.cert.is_none() || opts.key.is_none())
    {
        return Err(CliError::usage("--tls provided needs your certificate", "pass --cert (the chain, server certificate first) and --key (PEM, no passphrase), or put them in tls/fullchain.pem and tls/privkey.pem first"));
    }
    if (opts.cert.is_some() || opts.key.is_some()) && tls != Some("provided") {
        return Err(CliError::usage(
            "--cert and --key only apply to --tls provided",
            "remove them, or choose --tls provided",
        ));
    }
    if tls == Some("acme") {
        if opts.acme_email.is_none()
            && installed
                .and_then(|e| e.get("DYNODOC_ACME_EMAIL"))
                .is_none()
        {
            return Err(CliError::usage(
                "--tls acme needs --acme-email",
                "give the address the certificate authority may write to",
            ));
        }
        if opts.acme_ca.is_none() && installed.and_then(|e| e.get("DYNODOC_ACME_CA")).is_none() {
            return Err(CliError::usage("--tls acme needs --acme-ca", "name the authority: letsencrypt-staging to test, letsencrypt for real, or an ACME directory URL; there is no default on purpose"));
        }
        if !opts.acme_accept_tos
            && installed.and_then(|e| e.get("DYNODOC_ACME_ACCEPT_TOS")) != Some("true")
        {
            return Err(CliError::usage(
                "the certificate authority's terms are not accepted",
                "read them, then pass --acme-accept-tos; nothing is requested until you do",
            ));
        }
    } else if opts.acme_email.is_some() || opts.acme_ca.is_some() || opts.acme_accept_tos {
        return Err(CliError::usage(
            "--acme-* options only apply to --tls acme",
            "remove them, or choose --tls acme",
        ));
    }
    if opts.smtp_host.is_none()
        && (opts.smtp_from.is_some()
            || opts.smtp_user.is_some()
            || opts.smtp_port.is_some()
            || opts.smtp_security.is_some()
            || opts.smtp_ca_file.is_some())
    {
        return Err(CliError::usage(
            "--smtp-* options need --smtp-host",
            "give the mail server with --smtp-host, or remove the others",
        ));
    }
    if opts.smtp_host.is_some() && opts.smtp_from.is_none() {
        return Err(CliError::usage(
            "--smtp-host needs --smtp-from",
            "give the sender address people will see on sign-in mail",
        ));
    }
    if let Some(p) = &opts.cert {
        if !p.is_file() {
            return Err(CliError::usage(
                format!("{} does not exist", p.display()),
                "check the --cert path",
            ));
        }
    }
    if let Some(p) = &opts.key {
        if !p.is_file() {
            return Err(CliError::usage(
                format!("{} does not exist", p.display()),
                "check the --key path",
            ));
        }
    }
    if opts.build && (opts.registry.is_some() || opts.overlay.is_some()) {
        return Err(CliError::usage(
            "--build and a registry/overlay are two ways to get images",
            "choose one",
        ));
    }
    Ok(())
}

/// The `.env` settings this install wants, in order. Pure.
pub fn desired_settings(
    opts: &Options,
    origin: &Origin,
    installed: Option<&EnvFile>,
) -> Vec<(String, String)> {
    let mut s: Vec<(String, String)> = Vec::new();
    let tls = opts
        .tls
        .clone()
        .or_else(|| installed.and_then(|e| e.get("DYNODOC_TLS_MODE").map(str::to_string)))
        .unwrap_or_default();
    s.push(("DYNODOC_TLS_MODE".into(), tls.clone()));
    if let Some(v) = &opts.admin_emails {
        s.push(("ADMIN_EMAILS".into(), v.clone()));
    }
    if let Some(v) = &opts.bind {
        s.push(("DYNODOC_BIND".into(), v.clone()));
    }
    if let Some(p) = opts.http_port {
        s.push(("DYNODOC_HTTP_PORT".into(), p.to_string()));
    }
    match (opts.https_port, origin.explicit_port) {
        (Some(p), _) => s.push(("DYNODOC_HTTPS_PORT".into(), p.to_string())),
        (None, true) => s.push(("DYNODOC_HTTPS_PORT".into(), origin.port.to_string())),
        _ => {}
    }
    let mut profiles = Vec::new();
    if tls == "acme" {
        profiles.push("acme");
        if let Some(v) = &opts.acme_email {
            s.push(("DYNODOC_ACME_EMAIL".into(), v.clone()));
        }
        if opts.acme_accept_tos {
            s.push(("DYNODOC_ACME_ACCEPT_TOS".into(), "true".into()));
        }
        if let Some(v) = &opts.acme_ca {
            s.push(("DYNODOC_ACME_CA".into(), v.clone()));
        }
    }
    if opts.with_converter {
        profiles.push("converter");
    }
    if !profiles.is_empty() {
        s.push(("COMPOSE_PROFILES".into(), profiles.join(",")));
    }
    if let Some(v) = &opts.registry {
        s.push(("DYNODOC_IMAGE_REGISTRY".into(), v.clone()));
    }
    if let Some(v) = &opts.version {
        s.push(("DYNODOC_VERSION".into(), v.clone()));
    }
    s
}

/// Add the settings to the file; a setting that already has a different value is a conflict.
pub fn apply_settings(env: &mut EnvFile, settings: &[(String, String)]) -> Result<usize> {
    for (k, v) in settings {
        if let Some(existing) = env.get(k) {
            if existing != v {
                return Err(CliError::refused(
                    format!("{k} is already set to `{existing}` in .env and you asked for `{v}`"),
                    "an installation is never reconfigured by `install`; change it with `dynodoctl configure set`",
                ));
            }
        }
    }
    let mut added = 0;
    let mut wrote_header = false;
    for (k, v) in settings {
        if env.get(k).is_none() {
            if !wrote_header {
                env.blank();
                env.comment("--- bundle settings (written by dynodoctl install) ---");
                wrote_header = true;
            }
            env.set(k, v)?;
            added += 1;
        }
    }
    Ok(added)
}

fn populate_bundle(ctx: &Ctx<'_>, from: &Path) -> Result<usize> {
    if !from.join("compose.yml").is_file() {
        return Err(CliError::refused(
            format!(
                "{} is not a bundle directory (no compose.yml)",
                from.display()
            ),
            "pass --from with the unpacked release or the infra/selfhosted directory of a checkout",
        ));
    }
    let mut copied = 0;
    for name in ["compose.yml", "compose.no-egress.yml", "bundle.env.example"] {
        if from.join(name).is_file() {
            std::fs::copy(from.join(name), ctx.layout.dir.join(name))?;
            copied += 1;
        }
    }
    for dir in ["jobs", "tls", "config"] {
        let src = from.join(dir);
        if !src.is_dir() {
            continue;
        }
        let dst = ctx.layout.dir.join(dir);
        std::fs::create_dir_all(&dst)?;
        for entry in std::fs::read_dir(&src)?.flatten() {
            if entry.path().is_file() {
                std::fs::copy(entry.path(), dst.join(entry.file_name()))?;
                copied += 1;
            }
        }
    }
    Ok(copied)
}

fn image_refs(env: &EnvFile, tls: &str, converter: bool) -> Vec<String> {
    let registry = env
        .get("DYNODOC_IMAGE_REGISTRY")
        .filter(|r| !r.is_empty())
        .unwrap_or("dynodoc-local");
    let version = env
        .get("DYNODOC_VERSION")
        .filter(|r| !r.is_empty())
        .unwrap_or("dev");
    let mut services = vec!["api", "web", "tools", "proxy"];
    if tls == "acme" {
        services.push("acme");
    }
    if converter {
        services.push("converter");
    }
    services
        .iter()
        .map(|s| format!("{registry}/{s}:{version}"))
        .collect()
}

/// The explanation of the first administrator exactly as the bundle README states it today.
pub fn next_steps(origin: &Origin, tls: &str, env: &EnvFile, mail_configured: bool) -> Vec<String> {
    let admins = env.get("ADMIN_EMAILS").filter(|a| !a.is_empty());
    let mut v = vec![format!(
        "Open {}.{}",
        origin.url(),
        if tls == "evaluation" {
            " The browser warns about the certificate: that is expected in evaluation mode."
        } else {
            ""
        }
    )];
    match admins {
        Some(a) => v.push(format!("Administrators (/admin) are the addresses in ADMIN_EMAILS: {a}. An address becomes an administrator once its owner has signed in with an emailed link.")),
        None => v.push("No ADMIN_EMAILS is set, so nobody can use /admin yet: `dynodoctl configure set admin-emails=you@example.org --apply`.".into()),
    }
    if mail_configured {
        v.push("Sign in by asking for an email link on the sign-in page. Check the mail server first: `dynodoctl configure check --mail-to you@example.org`.".into());
    } else {
        v.push("Email sign-in is OFF because no mail server was given, and today it is the only way in (besides Google sign-in, which needs internet access): nobody can sign in, the first administrator included. Add a mail server with `dynodoctl configure set smtp-host=... smtp-from=... --apply`.".into());
    }
    v.push("There is no operator-issued sign-in link yet (task P0-3 is not built). The first request for a link creates the account, so anyone who can reach the sign-in page can make one: limit who can reach the installation (DYNODOC_BIND, a firewall, a VPN) until the identity features exist.".into());
    v.push("Take a backup now and keep .env and secrets/ somewhere separate from this server: `dynodoctl backup --secrets-to <other disk>`.".into());
    v.push("Check the installation from the outside with `dynodoctl verify-delivery`, and any time with `dynodoctl status`.".into());
    v
}

fn diagnose(ctx: &Ctx<'_>, c: &Compose<'_>) -> String {
    let mut text = String::new();
    if let Ok(services) = c.ps() {
        for s in services
            .iter()
            .filter(|s| (s.state == "exited" && s.exit_code != 0) || s.health == "unhealthy")
        {
            if let Ok(logs) = c.run(&["logs", "--no-color", "--tail", "12", &s.service]) {
                text.push_str(&format!(
                    "\n--- {} ({}{}) ---\n{}",
                    s.service,
                    s.state,
                    if s.exit_code != 0 {
                        format!(", exit {}", s.exit_code)
                    } else {
                        String::new()
                    },
                    Redactor::new(false).apply(logs.text().trim())
                ));
            }
        }
    }
    let _ = ctx;
    text
}

pub fn run(ctx: &Ctx<'_>, opts: &Options) -> Result<InstallReport> {
    let dir = ctx.layout.dir.clone();
    let mut warnings = Vec::new();
    let mut plan = Vec::new();
    let mut rec = Recorder::new();

    let installed_env = if ctx.layout.is_installed() {
        Some(ctx.layout.read_env()?)
    } else {
        None
    };
    if ctx.layout.env_file().is_file() != ctx.layout.secrets_dir().is_dir() {
        return Err(CliError::refused(
            "this directory is half installed: only one of .env and secrets/ exists",
            "restore the missing one from your backup, or remove the other to start again; install never overwrites either",
        ));
    }
    let tls_present = ctx.layout.tls_dir().join("fullchain.pem").is_file()
        && ctx.layout.tls_dir().join("privkey.pem").is_file();
    validate(opts, installed_env.as_ref(), tls_present)?;
    let origin_text = opts
        .origin
        .clone()
        .or_else(|| {
            installed_env
                .as_ref()
                .and_then(|e| e.get("ORIGIN").map(str::to_string))
        })
        .unwrap_or_default();
    let origin = Origin::parse(&origin_text)?;
    if let (Some(env), Some(given)) = (&installed_env, &opts.origin) {
        if env.get("ORIGIN").map(|o| o != given).unwrap_or(false) {
            return Err(CliError::refused(
                format!(
                    "ORIGIN is already {} and you passed {given}",
                    env.get("ORIGIN").unwrap_or("")
                ),
                "change it with `dynodoctl configure set origin=...`",
            ));
        }
    }
    let tls = opts
        .tls
        .clone()
        .or_else(|| {
            installed_env
                .as_ref()
                .and_then(|e| e.get("DYNODOC_TLS_MODE").map(str::to_string))
        })
        .unwrap_or_default();
    let settings = desired_settings(opts, &origin, installed_env.as_ref());
    if let Some(env) = &installed_env {
        // Conflicts are refused before anything is changed.
        let mut probe = env.clone();
        apply_settings(&mut probe, &settings)?;
    }

    // Is the bundle there?
    let need_populate = !ctx.layout.is_bundle();
    if need_populate && opts.from.is_none() {
        return Err(CliError::refused(
            format!("{} has no compose.yml", dir.display()),
            "run from the bundle directory (the unpacked release, or infra/selfhosted in a checkout), or pass --from <bundle directory> to copy it here",
        ));
    }
    let bind = opts
        .bind
        .clone()
        .or_else(|| {
            installed_env
                .as_ref()
                .and_then(|e| e.get("DYNODOC_BIND").map(str::to_string))
        })
        .unwrap_or_else(|| "127.0.0.1".into());
    let https_port = opts
        .https_port
        .or_else(|| origin.explicit_port.then_some(origin.port))
        .or_else(|| {
            installed_env
                .as_ref()
                .and_then(|e| e.get("DYNODOC_HTTPS_PORT")?.parse().ok())
        })
        .unwrap_or(443);
    let http_port = opts
        .http_port
        .or_else(|| {
            installed_env
                .as_ref()
                .and_then(|e| e.get("DYNODOC_HTTP_PORT")?.parse().ok())
        })
        .unwrap_or(80);
    let stack_running = ctx.layout.is_bundle()
        && Compose::new(ctx)
            .ps()
            .map(|s| s.iter().any(|x| x.service == "proxy" && x.running()))
            .unwrap_or(false);
    let checks = preflight::run(
        ctx,
        &preflight::Input {
            dir: &dir,
            bind: bind.clone(),
            http_port,
            https_port,
            tls_mode: Some(tls.clone()),
            origin: Some(origin.clone()),
            stack_running,
            needs_images: false,
        },
    );
    let preflight_ok = level_ok(&checks);
    if !preflight_ok && !opts.dry_run {
        let failures = preflight::failed(&checks);
        return Err(CliError::refused(
            format!(
                "the machine is not ready:\n{}",
                failures
                    .iter()
                    .map(|c| format!("  - {}: {}", c.title, c.detail))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            "fix what is listed and run `dynodoctl install` again; nothing was changed",
        ));
    }

    if need_populate {
        plan.push(format!(
            "copy the bundle files (compose.yml, jobs/, tls/, config/) from {} to {}",
            opts.from
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            dir.display()
        ));
    }
    let generate_secrets = installed_env.is_none();
    if generate_secrets {
        plan.push("create .env and secrets/ with the bundle's generator (random secrets, mode 0600, nothing overwritten)".to_string());
    } else {
        plan.push("keep the existing .env and secrets/ (never overwritten)".to_string());
    }
    plan.push(format!(
        "set {}",
        settings
            .iter()
            .map(|(k, v)| format!("{k}={}", crate::envfile::display_value(k, v)))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    if tls == "provided" {
        plan.push("copy your certificate and key into tls/ (the key with mode 0600)".to_string());
    }
    if opts.build {
        plan.push(
            "build the images from this source checkout (the first build takes a long time)"
                .to_string(),
        );
    } else if opts.registry.is_some()
        || installed_env
            .as_ref()
            .and_then(|e| e.get("DYNODOC_IMAGE_REGISTRY"))
            .is_some()
    {
        plan.push("pull the images from the registry".to_string());
    } else {
        plan.push("use the images already on this machine (dynodoc-local/*:dev)".to_string());
    }
    if !opts.no_start {
        plan.push(format!("docker compose up -d --wait (secrets-init, preflight, db-roles, init, then the services; up to {} s)", opts.wait_timeout));
    }
    if tls == "evaluation" {
        warnings.push("TLS mode `evaluation`: the certificate is self-signed and labelled \"NOT FOR PRODUCTION\"; browsers warn and Office add-ins refuse it. Use `provided` or `acme` for real use.".to_string());
    }
    if tls == "acme" && bind == "127.0.0.1" {
        warnings.push("automatic certificates need ports 80 and 443 reachable from the internet, but DYNODOC_BIND is 127.0.0.1 (this machine only). Add --bind 0.0.0.0 if that is what you want.".to_string());
    }
    if bind != "127.0.0.1" {
        warnings.push(format!("DYNODOC_BIND is {bind}: other computers can reach the installation. Docker publishes ports around host firewalls such as ufw."));
    }
    if opts.dry_run {
        return Ok(InstallReport {
            dry_run: true,
            directory: dir,
            plan,
            preflight: checks,
            steps: Vec::new(),
            generated_secrets: false,
            started: false,
            warnings,
            next_steps: Vec::new(),
            ok: preflight_ok,
        });
    }

    // From here on the directory is changed.
    if !ctx.confirm.confirm(
        &format!("Install Dynodoc in {} with the plan above?", dir.display()),
        "yes",
    ) {
        return Err(CliError::refused(
            "the install was not confirmed; nothing was changed",
            "run again and answer `yes`, or pass --yes",
        ));
    }
    std::fs::create_dir_all(&dir)?;
    if need_populate {
        let n = rec.step("copy the bundle files", || {
            Ok((
                populate_bundle(ctx, opts.from.as_ref().expect("checked"))?,
                "done".to_string(),
            ))
        })?;
        rec.steps.last_mut().expect("step").detail = format!("{n} files");
    }
    if let Some(overlay) = &opts.overlay {
        rec.step("install the release overlay", || {
            std::fs::copy(overlay, ctx.layout.overlay_file())?;
            Ok((
                (),
                format!("{} (images pinned by digest)", crate::ctx::OVERLAY_NAME),
            ))
        })?;
    }

    let preliminary_env = installed_env.clone().unwrap_or_default();
    let tools = generator::tools_image(
        opts.registry
            .as_deref()
            .or(preliminary_env.get("DYNODOC_IMAGE_REGISTRY")),
        opts.version
            .as_deref()
            .or(preliminary_env.get("DYNODOC_VERSION")),
    );
    let c = Compose::new(ctx);

    // Images first: the generator may live in the tools image.
    let env_for_images = {
        let mut e = preliminary_env.clone();
        if let Some(r) = &opts.registry {
            e.set("DYNODOC_IMAGE_REGISTRY", r)?;
        }
        if let Some(v) = &opts.version {
            e.set("DYNODOC_VERSION", v)?;
        }
        e
    };
    let refs = image_refs(&env_for_images, &tls, opts.with_converter);
    let missing: Vec<&String> = refs
        .iter()
        .filter(|r| {
            !docker(ctx, &["image", "inspect", r])
                .map(|o| o.ok())
                .unwrap_or(false)
        })
        .collect();
    let registry_mode = env_for_images.get("DYNODOC_IMAGE_REGISTRY").is_some();
    if !missing.is_empty() {
        if opts.build {
            if !ctx.layout.dir.join("compose.build.yml").is_file()
                || !ctx
                    .layout
                    .dir
                    .join("..")
                    .join("..")
                    .join("Cargo.toml")
                    .is_file()
            {
                return Err(CliError::refused("--build needs the bundle directory inside a source checkout (infra/selfhosted)", "run from infra/selfhosted of the repository, or pull images from a registry"));
            }
            rec.step("build the images", || {
                let builder = Compose::new(ctx)
                    .with_files(vec![ctx.layout.compose_file(), ctx.layout.dir.join("compose.build.yml")])
                    .with_env("ORIGIN", &origin.url())
                    .with_env("DYNODOC_TLS_MODE", &tls);
                let out = builder.run(&["--profile", "*", "build", "api", "web", "preflight", "proxy"])?;
                if !out.ok() {
                    return Err(CliError::failed(format!("the image build failed: {}", short(&out.text())), "run the build by hand to see all of it: docker compose -f compose.yml -f compose.build.yml build"));
                }
                Ok(((), "api, web, tools, proxy".to_string()))
            })?;
        } else if registry_mode {
            rec.step("docker compose pull", || {
                let out = c.run(&["pull"])?;
                if !out.ok() {
                    return Err(CliError::failed(
                        format!("pulling the images failed: {}", short(&out.text())),
                        "check the registry name, your network and `docker login`",
                    ));
                }
                Ok(((), format!("{} images", refs.len())))
            })?;
        } else {
            return Err(CliError::refused(
                format!("these images are not on this machine: {}", missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")),
                "build them from a source checkout (`dynodoctl install --build`, or `docker compose -f compose.yml -f compose.build.yml build`) or install from a registry (`--registry` and `--version`)",
            ));
        }
    }

    let mut generated = false;
    if generate_secrets {
        let args = GenerateArgs {
            origin: origin.url(),
            org_name: opts.org_name.clone(),
            brand_name: opts.brand_name.clone(),
            support_url: opts.support_url.clone(),
            smtp_host: opts.smtp_host.clone(),
            smtp_from: opts.smtp_from.clone(),
            smtp_user: opts.smtp_user.clone(),
            smtp_port: opts.smtp_port.clone(),
            smtp_security: opts.smtp_security.clone(),
            smtp_ca_file: opts.smtp_ca_file.clone(),
            with_converter: opts.with_converter,
        };
        let generator = generator::choose(
            ctx,
            opts.generator,
            opts.generator_script.as_deref(),
            &tools,
        )?;
        rec.step("create .env and secrets/ (generator)", || {
            generator::generate(ctx, &generator, &args)?;
            Ok((
                (),
                match &generator {
                    generator::Generator::Image(i) => format!("through the image {i}"),
                    generator::Generator::Node { .. } => {
                        "through Node and the source checkout".to_string()
                    }
                },
            ))
        })?;
        generated = true;
    }

    let mut env = ctx.layout.read_env()?;
    rec.step("write the bundle settings to .env", || {
        let n = apply_settings(&mut env, &settings)?;
        write_private(&ctx.layout.env_file(), env.render().as_bytes())?;
        Ok(((), format!("{n} settings added")))
    })?;

    if tls == "provided" {
        if let (Some(cert), Some(key)) = (&opts.cert, &opts.key) {
            rec.step("copy the certificate and key into tls/", || {
                for (src, name) in [(cert, "fullchain.pem"), (key, "privkey.pem")] {
                    let dst = ctx.layout.tls_dir().join(name);
                    let bytes = std::fs::read(src)?;
                    if dst.exists() && std::fs::read(&dst)? != bytes {
                        return Err(CliError::refused(format!("tls/{name} already exists and differs"), "renew a certificate with the procedure in the README (replace the file, then `proxy check`); install never overwrites it"));
                    }
                    write_private(&dst, &bytes)?;
                }
                Ok(((), "tls/fullchain.pem, tls/privkey.pem (key mode 0600)".to_string()))
            })?;
        }
    }

    if opts.smtp_user.is_some() {
        match &opts.smtp_password {
            Some(p) if !p.trim().is_empty() => {
                rec.step("store the mail server password", || {
                    write_private(
                        &ctx.layout.secrets_dir().join("smtp_password"),
                        format!("{}\n", p.trim()).as_bytes(),
                    )?;
                    Ok(((), "secrets/smtp_password (mode 0600)".to_string()))
                })?;
            }
            _ => {
                let empty = std::fs::metadata(ctx.layout.secrets_dir().join("smtp_password"))
                    .map(|m| m.len() == 0)
                    .unwrap_or(true);
                if empty {
                    return Err(CliError::refused(
                        "secrets/smtp_password is empty: the mail server password is not yours to invent",
                        "put the password (one line) in secrets/smtp_password, or pass --smtp-password-file, then run `dynodoctl install` again; it will carry on from here",
                    ));
                }
            }
        }
    }

    let mut started = false;
    if opts.no_start {
        rec.note("start", true, "skipped (--no-start)");
    } else {
        let timeout = opts.wait_timeout.to_string();
        let result = rec.step("docker compose up --wait", || {
            let out = c.run(&["up", "-d", "--wait", "--wait-timeout", &timeout])?;
            if !out.ok() {
                return Err(CliError::failed(
                    format!("the stack did not come up: {}{}", short(&out.text()), diagnose(ctx, &c)),
                    "read the log lines above; the README's Troubleshooting table lists each message. Fix it and run `dynodoctl install` again: it is safe to repeat",
                ));
            }
            Ok(((), "all services started".to_string()))
        });
        result?;
        let services = c.ps()?;
        let down: Vec<String> = services
            .iter()
            .filter(|s| crate::compose::LONG_RUNNING.contains(&s.service.as_str()) && !s.up())
            .map(|s| s.service.clone())
            .collect();
        if !down.is_empty() {
            return Err(CliError::failed(
                format!("these services are not healthy: {}", down.join(", ")),
                "run `dynodoctl status` and `docker compose logs <service>`",
            ));
        }
        started = true;
    }
    let mail = env.get("DYNODOC_MAIL_TRANSPORT") == Some("smtp");
    let next = next_steps(&origin, &tls, &env, mail);
    Ok(InstallReport {
        dry_run: false,
        directory: dir,
        plan,
        preflight: checks,
        steps: rec.steps,
        generated_secrets: generated,
        started,
        warnings,
        next_steps: next,
        ok: true,
    })
}

pub fn level_ok(checks: &[Check]) -> bool {
    !checks.iter().any(|c| c.level == Level::Fail)
}

#[cfg(test)]
#[path = "tests/install.rs"]
mod tests;
