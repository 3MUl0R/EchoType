use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::engine::cloud::CloudProvider;

const SERVICE_NAME: &str = "echotype";

/// Status of an API key (never exposes the raw key).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyStatus {
    pub provider: CloudProvider,
    pub has_key: bool,
    pub masked_last4: Option<String>,
}

/// Get the keyring service key for a provider.
fn keyring_key(provider: CloudProvider) -> String {
    format!("{SERVICE_NAME}:{}", provider.as_str())
}

/// Store an API key in the platform keychain.
pub fn set_api_key(provider: CloudProvider, key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(&keyring_key(provider), "api_key")
        .map_err(|e| format!("Keyring error: {e}"))?;
    entry
        .set_password(key)
        .map_err(|e| format!("Failed to store API key: {e}"))?;
    info!(provider = provider.as_str(), "API key stored in keychain");
    Ok(())
}

/// Get an API key from the platform keychain.
/// This is only called internally by cloud engine code — never exposed via IPC.
pub fn get_api_key(provider: CloudProvider) -> Result<Option<String>, String> {
    let entry = keyring::Entry::new(&keyring_key(provider), "api_key")
        .map_err(|e| format!("Keyring error: {e}"))?;
    match entry.get_password() {
        Ok(key) => Ok(Some(key)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(format!("Failed to read API key: {e}")),
    }
}

/// Get the status of an API key (has_key + masked last 4 chars).
pub fn get_api_key_status(provider: CloudProvider) -> Result<ApiKeyStatus, String> {
    let key = get_api_key(provider)?;
    let (has_key, masked) = match key {
        Some(ref k) if k.len() >= 4 => {
            let last4 = &k[k.len() - 4..];
            (true, Some(format!("...{last4}")))
        }
        Some(_) => (true, Some("...".to_string())),
        None => (false, None),
    };

    Ok(ApiKeyStatus {
        provider,
        has_key,
        masked_last4: masked,
    })
}

/// Delete an API key from the platform keychain.
pub fn delete_api_key(provider: CloudProvider) -> Result<(), String> {
    let entry = keyring::Entry::new(&keyring_key(provider), "api_key")
        .map_err(|e| format!("Keyring error: {e}"))?;
    match entry.delete_credential() {
        Ok(()) => {
            info!(
                provider = provider.as_str(),
                "API key deleted from keychain"
            );
            Ok(())
        }
        Err(keyring::Error::NoEntry) => {
            warn!(
                provider = provider.as_str(),
                "No API key to delete in keychain"
            );
            Ok(())
        }
        Err(e) => Err(format!("Failed to delete API key: {e}")),
    }
}

/// Validate an API key by making a lightweight test request.
/// Returns Ok(true) if valid, Ok(false) if invalid/failed, Err on error.
pub async fn validate_api_key(provider: CloudProvider) -> Result<bool, String> {
    let key =
        get_api_key(provider)?.ok_or_else(|| format!("No API key configured for {provider}"))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("HTTP client error: {e}"))?;

    let result = match provider {
        CloudProvider::Groq => {
            // Groq: list models endpoint is lightweight
            client
                .get("https://api.groq.com/openai/v1/models")
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .await
        }
        CloudProvider::OpenAi => {
            // OpenAI: list models endpoint
            client
                .get("https://api.openai.com/v1/models")
                .header("Authorization", format!("Bearer {key}"))
                .send()
                .await
        }
        CloudProvider::Deepgram => {
            // Deepgram: projects endpoint (lightweight)
            client
                .get("https://api.deepgram.com/v1/projects")
                .header("Authorization", format!("Token {key}"))
                .send()
                .await
        }
    };

    match result {
        Ok(response) => {
            let status = response.status();
            if status.is_success() {
                info!(
                    provider = provider.as_str(),
                    "API key validated successfully"
                );
                Ok(true)
            } else if status.as_u16() == 401 || status.as_u16() == 403 {
                info!(
                    provider = provider.as_str(),
                    status = %status,
                    "API key validation failed: unauthorized"
                );
                Ok(false)
            } else {
                warn!(
                    provider = provider.as_str(),
                    status = %status,
                    "API key validation returned unexpected status"
                );
                Ok(false)
            }
        }
        Err(e) => Err(format!("Validation request failed: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyring_key_format() {
        assert_eq!(keyring_key(CloudProvider::Groq), "echotype:groq");
        assert_eq!(keyring_key(CloudProvider::OpenAi), "echotype:openai");
        assert_eq!(keyring_key(CloudProvider::Deepgram), "echotype:deepgram");
    }

    #[test]
    fn api_key_status_masking() {
        // Test the masking logic directly
        let key = "sk-1234567890abcdef";
        let last4 = &key[key.len() - 4..];
        assert_eq!(last4, "cdef");
        assert_eq!(format!("...{last4}"), "...cdef");
    }
}
