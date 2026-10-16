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

//! Offline end-to-end smoke test: builds the real agent with a scripted LLM
//! and runs one turn through the adk-rust runner.
//!
//! Verifies that (1) uploaded files become session artifacts, (2) tools from
//! our toolsets are callable by the model and (3) the conversation is saved
//! to memory after the turn.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use adk_rust::artifact::{
    ArtifactService, InMemoryArtifactService, ListRequest as ArtifactListRequest, LoadRequest,
};
use adk_rust::memory::{InMemoryMemoryService, MemoryService, SearchRequest};
use adk_rust::runner::Runner;
use adk_rust::session::{CreateRequest, InMemorySessionService, SessionService};
use adk_rust::{Content, Llm, LlmRequest, LlmResponse, LlmResponseStream, Part, SessionId, UserId};
use adspace_agent::agent::{self, build_credential_manager};
use adspace_agent::config::{APP_NAME, CompactionSettings, Settings};
use adspace_agent::services;
use async_trait::async_trait;
use futures::StreamExt;

/// Returns one scripted response per model call and records the requests.
struct ScriptedLlm {
    responses: Mutex<VecDeque<LlmResponse>>,
    requests: Mutex<Vec<LlmRequest>>,
}

#[async_trait]
impl Llm for ScriptedLlm {
    fn name(&self) -> &str {
        "scripted"
    }

    async fn generate_content(
        &self,
        req: LlmRequest,
        _stream: bool,
    ) -> adk_rust::Result<LlmResponseStream> {
        self.requests.lock().unwrap().push(req);
        let response = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| {
                LlmResponse::new(Content::new("model").with_text("(no more scripted responses)"))
            });
        Ok(Box::pin(futures::stream::once(async move { Ok(response) })))
    }
}

fn settings() -> Settings {
    Settings {
        model: "scripted".into(),
        video_model: "veo".into(),
        image_model: "img".into(),
        google_cloud_project: None,
        google_cloud_location: "global".into(),
        use_vertex_ai: false,
        google_api_key: None,
        client_id: String::new(),
        client_secret: String::new(),
        google_ads_developer_token: String::new(),
        google_ads_login_customer_id: None,
        // No Discovery-based toolsets: keeps the test offline. CM360 is a
        // local toolset so it is included to exercise its instruction hook.
        enabled_toolsets: vec!["campaign_manager_360".into()],
        google_ads_tool_filter: None,
        lazy_toolsets: vec![],
        skills_bucket_name: None,
        local_skills_dir: "./does-not-exist".into(),
        compaction: CompactionSettings {
            compaction_interval: 3,
            overlap_size: 1,
            token_threshold: 500_000,
            event_retention_size: 3,
        },
    }
}

#[tokio::test]
async fn agent_runs_a_tool_turn_and_persists_artifacts_and_memory() {
    // SAFETY: tests in this file run on a single thread per process start;
    // the variables only steer backend selection.
    unsafe {
        std::env::remove_var("PYTHON_EXECUTOR");
        std::env::set_var("LOCAL_SKILLS_DIR", "./does-not-exist");
        std::env::remove_var("SKILLS_BUCKET_NAME");
    }
    let settings = settings();
    let credentials = build_credential_manager(&settings);
    let memory: Arc<InMemoryMemoryService> = Arc::new(InMemoryMemoryService::new());
    let memory_service: Arc<dyn MemoryService> = memory.clone();

    let llm = Arc::new(ScriptedLlm {
        responses: Mutex::new(VecDeque::from(vec![
            LlmResponse::new(Content {
                role: "model".into(),
                parts: vec![Part::FunctionCall {
                    name: "list_user_files".into(),
                    args: serde_json::json!({}),
                    id: None,
                    thought_signature: None,
                }],
            }),
            LlmResponse::new(Content::new("model").with_text("Here are your files.")),
        ])),
        requests: Mutex::new(Vec::new()),
    });

    let agent = agent::create_agent(&settings, llm.clone(), credentials, memory_service.clone())
        .await
        .expect("agent builds");
    assert_eq!(agent.name(), APP_NAME);

    let sessions = Arc::new(InMemorySessionService::new());
    let artifacts = Arc::new(InMemoryArtifactService::new());
    let runner = Runner::builder()
        .app_name(APP_NAME)
        .agent(agent)
        .session_service(sessions.clone())
        .artifact_service(artifacts.clone())
        .memory_service(Arc::new(adk_rust::memory::MemoryServiceAdapter::new(
            memory_service,
            APP_NAME,
            "tester",
        )))
        .build()
        .expect("runner builds");

    let session = sessions
        .create(CreateRequest {
            app_name: APP_NAME.into(),
            user_id: "tester".into(),
            session_id: None,
            state: HashMap::new(),
        })
        .await
        .unwrap();
    let session_id = session.id().to_string();

    // A user message with an attached CSV, as the web UI sends it.
    let csv = "Profile ID,Advertiser ID\n1,2\n";
    let mut message = Content::new("user").with_text("What files do I have?");
    message.parts.push(Part::InlineData {
        mime_type: "text/csv".into(),
        data: csv.as_bytes().to_vec(),
        uri: None,
        annotations: None,
    });

    let mut stream = runner
        .run(
            UserId::new("tester").unwrap(),
            SessionId::new(&session_id).unwrap(),
            message,
        )
        .await
        .unwrap();
    let mut texts = Vec::new();
    let mut tool_responses = Vec::new();
    while let Some(event) = stream.next().await {
        let event = event.expect("event ok");
        if let Some(content) = event.content() {
            for part in &content.parts {
                match part {
                    Part::Text { text } => texts.push(text.clone()),
                    Part::FunctionResponse {
                        function_response, ..
                    } => tool_responses.push(function_response.response.clone()),
                    _ => {}
                }
            }
        }
    }

    // (2) the tool ran and saw the uploaded artifact …
    assert_eq!(
        tool_responses.len(),
        1,
        "exactly one tool call: {tool_responses:?}"
    );
    let listing = tool_responses[0].to_string();
    assert!(
        listing.contains("artifact_") && listing.contains(".csv"),
        "tool saw the uploaded CSV: {listing}"
    );
    assert!(
        texts.iter().any(|t| t.contains("Here are your files.")),
        "final text present: {texts:?}"
    );

    // (1) … which was stored as text by the upload callback.
    let list = artifacts
        .list(ArtifactListRequest {
            app_name: APP_NAME.into(),
            user_id: "tester".into(),
            session_id: session_id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(list.file_names.len(), 1);
    let loaded = artifacts
        .load(LoadRequest {
            app_name: APP_NAME.into(),
            user_id: "tester".into(),
            session_id: session_id.clone(),
            file_name: list.file_names[0].clone(),
            version: None,
        })
        .await
        .unwrap();
    assert_eq!(
        loaded.part,
        Part::Text {
            text: csv.to_string()
        }
    );

    // The model saw our tool declarations and the CM360 workflow instructions.
    let first = llm
        .requests
        .lock()
        .unwrap()
        .first()
        .cloned()
        .expect("at least one model call");
    for expected in [
        "list_user_files",
        "load_artifacts",
        "parse_sheet_tool",
        "traffic_campaigns_in_cm360_tool",
        "gemini_code_execution",
        "reload_skills",
        "google_search",
        "url_context",
        "google_maps",
    ] {
        assert!(
            first.tools.contains_key(expected),
            "tool {expected} declared; have {:?}",
            first.tools.keys().collect::<Vec<_>>()
        );
    }
    assert!(
        first.contents.iter().any(|c| c.parts.iter().any(|p| p
            .text()
            .is_some_and(|t| t.contains("CM360 Campaign Trafficking Instructions")))),
        "CM360 instructions injected"
    );

    // (3) the conversation was saved to memory after the turn.
    let found = memory
        .search(SearchRequest {
            query: "files".into(),
            user_id: "tester".into(),
            app_name: APP_NAME.into(),
            limit: None,
            min_score: None,
            project_id: None,
        })
        .await
        .unwrap();
    assert!(
        !found.memories.is_empty(),
        "memory populated after the turn"
    );
}

#[tokio::test]
async fn in_memory_services_build_from_env() {
    unsafe {
        std::env::set_var("SESSION_SERVICE_URI", "memory://");
        std::env::set_var("ARTIFACT_SERVICE_URI", "memory://");
        std::env::set_var("MEMORY_SERVICE_URI", "memory://");
    }
    services::build_session_service()
        .await
        .expect("session service");
    services::build_artifact_service().expect("artifact service");
    services::build_memory_service().expect("memory service");
}
