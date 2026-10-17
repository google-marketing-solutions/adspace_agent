// Copyright 2026 Google LLC
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Authentication primitives shared by the Google API toolsets and tools.
//!
//! Two kinds of credentials are used by the agent:
//!
//! - **User OAuth 2.0 credentials** (authorization-code flow with the
//!   configured `CLIENT_ID` / `CLIENT_SECRET`) for Google Ads, Campaign
//!   Manager 360, Display & Video 360, Drive, YouTube, BigQuery, etc. They are
//!   attached to the chat session (stored in its state), never to the server.
//! - **Application Default Credentials** (ADC) for the agent's own
//!   infrastructure only: the Vertex AI model backend, Cloud Storage
//!   artifacts / skills staging and the Vertex code sandbox. No user-facing
//!   data tool falls back to ADC.
//!
//! Both are exposed to tools through the [`AccessTokenProvider`] trait so the
//! HTTP clients never need to know where a bearer token came from.

use std::sync::Arc;

use async_trait::async_trait;

pub mod adc;
pub mod oauth;

pub use adc::AdcTokenProvider;
pub use oauth::{
    AuthRequired, AuthenticatedUser, CREDENTIALS_STATE_KEY, GoogleOAuthFlow, LOGIN_SENTINEL_APP,
    OAuthClientConfig, OAuthToken, OAuthUiEvent, SessionCredentialManager, SessionRef,
    SessionTokenProvider, UserLogin, auth_required,
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
