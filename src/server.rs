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

use std::sync::Arc;

use adk_rust::server::shutdown_signal;
use adk_rust::session::SessionService;
use adk_rust::{CacheCapable, Launcher};
use axum::Router;
use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;

use crate::agent::{AgentRuntime, TurnRunner};
use crate::auth::oauth::OAUTH_CALLBACK_PATH;
use crate::auth::{OAuthToken, SessionCredentialManager, SessionRef};
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

fn html_page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
         <style>body{{font-family:system-ui,sans-serif;max-width:40rem;margin:4rem auto;padding:0 1rem;color:#202124}}\
         h1{{font-size:1.4rem}}code{{background:#f1f3f4;padding:.1rem .3rem;border-radius:4px}}</style></head>\
         <body><h1>{title}</h1>{body}</body></html>"
    ))
}

/// Handles the Google OAuth redirect: exchanges the code and stores the
/// tokens in the chat session that started the authorization.
pub async fn oauth_callback(
    State(routes): State<OAuthRoutes>,
    Query(query): Query<OAuthCallbackQuery>,
) -> Html<String> {
    if let Some(error) = query.error {
        return html_page(
            "Authorization failed",
            &format!(
                "<p>Google reported: <code>{error}</code>. Return to the chat and try again.</p>"
            ),
        );
    }
    let (Some(code), Some(state)) = (query.code, query.state) else {
        return html_page(
            "Authorization failed",
            "<p>Missing <code>code</code> or <code>state</code> parameter.</p>",
        );
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
            );
        }
    };
    match routes
        .credentials
        .persist(routes.sessions.as_ref(), &session, &token)
        .await
    {
        Ok(()) => match &routes.resume {
            Some(runner) => {
                spawn_resume(runner.clone(), session, &token);
                html_page(
                    "Authorization complete",
                    "<p>Google access was granted to this chat session and the agent is continuing where it left off. You can close this window and return to the chat.</p>",
                )
            }
            None => html_page(
                "Authorization complete",
                "<p>Google access was granted to this chat session. You can close this window, return to the chat and tell the agent to continue.</p>",
            ),
        },
        Err(e) => {
            tracing::error!("{e:#}");
            html_page(
                "Authorization failed",
                "<p>The chat session that requested access could not be found. Return to the chat and ask the agent to start the authorization again.</p>",
            )
        }
    }
}

/// Builds the router for the OAuth endpoints.
pub fn oauth_router(routes: OAuthRoutes) -> Router {
    Router::new()
        .route(OAUTH_CALLBACK_PATH, get(oauth_callback))
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
    let app = launcher(&runtime)
        .build_app()?
        .merge(oauth_router(runtime.oauth_routes(true)));
    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    tracing::info!(
        "AdSpace Agent listening on http://{host}:{port}  (web UI: http://{host}:{port}/ui/)"
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
