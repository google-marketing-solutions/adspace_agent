//! HTTP server (ADK web UI + REST API + OAuth callback) and console REPL.
//!
//! Both modes are built on adk-rust's [`Launcher`] (from `adk-cli`): the
//! console mode is its readline REPL, and the server is its Axum application
//! with our OAuth redirect route merged in.

use std::sync::Arc;

use adk_rust::server::shutdown_signal;
use adk_rust::{CacheCapable, Launcher};
use axum::Router;
use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;

use crate::agent::AgentRuntime;
use crate::auth::UserCredentialManager;
use crate::auth::oauth::OAUTH_CALLBACK_PATH;
use crate::config::APP_NAME;

/// Query parameters Google sends to the OAuth redirect URI.
#[derive(Debug, serde::Deserialize)]
pub struct OAuthCallbackQuery {
    /// Authorization code.
    pub code: Option<String>,
    /// Opaque state we generated when starting the flow.
    pub state: Option<String>,
    /// Error code when the user denied access.
    pub error: Option<String>,
}

fn html_page(title: &str, body: &str) -> Html<String> {
    Html(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{title}</title>\
         <style>body{{font-family:system-ui,sans-serif;max-width:40rem;margin:4rem auto;padding:0 1rem;color:#202124}}\
         h1{{font-size:1.4rem}}code{{background:#f1f3f4;padding:.1rem .3rem;border-radius:4px}}</style></head>\
         <body><h1>{title}</h1>{body}</body></html>"
    ))
}

/// Handles the Google OAuth redirect.
pub async fn oauth_callback(
    State(credentials): State<Arc<UserCredentialManager>>,
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
    match credentials.complete_authorization(&state, &code).await {
        Ok(user) => html_page(
            "Authorization complete",
            &format!(
                "<p>Google access was granted for user <code>{user}</code>. You can close this window, return to the chat and tell the agent to continue.</p>"
            ),
        ),
        Err(e) => html_page(
            "Authorization failed",
            &format!(
                "<p>{e}</p><p>Return to the chat and ask the agent to start the authorization again.</p>"
            ),
        ),
    }
}

/// Builds the router for the OAuth endpoints.
pub fn oauth_router(credentials: Arc<UserCredentialManager>) -> Router {
    Router::new()
        .route(OAUTH_CALLBACK_PATH, get(oauth_callback))
        .with_state(credentials)
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
        .merge(oauth_router(runtime.credentials.clone()));
    let listener = tokio::net::TcpListener::bind((host, port)).await?;
    let redirect = runtime.credentials.flow().config().redirect_uri.clone();
    tracing::info!(
        "AdSpace Agent listening on http://{host}:{port}  (web UI: http://{host}:{port}/ui/)"
    );
    tracing::info!(
        "OAuth redirect URI: {redirect} — register it on the OAuth client in Google Cloud Console"
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Runs the interactive console REPL (adk-cli's launcher: readline history,
/// streaming output, think-block rendering). The session belongs to user
/// [`DEFAULT_USER_ID`]; adk-cli does not expose a user-id setting.
///
/// # Errors
///
/// Returns an error when the REPL cannot start or a turn fails.
pub async fn console(runtime: AgentRuntime) -> anyhow::Result<()> {
    launcher(&runtime).run_console_directly().await?;
    Ok(())
}
