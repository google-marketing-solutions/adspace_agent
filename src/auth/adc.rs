//! Application Default Credentials (ADC) token provider.
//!
//! Used for Google Cloud services the *agent* owns (Vertex AI, Cloud
//! Storage), as opposed to the end user's OAuth credentials.

use std::sync::Arc;

use async_trait::async_trait;
use google_cloud_auth::credentials::{self, CacheableResource, Credentials};
use tokio::sync::RwLock;

use super::AccessTokenProvider;

/// Scope granting access to all Google Cloud APIs.
pub const CLOUD_PLATFORM_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

/// Token provider backed by Application Default Credentials.
///
/// Resolution follows the standard ADC chain: `GOOGLE_APPLICATION_CREDENTIALS`,
/// `gcloud auth application-default login`, then the metadata server.
#[derive(Clone)]
pub struct AdcTokenProvider {
    credentials: Credentials,
    cached: Arc<RwLock<Option<String>>>,
}

impl std::fmt::Debug for AdcTokenProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AdcTokenProvider").finish_non_exhaustive()
    }
}

impl AdcTokenProvider {
    /// Builds a provider requesting the given scopes (defaults to
    /// `cloud-platform` when empty).
    ///
    /// # Errors
    ///
    /// Returns an error when no ADC source can be found.
    pub fn new(scopes: &[&str]) -> anyhow::Result<Self> {
        let scopes: Vec<String> = if scopes.is_empty() {
            vec![CLOUD_PLATFORM_SCOPE.to_string()]
        } else {
            scopes.iter().map(|s| s.to_string()).collect()
        };
        let credentials = credentials::Builder::default()
            .with_scopes(scopes)
            .build()
            .map_err(|e| anyhow::anyhow!("failed to build Application Default Credentials: {e}"))?;
        Ok(Self {
            credentials,
            cached: Arc::new(RwLock::new(None)),
        })
    }

    /// Wraps explicit credentials (tests / custom sources).
    pub fn from_credentials(credentials: Credentials) -> Self {
        Self {
            credentials,
            cached: Arc::new(RwLock::new(None)),
        }
    }
}

#[async_trait]
impl AccessTokenProvider for AdcTokenProvider {
    async fn access_token(&self) -> anyhow::Result<String> {
        let headers = self
            .credentials
            .headers(Default::default())
            .await
            .map_err(|e| {
                anyhow::anyhow!("failed to acquire Application Default Credentials: {e}")
            })?;
        match headers {
            CacheableResource::New { data, .. } => {
                let value = data
                    .get(reqwest::header::AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| {
                        v.strip_prefix("Bearer ")
                            .or_else(|| v.strip_prefix("bearer "))
                    })
                    .map(str::to_string)
                    .ok_or_else(|| {
                        anyhow::anyhow!("ADC did not return a bearer Authorization header")
                    })?;
                *self.cached.write().await = Some(value.clone());
                Ok(value)
            }
            CacheableResource::NotModified => self.cached.read().await.clone().ok_or_else(|| {
                anyhow::anyhow!("ADC returned NotModified before any token was cached")
            }),
        }
    }
}
