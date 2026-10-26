//! Where the local workspace lives on disk, and which programs it runs.
//!
//! Everything the local workspace owns is under one folder in the per-user data directory, so
//! that "back up", "restore" and "uninstall and keep my data" each have one answer:
//!
//! ```text
//! <data dir>/workspace/
//!   pgdata/        the PostgreSQL cluster (the engine's database and the app's own state)
//!   logs/          postgres.log, engine.log (rotated at start-up, never holding a secret)
//!   run/           engine.pid (the engine's process, so an orphan can be stopped), port files
//!   workspace.json what the app knows about this workspace (schema, profile, versions)
//! ```

use std::path::{Path, PathBuf};

/// The folder names and files, in one place so a test can check them against the docs.
pub const WORKSPACE_DIR: &str = "workspace";
pub const PGDATA_DIR: &str = "pgdata";
pub const LOGS_DIR: &str = "logs";
pub const RUN_DIR: &str = "run";
pub const STATE_FILE: &str = "workspace.json";

#[derive(Debug, Clone)]
pub struct Layout {
    pub root: PathBuf,
    pub pgdata: PathBuf,
    pub logs: PathBuf,
    pub run: PathBuf,
    pub state_file: PathBuf,
}

impl Layout {
    pub fn new(data_dir: &Path) -> Self {
        let root = data_dir.join(WORKSPACE_DIR);
        Self {
            pgdata: root.join(PGDATA_DIR),
            logs: root.join(LOGS_DIR),
            run: root.join(RUN_DIR),
            state_file: root.join(STATE_FILE),
            root,
        }
    }

    /// Creates the folders the workspace writes to (not `pgdata`: `initdb` makes that one).
    pub fn ensure(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.logs)?;
        std::fs::create_dir_all(&self.run)?;
        Ok(())
    }

    pub fn postgres_log(&self) -> PathBuf {
        self.logs.join("postgres.log")
    }

    pub fn engine_log(&self) -> PathBuf {
        self.logs.join("engine.log")
    }

    pub fn engine_pid_file(&self) -> PathBuf {
        self.run.join("engine.pid")
    }

    /// True when a PostgreSQL cluster was created here (`initdb` writes `PG_VERSION` last).
    pub fn has_cluster(&self) -> bool {
        self.pgdata.join("PG_VERSION").is_file()
    }
}

/// The programs the workspace runs. All of them ship inside the installation.
#[derive(Debug, Clone)]
pub struct Programs {
    /// The PostgreSQL tree: `bin/pg_ctl`, `bin/initdb`, `bin/postgres`, `lib`, `share`.
    pub pg_root: PathBuf,
    pub engine_api: PathBuf,
    pub engine_cli: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing(pub String);

impl std::fmt::Display for Missing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "This installation is incomplete: {} is missing.", self.0)
    }
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

impl Programs {
    /// Looks in the places the installers put things: the sidecars next to the app, the
    /// database tree in the resource folder. Debug builds also honour `DYNODOC_PROGRAMS`, a
    /// folder holding `engine-api`, `engine-cli` and a `pg` tree (used by the tests and the
    /// acceptance scripts, never read by a release build).
    pub fn discover(resource_dir: Option<&Path>, exe_dir: &Path) -> Result<Self, Missing> {
        let mut places: Vec<PathBuf> = Vec::new();
        if cfg!(debug_assertions) {
            if let Some(dir) = std::env::var_os("DYNODOC_PROGRAMS") {
                places.push(PathBuf::from(dir));
            }
        }
        places.push(exe_dir.to_path_buf());
        if let Some(r) = resource_dir {
            places.push(r.to_path_buf());
        }
        let pg_places: Vec<PathBuf> = places.iter().map(|p| p.join("pg")).collect();
        let find = |name: &str| -> Result<PathBuf, Missing> {
            let file = exe(name);
            places
                .iter()
                .map(|p| p.join(&file))
                .find(|p| p.is_file())
                .ok_or_else(|| Missing(file))
        };
        let pg_root = pg_places
            .iter()
            .find(|p| p.join("bin").join(exe("pg_ctl")).is_file())
            .cloned()
            .ok_or_else(|| Missing("the bundled database (pg)".to_string()))?;
        Ok(Self {
            pg_root,
            engine_api: find("engine-api")?,
            engine_cli: find("engine-cli")?,
        })
    }

    pub fn pg_bin(&self, name: &str) -> PathBuf {
        self.pg_root.join("bin").join(exe(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_keeps_everything_under_one_folder() {
        let layout = Layout::new(Path::new("/data"));
        for p in [
            &layout.pgdata,
            &layout.logs,
            &layout.run,
            &layout.state_file,
        ] {
            assert!(p.starts_with("/data/workspace"), "{p:?}");
        }
        assert!(!layout.has_cluster());
    }

    #[test]
    fn programs_are_found_beside_the_app_and_in_the_resource_folder() {
        let root = std::env::temp_dir().join(format!("dynodoc-layout-{}", std::process::id()));
        let app = root.join("app");
        let res = root.join("res");
        std::fs::create_dir_all(app.join("pg").join("bin")).ok();
        std::fs::create_dir_all(res.join("pg").join("bin")).ok();
        let missing = Programs::discover(Some(&res), &app).unwrap_err();
        assert!(missing.to_string().contains("incomplete"));
        for name in ["engine-api", "engine-cli"] {
            std::fs::write(app.join(exe(name)), b"x").unwrap();
        }
        std::fs::write(res.join("pg").join("bin").join(exe("pg_ctl")), b"x").unwrap();
        let found = Programs::discover(Some(&res), &app).unwrap();
        assert_eq!(found.pg_root, res.join("pg"));
        assert_eq!(found.engine_api, app.join(exe("engine-api")));
        std::fs::remove_dir_all(&root).ok();
    }
}
