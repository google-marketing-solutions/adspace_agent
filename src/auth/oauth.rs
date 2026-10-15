//! Google OAuth 2.0 authorization-code flow for end-user credentials.
//!
//! The agent needs the *user's* Google credentials (not the service's) to
//! call Google Ads, Campaign Manager 360, Display & Video 360, Drive, YouTube
//! and friends. Credentials belong to the chat session, exactly like the
//! Python agent's `CREDENTIALS_CACHE_KEY` state entry:
//!
//! 1. A tool that needs a token asks the [`SessionCredentialManager`], which
//!    reads the token from the session state (refreshing it when expired).
//! 2. If no usable token exists, the manager returns an [`AuthRequired`]
//!    error carrying an authorization URL whose `state` parameter is a signed
//!    claim naming the session. The tool surfaces that URL to the user
//!    through the LLM.
//! 3. The user authorizes; Google redirects to our `/oauth2/callback` route,
//!    which calls [`SessionCredentialManager::complete_authorization`] and
//!    [`SessionCredentialManager::persist`] to write the tokens into that
//!    session's state.
//! 4. The next tool call of the session finds the stored token.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use adk_rust::session::{AppendEventRequest, SessionService};
use adk_rust::{AdkIdentity, AppName, Event, ReadonlyContext, SessionId, ToolContext, UserId};
use async_trait::async_trait;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::{broadcast, watch};

use super::AccessTokenProvider;

type HmacSha256 = Hmac<Sha256>;

/// Google's OAuth 2.0 authorization endpoint.
pub const GOOGLE_AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
/// Google's OAuth 2.0 token endpoint.
pub const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
/// Google's OAuth 2.0 revocation endpoint.
pub const GOOGLE_REVOKE_ENDPOINT: &str = "https://oauth2.googleapis.com/revoke";
/// Google OpenID Connect userinfo endpoint.
pub const GOOGLE_USERINFO_ENDPOINT: &str = "https://openidconnect.googleapis.com/v1/userinfo";
/// Path of the OAuth redirect handler served by the agent.
pub const OAUTH_CALLBACK_PATH: &str = "/oauth2/callback";
/// Sentinel `app_name` in [`SessionRef`] indicating a user Google Sign-In flow
/// (rather than a tool scope authorization for an existing chat session).
pub const LOGIN_SENTINEL_APP: &str = "__adspace_login__";
/// Session-state key holding the session's Google credentials (same name the
/// Python agent used).
pub const CREDENTIALS_STATE_KEY: &str = "CREDENTIALS_CACHE_KEY";

/// Signed-in Google user profile for the custom AdSpace Agent web interface.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthenticatedUser {
    /// Stable user identifier (Google email address when available, else `sub`).
    pub id: String,
    /// Google account email address.
    pub email: String,
    /// Display name.
    pub name: String,
    /// Profile avatar URL (`picture` claim from Google userinfo).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    /// Issued-at Unix timestamp.
    pub iat: i64,
}

#[derive(Debug, Deserialize)]
struct GoogleUserInfoResponse {
    #[serde(default)]
    sub: String,
    #[serde(default)]
    email: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    picture: Option<String>,
}

/// How long before expiry a token is treated as expired.
const EXPIRY_SKEW: Duration = Duration::from_secs(60);
/// Authorization links older than this are rejected by the callback.
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
    /// A granted scope covers itself and its read-only variant (a session
    /// that granted `drive` is not asked again for `drive.readonly`). Tokens
    /// whose granted scopes are unknown (empty) are assumed to cover
    /// everything.
    pub fn covers(&self, scopes: &[String]) -> bool {
        self.scopes.is_empty()
            || scopes.iter().all(|needed| {
                self.scopes
                    .iter()
                    .any(|granted| scope_covers(granted, needed))
            })
    }
}

/// Whether a granted scope satisfies a needed one.
fn scope_covers(granted: &str, needed: &str) -> bool {
    granted == needed
        || needed
            .strip_prefix(granted)
            .is_some_and(|rest| matches!(rest, ".readonly" | ".read_only" | ".read-only"))
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
    /// Scopes the link asks for (only what the calling tool needs).
    pub scopes: Vec<String>,
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
            "scopes": self.scopes,
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

    /// Builds a Google Sign-In URL (`openid email profile`) using the same
    /// registered redirect URI (`/oauth2/callback`).
    pub fn login_authorization_url(&self, state: &str) -> String {
        let mut url = url::Url::parse(&self.auth_endpoint).expect("valid auth endpoint");
        url.query_pairs_mut()
            .append_pair("client_id", &self.config.client_id)
            .append_pair("redirect_uri", &self.config.redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", "openid email profile")
            .append_pair("prompt", "select_account")
            .append_pair("state", state);
        url.to_string()
    }

    /// Fetches the user's Google profile (`sub`, `email`, `name`, `picture`)
    /// from OpenID Connect userinfo using `access_token`.
    ///
    /// # Errors
    ///
    /// Returns an error if the userinfo request fails.
    pub async fn fetch_user_info(&self, access_token: &str) -> anyhow::Result<AuthenticatedUser> {
        let resp = self
            .http
            .get(GOOGLE_USERINFO_ENDPOINT)
            .bearer_auth(access_token)
            .send()
            .await?;
        let status = resp.status();
        let body = resp.text().await?;
        if !status.is_success() {
            anyhow::bail!("Google userinfo request failed ({status}): {body}");
        }
        let info: GoogleUserInfoResponse = serde_json::from_str(&body)?;
        let email = if info.email.is_empty() {
            info.sub.clone()
        } else {
            info.email
        };
        let id = email.clone();
        let name = if info.name.is_empty() {
            email.split('@').next().unwrap_or("User").to_string()
        } else {
            info.name
        };
        Ok(AuthenticatedUser {
            id,
            email,
            name,
            picture: info.picture.filter(|p| !p.is_empty()),
            iat: Utc::now().timestamp(),
        })
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

/// Identity of the chat session a credential belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionRef {
    /// ADK app name.
    pub app_name: String,
    /// User id of the session owner.
    pub user_id: String,
    /// Session id.
    pub session_id: String,
}

impl SessionRef {
    /// The session of the current invocation.
    pub fn from_context(ctx: &dyn ReadonlyContext) -> Self {
        Self {
            app_name: ctx.app_name().to_string(),
            user_id: ctx.user_id().to_string(),
            session_id: ctx.session_id().to_string(),
        }
    }

    /// Typed identity triple used by the session service.
    ///
    /// # Errors
    ///
    /// Returns an error when one of the identifiers is invalid.
    pub fn identity(&self) -> anyhow::Result<AdkIdentity> {
        Ok(AdkIdentity::new(
            AppName::new(self.app_name.clone())?,
            UserId::new(self.user_id.clone())?,
            SessionId::new(self.session_id.clone())?,
        ))
    }
}

impl std::fmt::Display for SessionRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}/{}", self.app_name, self.user_id, self.session_id)
    }
}

/// Real-time event broadcast to the web UI (`/oauth2/events`) so it can open
/// the consent popup immediately while a tool call waits in-flight.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OAuthUiEvent {
    /// A tool call is waiting for the user to grant `scopes` for `session_id`.
    AuthRequired {
        /// Signed state token identifying this authorization request.
        state: String,
        /// Session ID that requested authorization.
        session_id: String,
        /// Full Google OAuth 2.0 consent URL to open in a popup.
        auth_url: String,
        /// Requested OAuth scopes.
        scopes: Vec<String>,
    },
    /// The authorization flow for `session_id` finished (`"ok"` or `"error"`).
    AuthCompleted {
        /// Session ID that requested authorization.
        session_id: String,
        /// `"ok"` or `"error"`.
        status: String,
    },
}

/// In-flight authorization state for a single session.
#[derive(Clone)]
struct PendingSessionAuth {
    scopes: Vec<String>,
    auth_required: AuthRequired,
    tx: broadcast::Sender<Result<OAuthToken, String>>,
}

/// Claims carried by the signed OAuth `state` parameter.
#[derive(Debug, Serialize, Deserialize)]
struct StateClaims {
    #[serde(flatten)]
    session: SessionRef,
    /// Scopes the link asked for (fallback when the token response omits
    /// `scope`).
    #[serde(default)]
    scopes: Vec<String>,
    /// Issued-at (Unix seconds).
    iat: i64,
    /// Random nonce so repeated authorizations of one session differ.
    nonce: String,
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn unb64(text: &str) -> anyhow::Result<Vec<u8>> {
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(text)?)
}

fn merge_tokens(older: &OAuthToken, newer: &OAuthToken) -> OAuthToken {
    let mut scopes = older.scopes.clone();
    for s in &newer.scopes {
        if !scopes.contains(s) {
            scopes.push(s.clone());
        }
    }
    OAuthToken {
        access_token: newer.access_token.clone(),
        refresh_token: newer
            .refresh_token
            .clone()
            .or_else(|| older.refresh_token.clone()),
        expires_at: newer.expires_at.or(older.expires_at),
        scopes,
    }
}

/// Opens `url` in the system browser (used on localhost when the browser's
/// popup blocker rejects `window.open`).
fn open_in_system_browser(url: &str) -> bool {
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("/usr/bin/open");
        c.arg(url);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    };
    match cmd
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!("Could not open OAuth consent URL in system browser: {e}");
            false
        }
    }
}

/// Google credentials attached to the chat session.
///
/// Tokens are stored in the session state (key [`CREDENTIALS_STATE_KEY`]):
/// every session authorizes on its own. The OAuth `state` parameter is a
/// signed, short-lived claim naming the session, so the redirect handler can
/// write the exchanged tokens straight into that session.
///
/// When interactive waiting is enabled (`serve` mode), [`Self::access_token`]
/// broadcasts an [`OAuthUiEvent::AuthRequired`] event to `/ui/` (which opens
/// the consent popup immediately) and awaits the `/oauth2/callback` completion
/// in-flight, so the calling tool finishes in the same turn without asking the
/// LLM to print an authorization link in the chat window.
pub struct SessionCredentialManager {
    flow: GoogleOAuthFlow,
    /// HMAC key for the `state` parameter (derived from the client secret).
    state_key: [u8; 32],
    /// How long [`Self::access_token`] waits in-flight for the user to finish
    /// OAuth consent in the popup before falling back to [`AuthRequired`].
    wait_timeout: RwLock<Option<Duration>>,
    /// Whether to fall back to the OS browser opener on localhost when the
    /// browser tab reports that `window.open` was blocked (or no tab acks).
    system_browser_fallback: AtomicBool,
    /// Broadcast channel for real-time `/oauth2/events` SSE notifications.
    ui_events: broadcast::Sender<OAuthUiEvent>,
    /// Signals open `/oauth2/events` SSE streams to close when the server shuts down.
    shutdown: watch::Sender<bool>,
    /// In-flight authorizations keyed by session.
    pending: Mutex<HashMap<SessionRef, PendingSessionAuth>>,
    /// Popup-open acknowledgments keyed by signed `state`.
    acks: Mutex<HashMap<String, watch::Sender<Option<bool>>>>,
    /// Newly granted tokens for active sessions (bridges the gap between
    /// `/oauth2/callback` persisting to `SessionService` and the in-flight
    /// `ToolContext::session()` snapshot of the currently running turn).
    live_tokens: Mutex<HashMap<SessionRef, OAuthToken>>,
    /// Runtime model controller allowing the Studio UI to inspect and switch
    /// the active LLM, video, and image models.
    models: RwLock<Arc<dyn crate::config::ModelController>>,
}

impl std::fmt::Debug for SessionCredentialManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionCredentialManager")
            .field("redirect_uri", &self.flow.config.redirect_uri)
            .finish()
    }
}

impl SessionCredentialManager {
    /// Creates a manager for the given flow.
    pub fn new(flow: GoogleOAuthFlow) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"adspace-agent/oauth-state:");
        hasher.update(flow.config.client_secret.as_bytes());
        let (ui_events, _) = broadcast::channel(64);
        let (shutdown, _) = watch::channel(false);
        Self {
            flow,
            state_key: hasher.finalize().into(),
            wait_timeout: RwLock::new(None),
            system_browser_fallback: AtomicBool::new(false),
            ui_events,
            shutdown,
            pending: Mutex::new(HashMap::new()),
            acks: Mutex::new(HashMap::new()),
            live_tokens: Mutex::new(HashMap::new()),
            models: RwLock::new(Arc::new(crate::config::SimpleModelController::default())),
        }
    }

    /// Returns the runtime model controller attached to this credential manager.
    pub fn model_controller(&self) -> Arc<dyn crate::config::ModelController> {
        self.models
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// Attaches a runtime model controller (e.g. [`crate::agent::RuntimeModelConfig`]).
    pub fn set_model_controller(&self, controller: Arc<dyn crate::config::ModelController>) {
        *self.models.write().unwrap_or_else(|e| e.into_inner()) = controller;
    }

    /// Enables in-flight waiting inside [`Self::access_token`] for up to
    /// `timeout` while the popup consent flow completes. When
    /// `system_browser_fallback` is `true` and the redirect URI points at
    /// `localhost` / `127.0.0.1`, the server also opens the consent URL via
    /// the OS browser opener if the web UI's `window.open` is blocked.
    pub fn enable_interactive_wait(&self, timeout: Duration, system_browser_fallback: bool) {
        *self.wait_timeout.write().unwrap() = Some(timeout);
        self.system_browser_fallback
            .store(system_browser_fallback, Ordering::Relaxed);
    }

    /// Subscribes to real-time UI authorization events (`/oauth2/events`).
    pub fn subscribe_events(&self) -> broadcast::Receiver<OAuthUiEvent> {
        self.ui_events.subscribe()
    }

    /// Subscribes to the server shutdown signal so long-lived SSE streams can exit immediately.
    pub fn subscribe_shutdown(&self) -> watch::Receiver<bool> {
        self.shutdown.subscribe()
    }

    /// Signals all open `/oauth2/events` SSE streams to close immediately.
    pub fn notify_shutdown(&self) {
        let _ = self.shutdown.send(true);
    }

    /// Records whether the web UI succeeded in opening the popup for `state`.
    /// Returns `true` if the server opened (or will open) the URL via the
    /// local system browser fallback instead.
    pub fn ack_popup(&self, state: &str, opened: bool) -> bool {
        if let Some(tx) = self.acks.lock().unwrap().get(state)
            && *tx.borrow() != Some(true)
        {
            let _ = tx.send(Some(opened));
        }
        !opened && self.should_fallback_to_system_browser()
    }

    fn should_fallback_to_system_browser(&self) -> bool {
        if !self.system_browser_fallback.load(Ordering::Relaxed) {
            return false;
        }
        url::Url::parse(&self.flow.config.redirect_uri)
            .ok()
            .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
            .is_some_and(|h| h == "localhost" || h == "127.0.0.1" || h == "::1")
    }

    /// The OAuth flow (for building URLs in UIs).
    pub fn flow(&self) -> &GoogleOAuthFlow {
        &self.flow
    }

    /// The token stored in the session state, if any (no refresh performed).
    ///
    /// The runner's tool context exposes the live session through
    /// [`ToolContext::session`] (its [`ReadonlyContext::state`] is `None`),
    /// so that is consulted first; `state()` is the fallback for contexts
    /// that only implement the read-only view.
    pub fn stored_token(ctx: &dyn ToolContext) -> Option<OAuthToken> {
        let value = ctx
            .session()
            .and_then(|s| s.state().get(CREDENTIALS_STATE_KEY))
            .or_else(|| ctx.state().and_then(|s| s.get(CREDENTIALS_STATE_KEY)))?;
        if value.is_null() {
            return None;
        }
        match serde_json::from_value(value) {
            Ok(token) => Some(token),
            Err(e) => {
                tracing::warn!("Ignoring unreadable credentials in session state: {e}");
                None
            }
        }
    }

    fn current_token(&self, ctx: &dyn ToolContext, session: &SessionRef) -> Option<OAuthToken> {
        let from_ctx = Self::stored_token(ctx);
        let from_live = self.live_tokens.lock().unwrap().get(session).cloned();
        match (from_ctx, from_live) {
            (Some(a), Some(b)) => Some(merge_tokens(&a, &b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    /// Writes (or, with `None`, clears) the token in the session state through
    /// the tool's event actions.
    pub fn store_token(ctx: &dyn ToolContext, token: Option<&OAuthToken>) {
        let value = token
            .and_then(|t| serde_json::to_value(t).ok())
            .unwrap_or(serde_json::Value::Null);
        let mut actions = ctx.actions();
        actions
            .state_delta
            .insert(CREDENTIALS_STATE_KEY.to_string(), value);
        ctx.set_actions(actions);
    }

    /// Returns a valid access token for the invocation's session covering
    /// `scopes`, refreshing (and re-storing) it when expired.
    ///
    /// When interactive waiting is enabled (`serve` mode), this triggers the
    /// OAuth popup immediately and awaits the callback in-flight so the tool
    /// call completes seamlessly in the same turn.
    ///
    /// # Errors
    ///
    /// Returns [`AuthRequired`] (wrapped in `anyhow`) when interactive waiting
    /// is disabled or when the user denies/times out the authorization.
    pub async fn access_token(
        &self,
        ctx: &dyn ToolContext,
        scopes: &[String],
    ) -> anyhow::Result<String> {
        let session = SessionRef::from_context(ctx);
        if let Some(token) = self.current_token(ctx, &session) {
            if token.covers(scopes) {
                if !token.is_expired() {
                    if Self::stored_token(ctx).as_ref() != Some(&token) {
                        Self::store_token(ctx, Some(&token));
                    }
                    return Ok(token.access_token);
                }
                if token.refresh_token.is_some() {
                    match self.flow.refresh(&token).await {
                        Ok(refreshed) => {
                            let merged = merge_tokens(&token, &refreshed);
                            self.live_tokens
                                .lock()
                                .unwrap()
                                .insert(session.clone(), merged.clone());
                            Self::store_token(ctx, Some(&merged));
                            return Ok(merged.access_token);
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to refresh Google credentials of session {session}: {e}"
                            );
                            self.live_tokens.lock().unwrap().remove(&session);
                            Self::store_token(ctx, None);
                        }
                    }
                }
            } else {
                tracing::info!(
                    "Credentials of session {session} lack required scopes; requesting authorization"
                );
            }
        }

        let wait_timeout = *self.wait_timeout.read().unwrap();
        let Some(timeout) = wait_timeout else {
            return Err(self.begin_authorization(&session, scopes).into());
        };
        if !self.flow.config.is_configured() || session.session_id.is_empty() || scopes.is_empty() {
            return Err(self.begin_authorization(&session, scopes).into());
        }

        let (mut rx, auth_req) = self.start_or_join_pending(&session, scopes);
        match tokio::time::timeout(timeout, rx.recv()).await {
            Ok(Ok(Ok(token))) => {
                Self::store_token(ctx, Some(&token));
                Ok(token.access_token)
            }
            Ok(Ok(Err(reason))) => {
                tracing::warn!("Google authorization for session {session} failed: {reason}");
                Err(auth_req.into())
            }
            Ok(Err(_)) | Err(_) => {
                self.pending.lock().unwrap().remove(&session);
                Err(auth_req.into())
            }
        }
    }

    fn start_or_join_pending(
        &self,
        session: &SessionRef,
        scopes: &[String],
    ) -> (
        broadcast::Receiver<Result<OAuthToken, String>>,
        AuthRequired,
    ) {
        let mut pending = self.pending.lock().unwrap();
        if let Some(existing) = pending.get(session)
            && scopes.iter().all(|needed| {
                existing
                    .scopes
                    .iter()
                    .any(|granted| scope_covers(granted, needed))
            })
        {
            return (existing.tx.subscribe(), existing.auth_required.clone());
        }

        let mut combined: Vec<String> = pending
            .get(session)
            .map(|p| p.scopes.clone())
            .unwrap_or_default();
        for scope in scopes {
            if !combined.contains(scope) {
                combined.push(scope.clone());
            }
        }
        let (state, auth_req) = self.begin_authorization_with_state(session, &combined);
        let tx = pending
            .get(session)
            .map(|p| p.tx.clone())
            .unwrap_or_else(|| broadcast::channel(8).0);
        let rx = tx.subscribe();
        pending.insert(
            session.clone(),
            PendingSessionAuth {
                scopes: combined.clone(),
                auth_required: auth_req.clone(),
                tx,
            },
        );
        drop(pending);

        let (ack_tx, mut ack_rx) = watch::channel(None);
        self.acks.lock().unwrap().insert(state.clone(), ack_tx);

        let _ = self.ui_events.send(OAuthUiEvent::AuthRequired {
            state: state.clone(),
            session_id: session.session_id.clone(),
            auth_url: auth_req.auth_url.clone(),
            scopes: auth_req.scopes.clone(),
        });

        if self.should_fallback_to_system_browser() {
            let url = auth_req.auth_url.clone();
            tokio::spawn(async move {
                let opened_in_ui = tokio::time::timeout(
                    Duration::from_millis(450),
                    ack_rx.wait_for(|&v| v == Some(true)),
                )
                .await
                .is_ok();
                if !opened_in_ui {
                    tracing::info!(
                        "Opening Google OAuth consent screen in system browser (popup blocked or no active UI ack)"
                    );
                    open_in_system_browser(&url);
                }
            });
        }

        (rx, auth_req)
    }

    /// Starts an authorization for `session` asking for `scopes`
    /// (least privilege), returning the instructions.
    pub fn begin_authorization(&self, session: &SessionRef, scopes: &[String]) -> AuthRequired {
        self.begin_authorization_with_state(session, scopes).1
    }

    fn begin_authorization_with_state(
        &self,
        session: &SessionRef,
        scopes: &[String],
    ) -> (String, AuthRequired) {
        let unavailable = |message: &str| {
            (
                String::new(),
                AuthRequired {
                    auth_url: String::new(),
                    scopes: Vec::new(),
                    message: message.to_string(),
                },
            )
        };
        if !self.flow.config.is_configured() {
            return unavailable(
                "Google OAuth is not configured: set CLIENT_ID and CLIENT_SECRET and restart the agent.",
            );
        }
        if session.session_id.is_empty() {
            return unavailable("Google authorization needs an active chat session.");
        }
        let mut requested: Vec<String> = Vec::new();
        for scope in scopes {
            if !requested.contains(scope) {
                requested.push(scope.clone());
            }
        }
        if requested.is_empty() {
            return unavailable(
                "This tool declares no OAuth scope, so no authorization can be requested for it.",
            );
        }
        let state = self.sign_state(session, &requested);
        let auth_url = self.flow.authorization_url(&requested, &state);
        let names: Vec<&str> = requested
            .iter()
            .map(|s| s.rsplit('/').next().unwrap_or(s))
            .collect();
        (
            state,
            AuthRequired {
                auth_url: auth_url.clone(),
                scopes: requested.clone(),
                message: format!(
                    "Authorization required for this chat session (requested access: {}). Show the user this link as a markdown link labelled \"Authorize Google access\" and ask them to open it and grant the permission; the conversation resumes automatically once access is granted (if it does not, they can say \"done\"): {auth_url}",
                    names.join(", ")
                ),
            },
        )
    }

    /// Completes an authorization from the redirect handler: validates the
    /// `state`, exchanges the code and returns the session the tokens belong
    /// to together with the tokens (see [`Self::persist`]).
    ///
    /// # Errors
    ///
    /// Returns an error when `state` is invalid/expired or the code exchange
    /// fails.
    pub async fn complete_authorization(
        &self,
        state: &str,
        code: &str,
    ) -> anyhow::Result<(SessionRef, OAuthToken)> {
        self.acks.lock().unwrap().remove(state);
        let claims = self.verify_state(state)?;
        let token = self.flow.exchange_code(code, &claims.scopes).await?;
        let merged = match self.live_tokens.lock().unwrap().get(&claims.session) {
            Some(existing) => merge_tokens(existing, &token),
            None => token,
        };
        Ok((claims.session, merged))
    }

    /// Notifies any in-flight tool call waiting on `state` that authorization
    /// was denied or failed.
    pub fn fail_authorization(&self, state: &str, error: &str) {
        self.acks.lock().unwrap().remove(state);
        if let Ok(claims) = self.verify_state(state) {
            if let Some(pending) = self.pending.lock().unwrap().remove(&claims.session) {
                let _ = pending.tx.send(Err(error.to_string()));
            }
            let _ = self.ui_events.send(OAuthUiEvent::AuthCompleted {
                session_id: claims.session.session_id,
                status: "error".to_string(),
            });
        }
    }

    /// Whether an in-flight tool call is currently awaiting OAuth completion
    /// for `session`.
    pub fn has_pending_waiter(&self, session: &SessionRef) -> bool {
        self.pending
            .lock()
            .unwrap()
            .get(session)
            .is_some_and(|p| p.tx.receiver_count() > 0)
    }

    /// Stores `token` in the state of `session` by appending a state-only
    /// event, and wakes any in-flight tool call awaiting authorization for
    /// that session.
    ///
    /// # Errors
    ///
    /// Returns an error when the session does not exist or the write fails.
    pub async fn persist(
        &self,
        sessions: &dyn SessionService,
        session: &SessionRef,
        token: &OAuthToken,
    ) -> anyhow::Result<()> {
        let mut event = Event::new(format!("oauth2-{}", uuid::Uuid::new_v4().simple()));
        event.author = "system".to_string();
        event.actions.state_delta.insert(
            CREDENTIALS_STATE_KEY.to_string(),
            serde_json::to_value(token)?,
        );
        sessions
            .append_event_for_identity(AppendEventRequest {
                identity: session.identity()?,
                event,
            })
            .await
            .map_err(|e| {
                anyhow::anyhow!("failed to store credentials in session {session}: {e}")
            })?;
        self.live_tokens
            .lock()
            .unwrap()
            .insert(session.clone(), token.clone());
        if let Some(pending) = self.pending.lock().unwrap().remove(session) {
            let _ = pending.tx.send(Ok(token.clone()));
        }
        let _ = self.ui_events.send(OAuthUiEvent::AuthCompleted {
            session_id: session.session_id.clone(),
            status: "ok".to_string(),
        });
        tracing::info!("Stored Google credentials in session {session}");
        Ok(())
    }

    fn mac(&self) -> HmacSha256 {
        HmacSha256::new_from_slice(&self.state_key).expect("HMAC accepts any key length")
    }

    /// Builds the signed `state` parameter naming `session` and `scopes`.
    fn sign_state(&self, session: &SessionRef, scopes: &[String]) -> String {
        let claims = StateClaims {
            session: session.clone(),
            scopes: scopes.to_vec(),
            iat: Utc::now().timestamp(),
            nonce: uuid::Uuid::new_v4().simple().to_string(),
        };
        let payload = b64(&serde_json::to_vec(&claims).expect("claims serialize"));
        let mut mac = self.mac();
        mac.update(payload.as_bytes());
        format!("{payload}.{}", b64(&mac.finalize().into_bytes()))
    }

    /// Validates a `state` parameter and returns its claims.
    fn verify_state(&self, state: &str) -> anyhow::Result<StateClaims> {
        let invalid = || {
            anyhow::anyhow!(
                "Invalid or expired OAuth state parameter; please restart the authorization from the chat."
            )
        };
        let (payload, signature) = state.split_once('.').ok_or_else(invalid)?;
        let signature = unb64(signature).map_err(|_| invalid())?;
        let mut mac = self.mac();
        mac.update(payload.as_bytes());
        mac.verify_slice(&signature).map_err(|_| invalid())?;
        let claims: StateClaims = serde_json::from_slice(&unb64(payload).map_err(|_| invalid())?)
            .map_err(|_| invalid())?;
        let age = Utc::now().timestamp() - claims.iat;
        if age < -60 || age > PENDING_TTL.as_secs() as i64 {
            return Err(invalid());
        }
        Ok(claims)
    }

    /// Starts a Google Sign-In flow (`openid email profile`), returning the
    /// Google OAuth URL whose signed `state` carries [`LOGIN_SENTINEL_APP`].
    ///
    /// # Errors
    ///
    /// Returns an error when `CLIENT_ID` or `CLIENT_SECRET` is not configured.
    pub fn begin_user_login(&self) -> anyhow::Result<String> {
        if !self.flow.config.is_configured() {
            anyhow::bail!(
                "Google OAuth is not configured: set CLIENT_ID and CLIENT_SECRET in .env and restart the server."
            );
        }
        let session = SessionRef {
            app_name: LOGIN_SENTINEL_APP.to_string(),
            user_id: "login".to_string(),
            session_id: "login".to_string(),
        };
        let scopes = vec![
            "openid".to_string(),
            "email".to_string(),
            "profile".to_string(),
        ];
        let state = self.sign_state(&session, &scopes);
        Ok(self.flow.login_authorization_url(&state))
    }

    /// Signs `user` into a tamper-proof cookie value (`<payload>.<sig>`).
    pub fn sign_user_cookie(&self, user: &AuthenticatedUser) -> String {
        let payload = b64(&serde_json::to_vec(user).expect("user serializes"));
        let mut mac = self.mac();
        mac.update(b"user-cookie:");
        mac.update(payload.as_bytes());
        format!("{payload}.{}", b64(&mac.finalize().into_bytes()))
    }

    /// Verifies a signed user cookie value and returns the [`AuthenticatedUser`]
    /// if valid and not older than 7 days.
    pub fn verify_user_cookie(&self, value: &str) -> Option<AuthenticatedUser> {
        let (payload, signature) = value.split_once('.')?;
        let signature = unb64(signature).ok()?;
        let mut mac = self.mac();
        mac.update(b"user-cookie:");
        mac.update(payload.as_bytes());
        mac.verify_slice(&signature).ok()?;
        let user: AuthenticatedUser = serde_json::from_slice(&unb64(payload).ok()?).ok()?;
        let age = Utc::now().timestamp() - user.iat;
        if (-60..=7 * 86_400).contains(&age) {
            Some(user)
        } else {
            None
        }
    }
}

/// [`AccessTokenProvider`] bound to one tool invocation (hence one session)
/// and a set of scopes.
#[derive(Clone)]
pub struct SessionTokenProvider {
    manager: Arc<SessionCredentialManager>,
    ctx: Arc<dyn ToolContext>,
    scopes: Vec<String>,
}

impl SessionTokenProvider {
    /// Creates a provider for the session of `ctx` needing `scopes`.
    pub fn new(
        manager: Arc<SessionCredentialManager>,
        ctx: Arc<dyn ToolContext>,
        scopes: Vec<String>,
    ) -> Self {
        Self {
            manager,
            ctx,
            scopes,
        }
    }
}

#[async_trait]
impl AccessTokenProvider for SessionTokenProvider {
    async fn access_token(&self) -> anyhow::Result<String> {
        self.manager
            .access_token(self.ctx.as_ref(), &self.scopes)
            .await
    }
}

/// Extracts an [`AuthRequired`] from an error chain, if present.
pub fn auth_required(err: &anyhow::Error) -> Option<&AuthRequired> {
    err.chain().find_map(|e| e.downcast_ref::<AuthRequired>())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex as StdMutex;

    use adk_rust::session::{CreateRequest, GetRequest, InMemorySessionService};
    use adk_rust::{
        Artifacts, CallbackContext, Content, EventActions, MemoryEntry, Session, State,
    };
    use serde_json::{Value, json};

    use super::*;

    fn config() -> OAuthClientConfig {
        OAuthClientConfig {
            client_id: "cid".into(),
            client_secret: "secret".into(),
            redirect_uri: "http://localhost:8000/oauth2/callback".into(),
        }
    }

    fn manager() -> SessionCredentialManager {
        SessionCredentialManager::new(GoogleOAuthFlow::new(config()))
    }

    fn session() -> SessionRef {
        SessionRef {
            app_name: "adspace_agent".into(),
            user_id: "local-user".into(),
            session_id: "s-1".into(),
        }
    }

    /// Tool context shaped like the runner's: the session (and its state) is
    /// reachable through `ToolContext::session()`, while
    /// `ReadonlyContext::state()` stays at its default `None`.
    struct FakeToolContext {
        state: StdMutex<HashMap<String, Value>>,
        actions: StdMutex<EventActions>,
        content: Content,
    }

    impl FakeToolContext {
        fn with_state(entries: &[(&str, Value)]) -> Arc<dyn ToolContext> {
            Arc::new(Self {
                state: StdMutex::new(
                    entries
                        .iter()
                        .map(|(k, v)| (k.to_string(), v.clone()))
                        .collect(),
                ),
                actions: StdMutex::new(EventActions::default()),
                content: Content::new("user"),
            })
        }
    }

    impl State for FakeToolContext {
        fn get(&self, key: &str) -> Option<Value> {
            self.state.lock().unwrap().get(key).cloned()
        }
        fn set(&mut self, key: String, value: Value) {
            self.state.lock().unwrap().insert(key, value);
        }
        fn all(&self) -> HashMap<String, Value> {
            self.state.lock().unwrap().clone()
        }
    }

    impl ReadonlyContext for FakeToolContext {
        fn invocation_id(&self) -> &str {
            "inv"
        }
        fn agent_name(&self) -> &str {
            "adspace_agent"
        }
        fn user_id(&self) -> &str {
            "local-user"
        }
        fn app_name(&self) -> &str {
            "adspace_agent"
        }
        fn session_id(&self) -> &str {
            "s-1"
        }
        fn branch(&self) -> &str {
            ""
        }
        fn user_content(&self) -> &Content {
            &self.content
        }
    }

    impl Session for FakeToolContext {
        fn id(&self) -> &str {
            "s-1"
        }
        fn app_name(&self) -> &str {
            "adspace_agent"
        }
        fn user_id(&self) -> &str {
            "local-user"
        }
        fn state(&self) -> &dyn State {
            self
        }
        fn conversation_history(&self) -> Vec<Content> {
            Vec::new()
        }
    }

    #[async_trait]
    impl CallbackContext for FakeToolContext {
        fn artifacts(&self) -> Option<Arc<dyn Artifacts>> {
            None
        }
    }

    #[async_trait]
    impl ToolContext for FakeToolContext {
        fn function_call_id(&self) -> &str {
            "call"
        }
        fn actions(&self) -> EventActions {
            self.actions.lock().unwrap().clone()
        }
        fn set_actions(&self, actions: EventActions) {
            *self.actions.lock().unwrap() = actions;
        }
        async fn search_memory(&self, _query: &str) -> adk_rust::Result<Vec<MemoryEntry>> {
            Ok(vec![])
        }
        fn session(&self) -> Option<&dyn Session> {
            Some(self)
        }
    }

    fn state_param(url: &str) -> String {
        url::Url::parse(url)
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.to_string())
            .unwrap()
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

    #[test]
    fn a_full_scope_covers_its_read_only_variant_only() {
        let drive = "https://www.googleapis.com/auth/drive";
        let token = OAuthToken {
            access_token: "a".into(),
            refresh_token: None,
            expires_at: None,
            scopes: vec![drive.to_string()],
        };
        assert!(token.covers(&[format!("{drive}.readonly")]));
        assert!(token.covers(&[drive.to_string()]));
        assert!(
            !token.covers(&[format!("{drive}.file")]),
            "narrower, but not read-only"
        );
        assert!(!token.covers(&["https://www.googleapis.com/auth/youtube".to_string()]));
        // The reverse never holds: read-only does not grant writes.
        let read_only = OAuthToken {
            scopes: vec![format!("{drive}.readonly")],
            ..token
        };
        assert!(!read_only.covers(&[drive.to_string()]));
    }

    #[test]
    fn state_parameter_round_trips_and_rejects_tampering() {
        let manager = manager();
        let state = manager.sign_state(&session(), &["s1".to_string()]);
        let claims = manager.verify_state(&state).unwrap();
        assert_eq!(claims.session, session());
        assert_eq!(claims.scopes, vec!["s1".to_string()]);

        // A different client secret yields a different key.
        let other = SessionCredentialManager::new(GoogleOAuthFlow::new(OAuthClientConfig {
            client_secret: "other".into(),
            ..config()
        }));
        assert!(other.verify_state(&state).is_err());

        // Tampered payload (another session id) fails the signature check.
        let (payload, sig) = state.split_once('.').unwrap();
        let mut claims: Value = serde_json::from_slice(&unb64(payload).unwrap()).unwrap();
        claims["session_id"] = json!("s-2");
        let forged = format!("{}.{sig}", b64(&serde_json::to_vec(&claims).unwrap()));
        assert!(manager.verify_state(&forged).is_err());
        assert!(manager.verify_state("garbage").is_err());

        // Expired claims are rejected even with a valid signature.
        let old = StateClaims {
            session: session(),
            scopes: Vec::new(),
            iat: Utc::now().timestamp() - PENDING_TTL.as_secs() as i64 - 1,
            nonce: "n".into(),
        };
        let payload = b64(&serde_json::to_vec(&old).unwrap());
        let mut mac = manager.mac();
        mac.update(payload.as_bytes());
        let expired = format!("{payload}.{}", b64(&mac.finalize().into_bytes()));
        assert!(manager.verify_state(&expired).is_err());
    }

    #[tokio::test]
    async fn requests_only_the_tools_scopes_bound_to_the_session() {
        let manager = manager();
        let ctx = FakeToolContext::with_state(&[]);
        let err = manager
            .access_token(
                ctx.as_ref(),
                &["tool-scope".to_string(), "tool-scope".to_string()],
            )
            .await
            .unwrap_err();
        let required = auth_required(&err).expect("AuthRequired error");
        // Least privilege: exactly the (deduplicated) scopes of the call.
        assert!(required.auth_url.contains("&scope=tool-scope&"));
        assert_eq!(required.scopes, vec!["tool-scope".to_string()]);
        let response = required.to_tool_response();
        assert_eq!(response["status"], "AUTH_REQUIRED");
        assert_eq!(response["scopes"], json!(["tool-scope"]));
        assert!(required.message.contains("requested access: tool-scope"));
        let claims = manager
            .verify_state(&state_param(&required.auth_url))
            .unwrap();
        assert_eq!(claims.session, session());
        assert_eq!(claims.scopes, vec!["tool-scope".to_string()]);
        assert!(
            manager
                .complete_authorization("nope", "code")
                .await
                .is_err()
        );
        // A tool without scopes cannot be authorized.
        let err = manager.access_token(ctx.as_ref(), &[]).await.unwrap_err();
        assert!(
            auth_required(&err)
                .unwrap()
                .message
                .contains("declares no OAuth scope")
        );
    }

    #[tokio::test]
    async fn uses_valid_token_from_session_state() {
        let token = OAuthToken {
            access_token: "tok".into(),
            refresh_token: None,
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
            scopes: vec![],
        };
        let ctx = FakeToolContext::with_state(&[(
            CREDENTIALS_STATE_KEY,
            serde_json::to_value(&token).unwrap(),
        )]);
        assert_eq!(
            manager()
                .access_token(ctx.as_ref(), &["s".to_string()])
                .await
                .unwrap(),
            "tok"
        );
        assert!(
            ctx.actions().state_delta.is_empty(),
            "no write when nothing changed"
        );

        // Missing scopes trigger a new authorization for this session.
        let narrow = OAuthToken {
            scopes: vec!["other".into()],
            ..token
        };
        let ctx = FakeToolContext::with_state(&[(
            CREDENTIALS_STATE_KEY,
            serde_json::to_value(&narrow).unwrap(),
        )]);
        let err = manager()
            .access_token(ctx.as_ref(), &["s".to_string()])
            .await
            .unwrap_err();
        assert!(auth_required(&err).is_some());
    }

    #[tokio::test]
    async fn expired_token_without_refresh_token_requires_authorization() {
        let token = OAuthToken {
            access_token: "old".into(),
            refresh_token: None,
            expires_at: Some(Utc::now() - chrono::Duration::hours(1)),
            scopes: vec![],
        };
        let ctx = FakeToolContext::with_state(&[(
            CREDENTIALS_STATE_KEY,
            serde_json::to_value(&token).unwrap(),
        )]);
        let err = manager().access_token(ctx.as_ref(), &[]).await.unwrap_err();
        assert!(auth_required(&err).is_some());
        // Null and garbage values are treated as "no credentials".
        let ctx = FakeToolContext::with_state(&[(CREDENTIALS_STATE_KEY, Value::Null)]);
        assert!(SessionCredentialManager::stored_token(ctx.as_ref()).is_none());
        let ctx = FakeToolContext::with_state(&[(CREDENTIALS_STATE_KEY, json!("nope"))]);
        assert!(SessionCredentialManager::stored_token(ctx.as_ref()).is_none());
    }

    #[tokio::test]
    async fn store_token_writes_and_clears_through_actions() {
        let ctx = FakeToolContext::with_state(&[]);
        let token = OAuthToken {
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            expires_at: None,
            scopes: vec!["s".into()],
        };
        SessionCredentialManager::store_token(ctx.as_ref(), Some(&token));
        assert_eq!(
            ctx.actions().state_delta[CREDENTIALS_STATE_KEY],
            serde_json::to_value(&token).unwrap()
        );
        SessionCredentialManager::store_token(ctx.as_ref(), None);
        assert_eq!(
            ctx.actions().state_delta[CREDENTIALS_STATE_KEY],
            Value::Null
        );
    }

    #[tokio::test]
    async fn unconfigured_client_gives_actionable_message() {
        let manager = SessionCredentialManager::new(GoogleOAuthFlow::new(OAuthClientConfig {
            client_id: String::new(),
            client_secret: String::new(),
            redirect_uri: String::new(),
        }));
        let ctx = FakeToolContext::with_state(&[]);
        let err = manager.access_token(ctx.as_ref(), &[]).await.unwrap_err();
        assert!(auth_required(&err).unwrap().message.contains("CLIENT_ID"));
    }

    #[tokio::test]
    async fn persist_writes_token_into_the_session_state() {
        let sessions = InMemorySessionService::new();
        let created = sessions
            .create(CreateRequest {
                app_name: "adspace_agent".into(),
                user_id: "local-user".into(),
                session_id: Some("s-1".into()),
                state: HashMap::new(),
            })
            .await
            .unwrap();
        assert_eq!(created.id(), "s-1");
        let token = OAuthToken {
            access_token: "a".into(),
            refresh_token: Some("r".into()),
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
            scopes: vec!["s".into()],
        };
        manager()
            .persist(&sessions, &session(), &token)
            .await
            .unwrap();
        let loaded = sessions
            .get(GetRequest {
                app_name: "adspace_agent".into(),
                user_id: "local-user".into(),
                session_id: "s-1".into(),
                num_recent_events: None,
                after: None,
            })
            .await
            .unwrap();
        assert_eq!(
            loaded.state().get(CREDENTIALS_STATE_KEY),
            Some(serde_json::to_value(&token).unwrap())
        );
        // The event carries no content, so it never reaches the model.
        assert_eq!(loaded.events().len(), 1);
        assert!(
            loaded
                .events()
                .all()
                .iter()
                .all(|e| e.llm_response.content.is_none())
        );

        // Unknown sessions are reported.
        let missing = SessionRef {
            session_id: "nope".into(),
            ..session()
        };
        assert!(
            manager()
                .persist(&sessions, &missing, &token)
                .await
                .is_err()
        );
    }
}
