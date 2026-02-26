use std::fs;
use std::path::PathBuf;

use serde::Serialize;

use crate::audio::capture;
use crate::platform::permissions;

/// Collected diagnostic information for bug reports.
#[derive(Debug, Serialize)]
pub struct DiagnosticInfo {
    pub version: String,
    pub platform: String,
    pub arch: String,
    pub flavor: String,
    pub os_version: String,
    pub permissions: PermissionInfo,
    pub audio_devices: Vec<String>,
    pub default_audio_device: Option<String>,
    pub active_model: Option<String>,
    pub engine_type: String,
    pub log_tail: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct PermissionInfo {
    pub microphone: bool,
    pub accessibility: bool,
}

/// Collect diagnostic info that doesn't require AppState (for CLI).
pub fn collect_basic() -> DiagnosticInfo {
    let meta = crate::updater::build_meta();
    let perms = permissions::check_permissions();
    let devices = capture::list_devices();

    let default_device = devices
        .iter()
        .find(|d| d.is_default)
        .map(|d| d.name.clone());

    let device_names: Vec<String> = devices.into_iter().map(|d| d.name).collect();

    DiagnosticInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: meta.platform.to_string(),
        arch: meta.arch.to_string(),
        flavor: meta.flavor.to_string(),
        os_version: get_os_version(),
        permissions: PermissionInfo {
            microphone: perms.microphone,
            accessibility: perms.accessibility,
        },
        audio_devices: device_names,
        default_audio_device: default_device,
        active_model: None,
        engine_type: "unknown".to_string(),
        log_tail: read_log_tail(20),
    }
}

/// Format diagnostic info as a Markdown-compatible text block for pasting
/// into GitHub issues.
pub fn format_markdown(info: &DiagnosticInfo) -> String {
    let mut out = String::new();

    out.push_str("## EchoType Diagnostic Info\n\n");
    out.push_str(&format!("- **Version:** {}\n", info.version));
    out.push_str(&format!("- **Platform:** {}\n", info.platform));
    out.push_str(&format!("- **Arch:** {}\n", info.arch));
    out.push_str(&format!("- **Flavor:** {}\n", info.flavor));
    out.push_str(&format!("- **OS Version:** {}\n", info.os_version));
    out.push_str(&format!(
        "- **Permissions:** mic={}, accessibility={}\n",
        if info.permissions.microphone {
            "granted"
        } else {
            "denied"
        },
        if info.permissions.accessibility {
            "granted"
        } else {
            "denied"
        },
    ));

    out.push_str(&format!(
        "- **Audio Device:** {}\n",
        info.default_audio_device
            .as_deref()
            .unwrap_or("(none detected)")
    ));

    if info.audio_devices.len() > 1 {
        out.push_str(&format!(
            "- **All Audio Devices:** {}\n",
            info.audio_devices.join(", ")
        ));
    }

    out.push_str(&format!(
        "- **Active Model:** {}\n",
        info.active_model.as_deref().unwrap_or("(none)")
    ));
    out.push_str(&format!("- **Engine Type:** {}\n", info.engine_type));

    if !info.log_tail.is_empty() {
        out.push_str("\n### Recent Log Lines\n\n```\n");
        for line in &info.log_tail {
            // Redact file paths that may contain usernames
            let redacted = redact_paths(line);
            out.push_str(&redacted);
            out.push('\n');
        }
        out.push_str("```\n");
    }

    out
}

fn get_os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("sw_vers")
            .arg("-productVersion")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| format!("macOS {}", s.trim()))
            .unwrap_or_else(|| "macOS (unknown version)".to_string())
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "ver"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "Windows (unknown version)".to_string())
    }
    #[cfg(target_os = "linux")]
    {
        std::fs::read_to_string("/etc/os-release")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|l| l.starts_with("PRETTY_NAME="))
                    .map(|l| {
                        l.trim_start_matches("PRETTY_NAME=")
                            .trim_matches('"')
                            .to_string()
                    })
            })
            .unwrap_or_else(|| "Linux (unknown distro)".to_string())
    }
}

fn log_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.echotype.app")
        .join("logs")
}

fn read_log_tail(max_lines: usize) -> Vec<String> {
    use std::io::{Read, Seek, SeekFrom};

    let dir = log_dir();
    // Find the most recent log file
    let mut log_files: Vec<_> = fs::read_dir(&dir)
        .ok()
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "log"))
        .collect();

    log_files.sort_by_key(|e| {
        e.metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });

    let Some(latest) = log_files.last() else {
        return vec![];
    };

    // Read only the last 64KB to avoid loading huge log files
    let Ok(mut file) = fs::File::open(latest.path()) else {
        return vec![];
    };
    let Ok(metadata) = file.metadata() else {
        return vec![];
    };
    let file_size = metadata.len();
    const MAX_READ: u64 = 64 * 1024;
    if file_size > MAX_READ {
        let _ = file.seek(SeekFrom::End(-(MAX_READ as i64)));
    }
    let mut content = String::new();
    if file.read_to_string(&mut content).is_err() {
        return vec![];
    }

    let lines: Vec<&str> = content.lines().collect();
    // If we seeked into the middle of a line, skip the first (partial) line
    let skip = if file_size > MAX_READ && !lines.is_empty() {
        1
    } else {
        0
    };
    let available = &lines[skip..];
    let start = available.len().saturating_sub(max_lines);
    available[start..].iter().map(|s| s.to_string()).collect()
}

/// Redact file paths that may contain usernames.
/// Replaces /Users/<name>/ or /home/<name>/ with /<REDACTED>/
/// and C:\Users\<name>\ with C:\<REDACTED>\
fn redact_paths(line: &str) -> String {
    let mut result = line.to_string();

    // macOS/Linux: /Users/<name>/ or /home/<name>/
    for prefix in ["/Users/", "/home/"] {
        while let Some(start) = result.find(prefix) {
            let after_prefix = start + prefix.len();
            // Find the end of the username: next '/' or whitespace
            let end = result[after_prefix..]
                .find(|c: char| c == '/' || c.is_whitespace())
                .map(|i| {
                    // Include the trailing '/' but not whitespace
                    if result.as_bytes()[after_prefix + i] == b'/' {
                        after_prefix + i + 1
                    } else {
                        after_prefix + i
                    }
                })
                .unwrap_or(result.len());
            result.replace_range(start..end, "/<REDACTED>/");
        }
    }

    // Windows: C:\Users\<name>\ (case-insensitive, handle multiple occurrences)
    loop {
        let lower = result.to_lowercase();
        let Some(start) = lower.find("c:\\users\\") else {
            break;
        };
        let after_prefix = start + "c:\\users\\".len();
        let end = result[after_prefix..]
            .find(|c: char| c == '\\' || c.is_whitespace())
            .map(|i| {
                if result.as_bytes()[after_prefix + i] == b'\\' {
                    after_prefix + i + 1
                } else {
                    after_prefix + i
                }
            })
            .unwrap_or(result.len());
        result.replace_range(start..end, "C:\\<REDACTED>\\");
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_basic_succeeds() {
        let info = collect_basic();
        assert!(!info.version.is_empty());
        assert!(!info.platform.is_empty());
    }

    #[test]
    fn format_markdown_contains_version() {
        let info = collect_basic();
        let md = format_markdown(&info);
        assert!(md.contains(&info.version));
        assert!(md.contains("EchoType Diagnostic Info"));
    }

    #[test]
    fn redact_paths_unix() {
        let input = "/Users/john/Library/app.log";
        let result = redact_paths(input);
        assert_eq!(result, "/<REDACTED>/Library/app.log");
        assert!(!result.contains("john"));
    }

    #[test]
    fn redact_paths_windows() {
        let input = r"C:\Users\john\AppData\log.txt";
        let result = redact_paths(input);
        assert!(result.contains("<REDACTED>"));
        assert!(!result.contains("john"));
    }

    #[test]
    fn redact_paths_no_match() {
        let input = "INFO echotype::engine started";
        let result = redact_paths(input);
        assert_eq!(result, input);
    }

    #[test]
    fn redact_multiple_unix_paths() {
        let input = "/Users/alice/foo and /home/bob/bar";
        let result = redact_paths(input);
        assert!(!result.contains("alice"));
        assert!(!result.contains("bob"));
        assert!(result.contains("foo"));
        assert!(result.contains("bar"));
    }

    #[test]
    fn redact_path_without_trailing_slash() {
        let input = "path is /Users/john then more text";
        let result = redact_paths(input);
        assert!(!result.contains("john"));
        assert!(result.contains("more text"));
    }

    #[test]
    fn redact_multiple_windows_paths() {
        let input = r"C:\Users\alice\foo and C:\Users\bob\bar";
        let result = redact_paths(input);
        assert!(!result.contains("alice"));
        assert!(!result.contains("bob"));
    }
}
