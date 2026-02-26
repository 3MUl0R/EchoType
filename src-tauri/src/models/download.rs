use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

use super::manifest::{is_host_allowed, ModelEntry};

/// Minimum free disk space margin beyond model size (50 MB).
const DISK_SPACE_MARGIN: u64 = 50 * 1024 * 1024;

/// Maximum age for stale partial files (24 hours in seconds).
const STALE_PARTIAL_SECS: u64 = 24 * 60 * 60;

/// Download progress event emitted to frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadProgress {
    pub model_id: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub status: DownloadStatus,
}

/// Status of a download.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadStatus {
    Downloading,
    Verifying,
    Complete,
    Failed,
    Cancelled,
}

/// Metadata sidecar for resumable downloads.
#[derive(Debug, Serialize, Deserialize)]
struct PartialMeta {
    etag: Option<String>,
    content_length: Option<u64>,
}

/// Manages model downloads with progress, resume, and cancellation.
pub struct DownloadManager {
    /// Set of model IDs currently being downloaded (prevents duplicates).
    active_downloads: Arc<Mutex<HashSet<String>>>,
    /// Cancellation flags keyed by model ID.
    cancel_flags: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

impl DownloadManager {
    pub fn new() -> Self {
        Self {
            active_downloads: Arc::new(Mutex::new(HashSet::new())),
            cancel_flags: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Get the models directory path.
    pub fn models_dir(app: &AppHandle) -> Result<PathBuf, String> {
        let app_data = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Cannot resolve app data dir: {e}"))?;
        Ok(app_data.join("models"))
    }

    /// Clean up stale partial files (older than 24 hours).
    pub async fn cleanup_stale_partials(models_dir: &Path) {
        let Ok(mut entries) = tokio::fs::read_dir(models_dir).await else {
            return;
        };

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("partial") {
                if let Ok(metadata) = tokio::fs::metadata(&path).await {
                    if let Ok(modified) = metadata.modified() {
                        if let Ok(age) = modified.elapsed() {
                            if age.as_secs() > STALE_PARTIAL_SECS {
                                info!(path = %path.display(), "Removing stale partial download");
                                let _ = tokio::fs::remove_file(&path).await;
                                let meta_path = path.with_extension("meta");
                                let _ = tokio::fs::remove_file(&meta_path).await;
                            }
                        }
                    }
                }
            }
        }
    }

    /// Start downloading a model. Returns immediately; progress is reported via events.
    pub async fn start_download(&self, app: &AppHandle, model: ModelEntry) -> Result<(), String> {
        let model_id = model.id.clone();

        // Prevent duplicate downloads
        {
            let active = self.active_downloads.lock().await;
            if active.contains(&model_id) {
                return Err(format!("Download already in progress for {model_id}"));
            }
        }

        // Run preflight checks before marking as active (so failures don't get stuck)
        let models_dir = Self::models_dir(app)?;
        tokio::fs::create_dir_all(&models_dir)
            .await
            .map_err(|e| format!("Cannot create models dir: {e}"))?;
        check_disk_space(&models_dir, model.size_bytes)?;

        // Now mark as active after preflight passed
        {
            let mut active = self.active_downloads.lock().await;
            if active.contains(&model_id) {
                return Err(format!("Download already in progress for {model_id}"));
            }
            active.insert(model_id.clone());
        }

        let cancelled = Arc::new(AtomicBool::new(false));
        {
            let mut flags = self.cancel_flags.lock().await;
            flags.insert(model_id.clone(), cancelled.clone());
        }

        let active_downloads = self.active_downloads.clone();
        let cancel_flags = self.cancel_flags.clone();
        let app_handle = app.clone();

        tokio::spawn(async move {
            let result = download_model_inner(&app_handle, &model, &models_dir, &cancelled).await;

            // Clean up tracking state
            {
                let mut active = active_downloads.lock().await;
                active.remove(&model_id);
            }
            {
                let mut flags = cancel_flags.lock().await;
                flags.remove(&model_id);
            }

            match result {
                Ok(()) => {
                    info!(model_id = %model_id, "Model download complete");
                }
                Err(e) => {
                    error!(model_id = %model_id, error = %e, "Model download failed");
                }
            }
        });

        Ok(())
    }

    /// Cancel an active download.
    pub async fn cancel_download(&self, model_id: &str) -> Result<(), String> {
        let flags = self.cancel_flags.lock().await;
        if let Some(flag) = flags.get(model_id) {
            flag.store(true, Ordering::SeqCst);
            info!(model_id = %model_id, "Download cancellation requested");
            Ok(())
        } else {
            Err(format!("No active download for {model_id}"))
        }
    }

    /// Check if a model is currently being downloaded.
    pub async fn is_downloading(&self, model_id: &str) -> bool {
        self.active_downloads.lock().await.contains(model_id)
    }

    /// List installed models (files in models_dir that match manifest entries).
    pub async fn list_installed(
        app: &AppHandle,
        manifest: &super::manifest::Manifest,
    ) -> Result<Vec<InstalledModel>, String> {
        let models_dir = Self::models_dir(app)?;
        let mut installed = Vec::new();

        for model in &manifest.models {
            let path = models_dir.join(&model.file);
            if path.exists() {
                if let Ok(metadata) = tokio::fs::metadata(&path).await {
                    installed.push(InstalledModel {
                        id: model.id.clone(),
                        file: model.file.clone(),
                        size_on_disk: metadata.len(),
                        path: path.to_string_lossy().to_string(),
                    });
                }
            }
        }

        Ok(installed)
    }

    /// Delete a model file.
    pub async fn delete_model(app: &AppHandle, model: &ModelEntry) -> Result<(), String> {
        let models_dir = Self::models_dir(app)?;
        let path = models_dir.join(&model.file);

        if !path.exists() {
            return Err(format!("Model file not found: {}", model.file));
        }

        tokio::fs::remove_file(&path)
            .await
            .map_err(|e| format!("Failed to delete model: {e}"))?;

        info!(model_id = %model.id, "Model deleted");
        Ok(())
    }
}

/// Info about an installed model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledModel {
    pub id: String,
    pub file: String,
    pub size_on_disk: u64,
    pub path: String,
}

/// Inner download logic with progress and resume support.
async fn download_model_inner(
    app: &AppHandle,
    model: &ModelEntry,
    models_dir: &Path,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), String> {
    let final_path = models_dir.join(&model.file);
    let partial_path = models_dir.join(format!("{}.partial", model.file));
    let meta_path = models_dir.join(format!("{}.meta", model.file));

    // Check if already downloaded
    if final_path.exists() {
        emit_progress(
            app,
            &model.id,
            model.size_bytes,
            model.size_bytes,
            DownloadStatus::Complete,
        );
        return Ok(());
    }

    let client = reqwest::Client::builder()
        .user_agent("EchoType/0.1")
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            // Validate redirect targets against the host allowlist
            if let Some(host) = attempt.url().host_str() {
                if is_host_allowed(host) {
                    return attempt.follow();
                }
            }
            attempt.error(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "Redirect to non-allowlisted host",
            ))
        }))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    // Check for existing partial download and metadata
    let (existing_size, prev_meta) = load_partial_state(&partial_path, &meta_path).await;

    // Build request with optional Range header for resume
    let mut request = client.get(&model.url);

    if existing_size > 0 {
        if let Some(ref meta) = prev_meta {
            if let Some(ref etag) = meta.etag {
                request = request.header("If-Range", etag);
            }
        }
        request = request.header("Range", format!("bytes={existing_size}-"));
        debug!(
            model_id = %model.id,
            existing_bytes = existing_size,
            "Attempting resume download"
        );
    }

    let response = request.send().await.map_err(|e| {
        emit_progress(app, &model.id, 0, model.size_bytes, DownloadStatus::Failed);
        format!("Download request failed: {e}")
    })?;

    let status = response.status();

    // Determine if we're resuming or starting fresh
    let (mut downloaded, total_bytes, append) = if status == reqwest::StatusCode::PARTIAL_CONTENT {
        let total = response
            .content_length()
            .map(|cl| cl + existing_size)
            .unwrap_or(model.size_bytes);
        info!(
            model_id = %model.id,
            from_byte = existing_size,
            "Resuming download"
        );
        (existing_size, total, true)
    } else if status.is_success() {
        if existing_size > 0 {
            debug!(model_id = %model.id, "Server returned full content, restarting download");
        }
        let total = response.content_length().unwrap_or(model.size_bytes);
        (0u64, total, false)
    } else {
        emit_progress(app, &model.id, 0, model.size_bytes, DownloadStatus::Failed);
        return Err(format!("Download failed with status {status}"));
    };

    // Save ETag for future resume
    let etag = response
        .headers()
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let meta = PartialMeta {
        etag,
        content_length: Some(total_bytes),
    };
    save_partial_meta(&meta_path, &meta).await;

    // Open file for writing (append or truncate)
    let mut file = if append {
        tokio::fs::OpenOptions::new()
            .append(true)
            .open(&partial_path)
            .await
    } else {
        tokio::fs::File::create(&partial_path).await
    }
    .map_err(|e| {
        emit_progress(app, &model.id, 0, model.size_bytes, DownloadStatus::Failed);
        format!("Cannot open partial file: {e}")
    })?;

    // Stream the response body
    let mut stream = response.bytes_stream();

    while let Some(chunk_result) = stream.next().await {
        // Check for cancellation
        if cancelled.load(Ordering::SeqCst) {
            info!(model_id = %model.id, "Download cancelled");
            let _ = file.flush().await;
            emit_progress(
                app,
                &model.id,
                downloaded,
                total_bytes,
                DownloadStatus::Cancelled,
            );
            return Err("Download cancelled".to_string());
        }

        let chunk = chunk_result.map_err(|e| {
            emit_progress(
                app,
                &model.id,
                downloaded,
                total_bytes,
                DownloadStatus::Failed,
            );
            format!("Download stream error: {e}")
        })?;

        file.write_all(&chunk).await.map_err(|e| {
            emit_progress(
                app,
                &model.id,
                downloaded,
                total_bytes,
                DownloadStatus::Failed,
            );
            format!("Failed to write chunk: {e}")
        })?;

        downloaded += chunk.len() as u64;
        emit_progress(
            app,
            &model.id,
            downloaded,
            total_bytes,
            DownloadStatus::Downloading,
        );
    }

    file.flush().await.map_err(|e| {
        emit_progress(
            app,
            &model.id,
            downloaded,
            total_bytes,
            DownloadStatus::Failed,
        );
        format!("Flush failed: {e}")
    })?;
    drop(file);

    // Verify checksum
    emit_progress(
        app,
        &model.id,
        downloaded,
        total_bytes,
        DownloadStatus::Verifying,
    );

    let checksum_ok = verify_checksum(&partial_path, &model.sha256)
        .await
        .inspect_err(|_| {
            emit_progress(
                app,
                &model.id,
                downloaded,
                total_bytes,
                DownloadStatus::Failed,
            );
        })?;
    if !checksum_ok {
        let _ = tokio::fs::remove_file(&partial_path).await;
        let _ = tokio::fs::remove_file(&meta_path).await;
        emit_progress(
            app,
            &model.id,
            downloaded,
            total_bytes,
            DownloadStatus::Failed,
        );
        return Err("Checksum verification failed — file may be corrupted".to_string());
    }

    // Rename to final path
    tokio::fs::rename(&partial_path, &final_path)
        .await
        .map_err(|e| {
            emit_progress(
                app,
                &model.id,
                downloaded,
                total_bytes,
                DownloadStatus::Failed,
            );
            format!("Failed to finalize download: {e}")
        })?;

    // Clean up meta file
    let _ = tokio::fs::remove_file(&meta_path).await;

    emit_progress(
        app,
        &model.id,
        total_bytes,
        total_bytes,
        DownloadStatus::Complete,
    );
    Ok(())
}

/// Emit download progress event.
fn emit_progress(
    app: &AppHandle,
    model_id: &str,
    downloaded: u64,
    total: u64,
    status: DownloadStatus,
) {
    let event = DownloadProgress {
        model_id: model_id.to_string(),
        downloaded_bytes: downloaded,
        total_bytes: total,
        status,
    };
    if let Err(e) = app.emit("model:download-progress", &event) {
        error!(%e, "Failed to emit download progress");
    }
}

/// Load existing partial download state.
async fn load_partial_state(partial_path: &Path, meta_path: &Path) -> (u64, Option<PartialMeta>) {
    let existing_size = tokio::fs::metadata(partial_path)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    let meta = if existing_size > 0 {
        tokio::fs::read_to_string(meta_path)
            .await
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
    } else {
        None
    };

    (existing_size, meta)
}

/// Save partial download metadata.
async fn save_partial_meta(meta_path: &Path, meta: &PartialMeta) {
    if let Ok(json) = serde_json::to_string(meta) {
        if let Err(e) = tokio::fs::write(meta_path, json).await {
            warn!(error = %e, "Failed to save download metadata");
        }
    }
}

/// Verify SHA-256 checksum of a file.
pub async fn verify_checksum(path: &Path, expected: &str) -> Result<bool, String> {
    let path = path.to_path_buf();
    let expected = expected.to_string();

    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        let mut file = std::fs::File::open(&path)
            .map_err(|e| format!("Cannot open file for checksum: {e}"))?;

        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];

        loop {
            let n = file
                .read(&mut buf)
                .map_err(|e| format!("Read error during checksum: {e}"))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }

        let hash = format!("{:x}", hasher.finalize());
        Ok(hash == expected)
    })
    .await
    .map_err(|e| format!("Checksum task failed: {e}"))?
}

/// Check available disk space.
fn check_disk_space(path: &Path, required_bytes: u64) -> Result<(), String> {
    #[cfg(unix)]
    {
        let output = std::process::Command::new("df")
            .arg("-k")
            .arg(path)
            .output()
            .map_err(|e| format!("Cannot check disk space: {e}"))?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = stdout.lines().nth(1) {
                let fields: Vec<&str> = line.split_whitespace().collect();
                if fields.len() >= 4 {
                    if let Ok(available_kb) = fields[3].parse::<u64>() {
                        let available = available_kb * 1024;
                        let required = required_bytes + DISK_SPACE_MARGIN;
                        if available < required {
                            return Err(format!(
                                "Not enough disk space: need {} MB, have {} MB",
                                required / (1024 * 1024),
                                available / (1024 * 1024)
                            ));
                        }
                        return Ok(());
                    }
                }
            }
        }
        warn!("Could not determine available disk space");
        Ok(())
    }

    #[cfg(not(unix))]
    {
        let _ = (path, required_bytes);
        warn!("Disk space check not implemented for this platform");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_progress_serializes() {
        let progress = DownloadProgress {
            model_id: "test".to_string(),
            downloaded_bytes: 100,
            total_bytes: 1000,
            status: DownloadStatus::Downloading,
        };
        let json = serde_json::to_string(&progress).unwrap();
        assert!(json.contains("\"downloading\""));
    }

    #[test]
    fn download_status_variants() {
        assert_eq!(
            serde_json::to_string(&DownloadStatus::Complete).unwrap(),
            "\"complete\""
        );
        assert_eq!(
            serde_json::to_string(&DownloadStatus::Failed).unwrap(),
            "\"failed\""
        );
        assert_eq!(
            serde_json::to_string(&DownloadStatus::Cancelled).unwrap(),
            "\"cancelled\""
        );
    }

    #[tokio::test]
    async fn verify_checksum_correct() {
        let dir = std::env::temp_dir().join("echotype_test_checksum");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("test_file.bin");
        std::fs::write(&path, b"hello world").unwrap();

        // SHA-256 of "hello world"
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert!(verify_checksum(&path, expected).await.unwrap());

        // Wrong hash
        assert!(!verify_checksum(&path, &"a".repeat(64)).await.unwrap());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
