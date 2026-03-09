use std::sync::Mutex;

use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{error, info, warn};

/// Default dictation hotkey per platform.
#[cfg(target_os = "macos")]
pub const DEFAULT_SHORTCUT: &str = "cmd+shift+space";
#[cfg(not(target_os = "macos"))]
pub const DEFAULT_SHORTCUT: &str = "ctrl+shift+space";

/// Tracks the currently registered shortcut string so we can unregister it
/// when the user changes hotkeys.
static CURRENT_SHORTCUT: Mutex<Option<String>> = Mutex::new(None);

/// Register the dictation hotkey on the given app handle.
///
/// If `shortcut` is `Some`, uses that shortcut string. Otherwise falls back to
/// `DEFAULT_SHORTCUT`.
///
/// Emits `dictation:start` on key press and `dictation:stop` on key release.
pub fn register_dictation_hotkey(
    app: &AppHandle,
    shortcut: Option<&str>,
) -> Result<(), String> {
    let shortcut_str = shortcut.unwrap_or(DEFAULT_SHORTCUT);

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

    // Track the registered shortcut
    if let Ok(mut current) = CURRENT_SHORTCUT.lock() {
        *current = Some(shortcut_str.to_string());
    }

    info!(shortcut = shortcut_str, "Dictation hotkey registered");
    Ok(())
}

/// Get the currently registered shortcut string.
pub fn current_shortcut() -> Option<String> {
    CURRENT_SHORTCUT.lock().ok().and_then(|g| g.clone())
}

/// Start suppressing the non-modifier key from the hotkey combo so it doesn't
/// repeat into the focused application while the user holds the hotkey.
///
/// Uses a low-level keyboard hook (`WH_KEYBOARD_LL`) on Windows to intercept
/// and swallow WM_KEYDOWN events for the letter/function key in the combo.
/// Key-up events are always passed through so the global shortcut plugin can
/// still detect when the hotkey is released (critical for push-to-talk).
#[cfg(target_os = "windows")]
pub fn start_key_suppression() {
    let shortcut = match current_shortcut() {
        Some(s) => s,
        None => return,
    };
    key_suppression::start(&shortcut);
}

/// Stop suppressing hotkey key events. Call when dictation recording stops.
#[cfg(target_os = "windows")]
pub fn stop_key_suppression() {
    key_suppression::stop();
}

#[cfg(not(target_os = "windows"))]
pub fn start_key_suppression() {}

#[cfg(not(target_os = "windows"))]
pub fn stop_key_suppression() {}

/// Windows low-level keyboard hook for suppressing hotkey key repeats.
#[cfg(target_os = "windows")]
mod key_suppression {
    use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
    use tracing::{debug, error};

    static SUPPRESSING: AtomicBool = AtomicBool::new(false);
    static SUPPRESS_VK: AtomicU16 = AtomicU16::new(0);
    static HOOK_STARTED: AtomicBool = AtomicBool::new(false);

    const WM_KEYDOWN: usize = 0x0100;
    const WM_SYSKEYDOWN: usize = 0x0104;

    unsafe extern "system" fn ll_keyboard_proc(
        code: i32,
        wparam: windows_sys::Win32::Foundation::WPARAM,
        lparam: windows_sys::Win32::Foundation::LPARAM,
    ) -> windows_sys::Win32::Foundation::LRESULT {
        if code >= 0 && SUPPRESSING.load(Ordering::Relaxed) {
            // Only suppress key-down events; let key-up pass through so
            // the global shortcut plugin detects the hotkey release.
            if wparam == WM_KEYDOWN || wparam == WM_SYSKEYDOWN {
                let kb = &*(lparam
                    as *const windows_sys::Win32::UI::WindowsAndMessaging::KBDLLHOOKSTRUCT);
                let target_vk = SUPPRESS_VK.load(Ordering::Relaxed) as u32;
                if target_vk != 0 && kb.vkCode == target_vk {
                    return 1; // Swallow this key-down event
                }
            }
        }
        windows_sys::Win32::UI::WindowsAndMessaging::CallNextHookEx(
            std::ptr::null_mut(),
            code,
            wparam,
            lparam,
        )
    }

    fn ensure_hook_thread() {
        if HOOK_STARTED.swap(true, Ordering::SeqCst) {
            return; // Already started
        }

        std::thread::Builder::new()
            .name("echotype-key-hook".into())
            .spawn(|| unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::*;

                let hook =
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_proc), std::ptr::null_mut(), 0);
                if hook.is_null() {
                    error!("Failed to install WH_KEYBOARD_LL hook");
                    HOOK_STARTED.store(false, Ordering::SeqCst);
                    return;
                }
                debug!("Keyboard suppression hook installed");

                // Message pump — required for WH_KEYBOARD_LL callbacks
                let mut msg: MSG = std::mem::zeroed();
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                UnhookWindowsHookEx(hook);
            })
            .ok();
    }

    /// Begin suppressing key-down events for the non-modifier key in the hotkey.
    pub fn start(shortcut: &str) {
        if let Some(vk) = parse_non_modifier_vk(shortcut) {
            SUPPRESS_VK.store(vk, Ordering::Relaxed);
            ensure_hook_thread();
            SUPPRESSING.store(true, Ordering::SeqCst);
            debug!(vk = vk, "Key suppression started");
        }
    }

    /// Stop suppressing key events.
    pub fn stop() {
        SUPPRESSING.store(false, Ordering::SeqCst);
        debug!("Key suppression stopped");
    }

    /// Extract the non-modifier virtual key code from a shortcut string.
    fn parse_non_modifier_vk(shortcut: &str) -> Option<u16> {
        for part in shortcut.split('+') {
            match part.to_lowercase().as_str() {
                "ctrl" | "control" | "shift" | "alt" | "super" | "cmd" | "meta" => continue,
                s if s.len() == 1 => {
                    let ch = s.chars().next().unwrap().to_ascii_uppercase();
                    return Some(ch as u16);
                }
                "space" => return Some(0x20),
                "tab" => return Some(0x09),
                "f1" => return Some(0x70),
                "f2" => return Some(0x71),
                "f3" => return Some(0x72),
                "f4" => return Some(0x73),
                "f5" => return Some(0x74),
                "f6" => return Some(0x75),
                "f7" => return Some(0x76),
                "f8" => return Some(0x77),
                "f9" => return Some(0x78),
                "f10" => return Some(0x79),
                "f11" => return Some(0x7A),
                "f12" => return Some(0x7B),
                _ => continue,
            }
        }
        None
    }
}

/// Unregister the currently registered dictation hotkey.
pub fn unregister_dictation_hotkey(app: &AppHandle) -> Result<(), String> {
    let shortcut_str = CURRENT_SHORTCUT
        .lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_else(|| DEFAULT_SHORTCUT.to_string());

    let global_shortcut = app.global_shortcut();

    if !global_shortcut.is_registered(shortcut_str.as_str()) {
        // Nothing to unregister
        return Ok(());
    }

    global_shortcut
        .unregister(shortcut_str.as_str())
        .map_err(|e| format!("Failed to unregister hotkey: {e}"))?;

    if let Ok(mut current) = CURRENT_SHORTCUT.lock() {
        *current = None;
    }

    info!(shortcut = %shortcut_str, "Dictation hotkey unregistered");
    Ok(())
}
