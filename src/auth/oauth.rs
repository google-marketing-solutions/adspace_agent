//! Google OAuth 2.0 authorization-code flow for end-user credentials.
//!
//! The agent needs the *user's* Google credentials (not the service's) to
//! call Google Ads, Campaign Manager 360, Display & Video 360, Drive, YouTube
//! and friends. This module implements the pieces the Python ADK provided
//! through its `request_credential` flow:
//!
//! 1. A tool that needs a token asks the [`UserCredentialManager`].
//! 2. If no usable token exists, the manager returns an [`AuthRequired`]
//!    error carrying an authorization URL. The tool surfaces that URL to the
//!    user through the LLM.
//! 3. The user authorizes; Google redirects to our `/oauth2/callback` route,
//!    which calls [`UserCredentialManager::complete_authorization`].
//! 4. The next tool call finds the stored token (refreshing it when expired).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio::sync::{Mutex, RwLock};

use super::AccessTokenProvider;

/// Google's OAuth 2.0 authorization endpoint.
pub const GOOGLE_AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
/// Google's OAuth 2.0 token endpoint.
pub const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
/// Google's OAuth 2.0 revocation endpoint.
pub const GOOGLE_REVOKE_ENDPOINT: &str = "https://oauth2.googleapis.com/revoke";
/// Path of the OAuth redirect handler served by the agent.
pub const OAUTH_CALLBACK_PATH: &str = "/oauth2/callback";
/// Session-state key under which the user's Google credentials are mirrored
/// (same name the Python agent used).
pub const CREDENTIALS_STATE_KEY: &str = "CREDENTIALS_CACHE_KEY";

/// How long before expiry a token is treated as expired.
const EXPIRY_SKEW: Duration = Duration::from_secs(60);
/// Pending authorizations older than this are dropped.
const PENDING_TTL: Duration = Duration::from_secs(15 * 60);

/// OAuth client configuration.
#[derive(Debug, Clone)]
pub struct OAuthClientConfig {
    /// OAuth client ID.
    pub client_id: String,
    /// OAuth client secret.
    pub client_secret: String,
    /// Redirect URI registered for the client (must end with
    /// [`OAUTH_CALLBACK_PATH`] for the built-in handler).
    pub redirect_uri: String,
}

impl OAuthClientConfig {
    /// Builds the config from `CLIENT_ID`, `CLIENT_SECRET` and
    /// `PUBLIC_BASE_URL` (default `http://localhost:8000`).
    pub fn from_env() -> Self {
        let base = std::env::var("PUBLIC_BASE_URL")
            .unwrap_or_else(|_| "http://localhost:8000".to_string());
        Self {
            client_id: std::env::var("CLIENT_ID").unwrap_or_default(),
            client_secret: std::env::var("CLIENT_SECRET").unwrap_or_default(),
            redirect_uri: format!("{}{OAUTH_CALLBACK_PATH}", base.trim_end_matches('/')),
        }
    }

    /// Whether a client ID and secret are configured.
    pub fn is_configured(&self) -> bool {
        !self.client_id.is_empty() && !self.client_secret.is_empty()
    }
}

/// An OAuth 2.0 token set.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OAuthToken {
    /// Bearer access token.
    pub access_token: String,
    /// Refresh token (present when `access_type=offline` was granted).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// Absolute expiry time of the access token.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<DateTime<Utc>>,
    /// Scopes granted by the user.
    #[serde(default)]
    pub scopes: Vec<String>,
}

impl OAuthToken {
    /// Whether the access token is (about to be) expired.
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(at) => Utc::now() + EXPIRY_SKEW >= at,
            None => false,
        }
    }

    /// Whether the token covers every requested scope.
    ///
    /// Tokens whose granted scopes are unknown (empty) are assumed to cover
    /// everything, matching how pre-seeded refresh tokens behave.
    pub fn covers(&self, scopes: &[String]) -> bool {
        self.scopes.is_empty() || scopes.iter().all(|s| self.scopes.iter().any(|g| g == s))
    }
}

/// Raw token endpoint response.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    #[serde(default)]
    expires_in: Option<i64>,
    #[serde(default)]
    scope: Option<String>,
}

impl TokenResponse {
    fn into_token(
        self,
        fallback_refresh: Option<String>,
        fallback_scopes: &[String],
    ) -> OAuthToken {
        let scopes = self
            .scope
            .as_deref()
            .map(|s| s.split_whitespace().map(str::to_string).collect::<Vec<_>>())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| fallback_scopes.to_vec());
        OAuthToken {
            access_token: self.access_token,
            refresh_token: self.refresh_token.or(fallback_refresh),
            expires_at: self
                .expires_in
                .map(|secs| Utc::now() + chrono::Duration::seconds(secs)),
            scopes,
        }
    }
}

/// The user must complete an OAuth authorization before the call can proceed.
#[derive(Debug, Clone, Error)]
#[error("{message}")]
pub struct AuthRequired {
    /// URL the user must open to grant access.
    pub auth_url: String,
    /// Human-readable instructions.
    pub message: String,
}

impl AuthRequired {
    /// JSON payload a tool can return to the model so it can relay the link.
    pub fn to_tool_response(&self) -> serde_json::Value {
        serde_json::json!({
            "status": "AUTH_REQUIRED",
            "pending": true,
            "auth_url": self.auth_url,
            "message": self.message,
        })
    }
}

/// Performs the HTTP legs of the authorization-code flow.
#[derive(Debug, Clone)]
pub struct GoogleOAuthFlow {
    config: OAuthClientConfig,
    http: reqwest::Client,
    auth_endpoint: String,
    token_endpoint: String,
}

impl GoogleOAuthFlow {
    /// Creates a flow for the given client configuration.
    pub fn new(config: OAuthClientConfig) -> Self {
        Self {
            config,
            http: reqwest::Client::new(),
            auth_endpoint: GOOGLE_AUTH_ENDPOINT.to_string(),
            token_endpoint: GOOGLE_TOKEN_ENDPOINT.to_string(),
        }
    }

    /// Overrides the Google endpoints (tests).
    pub fn with_endpoints(
        mut self,
        auth_endpoint: impl Into<String>,
        token_endpoint: impl Into<String>,
    ) -> Self {
        self.auth_endpoint = auth_endpoint.into();
        self.token_endpoint = token_endpoint.into();
        self
    }

    /// The client configuration.
    pub fn config(&self) -> &OAuthClientConfig {
        &self.config
    }

    /// Builds the consent-screen URL for `scopes`, binding it to `state`.
    pub fn authorization_url(&self, scopes: &[String], state: &str) -> String {
        let mut url = url::Url::parse(&self.auth_endpoint).expect("valid auth endpoint");
        url.query_pairs_mut()
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", &self.config.redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", &scopes.join(" "))
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent")
            .append_pair("include_granted_scopes", "true")
            .append_pair("state", state);
        url.to_string()
    }

    /// Exchanges an authorization code for tokens.
    ///
    /// # Errors
    ///
    /// Returns an error when the token endpoint rejects the exchange.
    pub async fn exchange_code(
        &self,
        code: &str,
        requested_scopes: &[String],
    ) -> anyhow::Result<OAuthToken> {
        let params = [
            ("code", code),
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
            ("redirect_uri", self.config.redirect_uri.as_str()),
            ("grant_type", "authorization_code"),
        ];
        let resp = self
            .http
            .post(&self.token_endpoint)
            .form(&params)
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            anyhow::bail!("OAuth code exchange failed ({status}): {body}");
        }
        let parsed: TokenResponse = serde_json::from_str(&body)?;
        Ok(parsed.into_token(None, requested_scopes))
    }

    /// Refreshes an access token.
    ///
    /// # Errors
    ///
    /// Returns an error when the refresh token is invalid or revoked.
    pub async fn refresh(&self, token: &OAuthToken) -> anyhow::Result<OAuthToken> {
        let Some(refresh_token) = token.refresh_token.as_deref() else {
            anyhow::bail!("no refresh token available");
        };
        let params = [
            ("refresh_token", refresh_token),
            ("client_id", self.config.client_id.as_str()),
            ("client_secret", self.config.client_secret.as_str()),
            ("grant_type", "refresh_token"),
        ];
        let resp = self
            .http
            .post(&self.token_endpoint)
            .form(&params)
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            anyhow::bail!("OAuth token refresh failed ({status}): {body}");
        }
        let parsed: TokenResponse = serde_json::from_str(&body)?;
        Ok(parsed.into_token(Some(refresh_token.to_string()), &token.scopes))
    }
}

/// Persistent storage for per-user tokens.
#[async_trait]
pub trait CredentialStore: Send + Sync {
    /// Loads the token for a user.
    async fn get(&self, user_id: &str) -> Option<OAuthToken>;
    /// Stores the token for a user.
    async fn put(&self, user_id: &str, token: OAuthToken);
    /// Deletes the token for a user.
    async fn remove(&self, user_id: &str);
}

/// Process-local token store.
#[derive(Debug, Default)]
pub struct InMemoryCredentialStore {
    tokens: RwLock<HashMap<String, OAuthToken>>,
}

#[async_trait]
impl CredentialStore for InMemoryCredentialStore {
    async fn get(&self, user_id: &str) -> Option<OAuthToken> {
        self.tokens.read().await.get(user_id).cloned()
    }

    async fn put(&self, user_id: &str, token: OAuthToken) {
        self.tokens.write().await.insert(user_id.to_string(), token);
    }

    async fn remove(&self, user_id: &str) {
        self.tokens.write().await.remove(user_id);
    }
}

/// JSON-file token store (one file holding every user's tokens).
///
/// Suitable for local development; the file is created with mode `0600` on
/// Unix.
#[derive(Debug)]
pub struct FileCredentialStore {
    path: PathBuf,
    inner: InMemoryCredentialStore,
    io: Mutex<()>,
}

impl FileCredentialStore {
    /// Opens (or creates) the store at `path`, loading existing tokens.
    pub async fn open(path: impl Into<PathBuf>) -> Self {
        let path = path.into();
        let inner = InMemoryCredentialStore::default();
        if let Ok(text) = tokio::fs::read_to_string(&path).await
            && let Ok(map) = serde_json::from_str::<HashMap<String, OAuthToken>>(&text)
        {
            *inner.tokens.write().await = map;
        }
        Self {
            path,
            inner,
            io: Mutex::new(()),
        }
    }

    async fn flush(&self) {
        let _guard = self.io.lock().await;
        let snapshot = self.inner.tokens.read().await.clone();
        let Ok(text) = serde_json::to_string_pretty(&snapshot) else {
            return;
        };
        if let Some(parent) = self.path.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }
        if let Err(e) = tokio::fs::write(&self.path, text).await {
            tracing::warn!(
                "Failed to persist credentials to {}: {e}",
                self.path.display()
            );
            return;
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.path, std::fs::Permissions::from_mode(0o600));
        }
    }
}

#[async_trait]
impl CredentialStore for FileCredentialStore {
    async fn get(&self, user_id: &str) -> Option<OAuthToken> {
        self.inner.get(user_id).await
    }

    async fn put(&self, user_id: &str, token: OAuthToken) {
        self.inner.put(user_id, token).await;
        self.flush().await;
    }

    async fn remove(&self, user_id: &str) {
        self.inner.remove(user_id).await;
        self.flush().await;
    }
}

#[derive(Debug, Clone)]
struct PendingAuthorization {
    user_id: String,
    scopes: Vec<String>,
    created_at: DateTime<Utc>,
}

/// Coordinates per-user Google credentials for the whole agent process.
pub struct UserCredentialManager {
    flow: GoogleOAuthFlow,
    store: Arc<dyn CredentialStore>,
    pending: Mutex<HashMap<String, PendingAuthorization>>,
    /// Scopes every authorization asks for (union of all enabled toolsets),
    /// so one consent covers the whole agent.
    base_scopes: RwLock<Vec<String>>,
    /// Optional pre-seeded token shared by every user (single-tenant setups).
    seeded: Option<OAuthToken>,
}

impl std::fmt::Debug for UserCredentialManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UserCredentialManager")
            .field("redirect_uri", &self.flow.config.redirect_uri)
            .finish()
    }
}

impl UserCredentialManager {
    /// Creates a manager.
    pub fn new(flow: GoogleOAuthFlow, store: Arc<dyn CredentialStore>) -> Self {
        Self {
            flow,
            store,
            pending: Mutex::new(HashMap::new()),
            base_scopes: RwLock::new(Vec::new()),
            seeded: None,
        }
    }

    /// Seeds a refresh token (from `GOOGLE_OAUTH_REFRESH_TOKEN`) used for any
    /// user that has not authorized interactively.
    pub fn with_seeded_refresh_token(mut self, refresh_token: Option<String>) -> Self {
        self.seeded = refresh_token
            .filter(|t| !t.is_empty())
            .map(|refresh_token| OAuthToken {
                access_token: String::new(),
                refresh_token: Some(refresh_token),
                expires_at: Some(DateTime::<Utc>::MIN_UTC),
                scopes: Vec::new(),
            });
        self
    }

    /// The OAuth flow (for building URLs in UIs).
    pub fn flow(&self) -> &GoogleOAuthFlow {
        &self.flow
    }

    /// Registers scopes that every consent screen should include.
    pub async fn register_scopes(&self, scopes: &[String]) {
        let mut base = self.base_scopes.write().await;
        for scope in scopes {
            if !base.contains(scope) {
                base.push(scope.clone());
            }
        }
    }

    /// Scopes requested on every consent screen.
    pub async fn registered_scopes(&self) -> Vec<String> {
        self.base_scopes.read().await.clone()
    }

    /// Returns a valid access token for `user_id` covering `scopes`.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRequired`] (wrapped in `anyhow`) when the user must
    /// authorize first, or another error when a refresh fails.
    pub async fn access_token(&self, user_id: &str, scopes: &[String]) -> anyhow::Result<String> {
        let stored = match self.store.get(user_id).await {
            Some(token) => Some(token),
            None => self.seeded.clone(),
        };

        if let Some(token) = stored {
            if !token.covers(scopes) {
                tracing::info!(
                    "Stored credentials for {user_id} lack required scopes; requesting re-authorization"
                );
                return Err(self.begin_authorization(user_id, scopes).await.into());
            }
            if !token.is_expired() {
                return Ok(token.access_token);
            }
            if token.refresh_token.is_some() {
                match self.flow.refresh(&token).await {
                    Ok(refreshed) => {
                        self.store.put(user_id, refreshed.clone()).await;
                        return Ok(refreshed.access_token);
                    }
                    Err(e) => {
                        tracing::warn!("Failed to refresh Google credentials for {user_id}: {e}");
                        self.store.remove(user_id).await;
                    }
                }
            }
        }
        Err(self.begin_authorization(user_id, scopes).await.into())
    }

    /// Starts an authorization for `user_id`, returning the instructions.
    pub async fn begin_authorization(&self, user_id: &str, scopes: &[String]) -> AuthRequired {
        if !self.flow.config.is_configured() {
            return AuthRequired {
                auth_url: String::new(),
                message: "Google OAuth is not configured: set CLIENT_ID and CLIENT_SECRET (or GOOGLE_OAUTH_REFRESH_TOKEN) and restart the agent.".to_string(),
            };
        }
        let mut all_scopes = self.registered_scopes().await;
        for scope in scopes {
            if !all_scopes.contains(scope) {
                all_scopes.push(scope.clone());
            }
        }
        let state = uuid::Uuid::new_v4().simple().to_string();
        let auth_url = self.flow.authorization_url(&all_scopes, &state);
        let mut pending = self.pending.lock().await;
        let now = Utc::now();
        pending.retain(|_, p| {
            now.signed_duration_since(p.created_at)
                .to_std()
                .unwrap_or_default()
                < PENDING_TTL
        });
        pending.insert(
            state,
            PendingAuthorization {
                user_id: user_id.to_string(),
                scopes: all_scopes,
                created_at: now,
            },
        );
        AuthRequired {
            auth_url: auth_url.clone(),
            message: format!(
                "Authorization required: ask the user to open this link, sign in with the Google account that has access, grant the requested permissions, and then say \"done\" so the request can be retried: {auth_url}"
            ),
        }
    }

    /// Completes an authorization from the redirect handler.
    ///
    /// # Errors
    ///
    /// Returns an error when `state` is unknown/expired or the code exchange
    /// fails.
    pub async fn complete_authorization(&self, state: &str, code: &str) -> anyhow::Result<String> {
        let pending = self.pending.lock().await.remove(state);
        let Some(pending) = pending else {
            anyhow::bail!(
                "Unknown or expired OAuth state parameter; please restart the authorization from the chat."
            );
        };
        let token = self.flow.exchange_code(code, &pending.scopes).await?;
        self.store.put(&pending.user_id, token).await;
        tracing::info!("Stored Google credentials for user {}", pending.user_id);
        Ok(pending.user_id)
    }

    /// Forgets the stored credentials of a user.
    pub async fn revoke(&self, user_id: &str) {
        self.store.remove(user_id).await;
    }

    /// The stored token for a user, if any (no refresh performed).
    pub async fn stored_token(&self, user_id: &str) -> Option<OAuthToken> {
        self.store.get(user_id).await
    }
}

/// [`AccessTokenProvider`] bound to one user and a set of scopes.
#[derive(Clone)]
pub struct UserTokenProvider {
    manager: Arc<UserCredentialManager>,
    user_id: String,
    scopes: Vec<String>,
}

impl UserTokenProvider {
    /// Creates a provider for `user_id` needing `scopes`.
    pub fn new(
        manager: Arc<UserCredentialManager>,
        user_id: impl Into<String>,
        scopes: Vec<String>,
    ) -> Self {
        Self {
            manager,
            user_id: user_id.into(),
            scopes,
        }
    }
}

#[async_trait]
impl AccessTokenProvider for UserTokenProvider {
    async fn access_token(&self) -> anyhow::Result<String> {
        self.manager.access_token(&self.user_id, &self.scopes).await
    }
}

/// Extracts an [`AuthRequired`] from an error chain, if present.
pub fn auth_required(err: &anyhow::Error) -> Option<&AuthRequired> {
    err.chain().find_map(|e| e.downcast_ref::<AuthRequired>())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> OAuthClientConfig {
        OAuthClientConfig {
            client_id: "cid".into(),
            client_secret: "secret".into(),
            redirect_uri: "http://localhost:8000/oauth2/callback".into(),
        }
    }

    #[test]
    fn authorization_url_contains_required_params() {
        let flow = GoogleOAuthFlow::new(config());
        let url = flow.authorization_url(
            &["https://www.googleapis.com/auth/adwords".to_string()],
            "abc",
        );
        assert!(url.starts_with(GOOGLE_AUTH_ENDPOINT));
        assert!(url.contains("client_id=cid"));
        assert!(url.contains("redirect_uri=http%3A%2F%2Flocalhost%3A8000%2Foauth2%2Fcallback"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("state=abc"));
        assert!(url.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fadwords"));
    }

    #[test]
    fn token_expiry_and_scope_coverage() {
        let mut token = OAuthToken {
            access_token: "a".into(),
            refresh_token: None,
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
            scopes: vec!["x".into(), "y".into()],
        };
        assert!(!token.is_expired());
        assert!(token.covers(&["x".into()]));
        assert!(!token.covers(&["z".into()]));
        token.expires_at = Some(Utc::now() + chrono::Duration::seconds(10));
        assert!(
            token.is_expired(),
            "inside the skew window counts as expired"
        );
        token.scopes.clear();
        assert!(
            token.covers(&["z".into()]),
            "unknown grants are assumed to cover"
        );
    }

    #[tokio::test]
    async fn manager_requests_authorization_when_no_token() {
        let manager = UserCredentialManager::new(
            GoogleOAuthFlow::new(config()),
            Arc::new(InMemoryCredentialStore::default()),
        );
        manager.register_scopes(&["base-scope".to_string()]).await;
        let err = manager
            .access_token("u1", &["tool-scope".to_string()])
            .await
            .unwrap_err();
        let required = auth_required(&err).expect("AuthRequired error");
        assert!(required.auth_url.contains("scope=base-scope+tool-scope"));
        assert_eq!(required.to_tool_response()["status"], "AUTH_REQUIRED");
        // the pending state is registered under the url's state param
        let state = url::Url::parse(&required.auth_url)
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.to_string())
            .unwrap();
        assert!(manager.pending.lock().await.contains_key(&state));
        // unknown state is rejected
        assert!(
            manager
                .complete_authorization("nope", "code")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn manager_returns_valid_stored_token() {
        let store = Arc::new(InMemoryCredentialStore::default());
        store
            .put(
                "u1",
                OAuthToken {
                    access_token: "tok".into(),
                    refresh_token: None,
                    expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
                    scopes: vec![],
                },
            )
            .await;
        let manager = UserCredentialManager::new(GoogleOAuthFlow::new(config()), store);
        assert_eq!(
            manager
                .access_token("u1", &["s".to_string()])
                .await
                .unwrap(),
            "tok"
        );
    }

    #[tokio::test]
    async fn unconfigured_client_gives_actionable_message() {
        let manager = UserCredentialManager::new(
            GoogleOAuthFlow::new(OAuthClientConfig {
                client_id: String::new(),
                client_secret: String::new(),
                redirect_uri: String::new(),
            }),
            Arc::new(InMemoryCredentialStore::default()),
        );
        let err = manager.access_token("u", &[]).await.unwrap_err();
        assert!(auth_required(&err).unwrap().message.contains("CLIENT_ID"));
    }

    #[tokio::test]
    async fn file_store_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("creds.json");
        let token = OAuthToken {
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            expires_at: None,
            scopes: vec!["s".into()],
        };
        {
            let store = FileCredentialStore::open(&path).await;
            store.put("u1", token.clone()).await;
        }
        let reopened = FileCredentialStore::open(&path).await;
        assert_eq!(reopened.get("u1").await, Some(token));
        reopened.remove("u1").await;
        assert_eq!(FileCredentialStore::open(&path).await.get("u1").await, None);
    }
}
