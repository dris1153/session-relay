use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};

use tauri::image::Image;
use std::time::{Duration, Instant};

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Wry};

use crate::app_state::AppState;
use crate::auto_save;
use crate::engine::claude_hook_config::HookStatus;

const TRAY_ID: &str = "main";
const CLAY: [u8; 4] = [0xd9, 0x77, 0x57, 0xff];

/// Tray copy lives in the same locale files as the UI.
fn label(language: &str, key: &str) -> String {
    let source = if language == "vi" { include_str!("../../src/locales/vi.json") } else { include_str!("../../src/locales/en.json") };
    let map: serde_json::Value = serde_json::from_str(source).unwrap_or_default();
    map.get(key).and_then(|v| v.as_str()).unwrap_or(key).to_string()
}

fn state(app: &AppHandle) -> Arc<AppState> {
    app.state::<Arc<AppState>>().inner().clone()
}

fn menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let state = state(app);
    let language = state.settings().language.unwrap_or_else(|| "en".into());
    let auto_save = auto_save::status(&state);
    Menu::with_items(
        app,
        &[
            &MenuItem::with_id(app, "open", label(&language, "tray.open"), true, None::<&str>)?,
            &MenuItem::with_id(app, "save_all", label(&language, "tray.save_all"), true, None::<&str>)?,
            &CheckMenuItem::with_id(app, "auto_save", label(&language, "tray.auto_save"), auto_save != HookStatus::Malformed, is_on(auto_save), None::<&str>)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, "quit", label(&language, "tray.quit"), true, None::<&str>)?,
        ],
    )
}

pub fn show_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

pub fn build(app: &AppHandle) -> tauri::Result<()> {
    let Some(icon) = app.default_window_icon().cloned() else { return Ok(()) };
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .tooltip("Session Relay")
        .menu(&menu(app)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "save_all" => {
                let app = app.clone();
                std::thread::spawn(move || {
                    let state = state(&app);
                    if let Err(e) = crate::dashboard::save_all(&state) {
                        log::warn!("save all from tray: {}", e.code());
                    }
                    crate::dashboard::publish(&app, &state, true);
                });
            }
            "auto_save" => {
                let state = state(app);
                if let Err(e) = auto_save::set(&state, !is_on(auto_save::status(&state))) {
                    log::warn!("auto-save toggle: {}", e.code());
                }
                refresh_menu(app);
            }
            "quit" => quit(app),
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    Ok(())
}

/// Labels follow the language; the auto-save check follows Claude's settings file.
pub fn refresh_menu(app: &AppHandle) {
    if let (Some(tray), Ok(menu)) = (app.tray_by_id(TRAY_ID), menu(app)) {
        let _ = tray.set_menu(Some(menu));
    }
}

/// Windows trays have no badge: the attention state is the app icon with a Clay dot.
pub fn set_attention(app: &AppHandle, attention: bool) {
    let (Some(tray), Some(icon)) = (app.tray_by_id(TRAY_ID), app.default_window_icon()) else { return };
    let icon = if attention { with_dot(icon) } else { icon.clone().to_owned() };
    let _ = tray.set_icon(Some(icon));
}

fn with_dot(icon: &Image<'_>) -> Image<'static> {
    let (w, h) = (icon.width(), icon.height());
    let mut rgba = icon.rgba().to_vec();
    let r = w.min(h) / 4;
    let (cx, cy) = (w - r - 1, h - r - 1);
    for y in cy.saturating_sub(r)..=(cy + r).min(h - 1) {
        for x in cx.saturating_sub(r)..=(cx + r).min(w - 1) {
            if (x.abs_diff(cx)).pow(2) + (y.abs_diff(cy)).pow(2) <= r * r {
                let i = ((y * w + x) * 4) as usize;
                rgba[i..i + 4].copy_from_slice(&CLAY);
            }
        }
    }
    Image::new_owned(rgba, w, h)
}

/// Registered for another path still counts as on: startup points it back at this exe.
fn is_on(status: HookStatus) -> bool {
    matches!(status, HookStatus::Installed | HookStatus::StalePath)
}

static QUITTING: AtomicBool = AtomicBool::new(false);

/// Saves Claude work the hooks have not saved yet, for at most 30 s, then quits regardless.
fn quit(app: &AppHandle) {
    if QUITTING.swap(true, Ordering::SeqCst) {
        return;
    }
    let (done, finished) = mpsc::channel();
    let flush_app = app.clone();
    std::thread::spawn(move || {
        if let Some(engine) = state(&flush_app).engine() {
            auto_save::finish_pending(&engine, Duration::ZERO, Instant::now() + Duration::from_secs(25), false);
        }
        let _ = done.send(());
    });
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = finished.recv_timeout(Duration::from_secs(30));
        app.exit(0);
    });
}

