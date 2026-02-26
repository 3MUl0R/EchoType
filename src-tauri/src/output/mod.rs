use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

/// Method for inserting transcribed text.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputMethod {
    /// Type text directly via simulated keystrokes (requires Accessibility on macOS).
    #[default]
    DirectInput,
    /// Copy to clipboard, then simulate paste keystroke.
    ClipboardPaste,
    /// Copy to clipboard only; user pastes manually.
    ClipboardOnly,
}

/// Insert text using the specified output method.
pub fn insert_text(text: &str, method: &OutputMethod) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }

    info!(
        method = ?method,
        text_len = text.len(),
        "Inserting text"
    );

    // Brief delay to let focus settle after window activation
    std::thread::sleep(std::time::Duration::from_millis(50));

    match method {
        OutputMethod::DirectInput => insert_direct(text),
        OutputMethod::ClipboardPaste => insert_clipboard_paste(text),
        OutputMethod::ClipboardOnly => insert_clipboard_only(text),
    }
}

/// Type text directly via simulated keystrokes.
fn insert_direct(text: &str) -> Result<(), String> {
    use enigo::{Enigo, Keyboard, Settings};

    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| format!("Failed to init enigo: {e}"))?;

    enigo
        .text(text)
        .map_err(|e| format!("Failed to type text: {e}"))?;

    debug!(text_len = text.len(), "Text typed via direct input");
    Ok(())
}

/// Copy text to clipboard and simulate paste.
fn insert_clipboard_paste(text: &str) -> Result<(), String> {
    use arboard::Clipboard;
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    let mut clipboard = Clipboard::new().map_err(|e| format!("Failed to open clipboard: {e}"))?;

    // Save current clipboard contents for potential restore
    let previous = clipboard.get_text().ok();

    // Set text on clipboard
    clipboard
        .set_text(text)
        .map_err(|e| format!("Failed to set clipboard: {e}"))?;

    // Brief delay for clipboard to settle
    std::thread::sleep(std::time::Duration::from_millis(30));

    // Simulate paste keystroke
    let mut enigo =
        Enigo::new(&Settings::default()).map_err(|e| format!("Failed to init enigo: {e}"))?;

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    enigo
        .key(modifier, Direction::Press)
        .map_err(|e| format!("Failed to press modifier: {e}"))?;
    enigo
        .key(Key::Unicode('v'), Direction::Click)
        .map_err(|e| format!("Failed to press V: {e}"))?;
    enigo
        .key(modifier, Direction::Release)
        .map_err(|e| format!("Failed to release modifier: {e}"))?;

    debug!("Text pasted via clipboard");

    // Conditional clipboard restore: only restore if clipboard still has our text
    std::thread::sleep(std::time::Duration::from_millis(100));
    if let Some(prev) = previous {
        if let Ok(current) = clipboard.get_text() {
            if current == text {
                if let Err(e) = clipboard.set_text(&prev) {
                    warn!(%e, "Failed to restore previous clipboard");
                }
            }
        }
    }

    Ok(())
}

/// Copy text to clipboard only (user pastes manually).
fn insert_clipboard_only(text: &str) -> Result<(), String> {
    use arboard::Clipboard;

    let mut clipboard = Clipboard::new().map_err(|e| format!("Failed to open clipboard: {e}"))?;

    clipboard
        .set_text(text)
        .map_err(|e| format!("Failed to set clipboard: {e}"))?;

    info!("Text copied to clipboard (paste manually)");
    Ok(())
}

/// Determine the best output method based on platform and permissions.
pub fn auto_select_method() -> OutputMethod {
    let permissions = crate::platform::permissions::check_permissions();

    #[cfg(target_os = "linux")]
    {
        // Check if running under Wayland
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            info!("Wayland detected: using clipboard-only mode");
            return OutputMethod::ClipboardOnly;
        }
    }

    if permissions.accessibility {
        OutputMethod::DirectInput
    } else {
        info!("No accessibility permission: using clipboard-only mode");
        OutputMethod::ClipboardOnly
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_method_default_is_direct() {
        assert_eq!(OutputMethod::default(), OutputMethod::DirectInput);
    }

    #[test]
    fn output_method_serializes() {
        let json = serde_json::to_string(&OutputMethod::ClipboardOnly).unwrap();
        assert_eq!(json, "\"clipboard_only\"");
    }

    #[test]
    fn insert_empty_text_is_ok() {
        assert!(insert_text("", &OutputMethod::DirectInput).is_ok());
    }
}
