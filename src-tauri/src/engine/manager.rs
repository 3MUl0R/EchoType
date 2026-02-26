use std::sync::Arc;

use tokio::sync::Mutex;
use tracing::{info, warn};

use super::{EngineError, SttEngine, TranscribeRequest, Transcription};

/// Manages the active STT engine. Only one engine is loaded at a time.
pub struct EngineManager {
    engine: Arc<Mutex<Option<Arc<dyn SttEngine>>>>,
}

impl EngineManager {
    pub fn new() -> Self {
        Self {
            engine: Arc::new(Mutex::new(None)),
        }
    }

    /// Load a new engine, replacing any currently loaded one.
    pub async fn load(&self, engine: Box<dyn SttEngine>) {
        let name = engine.name().to_string();
        let mut guard = self.engine.lock().await;
        if let Some(old) = guard.as_ref() {
            info!(old_engine = old.name(), "Unloading previous engine");
        }
        *guard = Some(Arc::from(engine));
        info!(engine = %name, "Engine loaded");
    }

    /// Unload the current engine.
    pub async fn unload(&self) {
        let mut guard = self.engine.lock().await;
        if let Some(engine) = guard.as_ref() {
            info!(engine = engine.name(), "Unloading engine");
        }
        *guard = None;
    }

    /// Run transcription on the active engine.
    ///
    /// Clones the engine Arc and releases the lock before running inference,
    /// so that load/unload/is_loaded are not blocked during transcription.
    pub async fn transcribe(
        &self,
        request: TranscribeRequest,
    ) -> Result<Transcription, EngineError> {
        let engine = {
            let guard = self.engine.lock().await;
            match guard.as_ref() {
                Some(engine) => Arc::clone(engine),
                None => {
                    warn!("Transcription requested but no engine loaded");
                    return Err(EngineError::NoEngineLoaded);
                }
            }
        };
        engine.transcribe(request).await
    }

    /// Check if an engine is currently loaded.
    pub async fn is_loaded(&self) -> bool {
        self.engine.lock().await.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn starts_unloaded() {
        let manager = EngineManager::new();
        assert!(!manager.is_loaded().await);
    }

    #[tokio::test]
    async fn transcribe_without_engine_returns_error() {
        let manager = EngineManager::new();
        let request = TranscribeRequest {
            audio: vec![0.0; 16000],
            sample_rate: 16000,
            language: None,
        };
        let result = manager.transcribe(request).await;
        assert!(matches!(result, Err(EngineError::NoEngineLoaded)));
    }
}
