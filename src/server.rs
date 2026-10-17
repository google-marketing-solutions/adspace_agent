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

use adk_rust::server::shutdown_signal;
use adk_rust::server::{RequestContext, RequestContextError, RequestContextExtractor};
use adk_rust::session::SessionService;
use adk_rust::{CacheCapable, Launcher};
use async_trait::async_trait;
use axum::body::{Body, Bytes, to_bytes};
use axum::extract::{Query, Request, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{AppendHeaders, Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
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
/// The helper script (see the file for what it does in the UI).
const AUTH_HELPER_JS: &str = include_str!("auth_helper.js");
/// Tag injected into the web UI page.
const AUTH_HELPER_TAG: &str = "<script src=\"/oauth2/helper.js\" defer></script>";
/// Custom AdSpace Agent studio interface HTML (`/app/`).
const STUDIO_HTML: &str = include_str!("studio.html");
/// Custom AdSpace Agent studio interface CSS (`/app/app.css`).
const STUDIO_CSS: &str = include_str!("studio.css");
/// Custom AdSpace Agent studio interface JS (`/app/app.js`).
const STUDIO_JS: &str = include_str!("studio.js");
/// Custom AdSpace Agent Interactive Canvas WebGL stage shader (`/app/stage.js`).
const STUDIO_STAGE_JS: &str = include_str!("stage.js");
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

/// Serves the browser helper.
async fn auth_helper() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/javascript; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
        ],
        AUTH_HELPER_JS,
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
    );
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
async fn studio_index() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
        ],
        STUDIO_HTML,
    )
        .into_response()
}

/// Serves the custom AdSpace Agent studio stylesheet (`/app/app.css`).
async fn studio_css() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/css; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
        ],
        STUDIO_CSS,
    )
        .into_response()
}

/// Serves the custom AdSpace Agent studio client script (`/app/app.js`).
async fn studio_js() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/javascript; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
        ],
        STUDIO_JS,
    )
        .into_response()
}

/// Serves the Interactive Canvas WebGL stage effect (`/app/stage.js`).
async fn studio_stage_js() -> Response {
    (
        [
            (
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/javascript; charset=utf-8"),
            ),
            (header::CACHE_CONTROL, HeaderValue::from_static("no-cache")),
        ],
        STUDIO_STAGE_JS,
    )
        .into_response()
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
}

/// State keys whose values are OAuth tokens and must never reach a browser.
const CREDENTIAL_KEYS: [&str; 2] = [USER_CREDENTIALS_STATE_KEY, CREDENTIALS_STATE_KEY];

fn scrub_credentials_from_json(val: &mut serde_json::Value) {
    match val {
        serde_json::Value::Object(map) => {
            for key in CREDENTIAL_KEYS {
                map.remove(key);
            }
            for v in map.values_mut() {
                scrub_credentials_from_json(v);
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr.iter_mut() {
                scrub_credentials_from_json(item);
            }
        }
        _ => {}
    }
}

/// Removes [`CREDENTIALS_STATE_KEY`] from the `data:` payload of one complete
/// SSE event (text up to and including its terminating blank line).
///
/// Non-`data:` lines (`event:`, `id:`, `retry:`, comments) are kept; the data
/// lines are joined as the SSE spec prescribes, parsed as JSON, scrubbed and
/// re-emitted as a single `data:` line. Data that is not JSON passes through.
fn scrub_sse_event(frame: &[u8]) -> Vec<u8> {
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
        while let Some(at) = self.buf[search_from..]
            .windows(2)
            .position(|w| w == b"\n\n")
        {
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
    );
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
    State(credentials): State<Arc<SessionCredentialManager>>,
    req: Request,
    next: Next,
) -> Response {
    let path = req.uri().path().to_string();
    let oauth_configured = credentials.flow().config().is_configured();
    if oauth_configured && (path == "/ui" || path.starts_with("/ui/")) {
        return if path == "/ui" || path == "/ui/" {
            Redirect::to("/app/").into_response()
        } else {
            StatusCode::NOT_FOUND.into_response()
        };
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
    if content_type.starts_with("application/json") {
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
        .route("/app/app.js", get(studio_js))
        .route("/app/stage.js", get(studio_stage_js))
        .route("/favicon.ico", get(studio_favicon))
        .route("/favicon.svg", get(studio_favicon))
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
    Launcher::new(runtime.agent.clone())
        .app_name(APP_NAME)
        .with_session_service(runtime.session_service.clone())
        .with_artifact_service(runtime.artifact_service.clone())
        .with_memory_service(runtime.memory_for(DEFAULT_USER_ID))
        .with_compaction(runtime.compaction_config())
        .with_context_cache(
            runtime.context_cache_config(),
            runtime.models.clone() as Arc<dyn CacheCapable>,
        )
}

/// Assembles the HTTP application: adk-server's routes (authenticated through
/// [`CookieIdentityExtractor`]), the OAuth / studio routes, the `/ui/` helper
/// injection and [`guard_adk_routes`].
///
/// # Errors
///
/// Returns an error when the adk-server application cannot be built.
pub fn build_app(launcher: Launcher, routes: OAuthRoutes) -> anyhow::Result<Router> {
    let credentials = routes.credentials.clone();
    let app = launcher
        .with_request_context_extractor(Arc::new(CookieIdentityExtractor::new(credentials.clone())))
        .build_app()?
        .merge(oauth_router(routes))
        .layer(middleware::from_fn(inject_auth_helper))
        .layer(middleware::from_fn_with_state(
            credentials,
            guard_adk_routes,
        ));
    Ok(app)
}

/// How long in-flight requests may finish after SIGTERM (`SHUTDOWN_GRACE_SECS`).
fn shutdown_grace_period() -> Duration {
    std::env::var("SHUTDOWN_GRACE_SECS")
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(DEFAULT_SHUTDOWN_GRACE, Duration::from_secs)
}

/// Serves the agent (web UI at `/ui/`, REST API under `/api`, OAuth callback).
///
/// # Errors
///
/// Returns an error when the listener cannot be bound or the server fails.
pub async fn serve(runtime: AgentRuntime, host: &str, port: u16) -> anyhow::Result<()> {
    runtime
        .credentials
        .enable_interactive_wait(Duration::from_secs(300), true);
    runtime
        .credentials
        .attach_session_service(runtime.session_service.clone());
    let app = build_app(launcher(&runtime), runtime.oauth_routes(true))?;
    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    if runtime.credentials.flow().config().is_configured() {
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
        // The tag loads the helper from our own origin (the UI's CSP is `script-src 'self'`).
        assert!(AUTH_HELPER_TAG.contains(AUTH_HELPER_PATH));
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
        assert!(AUTH_HELPER_JS.contains("https://accounts.google.com/o/oauth2/"));
        assert!(AUTH_HELPER_JS.contains("window.open("));
        assert!(AUTH_HELPER_JS.contains("new BroadcastChannel(CHANNEL)"));
        assert!(AUTH_HELPER_JS.contains("const CHANNEL = \"adspace:oauth\""));
        assert!(AUTH_HELPER_JS.contains("adk-ui-surface"));
        assert!(AUTH_HELPER_JS.contains("extractUiResponse"));
        assert!(AUTH_HELPER_JS.contains("Element.prototype.scrollTo"));
        // Must match the text `resume_message` produces.
        let marker = "Google access has been granted for this chat";
        assert!(AUTH_HELPER_JS.contains(marker));
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

        // 1b. `/favicon.ico` and `/favicon.svg` return 200 OK with the SVG icon.
        for path in ["/favicon.ico", "/favicon.svg"] {
            let fav = app
                .clone()
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(fav.status(), StatusCode::OK);
        }

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
                credentials.clone(),
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
                credentials.clone(),
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
        let configured = ui().layer(middleware::from_fn_with_state(
            test_credentials(true),
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
            .oneshot(
                Request::builder()
                    .uri("/ui/assets/x.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);

        let open = ui().layer(middleware::from_fn_with_state(
            test_credentials(false),
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
}
