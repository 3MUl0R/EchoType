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

/// Run an AppleScript with a hard timeout, killing osascript if it hangs
/// (Apple Events to a busy process can block indefinitely). Returns the
/// trimmed stdout on success, None on failure or timeout.
#[cfg(target_os = "macos")]
fn run_osascript(script: &str, timeout: std::time::Duration) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    use std::time::Instant;

    let mut child = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return None;
                }
                let mut out = String::new();
                if let Some(mut stdout) = child.stdout.take() {
                    let _ = stdout.read_to_string(&mut out);
                }
                return Some(out.trim().to_string());
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    warn!("osascript timed out; killing");
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

#[cfg(target_os = "macos")]
fn platform_capture_focus() -> Option<FocusTarget> {
    // Use osascript to get the frontmost app bundle ID
    let app_id = run_osascript(
        "tell application \"System Events\" to get bundle identifier of first process whose frontmost is true",
        std::time::Duration::from_millis(1000),
    )?;

    if !app_id.is_empty() {
        return Some(FocusTarget {
            app_id,
            id_type: AppIdentifierType::BundleId,
        });
    }

    None
}

#[cfg(target_os = "macos")]
fn platform_restore_focus(target: &FocusTarget) -> bool {
    use std::time::{Duration, Instant};

    // Always activate, even when the target already reports frontmost:
    // "frontmost" is app-level, but keyboard focus (the key window) can sit
    // elsewhere — activation re-keys the target's window, like the user
    // clicking back into it. Then poll: `activate` is asynchronous and
    // returns before the app is actually frontmost (especially across a
    // Space switch).
    let deadline = Instant::now() + Duration::from_millis(1500);
    loop {
        let script = format!("tell application id \"{}\" to activate", target.app_id);
        if run_osascript(&script, Duration::from_millis(1000)).is_none() {
            warn!(app_id = %target.app_id, "osascript activate failed");
            return false;
        }

        // Activation is asynchronous; give the window server time to re-key
        // the target's window before trusting the frontmost check — an app
        // can report frontmost while its window is not yet key again.
        std::thread::sleep(Duration::from_millis(100));

        if platform_capture_focus().is_some_and(|f| f.app_id == target.app_id) {
            return true;
        }
        if Instant::now() >= deadline {
            warn!(app_id = %target.app_id, "Target app did not become frontmost before deadline");
            return false;
        }
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
