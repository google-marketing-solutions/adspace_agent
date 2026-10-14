//! HTTP server (ADK web UI + REST API + OAuth callback) and console runner.

use std::collections::HashMap;
use std::sync::Arc;

use adk_rust::runner::Runner;
use adk_rust::server::{ServerBuilder, ServerConfig, shutdown_signal};
use adk_rust::session::CreateRequest;
use adk_rust::{CacheCapable, Content, Part, SessionId, SingleAgentLoader, UserId};
use axum::Router;
use axum::extract::{Query, State};
use axum::response::Html;
use axum::routing::get;
use futures::StreamExt;

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

/// Serves the agent (web UI at `/ui/`, REST API under `/api`, OAuth callback).
///
/// # Errors
///
/// Returns an error when the listener cannot be bound or the server fails.
pub async fn serve(runtime: AgentRuntime, host: &str, port: u16) -> anyhow::Result<()> {
    let loader = Arc::new(SingleAgentLoader::new(runtime.agent.clone()));
    let config = ServerConfig::new(loader, runtime.session_service.clone())
        .with_artifact_service(runtime.artifact_service.clone())
        .with_memory_service(runtime.memory_for("user"))
        .with_compaction(runtime.compaction_config())
        .with_context_cache(
            runtime.context_cache_config(),
            runtime.model.clone() as Arc<dyn CacheCapable>,
        );

    let app = ServerBuilder::new(config)
        .add_root_routes(oauth_router(runtime.credentials.clone()))
        .build();

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

/// Runs an interactive console session (stdin/stdout).
///
/// # Errors
///
/// Returns an error when the runner cannot be built or a turn fails.
pub async fn console(runtime: AgentRuntime, user_id: &str) -> anyhow::Result<()> {
    let runner = Runner::builder()
        .app_name(APP_NAME)
        .agent(runtime.agent.clone())
        .session_service(runtime.session_service.clone())
        .artifact_service(runtime.artifact_service.clone())
        .memory_service(runtime.memory_for(user_id))
        .compaction_config(runtime.compaction_config())
        .context_cache_config(runtime.context_cache_config())
        .cache_capable(runtime.model.clone() as Arc<dyn CacheCapable>)
        .build()?;

    let session = runtime
        .session_service
        .create(CreateRequest {
            app_name: APP_NAME.to_string(),
            user_id: user_id.to_string(),
            session_id: None,
            state: HashMap::new(),
        })
        .await?;
    let session_id = session.id().to_string();
    println!(
        "\n  AdSpace Agent — session {session_id}\n  Type a message to chat. Type 'exit' to quit.\n"
    );

    let stdin = std::io::stdin();
    loop {
        use std::io::Write as _;
        print!("You > ");
        std::io::stdout().flush().ok();
        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }
        let text = line.trim();
        if text.is_empty() {
            continue;
        }
        if matches!(text, "exit" | "quit" | "/exit" | "/quit") {
            break;
        }
        let mut stream = runner
            .run(
                UserId::new(user_id)?,
                SessionId::new(&session_id)?,
                Content::new("user").with_text(text),
            )
            .await?;
        print!("Agent > ");
        std::io::stdout().flush().ok();
        let mut printed = false;
        while let Some(event) = stream.next().await {
            match event {
                Ok(event) => {
                    if let Some(content) = event.content() {
                        for part in &content.parts {
                            match part {
                                Part::Text { text } if event.author != "user" => {
                                    if !event.llm_response.partial || !printed {
                                        print!("{text}");
                                        printed = true;
                                        std::io::stdout().flush().ok();
                                    }
                                }
                                Part::FunctionCall { name, .. } => println!("\n  [calling {name}]"),
                                _ => {}
                            }
                        }
                    }
                }
                Err(e) => println!("\n  [error] {e}"),
            }
        }
        println!("\n");
    }
    Ok(())
}
