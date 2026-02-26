use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Manifest version we support.
const SUPPORTED_VERSION: u32 = 1;

/// Bundled manifest JSON (compiled into the binary).
const BUNDLED_MANIFEST: &str = include_str!("../../../models/manifest.json");

/// Allowlisted hosts for model download URLs.
const ALLOWED_HOSTS: &[&str] = &["huggingface.co"];

/// The model catalog manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub models: Vec<ModelEntry>,
}

/// Speed tier label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeedTier {
    Fast,
    Moderate,
    Slow,
}

/// Accuracy tier label.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccuracyTier {
    Basic,
    Good,
    Best,
}

/// A single model entry in the manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelEntry {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub file: String,
    pub size_bytes: u64,
    pub sha256: String,
    pub url: String,
    pub languages: Vec<String>,
    pub speed_tier: SpeedTier,
    pub accuracy_tier: AccuracyTier,
    pub description: String,
}

/// Load the bundled manifest.
pub fn load_bundled() -> Result<Manifest, String> {
    let manifest: Manifest =
        serde_json::from_str(BUNDLED_MANIFEST).map_err(|e| format!("Invalid manifest: {e}"))?;

    validate_manifest(&manifest)?;
    info!(
        version = manifest.version,
        model_count = manifest.models.len(),
        "Loaded bundled model manifest"
    );
    Ok(manifest)
}

/// Parse a manifest from JSON string (e.g., fetched from remote).
#[allow(dead_code)]
pub fn parse_manifest(json: &str) -> Result<Manifest, String> {
    let manifest: Manifest =
        serde_json::from_str(json).map_err(|e| format!("Invalid manifest JSON: {e}"))?;
    validate_manifest(&manifest)?;
    Ok(manifest)
}

/// Validate manifest integrity.
fn validate_manifest(manifest: &Manifest) -> Result<(), String> {
    if manifest.version != SUPPORTED_VERSION {
        return Err(format!(
            "Unsupported manifest version {} (expected {})",
            manifest.version, SUPPORTED_VERSION
        ));
    }

    let mut seen_ids = std::collections::HashSet::new();
    let mut seen_files = std::collections::HashSet::new();

    for model in &manifest.models {
        validate_model_entry(model)?;

        if !seen_ids.insert(&model.id) {
            return Err(format!("Duplicate model ID: {}", model.id));
        }
        if !seen_files.insert(&model.file) {
            return Err(format!(
                "Duplicate model filename '{}' (model '{}')",
                model.file, model.id
            ));
        }
    }

    Ok(())
}

/// Validate a single model entry for security and correctness.
fn validate_model_entry(model: &ModelEntry) -> Result<(), String> {
    // ID must be non-empty
    if model.id.is_empty() {
        return Err("Model entry has empty ID".to_string());
    }

    // File must be a plain filename — reject path traversal, absolute paths,
    // backslashes (Windows), colons (drive letters), and null bytes.
    if model.file.contains("..")
        || model.file.contains('\0')
        || model.file.starts_with('/')
        || model.file.contains('\\')
        || model.file.contains(':')
    {
        return Err(format!(
            "Model '{}' has unsafe filename: {}",
            model.id, model.file
        ));
    }

    // URL must be HTTPS
    if !model.url.starts_with("https://") {
        return Err(format!(
            "Model '{}' URL must be HTTPS: {}",
            model.id, model.url
        ));
    }

    // URL must be from an allowlisted host
    if let Some(host) = extract_host(&model.url) {
        if !is_host_allowed(&host) {
            return Err(format!(
                "Model '{}' URL host '{}' is not in the allowlist",
                model.id, host
            ));
        }
    } else {
        return Err(format!(
            "Model '{}' has invalid URL: {}",
            model.id, model.url
        ));
    }

    // SHA-256 must be 64 hex chars
    if model.sha256.len() != 64 || !model.sha256.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("Model '{}' has invalid SHA-256 hash", model.id));
    }

    // Size must be positive
    if model.size_bytes == 0 {
        return Err(format!("Model '{}' has zero size", model.id));
    }

    Ok(())
}

/// Extract the host from a URL, rejecting URLs with userinfo (`@`).
fn extract_host(url: &str) -> Option<String> {
    let without_scheme = url.strip_prefix("https://")?;
    let authority = without_scheme.split('/').next()?;

    // Reject userinfo in URLs (e.g. "https://user:pass@evil.com/file")
    // — this prevents allowlist bypass via crafted URLs like
    // "https://huggingface.co:443@evil.com/file"
    if authority.contains('@') {
        return None;
    }

    let host = authority.split(':').next()?;
    if host.is_empty() {
        return None;
    }
    Some(host.to_string())
}

/// Check if a host matches the allowlist (exact match or subdomain).
pub fn is_host_allowed(host: &str) -> bool {
    ALLOWED_HOSTS
        .iter()
        .any(|&allowed| host == allowed || host.ends_with(&format!(".{allowed}")))
}

/// Merge a remote manifest with the bundled one.
///
/// Uses the remote catalog for freshness but preserves bundled checksums
/// for already-known model IDs (prevents silent replacement).
#[allow(dead_code)]
pub fn merge_manifests(bundled: &Manifest, remote: &Manifest) -> Manifest {
    let mut merged = bundled.clone();

    for remote_model in &remote.models {
        if let Some(existing) = merged.models.iter().find(|m| m.id == remote_model.id) {
            if existing.sha256 != remote_model.sha256 {
                warn!(
                    model_id = %remote_model.id,
                    "Remote manifest has different checksum for existing model — treating as update"
                );
                // Don't silently replace; the update flow will handle this
            }
        } else {
            // New model not in bundled manifest — add it
            info!(model_id = %remote_model.id, "Adding new model from remote manifest");
            merged.models.push(remote_model.clone());
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_manifest_loads() {
        let manifest = load_bundled().expect("Bundled manifest should load");
        assert_eq!(manifest.version, 1);
        assert!(!manifest.models.is_empty());
    }

    #[test]
    fn all_models_have_valid_urls() {
        let manifest = load_bundled().unwrap();
        for model in &manifest.models {
            assert!(
                model.url.starts_with("https://"),
                "{} URL not HTTPS",
                model.id
            );
            let host = extract_host(&model.url).expect("URL should have host");
            assert!(
                is_host_allowed(&host),
                "{} host {} not allowlisted",
                model.id,
                host
            );
        }
    }

    #[test]
    fn all_models_have_valid_sha256() {
        let manifest = load_bundled().unwrap();
        for model in &manifest.models {
            assert_eq!(model.sha256.len(), 64, "{} sha256 wrong length", model.id);
            assert!(
                model.sha256.chars().all(|c| c.is_ascii_hexdigit()),
                "{} sha256 not hex",
                model.id
            );
        }
    }

    #[test]
    fn rejects_path_traversal() {
        let mut model = dummy_model();
        model.file = "../../../etc/passwd".to_string();
        assert!(validate_model_entry(&model).is_err());
    }

    #[test]
    fn rejects_absolute_path() {
        let mut model = dummy_model();
        model.file = "/etc/passwd".to_string();
        assert!(validate_model_entry(&model).is_err());
    }

    #[test]
    fn rejects_http_url() {
        let mut model = dummy_model();
        model.url = "http://huggingface.co/model.bin".to_string();
        assert!(validate_model_entry(&model).is_err());
    }

    #[test]
    fn rejects_non_allowlisted_host() {
        let mut model = dummy_model();
        model.url = "https://evil.com/model.bin".to_string();
        assert!(validate_model_entry(&model).is_err());
    }

    #[test]
    fn accepts_valid_model() {
        let model = dummy_model();
        assert!(validate_model_entry(&model).is_ok());
    }

    #[test]
    fn merge_adds_new_models() {
        let bundled = load_bundled().unwrap();
        let mut remote = bundled.clone();
        remote.models.push(ModelEntry {
            id: "whisper-new-model".to_string(),
            ..dummy_model()
        });

        let merged = merge_manifests(&bundled, &remote);
        assert!(merged.models.iter().any(|m| m.id == "whisper-new-model"));
        assert_eq!(merged.models.len(), bundled.models.len() + 1);
    }

    #[test]
    fn rejects_backslash_filename() {
        let mut model = dummy_model();
        model.file = "..\\etc\\passwd".to_string();
        assert!(validate_model_entry(&model).is_err());
    }

    #[test]
    fn rejects_colon_filename() {
        let mut model = dummy_model();
        model.file = "C:model.bin".to_string();
        assert!(validate_model_entry(&model).is_err());
    }

    #[test]
    fn rejects_userinfo_url_bypass() {
        let mut model = dummy_model();
        model.url = "https://huggingface.co:443@evil.com/model.bin".to_string();
        assert!(validate_model_entry(&model).is_err());
    }

    #[test]
    fn rejects_userinfo_url_bypass_no_port() {
        let mut model = dummy_model();
        model.url = "https://huggingface.co@evil.com/model.bin".to_string();
        assert!(validate_model_entry(&model).is_err());
    }

    #[test]
    fn rejects_duplicate_ids() {
        let model = dummy_model();
        let manifest = Manifest {
            version: 1,
            models: vec![model.clone(), model],
        };
        assert!(validate_manifest(&manifest).is_err());
    }

    #[test]
    fn extract_host_works() {
        assert_eq!(
            extract_host("https://huggingface.co/some/path"),
            Some("huggingface.co".to_string())
        );
        assert_eq!(
            extract_host("https://cdn.huggingface.co/file"),
            Some("cdn.huggingface.co".to_string())
        );
        assert_eq!(extract_host("not-a-url"), None);
    }

    #[test]
    fn extract_host_rejects_userinfo() {
        assert_eq!(extract_host("https://user:pass@evil.com/file"), None);
        assert_eq!(
            extract_host("https://huggingface.co:443@evil.com/file"),
            None
        );
    }

    fn dummy_model() -> ModelEntry {
        ModelEntry {
            id: "test-model".to_string(),
            name: "Test Model".to_string(),
            engine: "whisper".to_string(),
            file: "ggml-test.bin".to_string(),
            size_bytes: 1000,
            sha256: "a".repeat(64),
            url: "https://huggingface.co/test/model.bin".to_string(),
            languages: vec!["en".to_string()],
            speed_tier: SpeedTier::Fast,
            accuracy_tier: AccuracyTier::Basic,
            description: "Test model".to_string(),
        }
    }
}
