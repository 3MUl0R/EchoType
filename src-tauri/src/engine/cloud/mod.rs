pub mod deepgram;
pub mod deepgram_streaming;
pub mod groq;
pub mod openai;
pub mod openai_streaming;

use std::time::Duration;

use reqwest::{Response, StatusCode};
use tracing::{info, warn};

/// Default request timeout for cloud API calls.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Cloud provider identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CloudProvider {
    #[serde(rename = "groq")]
    Groq,
    #[serde(rename = "openai")]
    OpenAi,
    #[serde(rename = "deepgram")]
    Deepgram,
}

impl CloudProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Groq => "groq",
            Self::OpenAi => "openai",
            Self::Deepgram => "deepgram",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Groq => "Groq",
            Self::OpenAi => "OpenAI",
            Self::Deepgram => "Deepgram",
        }
    }
}

impl std::fmt::Display for CloudProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

/// Classification of cloud API errors.
#[derive(Debug)]
pub enum CloudErrorKind {
    /// Retryable: timeout, network, 502/503/504.
    Transient(String),
    /// Not retryable: 401, 403, 400.
    Permanent(String),
    /// Rate limited (429). May have a Retry-After header.
    RateLimited {
        message: String,
        #[allow(dead_code)]
        retry_after: Option<Duration>,
    },
}

impl std::fmt::Display for CloudErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Transient(msg) => write!(f, "Transient error: {msg}"),
            Self::Permanent(msg) => write!(f, "{msg}"),
            Self::RateLimited { message, .. } => write!(f, "Rate limited: {message}"),
        }
    }
}

/// Classify an HTTP response status into a cloud error kind.
pub fn classify_response_error(response: &Response) -> CloudErrorKind {
    let status = response.status();
    let retry_after = response
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok())
        .map(Duration::from_secs);

    match status {
        StatusCode::UNAUTHORIZED => {
            CloudErrorKind::Permanent("API key invalid or expired (401)".to_string())
        }
        StatusCode::FORBIDDEN => {
            CloudErrorKind::Permanent("Access denied by provider (403)".to_string())
        }
        StatusCode::BAD_REQUEST => CloudErrorKind::Permanent("Bad request (400)".to_string()),
        StatusCode::TOO_MANY_REQUESTS => CloudErrorKind::RateLimited {
            message: "Rate limit exceeded (429)".to_string(),
            retry_after,
        },
        s if s == StatusCode::BAD_GATEWAY
            || s == StatusCode::SERVICE_UNAVAILABLE
            || s == StatusCode::GATEWAY_TIMEOUT =>
        {
            CloudErrorKind::Transient(format!("Server error ({s})"))
        }
        s => CloudErrorKind::Permanent(format!("Unexpected status: {s}")),
    }
}

/// Classify a reqwest transport error.
pub fn classify_request_error(error: &reqwest::Error) -> CloudErrorKind {
    if error.is_timeout() {
        CloudErrorKind::Transient("Request timed out".to_string())
    } else if error.is_connect() {
        CloudErrorKind::Transient("Connection failed".to_string())
    } else {
        CloudErrorKind::Transient(format!("Network error: {error}"))
    }
}

/// Send a request with retry on transient errors (up to 1 retry).
///
/// `make_request` is called to produce the request each time (reqwest Requests are not Clone).
/// Returns the successful response body bytes, or the classified error.
pub async fn send_with_retry<F, Fut>(
    provider: CloudProvider,
    make_request: F,
) -> Result<Vec<u8>, CloudErrorKind>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<Response, reqwest::Error>>,
{
    let mut attempt = 0u32;
    let backoff = [Duration::from_secs(1), Duration::from_secs(2)];

    loop {
        let start = std::time::Instant::now();
        let result = make_request().await;
        let elapsed = start.elapsed();

        match result {
            Ok(response) => {
                let status = response.status();
                info!(
                    provider = provider.as_str(),
                    status = %status,
                    duration_ms = elapsed.as_millis() as u64,
                    attempt = attempt + 1,
                    "Cloud API response"
                );

                if status.is_success() {
                    let body =
                        response.bytes().await.map(|b| b.to_vec()).map_err(|e| {
                            CloudErrorKind::Transient(format!("Body read error: {e}"))
                        })?;
                    return Ok(body);
                }

                let error = classify_response_error(&response);
                match &error {
                    CloudErrorKind::Transient(_) if attempt < backoff.len() as u32 => {
                        let delay = backoff[attempt as usize];
                        // Add jitter: 0-500ms using system time subsec for variance
                        let jitter = Duration::from_millis(
                            (std::time::SystemTime::now()
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .subsec_millis() as u64
                                % 500)
                                + 1,
                        );
                        warn!(
                            provider = provider.as_str(),
                            attempt = attempt + 1,
                            delay_ms = (delay + jitter).as_millis() as u64,
                            "Retrying transient cloud error"
                        );
                        tokio::time::sleep(delay + jitter).await;
                        attempt += 1;
                        continue;
                    }
                    _ => return Err(error),
                }
            }
            Err(e) => {
                info!(
                    provider = provider.as_str(),
                    duration_ms = elapsed.as_millis() as u64,
                    error = %e,
                    attempt = attempt + 1,
                    "Cloud API request failed"
                );

                let error = classify_request_error(&e);
                match &error {
                    CloudErrorKind::Transient(_) if attempt < backoff.len() as u32 => {
                        let delay = backoff[attempt as usize];
                        warn!(
                            provider = provider.as_str(),
                            attempt = attempt + 1,
                            "Retrying transient network error"
                        );
                        tokio::time::sleep(delay).await;
                        attempt += 1;
                        continue;
                    }
                    _ => return Err(error),
                }
            }
        }
    }
}

/// Build a reqwest Client with default settings for cloud API calls.
pub fn build_client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {e}"))
}

/// Encode audio samples to WAV bytes for cloud upload.
pub fn encode_audio_wav(audio: &[f32], sample_rate: u32) -> Result<Vec<u8>, String> {
    let buffer = crate::audio::AudioBuffer {
        samples: audio.to_vec(),
        sample_rate,
    };
    crate::audio::playback::encode_wav(&buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloud_provider_display() {
        assert_eq!(CloudProvider::Groq.display_name(), "Groq");
        assert_eq!(CloudProvider::OpenAi.display_name(), "OpenAI");
        assert_eq!(CloudProvider::Deepgram.display_name(), "Deepgram");
    }

    #[test]
    fn cloud_provider_serializes() {
        let json = serde_json::to_string(&CloudProvider::OpenAi).unwrap();
        assert_eq!(json, "\"openai\"");
    }

    #[test]
    fn encode_audio_produces_wav() {
        let samples = vec![0.0f32; 16000]; // 1 second of silence
        let wav = encode_audio_wav(&samples, 16000).unwrap();
        // WAV files start with "RIFF"
        assert_eq!(&wav[..4], b"RIFF");
    }
}
