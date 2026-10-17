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

//! End-to-end test of the session-bound OAuth flow over real HTTP:
//!
//! 1. a tool context of session `s-1` asks for credentials and gets an
//!    `AUTH_REQUIRED` link whose `state` names that session;
//! 2. Google (mocked) redirects to `/oauth2/callback?code&state`;
//! 3. the handler exchanges the code, writes the tokens into the *user's*
//!    state (reachable from every session of that user) and resumes that chat
//!    with a turn of its own (no "done" needed);
//! 4. a later tool call of `s-1` — and of the user's other session `s-2` —
//!    finds a valid token, while another user still has none and a new scope
//!    is requested incrementally without re-prompting consent.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use adk_rust::agent::LlmAgentBuilder;
use adk_rust::artifact::InMemoryArtifactService;
use adk_rust::memory::InMemoryMemoryService;
use adk_rust::session::{CreateRequest, GetRequest, InMemorySessionService, SessionService};
use adk_rust::tool::SimpleToolContext;
use adk_rust::{
    Artifacts, CallbackContext, Content, EventActions, Llm, LlmRequest, LlmResponse,
    LlmResponseStream, MemoryEntry, ReadonlyContext, Session, State, ToolContext,
};
use adspace_agent::agent::TurnRunner;
use adspace_agent::auth::{
    CREDENTIALS_STATE_KEY, GoogleOAuthFlow, OAuthClientConfig, OAuthToken, OAuthUiEvent,
    SessionCredentialManager, SessionRef, USER_CREDENTIALS_STATE_KEY, auth_required,
};
use adspace_agent::server::{AdminAllowList, OAuthRoutes, oauth_router, resume_message};
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

/// Model that answers every call with a fixed text and records the requests.
struct EchoLlm {
    requests: Mutex<Vec<LlmRequest>>,
}

#[async_trait]
impl Llm for EchoLlm {
    fn name(&self) -> &str {
        "echo"
    }

    async fn generate_content(
        &self,
        req: LlmRequest,
        _stream: bool,
    ) -> adk_rust::Result<LlmResponseStream> {
        self.requests.lock().unwrap().push(req);
        let response = LlmResponse::new(Content::new("model").with_text("Resuming the Ads query."));
        Ok(Box::pin(futures::stream::once(async move { Ok(response) })))
    }
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

    let sessions: Arc<dyn SessionService> = Arc::new(InMemorySessionService::new());
    for (user, id) in [
        ("local-user", "s-1"),
        ("local-user", "s-2"),
        ("other-user", "s-3"),
    ] {
        sessions
            .create(CreateRequest {
                app_name: "adspace_agent".into(),
                user_id: user.into(),
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
    let other_user = SessionRef {
        user_id: "other-user".into(),
        session_id: "s-3".into(),
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
    // Least privilege: the link asks for the calling tool's scope only.
    assert_eq!(
        required.scopes,
        vec!["https://www.googleapis.com/auth/adwords".to_string()]
    );
    let state = state_param(&required.auth_url);

    // 2./3. Google redirects to our callback with the code and that state. The
    // route is wired with a TurnRunner over a scripted model, as `serve` does.
    let llm = Arc::new(EchoLlm {
        requests: Mutex::new(Vec::new()),
    });
    let chat_agent = Arc::new(
        LlmAgentBuilder::new("adspace_agent")
            .model(llm.clone())
            .instruction("test agent")
            .build()
            .unwrap(),
    );
    let resume = TurnRunner::new(
        chat_agent,
        sessions.clone(),
        Arc::new(InMemoryArtifactService::new()),
        Arc::new(InMemoryMemoryService::new()),
    );
    let agent = spawn(oauth_router(OAuthRoutes {
        credentials: credentials.clone(),
        sessions: sessions.clone(),
        resume: Some(Arc::new(resume)),
        admins: AdminAllowList::default(),
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
    assert!(page.contains("continuing where it left off"), "{page}");

    // The chat was resumed in the background: wait for the model's answer to
    // land in the session.
    let resumed = async {
        loop {
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
            let texts: Vec<String> = loaded
                .events()
                .all()
                .iter()
                .filter_map(|e| e.content().map(|c| (e.author.clone(), c.clone())))
                .flat_map(|(author, c)| {
                    c.parts.into_iter().filter_map(move |p| match p {
                        adk_rust::Part::Text { text } => Some(format!("{author}: {text}")),
                        _ => None,
                    })
                })
                .collect();
            if texts.iter().any(|t| t.contains("Resuming the Ads query.")) {
                break texts;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
    };
    let texts = tokio::time::timeout(std::time::Duration::from_secs(5), resumed)
        .await
        .expect("the session is resumed after the callback");
    let expected_user_turn = resume_message(&OAuthToken {
        access_token: String::new(),
        refresh_token: None,
        expires_at: None,
        scopes: vec!["https://www.googleapis.com/auth/adwords".to_string()],
    });
    assert!(expected_user_turn.contains("(adwords)"));
    assert_eq!(
        texts,
        vec![
            format!("user: {expected_user_turn}"),
            "adspace_agent: Resuming the Ads query.".to_string(),
        ]
    );
    assert_eq!(llm.requests.lock().unwrap().len(), 1, "one background turn");

    // The tokens are now in the user's state (merged into every session of
    // `local-user`; no legacy per-session copy) ...
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
        serde_json::from_value(loaded.state().get(USER_CREDENTIALS_STATE_KEY).unwrap()).unwrap();
    assert!(loaded.state().get(CREDENTIALS_STATE_KEY).is_none());
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
    // ... and so does session s-2 of the same user: one grant per user, not
    // per chat ...
    let ctx = tool_context(sessions.as_ref(), &s2).await;
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
    // ... while another user's session still has to authorize, and a scope
    // the user has not granted yet asks only for that scope, without
    // re-prompting consent for what was already granted.
    let ctx = tool_context(sessions.as_ref(), &other_user).await;
    let err = credentials
        .access_token(
            &ctx,
            &["https://www.googleapis.com/auth/adwords".to_string()],
        )
        .await
        .unwrap_err();
    assert!(auth_required(&err).is_some());
    let ctx = tool_context(sessions.as_ref(), &s2).await;
    let err = credentials
        .access_token(
            &ctx,
            &["https://www.googleapis.com/auth/drive.readonly".to_string()],
        )
        .await
        .unwrap_err();
    let incremental = auth_required(&err).expect("new scope needs a grant");
    assert_eq!(
        incremental.scopes,
        vec!["https://www.googleapis.com/auth/drive.readonly"]
    );
    assert!(
        !incremental.auth_url.contains("prompt="),
        "{}",
        incremental.auth_url
    );
    assert!(required.auth_url.contains("prompt=consent"), "first grant");

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
    let link = credentials.begin_authorization(&ghost, &["s".to_string()]);
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

#[tokio::test]
async fn in_flight_tool_waits_for_popup_callback_without_stopping_turn() {
    let google = spawn(Router::new().route("/token", post(mock_token_endpoint))).await;
    let flow = GoogleOAuthFlow::new(OAuthClientConfig {
        client_id: "cid".into(),
        client_secret: "secret".into(),
        redirect_uri: "http://localhost:8000/oauth2/callback".into(),
    })
    .with_endpoints(format!("{google}/auth"), format!("{google}/token"));
    let credentials = Arc::new(SessionCredentialManager::new(flow));
    credentials.enable_interactive_wait(std::time::Duration::from_secs(5), false);

    let sessions: Arc<dyn SessionService> = Arc::new(InMemorySessionService::new());
    sessions
        .create(CreateRequest {
            app_name: "adspace_agent".into(),
            user_id: "local-user".into(),
            session_id: Some("s-popup".into()),
            state: HashMap::new(),
        })
        .await
        .unwrap();
    let session = SessionRef {
        app_name: "adspace_agent".into(),
        user_id: "local-user".into(),
        session_id: "s-popup".into(),
    };

    let llm = Arc::new(EchoLlm {
        requests: Mutex::new(Vec::new()),
    });
    let chat_agent = Arc::new(
        LlmAgentBuilder::new("adspace_agent")
            .model(llm.clone())
            .instruction("test agent")
            .build()
            .unwrap(),
    );
    let resume = TurnRunner::new(
        chat_agent,
        sessions.clone(),
        Arc::new(InMemoryArtifactService::new()),
        Arc::new(InMemoryMemoryService::new()),
    );
    let agent = spawn(oauth_router(OAuthRoutes {
        credentials: credentials.clone(),
        sessions: sessions.clone(),
        resume: Some(Arc::new(resume)),
        admins: AdminAllowList::default(),
    }))
    .await;

    let mut events_rx = credentials.subscribe_events();
    let ctx = Arc::new(tool_context(sessions.as_ref(), &session).await);
    let creds_for_tool = credentials.clone();
    let ctx_for_tool = ctx.clone();

    // Start the tool's access_token call: it stays in-flight awaiting the popup callback.
    let token_task = tokio::spawn(async move {
        creds_for_tool
            .access_token(
                ctx_for_tool.as_ref(),
                &["https://www.googleapis.com/auth/adwords".to_string()],
            )
            .await
    });

    // The server immediately broadcasts AuthRequired so `/ui/` pops up the consent window.
    // The event is addressed to the session's owner and carries the consent URL
    // plus an opaque request id — never the signed OAuth state itself.
    let event = tokio::time::timeout(std::time::Duration::from_secs(2), events_rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(event.user_id(), "local-user");
    let OAuthUiEvent::AuthRequired {
        request_id,
        session_id,
        auth_url,
        scopes,
        ..
    } = event.clone()
    else {
        panic!("expected AuthRequired, got {event:?}");
    };
    assert_eq!(session_id, "s-popup");
    assert!(auth_url.starts_with(&format!("{google}/auth?")));
    assert_eq!(scopes, vec!["https://www.googleapis.com/auth/adwords"]);
    let wire: Value = serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
    assert_eq!(wire["type"], "auth_required");
    assert_eq!(wire["request_id"], request_id);
    assert!(wire.get("state").is_none(), "{wire}");
    assert!(wire.get("user_id").is_none(), "{wire}");
    let state = state_param(&auth_url);

    // The popup acknowledgment is keyed by the request id and bound to the user.
    let ack = reqwest::Client::new()
        .post(format!("{agent}/oauth2/ack"))
        .json(&json!({ "request_id": request_id, "opened": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(ack.status(), reqwest::StatusCode::UNAUTHORIZED, "no cookie");

    // Complete the OAuth redirect in the popup while `access_token` is still waiting.
    let page = reqwest::get(format!(
        "{agent}/oauth2/callback?code=the-code&state={state}"
    ))
    .await
    .unwrap()
    .text()
    .await
    .unwrap();
    assert!(page.contains("Authorization complete"), "{page}");

    // The in-flight `access_token` call resolves directly with the bearer token!
    let access_token = tokio::time::timeout(std::time::Duration::from_secs(2), token_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(access_token, "ya29.session-token");

    // And because the original tool call stayed alive and resumed in-place,
    // no duplicate background `TurnRunner` turn was spawned.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(llm.requests.lock().unwrap().len(), 0);
}
