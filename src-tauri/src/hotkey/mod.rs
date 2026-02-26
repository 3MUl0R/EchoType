use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{error, info, warn};

/// Default dictation hotkey per platform.
#[cfg(target_os = "macos")]
pub const DEFAULT_SHORTCUT: &str = "cmd+shift+space";
#[cfg(not(target_os = "macos"))]
pub const DEFAULT_SHORTCUT: &str = "ctrl+shift+space";

/// Register the dictation hotkey on the given app handle.
///
/// Emits `dictation:start` on key press and `dictation:stop` on key release.
pub fn register_dictation_hotkey(app: &AppHandle) -> Result<(), String> {
    let shortcut_str = DEFAULT_SHORTCUT;

    let global_shortcut = app.global_shortcut();

    if global_shortcut.is_registered(shortcut_str) {
        warn!(shortcut = shortcut_str, "Hotkey already registered");
        return Ok(());
    }

    global_shortcut
        .on_shortcut(shortcut_str, |app, _shortcut, event| match event.state {
            ShortcutState::Pressed => {
                info!("Dictation hotkey pressed");
                if let Err(e) = app.emit("dictation:start", ()) {
                    error!(%e, "Failed to emit dictation:start");
                }
            }
            ShortcutState::Released => {
                info!("Dictation hotkey released");
                if let Err(e) = app.emit("dictation:stop", ()) {
                    error!(%e, "Failed to emit dictation:stop");
                }
            }
        })
        .map_err(|e| {
            error!(%e, shortcut = shortcut_str, "Failed to register dictation hotkey");
            format!("Failed to register hotkey {shortcut_str}: {e}")
        })?;

    info!(shortcut = shortcut_str, "Dictation hotkey registered");
    Ok(())
}

/// Unregister the dictation hotkey.
#[allow(dead_code)]
pub fn unregister_dictation_hotkey(app: &AppHandle) -> Result<(), String> {
    let global_shortcut = app.global_shortcut();
    global_shortcut
        .unregister(DEFAULT_SHORTCUT)
        .map_err(|e| format!("Failed to unregister hotkey: {e}"))?;
    info!(shortcut = DEFAULT_SHORTCUT, "Dictation hotkey unregistered");
    Ok(())
}
