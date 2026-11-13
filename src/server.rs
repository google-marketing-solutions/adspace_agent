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

//! HTTP server (ADK web UI + REST API + OAuth callback) and console REPL.
//!
//! Both modes are built on adk-rust's [`Launcher`] (from `adk-cli`): the
//! console mode is its readline REPL, and the server is its Axum application
//! with our OAuth redirect route merged in. The redirect handler writes the
//! exchanged Google tokens into the chat session named by the signed `state`
//! parameter, so credentials are attached to sessions, never to the server —
//! and then resumes that chat with a turn of its own, so the user does not
//! have to come back and say "done" (the web UI polls the session and shows
//! the continuation).

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use adk_rust::artifact::{ArtifactService, LoadRequest, SaveRequest};
use adk_rust::server::shutdown_signal;
use adk_rust::server::{RequestContext, RequestContextError, RequestContextExtractor};
use adk_rust::session::SessionService;
use adk_rust::{CacheCapable, Launcher};
use async_trait::async_trait;
use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{Path, Query, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{AppendHeaders, Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use futures::stream::StreamExt as _;

use crate::agent::{AgentRuntime, TurnRunner};
use crate::auth::oauth::OAUTH_CALLBACK_PATH;
use crate::auth::{
    AuthenticatedUser, CREDENTIALS_STATE_KEY, LOGIN_SENTINEL_APP, OAuthToken,
    SessionCredentialManager, SessionRef, USER_CREDENTIALS_STATE_KEY,
};
use crate::config::{APP_NAME, PRESET_IMAGE_MODELS, PRESET_MODELS, PRESET_VIDEO_MODELS};

/// Query parameters Google sends to the OAuth redirect URI.
#[derive(Debug, serde::Deserialize)]
pub struct OAuthCallbackQuery {
    /// Authorization code.
    pub code: Option<String>,
    /// Signed state we generated when starting the flow.
    pub state: Option<String>,
    /// Error code when the user denied access.
    pub error: Option<String>,
}

/// Who may change the server-wide LLM / image / video models through
/// `POST /auth/models` (`ADMIN_EMAILS`, comma-separated, case-insensitive).
///
/// The models are process-global — switching them affects every user — so
/// when Google Sign-In is configured only listed administrators may do it.
/// Without Sign-In (local development, single anonymous user) an empty list
/// means "anyone", because there is no identity to check.
#[derive(Clone, Debug, Default)]
pub struct AdminAllowList(Arc<Vec<String>>);

impl AdminAllowList {
    /// Reads `ADMIN_EMAILS` from the environment.
    pub fn from_env() -> Self {
        Self::new(
            std::env::var("ADMIN_EMAILS")
                .unwrap_or_default()
                .split(',')
                .map(str::to_string),
        )
    }

    /// Builds the list from email addresses (blank entries are ignored).
    pub fn new(emails: impl IntoIterator<Item = String>) -> Self {
        Self(Arc::new(
            emails
                .into_iter()
                .map(|e| e.trim().to_ascii_lowercase())
                .filter(|e| !e.is_empty())
                .collect(),
        ))
    }

    /// Whether no administrator is configured.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether `email` is an administrator.
    pub fn allows(&self, email: &str) -> bool {
        let email = email.trim().to_ascii_lowercase();
        self.0.contains(&email)
    }
}

/// Shared state of the OAuth routes.
#[derive(Clone)]
pub struct OAuthRoutes {
    /// OAuth client + state signing.
    pub credentials: Arc<SessionCredentialManager>,
    /// Where the exchanged tokens are written.
    pub sessions: Arc<dyn SessionService>,
    /// When set, the chat is resumed automatically after a successful
    /// authorization (see [`resume_message`]).
    pub resume: Option<Arc<TurnRunner>>,
    /// Users allowed to switch the server-wide models.
    pub admins: AdminAllowList,
}

/// The user-role message that resumes a chat after Google access was granted.
pub fn resume_message(token: &OAuthToken) -> String {
    let granted: Vec<&str> = token
        .scopes
        .iter()
        .map(|s| s.rsplit('/').next().unwrap_or(s))
        .collect();
    if granted.is_empty() {
        "Google access has been granted for this chat. Continue with the request that needed it."
            .to_string()
    } else {
        format!(
            "Google access has been granted for this chat ({}). Continue with the request that needed it.",
            granted.join(", ")
        )
    }
}

/// Resumes `session` in the background after an authorization.
fn spawn_resume(runner: Arc<TurnRunner>, session: SessionRef, token: &OAuthToken) {
    let message = resume_message(token);
    tokio::spawn(async move {
        match runner.run_text(&session, &message).await {
            Ok(events) => tracing::info!(
                "Resumed session {session} after Google authorization ({events} events)"
            ),
            Err(e) => {
                tracing::warn!("Could not resume session {session} after authorization: {e:#}")
            }
        }
    });
}

/// Path of the browser helper that turns auth links into a popup flow.
pub const AUTH_HELPER_PATH: &str = "/oauth2/helper.js";
/// Path of the SSE stream delivering real-time `auth_required` events to `/ui/`.
pub const AUTH_EVENTS_PATH: &str = "/oauth2/events";
/// Path where `/ui/` reports whether `window.open` succeeded.
pub const AUTH_ACK_PATH: &str = "/oauth2/ack";
/// Cookie name storing the signed Google user profile for `/app/`.
pub const USER_COOKIE_NAME: &str = "adspace_user";
/// Cookie binding a Google Sign-In started by `/auth/login` to the browser
/// that started it; only sent back to the callback path.
pub const LOGIN_NONCE_COOKIE_NAME: &str = "adspace_login";
/// Lifetime of the `adspace_user` cookie (matches `verify_user_cookie`).
const USER_COOKIE_MAX_AGE_SECS: u64 = 7 * 86_400;
/// Lifetime of the sign-in nonce cookie (matches the OAuth state TTL).
const LOGIN_NONCE_MAX_AGE_SECS: u64 = 15 * 60;
/// Largest JSON API response that is buffered to strip credentials from it;
/// bigger ones are refused (never served unscrubbed).
const MAX_SCRUBBED_JSON_BYTES: usize = 64 * 1024 * 1024;
/// Largest single SSE event that is buffered to strip credentials from it.
const MAX_SCRUBBED_SSE_EVENT_BYTES: usize = 32 * 1024 * 1024;
/// Default time in-flight requests get to finish after SIGTERM
/// (`SHUTDOWN_GRACE_SECS`; Cloud Run allows 10 s).
const DEFAULT_SHUTDOWN_GRACE: Duration = Duration::from_secs(8);
/// Module path (relative to `/app/`) of the `/ui/` helper script, which
/// imports the shared `adk-ui` renderer; see the file for what it does.
const AUTH_HELPER_MODULE: &str = "ui-helper.js";
/// Tag injected into the web UI page. A module script so the helper can
/// `import` the renderer from `/app/`; modules are deferred by default.
const AUTH_HELPER_TAG: &str = "<script type=\"module\" src=\"/oauth2/helper.js\"></script>";
/// Custom AdSpace Agent studio interface HTML (`/app/`).
const STUDIO_HTML: &str = include_str!("studio.html");
/// Custom AdSpace Agent studio interface CSS (`/app/app.css`).
const STUDIO_CSS: &str = include_str!("studio.css");
/// Stylesheet of the `adk-ui` renderer shared by `/app/` and `/ui/` (`/app/adk-ui.css`).
const ADK_UI_CSS: &str = include_str!("adk-ui.css");

/// Expands a list of module paths into the `match` arms of [`app_module`] so
/// the served file list lives in exactly one place.
macro_rules! app_modules {
    ($($path:literal),* $(,)?) => {
        /// Resolves a path relative to `/app/` to the embedded ES module
        /// source, or `None` for anything that is not a module we ship.
        fn app_module(name: &str) -> Option<&'static str> {
            match name {
                $($path => Some(include_str!(concat!("app/", $path))),)*
                _ => None,
            }
        }
    };
}

// Custom AdSpace Agent studio client, served as native ES modules under
// `/app/<path>` (`/app/app.js` is the entry point loaded by `studio.html`;
// `ui-helper.js` is the `/ui/` entry point, also served as `/oauth2/helper.js`).
app_modules! {
    "app.js",
    "adk-ui/charts.js",
    "adk-ui/components.js",
    "adk-ui/index.js",
    "adk-ui/modal.js",
    "adk-ui/tables.js",
    "auth.js",
    "dom.js",
    "format.js",
    "markdown.js",
    "oauth-popup.js",
    "prefs.js",
    "sessions.js",
    "settings-ui.js",
    "sse.js",
    "stage-bridge.js",
    "stage.js",
    "state.js",
    "status.js",
    "transcript.js",
    "ui-helper.js",
    "usage/counter.js",
    "usage/pricing.js",
    "usage/session-usage.js",
    "usage/telemetry.js",
}

/// Inline robot SVG favicon served at `/favicon.svg` and `/favicon.ico`.
const STUDIO_FAVICON_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 -960 960 960\">\
<defs><linearGradient id=\"g\" x1=\"120\" y1=\"-840\" x2=\"840\" y2=\"-120\" gradientUnits=\"userSpaceOnUse\">\
<stop offset=\"0%\" stop-color=\"#1a73e8\"/>\
<stop offset=\"40%\" stop-color=\"#5b6cf9\"/>\
<stop offset=\"75%\" stop-color=\"#9b72cb\"/>\
<stop offset=\"100%\" stop-color=\"#d96570\"/>\
</linearGradient></defs>\
<path fill=\"url(#g)\" d=\"M409.5-510.59q20.5-20.59 20.5-49.5t-20.59-49.41q-20.59-20.5-49.5-20.5t-49.41 20.59q-20.5 20.59-20.5 49.5t20.59 49.41q20.59 20.5 49.5 20.5t49.41-20.59Zm240 0q20.5-20.59 20.5-49.5t-20.59-49.41q-20.59-20.5-49.5-20.5t-49.41 20.59q-20.5 20.59-20.5 49.5t20.59 49.41q20.59 20.5 49.5 20.5t49.41-20.59ZM350-120v-170h100v170H350Zm160 0v-170h100v170H510Zm-330 0q-24.75 0-42.37-17.63Q120-155.25 120-180v-420q0-100 70-170t170-70h240q100 0 170 70t70 170v420q0 24.75-17.62 42.37Q804.75-120 780-120H670v-170q0-24.75-17.62-42.38Q634.75-350 610-350H350q-24.75 0-42.37 17.62Q290-314.75 290-290v170H180Z\"/>\
</svg>";

/// Body of `POST /oauth2/ack`.
#[derive(Debug, serde::Deserialize)]
pub struct OAuthAckRequest {
    /// Opaque `request_id` from the `auth_required` event.
    pub request_id: String,
    /// Whether `window.open` succeeded in the browser tab.
    pub opened: bool,
}

/// Script run by the callback page: tells the chat tab how the authorization
/// went — over a same-origin `BroadcastChannel`, because Google's sign-in
/// pages set COOP `same-origin` and sever `window.opener` (kept as a second
/// path) — and, on success, closes the popup immediately.
fn callback_script(status: &str) -> String {
    format!(
        "(function(){{var m={{type:\"adspace:oauth\",status:\"{status}\"}};\
         try{{if(\"BroadcastChannel\" in window){{new BroadcastChannel(\"adspace:oauth\").postMessage(m);}}}}catch(e){{}}\
         try{{if(window.opener&&!window.opener.closed){{window.opener.postMessage(m,location.origin);}}}}catch(e){{}}\
         {}}})();",
        if status == "ok" || status == "login_ok" {
            "try{window.close();}catch(e){}setTimeout(function(){try{window.close();}catch(e){}if(!window.closed){location.replace(\"/app/\");}},180);"
        } else {
            ""
        }
    )
}

fn escape_html(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn html_page(title: &str, body: &str, status: &str) -> Html<String> {
    let safe_title = escape_html(title);
    Html(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{safe_title}</title>\
         <style>body{{font-family:\"Google Sans Flex\",\"Google Sans\",-apple-system,BlinkMacSystemFont,sans-serif;background:#f8f9fc;color:#121317;margin:0;min-height:100vh;display:grid;place-items:center;padding:24px;box-sizing:border-box}}\
         .card{{background:#ffffff;border:1px solid rgba(33,34,38,0.08);border-radius:28px;padding:32px 36px;max-width:440px;width:100%;box-shadow:0 16px 40px -12px rgba(18,19,23,0.08),0 2px 8px -2px rgba(18,19,23,0.04)}}\
         h1{{font-size:22px;font-weight:450;letter-spacing:-0.012em;margin:0 0 10px}}p{{font-size:15px;line-height:1.55;color:#45474d;margin:0}}\
         code{{background:#f8f9fc;border:1px solid rgba(33,34,38,0.08);padding:2px 6px;border-radius:6px;font-size:13px}}</style></head>\
         <body><div class=\"card\"><h1>{safe_title}</h1>{body}</div><script>{}</script></body></html>",
        callback_script(status)
    ))
}

/// Serves the browser helper (`/oauth2/helper.js`, the same bytes as
/// `/app/ui-helper.js`).
async fn auth_helper(headers: HeaderMap) -> Response {
    static_asset_response(
        &headers,
        "application/javascript; charset=utf-8",
        app_module(AUTH_HELPER_MODULE).expect("ui-helper.js is an embedded module"),
    )
}

fn static_asset_response(
    req_headers: &HeaderMap,
    content_type: &'static str,
    body: &'static str,
) -> Response {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(body.as_bytes());
    // Weak validator: the compression layer may vary the exact bytes on the
    // wire, but the embedded asset content is what the tag identifies.
    let mut opaque = String::with_capacity(18);
    opaque.push('"');
    for b in &digest[..8] {
        use std::fmt::Write as _;
        let _ = write!(&mut opaque, "{b:02x}");
    }
    opaque.push('"');
    let etag = format!("W/{opaque}");

    // Weak comparison (RFC 9110 §8.8.3.2): ignore `W/` on both sides.
    let matches_etag = req_headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|raw| {
            raw.split(',').any(|candidate| {
                let candidate = candidate.trim();
                let stripped = candidate.strip_prefix("W/").unwrap_or(candidate);
                stripped == "*" || stripped == opaque
            })
        });

    let etag_val = HeaderValue::from_str(&etag).expect("ascii hex etag");
    if matches_etag {
        return (
            StatusCode::NOT_MODIFIED,
            [
                (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
                (header::ETAG, etag_val),
            ],
        )
            .into_response();
    }

    (
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
            (header::ETAG, etag_val),
        ],
        body,
    )
        .into_response()
}

/// Streams real-time `auth_required` / `auth_completed` events to `/ui/` so a
/// popup opens the instant an in-flight tool call needs Google OAuth consent.
///
/// The stream is bound to the signed-in user: it requires the `adspace_user`
/// cookie (or, without Google Sign-In, serves the anonymous default user) and
/// only delivers events for that user's sessions.
async fn oauth_events(State(routes): State<OAuthRoutes>, headers: HeaderMap) -> Response {
    let Some(user_id) = request_user_id(&headers, &routes.credentials) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let rx = routes.credentials.subscribe_events();
    let shutdown = routes.credentials.subscribe_shutdown();
    let stream = futures::stream::unfold(
        (rx, shutdown, user_id),
        |(mut rx, mut shutdown, user_id)| async move {
            if *shutdown.borrow() {
                return None;
            }
            loop {
                let next = tokio::select! {
                    _ = shutdown.changed() => None,
                    res = rx.recv() => Some(res),
                };
                match next? {
                    Ok(event) if event.user_id() == user_id => {
                        if let Ok(json) = serde_json::to_string(&event) {
                            return Some((
                                Ok::<_, Infallible>(SseEvent::default().data(json)),
                                (rx, shutdown, user_id),
                            ));
                        }
                    }
                    Ok(_) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                }
            }
        },
    )
    .fuse();
    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Records whether `/ui/` succeeded in opening the OAuth popup via `window.open`.
async fn oauth_ack(
    State(routes): State<OAuthRoutes>,
    headers: HeaderMap,
    Json(req): Json<OAuthAckRequest>,
) -> Response {
    let Some(user_id) = request_user_id(&headers, &routes.credentials) else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let fallback_opened = routes
        .credentials
        .ack_popup(&req.request_id, &user_id, req.opened);
    Json(serde_json::json!({ "fallback_opened": fallback_opened })).into_response()
}

/// Inserts the helper `<script>` tag before `</body>` (or at the end).
pub fn inject_auth_helper_tag(html: &str) -> String {
    match html.rfind("</body>").or_else(|| html.rfind("</BODY>")) {
        Some(at) => format!("{}{AUTH_HELPER_TAG}{}", &html[..at], &html[at..]),
        None => format!("{html}{AUTH_HELPER_TAG}"),
    }
}

/// Middleware that redirects `/` to `/app/` and injects the helper into `/ui/`.
pub async fn inject_auth_helper(req: Request, next: Next) -> Response {
    if req.method() == Method::GET && req.uri().path() == "/" {
        return Redirect::to("/app/").into_response();
    }
    let is_ui_page = req.method() == Method::GET
        && (req.uri().path() == "/ui" || req.uri().path().starts_with("/ui/"));
    let response = next.run(req).await;
    let is_html = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.starts_with("text/html"));
    if !is_ui_page || !is_html || !response.status().is_success() {
        return response;
    }
    let (mut parts, body) = response.into_parts();
    let bytes = match to_bytes(body, 8 * 1024 * 1024).await {
        Ok(bytes) => bytes,
        Err(e) => {
            tracing::warn!("Could not read the web UI page to inject the auth helper: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let html = inject_auth_helper_tag(&String::from_utf8_lossy(&bytes));
    parts.headers.remove(header::ETAG);
    parts.headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&html.len().to_string()).expect("ascii length"),
    );
    Response::from_parts(parts, Body::from(html))
}

/// Serves the custom AdSpace Agent studio HTML (`/app` and `/app/`).
async fn studio_index(headers: HeaderMap) -> Response {
    static_asset_response(&headers, "text/html; charset=utf-8", STUDIO_HTML)
}

/// Serves the custom AdSpace Agent studio stylesheet (`/app/app.css`).
async fn studio_css(headers: HeaderMap) -> Response {
    static_asset_response(&headers, "text/css; charset=utf-8", STUDIO_CSS)
}

/// Serves the shared `adk-ui` renderer stylesheet (`/app/adk-ui.css`).
async fn adk_ui_css(headers: HeaderMap) -> Response {
    static_asset_response(&headers, "text/css; charset=utf-8", ADK_UI_CSS)
}

/// Serves one of the studio's ES modules (`/app/{*path}`), e.g. `/app/app.js`
/// or `/app/stage.js`. Paths outside the embedded module list are 404.
async fn app_js(Path(path): Path<String>, headers: HeaderMap) -> Response {
    match app_module(&path) {
        Some(src) => static_asset_response(&headers, "application/javascript; charset=utf-8", src),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Path of the liveness check: [`healthcheck`].
pub const HEALTHCHECK_PATH: &str = "/healthcheck";

/// Answers `200 ok` when the server is responding, and looks at nothing
/// else. It is the target for a liveness probe, which restarts the instance
/// when it fails.
///
/// adk-server's `/api/health` is the wrong target for that: it asks each
/// backing service whether it is reachable (for a GCS artifact store, a
/// request to Cloud Storage on every check), so a slow or unreachable
/// dependency, or a missing permission on it, would have healthy instances
/// restarted.
async fn healthcheck() -> &'static str {
    "ok"
}

/// Serves the robot SVG favicon (`/favicon.svg` and `/favicon.ico`).
async fn studio_favicon() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("image/svg+xml; charset=utf-8"),
            ),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=86400"),
            ),
        ],
        STUDIO_FAVICON_SVG,
    )
        .into_response()
}

/// Starts a Google Sign-In flow (`openid email profile`) and redirects to Google.
///
/// The nonce embedded in the signed `state` is also set as a short-lived
/// cookie so only this browser can complete the sign-in on the callback.
async fn auth_login(State(routes): State<OAuthRoutes>) -> Response {
    match routes.credentials.begin_user_login() {
        Ok(login) => {
            let cookie = login_nonce_cookie(&routes.credentials, &login.nonce);
            ([(header::SET_COOKIE, cookie)], Redirect::to(&login.url)).into_response()
        }
        Err(e) => {
            let safe_err = escape_html(&e.to_string());
            html_page(
                "Sign-In Unavailable",
                &format!("<p>{safe_err}</p><p><a href=\"/app/\">Return to AdSpace Agent</a></p>"),
                "error",
            )
            .into_response()
        }
    }
}

/// `; Secure` when the public URL is HTTPS (cookies then never travel in clear).
fn cookie_secure_attr(credentials: &SessionCredentialManager) -> &'static str {
    if credentials.flow().config().is_https() {
        "; Secure"
    } else {
        ""
    }
}

/// `Set-Cookie` value for the signed user cookie (`max_age` 0 clears it).
fn user_cookie(credentials: &SessionCredentialManager, value: &str, max_age: u64) -> HeaderValue {
    let secure = cookie_secure_attr(credentials);
    HeaderValue::from_str(&format!(
        "{USER_COOKIE_NAME}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .expect("valid cookie header")
}

/// `Set-Cookie` value for the sign-in nonce (`nonce` empty clears it).
fn login_nonce_cookie(credentials: &SessionCredentialManager, nonce: &str) -> HeaderValue {
    let secure = cookie_secure_attr(credentials);
    let max_age = if nonce.is_empty() {
        0
    } else {
        LOGIN_NONCE_MAX_AGE_SECS
    };
    HeaderValue::from_str(&format!(
        "{LOGIN_NONCE_COOKIE_NAME}={nonce}; Path={OAUTH_CALLBACK_PATH}; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .expect("valid cookie header")
}

fn percent_decode_segment(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(hex) = std::str::from_utf8(&bytes[i + 1..i + 3])
            && let Ok(val) = u8::from_str_radix(hex, 16)
        {
            out.push(val);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The user id named in the path of a user-scoped adk-server route
/// (`/api/apps/{app}/users/{user}/…`, `/api/sessions/{app}/{user}/…`,
/// `/api/run/{app}/{user}/{session}`, `/api/debug/graph/{app}/{user}/…`).
fn extract_api_path_user_id(path: &str) -> Option<String> {
    let raw_user = if let Some(rest) = path.strip_prefix("/api/apps/") {
        let mut segments = rest.split('/');
        let _app = segments.next()?;
        if segments.next()? != "users" {
            return None;
        }
        segments.next()?
    } else {
        let rest = path
            .strip_prefix("/api/sessions/")
            .or_else(|| path.strip_prefix("/api/run/"))
            .or_else(|| path.strip_prefix("/api/debug/graph/"))?;
        let mut segments = rest.split('/');
        let _app = segments.next()?;
        segments.next()?
    };
    if raw_user.is_empty() {
        return None;
    }
    Some(percent_decode_segment(raw_user))
}

/// Whether an `/api/…` response can carry session events / state (and hence
/// the OAuth token stored under [`USER_CREDENTIALS_STATE_KEY`] /
/// [`CREDENTIALS_STATE_KEY`]).
fn carries_session_state(path: &str) -> bool {
    if path.contains("/artifacts") {
        return false;
    }
    (path.starts_with("/api/apps/") && path.contains("/sessions"))
        || path.starts_with("/api/sessions")
        || path == "/api/run"
        || path == "/api/run_sse"
        || path.starts_with("/api/run/")
        || path.starts_with("/api/debug/")
        || path == "/api/reasoning_engine"
        || path == "/api/stream_reasoning_engine"
}

/// State keys whose values are OAuth tokens and must never reach a browser.
const CREDENTIAL_KEYS: [&str; 2] = [USER_CREDENTIALS_STATE_KEY, CREDENTIALS_STATE_KEY];

fn scrub_credentials_from_json(val: &mut serde_json::Value) {
    let extra_keys = crate::auth::gemini_enterprise_auth_keys();
    scrub_credentials_from_json_with(val, &extra_keys);
}

fn scrub_credentials_from_json_with(val: &mut serde_json::Value, extra_keys: &[String]) {
    match val {
        serde_json::Value::Object(map) => {
            for key in CREDENTIAL_KEYS {
                map.remove(key);
            }
            for key in extra_keys {
                map.remove(key);
            }
            map.retain(|k, _| !k.starts_with("temp:") && !k.contains("/authorizations/"));
            for v in map.values_mut() {
                scrub_credentials_from_json_with(v, extra_keys);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                scrub_credentials_from_json_with(item, extra_keys);
            }
        }
        _ => {}
    }
}

/// Fast SIMD pre-check before JSON credential scrubbing: returns `false` when
/// `bytes` contains neither `CREDENTIALS_CACHE_KEY` (a substring of every key
/// in [`CREDENTIAL_KEYS`]), any configured Gemini Enterprise auth key, nor a
/// JSON `\u` escape sequence that could spell it.
#[inline]
fn bytes_may_contain_credentials(bytes: &[u8]) -> bool {
    memchr::memmem::find(bytes, CREDENTIALS_STATE_KEY.as_bytes()).is_some()
        || memchr::memmem::find(bytes, b"temp:").is_some()
        || memchr::memmem::find(bytes, b"/authorizations/").is_some()
        || memchr::memmem::find(bytes, b"\\u").is_some()
        || memchr::memmem::find(bytes, b"\\U").is_some()
        || crate::auth::gemini_enterprise_auth_keys()
            .iter()
            .any(|k| memchr::memmem::find(bytes, k.as_bytes()).is_some())
}

/// Removes [`CREDENTIALS_STATE_KEY`] from the `data:` payload of one complete
/// SSE event (text up to and including its terminating blank line).
///
/// Non-`data:` lines (`event:`, `id:`, `retry:`, comments) are kept; the data
/// lines are joined as the SSE spec prescribes, parsed as JSON, scrubbed and
/// re-emitted as a single `data:` line. Data that is not JSON passes through.
fn scrub_sse_event(frame: &[u8]) -> Vec<u8> {
    if !bytes_may_contain_credentials(frame)
        && frame.ends_with(b"\n\n")
        && !frame.ends_with(b"\n\n\n")
        && memchr::memmem::find(frame, b"\r").is_none()
    {
        return frame.to_vec();
    }
    let text = String::from_utf8_lossy(frame);
    let body = text.trim_end_matches('\n');
    let mut out = String::with_capacity(text.len());
    let mut data_lines: Vec<&str> = Vec::new();
    for line in body.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        match line.strip_prefix("data:") {
            Some(data) => data_lines.push(data.strip_prefix(' ').unwrap_or(data)),
            None => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    if !data_lines.is_empty() {
        let data = data_lines.join("\n");
        match serde_json::from_str::<serde_json::Value>(&data) {
            Ok(mut json) => {
                scrub_credentials_from_json(&mut json);
                out.push_str("data: ");
                out.push_str(&serde_json::to_string(&json).unwrap_or_default());
                out.push('\n');
            }
            Err(_) => {
                for line in data_lines {
                    out.push_str("data: ");
                    out.push_str(line);
                    out.push('\n');
                }
            }
        }
    }
    out.push('\n');
    out.into_bytes()
}

/// Incremental SSE scrubber: buffers bytes until an event is complete
/// (terminated by a blank line), scrubs it and emits it.
#[derive(Default)]
struct SseScrubber {
    buf: Vec<u8>,
}

impl SseScrubber {
    /// Feeds a chunk; returns the scrubbed bytes of every event completed by it.
    fn push(&mut self, chunk: &[u8]) -> Result<Bytes, axum::Error> {
        // Bytes already buffered were scanned before, except the last one,
        // which may pair with the first byte of this chunk into `\n\n`.
        let mut search_from = self.buf.len().saturating_sub(1);
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(at) = memchr::memmem::find(&self.buf[search_from..], b"\n\n") {
            let end = search_from + at + 2;
            out.extend(scrub_sse_event(&self.buf[..end]));
            self.buf.drain(..end);
            search_from = 0;
        }
        if self.buf.len() > MAX_SCRUBBED_SSE_EVENT_BYTES {
            self.buf.clear();
            return Err(axum::Error::new(
                "SSE event too large to scrub credentials from; stream aborted",
            ));
        }
        Ok(Bytes::from(out))
    }

    /// Flushes a trailing event that was not terminated by a blank line.
    fn finish(&mut self) -> Bytes {
        if self.buf.is_empty() {
            Bytes::new()
        } else {
            Bytes::from(scrub_sse_event(&std::mem::take(&mut self.buf)))
        }
    }
}

/// Re-streams a `text/event-stream` response with credentials removed from
/// every event (`/api/run_sse`, `/api/run/{app}/{user}/{session}`).
fn scrub_sse_response(response: Response) -> Response {
    let (mut parts, body) = response.into_parts();
    parts.headers.remove(header::CONTENT_LENGTH);
    let upstream = body.into_data_stream();
    let scrubbed = futures::stream::unfold(
        (upstream, SseScrubber::default(), false),
        |(mut upstream, mut scrubber, done)| async move {
            if done {
                return None;
            }
            match upstream.next().await {
                Some(Ok(chunk)) => {
                    let out = scrubber.push(&chunk);
                    let failed = out.is_err();
                    Some((out, (upstream, scrubber, failed)))
                }
                Some(Err(e)) => Some((Err(e), (upstream, scrubber, true))),
                None => {
                    let tail = scrubber.finish();
                    if tail.is_empty() {
                        None
                    } else {
                        Some((Ok(tail), (upstream, scrubber, true)))
                    }
                }
            }
        },
    )
    .fuse();
    Response::from_parts(parts, Body::from_stream(scrubbed))
}

/// Scrubs credentials from one newline-delimited JSON line (`/api/stream_reasoning_engine`).
fn scrub_ndjson_line(line: &[u8]) -> Vec<u8> {
    if !bytes_may_contain_credentials(line) {
        return line.to_vec();
    }
    let trimmed = line.strip_suffix(b"\n").unwrap_or(line);
    if let Ok(mut json_val) = serde_json::from_slice::<serde_json::Value>(trimmed) {
        scrub_credentials_from_json(&mut json_val);
        if let Ok(mut cleaned) = serde_json::to_vec(&json_val) {
            cleaned.push(b'\n');
            return cleaned;
        }
    }
    line.to_vec()
}

/// Incremental NDJSON scrubber for `/api/stream_reasoning_engine`.
#[derive(Default)]
struct NdjsonScrubber {
    buf: Vec<u8>,
}

impl NdjsonScrubber {
    fn push(&mut self, chunk: &[u8]) -> Result<Bytes, axum::Error> {
        let mut search_from = self.buf.len();
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(at) = memchr::memchr(b'\n', &self.buf[search_from..]) {
            let end = search_from + at + 1;
            out.extend(scrub_ndjson_line(&self.buf[..end]));
            self.buf.drain(..end);
            search_from = 0;
        }
        if self.buf.len() > MAX_SCRUBBED_SSE_EVENT_BYTES {
            self.buf.clear();
            return Err(axum::Error::new(
                "NDJSON event line too large to scrub credentials from; stream aborted",
            ));
        }
        Ok(Bytes::from(out))
    }

    fn finish(&mut self) -> Bytes {
        if self.buf.is_empty() {
            Bytes::new()
        } else {
            Bytes::from(scrub_ndjson_line(&std::mem::take(&mut self.buf)))
        }
    }
}

/// Re-streams an NDJSON response (`/api/stream_reasoning_engine`) with
/// credentials removed from every line without buffering the whole stream.
fn scrub_ndjson_response(response: Response) -> Response {
    let (mut parts, body) = response.into_parts();
    parts.headers.remove(header::CONTENT_LENGTH);
    let upstream = body.into_data_stream();
    let scrubbed = futures::stream::unfold(
        (upstream, NdjsonScrubber::default(), false),
        |(mut upstream, mut scrubber, done)| async move {
            if done {
                return None;
            }
            match upstream.next().await {
                Some(Ok(chunk)) => {
                    let out = scrubber.push(&chunk);
                    let failed = out.is_err();
                    Some((out, (upstream, scrubber, failed)))
                }
                Some(Err(e)) => Some((Err(e), (upstream, scrubber, true))),
                None => {
                    let tail = scrubber.finish();
                    if tail.is_empty() {
                        None
                    } else {
                        Some((Ok(tail), (upstream, scrubber, true)))
                    }
                }
            }
        },
    )
    .fuse();
    Response::from_parts(parts, Body::from_stream(scrubbed))
}

/// Buffers an `application/json` response and removes credentials from it.
/// Bodies above [`MAX_SCRUBBED_JSON_BYTES`] are refused with `413` rather
/// than served unscrubbed.
async fn scrub_json_response(response: Response) -> Response {
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = to_bytes(body, MAX_SCRUBBED_JSON_BYTES).await else {
        return (
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(serde_json::json!({
                "error": format!(
                    "The session is too large to serve safely (over {} MB of JSON); start a new session or delete this one.",
                    MAX_SCRUBBED_JSON_BYTES / (1024 * 1024)
                )
            })),
        )
            .into_response();
    };
    if !bytes_may_contain_credentials(&bytes) {
        return Response::from_parts(parts, Body::from(bytes));
    }
    if let Ok(mut json_val) = serde_json::from_slice::<serde_json::Value>(&bytes) {
        scrub_credentials_from_json(&mut json_val);
        if let Ok(cleaned) = serde_json::to_vec(&json_val) {
            parts.headers.remove(header::CONTENT_LENGTH);
            return Response::from_parts(parts, Body::from(cleaned));
        }
    }
    Response::from_parts(parts, Body::from(bytes))
}

/// Middleware in front of the adk-server routes:
///
/// 1. hides the stock `/ui/` dev UI once Google Sign-In is configured (it has
///    no sign-in; `/app/` is the user interface then);
/// 2. requires the identity named in a user-scoped `/api/…` path to match the
///    caller (defence in depth — [`CookieIdentityExtractor`] makes adk-server
///    enforce the same thing on every route, including body-addressed ones);
/// 3. strips raw OAuth token secrets ([`CREDENTIALS_STATE_KEY`]) from JSON and
///    SSE responses that carry session events or state.
pub async fn guard_adk_routes(
    State((credentials, admins)): State<(Arc<SessionCredentialManager>, AdminAllowList)>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    let oauth_configured = credentials.flow().config().is_configured();
    if oauth_configured && (path == "/ui" || path.starts_with("/ui/")) {
        // The stock ADK dev UI knows nothing about the signed-in Google user,
        // so once Sign-In is configured it is reserved for administrators
        // (`ADMIN_EMAILS`, the same list that may switch the server models);
        // everyone else is sent to `/app/`.
        let is_admin = extract_cookie_user(req.headers(), &credentials)
            .is_some_and(|user| admins.allows(&user.email));
        if !is_admin {
            return if path == "/ui" || path == "/ui/" {
                Redirect::to("/app/").into_response()
            } else {
                StatusCode::NOT_FOUND.into_response()
            };
        }
    }
    if let Some(target_user_id) = extract_api_path_user_id(&path) {
        match request_user_id(req.headers(), &credentials) {
            Some(caller) if caller == target_user_id => {}
            Some(_) => return StatusCode::FORBIDDEN.into_response(),
            None => return StatusCode::UNAUTHORIZED.into_response(),
        }
    }

    let scrub = carries_session_state(&path);
    let response = next.run(req).await;
    if !scrub || !response.status().is_success() {
        return response;
    }
    let content_type = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if path == "/api/stream_reasoning_engine" && content_type.starts_with("application/json") {
        scrub_ndjson_response(response)
    } else if content_type.starts_with("application/json") {
        scrub_json_response(response).await
    } else if content_type.starts_with("text/event-stream") {
        scrub_sse_response(response)
    } else {
        response
    }
}

/// Value of the cookie `name`, if present.
fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    let cookie_hdr = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_hdr.split(';').find_map(|part| {
        part.trim()
            .strip_prefix(name)
            .and_then(|rest| rest.strip_prefix('='))
            .map(str::to_string)
    })
}

fn extract_cookie_user(
    headers: &HeaderMap,
    credentials: &SessionCredentialManager,
) -> Option<AuthenticatedUser> {
    let cookie_hdr = headers.get(header::COOKIE)?.to_str().ok()?;
    for part in cookie_hdr.split(';') {
        let trimmed = part.trim();
        if let Some(val) = trimmed.strip_prefix(&format!("{USER_COOKIE_NAME}="))
            && let Some(user) = credentials.verify_user_cookie(val)
        {
            return Some(user);
        }
    }
    None
}

/// The user a request acts as: the signed-in Google user, or — only while
/// Google Sign-In is not configured — the anonymous [`DEFAULT_USER_ID`].
fn request_user_id(headers: &HeaderMap, credentials: &SessionCredentialManager) -> Option<String> {
    if let Some(user) = extract_cookie_user(headers, credentials) {
        return Some(user.id);
    }
    (!credentials.flow().config().is_configured()).then(|| DEFAULT_USER_ID.to_string())
}

/// adk-server [`RequestContextExtractor`] backed by the signed `adspace_user`
/// cookie.
///
/// With it installed, adk-server's own auth layer resolves the caller on
/// *every* session / runtime / artifact / debug route — including the ones
/// that take the user id from the JSON body (`POST /api/run`, `/api/run_sse`,
/// `/api/sessions`) — rejects anonymous calls with `401` when Sign-In is
/// configured, and answers `403` when a path names another user.
pub struct CookieIdentityExtractor {
    credentials: Arc<SessionCredentialManager>,
}

impl CookieIdentityExtractor {
    /// Creates the extractor for `credentials`.
    pub fn new(credentials: Arc<SessionCredentialManager>) -> Self {
        Self { credentials }
    }
}

#[async_trait]
impl RequestContextExtractor for CookieIdentityExtractor {
    async fn extract(&self, parts: &Parts) -> Result<RequestContext, RequestContextError> {
        match extract_cookie_user(&parts.headers, &self.credentials) {
            Some(user) => Ok(RequestContext {
                user_id: user.id,
                scopes: Vec::new(),
                metadata: HashMap::from([
                    ("email".to_string(), user.email),
                    ("name".to_string(), user.name),
                ]),
            }),
            None if self.credentials.flow().config().is_configured() => {
                Err(RequestContextError::MissingAuth)
            }
            None => Ok(RequestContext {
                user_id: DEFAULT_USER_ID.to_string(),
                scopes: Vec::new(),
                metadata: HashMap::new(),
            }),
        }
    }
}

/// Whether the caller may change the server-wide models; the `Err` is the
/// response to send otherwise.
fn authorize_model_update(routes: &OAuthRoutes, headers: &HeaderMap) -> Result<(), Box<Response>> {
    let deny = |status: StatusCode, error: &str| {
        Err(Box::new(
            (status, Json(serde_json::json!({ "error": error }))).into_response(),
        ))
    };
    let oauth_configured = routes.credentials.flow().config().is_configured();
    let user = extract_cookie_user(headers, &routes.credentials);
    if oauth_configured && user.is_none() {
        return deny(StatusCode::UNAUTHORIZED, "Authentication required");
    }
    if routes.admins.is_empty() {
        return if oauth_configured {
            deny(
                StatusCode::FORBIDDEN,
                "Model switching is disabled: set ADMIN_EMAILS to the administrators allowed to change the server-wide models",
            )
        } else {
            Ok(())
        };
    }
    match user {
        Some(user) if routes.admins.allows(&user.email) => Ok(()),
        _ => deny(
            StatusCode::FORBIDDEN,
            "Only administrators (ADMIN_EMAILS) may change the server-wide models",
        ),
    }
}

fn models_payload(routes: &OAuthRoutes) -> serde_json::Value {
    let active = routes.credentials.model_controller().active_models();
    serde_json::json!({
        "model": active.model,
        "video_model": active.video_model,
        "image_model": active.image_model,
        "presets": {
            "models": PRESET_MODELS,
            "video_models": PRESET_VIDEO_MODELS,
            "image_models": PRESET_IMAGE_MODELS,
        },
        "pricing": crate::config::model_pricing_table([active.model.as_str()]),
    })
}

/// Returns the currently signed-in Google user for `/app/` (`is_admin` tells
/// whether they may switch the server-wide models).
async fn auth_me(State(routes): State<OAuthRoutes>, headers: HeaderMap) -> Json<serde_json::Value> {
    let oauth_configured = routes.credentials.flow().config().is_configured();
    let models = models_payload(&routes);
    let is_admin = authorize_model_update(&routes, &headers).is_ok();
    match extract_cookie_user(&headers, &routes.credentials) {
        Some(user) => Json(serde_json::json!({
            "authenticated": true,
            "oauth_configured": oauth_configured,
            "user": user,
            "is_admin": is_admin,
            "models": models,
        })),
        None => Json(serde_json::json!({
            "authenticated": false,
            "oauth_configured": oauth_configured,
            "is_admin": is_admin,
            "models": models,
        })),
    }
}

/// Request payload for `POST /auth/models`.
#[derive(Debug, Default, serde::Deserialize)]
pub struct UpdateModelsRequest {
    /// Optional new ADK LLM model identifier.
    pub model: Option<String>,
    /// Optional new Veo video model identifier.
    pub video_model: Option<String>,
    /// Optional new image generation model identifier.
    pub image_model: Option<String>,
}

/// Returns the active LLM, video, and image models alongside their preset lists.
async fn get_models(State(routes): State<OAuthRoutes>) -> Json<serde_json::Value> {
    Json(models_payload(&routes))
}

/// Updates the active LLM, video, and/or image models at runtime.
///
/// The change is server-wide, so it is limited to administrators (see
/// [`AdminAllowList`]).
async fn update_models(
    State(routes): State<OAuthRoutes>,
    headers: HeaderMap,
    Json(body): Json<UpdateModelsRequest>,
) -> Response {
    if let Err(denied) = authorize_model_update(&routes, &headers) {
        return *denied;
    }

    match routes.credentials.model_controller().update_models(
        body.model.as_deref(),
        body.video_model.as_deref(),
        body.image_model.as_deref(),
    ) {
        Ok(_) => Json(models_payload(&routes)).into_response(),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// Logs out the current user by clearing the `adspace_user` cookie.
async fn auth_logout(State(routes): State<OAuthRoutes>) -> Response {
    (
        [(header::SET_COOKIE, user_cookie(&routes.credentials, "", 0))],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

/// Handles the Google OAuth redirect: exchanges the code and either signs in
/// the user (when `session.app_name == LOGIN_SENTINEL_APP`) or stores the
/// tokens in the chat session that started the authorization.
pub async fn oauth_callback(
    State(routes): State<OAuthRoutes>,
    headers: HeaderMap,
    Query(query): Query<OAuthCallbackQuery>,
) -> Response {
    if let Some(error) = query.error {
        if let Some(state) = query.state.as_deref() {
            routes.credentials.fail_authorization(state, &error);
        }
        let safe_error = escape_html(&error);
        return html_page(
            "Authorization failed",
            &format!(
                "<p>Google reported: <code>{safe_error}</code>. Return to the chat and try again.</p>"
            ),
            "error",
        )
        .into_response();
    }
    let (Some(code), Some(state)) = (query.code, query.state) else {
        return html_page(
            "Authorization failed",
            "<p>Missing <code>code</code> or <code>state</code> parameter.</p>",
            "error",
        )
        .into_response();
    };
    // A sign-in may only be completed by the browser that started it.
    if let Some(expected_nonce) = routes.credentials.login_state_nonce(&state)
        && cookie_value(&headers, LOGIN_NONCE_COOKIE_NAME).as_deref() != Some(&*expected_nonce)
    {
        return html_page(
            "Sign-in failed",
            "<p>This sign-in was not started in this browser (or it took too long). <a href=\"/auth/login\">Sign in again</a>.</p>",
            "error",
        )
        .into_response();
    }
    let (session, token) = match routes
        .credentials
        .complete_authorization(&state, &code)
        .await
    {
        Ok(result) => result,
        Err(e) => {
            let safe_err = escape_html(&e.to_string());
            return html_page(
                "Authorization failed",
                &format!(
                    "<p>{safe_err}</p><p>Return to the chat and ask the agent to start the authorization again.</p>"
                ),
                "error",
            )
            .into_response();
        }
    };

    if session.app_name == LOGIN_SENTINEL_APP {
        match routes
            .credentials
            .flow()
            .fetch_user_info(&token.access_token)
            .await
        {
            Ok(user) => {
                let signed = routes.credentials.sign_user_cookie(&user);
                let safe_name = escape_html(&user.name);
                let safe_email = escape_html(&user.email);
                let page = html_page(
                    "Signed in to AdSpace Agent",
                    &format!(
                        "<p>Signed in as <strong>{safe_name}</strong> (<code>{safe_email}</code>). Returning to AdSpace Agent…</p>",
                    ),
                    "login_ok",
                );
                return (
                    AppendHeaders([
                        (
                            header::SET_COOKIE,
                            user_cookie(&routes.credentials, &signed, USER_COOKIE_MAX_AGE_SECS),
                        ),
                        (
                            header::SET_COOKIE,
                            login_nonce_cookie(&routes.credentials, ""),
                        ),
                    ]),
                    page,
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!("Failed to fetch Google userinfo during login: {e:#}");
                let safe_err = escape_html(&e.to_string());
                return html_page(
                    "Sign-in failed",
                    &format!(
                        "<p>Could not retrieve your Google profile: {safe_err}</p><p><a href=\"/app/\">Try again</a></p>"
                    ),
                    "error",
                )
                .into_response();
            }
        }
    }

    let had_in_flight_waiter = routes.credentials.has_pending_waiter(&session);
    match routes
        .credentials
        .persist(routes.sessions.as_ref(), &session, &token)
        .await
    {
        Ok(()) => {
            if !had_in_flight_waiter && let Some(runner) = &routes.resume {
                spawn_resume(runner.clone(), session, &token);
            }
            if had_in_flight_waiter || routes.resume.is_some() {
                html_page(
                    "Authorization complete",
                    "<p>Google access was granted to this chat session and the agent is continuing where it left off. This window closes by itself; otherwise you can close it and return to the chat.</p>",
                    "ok",
                )
                .into_response()
            } else {
                html_page(
                    "Authorization complete",
                    "<p>Google access was granted to this chat session. You can close this window, return to the chat and tell the agent to continue.</p>",
                    "ok",
                )
                .into_response()
            }
        }
        Err(e) => {
            tracing::error!("{e:#}");
            html_page(
                "Authorization failed",
                "<p>The chat session that requested access could not be found. Return to the chat and ask the agent to start the authorization again.</p>",
                "error",
            )
            .into_response()
        }
    }
}

/// Builds the router for the OAuth endpoints and custom AdSpace Agent studio (`/app/`).
pub fn oauth_router(routes: OAuthRoutes) -> Router {
    Router::new()
        .route(OAUTH_CALLBACK_PATH, get(oauth_callback))
        .route(AUTH_HELPER_PATH, get(auth_helper))
        .route(AUTH_EVENTS_PATH, get(oauth_events))
        .route(AUTH_ACK_PATH, post(oauth_ack))
        .route("/auth/login", get(auth_login))
        .route("/auth/me", get(auth_me))
        .route("/auth/logout", post(auth_logout))
        .route("/auth/models", get(get_models).post(update_models))
        .route("/app", get(|| async { Redirect::to("/app/") }))
        .route("/app/", get(studio_index))
        .route("/app/app.css", get(studio_css))
        .route("/app/adk-ui.css", get(adk_ui_css))
        .route("/app/{*path}", get(app_js))
        .route("/favicon.ico", get(studio_favicon))
        .route("/favicon.svg", get(studio_favicon))
        .route(HEALTHCHECK_PATH, get(healthcheck))
        .route("/health", get(healthcheck))
        .with_state(routes)
}

impl AgentRuntime {
    fn oauth_routes(&self, resume: bool) -> OAuthRoutes {
        OAuthRoutes {
            credentials: self.credentials.clone(),
            sessions: self.session_service.clone(),
            resume: resume.then(|| Arc::new(self.turn_runner())),
            admins: AdminAllowList::from_env(),
        }
    }

    fn log_redirect_uri(&self) {
        let redirect = &self.credentials.flow().config().redirect_uri;
        tracing::info!(
            "OAuth redirect URI: {redirect} — register it on the OAuth client in Google Cloud Console"
        );
    }
}

/// User id of the runner-level memory binding and of the console session
/// (the same value adk-cli's console uses).
///
/// Per-user memory recall does not depend on this: `PreloadMemoryTool` queries
/// the memory service with the invocation's own `user_id`, and
/// `MemorySavingAgent` writes under it too. The runner-level `Memory` only
/// backs `ToolContext::search_memory()`, which no tool of ours uses.
///
/// Over HTTP this user only exists while Google Sign-In is **not**
/// configured; otherwise every `/api/…` call must carry the `adspace_user`
/// cookie (see [`CookieIdentityExtractor`]).
const DEFAULT_USER_ID: &str = "user";

/// adk-cli launcher configured with the runtime's services.
fn launcher(runtime: &AgentRuntime) -> Launcher {
    let mut l = Launcher::new(runtime.agent.clone())
        .app_name(APP_NAME)
        .with_session_service(runtime.session_service.clone())
        .with_artifact_service(runtime.artifact_service.clone())
        .with_memory_service(runtime.memory_for(DEFAULT_USER_ID))
        .with_compaction(runtime.compaction_config());
    if let Some(cache_cfg) = runtime.context_cache_config_opt() {
        l = l.with_context_cache(cache_cfg, runtime.models.clone() as Arc<dyn CacheCapable>);
    }
    if crate::logging::format() == crate::logging::LogFormat::Json {
        // The JSON logger is already installed. Left to itself the launcher
        // would install adk-rust's console logger and span exporter over it,
        // which panics once a global logger exists.
        l = l.with_telemetry(adk_cli::launcher::TelemetryConfig::None);
    }
    l
}

/// What the compression layer compresses: tower-http's default (which already
/// skips images, gRPC, event streams and tiny bodies) minus the artifact
/// downloads that are compressed already, where gzip only costs CPU and drops
/// `Content-Length`.
fn compressible() -> impl tower_http::compression::predicate::Predicate {
    use tower_http::compression::predicate::{DefaultPredicate, NotForContentType, Predicate as _};
    DefaultPredicate::new()
        .and(NotForContentType::new("video/"))
        .and(NotForContentType::new("audio/"))
        .and(NotForContentType::new("application/pdf"))
        .and(NotForContentType::new("application/zip"))
        .and(NotForContentType::new("application/gzip"))
        .and(NotForContentType::new("application/octet-stream"))
}

/// Whether Agent Engine BYOC dispatch routes (`/api/reasoning_engine` and
/// `/api/stream_reasoning_engine`) should be enabled.
///
/// True when running inside a Vertex AI Agent Engine container (which sets
/// `GOOGLE_CLOUD_AGENT_ENGINE_ID`) or when `AGENT_ENGINE=true` is set.
pub fn is_agent_engine_enabled() -> bool {
    crate::config::env_opt("GOOGLE_CLOUD_AGENT_ENGINE_ID").is_some()
        || crate::config::env_bool_or("AGENT_ENGINE", false)
}

/// Copies any `"temp:<key>"` entries in a state map into `"<key>"` so
/// `adk-session` does not drop Gemini Enterprise's delegated OAuth tokens when
/// persisting `create_session` or `append_event`.
fn copy_temp_state_keys(map: &mut serde_json::Map<String, serde_json::Value>) -> bool {
    let mut additions = Vec::new();
    for (k, v) in map.iter() {
        if let Some(rest) = k.strip_prefix("temp:")
            && !rest.is_empty()
        {
            if !map.contains_key(rest) {
                additions.push((rest.to_string(), v.clone()));
            }
            if let Some(short) = rest
                .rsplit('/')
                .next()
                .filter(|s| !s.is_empty() && *s != rest)
            {
                if !map.contains_key(short) {
                    additions.push((short.to_string(), v.clone()));
                }
                let temp_short = format!("temp:{short}");
                if !map.contains_key(&temp_short) {
                    additions.push((temp_short, v.clone()));
                }
            }
        }
    }
    let changed = !additions.is_empty();
    for (k, v) in additions {
        map.insert(k, v);
    }
    changed
}

/// Decodes a flexible base64 string (standard or URL-safe, with or without padding).
fn decode_flexible_base64(raw: &str) -> Option<Vec<u8>> {
    let trimmed = raw.trim();
    base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(trimmed))
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(trimmed))
        .or_else(|_| base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(trimmed))
        .ok()
}

/// Normalizes a single `google.genai.types.Part` JSON value into `adk_core::Part` JSON.
fn genai_part_to_adk_json(part: &mut serde_json::Value) {
    let Some(obj) = part.as_object_mut() else {
        return;
    };
    if obj.get("thought").and_then(serde_json::Value::as_bool) == Some(true)
        && let Some(text) = obj.get("text").and_then(serde_json::Value::as_str)
    {
        let mut out = serde_json::Map::new();
        out.insert(
            "thinking".to_string(),
            serde_json::Value::String(text.to_string()),
        );
        if let Some(sig) = obj
            .get("thought_signature")
            .or_else(|| obj.get("thoughtSignature"))
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            out.insert(
                "signature".to_string(),
                serde_json::Value::String(sig.to_string()),
            );
        }
        *obj = out;
        return;
    }
    if let Some(inline) = obj
        .get("inline_data")
        .or_else(|| obj.get("inlineData"))
        .and_then(serde_json::Value::as_object)
        .cloned()
    {
        let mime_type = inline
            .get("mime_type")
            .or_else(|| inline.get("mimeType"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("application/octet-stream")
            .to_string();
        let bytes = match inline.get("data") {
            Some(serde_json::Value::String(b64)) => decode_flexible_base64(b64).unwrap_or_default(),
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .filter_map(|v| v.as_u64().and_then(|n| u8::try_from(n).ok()))
                .collect(),
            _ => Vec::new(),
        };
        let mut out = serde_json::Map::new();
        out.insert(
            "mime_type".to_string(),
            serde_json::Value::String(mime_type),
        );
        out.insert(
            "data".to_string(),
            serde_json::Value::Array(bytes.into_iter().map(serde_json::Value::from).collect()),
        );
        *obj = out;
        return;
    }
    if let Some(serde_json::Value::String(b64)) = obj.get("data").cloned()
        && (obj.contains_key("mime_type") || obj.contains_key("mimeType"))
    {
        if let Some(mt) = obj.remove("mimeType") {
            obj.entry("mime_type".to_string()).or_insert(mt);
        }
        let bytes = decode_flexible_base64(&b64).unwrap_or_default();
        obj.insert(
            "data".to_string(),
            serde_json::Value::Array(bytes.into_iter().map(serde_json::Value::from).collect()),
        );
        return;
    }
    if let Some(file) = obj
        .get("file_data")
        .or_else(|| obj.get("fileData"))
        .and_then(serde_json::Value::as_object)
        .cloned()
    {
        let mime_type = file
            .get("mime_type")
            .or_else(|| file.get("mimeType"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or("application/octet-stream")
            .to_string();
        let file_uri = file
            .get("file_uri")
            .or_else(|| file.get("fileUri"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let mut out = serde_json::Map::new();
        out.insert(
            "mime_type".to_string(),
            serde_json::Value::String(mime_type),
        );
        out.insert("file_uri".to_string(), serde_json::Value::String(file_uri));
        *obj = out;
        return;
    }
    if let Some(fc) = obj
        .get("function_call")
        .or_else(|| obj.get("functionCall"))
        .and_then(serde_json::Value::as_object)
        .cloned()
    {
        let name = fc
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let args = fc
            .get("args")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let mut out = serde_json::Map::new();
        out.insert("name".to_string(), serde_json::Value::String(name));
        out.insert("args".to_string(), args);
        if let Some(id) = fc
            .get("id")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            out.insert("id".to_string(), serde_json::Value::String(id.to_string()));
        }
        if let Some(sig) = obj
            .get("thought_signature")
            .or_else(|| obj.get("thoughtSignature"))
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            out.insert(
                "thought_signature".to_string(),
                serde_json::Value::String(sig.to_string()),
            );
        }
        *obj = out;
        return;
    }
    if let Some(fr) = obj
        .get("function_response")
        .or_else(|| obj.get("functionResponse"))
        .and_then(serde_json::Value::as_object)
        .cloned()
    {
        let name = fr
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_string();
        let response = fr
            .get("response")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let mut out = serde_json::Map::new();
        out.insert(
            "functionResponse".to_string(),
            serde_json::json!({
                "name": name,
                "response": response,
            }),
        );
        if let Some(id) = fr
            .get("id")
            .or_else(|| obj.get("id"))
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            out.insert("id".to_string(), serde_json::Value::String(id.to_string()));
        }
        *obj = out;
    }
}

/// Normalizes a `message` / `Content` JSON value into `adk_core::Content` JSON.
fn normalize_agent_engine_content(mut message: serde_json::Value) -> serde_json::Value {
    match message {
        serde_json::Value::String(text) => serde_json::json!({
            "role": "user",
            "parts": [{"text": text}],
        }),
        serde_json::Value::Object(ref mut map) => {
            let role = map
                .get("role")
                .and_then(serde_json::Value::as_str)
                .filter(|r| !r.trim().is_empty())
                .unwrap_or("user")
                .to_string();
            map.insert("role".to_string(), serde_json::Value::String(role));
            if let Some(parts) = map
                .get_mut("parts")
                .and_then(serde_json::Value::as_array_mut)
            {
                for part in parts.iter_mut() {
                    genai_part_to_adk_json(part);
                }
            } else if let Some(text) = map.remove("text") {
                map.insert("parts".to_string(), serde_json::json!([{"text": text}]));
            } else {
                map.insert("parts".to_string(), serde_json::json!([]));
            }
            message
        }
        _ => serde_json::json!({
            "role": "user",
            "parts": [{"text": message.to_string()}],
        }),
    }
}

/// Converts a serialized `adk_core::Part` JSON value into `google.genai.types.Part` JSON.
fn adk_part_to_genai_json(part: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = part.as_object() else {
        return serde_json::json!({ "text": part.to_string() });
    };
    if let Some(thinking) = obj.get("thinking").and_then(serde_json::Value::as_str) {
        let mut out = serde_json::Map::new();
        out.insert(
            "text".to_string(),
            serde_json::Value::String(thinking.to_string()),
        );
        out.insert("thought".to_string(), serde_json::Value::Bool(true));
        if let Some(sig) = obj
            .get("signature")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            out.insert(
                "thought_signature".to_string(),
                serde_json::Value::String(sig.to_string()),
            );
        }
        return serde_json::Value::Object(out);
    }
    if let Some(text) = obj.get("text").and_then(serde_json::Value::as_str) {
        let mut out = serde_json::Map::new();
        out.insert(
            "text".to_string(),
            serde_json::Value::String(text.to_string()),
        );
        if let Some(thought) = obj.get("thought").and_then(serde_json::Value::as_bool) {
            out.insert("thought".to_string(), serde_json::Value::Bool(thought));
        }
        if let Some(sig) = obj
            .get("thought_signature")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            out.insert(
                "thought_signature".to_string(),
                serde_json::Value::String(sig.to_string()),
            );
        }
        return serde_json::Value::Object(out);
    }
    if let Some(name) = obj.get("name").and_then(serde_json::Value::as_str)
        && let Some(args) = obj.get("args")
    {
        let mut fc = serde_json::Map::new();
        fc.insert(
            "name".to_string(),
            serde_json::Value::String(name.to_string()),
        );
        let args_obj = if args.is_object() {
            args.clone()
        } else {
            serde_json::json!({ "args": args })
        };
        fc.insert("args".to_string(), args_obj);
        if let Some(id) = obj
            .get("id")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            fc.insert("id".to_string(), serde_json::Value::String(id.to_string()));
        }
        let mut out = serde_json::Map::new();
        out.insert("function_call".to_string(), serde_json::Value::Object(fc));
        if let Some(sig) = obj
            .get("thought_signature")
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            out.insert(
                "thought_signature".to_string(),
                serde_json::Value::String(sig.to_string()),
            );
        }
        return serde_json::Value::Object(out);
    }
    if let Some(fr) = obj
        .get("functionResponse")
        .or_else(|| obj.get("function_response"))
        .and_then(serde_json::Value::as_object)
    {
        let mut fr_out = serde_json::Map::new();
        if let Some(name) = fr.get("name").and_then(serde_json::Value::as_str) {
            fr_out.insert(
                "name".to_string(),
                serde_json::Value::String(name.to_string()),
            );
        }
        let resp_val = fr
            .get("response")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        let resp_obj = if resp_val.is_object() {
            resp_val
        } else {
            serde_json::json!({ "result": resp_val })
        };
        fr_out.insert("response".to_string(), resp_obj);
        if let Some(id) = obj
            .get("id")
            .or_else(|| fr.get("id"))
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            fr_out.insert("id".to_string(), serde_json::Value::String(id.to_string()));
        }
        let mut out = serde_json::Map::new();
        out.insert(
            "function_response".to_string(),
            serde_json::Value::Object(fr_out),
        );
        return serde_json::Value::Object(out);
    }
    if let Some(mime_type) = obj.get("mime_type").and_then(serde_json::Value::as_str) {
        if let Some(data_val) = obj.get("data") {
            let b64 = match data_val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Array(arr) => {
                    let bytes: Vec<u8> = arr
                        .iter()
                        .filter_map(|v| v.as_u64().and_then(|n| u8::try_from(n).ok()))
                        .collect();
                    base64::engine::general_purpose::STANDARD.encode(&bytes)
                }
                _ => String::new(),
            };
            return serde_json::json!({
                "inline_data": {
                    "mime_type": mime_type,
                    "data": b64,
                }
            });
        }
        if let Some(file_uri) = obj.get("file_uri").and_then(serde_json::Value::as_str) {
            return serde_json::json!({
                "file_data": {
                    "mime_type": mime_type,
                    "file_uri": file_uri,
                }
            });
        }
    }
    if obj.contains_key("inline_data")
        || obj.contains_key("file_data")
        || obj.contains_key("function_call")
        || obj.contains_key("function_response")
        || obj.contains_key("executable_code")
        || obj.contains_key("code_execution_result")
    {
        return part.clone();
    }
    serde_json::json!({ "text": part.to_string() })
}

/// Converts a serialized `adk_core::Event` JSON object into the Python `google-adk`
/// `Event.model_dump_json(exclude_none=True)` schema expected by Vertex AI Agent Engine
/// and Gemini Enterprise.
fn event_to_python_adk_json(event: &mut serde_json::Value) {
    scrub_credentials_from_json(event);
    let Some(map) = event.as_object_mut() else {
        return;
    };
    if !map.contains_key("author")
        && !map.contains_key("invocation_id")
        && !map.contains_key("actions")
    {
        return;
    }

    let inv_id = map
        .get("invocation_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string();
    map.insert(
        "invocation_id".to_string(),
        serde_json::Value::String(inv_id),
    );

    if let Some(ts_str) = map
        .get("timestamp")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        && let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&ts_str)
        && let Some(num) = serde_json::Number::from_f64(dt.timestamp_micros() as f64 / 1_000_000.0)
    {
        map.insert("timestamp".to_string(), serde_json::Value::Number(num));
    }

    if let Some(content_val) = map.get_mut("content") {
        if let Some(content_obj) = content_val.as_object_mut() {
            let role = match content_obj
                .get("role")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("model")
            {
                "model" | "assistant" | "agent" => "model",
                _ => "user",
            };
            content_obj.insert(
                "role".to_string(),
                serde_json::Value::String(role.to_string()),
            );
            if let Some(parts) = content_obj
                .get_mut("parts")
                .and_then(serde_json::Value::as_array_mut)
            {
                let converted: Vec<serde_json::Value> =
                    parts.iter().map(adk_part_to_genai_json).collect();
                *parts = converted;
            }
        } else if content_val.is_null() {
            map.remove("content");
        }
    }

    if let Some(fr_val) = map.get("finish_reason").cloned() {
        match fr_val.as_str() {
            Some(s) => {
                let mapped = match s {
                    "Stop" | "STOP" => "STOP",
                    "MaxTokens" | "MAX_TOKENS" => "MAX_TOKENS",
                    "Safety" | "SAFETY" => "SAFETY",
                    "Recitation" | "RECITATION" => "RECITATION",
                    _ => "OTHER",
                };
                map.insert(
                    "finish_reason".to_string(),
                    serde_json::Value::String(mapped.to_string()),
                );
            }
            None => {
                map.remove("finish_reason");
            }
        }
    }

    if let Some(usage_val) = map.remove("usage_metadata")
        && let Some(u) = usage_val.as_object()
    {
        let mut clean_usage = serde_json::Map::new();
        for key in [
            "prompt_token_count",
            "candidates_token_count",
            "total_token_count",
        ] {
            if let Some(v) = u.get(key).filter(|v| !v.is_null()) {
                clean_usage.insert(key.to_string(), v.clone());
            }
        }
        if let Some(v) = u
            .get("cached_content_token_count")
            .or_else(|| u.get("cache_read_input_token_count"))
            .filter(|v| !v.is_null())
        {
            clean_usage.insert("cached_content_token_count".to_string(), v.clone());
        }
        if let Some(v) = u
            .get("thoughts_token_count")
            .or_else(|| u.get("thinking_token_count"))
            .filter(|v| !v.is_null())
        {
            clean_usage.insert("thoughts_token_count".to_string(), v.clone());
        }
        if !clean_usage.is_empty() {
            map.insert(
                "usage_metadata".to_string(),
                serde_json::Value::Object(clean_usage),
            );
        }
    }

    if let Some(actions_val) = map.remove("actions")
        && let Some(a) = actions_val.as_object()
    {
        let mut clean_actions = serde_json::Map::new();
        let state_delta = a
            .get("state_delta")
            .or_else(|| a.get("stateDelta"))
            .and_then(serde_json::Value::as_object)
            .cloned()
            .unwrap_or_default();
        clean_actions.insert(
            "state_delta".to_string(),
            serde_json::Value::Object(state_delta),
        );
        let artifact_delta = a
            .get("artifact_delta")
            .or_else(|| a.get("artifactDelta"))
            .and_then(serde_json::Value::as_object)
            .cloned()
            .unwrap_or_default();
        clean_actions.insert(
            "artifact_delta".to_string(),
            serde_json::Value::Object(artifact_delta),
        );
        if a.get("skip_summarization")
            .or_else(|| a.get("skipSummarization"))
            .and_then(serde_json::Value::as_bool)
            == Some(true)
        {
            clean_actions.insert(
                "skip_summarization".to_string(),
                serde_json::Value::Bool(true),
            );
        }
        if let Some(transfer) = a
            .get("transfer_to_agent")
            .or_else(|| a.get("transferToAgent"))
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        {
            clean_actions.insert(
                "transfer_to_agent".to_string(),
                serde_json::Value::String(transfer.to_string()),
            );
        }
        if a.get("escalate").and_then(serde_json::Value::as_bool) == Some(true) {
            clean_actions.insert("escalate".to_string(), serde_json::Value::Bool(true));
        }
        clean_actions.insert(
            "requested_auth_configs".to_string(),
            a.get("requested_auth_configs")
                .and_then(serde_json::Value::as_object)
                .cloned()
                .map(serde_json::Value::Object)
                .unwrap_or_else(|| serde_json::json!({})),
        );
        clean_actions.insert(
            "requested_tool_confirmations".to_string(),
            a.get("requested_tool_confirmations")
                .and_then(serde_json::Value::as_object)
                .cloned()
                .map(serde_json::Value::Object)
                .unwrap_or_else(|| serde_json::json!({})),
        );
        map.insert(
            "actions".to_string(),
            serde_json::Value::Object(clean_actions),
        );
    }

    if map
        .get("branch")
        .is_some_and(|v| v.is_null() || v.as_str() == Some(""))
    {
        map.remove("branch");
    }
    if map
        .get("long_running_tool_ids")
        .is_some_and(|v| v.is_null() || v.as_array().is_some_and(Vec::is_empty))
    {
        map.remove("long_running_tool_ids");
    }

    for rust_only in ["llm_request", "event_metadata", "provider_metadata"] {
        map.remove(rust_only);
    }
    map.retain(|_, v| !v.is_null());
}

/// Extracts delegated OAuth access tokens from `_StreamRunRequest.authorizations`
/// (or top-level `authorizations`) and inserts them into `delta_map`.
fn extract_authorizations_into_delta(
    auths: &serde_json::Map<String, serde_json::Value>,
    delta_map: &mut serde_json::Map<String, serde_json::Value>,
) -> bool {
    let mut found = false;
    let configured_keys = crate::auth::gemini_enterprise_auth_keys();
    for (auth_id, auth_val) in auths {
        let token = auth_val
            .as_str()
            .or_else(|| {
                auth_val
                    .get("access_token")
                    .and_then(serde_json::Value::as_str)
            })
            .or_else(|| {
                auth_val
                    .get("accessToken")
                    .and_then(serde_json::Value::as_str)
            })
            .map(str::trim)
            .filter(|t| !t.is_empty());
        let Some(token) = token else {
            continue;
        };
        found = true;
        let token_val = serde_json::Value::String(token.to_string());
        delta_map.insert(format!("temp:{auth_id}"), token_val.clone());
        delta_map.insert(auth_id.clone(), token_val.clone());
        if let Some(short) = auth_id
            .rsplit('/')
            .next()
            .filter(|s| !s.is_empty() && *s != auth_id)
        {
            delta_map.insert(format!("temp:{short}"), token_val.clone());
            delta_map.insert(short.to_string(), token_val.clone());
        }
        for key in &configured_keys {
            delta_map
                .entry(key.clone())
                .or_insert_with(|| token_val.clone());
        }
    }
    found
}

/// Applies `state` / `state_delta` / `authorizations` carried on a `stream_query` /
/// `async_stream_query` input before delegating to `adk_server::agent_engine`.
async fn apply_stream_query_state_delta(
    sessions: &dyn SessionService,
    input: &mut serde_json::Map<String, serde_json::Value>,
) {
    let Some(user_id) = input
        .get("user_id")
        .or_else(|| input.get("userId"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
    else {
        return;
    };
    let mut delta_map = serde_json::Map::new();
    for field in ["state", "state_delta", "stateDelta"] {
        if let Some(serde_json::Value::Object(mut obj)) = input.get(field).cloned() {
            copy_temp_state_keys(&mut obj);
            for (k, v) in obj {
                delta_map.insert(k, v);
            }
        }
    }
    if let Some(auths) = input
        .get("authorizations")
        .and_then(serde_json::Value::as_object)
    {
        extract_authorizations_into_delta(auths, &mut delta_map);
    }
    if delta_map.is_empty() {
        return;
    }
    let delta: HashMap<String, serde_json::Value> = delta_map.into_iter().collect();
    let session_id = match input.get("session_id").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id.to_string(),
        _ => {
            let generated = uuid::Uuid::new_v4().to_string();
            input.insert(
                "session_id".to_string(),
                serde_json::Value::String(generated.clone()),
            );
            generated
        }
    };
    let existing = sessions
        .get(adk_rust::session::GetRequest {
            app_name: APP_NAME.to_string(),
            user_id: user_id.clone(),
            session_id: session_id.clone(),
            num_recent_events: Some(1),
            after: None,
        })
        .await;
    match existing {
        Ok(_) => {
            if let (Ok(app_name), Ok(uid), Ok(sid)) = (
                adk_rust::AppName::try_from(APP_NAME),
                adk_rust::UserId::try_from(user_id.as_str()),
                adk_rust::SessionId::try_from(session_id.as_str()),
            ) {
                let mut event =
                    adk_rust::Event::new(format!("agent-engine-state-{}", uuid::Uuid::new_v4()));
                event.author = "agent_engine_dispatch".to_string();
                event.actions.state_delta = delta;
                let _ = sessions
                    .append_event_for_identity(adk_rust::session::AppendEventRequest {
                        identity: adk_rust::AdkIdentity {
                            app_name,
                            user_id: uid,
                            session_id: sid,
                        },
                        event,
                    })
                    .await;
            }
        }
        Err(err) if err.is_not_found() => {
            let _ = sessions
                .create(adk_rust::session::CreateRequest {
                    app_name: APP_NAME.to_string(),
                    user_id,
                    session_id: Some(session_id),
                    state: delta,
                })
                .await;
        }
        Err(_) => {}
    }
}

/// Converts a historical event from `_StreamRunRequest.events` into an [`adk_rust::Event`].
fn parse_historical_event(raw: &serde_json::Value) -> Option<adk_rust::Event> {
    let obj = raw.as_object()?;
    let inv_id = obj
        .get("invocation_id")
        .or_else(|| obj.get("invocationId"))
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or("history");
    let mut ev = match obj
        .get("id")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty())
    {
        Some(id) => adk_rust::Event::with_id(id, inv_id),
        None => adk_rust::Event::new(inv_id),
    };
    ev.author = obj
        .get("author")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("user")
        .to_string();
    if let Some(content_val) = obj.get("content").filter(|v| !v.is_null()).cloned() {
        let normalized = normalize_agent_engine_content(content_val);
        if let Ok(content) = serde_json::from_value::<adk_rust::Content>(normalized) {
            ev.llm_response.content = Some(content);
        }
    }
    if let Some(actions) = obj.get("actions").and_then(serde_json::Value::as_object) {
        if let Some(mut sd) = actions
            .get("state_delta")
            .or_else(|| actions.get("stateDelta"))
            .and_then(serde_json::Value::as_object)
            .cloned()
        {
            copy_temp_state_keys(&mut sd);
            ev.actions.state_delta = sd.into_iter().collect();
        }
        if let Some(ad) = actions
            .get("artifact_delta")
            .or_else(|| actions.get("artifactDelta"))
            .and_then(serde_json::Value::as_object)
        {
            for (k, v) in ad {
                if let Some(ver) = v.as_i64() {
                    ev.actions.artifact_delta.insert(k.clone(), ver);
                }
            }
        }
    }
    Some(ev)
}

/// Saves artifacts supplied in `_StreamRunRequest.artifacts` into `artifact_service`.
async fn save_request_artifacts(
    artifacts: &dyn ArtifactService,
    user_id: &str,
    session_id: &str,
    raw_artifacts: &[serde_json::Value],
) {
    for item in raw_artifacts {
        let Some(obj) = item.as_object() else {
            continue;
        };
        let Some(file_name) = obj
            .get("file_name")
            .or_else(|| obj.get("fileName"))
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let Some(versions) = obj.get("versions").and_then(serde_json::Value::as_array) else {
            continue;
        };
        let mut sorted: Vec<(Option<i64>, serde_json::Value)> = versions
            .iter()
            .filter_map(|v| {
                let v_obj = v.as_object()?;
                let ver = v_obj.get("version").and_then(|ver| {
                    ver.as_i64()
                        .or_else(|| ver.as_str().and_then(|s| s.parse::<i64>().ok()))
                });
                let data = v_obj.get("data")?.clone();
                Some((ver, data))
            })
            .collect();
        sorted.sort_by_key(|(ver, _)| ver.unwrap_or(0));
        for (version, mut data) in sorted {
            genai_part_to_adk_json(&mut data);
            if let Ok(part) = serde_json::from_value::<adk_rust::Part>(data) {
                let _ = artifacts
                    .save(SaveRequest {
                        app_name: APP_NAME.to_string(),
                        user_id: user_id.to_string(),
                        session_id: session_id.to_string(),
                        file_name: file_name.to_string(),
                        part,
                        version,
                    })
                    .await;
            }
        }
    }
}

/// Context captured from a `streaming_agent_run_with_events` request so its
/// NDJSON response stream can be wrapped in `_StreamingRunResponse` objects.
#[derive(Clone)]
struct StreamRunWithEventsContext {
    user_id: String,
    session_id: String,
    ephemeral_session: bool,
    sessions: Arc<dyn SessionService>,
    artifacts: Arc<dyn ArtifactService>,
}

/// Formats one NDJSON line emitted by `adk_server::agent_engine::dispatch_stream`
/// into either a `_StreamingRunResponse` line (for `streaming_agent_run_with_events`)
/// or a Python-ADK `Event` line (for `stream_query` / `async_stream_query`).
async fn format_agent_engine_ndjson_line(
    line: &[u8],
    run_ctx: Option<&StreamRunWithEventsContext>,
) -> Vec<u8> {
    let trimmed = line
        .strip_suffix(b"\n")
        .unwrap_or(line)
        .strip_suffix(b"\r")
        .unwrap_or(line);
    if trimmed.is_empty() {
        return Vec::new();
    }
    let Ok(mut json_val) = serde_json::from_slice::<serde_json::Value>(trimmed) else {
        return scrub_ndjson_line(line);
    };
    scrub_credentials_from_json(&mut json_val);

    let is_event = json_val.as_object().is_some_and(|m| {
        m.contains_key("author") || m.contains_key("invocation_id") || m.contains_key("actions")
    });

    if let Some(ctx) = run_ctx {
        if !is_event {
            // Convert mid-stream problem-JSON error into a valid _StreamingRunResponse error event.
            let detail = json_val
                .get("detail")
                .or_else(|| json_val.get("error"))
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Agent execution error")
                .to_string();
            let code = json_val
                .get("code")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("INTERNAL_ERROR")
                .to_string();
            let now_secs = chrono::Utc::now().timestamp_micros() as f64 / 1_000_000.0;
            let wrapped = serde_json::json!({
                "events": [{
                    "id": uuid::Uuid::new_v4().to_string(),
                    "invocation_id": "",
                    "author": APP_NAME,
                    "timestamp": now_secs,
                    "error_code": code,
                    "error_message": detail,
                    "content": {
                        "role": "model",
                        "parts": [{"text": detail}],
                    },
                    "turn_complete": true,
                    "actions": {
                        "state_delta": {},
                        "artifact_delta": {},
                        "requested_auth_configs": {},
                        "requested_tool_confirmations": {},
                    }
                }],
                "session_id": ctx.session_id,
            });
            if let Ok(mut out) = serde_json::to_vec(&wrapped) {
                out.push(b'\n');
                return out;
            }
            return scrub_ndjson_line(line);
        }

        let artifact_delta: Vec<(String, i64)> = json_val
            .get("actions")
            .and_then(|a| a.get("artifact_delta").or_else(|| a.get("artifactDelta")))
            .and_then(serde_json::Value::as_object)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_i64().map(|ver| (k.clone(), ver)))
                    .collect()
            })
            .unwrap_or_default();

        event_to_python_adk_json(&mut json_val);

        let mut artifacts_out = Vec::new();
        for (file_name, version) in artifact_delta {
            if let Ok(loaded) = ctx
                .artifacts
                .load(LoadRequest {
                    app_name: APP_NAME.to_string(),
                    user_id: ctx.user_id.clone(),
                    session_id: ctx.session_id.clone(),
                    file_name: file_name.clone(),
                    version: Some(version),
                })
                .await
                && let Ok(raw_part) = serde_json::to_value(&loaded.part)
            {
                let part_json = adk_part_to_genai_json(&raw_part);
                artifacts_out.push(serde_json::json!({
                    "file_name": file_name,
                    "versions": [{
                        "version": version,
                        "data": part_json,
                    }],
                }));
            }
        }

        let mut resp = serde_json::Map::new();
        resp.insert("events".to_string(), serde_json::json!([json_val]));
        if !artifacts_out.is_empty() {
            resp.insert(
                "artifacts".to_string(),
                serde_json::Value::Array(artifacts_out),
            );
        }
        if !ctx.session_id.is_empty() {
            resp.insert(
                "session_id".to_string(),
                serde_json::Value::String(ctx.session_id.clone()),
            );
        }
        if let Ok(mut out) = serde_json::to_vec(&serde_json::Value::Object(resp)) {
            out.push(b'\n');
            return out;
        }
        return scrub_ndjson_line(line);
    }

    if is_event {
        event_to_python_adk_json(&mut json_val);
    }
    if let Ok(mut out) = serde_json::to_vec(&json_val) {
        out.push(b'\n');
        return out;
    }
    scrub_ndjson_line(line)
}

/// Re-streams an Agent Engine NDJSON response, converting each event line into
/// the Python `google-adk` schema (and wrapping `streaming_agent_run_with_events`
/// lines in `_StreamingRunResponse`).
fn transform_agent_engine_stream_response(
    response: Response,
    run_ctx: Option<StreamRunWithEventsContext>,
) -> Response {
    let (mut parts, body) = response.into_parts();
    parts.headers.remove(header::CONTENT_LENGTH);
    let upstream = body.into_data_stream();
    let stream = futures::stream::unfold(
        (upstream, Vec::<u8>::new(), run_ctx, false),
        |(mut upstream, mut buf, run_ctx, done)| async move {
            if done {
                return None;
            }
            loop {
                if let Some(at) = memchr::memchr(b'\n', &buf) {
                    let line: Vec<u8> = buf.drain(..=at).collect();
                    let formatted = format_agent_engine_ndjson_line(&line, run_ctx.as_ref()).await;
                    if !formatted.is_empty() {
                        return Some((
                            Ok::<_, axum::Error>(Bytes::from(formatted)),
                            (upstream, buf, run_ctx, false),
                        ));
                    }
                    continue;
                }
                if buf.len() > MAX_SCRUBBED_SSE_EVENT_BYTES {
                    buf.clear();
                    return Some((
                        Err(axum::Error::new(
                            "NDJSON event line too large to process; stream aborted",
                        )),
                        (upstream, buf, run_ctx, true),
                    ));
                }
                match upstream.next().await {
                    Some(Ok(chunk)) => {
                        buf.extend_from_slice(&chunk);
                    }
                    Some(Err(e)) => {
                        return Some((Err(e), (upstream, buf, run_ctx, true)));
                    }
                    None => {
                        let tail = if buf.is_empty() {
                            Vec::new()
                        } else {
                            let rem = std::mem::take(&mut buf);
                            format_agent_engine_ndjson_line(&rem, run_ctx.as_ref()).await
                        };
                        if let Some(ref ctx) = run_ctx
                            && ctx.ephemeral_session
                        {
                            let _ = ctx
                                .sessions
                                .delete(adk_rust::session::DeleteRequest {
                                    app_name: APP_NAME.to_string(),
                                    user_id: ctx.user_id.clone(),
                                    session_id: ctx.session_id.clone(),
                                })
                                .await;
                        }
                        if tail.is_empty() {
                            return None;
                        }
                        return Some((Ok(Bytes::from(tail)), (upstream, buf, run_ctx, true)));
                    }
                }
            }
        },
    )
    .fuse();
    Response::from_parts(parts, Body::from_stream(stream))
}

/// Converts any `events` arrays inside a unary `/api/reasoning_engine` response
/// (`get_session`, `create_session`, `list_sessions`) into Python `google-adk` format.
async fn transform_agent_engine_unary_response(response: Response) -> Response {
    let (mut parts, body) = response.into_parts();
    let Ok(bytes) = to_bytes(body, MAX_SCRUBBED_JSON_BYTES).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let Ok(mut json_val) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return Response::from_parts(parts, Body::from(bytes));
    };
    scrub_credentials_from_json(&mut json_val);
    if let Some(output) = json_val
        .get_mut("output")
        .and_then(serde_json::Value::as_object_mut)
    {
        if let Some(events) = output
            .get_mut("events")
            .and_then(serde_json::Value::as_array_mut)
        {
            for ev in events.iter_mut() {
                event_to_python_adk_json(ev);
            }
        }
        if let Some(sessions) = output
            .get_mut("sessions")
            .and_then(serde_json::Value::as_array_mut)
        {
            for sess in sessions.iter_mut() {
                if let Some(events) = sess
                    .get_mut("events")
                    .and_then(serde_json::Value::as_array_mut)
                {
                    for ev in events.iter_mut() {
                        event_to_python_adk_json(ev);
                    }
                }
            }
        }
    }
    if let Ok(cleaned) = serde_json::to_vec(&json_val) {
        parts.headers.remove(header::CONTENT_LENGTH);
        return Response::from_parts(parts, Body::from(cleaned));
    }
    Response::from_parts(parts, Body::from(bytes))
}

/// Normalizes incoming Agent Engine requests and outgoing responses so:
/// 1. Requests forwarded by the Vertex AI ReasoningEngine proxy without a
///    `Content-Type: application/json` header are still accepted by `axum::Json`.
/// 2. Both `class_method` and `classMethod` envelope keys are accepted.
/// 3. Gemini Enterprise's `streaming_agent_run_with_events` (`_StreamRunRequest`
///    carrying `message`, `authorizations`, `events`, `artifacts`, and optional
///    `user_id`/`session_id`) is translated into `AgentRunRequest` on input and
///    wrapped in `_StreamingRunResponse` (`{"events": [...], "artifacts": [...], "session_id": "..."}`)
///    on output.
/// 4. `temp:<AUTH_ID>` tokens and `stream_query` state deltas are preserved
///    across session persistence while scrubbed from all outgoing responses.
async fn normalize_agent_engine_request(
    State((sessions, artifacts)): State<(Arc<dyn SessionService>, Arc<dyn ArtifactService>)>,
    req: Request,
    next: Next,
) -> Response {
    let is_stream_path = req.uri().path() == "/api/stream_reasoning_engine";
    let (mut parts, body) = req.into_parts();
    parts.headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    parts.headers.remove(header::ACCEPT_ENCODING);
    let Ok(bytes) = to_bytes(body, 16 * 1024 * 1024).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    let Ok(mut payload) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        parts.headers.remove(header::CONTENT_LENGTH);
        return next
            .run(Request::from_parts(parts, Body::from(bytes)))
            .await;
    };
    let mut mutated = false;
    if let Some(obj) = payload.as_object_mut()
        && !obj.contains_key("class_method")
        && let Some(camel) = obj.remove("classMethod")
    {
        obj.insert("class_method".to_string(), camel);
        mutated = true;
    }
    let class_method = payload
        .get("class_method")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let mut stream_run_ctx: Option<StreamRunWithEventsContext> = None;

    if let Some(input) = payload.get_mut("input").and_then(|v| v.as_object_mut()) {
        if class_method == "async_add_session_to_memory"
            && (!input.contains_key("user_id") || !input.contains_key("session_id"))
            && let Some(session_obj) = input.get("session").and_then(|v| v.as_object())
        {
            let uid = session_obj
                .get("user_id")
                .or_else(|| session_obj.get("userId"))
                .cloned();
            let sid = session_obj.get("id").cloned();
            if let Some(uid) = uid {
                input.entry("user_id".to_string()).or_insert(uid);
                mutated = true;
            }
            if let Some(sid) = sid {
                input.entry("session_id".to_string()).or_insert(sid);
                mutated = true;
            }
        }
        if let Some(state_obj) = input.get_mut("state").and_then(|v| v.as_object_mut())
            && copy_temp_state_keys(state_obj)
        {
            mutated = true;
        }
        if matches!(class_method.as_str(), "stream_query" | "async_stream_query") {
            if !input.contains_key("user_id") {
                let uid = input
                    .get("userId")
                    .and_then(serde_json::Value::as_str)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("default_user")
                    .to_string();
                input.insert("user_id".to_string(), serde_json::Value::String(uid));
                mutated = true;
            }
            if let Some(msg) = input.get("message").cloned()
                && msg.is_object()
            {
                input.insert("message".to_string(), normalize_agent_engine_content(msg));
                mutated = true;
            }
            if input.contains_key("state_delta")
                || input.contains_key("stateDelta")
                || input.contains_key("state")
                || input.contains_key("authorizations")
            {
                apply_stream_query_state_delta(sessions.as_ref(), input).await;
                mutated = true;
            }
        }
        if class_method == "streaming_agent_run_with_events" {
            let parsed_inner = match input.get("request_json") {
                Some(serde_json::Value::String(raw_req)) => {
                    serde_json::from_str::<serde_json::Value>(raw_req).ok()
                }
                Some(serde_json::Value::Object(map)) => {
                    Some(serde_json::Value::Object(map.clone()))
                }
                _ => None,
            };
            if let Some(mut inner) = parsed_inner
                && let Some(inner_obj) = inner.as_object_mut()
            {
                let user_id = inner_obj
                    .get("user_id")
                    .or_else(|| inner_obj.get("userId"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("default_user")
                    .to_string();
                inner_obj.insert(
                    "user_id".to_string(),
                    serde_json::Value::String(user_id.clone()),
                );

                let raw_session_id = inner_obj
                    .get("session_id")
                    .or_else(|| inner_obj.get("sessionId"))
                    .and_then(serde_json::Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let ephemeral_session = raw_session_id.is_none();
                let session_id = raw_session_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
                inner_obj.insert(
                    "session_id".to_string(),
                    serde_json::Value::String(session_id.clone()),
                );

                if let Some(raw_msg) = inner_obj
                    .get("new_message")
                    .or_else(|| inner_obj.get("newMessage"))
                    .or_else(|| inner_obj.get("message"))
                    .cloned()
                {
                    let normalized_msg = normalize_agent_engine_content(raw_msg);
                    inner_obj.insert("new_message".to_string(), normalized_msg);
                }

                let mut delta_map = serde_json::Map::new();
                for field in ["state", "state_delta", "stateDelta"] {
                    if let Some(serde_json::Value::Object(mut obj)) = inner_obj.remove(field) {
                        copy_temp_state_keys(&mut obj);
                        for (k, v) in obj {
                            delta_map.insert(k, v);
                        }
                    }
                }
                let mut has_authorizations = false;
                if let Some(auths) = inner_obj
                    .get("authorizations")
                    .and_then(serde_json::Value::as_object)
                {
                    has_authorizations = extract_authorizations_into_delta(auths, &mut delta_map);
                }
                copy_temp_state_keys(&mut delta_map);
                if !delta_map.is_empty() {
                    inner_obj.insert(
                        "state_delta".to_string(),
                        serde_json::Value::Object(delta_map.clone()),
                    );
                }

                // Initialize session with historical events if the session does not yet exist.
                let history_events = inner_obj
                    .get("events")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if !history_events.is_empty() {
                    let exists = sessions
                        .get(adk_rust::session::GetRequest {
                            app_name: APP_NAME.to_string(),
                            user_id: user_id.clone(),
                            session_id: session_id.clone(),
                            num_recent_events: Some(1),
                            after: None,
                        })
                        .await
                        .is_ok();
                    if !exists {
                        let mut initial_state: HashMap<String, serde_json::Value> = delta_map
                            .iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        initial_state.insert(
                            "agent_engine:upstream_session_id".to_string(),
                            serde_json::Value::String(session_id.clone()),
                        );
                        if sessions
                            .create(adk_rust::session::CreateRequest {
                                app_name: APP_NAME.to_string(),
                                user_id: user_id.clone(),
                                session_id: Some(session_id.clone()),
                                state: initial_state,
                            })
                            .await
                            .is_ok()
                            && let (Ok(app_name), Ok(uid), Ok(sid)) = (
                                adk_rust::AppName::try_from(APP_NAME),
                                adk_rust::UserId::try_from(user_id.as_str()),
                                adk_rust::SessionId::try_from(session_id.as_str()),
                            )
                        {
                            for raw_ev in &history_events {
                                if let Some(ev) = parse_historical_event(raw_ev) {
                                    let _ = sessions
                                        .append_event_for_identity(
                                            adk_rust::session::AppendEventRequest {
                                                identity: adk_rust::AdkIdentity {
                                                    app_name: app_name.clone(),
                                                    user_id: uid.clone(),
                                                    session_id: sid.clone(),
                                                },
                                                event: ev,
                                            },
                                        )
                                        .await;
                                }
                            }
                        }
                    }
                }

                if let Some(req_artifacts) = inner_obj
                    .get("artifacts")
                    .and_then(serde_json::Value::as_array)
                    .filter(|a| !a.is_empty())
                {
                    save_request_artifacts(
                        artifacts.as_ref(),
                        &user_id,
                        &session_id,
                        req_artifacts,
                    )
                    .await;
                }

                tracing::info!(
                    class_method = %class_method,
                    user_id = %user_id,
                    session_id = %session_id,
                    ephemeral_session = ephemeral_session,
                    has_authorizations = has_authorizations,
                    history_events = history_events.len(),
                    "normalized streaming_agent_run_with_events request"
                );

                stream_run_ctx = Some(StreamRunWithEventsContext {
                    user_id,
                    session_id,
                    ephemeral_session,
                    sessions: sessions.clone(),
                    artifacts: artifacts.clone(),
                });

                if let Ok(updated) = serde_json::to_string(&inner) {
                    input.insert(
                        "request_json".to_string(),
                        serde_json::Value::String(updated),
                    );
                    mutated = true;
                }
            }
        }
    }
    let body_bytes = if mutated {
        Bytes::from(serde_json::to_vec(&payload).unwrap_or_else(|_| bytes.to_vec()))
    } else {
        bytes
    };
    parts.headers.remove(header::CONTENT_LENGTH);
    let response = next
        .run(Request::from_parts(parts, Body::from(body_bytes)))
        .await;
    if !response.status().is_success() {
        return response;
    }
    if is_stream_path {
        transform_agent_engine_stream_response(response, stream_run_ctx)
    } else {
        transform_agent_engine_unary_response(response).await
    }
}

/// Builds the Vertex AI Agent Engine (ReasoningEngine BYOC) dispatch router
/// (`POST /api/reasoning_engine`, `POST /api/stream_reasoning_engine`).
///
/// # Errors
///
/// Returns an error when the underlying runner cannot be constructed.
pub fn build_agent_engine_router(runtime: &AgentRuntime) -> anyhow::Result<Router> {
    let runner = runtime.build_runner(DEFAULT_USER_ID)?;
    let state = adk_rust::server::AgentEngineState::new(runner)
        .with_memory_service(runtime.memory_service.clone())
        .with_artifact_service(runtime.artifact_service.clone());
    Ok(
        adk_rust::server::agent_engine_router(state).layer(middleware::from_fn_with_state(
            (
                runtime.session_service.clone(),
                runtime.artifact_service.clone(),
            ),
            normalize_agent_engine_request,
        )),
    )
}

/// Assembles the HTTP application: adk-server's routes (authenticated through
/// [`CookieIdentityExtractor`]), the OAuth / studio routes, the `/ui/` helper
/// injection, [`guard_adk_routes`], and HTTP response compression.
///
/// # Errors
///
/// Returns an error when the adk-server application cannot be built.
pub fn build_app(launcher: Launcher, routes: OAuthRoutes) -> anyhow::Result<Router> {
    build_app_with_agent_engine(launcher, routes, None)
}

/// Like [`build_app`], optionally merging the Agent Engine BYOC dispatch router.
///
/// # Errors
///
/// Returns an error when the adk-server application cannot be built.
pub fn build_app_with_agent_engine(
    launcher: Launcher,
    routes: OAuthRoutes,
    agent_engine: Option<Router>,
) -> anyhow::Result<Router> {
    let credentials = routes.credentials.clone();
    let admins = routes.admins.clone();
    let mut app = launcher
        .with_request_context_extractor(Arc::new(CookieIdentityExtractor::new(credentials.clone())))
        .build_app()?
        .merge(oauth_router(routes));
    if let Some(ae_router) = agent_engine {
        app = app.merge(ae_router);
    }
    let app = app
        .layer(middleware::from_fn(inject_auth_helper))
        .layer(middleware::from_fn_with_state(
            (credentials, admins),
            guard_adk_routes,
        ))
        .layer(tower_http::compression::CompressionLayer::new().compress_when(compressible()));
    Ok(app)
}

/// How long in-flight requests may finish after SIGTERM (`SHUTDOWN_GRACE_SECS`).
fn shutdown_grace_period() -> Duration {
    std::env::var("SHUTDOWN_GRACE_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(DEFAULT_SHUTDOWN_GRACE, Duration::from_secs)
}

/// Serves the agent (web UI at `/ui/`, REST API under `/api`, OAuth callback,
/// and — when [`is_agent_engine_enabled`] is true — the Agent Engine BYOC
/// dispatch routes `/api/reasoning_engine` and `/api/stream_reasoning_engine`).
///
/// # Errors
///
/// Returns an error when the listener cannot be bound or the server fails.
pub async fn serve(runtime: AgentRuntime, host: &str, port: u16) -> anyhow::Result<()> {
    serve_with_mode(runtime, host, port, is_agent_engine_enabled()).await
}

/// Serves the agent in Agent Engine BYOC mode (`/api/reasoning_engine` and
/// `/api/stream_reasoning_engine` enabled).
///
/// # Errors
///
/// Returns an error when the listener cannot be bound or the server fails.
pub async fn serve_agent_engine(
    runtime: AgentRuntime,
    host: &str,
    port: u16,
) -> anyhow::Result<()> {
    serve_with_mode(runtime, host, port, true).await
}

async fn serve_with_mode(
    runtime: AgentRuntime,
    host: &str,
    port: u16,
    agent_engine: bool,
) -> anyhow::Result<()> {
    if !agent_engine {
        runtime
            .credentials
            .enable_interactive_wait(Duration::from_secs(300), true);
    }
    runtime
        .credentials
        .attach_session_service(runtime.session_service.clone());
    let ae_router = if agent_engine {
        Some(build_agent_engine_router(&runtime)?)
    } else {
        None
    };
    let app =
        build_app_with_agent_engine(launcher(&runtime), runtime.oauth_routes(true), ae_router)?;
    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    if agent_engine {
        tracing::info!(
            "AdSpace Agent (Agent Engine BYOC) listening on http://{host}:{port}  (/api/reasoning_engine, /api/stream_reasoning_engine, /health)"
        );
    } else if runtime.credentials.flow().config().is_configured() {
        tracing::info!(
            "AdSpace Agent listening on http://{host}:{port}  (studio: http://{host}:{port}/app/  ·  Google Sign-In required; /ui/ disabled)"
        );
    } else {
        tracing::info!(
            "AdSpace Agent listening on http://{host}:{port}  (studio: http://{host}:{port}/app/  ·  dev UI: http://{host}:{port}/ui/)"
        );
        tracing::warn!(
            "CLIENT_ID / CLIENT_SECRET are not set: the API and /ui/ are open to anyone who can reach this port as the anonymous user"
        );
    }
    runtime.log_redirect_uri();

    let credentials = runtime.credentials.clone();
    let mut graceful_rx = credentials.subscribe_shutdown();
    let mut force_rx = credentials.subscribe_shutdown();
    tokio::spawn(async move {
        shutdown_signal().await;
        credentials.notify_shutdown();
    });

    let server = axum::serve(listener, app).with_graceful_shutdown(async move {
        let _ = graceful_rx.wait_for(|&v| v).await;
    });

    // Let in-flight turns finish, but never outlive the platform's grace
    // period (Cloud Run sends SIGKILL 10 s after SIGTERM).
    let grace = shutdown_grace_period();
    tokio::select! {
        res = server => res?,
        _ = async move {
            let _ = force_rx.wait_for(|&v| v).await;
            tokio::time::sleep(grace).await;
            tracing::warn!("Shutdown grace period ({grace:?}) elapsed; aborting in-flight requests");
        } => {}
    }
    Ok(())
}

/// Runs the interactive console REPL (adk-cli's launcher: readline history,
/// streaming output, think-block rendering). The session belongs to user
/// [`DEFAULT_USER_ID`]; adk-cli does not expose a user-id setting.
///
/// The OAuth redirect handler is served on `host:port` in the background so
/// Google authorizations started from the console can complete into the
/// console's session.
///
/// # Errors
///
/// Returns an error when the REPL cannot start or a turn fails.
pub async fn console(runtime: AgentRuntime, host: &str, port: u16) -> anyhow::Result<()> {
    match tokio::net::TcpListener::bind((host, port)).await {
        Ok(listener) => {
            tracing::info!("OAuth callback listening on http://{host}:{port}{OAUTH_CALLBACK_PATH}");
            runtime.log_redirect_uri();
            // No automatic resume here: the REPL only prints its own turns.
            tokio::spawn(
                axum::serve(listener, oauth_router(runtime.oauth_routes(false))).into_future(),
            );
        }
        Err(e) => tracing::warn!(
            "Cannot bind {host}:{port} for the OAuth callback ({e}); Google authorizations will not complete in this console"
        ),
    }
    launcher(&runtime).run_console_directly().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helper_tag_goes_before_body_end_or_at_the_end() {
        let page = "<html><body><div id=\"root\"></div></body></html>";
        let injected = inject_auth_helper_tag(page);
        assert_eq!(
            injected,
            format!("<html><body><div id=\"root\"></div>{AUTH_HELPER_TAG}</body></html>")
        );
        assert_eq!(
            inject_auth_helper_tag("<p>no body tag</p>"),
            format!("<p>no body tag</p>{AUTH_HELPER_TAG}")
        );
        // The tag loads the helper from our own origin (the UI's CSP is `script-src 'self'`)
        // as a module, so it can import the shared renderer from `/app/`.
        assert!(AUTH_HELPER_TAG.contains(AUTH_HELPER_PATH));
        assert!(AUTH_HELPER_TAG.contains("type=\"module\""));
    }

    #[test]
    fn callback_script_reports_to_the_opener_and_closes_only_on_success() {
        let ok = callback_script("ok");
        assert!(ok.contains("new BroadcastChannel(\"adspace:oauth\").postMessage"));
        assert!(ok.contains("window.opener.postMessage"));
        assert!(ok.contains("status:\"ok\""));
        assert!(ok.contains("window.close()"));
        let error = callback_script("error");
        assert!(error.contains("status:\"error\""));
        assert!(!error.contains("window.close()"));
    }

    #[test]
    fn helper_script_targets_google_auth_links_and_the_resume_turn() {
        let helper = app_module(AUTH_HELPER_MODULE).expect("helper module is embedded");
        assert!(helper.contains("https://accounts.google.com/o/oauth2/"));
        assert!(helper.contains("window.open("));
        assert!(helper.contains("new BroadcastChannel(CHANNEL)"));
        assert!(helper.contains("const CHANNEL = \"adspace:oauth\""));
        // It is the `/ui/` host of the shared renderer, not a second copy of it:
        // served from `/oauth2/`, so the imports must be absolute `/app/` URLs.
        assert!(helper.contains("from \"/app/adk-ui/index.js\""));
        assert!(helper.contains("extractUiResponse"));
        assert!(
            helper.contains("href = SHARED_CSS_PATH") && helper.contains("\"/app/adk-ui.css\"")
        );
        assert!(!helper.contains("const renderInnerComponent"));
        assert!(helper.contains("Element.prototype.scrollTo"));
        // Must match the text `resume_message` produces.
        let marker = "Google access has been granted for this chat";
        assert!(helper.contains(marker));
        let token = OAuthToken {
            access_token: String::new(),
            refresh_token: None,
            expires_at: None,
            scopes: vec![],
        };
        assert!(resume_message(&token).starts_with(marker));
    }

    #[tokio::test]
    async fn middleware_injects_only_into_the_ui_html_page() {
        use axum::routing::get;
        use tower::ServiceExt as _;

        let app = Router::new()
            .route(
                "/ui/",
                get(|| async { Html("<html><body><div id=\"root\"></div></body></html>") }),
            )
            .route(
                "/ui/assets/app.js",
                get(|| async {
                    (
                        [(header::CONTENT_TYPE, "application/javascript")],
                        "console.log(1)",
                    )
                }),
            )
            .route(
                "/api/page",
                get(|| async { Html("<html><body>api</body></html>") }),
            )
            .layer(middleware::from_fn(inject_auth_helper));

        let get_body = |path: &'static str| {
            let app = app.clone();
            async move {
                let response = app
                    .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                    .await
                    .unwrap();
                let length = response
                    .headers()
                    .get(header::CONTENT_LENGTH)
                    .map(|v| v.to_str().unwrap().parse::<usize>().unwrap());
                let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
                (String::from_utf8(body.to_vec()).unwrap(), length)
            }
        };

        let (page, length) = get_body("/ui/").await;
        assert!(page.contains(AUTH_HELPER_TAG), "{page}");
        assert_eq!(length, Some(page.len()));
        let (asset, _) = get_body("/ui/assets/app.js").await;
        assert_eq!(asset, "console.log(1)");
        let (api, _) = get_body("/api/page").await;
        assert!(!api.contains(AUTH_HELPER_TAG));
    }

    #[tokio::test]
    async fn studio_routes_and_user_cookie_authentication_work() {
        use adk_rust::session::InMemorySessionService;
        use tower::ServiceExt as _;

        let flow = crate::auth::GoogleOAuthFlow::new(crate::auth::OAuthClientConfig {
            client_id: "cid".into(),
            client_secret: "secret".into(),
            redirect_uri: "http://localhost:8000/oauth2/callback".into(),
        });
        let credentials = Arc::new(SessionCredentialManager::new(flow));
        let routes = OAuthRoutes {
            credentials: credentials.clone(),
            sessions: Arc::new(InMemorySessionService::new()),
            resume: None,
            admins: AdminAllowList::new(["Nate@example.com".to_string()]),
        };
        let app = oauth_router(routes);

        // 1. `/app/` serves the studio shell.
        let resp = app
            .clone()
            .oneshot(Request::builder().uri("/app/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let html = String::from_utf8(
            to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(html.contains("AdSpace"));
        assert!(html.contains("/app/app.js"));

        // 1a. The studio client is served as ES modules under `/app/<path>`;
        // the two stylesheets keep their own routes and unknown paths are 404.
        for path in ["/app/app.js", "/app/state.js", "/app/adk-ui/charts.js"] {
            let js = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(js.status(), StatusCode::OK, "{path}");
            assert_eq!(
                js.headers()[header::CONTENT_TYPE],
                "application/javascript; charset=utf-8"
            );
            assert_eq!(js.headers()[header::CACHE_CONTROL], "no-cache");
        }
        // The `/ui/` helper is served as a JavaScript module from both paths
        // (the injected tag uses `/oauth2/helper.js`), with identical bytes.
        let mut helper_bodies = Vec::new();
        for path in [AUTH_HELPER_PATH, "/app/ui-helper.js"] {
            let js = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(js.status(), StatusCode::OK, "{path}");
            assert_eq!(
                js.headers()[header::CONTENT_TYPE],
                "application/javascript; charset=utf-8"
            );
            helper_bodies.push(to_bytes(js.into_body(), usize::MAX).await.unwrap());
        }
        assert_eq!(helper_bodies[0], helper_bodies[1]);
        for path in ["/app/app.css", "/app/adk-ui.css"] {
            let css = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(css.status(), StatusCode::OK, "{path}");
            assert_eq!(
                css.headers()[header::CONTENT_TYPE],
                "text/css; charset=utf-8"
            );
        }
        // The studio links both stylesheets.
        assert!(html.contains("/app/adk-ui.css") && html.contains("/app/app.css"));
        let missing = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/app/nope.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        // 1b. `/favicon.ico` and `/favicon.svg` return 200 OK with the SVG icon.
        for path in ["/favicon.ico", "/favicon.svg"] {
            let fav = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(fav.status(), StatusCode::OK);
        }

        // 1c. The liveness check answers without a cookie.
        let alive = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(HEALTHCHECK_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(alive.status(), StatusCode::OK);
        assert_eq!(
            to_bytes(alive.into_body(), usize::MAX)
                .await
                .unwrap()
                .as_ref(),
            b"ok"
        );

        // 2. `/auth/me` is unauthenticated without a cookie.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/auth/me")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(body["authenticated"], false);

        // 3. `/auth/me` returns the signed-in Google user when a valid cookie is present.
        let user = AuthenticatedUser {
            id: "nate@example.com".into(),
            email: "nate@example.com".into(),
            name: "Nate".into(),
            picture: Some("https://example.com/avatar.png".into()),
            sub: Some("1234567890".into()),
            iat: chrono::Utc::now().timestamp(),
        };
        let cookie_val = credentials.sign_user_cookie(&user);
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/auth/me")
                    .header(header::COOKIE, format!("{USER_COOKIE_NAME}={cookie_val}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let body: serde_json::Value =
            serde_json::from_slice(&to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(body["authenticated"], true);
        assert_eq!(body["user"]["email"], "nate@example.com");
        assert_eq!(body["is_admin"], true);
        assert!(body["models"]["pricing"].is_object());

        // 4. `/oauth2/callback` escapes reflected query error strings.
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/oauth2/callback?error=%3Cscript%3Ealert(1)%3C/script%3E")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let err_html = String::from_utf8(
            to_bytes(resp.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(!err_html.contains("<script>alert(1)</script>"));
        assert!(err_html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));

        // 5. `guard_adk_routes` blocks unauthenticated user API calls and scrubs
        // both credential keys from session responses.
        let api_app = Router::new()
            .route(
                "/api/apps/{app}/users/{user_id}/sessions",
                get(|| async {
                    Json(serde_json::json!([{
                        "id": "s1",
                        "state": {
                            CREDENTIALS_STATE_KEY: { "access_token": "secret-token" },
                            USER_CREDENTIALS_STATE_KEY: { "access_token": "user-secret-token" },
                            "visible": "ok"
                        }
                    }]))
                }),
            )
            .layer(middleware::from_fn_with_state(
                (credentials.clone(), AdminAllowList::default()),
                guard_adk_routes,
            ));

        let unauth = api_app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/apps/adspace_agent/users/nate%40example.com/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

        let authed = api_app
            .oneshot(
                Request::builder()
                    .uri("/api/apps/adspace_agent/users/nate%40example.com/sessions")
                    .header(header::COOKIE, format!("{USER_COOKIE_NAME}={cookie_val}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authed.status(), StatusCode::OK);
        let scrubbed: serde_json::Value =
            serde_json::from_slice(&to_bytes(authed.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert!(scrubbed[0]["state"].get(CREDENTIALS_STATE_KEY).is_none());
        assert!(
            scrubbed[0]["state"]
                .get(USER_CREDENTIALS_STATE_KEY)
                .is_none()
        );
        assert_eq!(scrubbed[0]["state"]["visible"], "ok");

        // 6. `/auth/models` allows administrators to update agent, image, and video models.
        let unauth_models = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/auth/models")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"model":"gemini-4-argon"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauth_models.status(), StatusCode::UNAUTHORIZED);

        let update_resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/auth/models")
                    .header(header::COOKIE, format!("{USER_COOKIE_NAME}={cookie_val}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        r#"{"model":"gemini-4-argon","image_model":"imagen-4.0-generate-001","video_model":"veo-3.1-generate-001"}"#,
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(update_resp.status(), StatusCode::OK);
        let updated_models: serde_json::Value =
            serde_json::from_slice(&to_bytes(update_resp.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert_eq!(updated_models["model"], "gemini-4-argon");
        assert_eq!(updated_models["image_model"], "imagen-4.0-generate-001");
        assert_eq!(updated_models["video_model"], "veo-3.1-generate-001");
        assert!(updated_models["pricing"].is_object());

        // 6b. A signed-in user who is not an administrator is refused.
        let other = AuthenticatedUser {
            id: "guest@example.com".into(),
            email: "guest@example.com".into(),
            ..user.clone()
        };
        let other_cookie = credentials.sign_user_cookie(&other);
        let forbidden = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/auth/models")
                    .header(header::COOKIE, format!("{USER_COOKIE_NAME}={other_cookie}"))
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"model":"gemini-4-argon"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
        let me = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/auth/me")
                    .header(header::COOKIE, format!("{USER_COOKIE_NAME}={other_cookie}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let me: serde_json::Value =
            serde_json::from_slice(&to_bytes(me.into_body(), usize::MAX).await.unwrap()).unwrap();
        assert_eq!(me["is_admin"], false);

        // 7. `/oauth2/events` requires the user cookie, then closes immediately
        // on `notify_shutdown()`.
        let anonymous = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(AUTH_EVENTS_PATH)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(AUTH_EVENTS_PATH)
                    .header(header::COOKIE, format!("{USER_COOKIE_NAME}={cookie_val}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        credentials.notify_shutdown();
        let bytes = tokio::time::timeout(
            Duration::from_millis(200),
            to_bytes(resp.into_body(), usize::MAX),
        )
        .await
        .expect("SSE stream must close immediately on shutdown")
        .unwrap();
        assert!(bytes.is_empty());
    }

    fn test_credentials(configured: bool) -> Arc<SessionCredentialManager> {
        let flow = crate::auth::GoogleOAuthFlow::new(crate::auth::OAuthClientConfig {
            client_id: if configured {
                "cid".into()
            } else {
                String::new()
            },
            client_secret: if configured {
                "secret".into()
            } else {
                String::new()
            },
            redirect_uri: "https://agent.example.com/oauth2/callback".into(),
        });
        Arc::new(SessionCredentialManager::new(flow))
    }

    fn signed_cookie(credentials: &SessionCredentialManager, email: &str) -> String {
        let user = AuthenticatedUser {
            id: email.into(),
            email: email.into(),
            name: "Test".into(),
            picture: None,
            sub: None,
            iat: chrono::Utc::now().timestamp(),
        };
        format!("{USER_COOKIE_NAME}={}", credentials.sign_user_cookie(&user))
    }

    #[test]
    fn user_scoped_api_paths_are_recognised() {
        for (path, expected) in [
            ("/api/apps/a/users/u%40x.com/sessions", Some("u@x.com")),
            ("/api/apps/a/users/u/sessions/s", Some("u")),
            ("/api/sessions/a/u/s", Some("u")),
            ("/api/sessions/a/u/s/artifacts/f", Some("u")),
            ("/api/run/a/u/s", Some("u")),
            ("/api/debug/graph/a/u/s/e", Some("u")),
            ("/api/run", None),
            ("/api/run_sse", None),
            ("/api/sessions", None),
            ("/api/apps", None),
            ("/api/apps/a/eval_sets", None),
            ("/auth/me", None),
        ] {
            assert_eq!(
                extract_api_path_user_id(path).as_deref(),
                expected,
                "{path}"
            );
        }
        assert!(carries_session_state("/api/run_sse"));
        assert!(carries_session_state("/api/run"));
        assert!(carries_session_state("/api/run/a/u/s"));
        assert!(carries_session_state("/api/sessions/a/u/s"));
        assert!(carries_session_state("/api/apps/a/users/u/sessions/s"));
        assert!(!carries_session_state("/api/sessions/a/u/s/artifacts/f"));
        assert!(!carries_session_state("/api/apps"));
        assert!(!carries_session_state("/api/health"));
    }

    #[test]
    fn sse_events_are_scrubbed_frame_by_frame() {
        let event = serde_json::json!({
            "id": "e1",
            "actions": { "stateDelta": {
                CREDENTIALS_STATE_KEY: { "access_token": "ya29.secret", "refresh_token": "1//r" },
                USER_CREDENTIALS_STATE_KEY: { "access_token": "ya29.user-secret" },
                "other": 1
            } }
        });
        let frame = format!("event: message\ndata: {event}\n\n");
        let mut scrubber = SseScrubber::default();
        // Fed in two pieces: nothing is emitted before the frame is complete.
        let (head, tail) = frame.split_at(frame.len() / 2);
        let first = scrubber.push(head.as_bytes()).unwrap();
        assert!(first.is_empty());
        let out = scrubber.push(tail.as_bytes()).unwrap();
        let text = String::from_utf8(out.to_vec()).unwrap();
        assert!(text.starts_with("event: message\ndata: "), "{text}");
        assert!(text.ends_with("\n\n"));
        assert!(!text.contains("ya29.secret"));
        assert!(!text.contains("ya29.user-secret"));
        assert!(!text.contains(CREDENTIALS_STATE_KEY));
        assert!(!text.contains(USER_CREDENTIALS_STATE_KEY));
        assert!(text.contains("\"other\":1"));

        // Keep-alive comments and non-JSON data pass through; a trailing
        // unterminated frame is still scrubbed on finish.
        let out = scrubber
            .push(b": keep-alive\n\ndata: plain text\n\n")
            .unwrap();
        assert_eq!(out.as_ref(), b": keep-alive\n\ndata: plain text\n\n");
        // A frame terminator split across chunks is still detected.
        assert!(scrubber.push(b"data: {\"a\":1}\n").unwrap().is_empty());
        assert_eq!(
            scrubber.push(b"\n").unwrap().as_ref(),
            b"data: {\"a\":1}\n\n"
        );
        let last = format!("data: {event}");
        assert!(scrubber.push(last.as_bytes()).unwrap().is_empty());
        let flushed = String::from_utf8(scrubber.finish().to_vec()).unwrap();
        assert!(!flushed.contains("ya29.secret"));
        assert!(flushed.starts_with("data: {"));
    }

    #[tokio::test]
    async fn api_responses_with_session_state_are_scrubbed_including_sse() {
        use tower::ServiceExt as _;

        let credentials = test_credentials(true);
        let cookie = signed_cookie(&credentials, "nate@example.com");
        let event = serde_json::json!({
            "id": "e1",
            "actions": { "stateDelta": {
                CREDENTIALS_STATE_KEY: { "access_token": "ya29.secret" },
                USER_CREDENTIALS_STATE_KEY: { "access_token": "ya29.secret-user" }
            } }
        });
        let sse_body = format!("data: {event}\n\ndata: {event}\n\n");
        let api_app = Router::new()
            .route(
                "/api/run_sse",
                post(move || {
                    let body = sse_body.clone();
                    async move {
                        (
                            [(header::CONTENT_TYPE, "text/event-stream")],
                            Body::from(body),
                        )
                    }
                }),
            )
            .route(
                "/api/sessions/{app}/{user}/{session}",
                get(move || {
                    let event = event.clone();
                    async move { Json(serde_json::json!({ "events": [event] })) }
                }),
            )
            .layer(middleware::from_fn_with_state(
                (credentials.clone(), AdminAllowList::default()),
                guard_adk_routes,
            ));

        let sse = api_app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/run_sse")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(sse.status(), StatusCode::OK);
        let text = String::from_utf8(
            to_bytes(sse.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert_eq!(text.matches("data: ").count(), 2, "{text}");
        assert!(!text.contains("ya29.secret"), "{text}");

        let session = api_app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/sessions/adspace_agent/nate%40example.com/s1")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(session.status(), StatusCode::OK);
        let json: serde_json::Value =
            serde_json::from_slice(&to_bytes(session.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert!(
            json["events"][0]["actions"]["stateDelta"]
                .get(CREDENTIALS_STATE_KEY)
                .is_none()
        );
        assert!(
            json["events"][0]["actions"]["stateDelta"]
                .get(USER_CREDENTIALS_STATE_KEY)
                .is_none()
        );
        assert!(!json.to_string().contains("ya29."), "{json}");

        // Path-scoped routes are refused for another user's id.
        let foreign = api_app
            .oneshot(
                Request::builder()
                    .uri("/api/sessions/adspace_agent/victim%40example.com/s1")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(foreign.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn dev_ui_is_hidden_once_sign_in_is_configured() {
        use tower::ServiceExt as _;

        let ui = || {
            Router::new()
                .route("/ui/", get(|| async { Html("<html></html>") }))
                .route("/ui/assets/x.js", get(|| async { "js" }))
        };
        let credentials = test_credentials(true);
        let admins = AdminAllowList::new(["admin@example.com".to_string()]);
        let configured = ui().layer(middleware::from_fn_with_state(
            (credentials.clone(), admins),
            guard_adk_routes,
        ));
        let resp = configured
            .clone()
            .oneshot(Request::builder().uri("/ui/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        assert_eq!(resp.headers()[header::LOCATION], "/app/");
        let resp = configured
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/ui/assets/x.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        // Administrators (ADMIN_EMAILS) keep access to the dev UI and its assets.
        let cookie_for = |email: &str| {
            let user = AuthenticatedUser {
                id: "u-1".into(),
                email: email.into(),
                name: "Someone".into(),
                picture: None,
                sub: Some("1".into()),
                iat: chrono::Utc::now().timestamp(),
            };
            format!("{USER_COOKIE_NAME}={}", credentials.sign_user_cookie(&user))
        };
        for (email, status) in [
            ("admin@example.com", StatusCode::OK),
            ("user@example.com", StatusCode::SEE_OTHER),
        ] {
            let resp = configured
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/ui/")
                        .header(header::COOKIE, cookie_for(email))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(resp.status(), status, "{email}");
        }
        let resp = configured
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/ui/assets/x.js")
                    .header(header::COOKIE, cookie_for("admin@example.com"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        let open = ui().layer(middleware::from_fn_with_state(
            (test_credentials(false), AdminAllowList::default()),
            guard_adk_routes,
        ));
        let resp = open
            .oneshot(Request::builder().uri("/ui/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn cookie_identity_extractor_follows_the_sign_in_configuration() {
        let configured = test_credentials(true);
        let extractor = CookieIdentityExtractor::new(configured.clone());
        let anonymous = Request::builder().body(()).unwrap().into_parts().0;
        assert!(matches!(
            extractor.extract(&anonymous).await,
            Err(RequestContextError::MissingAuth)
        ));
        let signed = Request::builder()
            .header(
                header::COOKIE,
                signed_cookie(&configured, "nate@example.com"),
            )
            .body(())
            .unwrap()
            .into_parts()
            .0;
        let context = extractor.extract(&signed).await.unwrap();
        assert_eq!(context.user_id, "nate@example.com");
        assert_eq!(context.metadata["email"], "nate@example.com");

        // Without an OAuth client there is no sign-in: everyone is the default user.
        let extractor = CookieIdentityExtractor::new(test_credentials(false));
        let context = extractor.extract(&anonymous).await.unwrap();
        assert_eq!(context.user_id, DEFAULT_USER_ID);
    }

    #[tokio::test]
    async fn login_sets_a_nonce_cookie_and_the_callback_requires_it() {
        use adk_rust::session::InMemorySessionService;
        use tower::ServiceExt as _;

        let credentials = test_credentials(true);
        let app = oauth_router(OAuthRoutes {
            credentials: credentials.clone(),
            sessions: Arc::new(InMemorySessionService::new()),
            resume: None,
            admins: AdminAllowList::default(),
        });
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/auth/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::SEE_OTHER);
        let location = resp.headers()[header::LOCATION]
            .to_str()
            .unwrap()
            .to_string();
        let set_cookie = resp.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .to_string();
        assert!(
            set_cookie.starts_with(&format!("{LOGIN_NONCE_COOKIE_NAME}=")),
            "{set_cookie}"
        );
        assert!(
            set_cookie.contains("; Secure"),
            "https base URL: {set_cookie}"
        );
        assert!(set_cookie.contains(&format!("Path={OAUTH_CALLBACK_PATH}")));
        let state = url::Url::parse(&location)
            .unwrap()
            .query_pairs()
            .find(|(k, _)| k == "state")
            .map(|(_, v)| v.to_string())
            .unwrap();
        let nonce = credentials.login_state_nonce(&state).unwrap();
        assert!(set_cookie.starts_with(&format!("{LOGIN_NONCE_COOKIE_NAME}={nonce};")));

        // The callback refuses a sign-in state from another browser (no / wrong
        // nonce cookie) before it ever contacts Google.
        for cookie in [None, Some(format!("{LOGIN_NONCE_COOKIE_NAME}=other"))] {
            let mut req = Request::builder().uri(format!("/oauth2/callback?code=c&state={state}"));
            if let Some(cookie) = cookie {
                req = req.header(header::COOKIE, cookie);
            }
            let resp = app
                .clone()
                .oneshot(req.body(Body::empty()).unwrap())
                .await
                .unwrap();
            let html = String::from_utf8(
                to_bytes(resp.into_body(), usize::MAX)
                    .await
                    .unwrap()
                    .to_vec(),
            )
            .unwrap();
            assert!(html.contains("not started in this browser"), "{html}");
        }

        // Logging out clears the (Secure) user cookie.
        let resp = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/auth/logout")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let cleared = resp.headers()[header::SET_COOKIE].to_str().unwrap();
        assert!(cleared.starts_with(&format!("{USER_COOKIE_NAME}=;")));
        assert!(cleared.contains("Max-Age=0") && cleared.contains("; Secure"));
    }

    #[test]
    fn admin_allow_list_is_case_insensitive_and_ignores_blanks() {
        let admins = AdminAllowList::new([" Admin@Example.com ".to_string(), String::new()]);
        assert!(!admins.is_empty());
        assert!(admins.allows("admin@example.com"));
        assert!(admins.allows("ADMIN@example.com"));
        assert!(!admins.allows("other@example.com"));
        assert!(AdminAllowList::new(Vec::<String>::new()).is_empty());
    }

    #[test]
    fn sse_scrubber_catches_unicode_escaped_credential_keys_and_fast_paths_clean_frames() {
        let mut scrubber = SseScrubber::default();
        // Clean frame preserves exact whitespace formatting via SIMD fast path.
        let clean = b"data: { \"hello\" : \"world\" }\n\n";
        let out = scrubber.push(clean).unwrap();
        assert_eq!(out.as_ref(), clean);

        // Unicode-escaped `"CREDENTIALS_CACHE_KEY"` (`\u0043...`) still triggers
        // full JSON parsing and removes the secret.
        let escaped =
            b"data: {\"state\":{\"\\u0043REDENTIALS_CACHE_KEY\":\"ya29.escaped\",\"ok\":1}}\n\n";
        let scrubbed = String::from_utf8(scrubber.push(escaped).unwrap().to_vec()).unwrap();
        assert!(!scrubbed.contains("ya29.escaped"), "{scrubbed}");
        assert!(scrubbed.contains("\"ok\":1"), "{scrubbed}");
    }

    #[tokio::test]
    async fn studio_static_assets_emit_etag_and_honor_if_none_match() {
        use tower::ServiceExt as _;

        let app = oauth_router(OAuthRoutes {
            credentials: test_credentials(false),
            sessions: Arc::new(adk_rust::session::InMemorySessionService::new()),
            resume: None,
            admins: AdminAllowList::default(),
        });
        let first = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/app/app.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(first.status(), StatusCode::OK);
        let etag = first.headers()[header::ETAG].to_str().unwrap().to_string();
        assert!(etag.starts_with("W/\"") && etag.ends_with('"'));

        let cached = app
            .oneshot(
                Request::builder()
                    .uri("/app/app.js")
                    .header(header::IF_NONE_MATCH, &etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cached.status(), StatusCode::NOT_MODIFIED);
        let body = to_bytes(cached.into_body(), usize::MAX).await.unwrap();
        assert!(body.is_empty());
    }

    #[tokio::test]
    async fn agent_engine_dispatch_registers_operations_persists_temp_state_and_scrubs_credentials()
    {
        use adk_rust::artifact::InMemoryArtifactService;
        use adk_rust::session::InMemorySessionService;
        use tower::ServiceExt as _;

        struct EchoAgent;
        #[async_trait]
        impl adk_rust::Agent for EchoAgent {
            fn name(&self) -> &str {
                APP_NAME
            }
            fn description(&self) -> &str {
                "echo"
            }
            fn sub_agents(&self) -> &[Arc<dyn adk_rust::Agent>] {
                &[]
            }
            async fn run(
                &self,
                ctx: Arc<dyn adk_rust::InvocationContext>,
            ) -> adk_rust::Result<adk_rust::EventStream> {
                if let Some(artifacts) = ctx.artifacts() {
                    let _ = artifacts
                        .save(
                            "report.csv",
                            &adk_rust::Part::Text {
                                text: "col1,col2\n1,2\n".to_string(),
                            },
                        )
                        .await;
                }
                let mut ev = adk_rust::Event::new(ctx.invocation_id());
                ev.author = APP_NAME.to_string();
                ev.actions.state_delta.insert(
                    USER_CREDENTIALS_STATE_KEY.to_string(),
                    serde_json::json!({"access_token": "ya29.stream-secret"}),
                );
                ev.actions
                    .artifact_delta
                    .insert("report.csv".to_string(), 1);
                ev.llm_response.content = Some(
                    adk_rust::Content::new("model")
                        .with_thinking("thinking step")
                        .with_text("ack"),
                );
                ev.llm_response.finish_reason = Some(adk_rust::FinishReason::Stop);
                ev.llm_response.turn_complete = true;
                Ok(Box::pin(futures::stream::iter(vec![Ok(ev)])))
            }
        }

        let sessions: Arc<dyn SessionService> = Arc::new(InMemorySessionService::new());
        let artifacts: Arc<dyn ArtifactService> = Arc::new(InMemoryArtifactService::new());
        let runner = Arc::new(
            adk_rust::runner::Runner::builder()
                .app_name(APP_NAME)
                .agent(Arc::new(EchoAgent))
                .session_service(sessions.clone())
                .artifact_service(artifacts.clone())
                .build()
                .unwrap(),
        );
        let state = adk_rust::server::AgentEngineState::new(runner)
            .with_artifact_service(artifacts.clone());
        let ae_router = adk_rust::server::agent_engine_router(state)
            .layer(middleware::from_fn_with_state(
                (sessions.clone(), artifacts.clone()),
                normalize_agent_engine_request,
            ))
            .layer(middleware::from_fn_with_state(
                (test_credentials(true), AdminAllowList::default()),
                guard_adk_routes,
            ));

        // 1. register_operations succeeds without browser cookies or Content-Type header,
        //    and accepts both class_method and classMethod envelope keys.
        let reg_resp = ae_router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/reasoning_engine")
                    .body(Body::from(r#"{"classMethod":"register_operations"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(reg_resp.status(), StatusCode::OK);
        let reg_json: serde_json::Value =
            serde_json::from_slice(&to_bytes(reg_resp.into_body(), usize::MAX).await.unwrap())
                .unwrap();
        assert!(
            reg_json["output"]["async_stream"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!("async_stream_query"))
        );

        // 2. create_session preserves temp:<auth_id> as <auth_id> in the store while scrubbing credentials from the HTTP response.
        let create_resp = ae_router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/reasoning_engine")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(
                        serde_json::json!({
                            "class_method": "create_session",
                            "input": {
                                "user_id": "ge-user",
                                "session_id": "ge-s1",
                                "state": {
                                    "temp:adspace-oauth": "ya29.ge-delegated",
                                    USER_CREDENTIALS_STATE_KEY: {"access_token": "ya29.stored"}
                                }
                            }
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(create_resp.status(), StatusCode::OK);
        let create_body = String::from_utf8(
            to_bytes(create_resp.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(!create_body.contains("ya29.stored"), "{create_body}");

        let stored = sessions
            .get(adk_rust::session::GetRequest {
                app_name: APP_NAME.to_string(),
                user_id: "ge-user".to_string(),
                session_id: "ge-s1".to_string(),
                num_recent_events: None,
                after: None,
            })
            .await
            .unwrap();
        assert_eq!(
            stored.state().get("adspace-oauth"),
            Some(serde_json::json!("ya29.ge-delegated")),
            "temp:adspace-oauth is preserved as adspace-oauth across adk-session persistence"
        );

        // 3. async_stream_query applies state_delta and scrubs credentials from the NDJSON stream.
        let stream_resp = ae_router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/stream_reasoning_engine")
                    .header(header::ACCEPT_ENCODING, "gzip")
                    .body(Body::from(
                        serde_json::json!({
                            "class_method": "async_stream_query",
                            "input": {
                                "user_id": "ge-user",
                                "session_id": "ge-s1",
                                "message": "hello",
                                "state_delta": {
                                    "temp:adspace-oauth": "ya29.updated-delegated"
                                }
                            }
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stream_resp.status(), StatusCode::OK);
        let ndjson = String::from_utf8(
            to_bytes(stream_resp.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(ndjson.contains("\"ack\""), "{ndjson}");
        assert!(!ndjson.contains("ya29.stream-secret"), "{ndjson}");

        let updated = sessions
            .get(adk_rust::session::GetRequest {
                app_name: APP_NAME.to_string(),
                user_id: "ge-user".to_string(),
                session_id: "ge-s1".to_string(),
                num_recent_events: None,
                after: None,
            })
            .await
            .unwrap();
        assert_eq!(
            updated.state().get("adspace-oauth"),
            Some(serde_json::json!("ya29.updated-delegated"))
        );

        // 4. streaming_agent_run_with_events accepts Gemini Enterprise's _StreamRunRequest
        //    (message, authorizations, session_id) and returns _StreamingRunResponse lines
        //    with Python-ADK Event formatting and artifacts.
        let ge_req_json = serde_json::to_string_pretty(&serde_json::json!({
            "message": {
                "role": "user",
                "parts": [{"text": "run report"}]
            },
            "session_id": "12828825004904213795",
            "authorizations": {
                "projects/123/locations/global/authorizations/adspace_agent": {
                    "access_token": "ya29.ge-auth-token"
                }
            }
        }))
        .unwrap();
        let ge_resp = ae_router
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/stream_reasoning_engine")
                    .body(Body::from(
                        serde_json::json!({
                            "class_method": "streaming_agent_run_with_events",
                            "input": {
                                "request_json": ge_req_json
                            }
                        })
                        .to_string(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(ge_resp.status(), StatusCode::OK);
        let ge_ndjson = String::from_utf8(
            to_bytes(ge_resp.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(!ge_ndjson.contains("ya29.stream-secret"), "{ge_ndjson}");
        assert!(!ge_ndjson.contains("ya29.ge-auth-token"), "{ge_ndjson}");
        let line_val: serde_json::Value =
            serde_json::from_str(ge_ndjson.lines().next().unwrap()).unwrap();
        assert_eq!(line_val["session_id"], "12828825004904213795");
        let ev0 = &line_val["events"][0];
        assert!(ev0["timestamp"].is_f64(), "{ev0}");
        assert_eq!(ev0["finish_reason"], "STOP");
        assert_eq!(ev0["content"]["parts"][0]["thought"], true);
        assert_eq!(ev0["content"]["parts"][0]["text"], "thinking step");
        assert_eq!(ev0["content"]["parts"][1]["text"], "ack");
        assert_eq!(line_val["artifacts"][0]["file_name"], "report.csv");

        let ge_session = sessions
            .get(adk_rust::session::GetRequest {
                app_name: APP_NAME.to_string(),
                user_id: "default_user".to_string(),
                session_id: "12828825004904213795".to_string(),
                num_recent_events: None,
                after: None,
            })
            .await
            .unwrap();
        assert_eq!(
            ge_session.state().get("adspace_agent"),
            Some(serde_json::json!("ya29.ge-auth-token"))
        );
    }
}
