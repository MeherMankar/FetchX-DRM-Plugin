//! Shared types and traits used across the DRM plugin modules.

/// A single decrypted content key.
#[derive(Clone, Debug)]
pub struct ContentKey {
    /// Key identifier bytes.
    pub key_id: Vec<u8>,
    /// Decrypted key bytes.
    pub key: Vec<u8>,
}

/// Trait implemented by both [`crate::local_cdm::LocalCdm`] and any remote CDM adapter.
#[async_trait::async_trait]
pub trait WidevineProvider: Send + Sync {
    async fn get_keys(
        &self,
        pssh: &[u8],
        license_url: &str,
    ) -> Result<Vec<ContentKey>, Box<dyn std::error::Error + Send + Sync>>;
}

/// Trait implemented by both local and remote PlayReady CDM adapters.
#[async_trait::async_trait]
pub trait PlayReadyProvider: Send + Sync {
    async fn get_keys(
        &self,
        pssh: &[u8],
        license_url: &str,
    ) -> Result<Vec<ContentKey>, Box<dyn std::error::Error + Send + Sync>>;
}
