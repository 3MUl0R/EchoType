use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Status of OS-level permissions required for full functionality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionStatus {
    /// macOS Accessibility permission (required for direct keyboard input).
    pub accessibility: bool,
    /// Microphone permission (required for audio capture).
    pub microphone: bool,
}

/// Check current permission status.
pub fn check_permissions() -> PermissionStatus {
    let status = PermissionStatus {
        accessibility: check_accessibility(),
        microphone: check_microphone(),
    };

    if status.accessibility {
        info!("Accessibility permission: granted");
    } else {
        warn!("Accessibility permission: not granted — falling back to clipboard-only mode");
    }

    if status.microphone {
        info!("Microphone permission: granted");
    } else {
        warn!("Microphone permission: not granted — recording will be unavailable");
    }

    status
}

/// Open the system settings pane for the given permission.
pub fn open_permission_settings(permission: &str) -> Result<(), String> {
    match permission {
        "accessibility" => open_accessibility_settings(),
        "microphone" => open_microphone_settings(),
        _ => Err(format!("Unknown permission: {permission}")),
    }
}

#[cfg(target_os = "macos")]
fn check_accessibility() -> bool {
    use std::process::Command;

    let output = Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to return (exists process 1)")
        .output();

    matches!(output, Ok(o) if o.status.success())
}

#[cfg(not(target_os = "macos"))]
fn check_accessibility() -> bool {
    true
}

fn check_microphone() -> bool {
    // Check if we can enumerate input devices. If we can find at least one,
    // it means we have permission to access the microphone subsystem.
    use cpal::traits::{DeviceTrait, HostTrait};

    let host = cpal::default_host();

    // Try to get the default input device — this will fail if permission is denied
    match host.default_input_device() {
        Some(device) => {
            // Try to get supported configs — further validates access
            device.supported_input_configs().is_ok()
        }
        None => {
            // No default device could mean no mic or denied permission
            // Check if any input devices exist at all
            host.input_devices()
                .map(|mut d| d.next().is_some())
                .unwrap_or(false)
        }
    }
}

#[cfg(target_os = "macos")]
fn open_accessibility_settings() -> Result<(), String> {
    use std::process::Command;
    Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
        .spawn()
        .map_err(|e| format!("Failed to open Accessibility settings: {e}"))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn open_accessibility_settings() -> Result<(), String> {
    Ok(()) // No-op on non-macOS
}

#[cfg(target_os = "macos")]
fn open_microphone_settings() -> Result<(), String> {
    use std::process::Command;
    Command::new("open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        .spawn()
        .map_err(|e| format!("Failed to open Microphone settings: {e}"))?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn open_microphone_settings() -> Result<(), String> {
    Ok(()) // No-op on non-macOS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_status_serializes() {
        let status = PermissionStatus {
            accessibility: true,
            microphone: true,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"accessibility\":true"));
        assert!(json.contains("\"microphone\":true"));
    }

    #[test]
    fn open_unknown_permission_returns_error() {
        assert!(open_permission_settings("unknown").is_err());
    }
}
