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

//! End-to-end HTTP integration tests for write-API tool confirmation:
//! - a non-admin signed-in user calling a write tool via `POST /api/run_sse`
//!   (`x-adk-ui-protocol: a2ui`) pauses with `confirmation_pending` and the tool
//!   does NOT execute yet;
//! - only that session's owner sees and can decide the pending approval in
//!   `/api/approvals` (another user and even an `ADMIN_EMAILS` `"operator"` get
//!   empty inboxes, 404 on GET, and 403 on decide);
//! - approving via `POST /api/approvals/{id}/decide` and resuming with
//!   `POST /api/run_sse` (`parts: []`) executes the write tool once under its
//!   original call ID and completes the turn;
//! - denying via `POST /api/approvals/{id}/decide` and resuming prevents tool
//!   execution completely.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use adk_rust::agent::LlmAgentBuilder;
use adk_rust::memory::InMemoryMemoryService;
use adk_rust::server::ServerConfig;
use adk_rust::session::{CreateRequest, InMemorySessionService, SessionService};
use adk_rust::{
    CONFIRMATION_PENDING_STATUS, Content, Llm, LlmRequest, LlmResponse, LlmResponseStream, Part,
    Result as AdkResult, SingleAgentLoader, Tool, ToolContext,
};
use adspace_agent::agent::{MemorySavingAgent, build_approval_service};
use adspace_agent::auth::{
    AuthenticatedUser, GoogleOAuthFlow, OAuthClientConfig, SessionCredentialManager,
};
use adspace_agent::config::APP_NAME;
use adspace_agent::server::{
    AdminAllowList, OAuthRoutes, USER_COOKIE_NAME, build_app_with_services,
};
use async_trait::async_trait;
use reqwest::StatusCode;
use serde_json::{Value, json};

struct WriteCampaignTool {
    executions: Arc<AtomicUsize>,
}

#[async_trait]
impl Tool for WriteCampaignTool {
    fn name(&self) -> &str {
        "write_campaign"
    }

    fn description(&self) -> &str {
        "Mutates a campaign."
    }

    async fn execute(&self, _ctx: Arc<dyn ToolContext>, args: Value) -> AdkResult<Value> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        Ok(json!({ "status": "updated", "args": args }))
    }
}

struct ScriptedLlm {
    responses: Mutex<VecDeque<LlmResponse>>,
    requests: Mutex<Vec<LlmRequest>>,
}

#[async_trait]
impl Llm for ScriptedLlm {
    fn name(&self) -> &str {
        "scripted-confirm"
    }

    async fn generate_content(
        &self,
        req: LlmRequest,
        _stream: bool,
    ) -> AdkResult<LlmResponseStream> {
        self.requests.lock().unwrap().push(req);
        let next = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| LlmResponse::new(Content::new("model").with_text("done")));
        Ok(Box::pin(futures::stream::once(async move { Ok(next) })))
    }
}

struct ConfirmationTestServer {
    base: String,
    credentials: Arc<SessionCredentialManager>,
    sessions: Arc<dyn SessionService>,
    http: reqwest::Client,
    executions: Arc<AtomicUsize>,
    llm: Arc<ScriptedLlm>,
}

impl ConfirmationTestServer {
    async fn start(responses: Vec<LlmResponse>) -> Self {
        let flow = GoogleOAuthFlow::new(OAuthClientConfig {
            client_id: "cid".into(),
            client_secret: "secret".into(),
            redirect_uri: "http://localhost:8000/oauth2/callback".into(),
        });
        let credentials = Arc::new(SessionCredentialManager::new(flow));
        let sessions: Arc<dyn SessionService> = Arc::new(InMemorySessionService::new());
        let approvals = build_approval_service();
        let executions = Arc::new(AtomicUsize::new(0));
        let llm = Arc::new(ScriptedLlm {
            responses: Mutex::new(VecDeque::from(responses)),
            requests: Mutex::new(Vec::new()),
        });

        let inner = LlmAgentBuilder::new(APP_NAME)
            .model(llm.clone())
            .instruction("test confirm agent")
            .tool(Arc::new(WriteCampaignTool {
                executions: executions.clone(),
            }))
            .require_tool_confirmation("write_campaign")
            .build()
            .unwrap();
        let agent = Arc::new(
            MemorySavingAgent::new(Arc::new(inner), Arc::new(InMemoryMemoryService::new()))
                .with_approval_service(approvals.clone()),
        );

        let config = ServerConfig::new(Arc::new(SingleAgentLoader::new(agent)), sessions.clone());
        let app = build_app_with_services(
            config,
            OAuthRoutes {
                credentials: credentials.clone(),
                sessions: sessions.clone(),
                resume: None,
                admins: AdminAllowList::new(["admin@example.com".to_string()]),
            },
            None,
            Some(approvals),
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
            executions,
            llm,
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

    async fn create_session(&self, user: &str, session_id: &str) {
        self.sessions
            .create(CreateRequest {
                app_name: APP_NAME.into(),
                user_id: user.into(),
                session_id: Some(session_id.into()),
                state: HashMap::new(),
            })
            .await
            .unwrap();
    }

    async fn run_sse(&self, cookie: &str, body: Value) -> Vec<Value> {
        let resp = self
            .http
            .post(format!("{}/api/run_sse", self.base))
            .header(reqwest::header::COOKIE, cookie)
            .header("x-adk-ui-protocol", "a2ui")
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let text = resp.text().await.unwrap();
        parse_sse_events(&text)
    }
}

fn parse_sse_events(body: &str) -> Vec<Value> {
    body.split("\n\n")
        .filter_map(|chunk| {
            let data = chunk
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(str::trim_start)
                .collect::<Vec<_>>()
                .join("\n");
            if data.is_empty() || data == "[DONE]" {
                return None;
            }
            let parsed: Value = serde_json::from_str(&data).ok()?;
            Some(parsed.get("event").cloned().unwrap_or(parsed))
        })
        .collect()
}

fn has_tool_confirmation(ev: &Value, tool: &str, call_id: &str) -> bool {
    let conf = ev.get("actions").and_then(|a| {
        a.get("tool_confirmation")
            .or_else(|| a.get("toolConfirmation"))
    });
    conf.is_some_and(|c| {
        c.get("toolName").and_then(Value::as_str) == Some(tool)
            && c.get("functionCallId").and_then(Value::as_str) == Some(call_id)
    })
}

fn has_pending_response(ev: &Value) -> bool {
    ev.pointer("/content/parts")
        .and_then(Value::as_array)
        .is_some_and(|parts| {
            parts.iter().any(|p| {
                p.pointer("/functionResponse/response/status")
                    .and_then(Value::as_str)
                    == Some(CONFIRMATION_PENDING_STATUS)
            })
        })
}

#[tokio::test]
async fn write_tool_confirmation_isolates_owner_and_resumes_on_approve_and_reject() {
    // 1. Approve flow + owner isolation
    let server = ConfirmationTestServer::start(vec![
        LlmResponse::new(Content {
            role: "model".into(),
            parts: vec![Part::FunctionCall {
                name: "write_campaign".into(),
                args: json!({ "campaignId": "c-100", "budgetMicros": 5_000_000 }),
                id: Some("call-write-1".into()),
                thought_signature: None,
            }],
        }),
        LlmResponse::new(Content::new("model").with_text("Campaign updated.")),
    ])
    .await;

    let victim_cookie = server.cookie("victim@example.com");
    let attacker_cookie = server.cookie("attacker@example.com");
    let admin_cookie = server.cookie("admin@example.com");
    server
        .create_session("victim@example.com", "s-approve")
        .await;

    let hold_events = server
        .run_sse(
            &victim_cookie,
            json!({
                "appName": APP_NAME,
                "userId": "victim@example.com",
                "sessionId": "s-approve",
                "streaming": false,
                "newMessage": { "role": "user", "parts": [{ "text": "Update campaign c-100" }] }
            }),
        )
        .await;

    assert_eq!(
        server.executions.load(Ordering::SeqCst),
        0,
        "write tool must NOT execute before user confirmation"
    );
    assert!(
        hold_events.iter().any(has_pending_response),
        "expected confirmation_pending FunctionResponse in SSE stream: {hold_events:?}"
    );
    assert!(
        hold_events
            .iter()
            .any(|ev| has_tool_confirmation(ev, "write_campaign", "call-write-1")),
        "expected actions.tool_confirmation event in SSE stream: {hold_events:?}"
    );

    // Owner sees pending approval in GET /api/approvals.
    let inbox: Value = server
        .http
        .get(format!(
            "{}/api/approvals?app={APP_NAME}&session=s-approve&state=pending",
            server.base
        ))
        .header(reqwest::header::COOKIE, &victim_cookie)
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    let items = inbox["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "{inbox}");
    let approval_id = items[0]["id"].as_str().unwrap().to_string();
    let version = items[0]["version"].as_u64().unwrap();
    assert_eq!(items[0]["toolName"], "write_campaign");
    assert_eq!(items[0]["state"], "pending");

    // Attacker AND even an ADMIN_EMAILS "operator" cannot see or decide victim's approval.
    for other_cookie in [&attacker_cookie, &admin_cookie] {
        let other_inbox: Value = server
            .http
            .get(format!(
                "{}/api/approvals?app={APP_NAME}&session=s-approve",
                server.base
            ))
            .header(reqwest::header::COOKIE, other_cookie)
            .send()
            .await
            .unwrap()
            .error_for_status()
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(other_inbox["items"].as_array().unwrap().len(), 0);

        let get_other = server
            .http
            .get(format!("{}/api/approvals/{approval_id}", server.base))
            .header(reqwest::header::COOKIE, other_cookie)
            .send()
            .await
            .unwrap();
        assert_eq!(get_other.status(), StatusCode::NOT_FOUND);

        let decide_other = server
            .http
            .post(format!(
                "{}/api/approvals/{approval_id}/decide",
                server.base
            ))
            .header(reqwest::header::COOKIE, other_cookie)
            .json(&json!({ "expectedVersion": version, "action": "approve" }))
            .send()
            .await
            .unwrap();
        assert_eq!(decide_other.status(), StatusCode::FORBIDDEN);
    }

    // Victim approves and resumes with empty newMessage.
    let decided: Value = server
        .http
        .post(format!(
            "{}/api/approvals/{approval_id}/decide",
            server.base
        ))
        .header(reqwest::header::COOKIE, &victim_cookie)
        .json(&json!({ "expectedVersion": version, "action": "approve" }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(decided["state"], "approved");

    let resume_events = server
        .run_sse(
            &victim_cookie,
            json!({
                "appName": APP_NAME,
                "userId": "victim@example.com",
                "sessionId": "s-approve",
                "streaming": false,
                "newMessage": { "role": "user", "parts": [] }
            }),
        )
        .await;

    assert_eq!(
        server.executions.load(Ordering::SeqCst),
        1,
        "write tool must execute exactly once on approve+resume"
    );
    assert!(resume_events.iter().any(|ev| {
        ev.pointer("/content/parts")
            .and_then(Value::as_array)
            .is_some_and(|parts| {
                parts
                    .iter()
                    .any(|p| p.get("text").and_then(Value::as_str) == Some("Campaign updated."))
            })
    }));
    // Second LLM request received the executed FunctionResponse under original call ID.
    {
        let requests = server.llm.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests[1].contents.iter().any(|c| {
            c.parts.iter().any(|p| {
                matches!(p, Part::FunctionResponse { id: Some(id), function_response, .. }
                    if id == "call-write-1" && function_response.response["status"] == "updated")
            })
        }));
    }

    // 2. Reject path never executes the write tool.
    let reject_server = ConfirmationTestServer::start(vec![
        LlmResponse::new(Content {
            role: "model".into(),
            parts: vec![Part::FunctionCall {
                name: "write_campaign".into(),
                args: json!({ "campaignId": "c-200", "budgetMicros": 9_000_000 }),
                id: Some("call-write-2".into()),
                thought_signature: None,
            }],
        }),
        LlmResponse::new(Content::new("model").with_text("Cancelled as requested.")),
    ])
    .await;

    let reject_cookie = reject_server.cookie("victim@example.com");
    reject_server
        .create_session("victim@example.com", "s-reject")
        .await;

    reject_server
        .run_sse(
            &reject_cookie,
            json!({
                "appName": APP_NAME,
                "userId": "victim@example.com",
                "sessionId": "s-reject",
                "streaming": false,
                "newMessage": { "role": "user", "parts": [{ "text": "Update campaign c-200" }] }
            }),
        )
        .await;
    assert_eq!(reject_server.executions.load(Ordering::SeqCst), 0);

    let reject_inbox: Value = reject_server
        .http
        .get(format!(
            "{}/api/approvals?app={APP_NAME}&session=s-reject&state=pending",
            reject_server.base
        ))
        .header(reqwest::header::COOKIE, &reject_cookie)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let reject_id = reject_inbox["items"][0]["id"].as_str().unwrap();
    let reject_ver = reject_inbox["items"][0]["version"].as_u64().unwrap();

    let rejected: Value = reject_server
        .http
        .post(format!(
            "{}/api/approvals/{reject_id}/decide",
            reject_server.base
        ))
        .header(reqwest::header::COOKIE, &reject_cookie)
        .json(&json!({
            "expectedVersion": reject_ver,
            "action": "reject",
            "reason": "Too expensive"
        }))
        .send()
        .await
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(rejected["state"], "rejected");

    let reject_resume = reject_server
        .run_sse(
            &reject_cookie,
            json!({
                "appName": APP_NAME,
                "userId": "victim@example.com",
                "sessionId": "s-reject",
                "streaming": false,
                "newMessage": { "role": "user", "parts": [] }
            }),
        )
        .await;

    assert_eq!(
        reject_server.executions.load(Ordering::SeqCst),
        0,
        "denied write tool must NEVER execute"
    );
    assert!(reject_resume.iter().any(|ev| {
        ev.pointer("/content/parts")
            .and_then(Value::as_array)
            .is_some_and(|parts| {
                parts.iter().any(|p| {
                    p.get("text").and_then(Value::as_str) == Some("Cancelled as requested.")
                })
            })
    }));
}
