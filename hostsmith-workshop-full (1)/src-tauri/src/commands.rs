//! The complete command surface of HostSmith Workshop (thin adapter).
//!
//! Every command delegates to a headless, tested crate function; no business
//! logic lives here. Editing commands follow the pipeline of review 6.3:
//! propose (returns a diff preview) -> the user confirms -> apply (backs up,
//! writes, journals). Cloudflare commands emit `deploy-progress` events.

use crate::state::{project_dir_name, AppState, ProjectSession};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use hsw_cloudflare::pages::{CloudflareClient, DeployOutcome, DeployStep};
use hsw_core::backup::{self, BackupInfo};
use hsw_core::persist::{JournalOutcome, PersistedFile, PersistentJournal};
use hsw_core::project::{scan_project, ProjectScan};
use hsw_core::{fsutil, naming, watcher};
use hsw_edit::css;
use hsw_edit::diff;
use hsw_edit::import::{self, ImportResult};
use hsw_edit::rewrite::{self, ElementEdit};
use hsw_edit::diff::DiffSummary;
use hsw_favicon::detect::FaviconDetection;
use hsw_favicon::generate::GeneratedFavicons;
use hsw_html::queries::{element_class_lists, summarize};
use hsw_imaging::ops::{self, ImageInfo, OpResult, OutputFormat};
use hsw_seo::checks::Analysis;
use hsw_tools::palette::PaletteEntry;
use hsw_validators::structure::{BadgeSignature, CheckResult, Profile};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use tauri::Emitter;

// ---------------------------------------------------------------- helpers

fn with_session<T>(
    state: &AppState,
    function: impl FnOnce(&ProjectSession) -> Result<T, String>,
) -> Result<T, String> {
    let guard = state.session.lock().map_err(|_| "project state is corrupted".to_string())?;
    match guard.as_ref() {
        Some(session) => function(session),
        None => Err("no project is open".to_string()),
    }
}

fn with_session_mut<T>(
    state: &AppState,
    function: impl FnOnce(&mut ProjectSession) -> Result<T, String>,
) -> Result<T, String> {
    let mut guard = state.session.lock().map_err(|_| "project state is corrupted".to_string())?;
    match guard.as_mut() {
        Some(session) => function(session),
        None => Err("no project is open".to_string()),
    }
}

fn resolve_in_project(session: &ProjectSession, rel: &str) -> Result<PathBuf, String> {
    fsutil::safe_join(&session.root, Path::new(rel)).map_err(|error| format!("{rel}: {error}"))
}

fn read_text(session: &ProjectSession, rel: &str) -> Result<String, String> {
    let path = resolve_in_project(session, rel)?;
    std::fs::read_to_string(path).map_err(|error| format!("{rel}: {error}"))
}

fn token_store() -> Arc<dyn hsw_cloudflare::store::TokenStore> {
    static STORE: OnceLock<Arc<dyn hsw_cloudflare::store::TokenStore>> = OnceLock::new();
    STORE.get_or_init(hsw_cloudflare::store::default_store).clone()
}

fn cf_client() -> Result<CloudflareClient, String> {
    let token = token_store()
        .load("cloudflare-api-token")
        .map_err(|error| error.to_string())?
        .ok_or("no Cloudflare token stored - connect first in the Upload panel")?;
    Ok(CloudflareClient::new(token))
}

// ---------------------------------------------------------------- dialogs

#[tauri::command]
pub async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |path| {
        let _ = sender.send(path.map(|p| p.to_string()));
    });
    receiver.await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn pick_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_file(move |path| {
        let _ = sender.send(path.map(|p| p.to_string()));
    });
    receiver.await.map_err(|error| error.to_string())
}

// ---------------------------------------------------------------- project

#[derive(Serialize)]
pub struct ProjectInfo {
    pub root: String,
    pub scan: ProjectScan,
}

#[tauri::command]
pub fn open_project(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<ProjectInfo, String> {
    let root = PathBuf::from(&path);
    if !root.is_dir() {
        return Err(format!("not a folder: {path}"));
    }
    close_project(state.clone())?;

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?
        .join("projects")
        .join(project_dir_name(&root));
    std::fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
    let scan = scan_project(&root).map_err(|error| error.to_string())?;

    let suppress = Arc::new(std::sync::Mutex::new(std::collections::HashSet::new()));
    let journal_file = app_data_dir.join("journal.json");
    let watcher_handle = {
        let suppress = suppress.clone();
        let journal_file = journal_file.clone();
        let emitter = app.clone();
        watcher::watch_project(&root, std::time::Duration::from_millis(400), move |paths| {
            let mut guard = suppress.lock().expect("suppress mutex");
            let changed: Vec<String> = paths
                .iter()
                .map(|path| fsutil::slash_separated(path))
                .filter(|rel| !guard.remove(rel))
                .collect();
            drop(guard);
            if changed.is_empty() {
                return;
            }
            let mut journal = PersistentJournal::load(&journal_file);
            for rel in &changed {
                let _ = journal.invalidate_path(rel);
            }
            let _ = emitter.emit("fs-changed", changed);
        })
        .map_err(|error| error.to_string())?
    };

    *state
        .session
        .lock()
        .map_err(|_| "project state is corrupted".to_string())? = Some(ProjectSession {
        root,
        app_data_dir,
        server: None,
        watcher: Some(watcher_handle),
        suppress,
    });
    Ok(ProjectInfo { root: path, scan })
}

#[tauri::command]
pub fn close_project(state: tauri::State<'_, AppState>) -> Result<(), String> {
    with_session_mut(&state, |session| {
        if let Some(server) = session.server.take() {
            server.stop();
        }
        if let Some(handle) = session.watcher.take() {
            handle.stop();
        }
        Ok(())
    })?;
    *state
        .session
        .lock()
        .map_err(|_| "project state is corrupted".to_string())? = None;
    Ok(())
}

#[tauri::command]
pub fn rescan_project(state: tauri::State<'_, AppState>) -> Result<ProjectInfo, String> {
    with_session(&state, |session| {
        let scan = scan_project(&session.root).map_err(|error| error.to_string())?;
        Ok(ProjectInfo { root: session.root.to_string_lossy().to_string(), scan })
    })
}

#[tauri::command]
pub fn import_zip(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    zip_path: String,
    folder_name: String,
) -> Result<ProjectInfo, String> {
    let desktop = app.path().desktop_dir().map_err(|error| error.to_string())?;
    let safe_name = naming::slugify_stem(&folder_name);
    if safe_name.is_empty() {
        return Err("the folder name produced no safe slug".to_string());
    }
    let destination = desktop.join(&safe_name);
    let not_empty = std::fs::read_dir(&destination)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false);
    if not_empty {
        return Err(format!(
            "the destination folder \"{safe_name}\" already exists on the Desktop and is not empty - choose a different name"
        ));
    }
    std::fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
    let file = std::fs::File::open(&zip_path).map_err(|error| error.to_string())?;
    hsw_core::zipguard::extract_zip(file, &destination).map_err(|error| error.to_string())?;
    open_project(app, state, destination.to_string_lossy().to_string())
}

#[derive(Serialize)]
pub struct DirEntry {
    pub name: String,
    pub rel: String,
    pub is_dir: bool,
    pub size: u64,
}

#[tauri::command]
pub fn list_project_dir(
    state: tauri::State<'_, AppState>,
    rel: String,
) -> Result<Vec<DirEntry>, String> {
    with_session(&state, |session| {
        let directory = resolve_in_project(session, &rel)?;
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&directory).map_err(|error| error.to_string())? {
            let entry = entry.map_err(|error| error.to_string())?;
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let metadata = entry.metadata().map_err(|error| error.to_string())?;
            let child_rel = if rel.is_empty() || rel == "." {
                name.clone()
            } else {
                format!("{}/{}", rel.trim_end_matches('/'), name)
            };
            out.push(DirEntry {
                name,
                rel: child_rel,
                is_dir: metadata.is_dir(),
                size: metadata.len(),
            });
        }
        out.sort_by(|a, b| {
            b.is_dir
                .cmp(&a.is_dir)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });
        Ok(out)
    })
}

#[tauri::command]
pub fn read_text_file(state: tauri::State<'_, AppState>, rel: String) -> Result<String, String> {
    with_session(&state, |session| read_text(session, &rel))
}

#[tauri::command]
pub fn read_binary_file_base64(
    state: tauri::State<'_, AppState>,
    rel: String,
) -> Result<String, String> {
    with_session(&state, |session| {
        let path = resolve_in_project(session, &rel)?;
        let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
        Ok(B64.encode(bytes))
    })
}

// ---------------------------------------------------------------- preview

#[tauri::command]
pub fn start_preview(state: tauri::State<'_, AppState>) -> Result<String, String> {
    with_session_mut(&state, |session| {
        if let Some(server) = &session.server {
            return Ok(server.info().url.clone());
        }
        let server = hsw_server::server::PreviewServer::start(&session.root)
            .map_err(|error| error.to_string())?;
        let url = server.info().url.clone();
        session.server = Some(server);
        Ok(url)
    })
}

#[tauri::command]
pub fn stop_preview(state: tauri::State<'_, AppState>) -> Result<(), String> {
    with_session_mut(&state, |session| {
        if let Some(server) = session.server.take() {
            server.stop();
        }
        Ok(())
    })
}

// ------------------------------------------------------------ edit pipeline

#[derive(Serialize)]
pub struct EditPreview {
    pub path: String,
    pub new_content: String,
    pub diff: String,
    pub added: usize,
    pub removed: usize,
}

fn preview_from(path: &str, before: &str, after: String) -> EditPreview {
    let DiffSummary { added, removed } = diff::summarize(before, &after);
    EditPreview {
        path: path.to_string(),
        diff: diff::unified_diff(before, &after, 3),
        new_content: after,
        added,
        removed,
    }
}

#[tauri::command]
pub fn propose_text_edit(
    state: tauri::State<'_, AppState>,
    path: String,
    element_index: usize,
    ordinal: usize,
    replacement: String,
) -> Result<EditPreview, String> {
    with_session(&state, |session| {
        let html = read_text(session, &path)?;
        let new_content = rewrite::replace_text_node(&html, element_index, ordinal, &replacement)
            .map_err(|error| error.to_string())?;
        Ok(preview_from(&path, &html, new_content))
    })
}

#[derive(serde::Deserialize)]
pub struct AttributeEditPayload {
    pub name: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub remove: bool,
}

#[tauri::command]
pub fn propose_attribute_edit(
    state: tauri::State<'_, AppState>,
    path: String,
    element_index: usize,
    edit: AttributeEditPayload,
) -> Result<EditPreview, String> {
    with_session(&state, |session| {
        let html = read_text(session, &path)?;
        let element_edit = if edit.remove {
            ElementEdit::RemoveAttribute { name: edit.name }
        } else {
            ElementEdit::SetAttribute { name: edit.name, value: edit.value }
        };
        let new_content = rewrite::edit_element(&html, element_index, element_edit)
            .map_err(|error| error.to_string())?;
        Ok(preview_from(&path, &html, new_content))
    })
}

fn rewrite_srcset(srcset: &str, page_dir: &str, old_asset_rel: &str, new_asset_rel: &str) -> String {
    srcset
        .split(',')
        .map(|entry| {
            let trimmed = entry.trim();
            let mut parts = trimmed.split_whitespace();
            let url = parts.next().unwrap_or("");
            let rest: Vec<&str> = parts.collect();
            let resolved = fsutil::resolve_reference(page_dir, url);
            if resolved == old_asset_rel {
                let new_url = fsutil::relative_reference(page_dir, new_asset_rel);
                if rest.is_empty() {
                    new_url
                } else {
                    format!("{new_url} {}", rest.join(" "))
                }
            } else {
                trimmed.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[derive(Serialize)]
pub struct MediaSwapPreview {
    pub files: Vec<EditPreview>,
}

#[tauri::command]
pub fn propose_media_swap(
    state: tauri::State<'_, AppState>,
    page_path: String,
    page_dir: String,
    element_index: usize,
    attribute: String,
    new_asset_rel: String,
    old_asset_rel: String,
    srcset: Option<String>,
) -> Result<MediaSwapPreview, String> {
    with_session(&state, |session| {
        let scan = scan_project(&session.root).map_err(|error| error.to_string())?;
        let html = read_text(session, &page_path)?;
        let new_reference = fsutil::relative_reference(&page_dir, &new_asset_rel);

        let mut new_html = rewrite::edit_element(
            &html,
            element_index,
            ElementEdit::SetAttribute { name: attribute.clone(), value: new_reference.clone() },
        )
        .map_err(|error| error.to_string())?;
        if let Some(srcset_value) = &srcset {
            let rewritten = rewrite_srcset(srcset_value, &page_dir, &old_asset_rel, &new_asset_rel);
            if rewritten != *srcset_value {
                new_html = rewrite::edit_element(
                    &new_html,
                    element_index,
                    ElementEdit::SetAttribute { name: "srcset".to_string(), value: rewritten },
                )
                .map_err(|error| error.to_string())?;
            }
        }
        let mut files = vec![preview_from(&page_path, &html, new_html)];

        // Stylesheets that reference the old asset keep their reference
        // style (site-absolute stays site-absolute).
        for asset in &scan.asset_files {
            let asset_text = fsutil::slash_separated(asset);
            if !asset_text.to_ascii_lowercase().ends_with(".css") {
                continue;
            }
            let css_dir = asset_text
                .rsplit_once('/')
                .map(|(dir, _)| dir.to_string())
                .unwrap_or_default();
            let css_path = resolve_in_project(session, &asset_text)?;
            let content = std::fs::read_to_string(&css_path)
                .map_err(|error| format!("{asset_text}: {error}"))?;
            let updated =
                css::update_references_to_target(&content, &css_dir, &old_asset_rel, &new_asset_rel);
            if updated != content {
                files.push(preview_from(&asset_text, &content, updated));
            }
        }
        Ok(MediaSwapPreview { files })
    })
}

#[derive(serde::Deserialize)]
pub struct EditFilePayload {
    pub path: String,
    pub new_content: String,
}

#[derive(Serialize)]
pub struct AppliedResult {
    pub backup: String,
    pub files: Vec<String>,
}

/// Applies one confirmed operation: backs up the affected files, writes the
/// new contents and records one journal entry covering all of them.
#[tauri::command]
pub fn apply_edited_files(
    state: tauri::State<'_, AppState>,
    files: Vec<EditFilePayload>,
    description: String,
) -> Result<AppliedResult, String> {
    with_session_mut(&state, |session| {
        let mut journal_files = Vec::new();
        let mut writes = Vec::new();
        let mut backup_paths = Vec::new();
        for payload in &files {
            let abs = resolve_in_project(session, &payload.path)?;
            let old_bytes = std::fs::read(&abs).map_err(|error| format!("{}: {error}", payload.path))?;
            let old_text =
                String::from_utf8(old_bytes).map_err(|_| format!("{} is not a text file", payload.path))?;
            journal_files.push(PersistedFile::text(
                payload.path.clone(),
                &old_text,
                &payload.new_content,
            ));
            backup_paths.push(PathBuf::from(&payload.path));
            writes.push((payload.path.clone(), abs, payload.new_content.clone().into_bytes()));
        }

        let backup =
            backup::create_backup(&session.root, &backup_paths, &session.backups_dir())
                .map_err(|error| error.to_string())?;

        for (rel, abs, new_bytes) in &writes {
            session.suppress_path(rel);
            std::fs::write(abs, new_bytes).map_err(|error| format!("{rel}: {error}"))?;
        }
        let mut journal = session.journal();
        journal
            .record(&description, journal_files)
            .map_err(|error| error.to_string())?;
        let _ = backup::trim_backups(&session.backups_dir(), 50);

        Ok(AppliedResult {
            backup: backup.to_string_lossy().to_string(),
            files: writes.into_iter().map(|(rel, _, _)| rel).collect(),
        })
    })
}

#[tauri::command]
pub fn import_media(
    state: tauri::State<'_, AppState>,
    source_path: String,
    original_name: String,
) -> Result<ImportResult, String> {
    with_session(&state, |session| {
        let bytes = std::fs::read(&source_path).map_err(|error| error.to_string())?;
        import::import_asset(&session.root, &original_name, &bytes).map_err(|error| error.to_string())
    })
}

#[derive(Serialize)]
pub struct JournalStepOutcome {
    pub description: String,
    pub files: Vec<String>,
    pub skipped: Vec<String>,
}

fn serialize_outcome(outcome: JournalOutcome, root: &Path) -> JournalStepOutcome {
    JournalStepOutcome {
        description: outcome.description,
        files: outcome
            .files_restored
            .iter()
            .filter_map(|path| path.strip_prefix(root).ok())
            .map(|rel| rel.to_string_lossy().replace('\\', "/"))
            .collect(),
        skipped: outcome.skipped,
    }
}

#[tauri::command]
pub fn undo_step(
    state: tauri::State<'_, AppState>,
    redo: bool,
) -> Result<Option<JournalStepOutcome>, String> {
    with_session_mut(&state, |session| {
        let mut journal = session.journal();
        let outcome = journal.step(&session.root, redo).map_err(|error| error.to_string())?;
        let Ok(root) = session.root.canonicalize() else {
            return Ok(outcome.map(|outcome| serialize_outcome(outcome, &session.root)));
        };
        if let Some(outcome) = &outcome {
            for path in &outcome.files_restored {
                if let Ok(rel) = path.strip_prefix(&root) {
                    session.suppress_path(&rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        Ok(outcome.map(|outcome| serialize_outcome(outcome, &root)))
    })
}

#[tauri::command]
pub fn journal_entries(state: tauri::State<'_, AppState>) -> Result<Vec<hsw_core::persist::PersistedEntry>, String> {
    with_session(&state, |session| {
        Ok(session.journal().entries().to_vec())
    })
}

// ---------------------------------------------------------------- backups

#[tauri::command]
pub fn list_backups(state: tauri::State<'_, AppState>) -> Result<Vec<BackupInfo>, String> {
    with_session(&state, |session| {
        backup::list_backups(&session.backups_dir()).map_err(|error| error.to_string())
    })
}

#[tauri::command]
pub fn restore_backup(
    state: tauri::State<'_, AppState>,
    backup_path: String,
    only: Option<Vec<String>>,
) -> Result<usize, String> {
    with_session_mut(&state, |session| {
        let restored = backup::restore_backup(&session.root, Path::new(&backup_path), only.as_deref())
            .map_err(|error| error.to_string())?;
        if let Ok(entries) = backup::list_backups(&session.backups_dir()) {
            if let Some(info) = entries.iter().find(|info| info.path.to_string_lossy() == backup_path) {
                for file in &info.files {
                    session.suppress_path(file);
                }
            }
        }
        // Restores change content under the journal's feet: clear it so no
        // undo can revert onto the restored state unknowingly.
        let mut journal = session.journal();
        let _ = journal.clear();
        Ok(restored)
    })
}

// ---------------------------------------------------------------- seo

#[tauri::command]
pub fn analyze_seo(state: tauri::State<'_, AppState>) -> Result<Analysis, String> {
    with_session(&state, |session| {
        let scan = scan_project(&session.root).map_err(|error| error.to_string())?;
        Ok(hsw_seo::checks::analyze_project(&session.root, &scan))
    })
}

#[tauri::command]
pub fn plan_seo_fix(
    state: tauri::State<'_, AppState>,
    page_path: String,
    kind: String,
    value: Option<String>,
    lang: Option<String>,
) -> Result<EditPreview, String> {
    with_session(&state, |session| {
        let html = read_text(session, &page_path)?;
        let new_content = match kind.as_str() {
            "viewport" => hsw_seo::fixes::fix_viewport(&html),
            "lang" => hsw_seo::fixes::fix_lang(&html, lang.as_deref().unwrap_or("en")),
            "title" => hsw_seo::fixes::fix_title(
                &html,
                value.as_deref().ok_or("a title value is required for this fix")?,
            ),
            "description" => hsw_seo::fixes::fix_description(
                &html,
                value.as_deref().ok_or("a description value is required for this fix")?,
            ),
            "canonical" => hsw_seo::fixes::fix_canonical(
                &html,
                value.as_deref().ok_or("a canonical URL is required for this fix")?,
            ),
            other => return Err(format!("unknown fix kind: {other}")),
        }
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "nothing to fix - the page already satisfies this check".to_string())?;
        Ok(preview_from(&page_path, &html, new_content))
    })
}

// ---------------------------------------------------------------- favicon

#[tauri::command]
pub fn favicon_detect(
    state: tauri::State<'_, AppState>,
    page_path: String,
    page_dir: Option<String>,
) -> Result<FaviconDetection, String> {
    with_session(&state, |session| {
        let html = read_text(session, &page_path)?;
        let dir = page_dir.unwrap_or_default();
        Ok(hsw_favicon::detect::detect(&session.root, &html, &dir))
    })
}

#[tauri::command]
pub fn favicon_generate(
    state: tauri::State<'_, AppState>,
    source_path: String,
) -> Result<GeneratedFavicons, String> {
    with_session(&state, |session| {
        hsw_favicon::generate::generate(&session.root, Path::new(&source_path))
            .map_err(|error| error.to_string())
    })
}

// ---------------------------------------------------------------- validators

#[tauri::command]
pub fn validate_structure(
    state: tauri::State<'_, AppState>,
    profile: Profile,
    required_classes: Vec<String>,
) -> Result<Vec<CheckResult>, String> {
    with_session(&state, |session| {
        let scan = scan_project(&session.root).map_err(|error| error.to_string())?;
        let badge = BadgeSignature { required_classes };
        Ok(hsw_validators::structure::validate(&session.root, &scan, profile, &badge))
    })
}

// ---------------------------------------------------------------- toolbox

#[tauri::command]
pub fn tool_format_json(text: String) -> Result<String, String> {
    hsw_tools::minify::format_json(&text).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_minify_json(text: String) -> Result<String, String> {
    hsw_tools::minify::minify_json(&text).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_validate_json(text: String) -> Result<hsw_tools::minify::JsonDiagnostic, String> {
    match hsw_tools::minify::validate_json(&text) {
        Ok(()) => Ok(hsw_tools::minify::JsonDiagnostic {
            line: 0,
            column: 0,
            message: "valid JSON".to_string(),
        }),
        Err(diagnostic) => Ok(diagnostic),
    }
}

#[tauri::command]
pub fn tool_minify_html(text: String) -> String {
    hsw_tools::minify::minify_html_str(&text)
}

#[tauri::command]
pub fn tool_minify_css(text: String) -> String {
    hsw_tools::minify::minify_css_str(&text)
}

#[tauri::command]
pub fn tool_minify_js(text: String) -> Result<String, String> {
    hsw_tools::minify::minify_js_str(&text).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_qr_svg(text: String, dark: String, light: String, width: usize) -> Result<String, String> {
    hsw_tools::qr::qr_svg(&text, &dark, &light, width).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_palette(source_path: String, max_colors: usize) -> Result<Vec<PaletteEntry>, String> {
    let bytes = std::fs::read(&source_path).map_err(|error| error.to_string())?;
    hsw_tools::palette::extract_palette(&bytes, max_colors).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_image_info(source_path: String) -> Result<ImageInfo, String> {
    ops::inspect(Path::new(&source_path)).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_image_resize(
    source_path: String,
    dest_path: String,
    max_width: u32,
    max_height: u32,
    format: OutputFormat,
) -> Result<OpResult, String> {
    ops::resize(Path::new(&source_path), Path::new(&dest_path), max_width, max_height, format)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_image_crop(
    source_path: String,
    dest_path: String,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    format: OutputFormat,
) -> Result<OpResult, String> {
    ops::crop(Path::new(&source_path), Path::new(&dest_path), x, y, width, height, format)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_image_rotate(
    source_path: String,
    dest_path: String,
    quarter_turns: u32,
    format: OutputFormat,
) -> Result<OpResult, String> {
    ops::rotate(Path::new(&source_path), Path::new(&dest_path), quarter_turns, format)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_image_convert(
    source_path: String,
    dest_path: String,
    format: OutputFormat,
    jpeg_quality: u8,
) -> Result<OpResult, String> {
    ops::convert(Path::new(&source_path), Path::new(&dest_path), format, jpeg_quality)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_png_optimize(source_path: String, dest_path: String) -> Result<OpResult, String> {
    ops::optimize_png(Path::new(&source_path), Path::new(&dest_path)).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn tool_jpeg_optimize(
    source_path: String,
    dest_path: String,
    quality: u8,
) -> Result<OpResult, String> {
    ops::optimize_jpeg(Path::new(&source_path), Path::new(&dest_path), quality)
        .map_err(|error| error.to_string())
}

// ---------------------------------------------------------------- settings

#[derive(Serialize, serde::Deserialize, Clone, Default)]
pub struct AppConfig {
    #[serde(default)]
    pub badge_required_classes: Vec<String>,
    /// The exact badge markup supplied by the project owner, stored and
    /// applied verbatim - never invented by the application (PR-08).
    #[serde(default)]
    pub badge_snippet: Option<String>,
    #[serde(default = "default_lang")]
    pub seo_default_lang: String,
    #[serde(default = "default_retention")]
    pub backup_retention: u32,
}

fn default_lang() -> String {
    "en".to_string()
}

fn default_retention() -> u32 {
    50
}

fn config_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_data_dir().map_err(|error| error.to_string())?.join("config.json"))
}

fn read_config(app: &tauri::AppHandle) -> AppConfig {
    config_path(app)
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

fn write_config(app: &tauri::AppHandle, config: &AppConfig) -> Result<(), String> {
    let path = config_path(app)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(config).map_err(|error| error.to_string())?;
    std::fs::write(path, text).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn get_settings(app: tauri::AppHandle) -> Result<AppConfig, String> {
    Ok(read_config(&app))
}

#[tauri::command]
pub fn set_badge_signature(
    app: tauri::AppHandle,
    required_classes: Vec<String>,
) -> Result<(), String> {
    let mut config = read_config(&app);
    config.badge_required_classes = required_classes;
    write_config(&app, &config)
}

#[tauri::command]
pub fn set_badge_snippet(app: tauri::AppHandle, snippet: String) -> Result<(), String> {
    let trimmed = snippet.trim().to_string();
    if trimmed.is_empty() {
        return Err("the badge snippet is empty".to_string());
    }
    if !trimmed.contains("hs-") {
        return Err("the badge snippet must use the supplied hs- class names; a redesign is not accepted".to_string());
    }
    let mut config = read_config(&app);
    config.badge_snippet = Some(trimmed);
    write_config(&app, &config)
}

/// Inserts the exact supplied badge snippet before </body> on a page that
/// does not carry it yet. The snippet is applied byte-for-byte as supplied.
#[tauri::command]
pub fn apply_badge(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
    page_path: String,
) -> Result<EditPreview, String> {
    let config = read_config(&app);
    let Some(snippet) = config.badge_snippet.clone() else {
        return Err("the exact badge markup is still pending (PR-08); supply it in Settings first".to_string());
    };
    with_session(&state, |session| {
        let html = read_text(session, &page_path)?;
        let lists = element_class_lists(&html);
        let present = if config.badge_required_classes.is_empty() {
            lists.iter().any(|list| list.iter().any(|class| class.starts_with("hs-")))
        } else {
            lists.iter().any(|list| {
                config.badge_required_classes.iter().all(|required| list.contains(required))
            })
        };
        if present {
            return Err("the badge signature is already present on this page".to_string());
        }
        let new_content = rewrite::append_to_body_end(&html, &snippet).map_err(|error| error.to_string())?;
        Ok(preview_from(&page_path, &html, new_content))
    })
}

// ---------------------------------------------------------------- cloudflare

#[tauri::command]
pub fn cf_save_token(app: tauri::AppHandle, token: String) -> Result<(), String> {
    let trimmed = token.trim().to_string();
    if trimmed.is_empty() {
        return Err("the token is empty".to_string());
    }
    if trimmed.contains(':') {
        return Err("this looks like a Global API Key - HostSmith only accepts scoped API tokens".to_string());
    }
    token_store()
        .save("cloudflare-api-token", &trimmed)
        .map_err(|error| error.to_string())?;
    let _ = app; // reserved for future status events
    Ok(())
}

#[tauri::command]
pub fn cf_has_token() -> Result<bool, String> {
    Ok(token_store()
        .load("cloudflare-api-token")
        .map_err(|error| error.to_string())?
        .is_some())
}

#[tauri::command]
pub fn cf_disconnect() -> Result<(), String> {
    token_store()
        .delete("cloudflare-api-token")
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn cf_accounts() -> Result<Vec<hsw_cloudflare::pages::Account>, String> {
    cf_client()?.accounts().await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn cf_pages_projects(
    account: String,
) -> Result<Vec<hsw_cloudflare::pages::PagesProject>, String> {
    cf_client()?.pages_projects(&account).await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn cf_create_pages_project(
    account: String,
    name: String,
    production_branch: String,
) -> Result<hsw_cloudflare::pages::PagesProject, String> {
    cf_client()?
        .create_pages_project(&account, &name, &production_branch)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn cf_deploy_pages(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    account: String,
    project: String,
) -> Result<DeployOutcome, String> {
    let client = cf_client()?;
    let root = with_session(&state, |session| Ok(session.root.clone()))?;
    let emitter = app.clone();
    let progress = move |step: DeployStep, done: usize, total: usize| {
        let _ = emitter.emit("deploy-progress", serde_json::json!({ "step": step, "done": done, "total": total }));
    };
    client
        .deploy_directory(&account, &project, &root, &progress)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn cf_workers_scripts(
    account: String,
) -> Result<Vec<hsw_cloudflare::workers::WorkerScript>, String> {
    cf_client()?.workers_scripts(&account).await.map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn cf_upload_worker(
    account: String,
    name: String,
    main_module: String,
    code: String,
    bindings_json: String,
) -> Result<(), String> {
    cf_client()?
        .upload_worker_script(&account, &name, &main_module, &code, &bindings_json)
        .await
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn cf_delete_worker(
    account: String,
    name: String,
    confirm: bool,
) -> Result<(), String> {
    if !confirm {
        return Err("deletion requires the explicit confirmation flag".to_string());
    }
    cf_client()?.delete_worker_script(&account, &name).await.map_err(|error| error.to_string())
}
