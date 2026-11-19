//! Balance Studio: commands are thin, the logic is in `balance-kit` (shared with `cargo xtask balance patch`).
//! The app reads a balance kit and, from M2 on, writes a patch file. It has no network access and starts no processes.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use balance_kit::{draft_fits, image_type, import, patch_file_name, BulkSpec, KitError, KitView, Limits, Patch, Query};
use serde::{Deserialize, Serialize};
use serde_json::Value as Json;
use tauri::http::{header, Response, StatusCode};
use tauri::{AppHandle, Manager, State};
use tauri_plugin_dialog::DialogExt;

const MAX_RECENT: usize = 8;
const MAX_PAGE: usize = 500;
/// A patch is a list of changes; anything larger is not a patch
const MAX_PATCH_BYTES: u64 = 32 << 20;

#[derive(Clone, Debug, Serialize)]
pub struct Failure {
    code: &'static str,
    message: String,
}

impl From<KitError> for Failure {
    fn from(e: KitError) -> Self {
        Failure { code: e.code, message: e.message }
    }
}

fn fail(code: &'static str, message: impl Into<String>) -> Failure {
    Failure { code, message: message.into() }
}

type Result<T> = std::result::Result<T, Failure>;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Recent {
    version: String,
    title: String,
    season: Option<String>,
    source: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    lang: String,
    author: String,
    recent: Vec<Recent>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { lang: "en".into(), author: String::new(), recent: Vec::new() }
    }
}

pub struct App {
    data: PathBuf,
    settings: Mutex<Settings>,
    kit: Mutex<Option<Arc<KitView>>>,
}

/// `%LOCALAPPDATA%\BalanceStudio`; `BALANCE_STUDIO_DATA` — another folder, for tests.
fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("BALANCE_STUDIO_DATA") {
        return PathBuf::from(p);
    }
    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_else(|| ".".into())).join("BalanceStudio")
}

impl App {
    fn load() -> App {
        let data = data_dir();
        let settings = std::fs::read(data.join("settings.json")).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or_default();
        App { data, settings: Mutex::new(settings), kit: Mutex::new(None) }
    }

    fn kits_dir(&self) -> PathBuf {
        self.data.join("kits")
    }

    /// One draft per kit version; the version was checked when the kit was imported, so it is a safe file name.
    fn draft_path(&self, kit: &KitView) -> PathBuf {
        self.data.join("drafts").join(format!("{}.json", kit.kit.version()))
    }

    /// Called after every change: closing the app or a crash loses nothing. An empty draft leaves no file.
    fn save_draft(&self, kit: &KitView) -> Result<()> {
        let path = self.draft_path(kit);
        let draft = kit.draft_json();
        if draft["changes"].as_array().is_none_or(|c| c.is_empty()) && draft["comment"].as_str().is_none_or(str::is_empty) {
            let _ = std::fs::remove_file(&path);
            return Ok(());
        }
        let text = serde_json::to_vec_pretty(&draft).map_err(|e| fail("draft_io", e.to_string()))?;
        write_atomic(&path, &text).map_err(|e| fail("draft_io", e.to_string()))
    }

    fn save_settings(&self) {
        let text = serde_json::to_vec_pretty(&*self.settings.lock().unwrap()).unwrap_or_default();
        let _ = std::fs::create_dir_all(&self.data);
        let tmp = self.data.join("settings.json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, self.data.join("settings.json"));
        }
    }

    /// The newest draft of the same season from an older kit, not transferred yet
    fn rebase_source(&self, kit: &KitView) -> Option<(PathBuf, Json)> {
        let dir = std::fs::read_dir(self.data.join("drafts")).ok()?;
        let mut best: Option<(std::time::SystemTime, PathBuf, Json)> = None;
        for e in dir.flatten() {
            let path = e.path();
            if path.extension().is_none_or(|x| x != "json") {
                continue;
            }
            let Some(draft) = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Json>(&b).ok()) else { continue };
            if !draft_fits(&draft, kit.kit.version(), kit.kit.season()) {
                continue;
            }
            let time = e.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
            if best.as_ref().is_none_or(|b| time > b.0) {
                best = Some((time, path, draft));
            }
        }
        best.map(|(_, p, d)| (p, d))
    }

    fn current(&self) -> Result<Arc<KitView>> {
        self.kit.lock().unwrap().clone().ok_or(fail("no_kit", "no kit is open"))
    }

    /// Opens an already imported kit, puts it first in the recent list, drops copies no longer listed.
    fn activate(&self, root: &Path, source: &str) -> Result<Json> {
        let view = KitView::load(root).map_err(|e| fail("kit_invalid", e))?;
        let en = self.settings.lock().unwrap().lang == "en";
        let overview = view.overview_in(en);
        let entry = Recent {
            version: view.kit.version().to_string(),
            title: overview["title"].as_str().unwrap_or_default().to_string(),
            season: view.kit.season().map(str::to_string),
            source: source.to_string(),
        };
        {
            let mut s = self.settings.lock().unwrap();
            s.recent.retain(|r| r.version != entry.version);
            s.recent.insert(0, entry);
            s.recent.truncate(MAX_RECENT);
            if let Ok(dir) = std::fs::read_dir(self.kits_dir()) {
                for e in dir.flatten() {
                    let name = e.file_name().to_string_lossy().into_owned();
                    if !name.starts_with('.') && !s.recent.iter().any(|r| r.version == name) {
                        let _ = std::fs::remove_dir_all(e.path());
                    }
                }
            }
        }
        self.save_settings();
        let mut dropped = 0;
        if let Ok(bytes) = std::fs::read(self.draft_path(&view)) {
            if let Ok(draft) = serde_json::from_slice::<Json>(&bytes) {
                dropped = view.load_draft(&draft);
            }
        }
        let mut out = overview;
        out["draft_dropped"] = serde_json::json!(dropped);
        out["edits"] = view.status();
        if view.status()["count"] == 0 {
            if let Some((_, draft)) = self.rebase_source(&view) {
                out["rebase_from"] = draft["base"].clone();
            }
        }
        *self.kit.lock().unwrap() = Some(Arc::new(view));
        Ok(out)
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[tauri::command]
fn app_state(app: State<'_, Arc<App>>) -> Json {
    let en = app.settings.lock().unwrap().lang == "en";
    let kit = app.kit.lock().unwrap().as_ref().map(|k| {
        let mut o = k.overview_in(en);
        o["edits"] = k.status();
        o
    });
    let s = app.settings.lock().unwrap();
    serde_json::json!({ "lang": s.lang, "author": s.author, "recent": s.recent, "kit": kit, "version": env!("CARGO_PKG_VERSION") })
}

#[tauri::command]
fn set_language(app: State<'_, Arc<App>>, lang: String) -> Result<()> {
    if lang != "ru" && lang != "en" {
        return Err(fail("bad_request", lang));
    }
    app.settings.lock().unwrap().lang = lang;
    app.save_settings();
    Ok(())
}

/// The file dialog is opened here, so the page itself needs no file permissions.
#[tauri::command(async)]
fn pick_kit(handle: AppHandle, kind: String) -> Option<String> {
    let dialog = handle.dialog().file();
    let picked = if kind == "folder" { dialog.blocking_pick_folder() } else { dialog.add_filter("Zip", &["zip"]).blocking_pick_file() };
    picked.and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command(async)]
fn open_kit(app: State<'_, Arc<App>>, path: String) -> Result<Json> {
    let imported = import(Path::new(&path), &app.kits_dir(), Limits::default())?;
    app.activate(&imported.root, &path)
}

#[tauri::command(async)]
fn open_recent(app: State<'_, Arc<App>>, version: String) -> Result<Json> {
    let recent = app.settings.lock().unwrap().recent.iter().find(|r| r.version == version).cloned();
    let recent = recent.ok_or(fail("kit_not_found", version))?;
    let root = app.kits_dir().join(&recent.version);
    if !root.join("manifest.json").is_file() {
        app.settings.lock().unwrap().recent.retain(|r| r.version != recent.version);
        app.save_settings();
        return Err(fail("kit_not_found", recent.version));
    }
    app.activate(&root, &recent.source)
}

#[derive(Deserialize)]
pub struct CellEdit {
    table: String,
    row: usize,
    path: String,
    new: String,
}

#[derive(Deserialize)]
pub struct CellRef {
    table: String,
    row: usize,
    path: String,
}

/// Runs a change on the open kit and saves the draft. The result says the draft could not be saved instead of failing
/// the edit: the change is in memory and the next save may succeed.
fn changed(app: &App, kit: &KitView, mut result: Json) -> Json {
    if let Err(e) = app.save_draft(kit) {
        result["draft_error"] = serde_json::json!(e.message);
    }
    result
}

#[tauri::command(async)]
fn set_values(app: State<'_, Arc<App>>, edits: Vec<CellEdit>) -> Result<Json> {
    let kit = app.current()?;
    let list: Vec<_> = edits.into_iter().map(|e| (e.table, e.row, e.path, e.new)).collect();
    Ok(changed(&app, &kit, kit.set_values(&list)))
}

#[tauri::command(async)]
fn revert(app: State<'_, Arc<App>>, cells: Vec<CellRef>) -> Result<Json> {
    let kit = app.current()?;
    let list: Vec<_> = cells.into_iter().map(|c| (c.table, c.row, c.path)).collect();
    Ok(changed(&app, &kit, kit.revert(&list)))
}

#[tauri::command(async)]
fn undo(app: State<'_, Arc<App>>) -> Result<Json> {
    let kit = app.current()?;
    Ok(changed(&app, &kit, kit.undo()))
}

#[tauri::command(async)]
fn redo(app: State<'_, Arc<App>>) -> Result<Json> {
    let kit = app.current()?;
    Ok(changed(&app, &kit, kit.redo()))
}

#[tauri::command(async)]
fn list_changes(app: State<'_, Arc<App>>) -> Result<Json> {
    let kit = app.current()?;
    let en = app.settings.lock().unwrap().lang == "en";
    Ok(kit.list_changes(en))
}

#[tauri::command(async)]
fn set_note(app: State<'_, Arc<App>>, author: String, comment: String) -> Result<()> {
    let kit = app.current()?;
    kit.set_comment(&comment);
    app.settings.lock().unwrap().author = author;
    app.save_settings();
    app.save_draft(&kit)
}

#[tauri::command(async)]
fn pick_patch_open(handle: AppHandle) -> Option<String> {
    handle.dialog().file().add_filter("JSON", &["json"]).blocking_pick_file().and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().into_owned())
}

/// `stamp` comes from the page because it knows the local time.
#[tauri::command(async)]
fn pick_patch_save(handle: AppHandle, author: String, stamp: String) -> Option<String> {
    let name = patch_file_name(&author, &stamp);
    handle.dialog().file().add_filter("JSON", &["json"]).set_file_name(name).blocking_save_file().and_then(|p| p.into_path().ok()).map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command(async)]
fn import_patch(app: State<'_, Arc<App>>, path: String) -> Result<Json> {
    let kit = app.current()?;
    let size = std::fs::metadata(&path).map_err(|e| fail("patch_io", e.to_string()))?.len();
    if size > MAX_PATCH_BYTES {
        return Err(fail("patch_invalid", format!("file is {} bytes", size)));
    }
    let patch = Patch::read(Path::new(&path)).map_err(|e| fail("patch_invalid", e))?;
    let report = kit.import_patch(&patch).map_err(|(code, message)| Failure { code, message })?;
    Ok(changed(&app, &kit, report))
}

#[tauri::command(async)]
fn export_patch(app: State<'_, Arc<App>>, path: String, author: String, comment: String, created: String) -> Result<Json> {
    let kit = app.current()?;
    if author.trim().is_empty() {
        return Err(fail("patch_no_author", "a nickname is required"));
    }
    let tool = concat!("balance-studio/", env!("CARGO_PKG_VERSION"));
    let patch = kit.patch_json(author.trim(), comment.trim(), &created, tool).map_err(|(code, message)| Failure { code, message })?;
    let text = serde_json::to_vec_pretty(&patch).map_err(|e| fail("patch_io", e.to_string()))?;
    write_atomic(Path::new(&path), &text).map_err(|e| fail("patch_io", e.to_string()))?;
    Ok(serde_json::json!({ "path": path, "count": patch["changes"].as_array().map_or(0, |c| c.len()) }))
}

fn is_en(app: &App) -> bool {
    app.settings.lock().unwrap().lang == "en"
}

#[tauri::command(async)]
fn list_items(app: State<'_, Arc<App>>, query: Json) -> Result<Json> {
    Ok(app.current()?.list_items(&query))
}

#[tauri::command(async)]
fn loot_groups(app: State<'_, Arc<App>>, query: Json) -> Result<Json> {
    app.current()?.loot_groups(&query).map_err(|e| fail("kit_invalid", e))
}

#[tauri::command(async)]
fn loot_group(app: State<'_, Arc<App>>, key: String) -> Result<Json> {
    let kit = app.current()?;
    kit.loot_group(&key, is_en(&app)).map_err(|e| fail("kit_invalid", e))
}

#[tauri::command(async)]
fn get_item(app: State<'_, Arc<App>>, id: String, rarity: String) -> Result<Json> {
    let kit = app.current()?;
    kit.get_item(&id, &rarity, is_en(&app)).map_err(|e| fail("kit_invalid", e))
}

#[tauri::command(async)]
fn class_curves(app: State<'_, Arc<App>>, path: String) -> Result<Json> {
    let kit = app.current()?;
    kit.class_curves(&path, is_en(&app)).map_err(|e| fail("kit_invalid", e))
}

#[tauri::command(async)]
fn bulk_fields(app: State<'_, Arc<App>>, table: String, query: Json, rows: Json) -> Result<Json> {
    app.current()?.bulk_fields(&table, &query, &rows).map_err(|e| fail("kit_invalid", e))
}

#[tauri::command(async)]
fn preview_bulk(app: State<'_, Arc<App>>, spec: Json) -> Result<Json> {
    let spec = BulkSpec::from_json(&spec).map_err(|e| fail("bad_request", e))?;
    app.current()?.preview_bulk(&spec).map_err(|e| fail("bad_request", e))
}

#[tauri::command(async)]
fn apply_bulk(app: State<'_, Arc<App>>, spec: Json) -> Result<Json> {
    let kit = app.current()?;
    let spec = BulkSpec::from_json(&spec).map_err(|e| fail("bad_request", e))?;
    let result = kit.apply_bulk(&spec).map_err(|e| fail("bad_request", e))?;
    Ok(changed(&app, &kit, result))
}

/// What would happen to the changes of the older draft. Changes nothing.
#[tauri::command(async)]
fn rebase_preview(app: State<'_, Arc<App>>) -> Result<Json> {
    let kit = app.current()?;
    let (_, draft) = app.rebase_source(&kit).ok_or(fail("rebase_none", "no draft to move"))?;
    Ok(kit.plan_rebase(&draft, is_en(&app)))
}

/// The old draft is rewritten only now that the result is confirmed: what found no place stays in it, the rest is gone.
#[tauri::command(async)]
fn rebase_apply(app: State<'_, Arc<App>>, decisions: Json) -> Result<Json> {
    let kit = app.current()?;
    if kit.status()["count"] != 0 {
        return Err(fail("rebase_none", "this kit already has changes"));
    }
    let (path, mut draft) = app.rebase_source(&kit).ok_or(fail("rebase_none", "no draft to move"))?;
    let result = kit.apply_rebase(&draft, &decisions).map_err(|e| fail("bad_request", e))?;
    app.save_draft(&kit)?;
    let lost = result["lost"].clone();
    if lost.as_array().is_some_and(|l| !l.is_empty()) {
        draft["changes"] = lost;
        draft["rebased_to"] = serde_json::json!(kit.kit.version());
        let text = serde_json::to_vec_pretty(&draft).map_err(|e| fail("draft_io", e.to_string()))?;
        write_atomic(&path, &text).map_err(|e| fail("draft_io", e.to_string()))?;
    } else {
        std::fs::remove_file(&path).map_err(|e| fail("draft_io", e.to_string()))?;
    }
    Ok(changed(&app, &kit, result))
}

#[tauri::command]
fn close_kit(app: State<'_, Arc<App>>) {
    *app.kit.lock().unwrap() = None;
}

#[tauri::command(async)]
fn query_rows(app: State<'_, Arc<App>>, table: String, query: Json, offset: usize, limit: usize) -> Result<Json> {
    let kit = app.current()?;
    kit.query(&table, &Query::from_json(&query), offset, limit.min(MAX_PAGE)).map_err(|e| fail("kit_invalid", e))
}

/// `kit://localhost/<version>/<file>`: only icons of the open kit, by the names it listed when it was loaded.
fn serve_icon(app: &App, uri_path: &str) -> Response<Vec<u8>> {
    let empty = |code: StatusCode| Response::builder().status(code).body(Vec::new()).unwrap();
    let decoded = uri_path.replace("%2F", "/").replace("%2f", "/");
    let Some(file) = decoded.rsplit('/').next() else { return empty(StatusCode::NOT_FOUND) };
    let Some(kit) = app.kit.lock().unwrap().clone() else { return empty(StatusCode::NOT_FOUND) };
    let Some(path) = kit.icon_path(file) else { return empty(StatusCode::NOT_FOUND) };
    let Ok(bytes) = std::fs::read(path) else { return empty(StatusCode::NOT_FOUND) };
    let Some(mime) = image_type(&bytes) else { return empty(StatusCode::NOT_FOUND) };
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, "max-age=86400")
        .header(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(bytes)
        .unwrap()
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_uri_scheme_protocol("kit", |ctx, request| {
            let app = ctx.app_handle().state::<Arc<App>>();
            serve_icon(&app, request.uri().path())
        })
        .setup(|tauri_app| {
            tauri_app.manage(Arc::new(App::load()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_state, set_language, pick_kit, open_kit, open_recent, close_kit, query_rows, set_values, revert, undo, redo, list_changes, set_note,
            pick_patch_open, pick_patch_save, import_patch, export_patch, list_items, get_item, loot_groups, loot_group, class_curves, bulk_fields, preview_bulk, apply_bulk, rebase_preview, rebase_apply
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Balance Studio");
}
