mod clickthrough;
pub mod settings;
mod window_follow;

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{
    Emitter, Manager, PhysicalPosition, PhysicalSize, State, WebviewUrl, WebviewWindowBuilder,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

const CREDENTIAL_SERVICE: &str = "PrimevalOverlay";
const CREDENTIAL_USER: &str = "overlay-api";

#[derive(Default)]
pub struct RuntimeState {
    pub target_title: Mutex<String>,
    pub bearer: Mutex<Option<String>>,
    pub edit_mode: Mutex<bool>,
    pub layout_unlocked: Mutex<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoginReply {
    access_token: String,
}

fn checked_url(base: &str, path: &str) -> Result<reqwest::Url, String> {
    let mut url = reqwest::Url::parse(base).map_err(|_| "API URL is invalid".to_string())?;
    if url.scheme() != "https" || url.host_str().is_none() || !url.username().is_empty() {
        return Err("API URL must be an HTTPS origin without embedded credentials".into());
    }
    url.set_path(path);
    url.set_query(None);
    Ok(url)
}

fn api_client() -> Result<reqwest::Client, String> {
    let certificate =
        reqwest::Certificate::from_pem(include_bytes!("../certs/primeval-overlay.pem"))
            .map_err(|_| "Pinned overlay certificate is invalid")?;
    // Trust only the pinned overlay cert. Native TLS (Schannel) handles the
    // self-signed IP certificate more reliably on Windows than rustls.
    reqwest::Client::builder()
        .https_only(true)
        .tls_built_in_root_certs(false)
        .add_root_certificate(certificate)
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| "Could not initialize secure overlay connection".to_string())
}

fn transport_error(action: &str, error: reqwest::Error) -> String {
    let detail = error.to_string();
    let lower = detail.to_ascii_lowercase();
    if error.is_timeout() {
        return format!("{action} timed out — check API URL and port 25897");
    }
    if lower.contains("certificate")
        || lower.contains("tls")
        || lower.contains("ssl")
        || lower.contains("cert")
    {
        return format!("{action} failed TLS trust check ({detail})");
    }
    if error.is_connect() {
        return format!("{action} unreachable — check API URL / firewall ({detail})");
    }
    format!("{action} unreachable ({detail})")
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

fn ensure_editor_window(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("editor") {
        return Ok(window);
    }
    WebviewWindowBuilder::new(
        app,
        "editor",
        WebviewUrl::App("index.html?window=editor".into()),
    )
    .title("Fallen Earth Control Center")
    .inner_size(1180.0, 760.0)
    .min_inner_size(860.0, 560.0)
    .resizable(true)
    .decorations(true)
    .always_on_top(true)
    .skip_taskbar(false)
    .visible(false)
    .build()
    .map_err(|error| error.to_string())
}

fn editor_is_visible(app: &tauri::AppHandle) -> bool {
    app.get_webview_window("editor")
        .and_then(|window| window.is_visible().ok())
        .unwrap_or(false)
}

fn apply_edit_mode(
    app: &tauri::AppHandle,
    state: &RuntimeState,
    editable: bool,
) -> Result<(), String> {
    *state.edit_mode.lock().map_err(|_| "state unavailable")? = editable;
    if editable {
        *state
            .layout_unlocked
            .lock()
            .map_err(|_| "state unavailable")? = false;
    }
    if let Some(main) = app.get_webview_window("main") {
        // In-game overlay stays click-through unless layout is unlocked for dragging.
        let layout_unlocked = state
            .layout_unlocked
            .lock()
            .map(|value| *value)
            .unwrap_or(false);
        main.set_ignore_cursor_events(!(layout_unlocked && !editable))
            .map_err(|error| error.to_string())?;
        let _ = main.emit("edit-mode", editable);
        let _ = main.emit("layout-unlocked", false);
    }
    let editor = ensure_editor_window(app)?;
    if editable {
        editor.show().map_err(|error| error.to_string())?;
        editor.set_focus().map_err(|error| error.to_string())?;
        let _ = editor.set_always_on_top(true);
    } else {
        editor.hide().map_err(|error| error.to_string())?;
    }
    let _ = editor.emit("edit-mode", editable);
    let _ = app.emit("edit-mode", editable);
    Ok(())
}

fn toggle_control_center(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<Arc<RuntimeState>>();
    apply_edit_mode(app, state.inner(), !editor_is_visible(app))
}

fn open_control_center(app: &tauri::AppHandle) -> Result<(), String> {
    let state = app.state::<Arc<RuntimeState>>();
    apply_edit_mode(app, state.inner(), true)
}

#[tauri::command]
fn has_session(state: State<'_, Arc<RuntimeState>>) -> Result<bool, String> {
    Ok(state
        .bearer
        .lock()
        .map_err(|_| "state unavailable".to_string())?
        .as_ref()
        .map(|token| !token.is_empty())
        .unwrap_or(false))
}

#[tauri::command]
fn set_edit_mode(
    app: tauri::AppHandle,
    state: State<'_, Arc<RuntimeState>>,
    editable: bool,
) -> Result<(), String> {
    apply_edit_mode(&app, state.inner(), editable)
}

#[tauri::command]
fn set_layout_unlocked(
    app: tauri::AppHandle,
    state: State<'_, Arc<RuntimeState>>,
    unlocked: bool,
) -> Result<(), String> {
    *state
        .layout_unlocked
        .lock()
        .map_err(|_| "state unavailable")? = unlocked;
    let edit_mode = state.edit_mode.lock().map(|value| *value).unwrap_or(false);
    if let Some(main) = app.get_webview_window("main") {
        main.set_ignore_cursor_events(!(unlocked && !edit_mode))
            .map_err(|error| error.to_string())?;
        let _ = main.emit("layout-unlocked", unlocked);
    }
    let _ = app.emit("layout-unlocked", unlocked);
    Ok(())
}

#[tauri::command]
fn set_map_hitbox(_x: f64, _y: f64, _width: f64, _height: f64) -> Result<(), String> {
    // Kept for frontend compatibility. Overlay is fully click-through in play mode.
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HotkeyConfig {
    edit: String,
}

#[tauri::command]
fn set_target_title(state: State<'_, Arc<RuntimeState>>, title: String) -> Result<(), String> {
    let title = title.trim();
    if title.is_empty() || title.len() > 120 {
        return Err("Window title fragment must be 1–120 characters".into());
    }
    *state.target_title.lock().map_err(|_| "state unavailable")? = title.to_owned();
    Ok(())
}

#[tauri::command]
fn configure_hotkeys(app: tauri::AppHandle, config: HotkeyConfig) -> Result<(), String> {
    let primary = normalize_hotkey(&config.edit);
    let edit_shortcut: Shortcut = primary
        .parse()
        .map_err(|_| format!("Edit hotkey is invalid ({primary})"))?;
    let backup: Shortcut = "Ctrl+Shift+E"
        .parse()
        .map_err(|_| "Backup hotkey is invalid".to_string())?;
    let shortcuts = app.global_shortcut();
    shortcuts
        .unregister_all()
        .map_err(|error| error.to_string())?;
    let register = |shortcut: Shortcut| -> Result<(), String> {
        shortcuts
            .on_shortcut(shortcut, |app, _, event| {
                if event.state == ShortcutState::Pressed {
                    if let Err(error) = toggle_control_center(app) {
                        eprintln!("[OVERLAY] control center toggle failed: {error}");
                    }
                }
            })
            .map_err(|error| error.to_string())
    };
    register(edit_shortcut)?;
    // Always keep a second chord in case Ctrl+Alt+E is taken by another overlay/game.
    if primary != "Ctrl+Shift+E" {
        if let Err(error) = register(backup) {
            eprintln!("[OVERLAY] backup hotkey failed: {error}");
        }
    }
    Ok(())
}

fn normalize_hotkey(raw: &str) -> String {
    // global-hotkey splits on '+', and "Plus" is not a Code name.
    raw.trim()
        .replace("Plus", "Equal")
        .replace("plus", "Equal")
        .replace("++", "+Equal")
}

#[tauri::command]
async fn probe_api(api_base: String) -> Result<String, String> {
    let url = checked_url(&api_base, "/overlay/health")?;
    let response = api_client()?
        .get(url)
        .send()
        .await
        .map_err(|error| transport_error("Overlay API", error))?;
    if !response.status().is_success() {
        return Err(format!(
            "Overlay API unhealthy ({})",
            response.status().as_u16()
        ));
    }
    Ok("Overlay API reachable".into())
}

#[tauri::command]
async fn login_with_code(
    app: tauri::AppHandle,
    state: State<'_, Arc<RuntimeState>>,
    api_base: String,
    code: String,
) -> Result<(), String> {
    let code = code.trim();
    if code.len() < 4 || code.len() > 128 {
        return Err("Login code length is invalid".into());
    }
    let url = checked_url(&api_base, "/overlay/login")?;
    let response = api_client()?
        .post(url)
        .json(&serde_json::json!({ "code": code }))
        .send()
        .await
        .map_err(|error| transport_error("Login service", error))?;
    if !response.status().is_success() {
        return Err(format!("Login failed ({})", response.status().as_u16()));
    }
    let reply: LoginReply = response
        .json()
        .await
        .map_err(|_| "Login response is invalid".to_string())?;
    if reply.access_token.is_empty() || reply.access_token.len() > 4096 {
        return Err("Login token is invalid".into());
    }
    if let Ok(entry) = keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_USER) {
        let _ = entry.set_password(&reply.access_token);
    }
    *state.bearer.lock().map_err(|_| "state unavailable")? = Some(reply.access_token);
    let _ = app.emit("session-ready", ());
    Ok(())
}

#[tauri::command]
async fn fetch_snapshot(
    state: State<'_, Arc<RuntimeState>>,
    api_base: String,
) -> Result<serde_json::Value, String> {
    let token = state
        .bearer
        .lock()
        .map_err(|_| "state unavailable")?
        .clone()
        .ok_or_else(|| "Sign in with a Discord overlay code first".to_string())?;
    let response = api_client()?
        .get(checked_url(&api_base, "/overlay/snapshot")?)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| transport_error("Live overlay service", error))?;
    if response.status().as_u16() == 401 {
        return Err("Session expired — request a new Discord overlay code".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "Live overlay failed ({})",
            response.status().as_u16()
        ));
    }
    response
        .json()
        .await
        .map_err(|_| "Live overlay response is invalid".into())
}

#[tauri::command]
async fn fetch_manifest(
    state: State<'_, Arc<RuntimeState>>,
    api_base: String,
) -> Result<serde_json::Value, String> {
    let token = state
        .bearer
        .lock()
        .map_err(|_| "state unavailable")?
        .clone()
        .ok_or_else(|| "Sign in with a Discord overlay code first".to_string())?;
    let response = api_client()?
        .get(checked_url(&api_base, "/overlay/manifest")?)
        .bearer_auth(token)
        .send()
        .await
        .map_err(|error| transport_error("Map manifest service", error))?;
    if response.status().as_u16() == 401 {
        return Err("Session expired — request a new Discord overlay code".into());
    }
    if !response.status().is_success() {
        return Err(format!(
            "Map manifest failed ({})",
            response.status().as_u16()
        ));
    }
    response
        .json()
        .await
        .map_err(|_| "Map manifest response is invalid".into())
}

#[tauri::command]
fn logout(state: State<'_, Arc<RuntimeState>>) -> Result<(), String> {
    *state.bearer.lock().map_err(|_| "state unavailable")? = None;
    if let Ok(entry) = keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_USER) {
        let _ = entry.delete_credential();
    }
    Ok(())
}

fn start_window_follower(app: tauri::AppHandle, state: Arc<RuntimeState>) {
    std::thread::spawn(move || {
        let mut last_bounds: Option<window_follow::TargetBounds> = None;
        let mut last_online = false;
        let mut last_status_at = std::time::Instant::now()
            .checked_sub(Duration::from_secs(10))
            .unwrap_or_else(std::time::Instant::now);
        loop {
            let title = state
                .target_title
                .lock()
                .map(|value| value.clone())
                .unwrap_or_default();
            let bounds = window_follow::find_window_bounds(&title);
            if let Some(window) = app.get_webview_window("main") {
                match bounds {
                    Some(bounds) => {
                        let changed = last_bounds
                            .map(|prev| {
                                (prev.x - bounds.x).abs() > 1
                                    || (prev.y - bounds.y).abs() > 1
                                    || prev.width != bounds.width
                                    || prev.height != bounds.height
                            })
                            .unwrap_or(true);
                        if changed {
                            // Avoid show()/focus churn — only move/resize when the game
                            // window actually moved. Repeated SetWindowPos over a
                            // fullscreen game causes hitching and rubber-banding feel.
                            let _ = window.set_position(PhysicalPosition::new(bounds.x, bounds.y));
                            let _ = window.set_size(PhysicalSize::new(bounds.width, bounds.height));
                            if !last_online {
                                let _ = window.show();
                            }
                            last_bounds = Some(bounds);
                        }
                        let should_emit =
                            !last_online || last_status_at.elapsed() >= Duration::from_secs(2);
                        if should_emit {
                            let payload = serde_json::json!({
                                "online": true,
                                "bounds": bounds
                            });
                            // Broadcast so Control Center gets live online/bounds too
                            // (position sliders must use game client size, not editor size).
                            let _ = app.emit("target-status", &payload);
                            last_status_at = std::time::Instant::now();
                        }
                        last_online = true;
                    }
                    None => {
                        if last_online || last_status_at.elapsed() >= Duration::from_secs(2) {
                            let _ = app.emit(
                                "target-status",
                                serde_json::json!({ "online": false }),
                            );
                            last_status_at = std::time::Instant::now();
                        }
                        last_online = false;
                        last_bounds = None;
                    }
                }
            }
            std::thread::sleep(Duration::from_millis(750));
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = Arc::new(RuntimeState {
        target_title: Mutex::new("The Isle".into()),
        bearer: Mutex::new(
            keyring::Entry::new(CREDENTIAL_SERVICE, CREDENTIAL_USER)
                .ok()
                .and_then(|entry| entry.get_password().ok()),
        ),
        edit_mode: Mutex::new(false),
        layout_unlocked: Mutex::new(false),
    });
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(state.clone())
        .invoke_handler(tauri::generate_handler![
            set_edit_mode,
            set_layout_unlocked,
            set_map_hitbox,
            set_target_title,
            configure_hotkeys,
            probe_api,
            login_with_code,
            has_session,
            fetch_snapshot,
            fetch_manifest,
            logout,
            quit_app
        ])
        .setup(move |app| {
            if let Some(window) = app.get_webview_window("main") {
                window.set_always_on_top(true)?;
                window.set_ignore_cursor_events(true)?;
            }
            let _ = ensure_editor_window(app.handle());
            if let Err(error) = configure_hotkeys(
                app.handle().clone(),
                HotkeyConfig {
                    edit: "Ctrl+Alt+E".into(),
                },
            ) {
                eprintln!("[OVERLAY] default hotkeys failed: {error}");
            }

            let show_item = MenuItem::with_id(
                app,
                "show-control-center",
                "Open Control Center",
                true,
                None::<&str>,
            )?;
            let quit_item =
                MenuItem::with_id(app, "quit", "Quit Fallen Earth Overlay", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&show_item, &quit_item])?;
            let mut tray = TrayIconBuilder::new()
                .menu(&tray_menu)
                .tooltip("Fallen Earth Overlay")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show-control-center" => {
                        if let Err(error) = open_control_center(app) {
                            eprintln!("[OVERLAY] tray open failed: {error}");
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        if let Err(error) = open_control_center(tray.app_handle()) {
                            eprintln!("[OVERLAY] tray click failed: {error}");
                        }
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            let _ = tray.build(app);

            // Always surface Control Center on launch so connection/settings are reachable.
            let startup = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(700));
                if let Err(error) = open_control_center(&startup) {
                    eprintln!("[OVERLAY] startup control center failed: {error}");
                }
            });

            clickthrough::start_clickthrough_controller(app.handle().clone(), state.clone());
            start_window_follower(app.handle().clone(), state.clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("failed to run Fallen Earth Overlay");
}
