//! Which folder each project is bound to, and what the app knows about the files in it. Kept in
//! the workspace's own database, so a backup carries it.

use super::appdb::now_ms;
use super::Local;
use crate::files::Picks;
use crate::watch::{self, Known, Watcher};
use std::collections::HashMap;
use std::path::PathBuf;

impl Local {
    pub async fn folder_of(&self, project_id: &str) -> Result<Option<PathBuf>, String> {
        let app = self.app().await?;
        app.query_opt("select folder from folder_binding where project_id = $1", &[&project_id])
            .await
            .map(|r| r.map(|r| PathBuf::from(r.get::<_, String>(0))))
            .map_err(|_| "The project's folder could not be read.".to_string())
    }

    pub async fn bind_folder(&self, project_id: &str, folder: &std::path::Path) -> Result<(), String> {
        let app = self.app().await?;
        app.execute(
            "insert into folder_binding (project_id, folder, created_at) values ($1, $2, $3)
             on conflict (project_id) do update set folder = excluded.folder",
            &[&project_id, &folder.to_string_lossy().to_string(), &now_ms()],
        )
        .await
        .map(|_| ())
        .map_err(|_| "The folder could not be saved.".to_string())
    }

    pub async fn unbind_folder(&self, project_id: &str) -> Result<(), String> {
        let app = self.app().await?;
        app.execute("delete from folder_binding where project_id = $1", &[&project_id])
            .await
            .map(|_| ())
            .map_err(|_| "The folder could not be released.".to_string())
    }

    pub async fn bound_folders(&self) -> Vec<(String, PathBuf)> {
        let Ok(app) = self.app().await else { return Vec::new() };
        app.query("select project_id, folder from folder_binding", &[])
            .await
            .map(|rows| {
                rows.iter()
                    .map(|r| (r.get::<_, String>(0), PathBuf::from(r.get::<_, String>(1))))
                    .collect()
            })
            .unwrap_or_default()
    }

    pub async fn known_files(&self, project_id: &str) -> HashMap<String, Known> {
        let Ok(app) = self.app().await else { return HashMap::new() };
        app.query(
            "select rel_path, document_id, base_seq, seen_mtime from folder_file where project_id = $1",
            &[&project_id],
        )
        .await
        .map(|rows| {
            rows.iter()
                .map(|r| {
                    (
                        r.get::<_, String>(0),
                        Known {
                            document_id: r.get::<_, String>(1),
                            base_seq: r.get::<_, i32>(2),
                            seen_mtime_ms: r.get::<_, i64>(3),
                        },
                    )
                })
                .collect()
        })
        .unwrap_or_default()
    }

    /// Records that a file of a folder is in step with a document at a revision.
    pub async fn note_file(
        &self,
        project_id: &str,
        rel_path: &str,
        document_id: &str,
        base_seq: i32,
        hash: &str,
        mtime_ms: i64,
    ) -> Result<(), String> {
        let app = self.app().await?;
        app.execute(
            "insert into folder_file (project_id, rel_path, document_id, base_seq, seen_hash, seen_mtime)
             values ($1,$2,$3,$4,$5,$6)
             on conflict (project_id, rel_path) do update set document_id = excluded.document_id,
               base_seq = excluded.base_seq, seen_hash = excluded.seen_hash, seen_mtime = excluded.seen_mtime",
            &[&project_id, &rel_path, &document_id, &base_seq, &hash, &mtime_ms],
        )
        .await
        .map(|_| ())
        .map_err(|_| "The file could not be recorded.".to_string())
    }

    /// One look at every bound folder (called every couple of seconds while the app runs).
    pub async fn watch_tick(&self, watcher: &Watcher, picks: &Picks) {
        if !self.is_ready() {
            return;
        }
        for (project_id, folder) in self.bound_folders().await {
            let listing = {
                let folder = folder.clone();
                tauri::async_runtime::spawn_blocking(move || watch::list_folder(&folder)).await
            };
            let Ok(files) = listing else { continue };
            let known = self.known_files(&project_id).await;
            watcher.scan(&project_id, files, &known, picks);
        }
    }
}
