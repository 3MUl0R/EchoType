use std::fs;
use std::path::PathBuf;

use tracing::info;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::fmt::format::FmtSpan;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;

fn log_dir() -> PathBuf {
    let base = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.echotype.app")
        .join("logs");
    fs::create_dir_all(&base).ok();
    base
}

fn cleanup_old_logs(dir: &std::path::Path, max_age_days: u64) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let cutoff =
        std::time::SystemTime::now() - std::time::Duration::from_secs(max_age_days * 24 * 60 * 60);

    for entry in entries.flatten() {
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if modified < cutoff {
            fs::remove_file(entry.path()).ok();
        }
    }
}

/// Initialize the tracing subscriber with file and stderr output.
/// Returns a guard that must be held for the lifetime of the application.
pub fn init() -> WorkerGuard {
    let log_dir = log_dir();
    cleanup_old_logs(&log_dir, 3);

    let file_appender = tracing_appender::rolling::daily(&log_dir, "echotype.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    let env_filter =
        EnvFilter::try_from_env("ECHOTYPE_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

    let json_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(non_blocking)
        .with_span_events(FmtSpan::NONE)
        .with_target(true)
        .with_level(true);

    let stderr_layer = tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(true)
        .with_level(true)
        .compact();

    tracing_subscriber::registry()
        .with(env_filter)
        .with(json_layer)
        .with(stderr_layer)
        .init();

    info!(path = %log_dir.display(), "Log directory initialized");

    guard
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_dir_is_valid() {
        let dir = log_dir();
        assert!(
            dir.to_str().unwrap().contains("echotype")
                || dir.to_str().unwrap().contains("com.echotype")
        );
    }
}
