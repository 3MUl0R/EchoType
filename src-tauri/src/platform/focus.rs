use tracing::{debug, info, warn};

use crate::db::profiles::AppIdentifierType;

/// Represents the target window/app that was focused when dictation started.
#[derive(Debug, Clone)]
pub struct FocusTarget {
    /// Platform-specific identifier for the focused application.
    pub app_id: String,
    /// Type of identifier (for profile matching).
    pub id_type: AppIdentifierType,
}

/// Capture the currently focused application.
pub fn capture_focus() -> Option<FocusTarget> {
    let target = platform_capture_focus();
    if let Some(ref t) = target {
        info!(app_id = %t.app_id, "Captured focus target");
    } else {
        warn!("Could not capture focus target");
    }
    target
}

/// Re-focus the previously captured target.
pub fn restore_focus(target: &FocusTarget) -> bool {
    debug!(app_id = %target.app_id, "Restoring focus");
    platform_restore_focus(target)
}

// --- macOS implementation ---

#[cfg(target_os = "macos")]
fn platform_capture_focus() -> Option<FocusTarget> {
    use std::process::Command;

    // Use osascript to get the frontmost app bundle ID
    let output = Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to get bundle identifier of first process whose frontmost is true")
        .output()
        .ok()?;

    if output.status.success() {
        let app_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !app_id.is_empty() {
            return Some(FocusTarget {
                app_id,
                id_type: AppIdentifierType::BundleId,
            });
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn platform_restore_focus(target: &FocusTarget) -> bool {
    use std::process::Command;
    use std::time::{Duration, Instant};

    // `activate` is asynchronous: it returns before the target app is actually
    // frontmost (especially across a Space switch). Poll until it is, or text
    // insertion will type into whatever still holds keyboard focus.
    let deadline = Instant::now() + Duration::from_millis(1500);
    loop {
        if platform_capture_focus().is_some_and(|f| f.app_id == target.app_id) {
            return true;
        }
        if Instant::now() >= deadline {
            warn!(app_id = %target.app_id, "Target app did not become frontmost before deadline");
            return false;
        }

        let script = format!("tell application id \"{}\" to activate", target.app_id);
        let activated = Command::new("osascript")
            .arg("-e")
            .arg(&script)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if !activated {
            warn!(app_id = %target.app_id, "osascript activate failed");
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

// --- Windows implementation ---

#[cfg(target_os = "windows")]
fn platform_capture_focus() -> Option<FocusTarget> {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_null() {
        warn!("GetForegroundWindow returned null");
        return None;
    }

    // Store the HWND as a string so it can be used for restore
    let hwnd_val = hwnd as usize;
    debug!(hwnd = hwnd_val, "Captured foreground window");
    Some(FocusTarget {
        app_id: hwnd_val.to_string(),
        id_type: AppIdentifierType::WindowHandle,
    })
}

#[cfg(target_os = "windows")]
fn platform_restore_focus(target: &FocusTarget) -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

    let hwnd: usize = match target.app_id.parse() {
        Ok(h) => h,
        Err(_) => {
            warn!(app_id = %target.app_id, "Invalid HWND for focus restore");
            return false;
        }
    };

    let result = unsafe { SetForegroundWindow(hwnd as windows_sys::Win32::Foundation::HWND) };
    if result == 0 {
        warn!(hwnd = hwnd, "SetForegroundWindow failed");
        false
    } else {
        debug!(hwnd = hwnd, "Restored foreground window");
        true
    }
}

// --- Linux implementation ---

#[cfg(target_os = "linux")]
fn platform_capture_focus() -> Option<FocusTarget> {
    use std::process::Command;

    // Try xdotool for X11
    let output = Command::new("xdotool")
        .arg("getactivewindow")
        .output()
        .ok()?;

    if output.status.success() {
        let window_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !window_id.is_empty() {
            return Some(FocusTarget {
                app_id: window_id,
                id_type: AppIdentifierType::WmClass,
            });
        }
    }

    // Wayland: focus detection is limited
    warn!("Could not detect focused window (Wayland does not support this)");
    None
}

#[cfg(target_os = "linux")]
fn platform_restore_focus(target: &FocusTarget) -> bool {
    use std::process::Command;

    Command::new("xdotool")
        .arg("windowactivate")
        .arg(&target.app_id)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_target_debug() {
        let target = FocusTarget {
            app_id: "com.example.app".to_string(),
            id_type: crate::db::profiles::AppIdentifierType::BundleId,
        };
        let debug = format!("{target:?}");
        assert!(debug.contains("com.example.app"));
    }
}
