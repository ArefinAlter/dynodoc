//! Settings and secrets are made by `frontend/researcher/scripts/dynodoc-env.mjs`, the generator
//! the bundle README documents. dynodoctl CALLS it and does not re-implement it: one source of
//! truth for the file formats, the secret shapes and the validator that `preflight` and the web
//! server run. It is reached through the `tools` image (what a release ships; no Node needed on
//! the host) or through Node and a source checkout.

use crate::compose::{docker, run_cmd, short};
use crate::ctx::Ctx;
use crate::error::{CliError, Result};
use crate::preflight::parse_version;
use crate::runner::Cmd;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Choice {
    Auto,
    Node,
    Image,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Generator {
    Image(String),
    Node { script: PathBuf },
}

/// The flags that follow `generate-env --dir ...`, from the install options. Pure, so it is tested.
#[derive(Clone, Debug, Default)]
pub struct GenerateArgs {
    pub origin: String,
    pub org_name: Option<String>,
    pub brand_name: Option<String>,
    pub support_url: Option<String>,
    pub smtp_host: Option<String>,
    pub smtp_from: Option<String>,
    pub smtp_user: Option<String>,
    pub smtp_port: Option<String>,
    pub smtp_security: Option<String>,
    pub smtp_ca_file: Option<String>,
    pub with_converter: bool,
}

impl GenerateArgs {
    pub fn flags(&self) -> Vec<String> {
        let mut args = vec!["--origin".to_string(), self.origin.clone(), "--container-secrets-dir".into(), "/run/secrets".into()];
        let mut opt = |name: &str, value: &Option<String>| {
            if let Some(v) = value {
                args.push(name.to_string());
                args.push(v.clone());
            }
        };
        opt("--org-name", &self.org_name);
        opt("--brand-name", &self.brand_name);
        opt("--support-url", &self.support_url);
        opt("--smtp-host", &self.smtp_host);
        opt("--smtp-from", &self.smtp_from);
        opt("--smtp-user", &self.smtp_user);
        opt("--smtp-port", &self.smtp_port);
        opt("--smtp-security", &self.smtp_security);
        opt("--smtp-ca-file", &self.smtp_ca_file);
        if self.with_converter {
            args.push("--with-converter".into());
        }
        args
    }
}

/// `registry/tools:version`, the image the `preflight` job also runs.
pub fn tools_image(registry: Option<&str>, version: Option<&str>) -> String {
    format!("{}/tools:{}", registry.unwrap_or("dynodoc-local"), version.unwrap_or("dev"))
}

fn node_ok(ctx: &Ctx<'_>) -> bool {
    let mut cmd = Cmd::new("node", &["--version"]);
    cmd.scrub_env = true;
    match run_cmd(ctx, &cmd) {
        Ok(o) if o.ok() => parse_version(&o.stdout_text()).map(|v| v.0 >= 20).unwrap_or(false),
        _ => false,
    }
}

fn find_script(ctx: &Ctx<'_>, explicit: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return p.is_file().then(|| p.to_path_buf());
    }
    let candidate = ctx.layout.dir.join("..").join("..").join("frontend").join("researcher").join("scripts").join("dynodoc-env.mjs");
    candidate.is_file().then_some(candidate)
}

pub fn choose(ctx: &Ctx<'_>, choice: Choice, script: Option<&Path>, image: &str) -> Result<Generator> {
    let have_image = || docker(ctx, &["image", "inspect", image]).map(|o| o.ok()).unwrap_or(false);
    let node = || find_script(ctx, script).filter(|_| node_ok(ctx));
    match choice {
        Choice::Image => {
            if have_image() {
                Ok(Generator::Image(image.to_string()))
            } else {
                Err(CliError::refused(format!("the tools image {image} is not on this machine"), "pull or build the images first (`docker compose pull`, or `install --build` in a source checkout), or use --generator node"))
            }
        }
        Choice::Node => node().map(|script| Generator::Node { script }).ok_or_else(|| {
            CliError::refused("Node 20 or newer and the generator script were not both found", "install Node 20+, run from a source checkout or pass --generator-script PATH, or use --generator image")
        }),
        Choice::Auto => {
            if have_image() {
                Ok(Generator::Image(image.to_string()))
            } else if let Some(script) = node() {
                Ok(Generator::Node { script })
            } else {
                Err(CliError::refused(
                    format!("neither the tools image {image} nor Node 20 with the generator script is available"),
                    "get the images first (a release: `docker compose pull`; a source checkout: `install --build`), or install Node 20+ and run from the checkout",
                ))
            }
        }
    }
}

#[cfg(unix)]
fn user_flag() -> Vec<String> {
    // SAFETY: getuid and getgid have no preconditions and cannot fail.
    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    vec!["--user".into(), format!("{uid}:{gid}")]
}

#[cfg(not(unix))]
fn user_flag() -> Vec<String> {
    Vec::new()
}

/// The command that runs `generate-env` into `dir`.
pub fn command(generator: &Generator, dir: &Path, args: &GenerateArgs) -> Cmd {
    match generator {
        Generator::Image(image) => {
            let mut all: Vec<String> = vec!["run".into(), "--rm".into()];
            all.extend(user_flag());
            all.extend(["-v".into(), format!("{}:/bundle", dir.display()), image.clone(), "node".into(), "scripts/dynodoc-env.mjs".into(), "generate-env".into(), "--dir".into(), "/bundle".into()]);
            all.extend(args.flags());
            Cmd { program: "docker".into(), args: all, scrub_env: true, ..Cmd::default() }
        }
        Generator::Node { script } => {
            let mut all: Vec<String> = vec![script.display().to_string(), "generate-env".into(), "--dir".into(), dir.display().to_string()];
            all.extend(args.flags());
            Cmd { program: "node".into(), args: all, scrub_env: true, ..Cmd::default() }
        }
    }
}

/// Run the generator. Its own output lists the files and what they protect; it prints no secret.
pub fn generate(ctx: &Ctx<'_>, generator: &Generator, args: &GenerateArgs) -> Result<String> {
    let out = run_cmd(ctx, &command(generator, &ctx.layout.dir, args))?;
    if !out.ok() {
        return Err(CliError::failed(
            format!("the settings generator failed: {}", short(&out.text())),
            "fix the option it names and run `dynodoctl install` again; nothing is overwritten, so a partial result can be removed by deleting .env and secrets/",
        ));
    }
    Ok(out.stdout_text())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ctx::{Layout, ScriptedConfirm};
    use crate::runner::FakeRunner;

    #[test]
    fn flags_are_passed_through_in_the_generators_vocabulary() {
        let a = GenerateArgs {
            origin: "https://docs.example.org".into(),
            org_name: Some("Example Ltd.".into()),
            smtp_host: Some("smtp.example.org".into()),
            smtp_from: Some("docs@example.org".into()),
            with_converter: true,
            ..Default::default()
        };
        let f = a.flags();
        assert_eq!(&f[..4], ["--origin", "https://docs.example.org", "--container-secrets-dir", "/run/secrets"]);
        assert!(f.windows(2).any(|w| w == ["--org-name", "Example Ltd."]));
        assert!(f.windows(2).any(|w| w == ["--smtp-host", "smtp.example.org"]));
        assert!(f.contains(&"--with-converter".to_string()));
        assert!(!f.iter().any(|x| x == "--smtp-user"));
    }

    #[test]
    fn the_image_route_mounts_the_bundle_directory_and_runs_the_scripts_command() {
        let cmd = command(&Generator::Image("dynodoc-local/tools:dev".into()), Path::new("/srv/dynodoc"), &GenerateArgs { origin: "https://x.example".into(), ..Default::default() });
        let line = cmd.line();
        assert!(line.starts_with("docker run --rm"));
        assert!(line.contains("-v /srv/dynodoc:/bundle dynodoc-local/tools:dev node scripts/dynodoc-env.mjs generate-env --dir /bundle --origin https://x.example"));
    }

    #[test]
    fn the_node_route_runs_the_script_with_the_directory() {
        let cmd = command(&Generator::Node { script: PathBuf::from("/repo/frontend/researcher/scripts/dynodoc-env.mjs") }, Path::new("/srv/d"), &GenerateArgs { origin: "https://x.example".into(), ..Default::default() });
        assert_eq!(cmd.program, "node");
        assert_eq!(&cmd.args[1..4], ["generate-env", "--dir", "/srv/d"]);
    }

    #[test]
    fn auto_prefers_the_image_then_node_then_explains() {
        let dir = tempfile::tempdir().unwrap();
        let confirm = ScriptedConfirm::new(&[]);
        let with_image = FakeRunner::new();
        let ctx = Ctx::new(&with_image, Layout::new(dir.path()), &confirm);
        assert_eq!(choose(&ctx, Choice::Auto, None, "i:1").unwrap(), Generator::Image("i:1".into()));

        let neither = FakeRunner::new();
        neither.on("image inspect", 1, "");
        neither.on("node --version", 1, "");
        let ctx = Ctx::new(&neither, Layout::new(dir.path()), &confirm);
        let err = choose(&ctx, Choice::Auto, None, "i:1").unwrap_err();
        assert_eq!(err.code(), crate::error::exit::REFUSED);
        assert!(err.todo.contains("install --build") || err.todo.contains("install Node"));
    }

    #[test]
    fn node_needs_version_20_and_the_script() {
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("dynodoc-env.mjs");
        std::fs::write(&script, "//").unwrap();
        let confirm = ScriptedConfirm::new(&[]);
        let old = FakeRunner::new();
        old.on("node --version", 0, "v18.19.0\n");
        let ctx = Ctx::new(&old, Layout::new(dir.path()), &confirm);
        assert!(choose(&ctx, Choice::Node, Some(&script), "i:1").is_err());
        let new = FakeRunner::new();
        new.on("node --version", 0, "v22.1.0\n");
        let ctx = Ctx::new(&new, Layout::new(dir.path()), &confirm);
        assert_eq!(choose(&ctx, Choice::Node, Some(&script), "i:1").unwrap(), Generator::Node { script });
    }

    #[test]
    fn a_failing_generator_reports_its_words_and_what_to_do() {
        let dir = tempfile::tempdir().unwrap();
        let confirm = ScriptedConfirm::new(&[]);
        let runner = FakeRunner::new();
        runner.on("generate-env", 2, "");
        let ctx = Ctx::new(&runner, Layout::new(dir.path()), &confirm);
        let err = generate(&ctx, &Generator::Image("i:1".into()), &GenerateArgs { origin: "https://x.example".into(), ..Default::default() }).unwrap_err();
        assert!(err.what.contains("generator failed"));
        assert!(err.todo.contains("nothing is overwritten"));
    }
}
