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
use adk_rust::{
    Content, Llm, LlmRequest, LlmResponse, LlmResponseStream, Part, ReadonlyContext, SessionId,
    Tool, Toolset, UserId,
};
use adspace_agent::agent::{self, build_credential_manager};
use adspace_agent::config::{APP_NAME, CompactionSettings, Settings};
use adspace_agent::services;
use adspace_agent::tools::bigquery::BigQueryToolset;
use adspace_agent::tools::cm360_trafficking::{Cm360Services, Cm360TraffickingToolset};
use adspace_agent::tools::ui::ui_toolset;
use adspace_agent::tools::utilities::utilities_toolset;
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
        lazy_split_writes: true,
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

/// Minimal context used only to enumerate toolset contents.
struct ListingContext {
    content: Content,
}

impl ReadonlyContext for ListingContext {
    fn invocation_id(&self) -> &str {
        "smoke-list-tools"
    }
    fn agent_name(&self) -> &str {
        APP_NAME
    }
    fn user_id(&self) -> &str {
        "tester"
    }
    fn app_name(&self) -> &str {
        APP_NAME
    }
    fn session_id(&self) -> &str {
        "smoke-list-tools"
    }
    fn branch(&self) -> &str {
        ""
    }
    fn user_content(&self) -> &Content {
        &self.content
    }
}

async fn tools_of(toolset: &dyn Toolset) -> Vec<Arc<dyn Tool>> {
    let ctx: Arc<dyn ReadonlyContext> = Arc::new(ListingContext {
        content: Content::new("user"),
    });
    toolset.tools(ctx).await.expect("toolset enumerates")
}

/// Cheap, offline checks on the locally built toolsets: tool counts, the
/// CM360 trafficking tools, and that read-only/concurrency flags match what
/// each tool actually does (adk runs non-read-only tools serially).
#[tokio::test]
async fn local_toolsets_expose_expected_tools_and_flags() {
    let credentials = build_credential_manager(&settings());

    // BigQuery: the four adk-compatible tools, acting with the user's OAuth
    // token. In read-only mode (the default) every tool is read-only and
    // concurrency-safe and the toolset asks for the read-only scope only.
    let bigquery = BigQueryToolset::new(credentials.clone()).with_project(Some("proj".into()));
    assert_eq!(bigquery.name(), "bigquery");
    assert_eq!(
        agent::list_tool_names(&bigquery).await.unwrap(),
        vec![
            "bigquery_list_datasets",
            "bigquery_list_tables",
            "bigquery_get_table_schema",
            "bigquery_execute_sql",
        ]
    );
    assert_eq!(
        bigquery.scopes(),
        vec!["https://www.googleapis.com/auth/bigquery.readonly"]
    );
    for tool in tools_of(&bigquery).await {
        assert!(tool.is_read_only(), "{} is read-only", tool.name());
        assert!(
            tool.is_concurrency_safe(),
            "{} is concurrency-safe",
            tool.name()
        );
    }
    let writable = BigQueryToolset::new(credentials.clone()).with_read_only(false);
    assert_eq!(
        writable.scopes(),
        vec!["https://www.googleapis.com/auth/bigquery"]
    );
    let execute = tools_of(&writable)
        .await
        .into_iter()
        .find(|t| t.name() == "bigquery_execute_sql")
        .expect("bigquery_execute_sql present");
    assert!(!execute.is_read_only(), "DML allowed ⇒ not read-only");

    // CM360 trafficking: exactly the two workflow tools, both mutating
    // (they stage artifacts and write to CM360), so neither is read-only.
    let cm360 = Cm360TraffickingToolset::new(Cm360Services {
        credentials,
        api_override: None,
    });
    assert_eq!(cm360.name(), "cm360_trafficking");
    assert_eq!(
        agent::list_tool_names(&cm360).await.unwrap(),
        vec!["parse_sheet_tool", "traffic_campaigns_in_cm360_tool"]
    );
    for tool in tools_of(&cm360).await {
        assert!(
            !tool.is_read_only(),
            "{} mutates session/CM360 state and must not be read-only",
            tool.name()
        );
    }
    let (count, bytes) = agent::declaration_size(&cm360).await.unwrap();
    assert_eq!(count, 2);
    assert!(bytes > 0, "CM360 declarations serialise");

    // Utilities: the artifact listing tool is read-only and concurrency-safe.
    let utilities = utilities_toolset();
    let utility_tools = tools_of(&utilities).await;
    let list_files = utility_tools
        .iter()
        .find(|t| t.name() == "list_user_files")
        .expect("list_user_files present");
    assert!(list_files.is_read_only());
    assert!(list_files.is_concurrency_safe());

    // UI rendering: every `render_*` tool only emits UI and is read-only.
    let ui = ui_toolset();
    let ui_tools = tools_of(&ui).await;
    assert!(!ui_tools.is_empty(), "UI toolset exposes render tools");
    for tool in &ui_tools {
        assert!(
            tool.name().starts_with("render_"),
            "unexpected UI tool {}",
            tool.name()
        );
        assert!(tool.is_read_only(), "{} should be read-only", tool.name());
    }
    let (ui_count, _) = agent::declaration_size(&ui).await.unwrap();
    assert_eq!(ui_count, ui_tools.len());
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
