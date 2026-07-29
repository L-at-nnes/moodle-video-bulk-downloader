use mvbd_core::{cookies, input, pipeline, tools, RunOptions};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::DialogExt;
use tokio_util::sync::CancellationToken;

struct AppState {
    cancel: Mutex<Option<CancellationToken>>,
}

#[derive(Debug, Deserialize)]
struct StartPayload {
    urls_text: String,
    cookies_text: String,
    options: RunOptions,
}

#[derive(Debug, Serialize, Clone)]
#[serde(tag = "type")]
enum FinishedPayload {
    Ok { success_count: usize, errors: Vec<(String, String)> },
    Cancelled,
    Error { message: String },
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct PersistedSettings {
    output_dir: Option<String>,
    concurrency: Option<usize>,
    download_threads: Option<usize>,
    capture_wait_ms: Option<u64>,
    ffmpeg_timeout_s: Option<u64>,
    headless: Option<bool>,
    keep_temp: Option<bool>,
    dry_run: Option<bool>,
    set_file_date: Option<bool>,
}

fn emit_log(app: &tauri::AppHandle, message: impl Into<String>) {
    let _ = app.emit("mvbd://log", message.into());
}

#[tauri::command]
async fn start_download(app: tauri::AppHandle, payload: StartPayload) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let mut guard = state.cancel.lock().unwrap();
        if guard.is_some() {
            return Err("Un téléchargement est déjà en cours.".to_string());
        }
        *guard = Some(CancellationToken::new());
    }

    let cancel = app
        .state::<AppState>()
        .cancel
        .lock()
        .unwrap()
        .clone()
        .expect("just set");

    let app_bg = app.clone();
    tokio::spawn(async move {
        let outcome = run_pipeline(&app_bg, payload, cancel.clone()).await;

        let payload = match outcome {
            Ok(_) if cancel.is_cancelled() => FinishedPayload::Cancelled,
            Ok(summary) => FinishedPayload::Ok {
                success_count: summary.success_count,
                errors: summary.errors,
            },
            Err(e) => FinishedPayload::Error { message: e.to_string() },
        };

        *app_bg.state::<AppState>().cancel.lock().unwrap() = None;
        let _ = app_bg.emit("mvbd://finished", payload);
    });

    Ok(())
}

async fn run_pipeline(
    app: &tauri::AppHandle,
    payload: StartPayload,
    cancel: CancellationToken,
) -> mvbd_core::Result<pipeline::RunSummary> {
    let project_root = std::env::current_dir()?;
    let cookie_list = cookies::parse_cookie_text(&payload.cookies_text)?;
    let mut entries = input::parse_input_text(&payload.urls_text)?;

    emit_log(app, "Préparation des outils (ffmpeg / mkvmerge / chromium)…");
    let tool_paths = tools::ensure_tools(&project_root).await?;

    emit_log(app, "Authentification via cookies…");
    pipeline::verify_auth(&entries, &cookie_list, &tool_paths, payload.options.headless).await?;

    emit_log(app, "Scan des pages de cours…");
    entries = pipeline::expand_courses(entries, &cookie_list, &tool_paths, payload.options.headless).await?;
    emit_log(app, format!("{} vidéo(s) à traiter.", entries.len()));
    let _ = app.emit("mvbd://entries", &entries);

    let app_events = app.clone();
    let on_event: Arc<dyn Fn(mvbd_core::ProgressEvent) + Send + Sync> = Arc::new(move |event| {
        let _ = app_events.emit("mvbd://progress", event);
    });

    pipeline::run(entries, cookie_list, tool_paths, payload.options, cancel, on_event).await
}

#[tauri::command]
fn cancel_download(app: tauri::AppHandle) {
    if let Some(token) = app.state::<AppState>().cancel.lock().unwrap().as_ref() {
        token.cancel();
    }
    mvbd_core::download::kill_all_procs();
}

#[tauri::command]
async fn pick_output_dir(app: tauri::AppHandle) -> Option<String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    rx.await.ok().flatten().map(|p| p.to_string())
}

#[tauri::command]
async fn pick_cookie_file(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().add_filter("Cookies", &["txt"]).pick_file(move |file| {
        let _ = tx.send(file);
    });
    let Some(picked) = rx.await.map_err(|e| e.to_string())? else {
        return Ok(None);
    };
    let path = picked.into_path().map_err(|e| e.to_string())?;
    let content = tokio::fs::read_to_string(path).await.map_err(|e| e.to_string())?;
    Ok(Some(content))
}

fn settings_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

#[tauri::command]
async fn load_settings(app: tauri::AppHandle) -> Result<PersistedSettings, String> {
    let path = settings_path(&app)?;
    match tokio::fs::read_to_string(&path).await {
        Ok(raw) => serde_json::from_str(&raw).map_err(|e| e.to_string()),
        Err(_) => Ok(PersistedSettings::default()),
    }
}

#[tauri::command]
async fn save_settings(app: tauri::AppHandle, settings: PersistedSettings) -> Result<(), String> {
    let path = settings_path(&app)?;
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| e.to_string())?;
    }
    let raw = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    tokio::fs::write(&path, raw).await.map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState { cancel: Mutex::new(None) })
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            start_download,
            cancel_download,
            pick_output_dir,
            pick_cookie_file,
            load_settings,
            save_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
