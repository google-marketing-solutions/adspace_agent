//! Authentication primitives shared by the Google API toolsets and tools.
//!
//! Two kinds of credentials are used by the agent:
//!
//! - **User OAuth 2.0 credentials** (authorization-code flow with the
//!   configured `CLIENT_ID` / `CLIENT_SECRET`) for Google Ads, Campaign
//!   Manager 360, Display & Video 360, Drive, YouTube, etc. They are attached
//!   to the chat session (stored in its state), never to the server.
//! - **Application Default Credentials** (ADC) for Google Cloud services such
//!   as Vertex AI and Cloud Storage.
//!
//! Both are exposed to tools through the [`AccessTokenProvider`] trait so the
//! HTTP clients never need to know where a bearer token came from.

use std::sync::Arc;

use async_trait::async_trait;

pub mod adc;
pub mod oauth;

pub use adc::AdcTokenProvider;
pub use oauth::{
    AuthRequired, CREDENTIALS_STATE_KEY, GoogleOAuthFlow, OAuthClientConfig, OAuthToken,
    OAuthUiEvent, SessionCredentialManager, SessionRef, SessionTokenProvider, auth_required,
};

/// Something that can produce a valid OAuth 2.0 bearer access token.
#[async_trait]
pub trait AccessTokenProvider: Send + Sync {
    /// Returns a currently-valid access token, refreshing it if necessary.
    async fn access_token(&self) -> anyhow::Result<String>;
}

/// A provider that always returns the same static token (useful in tests).
#[derive(Debug, Clone)]
pub struct StaticTokenProvider {
    token: String,
}

impl StaticTokenProvider {
    /// Creates a provider for a fixed token.
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
        }
    }
}

#[async_trait]
impl AccessTokenProvider for StaticTokenProvider {
    async fn access_token(&self) -> anyhow::Result<String> {
        Ok(self.token.clone())
    }
}

/// Shared, reference-counted token provider handle.
pub type SharedTokenProvider = Arc<dyn AccessTokenProvider>;
