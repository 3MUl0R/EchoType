use tracing::{debug, info, warn};

/// Represents the target window/app that was focused when dictation started.
#[derive(Debug, Clone)]
pub struct FocusTarget {
    /// Platform-specific identifier for the focused application.
    pub app_id: String,
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
            return Some(FocusTarget { app_id });
        }
    }

    None
}

#[cfg(target_os = "macos")]
fn platform_restore_focus(target: &FocusTarget) -> bool {
    use std::process::Command;

    let script = format!("tell application id \"{}\" to activate", target.app_id);

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// --- Windows implementation ---

#[cfg(target_os = "windows")]
fn platform_capture_focus() -> Option<FocusTarget> {
    // Windows: use GetForegroundWindow
    // Simplified: store window title as identifier
    warn!("Windows focus capture: using placeholder implementation");
    None
}

#[cfg(target_os = "windows")]
fn platform_restore_focus(_target: &FocusTarget) -> bool {
    warn!("Windows focus restore: using placeholder implementation");
    false
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
            return Some(FocusTarget { app_id: window_id });
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
        };
        let debug = format!("{target:?}");
        assert!(debug.contains("com.example.app"));
    }
}
