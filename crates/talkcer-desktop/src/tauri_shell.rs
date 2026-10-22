//! Native Tauri shell.
//!
//! Built only with the `tauri-shell` feature, because it needs the WebView2
//! toolchain. The shell opens the web UI in the main window, keeps a compact
//! always-on-top overlay as a second window, and drives both from the core API
//! through the headless [`crate::shell::OverlayController`].
//!
//! The shell never captures audio and never talks to a provider directly. It
//! only subscribes to the active session and applies window commands.

use std::sync::Arc;

use talkcer_core::OverlayState;
use tauri::{AppHandle, Manager, WebviewWindow, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::client::{HttpOverlayApi, OverlayApi};
use crate::hotkeys::{self, ShortcutAction};
use crate::overlay::WindowCommand;
use crate::shell::{OverlayController, QuickAction};
use crate::stealth::{self, StealthCommand};
use crate::tray;

/// Main workspace window label.
pub const MAIN_WINDOW: &str = "main";
/// Overlay window label.
pub const OVERLAY_WINDOW: &str = "overlay";

/// Build one global shortcut from a display combo such as `Ctrl+3`.
///
/// Only the `Ctrl+<digit>` family the shell actually binds is supported; the
/// result is `None` for anything else so a typo cannot register silently.
#[must_use]
pub fn shortcut_for(combo: &str) -> Option<Shortcut> {
    let key = combo.strip_prefix("Ctrl+")?;
    let code = match key {
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        _ => return None,
    };
    Some(Shortcut::new(Some(Modifiers::CONTROL), code))
}

/// Apply one window command to the overlay window.
///
/// A hidden overlay is really hidden and never intercepts clicks, so it cannot
/// block the application underneath.
pub fn apply(window: &WebviewWindow, command: &WindowCommand) -> tauri::Result<()> {
    match command {
        WindowCommand::Show {
            always_on_top,
            opacity,
            x,
            y,
            width,
            height,
        } => {
            window.set_always_on_top(*always_on_top)?;
            window.set_size(tauri::PhysicalSize::new(*width as u32, *height as u32))?;
            window.set_position(tauri::PhysicalPosition::new(*x as i32, *y as i32))?;
            window.set_ignore_cursor_events(false)?;
            window.show()?;
            // Tauri exposes no per-window alpha; the overlay page draws its own
            // background from the saved opacity instead.
            let _ = opacity;
            Ok(())
        }
        WindowCommand::Hide => {
            window.set_ignore_cursor_events(false)?;
            window.hide()?;
            Ok(())
        }
        WindowCommand::IgnoreCursor(enabled) => window.set_ignore_cursor_events(*enabled),
    }
}

/// Apply a full plan to the overlay window.
///
/// # Errors
///
/// Returns a Tauri error when a window call fails.
pub fn apply_plan(window: &WebviewWindow, commands: &[WindowCommand]) -> tauri::Result<()> {
    for command in commands {
        apply(window, command)?;
    }
    Ok(())
}

fn controller() -> OverlayController {
    OverlayController::new(Arc::new(HttpOverlayApi::new(crate::DEFAULT_API)))
}

fn overlay_window(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(OVERLAY_WINDOW)
}

/// The action registered for one shortcut, matched by its key combination.
fn action_of(shortcut: &Shortcut) -> Option<ShortcutAction> {
    hotkeys::bindings().into_iter().find_map(|binding| {
        shortcut_for(binding.combo)
            .filter(|built| built.id() == shortcut.id())
            .map(|_| binding.action)
    })
}

/// Register the global shortcut table.
///
/// A busy combo must not kill the shell: when another application owns one of
/// them, registration fails loudly on stderr and the tray remains the fallback.
fn register_shortcuts(app: &mut tauri::App) {
    for binding in hotkeys::bindings() {
        let Some(shortcut) = shortcut_for(binding.combo) else {
            eprintln!("unknown shortcut combo {}", binding.combo);
            continue;
        };
        if let Err(error) = app.global_shortcut().register(shortcut) {
            eprintln!("shortcut {} unavailable: {error}", binding.combo);
        }
    }
}
///
/// The web workspace can change the overlay through the API directly; polling
/// keeps the native window correct without a second event bus. The plan is
/// idempotent, so re-applying a cached state is free.
async fn sync_loop(handle: AppHandle, stealth: Arc<std::sync::atomic::AtomicBool>) {
    let api = HttpOverlayApi::new(crate::DEFAULT_API);
    let mut last: Option<serde_json::Value> = None;
    loop {
        if let Ok(preferences) = api.preferences().await {
            // The saved flag is authoritative, so Ctrl+2 and the workspace
            // toggle both drive the shell through this poll.
            stealth.store(
                preferences.undetectable,
                std::sync::atomic::Ordering::Relaxed,
            );
            let fingerprint = serde_json::to_value(&preferences).ok();
            if fingerprint.as_ref() != last.as_ref() {
                last = fingerprint;
                // Apply both directions. While on, the re-assert loop below keeps
                // the capture exclusion alive; the restore only happens here, when
                // the saved flag changes, so it must not be skipped.
                apply_stealth(&handle, preferences.undetectable);
                let commands = crate::overlay::plan(&preferences);
                if let Some(window) = overlay_window(&handle) {
                    if let Err(error) = apply_plan(&window, &commands) {
                        eprintln!("overlay window sync failed: {error}");
                    }
                }
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

/// Toggle the overlay and apply the resulting plan.
async fn toggle_overlay(app: AppHandle) {
    let state = match controller().toggle().await {
        Ok(state) => state,
        Err(error) => {
            eprintln!("overlay toggle failed: {error}");
            return;
        }
    };
    if let Some(window) = overlay_window(&app) {
        if let Err(error) = apply_plan(&window, &state.commands) {
            eprintln!("overlay window update failed: {error}");
        }
    }
}

/// Run one quick action and apply the resulting plan.
async fn run_quick_action(app: AppHandle, action: QuickAction) {
    let state = match controller().quick_action(action).await {
        Ok(state) => state,
        Err(error) => {
            eprintln!("overlay action failed: {error}");
            return;
        }
    };
    if let Some(window) = overlay_window(&app) {
        if let Err(error) = apply_plan(&window, &state.commands) {
            eprintln!("overlay window update failed: {error}");
        }
    }
}

/// Shared shell runtime state.
///
/// `undetectable` is written by the preference sync loop and read by the
/// capture-exclusion re-assert loop, both on the Tauri async runtime. An atomic
/// keeps those two loops independent. `app_visible` only changes from the
/// shortcut handler.
#[derive(Debug)]
struct ShellRuntime {
    undetectable: Arc<std::sync::atomic::AtomicBool>,
    app_visible: std::sync::Mutex<bool>,
}

impl ShellRuntime {
    /// Build with the app visible and undetectable off.
    fn new() -> Self {
        Self {
            undetectable: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            app_visible: std::sync::Mutex::new(true),
        }
    }

    fn undetectable_flag(&self) -> Arc<std::sync::atomic::AtomicBool> {
        Arc::clone(&self.undetectable)
    }

    fn toggle_app(&self) -> bool {
        let mut state = self.app_visible.lock().expect("app lock");
        *state = !*state;
        *state
    }
}

/// Apply one undetectable-mode plan.
///
/// The capture-exclusion half is Windows-only (the Win32 display affinity).
/// On other platforms the taskbar and title commands still apply, so the
/// observable behaviour stays consistent where Tauri runs.
fn apply_stealth(handle: &AppHandle, undetectable: bool) {
    for command in stealth::stealth_plan(undetectable) {
        match command {
            StealthCommand::ExcludeFromCapture | StealthCommand::RestoreCapture => {
                #[cfg(windows)]
                {
                    let exclude = matches!(command, StealthCommand::ExcludeFromCapture);
                    let changed = crate::win_stealth::set_capture_excluded(exclude);
                    let (excluded, plain) = crate::win_stealth::exclusion_counts();
                    eprintln!("stealth: capture exclusion changed {changed}, now {excluded} excluded and {plain} plain");
                }
            }
            StealthCommand::HideFromTaskbar => {
                taskbar_visible(handle, false);
                tray_visible(handle, false);
            }
            StealthCommand::ShowInTaskbar => {
                taskbar_visible(handle, true);
                tray_visible(handle, true);
            }
            StealthCommand::DisguiseTitles => set_titles(handle, stealth::disguise_title()),
            StealthCommand::RestoreTitles => set_titles(handle, "Talkcer"),
        }
    }
}

fn set_titles(handle: &AppHandle, title: &str) {
    if let Some(window) = handle.get_webview_window(MAIN_WINDOW) {
        let _ = window.set_title(title);
    }
}

/// Show or hide the tray icon.
///
/// The icon is the last branded surface once the taskbar and titles are gone,
/// so undetectable mode removes it too. Global shortcuts still work while it is
/// hidden.
fn tray_visible(handle: &AppHandle, visible: bool) {
    if let Some(icon) = handle.tray_by_id("talkcer") {
        let _ = icon.set_visible(visible);
        eprintln!("stealth: tray set visible {visible}");
    }
}

/// Show or hide the main window's taskbar button.
///
/// The overlay never has one (`skipTaskbar` in the window config), so only the
/// main window matters. Implemented here so the stealth plan stays free of the
/// Win32 module.
fn taskbar_visible(_handle: &AppHandle, visible: bool) {
    #[cfg(windows)]
    {
        // `set_skip_taskbar` uses DeleteTab, which the shell restores the moment
        // the window is focused. The extended style survives activation.
        let _ = crate::win_stealth::set_taskbar_visible(visible);
    }
}

/// Show or hide the whole application (main window plus overlay).
fn set_app_visible(handle: &AppHandle, visible: bool) {
    if let Some(main) = handle.get_webview_window(MAIN_WINDOW) {
        if visible {
            let _ = main.show();
            let _ = main.set_focus();
        } else {
            let _ = main.hide();
        }
    }
    if let Some(overlay) = overlay_window(handle) {
        if visible {
            let _ = overlay.show();
        } else {
            let _ = overlay.hide();
        }
    }
}

/// Focus the main window and navigate it to one route.
async fn open_route(handle: AppHandle, route: String) {
    if let Some(main) = handle.get_webview_window(MAIN_WINDOW) {
        let _ = main.show();
        let _ = main.set_focus();
        let url = format!("{}{route}", crate::tray::WORKSPACE_URL);
        if let Ok(parsed) = url.parse() {
            let _ = main.navigate(parsed);
        }
    }
}

/// Toggle undetectable mode and persist it.
///
/// The sync loop reads the saved flag back and applies the plan, so the shell
/// never keeps a second copy that can drift from the workspace setting.
async fn toggle_undetectable(handle: AppHandle) {
    let api = HttpOverlayApi::new(crate::DEFAULT_API);
    let current = api
        .preferences()
        .await
        .map(|saved| saved.undetectable)
        .unwrap_or(false);
    if let Err(error) = api.set_undetectable(!current).await {
        eprintln!("undetectable toggle failed: {error}");
        return;
    }
    // Apply at once instead of waiting for the next poll tick.
    apply_stealth(&handle, !current);
}

/// Fetch the current running mode and open it.
async fn open_current_mode(handle: AppHandle) {
    let route = match current_running().await {
        Some(running) => {
            hotkeys::route_for(ShortcutAction::OpenCurrentMode, Some(running)).to_owned()
        }
        None => "/".to_owned(),
    };
    open_route(handle, route).await;
}

/// The session that is currently running (`active`) or resumable (`paused`).
async fn current_running() -> Option<hotkeys::RunningMode> {
    let client = reqwest::Client::new();
    let response = client
        .get(format!("{}/sessions/current", crate::DEFAULT_API))
        .send()
        .await
        .ok()?;
    if !response.status().is_success() {
        return None;
    }
    let body: serde_json::Value = response.json().await.ok()?;
    let session = body.get("session")?;
    let mode = talkcer_core::SessionMode::parse(session.get("mode")?.as_str()?).ok()?;
    let session_id = session
        .get("id")
        .and_then(|value| value.as_str())
        .and_then(|text| text.parse().ok());
    Some(hotkeys::RunningMode { mode, session_id })
}

/// Dispatch one shortcut press to the shell.
async fn dispatch(handle: AppHandle, runtime: Arc<ShellRuntime>, action: ShortcutAction) {
    match action {
        ShortcutAction::ToggleApp => set_app_visible(&handle, runtime.toggle_app()),
        ShortcutAction::ToggleUndetectable => toggle_undetectable(handle).await,
        ShortcutAction::ToggleOverlay => toggle_overlay(handle).await,
        ShortcutAction::OpenSettings => open_route(handle, "/settings/ai".to_owned()).await,
        ShortcutAction::OpenCurrentMode => open_current_mode(handle).await,
    }
}
///
/// # Errors
///
/// Returns a Tauri error when the runtime cannot start.
pub fn run() -> tauri::Result<()> {
    let runtime = Arc::new(ShellRuntime::new());
    let handler_runtime = runtime.clone();
    tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, pressed, event| {
                    if event.state() != ShortcutState::Pressed {
                        return;
                    }
                    let Some(action) = action_of(pressed) else {
                        return;
                    };
                    let app = app.clone();
                    let runtime = handler_runtime.clone();
                    tauri::async_runtime::spawn(dispatch(app, runtime, action));
                })
                .build(),
        )
        .setup(move |app| {
            let handle = app.handle().clone();
            register_shortcuts(app);
            // Keep native window state and the saved privacy flag in sync.
            tauri::async_runtime::spawn(sync_loop(handle.clone(), runtime.undetectable_flag()));
            // Re-assert capture exclusion while undetectable; the OS can drop
            // the flag silently, so the reference re-applies it every 500 ms.
            let stealth_handle = handle.clone();
            let stealth_flag = runtime.undetectable_flag();
            tauri::async_runtime::spawn(async move {
                loop {
                    if stealth_flag.load(std::sync::atomic::Ordering::Relaxed) {
                        apply_stealth(&stealth_handle, true);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            });

            // The tray keeps the overlay reachable when no window has focus.
            let menu = tauri::menu::MenuBuilder::new(app)
                .text(
                    "toggle",
                    tray::tray_label(&crate::tray::TrayItem::ToggleOverlay),
                )
                .text(
                    "show",
                    tray::tray_label(&crate::tray::TrayItem::ShowOverlay),
                )
                .text(
                    "hide",
                    tray::tray_label(&crate::tray::TrayItem::HideOverlay),
                )
                .separator()
                .text(
                    "workspace",
                    tray::tray_label(&crate::tray::TrayItem::OpenWorkspace),
                )
                .separator()
                .text("quit", tray::tray_label(&crate::tray::TrayItem::Quit))
                .build()?;
            tauri::tray::TrayIconBuilder::with_id("talkcer")
                .icon(app.default_window_icon().cloned().unwrap())
                .tooltip("Talkcer")
                .menu(&menu)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "toggle" => {
                        let app = app.clone();
                        tauri::async_runtime::spawn(toggle_overlay(app));
                    }
                    "show" => {
                        if let Some(window) = overlay_window(app) {
                            let _ = window.show();
                        }
                    }
                    "hide" => {
                        if let Some(window) = overlay_window(app) {
                            let _ = apply(&window, &WindowCommand::Hide);
                        }
                    }
                    "workspace" => {
                        if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => app.exit(0),
                    _ => {}
                })
                .build(app)?;

            // Closing the overlay hides it; closing the workspace quits.
            if let Some(overlay) = overlay_window(&handle) {
                let window = overlay.clone();
                overlay.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            overlay_state,
            overlay_toggle,
            overlay_hide,
            overlay_move,
            overlay_opacity,
            overlay_click_through,
            overlay_follow,
            overlay_quick_action
        ])
        .run(tauri::generate_context!())
}

/// Return the current overlay plan so the overlay page can render it.
#[tauri::command]
async fn overlay_state() -> Result<serde_json::Value, String> {
    let state = controller().refresh().await?;
    Ok(serde_json::json!({
        "preferences": state.preferences,
        "items": state.items,
        "commands": state.commands,
        "actions": state.actions.iter().map(|action| action.label()).collect::<Vec<_>>(),
    }))
}

#[tauri::command]
async fn overlay_toggle(app: AppHandle) -> Result<serde_json::Value, String> {
    let state = controller().toggle().await?;
    if let Some(window) = overlay_window(&app) {
        apply_plan(&window, &state.commands).map_err(|error| error.to_string())?;
    }
    Ok(serde_json::json!({ "state": state.preferences.state }))
}

#[tauri::command]
async fn overlay_hide(app: AppHandle) -> Result<serde_json::Value, String> {
    let state = controller().hide().await?;
    if let Some(window) = overlay_window(&app) {
        apply_plan(&window, &state.commands).map_err(|error| error.to_string())?;
    }
    Ok(serde_json::json!({ "state": state.preferences.state }))
}

#[tauri::command]
async fn overlay_move(app: AppHandle, x: i64, y: i64) -> Result<serde_json::Value, String> {
    let state = controller().move_to(x, y).await?;
    if let Some(window) = overlay_window(&app) {
        apply_plan(&window, &state.commands).map_err(|error| error.to_string())?;
    }
    Ok(serde_json::json!({ "state": state.preferences.state }))
}

#[tauri::command]
async fn overlay_opacity(app: AppHandle, opacity: f64) -> Result<serde_json::Value, String> {
    let state = controller().set_opacity(opacity).await?;
    if let Some(window) = overlay_window(&app) {
        apply_plan(&window, &state.commands).map_err(|error| error.to_string())?;
    }
    Ok(serde_json::json!({ "opacity": state.preferences.opacity }))
}

#[tauri::command]
async fn overlay_click_through(app: AppHandle, enabled: bool) -> Result<serde_json::Value, String> {
    let state = controller().set_click_through(enabled).await?;
    if let Some(window) = overlay_window(&app) {
        apply_plan(&window, &state.commands).map_err(|error| error.to_string())?;
    }
    Ok(serde_json::json!({ "state": state.preferences.state }))
}

#[tauri::command]
async fn overlay_follow(
    app: AppHandle,
    session_id: uuid::Uuid,
) -> Result<serde_json::Value, String> {
    let state = controller().follow(session_id).await?;
    if let Some(window) = overlay_window(&app) {
        apply_plan(&window, &state.commands).map_err(|error| error.to_string())?;
    }
    Ok(serde_json::json!({ "session_id": session_id }))
}

#[tauri::command]
async fn overlay_quick_action(app: AppHandle, action: String) -> Result<serde_json::Value, String> {
    let action = match action.as_str() {
        "explain" => QuickAction::Explain,
        "translate" => QuickAction::Translate,
        "suggest" => QuickAction::SuggestPoints,
        "hide" => QuickAction::Hide,
        other => return Err(format!("unknown overlay action '{other}'")),
    };
    run_quick_action(app, action).await;
    Ok(serde_json::json!({ "action": action.label() }))
}

/// Whether a saved state is visible, used by the shell when it restores a window.
#[must_use]
pub fn is_visible(state: OverlayState) -> bool {
    state != OverlayState::Hidden
}
