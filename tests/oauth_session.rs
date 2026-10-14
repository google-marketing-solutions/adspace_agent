//! End-to-end test of the session-bound OAuth flow over real HTTP:
//!
//! 1. a tool context of session `s-1` asks for credentials and gets an
//!    `AUTH_REQUIRED` link whose `state` names that session;
//! 2. Google (mocked) redirects to `/oauth2/callback?code&state`;
//! 3. the handler exchanges the code and writes the tokens into session `s-1`;
//! 4. a later tool call of `s-1` finds a valid token in its state, while
//!    another session still has none.

use std::collections::HashMap;
use std::sync::Arc;

use adk_rust::session::{CreateRequest, GetRequest, InMemorySessionService, SessionService};
use adk_rust::tool::SimpleToolContext;
use adk_rust::{
    Artifacts, CallbackContext, Content, EventActions, MemoryEntry, ReadonlyContext, Session,
    State, ToolContext,
};
use adspace_agent::auth::{
    CREDENTIALS_STATE_KEY, GoogleOAuthFlow, OAuthClientConfig, OAuthToken,
    SessionCredentialManager, SessionRef, auth_required,
};
use adspace_agent::server::{OAuthRoutes, oauth_router};
use async_trait::async_trait;
use axum::Router;
use axum::routing::post;
use serde_json::{Value, json};

/// Minimal stand-in for Google's token endpoint.
async fn mock_token_endpoint(body: String) -> axum::Json<Value> {
    assert!(
        body.contains("grant_type=authorization_code"),
        "unexpected body: {body}"
    );
    assert!(body.contains("code=the-code"));
    axum::Json(json!({
        "access_token": "ya29.session-token",
        "refresh_token": "1//refresh",
        "expires_in": 3600,
        "scope": "https://www.googleapis.com/auth/adwords",
        "token_type": "Bearer",
    }))
}

async fn spawn(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, router).into_future());
    format!("http://{addr}")
}

/// Tool context backed by the session's persisted state, shaped like the
/// runner's `AgentToolContext`: the session is reachable through
/// `ToolContext::session()` and `ReadonlyContext::state()` is `None`.
struct SessionToolContext {
    session: SessionRef,
    state: HashMap<String, Value>,
    actions: std::sync::Mutex<EventActions>,
    content: Content,
}

impl State for SessionToolContext {
    fn get(&self, key: &str) -> Option<Value> {
        self.state.get(key).cloned()
    }
    fn set(&mut self, key: String, value: Value) {
        self.state.insert(key, value);
    }
    fn all(&self) -> HashMap<String, Value> {
        self.state.clone()
    }
}

impl ReadonlyContext for SessionToolContext {
    fn invocation_id(&self) -> &str {
        "inv"
    }
    fn agent_name(&self) -> &str {
        "adspace_agent"
    }
    fn user_id(&self) -> &str {
        &self.session.user_id
    }
    fn app_name(&self) -> &str {
        &self.session.app_name
    }
    fn session_id(&self) -> &str {
        &self.session.session_id
    }
    fn branch(&self) -> &str {
        ""
    }
    fn user_content(&self) -> &Content {
        &self.content
    }
}

impl Session for SessionToolContext {
    fn id(&self) -> &str {
        &self.session.session_id
    }
    fn app_name(&self) -> &str {
        &self.session.app_name
    }
    fn user_id(&self) -> &str {
        &self.session.user_id
    }
    fn state(&self) -> &dyn State {
        self
    }
    fn conversation_history(&self) -> Vec<Content> {
        Vec::new()
    }
}

#[async_trait]
impl CallbackContext for SessionToolContext {
    fn artifacts(&self) -> Option<Arc<dyn Artifacts>> {
        None
    }
}

#[async_trait]
impl ToolContext for SessionToolContext {
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

async fn tool_context(sessions: &dyn SessionService, session: &SessionRef) -> SessionToolContext {
    let loaded = sessions
        .get(GetRequest {
            app_name: session.app_name.clone(),
            user_id: session.user_id.clone(),
            session_id: session.session_id.clone(),
            num_recent_events: None,
            after: None,
        })
        .await
        .unwrap();
    SessionToolContext {
        session: session.clone(),
        state: loaded.state().all(),
        actions: std::sync::Mutex::new(EventActions::default()),
        content: Content::new("user"),
    }
}

fn state_param(url: &str) -> String {
    url::Url::parse(url)
        .unwrap()
        .query_pairs()
        .find(|(k, _)| k == "state")
        .map(|(_, v)| v.to_string())
        .expect("auth url carries a state parameter")
}

#[tokio::test]
async fn oauth_callback_attaches_tokens_to_the_requesting_session() {
    let google = spawn(Router::new().route("/token", post(mock_token_endpoint))).await;
    let flow = GoogleOAuthFlow::new(OAuthClientConfig {
        client_id: "cid".into(),
        client_secret: "secret".into(),
        redirect_uri: "http://localhost:8000/oauth2/callback".into(),
    })
    .with_endpoints(format!("{google}/auth"), format!("{google}/token"));
    let credentials = Arc::new(SessionCredentialManager::new(flow));
    credentials
        .register_scopes(&["https://www.googleapis.com/auth/adwords".to_string()])
        .await;

    let sessions: Arc<dyn SessionService> = Arc::new(InMemorySessionService::new());
    for id in ["s-1", "s-2"] {
        sessions
            .create(CreateRequest {
                app_name: "adspace_agent".into(),
                user_id: "local-user".into(),
                session_id: Some(id.into()),
                state: HashMap::new(),
            })
            .await
            .unwrap();
    }
    let s1 = SessionRef {
        app_name: "adspace_agent".into(),
        user_id: "local-user".into(),
        session_id: "s-1".into(),
    };
    let s2 = SessionRef {
        session_id: "s-2".into(),
        ..s1.clone()
    };

    // 1. A tool of session s-1 needs Google access: it gets an AUTH_REQUIRED link.
    let ctx = tool_context(sessions.as_ref(), &s1).await;
    let err = credentials
        .access_token(
            &ctx,
            &["https://www.googleapis.com/auth/adwords".to_string()],
        )
        .await
        .unwrap_err();
    let required = auth_required(&err).expect("AUTH_REQUIRED").clone();
    assert!(required.auth_url.starts_with(&format!("{google}/auth?")));
    let state = state_param(&required.auth_url);

    // 2./3. Google redirects to our callback with the code and that state.
    let agent = spawn(oauth_router(OAuthRoutes {
        credentials: credentials.clone(),
        sessions: sessions.clone(),
    }))
    .await;
    let page = reqwest::get(format!(
        "{agent}/oauth2/callback?code=the-code&state={state}"
    ))
    .await
    .unwrap()
    .text()
    .await
    .unwrap();
    assert!(page.contains("Authorization complete"), "{page}");

    // The tokens are now in session s-1's state ...
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
    let stored: OAuthToken =
        serde_json::from_value(loaded.state().get(CREDENTIALS_STATE_KEY).unwrap()).unwrap();
    assert_eq!(stored.access_token, "ya29.session-token");
    assert_eq!(stored.refresh_token.as_deref(), Some("1//refresh"));
    assert_eq!(
        stored.scopes,
        vec!["https://www.googleapis.com/auth/adwords"]
    );

    // 4. ... so the next tool call of s-1 proceeds without any server-side store,
    let ctx = tool_context(sessions.as_ref(), &s1).await;
    assert_eq!(
        credentials
            .access_token(
                &ctx,
                &["https://www.googleapis.com/auth/adwords".to_string()]
            )
            .await
            .unwrap(),
        "ya29.session-token"
    );
    // ... while session s-2 still has to authorize on its own.
    let ctx = tool_context(sessions.as_ref(), &s2).await;
    let err = credentials.access_token(&ctx, &[]).await.unwrap_err();
    assert!(auth_required(&err).is_some());

    // A replayed/forged state is rejected, and an unknown session is reported.
    let page = reqwest::get(format!("{agent}/oauth2/callback?code=x&state=forged.sig"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(page.contains("Authorization failed"));
    let ghost = SessionRef {
        session_id: "nope".into(),
        ..s1
    };
    let link = credentials.begin_authorization(&ghost, &[]).await;
    let page = reqwest::get(format!(
        "{agent}/oauth2/callback?code=the-code&state={}",
        state_param(&link.auth_url)
    ))
    .await
    .unwrap()
    .text()
    .await
    .unwrap();
    assert!(page.contains("could not be found"), "{page}");

    // A plain SimpleToolContext (no session) gets an actionable message.
    let ctx = SimpleToolContext::new("test");
    let err = credentials.access_token(&ctx, &[]).await.unwrap_err();
    assert!(
        auth_required(&err)
            .unwrap()
            .message
            .contains("active chat session")
    );
}
