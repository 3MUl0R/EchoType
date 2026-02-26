use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Update behavior preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateBehavior {
    /// Download and install silently on next restart.
    AutoUpdate,
    /// Download in background, prompt to install (default).
    DownloadAndPrompt,
    /// Notify only; user decides when to download.
    NotifyOnly,
}

impl UpdateBehavior {
    pub fn from_str(s: &str) -> Self {
        match s {
            "auto_update" => Self::AutoUpdate,
            "notify_only" => Self::NotifyOnly,
            _ => Self::DownloadAndPrompt,
        }
    }
}

/// Information about an available update.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: Option<String>,
    pub date: Option<String>,
}

/// Build metadata compiled into the binary.
pub struct BuildMeta {
    pub platform: &'static str,
    pub arch: &'static str,
    pub flavor: &'static str,
}

/// Get the build metadata for this binary.
pub fn build_meta() -> BuildMeta {
    BuildMeta {
        platform: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        // Flavor is determined at build time via env var or default to "cpu"
        flavor: option_env!("ECHOTYPE_GPU_FLAVOR").unwrap_or("cpu"),
    }
}

/// Construct the expected update endpoint URL based on build metadata.
pub fn update_endpoint() -> String {
    let meta = build_meta();
    format!(
        "https://releases.echotype.app/{}-{}/{}/latest.json",
        meta.platform, meta.arch, meta.flavor
    )
}

/// Validate that an update manifest targets this binary's build.
/// Returns an error message if there's a mismatch.
pub fn validate_manifest_target(
    manifest_platform: &str,
    manifest_arch: &str,
    manifest_flavor: &str,
) -> Result<(), String> {
    let meta = build_meta();

    if manifest_platform != meta.platform {
        return Err(format!(
            "Platform mismatch: update is for '{}', running '{}'",
            manifest_platform, meta.platform
        ));
    }
    if manifest_arch != meta.arch {
        return Err(format!(
            "Architecture mismatch: update is for '{}', running '{}'",
            manifest_arch, meta.arch
        ));
    }
    if manifest_flavor != meta.flavor {
        return Err(format!(
            "Flavor mismatch: update is for '{}', running '{}'",
            manifest_flavor, meta.flavor
        ));
    }

    Ok(())
}

/// Get the currently configured update behavior from settings.
pub fn get_update_behavior(conn: &rusqlite::Connection) -> UpdateBehavior {
    let raw = crate::settings::get_typed::<String>(conn, crate::settings::keys::UPDATE_BEHAVIOR)
        .unwrap_or_else(|_| "download_and_prompt".to_string());
    UpdateBehavior::from_str(&raw)
}

/// Check for available updates.
///
/// This is a stub that will be connected to tauri-plugin-updater when the
/// plugin is vendored. For now it returns None (no update available).
pub async fn check_for_update() -> Option<UpdateInfo> {
    info!("Checking for updates (endpoint: {})", update_endpoint());

    // TODO: Integrate with tauri-plugin-updater when available.
    // For now, we log the check and return None.
    // The actual implementation will:
    // 1. Fetch the manifest from update_endpoint()
    // 2. Validate the manifest target via validate_manifest_target()
    // 3. Compare versions
    // 4. Return Some(UpdateInfo) if an update is available

    warn!("Update check skipped: tauri-plugin-updater not yet integrated");
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_meta_is_valid() {
        let meta = build_meta();
        assert!(!meta.platform.is_empty());
        assert!(!meta.arch.is_empty());
        assert!(!meta.flavor.is_empty());
    }

    #[test]
    fn update_endpoint_has_correct_format() {
        let url = update_endpoint();
        assert!(url.starts_with("https://"));
        assert!(url.ends_with("/latest.json"));
    }

    #[test]
    fn validate_manifest_rejects_wrong_platform() {
        let meta = build_meta();
        let result = validate_manifest_target("wrong-os", meta.arch, meta.flavor);
        assert!(result.is_err());
    }

    #[test]
    fn validate_manifest_rejects_wrong_arch() {
        let meta = build_meta();
        let result = validate_manifest_target(meta.platform, "wrong-arch", meta.flavor);
        assert!(result.is_err());
    }

    #[test]
    fn validate_manifest_rejects_wrong_flavor() {
        let meta = build_meta();
        let result = validate_manifest_target(meta.platform, meta.arch, "cuda");
        // Only fails if our flavor is not cuda (which it likely isn't in test)
        if meta.flavor != "cuda" {
            assert!(result.is_err());
        }
    }

    #[test]
    fn validate_manifest_accepts_matching_target() {
        let meta = build_meta();
        let result = validate_manifest_target(meta.platform, meta.arch, meta.flavor);
        assert!(result.is_ok());
    }

    #[test]
    fn update_behavior_from_str() {
        assert_eq!(
            UpdateBehavior::from_str("auto_update"),
            UpdateBehavior::AutoUpdate
        );
        assert_eq!(
            UpdateBehavior::from_str("download_and_prompt"),
            UpdateBehavior::DownloadAndPrompt
        );
        assert_eq!(
            UpdateBehavior::from_str("notify_only"),
            UpdateBehavior::NotifyOnly
        );
        assert_eq!(
            UpdateBehavior::from_str("unknown"),
            UpdateBehavior::DownloadAndPrompt
        );
    }
}
