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

//! Authorization boundary of the full HTTP application (adk-server routes +
//! the cookie identity extractor + the response guard), over real HTTP:
//!
//! - every adk-server route — path-addressed (`/api/sessions/{app}/{user}/…`,
//!   `/api/apps/{app}/users/{user}/…`) and body-addressed (`/api/run_sse`) —
//!   refuses anonymous callers and callers whose cookie names another user;
//! - the OAuth token persisted in a session never reaches the browser.

use std::collections::HashMap;
use std::sync::Arc;

use adk_rust::Launcher;
use adk_rust::agent::LlmAgentBuilder;
use adk_rust::session::{CreateRequest, GetRequest, InMemorySessionService, SessionService};
use adk_rust::{Content, Llm, LlmRequest, LlmResponse, LlmResponseStream};
use adspace_agent::auth::{
    AuthenticatedUser, CREDENTIALS_STATE_KEY, GoogleOAuthFlow, OAuthClientConfig, OAuthToken,
    SessionCredentialManager, SessionRef,
};
use adspace_agent::config::APP_NAME;
use adspace_agent::server::{AdminAllowList, OAuthRoutes, USER_COOKIE_NAME, build_app};
use async_trait::async_trait;
use reqwest::StatusCode;
use serde_json::json;

struct EchoLlm;

#[async_trait]
impl Llm for EchoLlm {
    fn name(&self) -> &str {
        "echo"
    }

    async fn generate_content(
        &self,
        _req: LlmRequest,
        _stream: bool,
    ) -> adk_rust::Result<LlmResponseStream> {
        let response = LlmResponse::new(Content::new("model").with_text("ok"));
        Ok(Box::pin(futures::stream::once(async move { Ok(response) })))
    }
}

struct TestServer {
    base: String,
    credentials: Arc<SessionCredentialManager>,
    sessions: Arc<dyn SessionService>,
    http: reqwest::Client,
}

impl TestServer {
    async fn start(oauth_configured: bool) -> Self {
        let flow = GoogleOAuthFlow::new(OAuthClientConfig {
            client_id: if oauth_configured { "cid" } else { "" }.into(),
            client_secret: if oauth_configured { "secret" } else { "" }.into(),
            redirect_uri: "http://localhost:8000/oauth2/callback".into(),
        });
        let credentials = Arc::new(SessionCredentialManager::new(flow));
        let sessions: Arc<dyn SessionService> = Arc::new(InMemorySessionService::new());
        let agent = Arc::new(
            LlmAgentBuilder::new(APP_NAME)
                .model(Arc::new(EchoLlm))
                .instruction("test agent")
                .build()
                .unwrap(),
        );
        let launcher = Launcher::new(agent)
            .app_name(APP_NAME)
            .with_session_service(sessions.clone());
        let app = build_app(
            launcher,
            OAuthRoutes {
                credentials: credentials.clone(),
                sessions: sessions.clone(),
                resume: None,
                admins: AdminAllowList::default(),
            },
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(axum::serve(listener, app).into_future());
        Self {
            base: format!("http://{addr}"),
            credentials,
            sessions,
            http: reqwest::Client::new(),
        }
    }

    fn cookie(&self, email: &str) -> String {
        let user = AuthenticatedUser {
            id: email.into(),
            email: email.into(),
            name: email.split('@').next().unwrap_or("u").into(),
            picture: None,
            sub: None,
            iat: chrono::Utc::now().timestamp(),
        };
        format!(
            "{USER_COOKIE_NAME}={}",
            self.credentials.sign_user_cookie(&user)
        )
    }

    async fn create_session(&self, user_id: &str, session_id: &str) -> SessionRef {
        self.sessions
            .create(CreateRequest {
                app_name: APP_NAME.into(),
                user_id: user_id.into(),
                session_id: Some(session_id.into()),
                state: HashMap::new(),
            })
            .await
            .unwrap();
        SessionRef {
            app_name: APP_NAME.into(),
            user_id: user_id.into(),
            session_id: session_id.into(),
        }
    }

    async fn event_count(&self, session: &SessionRef) -> usize {
        self.sessions
            .get(GetRequest {
                app_name: session.app_name.clone(),
                user_id: session.user_id.clone(),
                session_id: session.session_id.clone(),
                num_recent_events: None,
                after: None,
            })
            .await
            .unwrap()
            .events()
            .len()
    }

    fn get(&self, path: &str, cookie: Option<&str>) -> reqwest::RequestBuilder {
        let req = self.http.get(format!("{}{path}", self.base));
        match cookie {
            Some(c) => req.header(reqwest::header::COOKIE, c),
            None => req,
        }
    }

    fn run_sse(
        &self,
        user_id: &str,
        session_id: &str,
        cookie: Option<&str>,
    ) -> reqwest::RequestBuilder {
        let req = self
            .http
            .post(format!("{}/api/run_sse", self.base))
            .json(&json!({
                "appName": APP_NAME,
                "userId": user_id,
                "sessionId": session_id,
                "newMessage": { "role": "user", "parts": [{ "text": "hi" }] },
            }));
        match cookie {
            Some(c) => req.header(reqwest::header::COOKIE, c),
            None => req,
        }
    }
}

#[tokio::test]
async fn every_api_route_requires_the_owning_users_cookie() {
    let server = TestServer::start(true).await;
    let victim = server
        .create_session("victim@example.com", "s-victim")
        .await;
    let token = OAuthToken {
        access_token: "ya29.victim-secret".into(),
        refresh_token: Some("1//victim-refresh".into()),
        expires_at: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
        scopes: vec!["https://www.googleapis.com/auth/adwords".into()],
    };
    server
        .credentials
        .persist(server.sessions.as_ref(), &victim, &token)
        .await
        .unwrap();
    let victim_cookie = server.cookie("victim@example.com");
    let attacker_cookie = server.cookie("attacker@example.com");
    let session_paths = [
        "/api/sessions/adspace_agent/victim%40example.com/s-victim".to_string(),
        "/api/apps/adspace_agent/users/victim%40example.com/sessions/s-victim".to_string(),
        "/api/apps/adspace_agent/users/victim%40example.com/sessions".to_string(),
        "/api/sessions/adspace_agent/victim%40example.com/s-victim/artifacts".to_string(),
    ];

    // 1. Anonymous callers get 401 on every path-addressed route and on the
    //    body-addressed `/api/run_sse`.
    for path in &session_paths {
        let resp = server.get(path, None).send().await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{path}");
    }
    let resp = server
        .run_sse("victim@example.com", "s-victim", None)
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "anonymous /api/run_sse"
    );

    // 2. A signed-in caller naming another user's session is refused ...
    for path in &session_paths {
        let resp = server
            .get(path, Some(&attacker_cookie))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN, "{path}");
    }
    // ... and `/api/run_sse` with the victim's `userId` in the body runs under
    // the caller's own identity: the victim's session is untouched.
    let before = server.event_count(&victim).await;
    let resp = server
        .run_sse("victim@example.com", "s-victim", Some(&attacker_cookie))
        .send()
        .await
        .unwrap();
    let status = resp.status();
    let body = resp.text().await.unwrap();
    assert!(!body.contains("ya29.victim-secret"), "{body}");
    assert_eq!(server.event_count(&victim).await, before, "{status} {body}");

    // 3. The owner can read the session, but the token never leaves the server.
    let resp = server
        .get(&session_paths[0], Some(&victim_cookie))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["state"].get(CREDENTIALS_STATE_KEY).is_none(), "{body}");
    assert!(!body.to_string().contains("ya29.victim-secret"), "{body}");
    let resp = server
        .get(&session_paths[1], Some(&victim_cookie))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let text = resp.text().await.unwrap();
    assert!(!text.contains("ya29.victim-secret"), "{text}");

    let resp = server
        .run_sse("victim@example.com", "s-victim", Some(&victim_cookie))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(
        resp.headers()[reqwest::header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("text/event-stream")
    );
    let stream = resp.text().await.unwrap();
    assert!(stream.contains("data: "), "{stream}");
    assert!(!stream.contains("ya29.victim-secret"), "{stream}");
    assert!(server.event_count(&victim).await > before);

    // 4. With Google Sign-In configured the stock dev UI is gone.
    let resp = server
        .http
        .get(format!("{}/ui/", server.base))
        .send()
        .await
        .unwrap();
    assert!(resp.url().path().starts_with("/app"), "{}", resp.url());
}

#[tokio::test]
async fn without_sign_in_only_the_anonymous_default_user_exists() {
    let server = TestServer::start(false).await;
    server.create_session("user", "s-dev").await;
    server
        .create_session("someone@example.com", "s-other")
        .await;

    let resp = server
        .get("/api/sessions/adspace_agent/user/s-dev", None)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let resp = server
        .get(
            "/api/sessions/adspace_agent/someone%40example.com/s-other",
            None,
        )
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let resp = server
        .http
        .get(format!("{}/ui/", server.base))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "dev UI stays available");
}
