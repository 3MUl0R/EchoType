use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Listener, Manager};
use tracing::{error, info, warn};

use crate::dictation::{DictationEvent, DictationState};

/// Tray icon ID used to retrieve it later.
const TRAY_ID: &str = "main-tray";

/// Icon size in pixels.
const ICON_SIZE: u32 = 32;

/// Set up the system tray icon with menu and event handlers.
pub fn setup(app: &AppHandle) -> Result<(), String> {
    let show = MenuItem::with_id(app, "show", "Open EchoType", true, None::<&str>)
        .map_err(|e| format!("Failed to create menu item: {e}"))?;
    let settings = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)
        .map_err(|e| format!("Failed to create menu item: {e}"))?;
    let history = MenuItem::with_id(app, "history", "History", true, None::<&str>)
        .map_err(|e| format!("Failed to create menu item: {e}"))?;
    let sep = PredefinedMenuItem::separator(app)
        .map_err(|e| format!("Failed to create separator: {e}"))?;
    let quit = MenuItem::with_id(app, "quit", "Quit EchoType", true, None::<&str>)
        .map_err(|e| format!("Failed to create menu item: {e}"))?;

    let menu = Menu::with_items(app, &[&show, &settings, &history, &sep, &quit])
        .map_err(|e| format!("Failed to create tray menu: {e}"))?;

    let idle_icon = make_icon(&DictationState::Idle);

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(idle_icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .tooltip("EchoType")
        .on_menu_event(|app, event| match event.id.as_ref() {
            "quit" => {
                info!("Quit requested from tray menu");
                app.exit(0);
            }
            "show" => show_main_window(app),
            "settings" => {
                show_main_window(app);
                let _ = app.emit("navigate", "settings");
            }
            "history" => {
                show_main_window(app);
                let _ = app.emit("navigate", "history");
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)
        .map_err(|e| format!("Failed to build tray icon: {e}"))?;

    // Listen for dictation state changes to update the tray icon
    let app_clone = app.clone();
    app.listen("dictation:state", move |event| {
        if let Ok(de) = serde_json::from_str::<DictationEvent>(event.payload()) {
            if let Err(e) = update_icon(&app_clone, &de.state) {
                warn!(%e, "Failed to update tray icon");
            }
        }
    });

    info!("System tray initialized");
    Ok(())
}

/// Show and focus the main window.
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Update the tray icon to reflect the current dictation state.
fn update_icon(app: &AppHandle, state: &DictationState) -> Result<(), String> {
    let tray = app.tray_by_id(TRAY_ID).ok_or("Tray icon not found")?;

    let icon = make_icon(state);
    tray.set_icon(Some(icon))
        .map_err(|e| format!("Failed to set tray icon: {e}"))?;

    // Update tooltip to reflect state
    let tooltip = match state {
        DictationState::Idle => "EchoType",
        DictationState::Recording => "EchoType — Recording",
        DictationState::Transcribing => "EchoType — Transcribing",
        DictationState::Inserting => "EchoType — Inserting",
    };
    tray.set_tooltip(Some(tooltip))
        .map_err(|e| format!("Failed to set tooltip: {e}"))?;

    Ok(())
}

/// Create a tray icon image for the given dictation state.
fn make_icon(state: &DictationState) -> Image<'static> {
    let (r, g, b) = match state {
        DictationState::Idle => (128, 128, 128),        // Gray
        DictationState::Recording => (220, 50, 50),     // Red
        DictationState::Transcribing => (50, 130, 220), // Blue
        DictationState::Inserting => (50, 180, 80),     // Green
    };

    let rgba = generate_circle_rgba(r, g, b);
    Image::new_owned(rgba, ICON_SIZE, ICON_SIZE)
}

/// Generate a simple colored circle as RGBA pixel data.
fn generate_circle_rgba(r: u8, g: u8, b: u8) -> Vec<u8> {
    let size = ICON_SIZE;
    let center = size as f32 / 2.0;
    let radius = center - 2.0;
    let mut rgba = vec![0u8; (size * size * 4) as usize];

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - center + 0.5;
            let dy = y as f32 - center + 0.5;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist <= radius {
                // Anti-alias the edge
                let alpha = if dist > radius - 1.0 {
                    ((radius - dist) * 255.0) as u8
                } else {
                    255
                };
                let idx = ((y * size + x) * 4) as usize;
                rgba[idx] = r;
                rgba[idx + 1] = g;
                rgba[idx + 2] = b;
                rgba[idx + 3] = alpha;
            }
        }
    }

    rgba
}

/// Set up window close behavior: hide instead of quit.
pub fn setup_window_close_behavior(app: &AppHandle) {
    let handle = app.clone();
    if let Some(window) = app.get_webview_window("main") {
        window.on_window_event(move |event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Some(w) = handle.get_webview_window("main") {
                    if let Err(e) = w.hide() {
                        error!(%e, "Failed to hide main window");
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_rgba_has_correct_size() {
        let rgba = generate_circle_rgba(255, 0, 0);
        assert_eq!(rgba.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
    }

    #[test]
    fn icon_rgba_center_is_colored() {
        let rgba = generate_circle_rgba(255, 0, 0);
        let center = ICON_SIZE / 2;
        let idx = ((center * ICON_SIZE + center) * 4) as usize;
        assert_eq!(rgba[idx], 255); // R
        assert_eq!(rgba[idx + 1], 0); // G
        assert_eq!(rgba[idx + 2], 0); // B
        assert_eq!(rgba[idx + 3], 255); // A
    }

    #[test]
    fn icon_rgba_corner_is_transparent() {
        let rgba = generate_circle_rgba(255, 0, 0);
        // Top-left corner (0,0) should be transparent
        assert_eq!(rgba[3], 0); // Alpha at (0,0)
    }

    #[test]
    fn make_icon_all_states() {
        for state in [
            DictationState::Idle,
            DictationState::Recording,
            DictationState::Transcribing,
            DictationState::Inserting,
        ] {
            let _ = make_icon(&state); // Should not panic
        }
    }
}
