use std::time::Instant;
use tracing::{debug, info, warn};

/// Result of selection detection.
#[derive(Debug, Clone, PartialEq)]
pub enum SelectionState {
    /// Text was selected in the target application.
    Selected(String),
    /// No selection detected.
    NoSelection,
    /// Detection was inconclusive (secure field, timeout, etc.).
    Unknown,
}

/// Detect if the target application has text selected.
///
/// Method: save clipboard, simulate Cmd/Ctrl+C, check if clipboard changed.
/// Restores the original clipboard afterwards.
///
/// Disabled on Wayland (always returns `Unknown`).
pub fn detect_selection() -> SelectionState {
    #[cfg(target_os = "linux")]
    {
        if std::env::var("WAYLAND_DISPLAY").is_ok() {
            debug!("Wayland detected: skipping selection detection");
            return SelectionState::Unknown;
        }
    }

    let start = Instant::now();

    let result = detect_selection_inner();

    let elapsed = start.elapsed();
    info!(
        elapsed_ms = elapsed.as_millis(),
        result = ?result,
        "Selection detection complete"
    );

    result
}

fn detect_selection_inner() -> SelectionState {
    use arboard::Clipboard;
    use enigo::{Direction, Enigo, Key, Keyboard, Settings};

    // Save current clipboard
    let mut clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(e) => {
            warn!(%e, "Failed to open clipboard for selection detection");
            return SelectionState::Unknown;
        }
    };

    let previous_text = clipboard.get_text().ok();

    // Set a sentinel value so we can detect if Cmd/Ctrl+C changed the clipboard
    let sentinel = format!("__echotype_sentinel_{}", std::process::id());
    if clipboard.set_text(&sentinel).is_err() {
        return SelectionState::Unknown;
    }

    // Brief delay for clipboard to settle
    std::thread::sleep(std::time::Duration::from_millis(20));

    // Simulate Cmd/Ctrl+C
    let mut enigo = match Enigo::new(&Settings::default()) {
        Ok(e) => e,
        Err(_) => {
            // Restore clipboard
            restore_clipboard(&mut clipboard, previous_text.as_deref());
            return SelectionState::Unknown;
        }
    };

    #[cfg(target_os = "macos")]
    let modifier = Key::Meta;
    #[cfg(not(target_os = "macos"))]
    let modifier = Key::Control;

    let copy_ok = enigo.key(modifier, Direction::Press).is_ok()
        && enigo.key(Key::Unicode('c'), Direction::Click).is_ok()
        && enigo.key(modifier, Direction::Release).is_ok();

    if !copy_ok {
        restore_clipboard(&mut clipboard, previous_text.as_deref());
        return SelectionState::Unknown;
    }

    // Wait for the copy to take effect
    std::thread::sleep(std::time::Duration::from_millis(50));

    // Check clipboard
    let new_text = clipboard.get_text().ok();

    // Restore original clipboard
    restore_clipboard(&mut clipboard, previous_text.as_deref());

    match new_text {
        Some(ref text) if text == &sentinel => {
            // Clipboard unchanged — no selection
            SelectionState::NoSelection
        }
        Some(text) if !text.is_empty() => {
            // Clipboard changed to text — selection detected
            debug!(selection_len = text.len(), "Selection detected");
            SelectionState::Selected(text)
        }
        _ => {
            // Clipboard became empty or non-text
            SelectionState::Unknown
        }
    }
}

fn restore_clipboard(clipboard: &mut arboard::Clipboard, previous: Option<&str>) {
    if let Some(text) = previous {
        if let Err(e) = clipboard.set_text(text) {
            warn!(%e, "Failed to restore clipboard after selection detection");
        }
    }
    // If previous was None, the clipboard had non-text content (images, files, etc).
    // Don't overwrite it with an empty string — the sentinel was already replaced by
    // the copy operation or will be harmless if it remains.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_state_debug() {
        let s = SelectionState::NoSelection;
        assert_eq!(format!("{s:?}"), "NoSelection");

        let s = SelectionState::Selected("hello".to_string());
        assert!(format!("{s:?}").contains("hello"));

        let s = SelectionState::Unknown;
        assert_eq!(format!("{s:?}"), "Unknown");
    }
}
