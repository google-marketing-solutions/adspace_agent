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

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use adk_rust::server::shutdown_signal;
use adk_rust::session::SessionService;
use adk_rust::{CacheCapable, Launcher};
use axum::body::{Body, to_bytes};
use axum::extract::{Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures::stream::Stream;

use crate::agent::{AgentRuntime, TurnRunner};
use crate::auth::oauth::OAUTH_CALLBACK_PATH;
use crate::auth::{
    AuthenticatedUser, LOGIN_SENTINEL_APP, OAuthToken, SessionCredentialManager, SessionRef,
};
use crate::config::APP_NAME;

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

/// Body of `POST /oauth2/ack`.
#[derive(Debug, serde::Deserialize)]
pub struct OAuthAckRequest {
    /// Signed OAuth state parameter from the `auth_required` event.
    pub state: String,
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

fn html_page(title: &str, body: &str, status: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
         <style>body{{font-family:system-ui,sans-serif;max-width:40rem;margin:4rem auto;padding:0 1rem;color:#202124}}\
         h1{{font-size:1.4rem}}code{{background:#f1f3f4;padding:.1rem .3rem;border-radius:4px}}</style></head>\
         <body><h1>{title}</h1>{body}<script>{}</script></body></html>",
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
async fn oauth_events(
    State(routes): State<OAuthRoutes>,
) -> Sse<impl Stream<Item = Result<SseEvent, Infallible>>> {
    let rx = routes.credentials.subscribe_events();
    let stream = futures::stream::unfold(rx, |mut rx| async move {
        loop {
            match rx.recv().await {
                Ok(event) => {
                    if let Ok(json) = serde_json::to_string(&event) {
                        return Some((Ok(SseEvent::default().data(json)), rx));
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

/// Records whether `/ui/` succeeded in opening the OAuth popup via `window.open`.
async fn oauth_ack(
    State(routes): State<OAuthRoutes>,
    Json(req): Json<OAuthAckRequest>,
) -> Json<serde_json::Value> {
    let fallback_opened = routes.credentials.ack_popup(&req.state, req.opened);
    Json(serde_json::json!({ "fallback_opened": fallback_opened }))
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

/// Starts a Google Sign-In flow (`openid email profile`) and redirects to Google.
async fn auth_login(State(routes): State<OAuthRoutes>) -> Response {
    match routes.credentials.begin_user_login() {
        Ok(url) => Redirect::to(&url).into_response(),
        Err(e) => html_page(
            "Sign-In Unavailable",
            &format!("<p>{e}</p><p><a href=\"/app/\">Return to AdSpace Agent</a></p>"),
            "error",
        )
        .into_response(),
    }
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

/// Returns the currently signed-in Google user for `/app/`.
async fn auth_me(State(routes): State<OAuthRoutes>, headers: HeaderMap) -> Json<serde_json::Value> {
    let oauth_configured = routes.credentials.flow().config().is_configured();
    match extract_cookie_user(&headers, &routes.credentials) {
        Some(user) => Json(serde_json::json!({
            "authenticated": true,
            "oauth_configured": oauth_configured,
            "user": user,
        })),
        None => Json(serde_json::json!({
            "authenticated": false,
            "oauth_configured": oauth_configured,
        })),
    }
}

/// Logs out the current user by clearing the `adspace_user` cookie.
async fn auth_logout() -> Response {
    let clear = format!("{USER_COOKIE_NAME}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    (
        [(
            header::SET_COOKIE,
            HeaderValue::from_str(&clear).expect("valid cookie header"),
        )],
        Json(serde_json::json!({ "ok": true })),
    )
        .into_response()
}

/// Handles the Google OAuth redirect: exchanges the code and either signs in
/// the user (when `session.app_name == LOGIN_SENTINEL_APP`) or stores the
/// tokens in the chat session that started the authorization.
pub async fn oauth_callback(
    State(routes): State<OAuthRoutes>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Response {
    if let Some(error) = query.error {
        if let Some(state) = query.state.as_deref() {
            routes.credentials.fail_authorization(state, &error);
        }
        return html_page(
            "Authorization failed",
            &format!(
                "<p>Google reported: <code>{error}</code>. Return to the chat and try again.</p>"
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
    let (session, token) = match routes
        .credentials
        .complete_authorization(&state, &code)
        .await
    {
        Ok(result) => result,
        Err(e) => {
            return html_page(
                "Authorization failed",
                &format!(
                    "<p>{e}</p><p>Return to the chat and ask the agent to start the authorization again.</p>"
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
                let cookie = format!(
                    "{USER_COOKIE_NAME}={signed}; Path=/; HttpOnly; SameSite=Lax; Max-Age=604800"
                );
                let page = html_page(
                    "Signed in to AdSpace Agent",
                    &format!(
                        "<p>Signed in as <strong>{}</strong> (<code>{}</code>). Returning to AdSpace Agent…</p>",
                        user.name, user.email
                    ),
                    "login_ok",
                );
                return (
                    [(
                        header::SET_COOKIE,
                        HeaderValue::from_str(&cookie).expect("valid cookie header"),
                    )],
                    page,
                )
                    .into_response();
            }
            Err(e) => {
                tracing::error!("Failed to fetch Google userinfo during login: {e:#}");
                return html_page(
                    "Sign-in failed",
                    &format!(
                        "<p>Could not retrieve your Google profile: {e}</p><p><a href=\"/app/\">Try again</a></p>"
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
        .route("/app", get(|| async { Redirect::to("/app/") }))
        .route("/app/", get(studio_index))
        .route("/app/app.css", get(studio_css))
        .route("/app/app.js", get(studio_js))
        .with_state(routes)
}

impl AgentRuntime {
    fn oauth_routes(&self, resume: bool) -> OAuthRoutes {
        OAuthRoutes {
            credentials: self.credentials.clone(),
            sessions: self.session_service.clone(),
            resume: resume.then(|| Arc::new(self.turn_runner())),
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
            runtime.model.clone() as Arc<dyn CacheCapable>,
        )
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
    let app = launcher(&runtime)
        .build_app()?
        .merge(oauth_router(runtime.oauth_routes(true)))
        .layer(middleware::from_fn(inject_auth_helper));
    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    tracing::info!(
        "AdSpace Agent listening on http://{host}:{port}  (studio: http://{host}:{port}/app/  ·  dev UI: http://{host}:{port}/ui/)"
    );
    runtime.log_redirect_uri();
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
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
    }
}
