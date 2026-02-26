use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Status of OS-level permissions required for full functionality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionStatus {
    /// macOS Accessibility permission (required for direct keyboard input).
    pub accessibility: bool,
}

/// Check current permission status.
pub fn check_permissions() -> PermissionStatus {
    let status = PermissionStatus {
        accessibility: check_accessibility(),
    };

    if status.accessibility {
        info!("Accessibility permission: granted");
    } else {
        warn!("Accessibility permission: not granted — falling back to clipboard-only mode");
    }

    status
}

#[cfg(target_os = "macos")]
fn check_accessibility() -> bool {
    // Use the CoreGraphics framework to check accessibility trust.
    // AXIsProcessTrustedWithOptions is in ApplicationServices framework.
    // We shell out to avoid linking complexity for now.
    use std::process::Command;

    let output = Command::new("osascript")
        .arg("-e")
        .arg("tell application \"System Events\" to return (exists process 1)")
        .output();

    matches!(output, Ok(o) if o.status.success())
}

#[cfg(not(target_os = "macos"))]
fn check_accessibility() -> bool {
    // On Windows and Linux, accessibility-style permissions are not typically required.
    // Direct input via enigo works without special permissions on these platforms.
    // Linux Wayland is handled at the output method level (clipboard-only).
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn permission_status_serializes() {
        let status = PermissionStatus {
            accessibility: true,
        };
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"accessibility\":true"));
    }
}
