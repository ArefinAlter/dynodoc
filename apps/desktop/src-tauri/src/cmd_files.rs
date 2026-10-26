//! Commands for the files and folders of the person: the system's open and save dialogs, reading a
//! chosen file, writing an exported copy and opening it in the program the system uses for it,
//! binding a project to a folder, and the folder watch. Same rules as `commands.rs`: only the
//! bundled launcher page, and the page never names a path (it passes tokens the app issued).

use crate::commands::require_launcher;
use crate::files::{self, Picks};
use crate::watch::{Change, Watcher};
use crate::AppState;
use base64::Engine as _;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{Manager, Webview};

#[cfg(test)]
pub const COMMANDS: [&str; 8] = [
    "files_pick",
    "file_read",
    "file_export",
    "folder_bind",
    "folder_unbind",
    "folder_status",
    "watch_poll",
    "watch_seen",
];

fn state_of(webview: &Webview) -> tauri::State<'_, AppState> {
    webview.app_handle().state::<AppState>()
}

#[derive(Serialize)]
pub struct Picked {
    token: String,
    name: String,
    size: u64,
    /// For showing to the person (a folder's address, a file's place).
    path: String,
}

fn picked(picks: &Picks, path: std::path::PathBuf) -> Option<Picked> {
    let meta = std::fs::metadata(&path).ok();
    let name = path.file_name()?.to_string_lossy().to_string();
    let display = path.display().to_string();
    Some(Picked {
        token: picks.remember(path)?,
        name,
        size: meta.map(|m| m.len()).unwrap_or(0),
        path: display,
    })
}

/// Debug builds only: `DYNODOC_TEST_PICK` names a JSON file `{ "import": [paths], "folder": [...], ... }`
/// that answers the dialogs without showing them, so the acceptance scripts can drive the real
/// window. Release builds do not contain this.
#[cfg(debug_assertions)]
fn test_pick(purpose: &str) -> Option<Vec<std::path::PathBuf>> {
    let file = std::env::var_os("DYNODOC_TEST_PICK")?;
    let text = std::fs::read_to_string(file).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let list = value.get(purpose)?.as_array()?;
    Some(
        list.iter()
            .filter_map(|p| p.as_str().map(std::path::PathBuf::from))
            .collect(),
    )
}

/// Shows the system's file dialog. `purpose` is `import` (several files), `folder`, `backup_open`
/// or `backup_save`. Returns what was chosen (nothing if the person cancelled).
#[tauri::command]
pub async fn files_pick(webview: Webview, purpose: String) -> Result<Vec<Picked>, String> {
    require_launcher(webview.url())?;
    #[cfg(debug_assertions)]
    if let Some(paths) = test_pick(&purpose) {
        let state = state_of(&webview);
        return Ok(paths
            .into_iter()
            .filter_map(|p| picked(&state.picks, p))
            .collect());
    }
    let kind = purpose.clone();
    let paths: Vec<std::path::PathBuf> =
        tauri::async_runtime::spawn_blocking(move || -> Vec<std::path::PathBuf> {
            let dialog = rfd::FileDialog::new();
            match kind.as_str() {
                "import" => dialog
                    .set_title("Add files to this project")
                    .add_filter("Word, Excel and PowerPoint", &["docx", "xlsx", "pptx"])
                    .add_filter("Text and tables", &["txt", "md", "csv"])
                    .pick_files()
                    .unwrap_or_default(),
                "folder" => dialog
                    .set_title("Choose the folder this project keeps its files in")
                    .pick_folder()
                    .into_iter()
                    .collect(),
                "backup_open" => dialog
                    .set_title("Choose a Dynodoc backup")
                    .add_filter("Dynodoc backup", &["zip"])
                    .pick_file()
                    .into_iter()
                    .collect(),
                "backup_save" => dialog
                    .set_title("Save a backup of this workspace")
                    .set_file_name("Dynodoc backup.zip")
                    .add_filter("Dynodoc backup", &["zip"])
                    .save_file()
                    .into_iter()
                    .collect(),
                _ => Vec::new(),
            }
        })
        .await
        .map_err(|_| "The file dialog stopped unexpectedly.".to_string())?;
    if !["import", "folder", "backup_open", "backup_save"].contains(&purpose.as_str()) {
        return Err("That is not something the client may ask for.".to_string());
    }
    let state = state_of(&webview);
    Ok(paths
        .into_iter()
        .filter_map(|p| picked(&state.picks, p))
        .collect())
}

#[derive(Serialize)]
pub struct FileBytes {
    name: String,
    size: u64,
    /// The file's bytes, base64.
    base64: String,
}

/// Reads a file the person chose (a token from `files_pick` or `watch_poll`). Office files and
/// text only, up to the engine's upload limit.
#[tauri::command]
pub async fn file_read(webview: Webview, token: String) -> Result<FileBytes, String> {
    require_launcher(webview.url())?;
    let path = state_of(&webview)
        .picks
        .path(&token)
        .ok_or_else(|| "That file is no longer available. Choose it again.".to_string())?;
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = files::extension_of(&name);
    if !["docx", "xlsx", "pptx", "txt", "md", "csv", "doc"].contains(&ext.as_str()) {
        return Err("Only Word, Excel, PowerPoint and text files can be read here.".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let meta = std::fs::symlink_metadata(&path)
            .map_err(|_| "The file could not be read.".to_string())?;
        if !meta.is_file() {
            return Err("That is not a file.".to_string());
        }
        if meta.len() > files::MAX_READ_BYTES {
            return Err("Files are limited to 3 MB (3,000,000 bytes) in this release.".to_string());
        }
        let bytes = std::fs::read(&path).map_err(|_| {
            "The file could not be read. Is it open in another program?".to_string()
        })?;
        Ok(FileBytes {
            name,
            size: meta.len(),
            base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        })
    })
    .await
    .map_err(|_| "The file could not be read.".to_string())?
}

#[derive(Serialize)]
pub struct Exported {
    /// Where the copy was written, for showing to the person.
    path: String,
    opened: bool,
}

/// Writes an exported copy of a document and (when asked) opens it in the program the system
/// uses for that kind of file. The copy goes to the project's folder if it has one, otherwise to
/// `Documents/Dynodoc/<project>/`. The name is the page's suggestion, made safe; an existing file
/// is never overwritten.
#[tauri::command]
pub async fn file_export(
    webview: Webview,
    project_id: String,
    project_name: String,
    name: String,
    ext: String,
    base64: String,
    open: bool,
    document_id: Option<String>,
    base_seq: Option<i32>,
) -> Result<Exported, String> {
    require_launcher(webview.url())?;
    if !files::OFFICE_EXTENSIONS.contains(&ext.as_str()) {
        return Err("Only Word, Excel and PowerPoint files can be written.".to_string());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64.as_bytes())
        .map_err(|_| "The file's content was not valid.".to_string())?;
    if bytes.len() > 40 * 1024 * 1024 {
        return Err("That file is too large to write.".to_string());
    }
    let state = state_of(&webview);
    let bound = state.local.folder_of(&project_id).await?;
    let dir = match bound.clone() {
        Some(dir) => dir,
        None => {
            let documents = webview
                .app_handle()
                .path()
                .document_dir()
                .map_err(|_| "There is no Documents folder to write to.".to_string())?;
            files::default_export_dir(&documents, &project_name)
        }
    };
    let stem = files::safe_file_name(&name);
    let path =
        tauri::async_runtime::spawn_blocking(move || -> Result<std::path::PathBuf, String> {
            std::fs::create_dir_all(&dir)
                .map_err(|_| "The folder could not be created.".to_string())?;
            let path = files::free_name(&dir, &stem, &ext);
            std::fs::write(&path, &bytes)
                .map_err(|_| "The file could not be written.".to_string())?;
            Ok(path)
        })
        .await
        .map_err(|_| "The file could not be written.".to_string())??;
    // A copy written into the project's own folder is in step with the revision it was made from:
    // the file watch reports only what is saved after this.
    if let (Some(folder), Some(doc), Some(seq)) = (bound, document_id.as_deref(), base_seq) {
        if let Some(rel) = path
            .strip_prefix(&folder)
            .ok()
            .map(|r| r.to_string_lossy().replace('\\', "/"))
        {
            if let Ok(bytes) = std::fs::read(&path) {
                let mtime = std::fs::metadata(&path)
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_millis() as i64)
                    .unwrap_or(0);
                let _ = state
                    .local
                    .note_file(
                        &project_id,
                        &rel,
                        doc,
                        seq,
                        &hex::encode(Sha256::digest(&bytes)),
                        mtime,
                    )
                    .await;
            }
        }
    }
    let mut opened = false;
    if open {
        use tauri_plugin_opener::OpenerExt;
        opened = webview
            .app_handle()
            .opener()
            .open_path(path.to_string_lossy().to_string(), None::<&str>)
            .is_ok();
    }
    Ok(Exported {
        path: path.display().to_string(),
        opened,
    })
}

/// Binds a project to a folder the person chose (a token from `files_pick("folder")`).
#[tauri::command]
pub async fn folder_bind(
    webview: Webview,
    project_id: String,
    token: String,
) -> Result<String, String> {
    require_launcher(webview.url())?;
    let state = state_of(&webview);
    let path = state
        .picks
        .path(&token)
        .ok_or_else(|| "Choose the folder again.".to_string())?;
    if !path.is_dir() {
        return Err("That is not a folder.".to_string());
    }
    state.local.bind_folder(&project_id, &path).await?;
    state.watcher.forget(&project_id);
    Ok(path.display().to_string())
}

#[tauri::command]
pub async fn folder_unbind(webview: Webview, project_id: String) -> Result<(), String> {
    require_launcher(webview.url())?;
    let state = state_of(&webview);
    state.local.unbind_folder(&project_id).await?;
    state.watcher.forget(&project_id);
    Ok(())
}

#[derive(Serialize)]
pub struct FolderStatus {
    path: String,
    exists: bool,
    files: usize,
}

#[tauri::command]
pub async fn folder_status(
    webview: Webview,
    project_id: String,
) -> Result<Option<FolderStatus>, String> {
    require_launcher(webview.url())?;
    let state = state_of(&webview);
    let Some(path) = state.local.folder_of(&project_id).await? else {
        return Ok(None);
    };
    let exists = path.is_dir();
    let files = if exists {
        crate::watch::list_folder(&path).len()
    } else {
        0
    };
    Ok(Some(FolderStatus {
        path: path.display().to_string(),
        exists,
        files,
    }))
}

/// The files of bound folders that are new or were saved since the project last saw them (each
/// reported once). The page reads each through its token, makes changes, and calls `watch_seen`.
#[tauri::command]
pub fn watch_poll(webview: Webview) -> Result<Vec<Change>, String> {
    require_launcher(webview.url())?;
    Ok(state_of(&webview).watcher.drain())
}

/// Records that a file of a bound folder is in step with a document at a revision: the file as it
/// is now (its time) is what the project has seen, so only a later save is reported again.
#[tauri::command]
pub async fn watch_seen(
    webview: Webview,
    project_id: String,
    rel_path: String,
    token: String,
    document_id: String,
    base_seq: i32,
) -> Result<(), String> {
    require_launcher(webview.url())?;
    let state = state_of(&webview);
    let path = state
        .picks
        .path(&token)
        .ok_or_else(|| "That file is no longer available.".to_string())?;
    let (hash, mtime) =
        tauri::async_runtime::spawn_blocking(move || -> Result<(String, i64), String> {
            let bytes =
                std::fs::read(&path).map_err(|_| "The file could not be read.".to_string())?;
            let mtime = std::fs::metadata(&path)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            Ok((hex::encode(Sha256::digest(&bytes)), mtime))
        })
        .await
        .map_err(|_| "The file could not be read.".to_string())??;
    state
        .local
        .note_file(&project_id, &rel_path, &document_id, base_seq, &hash, mtime)
        .await
}

/// Runs the watch loop for the life of the app.
pub fn spawn_watch_loop(app: &tauri::AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            let state = app.state::<AppState>();
            if state.local.is_closing() {
                return;
            }
            let watcher: &Watcher = &state.watcher;
            state.local.watch_tick(watcher, &state.picks).await;
        }
    });
}
