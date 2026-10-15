//! The AdSpace Agent main application: builds the root agent and runtime.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use adk_rust::agent::{LlmAgentBuilder, LlmEventSummarizer};
use adk_rust::artifact::ArtifactService;
use adk_rust::memory::{MemoryService, MemoryServiceAdapter};
use adk_rust::model::GeminiModel;
use adk_rust::session::SessionService;
use adk_rust::tool::BasicToolset;
use adk_rust::tool::bigquery::BigQueryToolset;
use adk_rust::tool::memory::PreloadMemoryTool;
use adk_rust::{
    Agent, BeforeAgentCallback, CacheCapable, CallbackContext, Content, ContextCacheConfig,
    EventStream, EventsCompactionConfig, InvocationContext, Part, Result as AdkResult, SessionId,
    Tool, ToolExecutionStrategy, Toolset, UserId,
};
use anyhow::Context;
use async_trait::async_trait;
use futures::StreamExt;

use crate::auth::{
    AdcTokenProvider, GoogleOAuthFlow, OAuthClientConfig, SessionCredentialManager, SessionRef,
};
use crate::config::{
    self, APP_NAME, ActiveModels, DISPLAY_VIDEO_360_TOOL_FILTER, DRIVE_TOOL_FILTER,
    GOOGLE_ADS_API_VERSION, ModelController, STORAGE_TOOL_FILTER, Settings, SimpleModelController,
    YOUTUBE_TOOL_FILTER, validate_model_name,
};
use crate::gcs::GcsClient;
use crate::genai::GenAiClient;
use crate::google_api::{DiscoveryFetcher, GoogleApiToolset, GoogleApiToolsetConfig, SpecOptions};
use crate::services;
use crate::tools::cm360_trafficking::{Cm360Services, Cm360TraffickingToolset};
use crate::tools::data_analysis::{PythonExecutorKind, data_analysis_toolset};
use crate::tools::gemini_builtins::{GeminiBuiltinTool, gemini_builtin_toolset};
use crate::tools::genai::google_genai_toolset_with_controller;
use crate::tools::lazy::{EnableToolsetsTool, LazyToolset, LazyToolsetEntry};
use crate::tools::skills::{SkillSources, SkillsToolset};
use crate::tools::ui::ui_toolset;
use crate::tools::utilities::utilities_toolset;

/// Agent description (shown in the UI and A2A card).
pub const AGENT_DESCRIPTION: &str = "AdSpace Agent is designed to provide a standardized way to integrate an LLM with Google Ads, \
Display & Video 360, Campaign Manager 360, Search Ads 360, YouTube, Google Drive, and Google Cloud storage to form a more \
comprehensive campaign and marketing plan for agencies.";

/// Root agent instruction.
pub const AGENT_INSTRUCTION: &str = "You are a helpful agent who can answer user questions about ads, creatives, data science, \
performance, analytics, and campaigns. NOTE: Tools for Search Ads 360 and tools for Google Ads are distinct. DO NOT group tools \
from one platform with tools from another platform if asked what tools are available. When asked for multiple pieces of \
information or to perform multiple actions, always try to call functions in parallel if possible. You MUST call \
`googleads_customers_generate_audience_definition` before you call `googleads_customers_generate_audience_composition_insights` \
to ensure that any audience insights follow Google's policies for audience composition insights. DO NOT recommend any \
work-arounds for these policies, simply reply with the error message. \
AUTHORIZATION: Google API tools act on behalf of the signed-in user. When a tool returns status `AUTH_REQUIRED`, show the user \
the `auth_url` as a clickable link, ask them to complete the authorization, and retry the same tool call after they confirm. \
FILES: files the user uploads are saved as session artifacts; use `list_user_files` to see them and `load_artifacts` to read them. \
RESEARCH: use `google_search` for current information from the web, `url_context` to read pages the user links to, and \
`google_maps` for places, routes and local context. \
UI RENDERING (`render_*` tools): when presenting campaign metrics, rankings, time-series trends, or multi-metric comparisons, \
use `render_chart` (bar/line/area/pie), `render_table` (with `sortable: true` and `striped: true`), or `render_layout` \
(combining `stats`, `chart`, and `table` sections into an executive dashboard) so the user gets an interactive visual in the UI, \
followed by a concise written takeaway (never dump raw component JSON in text). Use `render_form` when collecting structured \
campaign parameters or missing IDs, and `render_confirm` / `render_card` before executing write/trafficking actions.";

fn intern_model_name(name: &str) -> &'static str {
    static INTERNED: std::sync::LazyLock<Mutex<HashSet<&'static str>>> =
        std::sync::LazyLock::new(|| Mutex::new(HashSet::new()));
    let mut set = INTERNED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(&existing) = set.get(name) {
        return existing;
    }
    let leaked: &'static str = Box::leak(name.to_string().into_boxed_str());
    set.insert(leaked);
    leaked
}

type LlmFactory = Box<
    dyn Fn(&str) -> anyhow::Result<(Arc<dyn adk_rust::Llm>, Option<Arc<dyn CacheCapable>>)>
        + Send
        + Sync,
>;

/// Hot-swappable model configuration implementing [`adk_rust::Llm`],
/// [`CacheCapable`], and [`ModelController`].
pub struct RuntimeModelConfig {
    schema_source: Arc<dyn adk_rust::Llm>,
    llm_name: std::sync::RwLock<&'static str>,
    llm: std::sync::RwLock<Arc<dyn adk_rust::Llm>>,
    cache_capable: std::sync::RwLock<Option<Arc<dyn CacheCapable>>>,
    video_model: std::sync::RwLock<String>,
    image_model: std::sync::RwLock<String>,
    factory: LlmFactory,
}

impl RuntimeModelConfig {
    /// Creates a dynamic model configuration backed by [`GeminiModel::from_env`].
    pub fn from_gemini(
        initial: Arc<GeminiModel>,
        video_model: impl Into<String>,
        image_model: impl Into<String>,
    ) -> Self {
        let initial_name = intern_model_name(adk_rust::Llm::name(initial.as_ref()));
        let llm: Arc<dyn adk_rust::Llm> = initial.clone();
        let cache: Arc<dyn CacheCapable> = initial;
        Self {
            schema_source: llm.clone(),
            llm_name: std::sync::RwLock::new(initial_name),
            llm: std::sync::RwLock::new(llm),
            cache_capable: std::sync::RwLock::new(Some(cache)),
            video_model: std::sync::RwLock::new(video_model.into()),
            image_model: std::sync::RwLock::new(image_model.into()),
            factory: Box::new(|model_name| {
                let built = Arc::new(
                    GeminiModel::from_env(model_name).map_err(|e| anyhow::anyhow!("{e}"))?,
                );
                Ok((
                    built.clone() as Arc<dyn adk_rust::Llm>,
                    Some(built as Arc<dyn CacheCapable>),
                ))
            }),
        }
    }
}

impl ModelController for RuntimeModelConfig {
    fn active_models(&self) -> ActiveModels {
        ActiveModels {
            model: (*self.llm_name.read().unwrap_or_else(|e| e.into_inner())).to_string(),
            video_model: self
                .video_model
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
            image_model: self
                .image_model
                .read()
                .unwrap_or_else(|e| e.into_inner())
                .clone(),
        }
    }

    fn update_models(
        &self,
        model: Option<&str>,
        video_model: Option<&str>,
        image_model: Option<&str>,
    ) -> anyhow::Result<ActiveModels> {
        let validated_model = model.map(validate_model_name).transpose()?;
        let validated_video = video_model.map(validate_model_name).transpose()?;
        let validated_image = image_model.map(validate_model_name).transpose()?;

        if let Some(new_model) = validated_model {
            let current = *self.llm_name.read().unwrap_or_else(|e| e.into_inner());
            if new_model != current {
                let (new_llm, new_cache) = (self.factory)(&new_model)?;
                *self.llm.write().unwrap_or_else(|e| e.into_inner()) = new_llm;
                *self
                    .cache_capable
                    .write()
                    .unwrap_or_else(|e| e.into_inner()) = new_cache;
                *self.llm_name.write().unwrap_or_else(|e| e.into_inner()) =
                    intern_model_name(&new_model);
                tracing::info!("Switched active agent LLM model to {new_model}");
            }
        }
        if let Some(new_video) = validated_video {
            let mut guard = self.video_model.write().unwrap_or_else(|e| e.into_inner());
            if *guard != new_video {
                tracing::info!("Switched active video model to {new_video}");
                *guard = new_video;
            }
        }
        if let Some(new_image) = validated_image {
            let mut guard = self.image_model.write().unwrap_or_else(|e| e.into_inner());
            if *guard != new_image {
                tracing::info!("Switched active image model to {new_image}");
                *guard = new_image;
            }
        }
        Ok(self.active_models())
    }
}

#[async_trait]
impl adk_rust::Llm for RuntimeModelConfig {
    fn name(&self) -> &str {
        *self.llm_name.read().unwrap_or_else(|e| e.into_inner())
    }

    fn schema_adapter(&self) -> &dyn adk_rust::SchemaAdapter {
        self.schema_source.schema_adapter()
    }

    fn uses_interactions_api(&self) -> bool {
        self.llm
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .uses_interactions_api()
    }

    async fn generate_content(
        &self,
        mut req: adk_rust::LlmRequest,
        stream: bool,
    ) -> AdkResult<adk_rust::LlmResponseStream> {
        let active = self.llm.read().unwrap_or_else(|e| e.into_inner()).clone();
        let model_name = active.name().to_string();
        req.model = model_name.clone();
        let usage_span = adk_rust::telemetry::llm_generate_span("gemini", &model_name, stream);
        let response_stream = active.generate_content(req, stream).await?;
        Ok(track_stream_token_usage(
            response_stream,
            usage_span,
            model_name,
        ))
    }
}

/// Wraps an [`adk_rust::LlmResponseStream`] to record OpenTelemetry GenAI token
/// usage via [`adk_rust::telemetry::record_llm_usage`] and enrich each
/// [`adk_rust::UsageMetadata`] with the active model name, OTel `gen_ai.usage.*`
/// attributes, and estimated USD cost.
fn track_stream_token_usage(
    stream: adk_rust::LlmResponseStream,
    span: tracing::Span,
    model_name: String,
) -> adk_rust::LlmResponseStream {
    let tracked = stream.map(move |mut item| {
        if let Ok(ref mut response) = item
            && let Some(ref mut usage) = response.usage_metadata
        {
            let llm_usage = adk_rust::telemetry::LlmUsage {
                input_tokens: usage.prompt_token_count,
                output_tokens: usage.candidates_token_count,
                total_tokens: usage.total_token_count,
                cache_read_tokens: usage.cache_read_input_token_count,
                cache_creation_tokens: usage.cache_creation_input_token_count,
                thinking_tokens: usage.thinking_token_count,
                audio_input_tokens: usage.audio_input_token_count,
                audio_output_tokens: usage.audio_output_token_count,
            };
            {
                let _enter = span.enter();
                adk_rust::telemetry::record_llm_usage(&llm_usage);
            }
            adk_rust::telemetry::record_llm_usage(&llm_usage);

            let estimated_cost = config::estimate_usage_cost_usd(&model_name, usage);
            if usage.cost.is_none() {
                usage.cost = Some(estimated_cost);
            }

            let mut meta_obj = match usage.provider_usage.take() {
                Some(serde_json::Value::Object(map)) => map,
                Some(other) => {
                    let mut map = serde_json::Map::new();
                    map.insert("raw".to_string(), other);
                    map
                }
                None => serde_json::Map::new(),
            };
            meta_obj.insert("model".to_string(), serde_json::json!(model_name));
            meta_obj.insert("gen_ai.system".to_string(), serde_json::json!("gemini"));
            meta_obj.insert(
                "gen_ai.request.model".to_string(),
                serde_json::json!(model_name),
            );
            meta_obj.insert(
                "gen_ai.usage.input_tokens".to_string(),
                serde_json::json!(llm_usage.input_tokens),
            );
            meta_obj.insert(
                "gen_ai.usage.output_tokens".to_string(),
                serde_json::json!(llm_usage.output_tokens),
            );
            meta_obj.insert(
                "gen_ai.usage.total_tokens".to_string(),
                serde_json::json!(llm_usage.total_tokens),
            );
            if let Some(v) = llm_usage.cache_read_tokens {
                meta_obj.insert(
                    "gen_ai.usage.cache_read_tokens".to_string(),
                    serde_json::json!(v),
                );
            }
            if let Some(v) = llm_usage.cache_creation_tokens {
                meta_obj.insert(
                    "gen_ai.usage.cache_creation_tokens".to_string(),
                    serde_json::json!(v),
                );
            }
            if let Some(v) = llm_usage.thinking_tokens {
                meta_obj.insert(
                    "gen_ai.usage.thinking_tokens".to_string(),
                    serde_json::json!(v),
                );
            }
            if let Some(v) = llm_usage.audio_input_tokens {
                meta_obj.insert(
                    "gen_ai.usage.audio_input_tokens".to_string(),
                    serde_json::json!(v),
                );
            }
            if let Some(v) = llm_usage.audio_output_tokens {
                meta_obj.insert(
                    "gen_ai.usage.audio_output_tokens".to_string(),
                    serde_json::json!(v),
                );
            }
            usage.provider_usage = Some(serde_json::Value::Object(meta_obj));
        }
        item
    });
    Box::pin(tracked)
}

#[async_trait]
impl CacheCapable for RuntimeModelConfig {
    async fn create_cache(
        &self,
        system_instruction: &str,
        tools: &std::collections::HashMap<String, serde_json::Value>,
        ttl_seconds: u32,
    ) -> AdkResult<String> {
        let active = self
            .cache_capable
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        match active {
            Some(cache) => {
                cache
                    .create_cache(system_instruction, tools, ttl_seconds)
                    .await
            }
            None => Err(adk_rust::AdkError::model(
                "Active model does not support context caching",
            )),
        }
    }

    async fn delete_cache(&self, name: &str) -> AdkResult<()> {
        let active = self
            .cache_capable
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        match active {
            Some(cache) => cache.delete_cache(name).await,
            None => Ok(()),
        }
    }
}

/// Everything needed to run the agent (console, server or Agent Engine).
pub struct AgentRuntime {
    /// Resolved settings.
    pub settings: Settings,
    /// Root agent (LLM agent wrapped with memory persistence).
    pub agent: Arc<dyn Agent>,
    /// Initial Gemini model.
    pub model: Arc<GeminiModel>,
    /// Hot-swappable model controller (LLM, video, and image models).
    pub models: Arc<RuntimeModelConfig>,
    /// Session-bound Google credentials (OAuth client + state signing).
    pub credentials: Arc<SessionCredentialManager>,
    /// Session service.
    pub session_service: Arc<dyn SessionService>,
    /// Artifact service.
    pub artifact_service: Arc<dyn ArtifactService>,
    /// Memory service.
    pub memory_service: Arc<dyn MemoryService>,
}

impl AgentRuntime {
    /// Events-compaction configuration derived from the settings.
    pub fn compaction_config(&self) -> EventsCompactionConfig {
        let c = self.settings.compaction;
        EventsCompactionConfig {
            compaction_interval: c.compaction_interval.min(u32::MAX as u64) as u32,
            overlap_size: c.overlap_size.min(u32::MAX as u64) as u32,
            summarizer: Arc::new(LlmEventSummarizer::new(self.models.clone())),
        }
    }

    /// Context-cache configuration (mirrors the Python `ContextCacheConfig`).
    pub fn context_cache_config(&self) -> ContextCacheConfig {
        ContextCacheConfig {
            min_tokens: config::env_u64_or("CONTEXT_CACHE_MIN_TOKENS", 2048) as u32,
            ttl_seconds: config::env_u64_or("CONTEXT_CACHE_TTL_SECONDS", 600) as u32,
            cache_intervals: config::env_u64_or("CONTEXT_CACHE_INTERVALS", 5) as u32,
        }
    }

    /// Memory bound to a specific app/user, as the runner expects it.
    pub fn memory_for(&self, user_id: &str) -> Arc<dyn adk_rust::Memory> {
        Arc::new(MemoryServiceAdapter::new(
            self.memory_service.clone(),
            APP_NAME,
            user_id,
        ))
    }

    /// A [`TurnRunner`] configured like the server's request runner.
    pub fn turn_runner(&self) -> TurnRunner {
        TurnRunner::new(
            self.agent.clone(),
            self.session_service.clone(),
            self.artifact_service.clone(),
            self.memory_service.clone(),
        )
        .with_compaction(self.compaction_config())
        .with_context_cache(
            self.context_cache_config(),
            self.models.clone() as Arc<dyn CacheCapable>,
        )
    }
}

/// Runs agent turns outside of an HTTP request — e.g. to resume a chat after
/// the user granted Google access — with the same services and settings the
/// server's request runner uses, so the events land in the session exactly as
/// if the user had typed the message.
#[derive(Clone)]
pub struct TurnRunner {
    agent: Arc<dyn Agent>,
    session_service: Arc<dyn SessionService>,
    artifact_service: Arc<dyn ArtifactService>,
    memory_service: Arc<dyn MemoryService>,
    compaction: Option<EventsCompactionConfig>,
    context_cache: Option<(ContextCacheConfig, Arc<dyn CacheCapable>)>,
}

impl TurnRunner {
    /// Creates a runner over the given agent and services.
    pub fn new(
        agent: Arc<dyn Agent>,
        session_service: Arc<dyn SessionService>,
        artifact_service: Arc<dyn ArtifactService>,
        memory_service: Arc<dyn MemoryService>,
    ) -> Self {
        Self {
            agent,
            session_service,
            artifact_service,
            memory_service,
            compaction: None,
            context_cache: None,
        }
    }

    /// Enables events compaction.
    pub fn with_compaction(mut self, config: EventsCompactionConfig) -> Self {
        self.compaction = Some(config);
        self
    }

    /// Enables context caching.
    pub fn with_context_cache(
        mut self,
        config: ContextCacheConfig,
        model: Arc<dyn CacheCapable>,
    ) -> Self {
        self.context_cache = Some((config, model));
        self
    }

    /// Runs one user turn with `text` in `session` to completion and returns
    /// the number of events the agent produced.
    ///
    /// # Errors
    ///
    /// Returns an error when the session identity is invalid, the runner
    /// cannot start, or the turn fails.
    pub async fn run_text(&self, session: &SessionRef, text: &str) -> anyhow::Result<usize> {
        let mut builder = adk_rust::runner::Runner::builder()
            .app_name(session.app_name.clone())
            .agent(self.agent.clone())
            .session_service(self.session_service.clone())
            .artifact_service(self.artifact_service.clone())
            .memory_service(Arc::new(MemoryServiceAdapter::new(
                self.memory_service.clone(),
                session.app_name.clone(),
                session.user_id.clone(),
            )));
        if let Some(compaction) = &self.compaction {
            builder = builder.compaction_config(compaction.clone());
        }
        if let Some((config, model)) = &self.context_cache {
            builder = builder
                .context_cache_config(config.clone())
                .cache_capable(model.clone());
        }
        let runner = builder.build()?;
        let mut events = runner
            .run(
                UserId::new(session.user_id.clone())?,
                SessionId::new(session.session_id.clone())?,
                Content::new("user").with_text(text),
            )
            .await?;
        let mut count = 0usize;
        while let Some(event) = events.next().await {
            event?;
            count += 1;
        }
        Ok(count)
    }
}

/// Builds the session-credential manager from the environment.
///
/// Credentials themselves live in each chat session's state; the manager only
/// holds the OAuth client configuration.
pub fn build_credential_manager(settings: &Settings) -> Arc<SessionCredentialManager> {
    let config = OAuthClientConfig {
        client_id: settings.client_id.clone(),
        client_secret: settings.client_secret.clone(),
        ..OAuthClientConfig::from_env()
    };
    let manager = Arc::new(SessionCredentialManager::new(GoogleOAuthFlow::new(config)));
    manager.set_model_controller(Arc::new(SimpleModelController::new(
        settings.model.clone(),
        settings.video_model.clone(),
        settings.image_model.clone(),
    )));
    manager
}

/// Builds the Discovery-based Google API toolset configs for the enabled
/// toolsets (the counterpart of `_build_toolset_factories`).
pub fn google_api_toolset_configs(settings: &Settings) -> Vec<GoogleApiToolsetConfig> {
    let options = SpecOptions::from_env();
    let mut configs = Vec::new();
    for key in &settings.enabled_toolsets {
        let config = match key.as_str() {
            "bid_manager" => {
                GoogleApiToolsetConfig::new("bid_manager", "doubleclickbidmanager", "v2")
            }
            // No scope override (the Python agent needed one because the ADK
            // took the API's first scope): each method's own Discovery scope
            // is used, so trafficking calls ask for `dfatrafficking` only.
            "campaign_manager_360" => {
                GoogleApiToolsetConfig::new("campaign_manager_360", "dfareporting", "v5")
            }
            "display_video_360" => {
                GoogleApiToolsetConfig::new("display_video_360", "displayvideo", "v4")
                    .with_tool_filter(DISPLAY_VIDEO_360_TOOL_FILTER.iter().copied())
            }
            "drive" => GoogleApiToolsetConfig::new("drive", "drive", "v3")
                .with_tool_filter(DRIVE_TOOL_FILTER.iter().copied()),
            "google_ads" => {
                let mut c = GoogleApiToolsetConfig::new("google_ads", "googleads", GOOGLE_ADS_API_VERSION)
                    .with_discovery_url(format!("https://googleads.googleapis.com/$discovery/rest?version={GOOGLE_ADS_API_VERSION}"))
                    .with_header("developer-token", settings.google_ads_developer_token.clone())
                    .with_optional_tool_filter(settings.google_ads_tool_filter.clone());
                if let Some(login) = &settings.google_ads_login_customer_id {
                    c = c.with_header("login-customer-id", login.clone());
                }
                c
            }
            "merchant_center_inventories" => GoogleApiToolsetConfig::new(
                "merchant_center_inventories",
                "merchantapi",
                "inventories_v1",
            ),
            "merchant_center_products" => GoogleApiToolsetConfig::new(
                "merchant_center_products",
                "merchantapi",
                "products_v1",
            ),
            "merchant_center_reports" => {
                GoogleApiToolsetConfig::new("merchant_center_reports", "merchantapi", "reports_v1")
            }
            "search_ads_360" => GoogleApiToolsetConfig::new("search_ads_360", "searchads360", "v0"),
            "storage" => GoogleApiToolsetConfig::new("storage", "storage", "v1")
                .with_tool_filter(STORAGE_TOOL_FILTER.iter().copied()),
            "youtube" => GoogleApiToolsetConfig::new("youtube", "youtube", "v3")
                .with_tool_filter(YOUTUBE_TOOL_FILTER.iter().copied()),
            // bigquery / google_genai are native toolsets, not Discovery based.
            _ => continue,
        };
        configs.push(config.with_spec_options(options));
    }
    configs
}

/// Loads every enabled Discovery-based toolset concurrently. Toolsets whose
/// Discovery document cannot be fetched are skipped with a warning.
async fn load_google_api_toolsets(
    settings: &Settings,
    credentials: &Arc<SessionCredentialManager>,
) -> Vec<Arc<GoogleApiToolset>> {
    let fetcher = DiscoveryFetcher::new();
    let futures = google_api_toolset_configs(settings)
        .into_iter()
        .map(|config| {
            let fetcher = fetcher.clone();
            let credentials = credentials.clone();
            async move {
                let name = config.toolset_name.clone();
                match GoogleApiToolset::load(config, &fetcher, credentials).await {
                    Ok(toolset) => Some(Arc::new(toolset)),
                    Err(e) => {
                        tracing::warn!("Skipping toolset '{name}': {e:#}");
                        None
                    }
                }
            }
        });
    futures::future::join_all(futures)
        .await
        .into_iter()
        .flatten()
        .collect()
}

/// Saves files attached to the user message as session artifacts (the Rust
/// counterpart of `SaveFilesAsArtifactsPlugin`).
pub fn save_uploaded_files_callback() -> BeforeAgentCallback {
    Box::new(|ctx: Arc<dyn CallbackContext>| {
        Box::pin(async move {
            let Some(artifacts) = ctx.artifacts() else {
                return Ok(None);
            };
            let parts: Vec<(usize, Part)> = ctx
                .user_content()
                .parts
                .iter()
                .enumerate()
                .filter(|(_, p)| matches!(p, Part::InlineData { .. }))
                .map(|(i, p)| (i, p.clone()))
                .collect();
            for (index, part) in parts {
                let Part::InlineData {
                    mime_type,
                    data,
                    uri,
                    ..
                } = &part
                else {
                    continue;
                };
                let filename = uri
                    .clone()
                    .filter(|u| !u.is_empty() && !u.contains('/'))
                    .unwrap_or_else(|| {
                        format!(
                            "artifact_{}_{index}.{}",
                            ctx.invocation_id(),
                            extension_for(mime_type)
                        )
                    });
                // Text-like uploads are stored as text so the built-in
                // `load_artifacts` tool returns readable content (not base64).
                let stored = match (is_text_mime(mime_type), std::str::from_utf8(data)) {
                    (true, Ok(text)) => Part::Text {
                        text: text.to_string(),
                    },
                    _ => part.clone(),
                };
                match artifacts.save(&filename, &stored).await {
                    Ok(version) => tracing::info!(
                        "Saved uploaded file as artifact '{filename}' (version {version})"
                    ),
                    Err(e) => {
                        tracing::warn!("Failed to save uploaded file as artifact '{filename}': {e}")
                    }
                }
            }
            Ok(None)
        })
    })
}

/// Whether a MIME type denotes UTF-8 text content.
pub fn is_text_mime(mime_type: &str) -> bool {
    let base = mime_type.split(';').next().unwrap_or_default().trim();
    base.starts_with("text/")
        || matches!(
            base,
            "application/json" | "application/csv" | "application/xml" | "application/x-yaml"
        )
}

/// File extension for a MIME type (used when uploads carry no name).
pub fn extension_for(mime_type: &str) -> &'static str {
    match mime_type.split(';').next().unwrap_or_default().trim() {
        "text/csv" | "application/csv" | "application/vnd.ms-excel" => "csv",
        "text/plain" => "txt",
        "text/markdown" => "md",
        "application/json" => "json",
        "application/pdf" => "pdf",
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "video/mp4" => "mp4",
        "audio/mpeg" => "mp3",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        _ => "bin",
    }
}

/// Wraps an agent so that, after every invocation, the session conversation
/// is written to the memory service (the counterpart of the Python
/// `auto_save_session_to_memory_callback`).
pub struct MemorySavingAgent {
    inner: Arc<dyn Agent>,
    memory: Arc<dyn MemoryService>,
    sub_agents: Vec<Arc<dyn Agent>>,
}

impl MemorySavingAgent {
    /// Wraps `inner`.
    pub fn new(inner: Arc<dyn Agent>, memory: Arc<dyn MemoryService>) -> Self {
        Self {
            inner,
            memory,
            sub_agents: Vec::new(),
        }
    }
}

#[async_trait]
impl Agent for MemorySavingAgent {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn sub_agents(&self) -> &[Arc<dyn Agent>] {
        &self.sub_agents
    }

    async fn run(&self, ctx: Arc<dyn InvocationContext>) -> AdkResult<EventStream> {
        let history: Vec<Content> = ctx.session().conversation_history();
        let user_content = ctx.user_content().clone();
        let (app_name, user_id, session_id, agent_name) = (
            ctx.app_name().to_string(),
            ctx.user_id().to_string(),
            ctx.session_id().to_string(),
            self.inner.name().to_string(),
        );
        let collected: Arc<Mutex<Vec<Content>>> = Arc::new(Mutex::new(Vec::new()));
        let memory = self.memory.clone();

        let stream = self.inner.run(ctx).await?;
        let sink = collected.clone();
        let forwarded = stream.inspect(move |event| {
            if let Ok(event) = event
                && !event.llm_response.partial
                && let Some(content) = event.content()
                && content.parts.iter().any(|p| matches!(p, Part::Text { .. }))
            {
                sink.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .push(content.clone());
            }
        });
        let tail = futures::stream::once(async move {
            let mut contents = history;
            contents.push(user_content);
            contents.extend(
                collected
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .drain(..),
            );
            let mut seen = HashSet::new();
            let entries: Vec<adk_rust::memory::MemoryEntry> = contents
                .into_iter()
                .filter(|c| c.parts.iter().any(|p| matches!(p, Part::Text { .. })))
                .filter(|c| seen.insert(serde_json::to_string(c).unwrap_or_default()))
                .map(|content| {
                    let author = if content.role == "user" {
                        "user".to_string()
                    } else {
                        agent_name.clone()
                    };
                    adk_rust::memory::MemoryEntry {
                        content,
                        author,
                        timestamp: chrono::Utc::now(),
                    }
                })
                .collect();
            if let Err(e) = memory
                .add_session(&app_name, &user_id, &session_id, entries)
                .await
            {
                tracing::warn!("Failed to save session {session_id} to memory: {e}");
            }
        })
        .filter_map(|()| async { None });
        Ok(Box::pin(forwarded.chain(tail)))
    }
}

/// Creates and configures the AdSpace Agent.
///
/// # Errors
///
/// Returns an error when the model or a required service cannot be built.
pub async fn create_agent(
    settings: &Settings,
    model: Arc<dyn adk_rust::Llm>,
    credentials: Arc<SessionCredentialManager>,
    memory_service: Arc<dyn MemoryService>,
) -> anyhow::Result<Arc<dyn Agent>> {
    let adc = Arc::new(AdcTokenProvider::new(&[]).ok());
    let gcs_client = adc.as_ref().clone().map(|p| GcsClient::new(Arc::new(p)));

    let mut toolsets: Vec<Arc<dyn Toolset>> = Vec::new();

    // Always-on toolsets (DataAnalysis, Utilities, Skills; PreloadMemory and
    // LoadArtifacts are part of Utilities / model callbacks).
    match data_analysis_toolset() {
        Ok(ts) => toolsets.push(Arc::new(ts)),
        Err(e) => tracing::warn!("Data analysis toolset disabled: {e:#}"),
    }
    // Gemini server-side tools (search, URL context, maps, code execution).
    // Code execution is skipped here when it is already the Python backend
    // (`PYTHON_EXECUTOR=gemini`, the default).
    let exclude = match PythonExecutorKind::from_env() {
        Ok(PythonExecutorKind::Gemini) | Err(_) => vec![GeminiBuiltinTool::CodeExecution],
        Ok(_) => vec![],
    };
    match gemini_builtin_toolset(&exclude) {
        Ok(Some(ts)) => toolsets.push(Arc::new(ts)),
        Ok(None) => {}
        Err(e) => tracing::warn!("Gemini built-in tools disabled: {e:#}"),
    }
    toolsets.push(Arc::new(utilities_toolset()));
    toolsets.push(Arc::new(ui_toolset()));
    toolsets.push(Arc::new(
        SkillsToolset::load(SkillSources::from_env(gcs_client.clone())).await,
    ));

    if settings.is_toolset_enabled("campaign_manager_360") {
        toolsets.push(Arc::new(Cm360TraffickingToolset::new(Cm360Services {
            credentials: credentials.clone(),
            api_override: None,
        })));
    }

    let mut deferred: Vec<LazyToolsetEntry> = Vec::new();
    for toolset in load_google_api_toolsets(settings, &credentials).await {
        if settings.lazy_toolsets.iter().any(|n| n == toolset.name()) {
            deferred.push(LazyToolsetEntry {
                name: toolset.name().to_string(),
                summary: config::toolset_summary(toolset.name()).to_string(),
                tool_count: toolset.tool_names().len(),
            });
            toolsets.push(Arc::new(LazyToolset::new(toolset)));
        } else {
            toolsets.push(toolset);
        }
    }
    if !deferred.is_empty() {
        tracing::info!(
            "Deferred toolsets (loaded on demand via enable_toolsets): {}",
            deferred
                .iter()
                .map(|d| format!("{} ({} tools)", d.name, d.tool_count))
                .collect::<Vec<_>>()
                .join(", ")
        );
        toolsets.push(Arc::new(BasicToolset::new(
            "lazy_toolsets",
            vec![Arc::new(EnableToolsetsTool::new(deferred))],
        )));
    }

    if settings.is_toolset_enabled("bigquery") {
        let bq = match &settings.google_cloud_project {
            Some(project) => BigQueryToolset::with_project(project.clone()),
            None => BigQueryToolset::new(),
        };
        toolsets.push(Arc::new(bq.with_read_only(
            config::env_or("BIGQUERY_READ_ONLY", "true").eq_ignore_ascii_case("true"),
        )));
    }

    if settings.is_toolset_enabled("google_genai") {
        match GenAiClient::from_settings(settings) {
            Ok(client) => toolsets.push(Arc::new(google_genai_toolset_with_controller(
                client,
                credentials.model_controller(),
            ))),
            Err(e) => tracing::warn!("Google GenAI toolset disabled: {e:#}"),
        }
    }

    let preload = PreloadMemoryTool::builder()
        .memory_service(memory_service.clone())
        .max_results(5)
        .build()?;

    let mut builder = LlmAgentBuilder::new(APP_NAME)
        .description(AGENT_DESCRIPTION)
        .instruction(AGENT_INSTRUCTION)
        .model(model)
        .tool_execution_strategy(ToolExecutionStrategy::Auto)
        .before_callback(save_uploaded_files_callback())
        .before_model_callback(preload.into_before_model_callback());
    for toolset in toolsets {
        builder = builder.toolset(toolset);
    }
    let llm_agent = builder.build().context("building the AdSpace LLM agent")?;
    Ok(Arc::new(MemorySavingAgent::new(
        Arc::new(llm_agent),
        memory_service,
    )))
}

/// Builds the Gemini model from the environment (`MODEL`,
/// `GOOGLE_GENAI_USE_VERTEXAI`, `GOOGLE_CLOUD_PROJECT`, `GOOGLE_CLOUD_LOCATION`
/// or `GOOGLE_API_KEY`).
///
/// # Errors
///
/// Returns an error when the model configuration is incomplete.
pub fn build_model(settings: &Settings) -> anyhow::Result<Arc<GeminiModel>> {
    let model = GeminiModel::from_env(&settings.model).map_err(|e| anyhow::anyhow!("{e}"))?;
    Ok(Arc::new(model))
}

/// Builds the full runtime: settings, services, credentials and agent.
///
/// # Errors
///
/// Returns an error when configuration is invalid or a service fails to start.
pub async fn build_runtime() -> anyhow::Result<AgentRuntime> {
    let settings = Settings::from_env()?;
    let model = build_model(&settings)?;
    let models = Arc::new(RuntimeModelConfig::from_gemini(
        model.clone(),
        settings.video_model.clone(),
        settings.image_model.clone(),
    ));
    let credentials = build_credential_manager(&settings);
    credentials.set_model_controller(models.clone());
    let session_service = services::build_session_service().await?;
    let artifact_service = services::build_artifact_service()?;
    let memory_service = services::build_memory_service()?;
    let agent = create_agent(
        &settings,
        models.clone() as Arc<dyn adk_rust::Llm>,
        credentials.clone(),
        memory_service.clone(),
    )
    .await?;
    Ok(AgentRuntime {
        settings,
        agent,
        model,
        models,
        credentials,
        session_service,
        artifact_service,
        memory_service,
    })
}

/// Names of every tool the agent exposes (resolved against a dummy context).
pub async fn list_tool_names(toolset: &dyn Toolset) -> anyhow::Result<Vec<String>> {
    let ctx: Arc<dyn adk_rust::ReadonlyContext> = Arc::new(ListingContext {
        content: Content::new("user"),
    });
    Ok(toolset
        .tools(ctx)
        .await?
        .iter()
        .map(|t: &Arc<dyn Tool>| t.name().to_string())
        .collect())
}

/// Number of tools and the serialized size of their declarations (what is
/// sent to the model on every request).
pub async fn declaration_size(toolset: &dyn Toolset) -> anyhow::Result<(usize, usize)> {
    let ctx: Arc<dyn adk_rust::ReadonlyContext> = Arc::new(ListingContext {
        content: Content::new("user"),
    });
    let tools = toolset.tools(ctx).await?;
    let bytes = tools
        .iter()
        .map(|t| {
            serde_json::to_string(&t.declaration())
                .map(|s| s.len())
                .unwrap_or(0)
        })
        .sum();
    Ok((tools.len(), bytes))
}

/// Minimal context used only to enumerate toolset contents.
struct ListingContext {
    content: Content,
}

impl adk_rust::ReadonlyContext for ListingContext {
    fn invocation_id(&self) -> &str {
        "list-tools"
    }
    fn agent_name(&self) -> &str {
        APP_NAME
    }
    fn user_id(&self) -> &str {
        "user"
    }
    fn app_name(&self) -> &str {
        APP_NAME
    }
    fn session_id(&self) -> &str {
        "list-tools"
    }
    fn branch(&self) -> &str {
        ""
    }
    fn user_content(&self) -> &Content {
        &self.content
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolset_configs_follow_enabled_toolsets() {
        let mut settings = Settings {
            model: "m".into(),
            video_model: "v".into(),
            image_model: "i".into(),
            google_cloud_project: None,
            google_cloud_location: "global".into(),
            use_vertex_ai: true,
            google_api_key: None,
            client_id: String::new(),
            client_secret: String::new(),
            google_ads_developer_token: "dev".into(),
            google_ads_login_customer_id: Some("123".into()),
            enabled_toolsets: vec![
                "google_ads".into(),
                "drive".into(),
                "bigquery".into(),
                "campaign_manager_360".into(),
            ],
            google_ads_tool_filter: Some(vec!["googleads_customers_google_ads_search".into()]),
            lazy_toolsets: vec![],
            skills_bucket_name: None,
            local_skills_dir: "./skills".into(),
            compaction: config::CompactionSettings::from_env(),
        };
        let configs = google_api_toolset_configs(&settings);
        let names: Vec<&str> = configs.iter().map(|c| c.toolset_name.as_str()).collect();
        assert_eq!(names, vec!["google_ads", "drive", "campaign_manager_360"]);
        let ads = &configs[0];
        assert_eq!(
            ads.discovery_url.as_deref(),
            Some("https://googleads.googleapis.com/$discovery/rest?version=v25")
        );
        assert!(
            ads.additional_headers
                .contains(&("developer-token".to_string(), "dev".to_string()))
        );
        assert!(
            ads.additional_headers
                .contains(&("login-customer-id".to_string(), "123".to_string()))
        );
        assert!(
            configs[2].scopes.is_none(),
            "CM360 tools pick their own least-privilege scope"
        );
        assert_eq!(
            configs[1].tool_filter.as_ref().unwrap().len(),
            DRIVE_TOOL_FILTER.len()
        );

        settings.enabled_toolsets = vec![];
        assert!(google_api_toolset_configs(&settings).is_empty());
    }

    #[test]
    fn text_mime_detection() {
        assert!(is_text_mime("text/csv"));
        assert!(is_text_mime("application/json; charset=utf-8"));
        assert!(!is_text_mime("image/png"));
    }

    #[test]
    fn extensions_for_common_uploads() {
        assert_eq!(extension_for("text/csv"), "csv");
        assert_eq!(extension_for("text/csv; charset=utf-8"), "csv");
        assert_eq!(extension_for("image/png"), "png");
        assert_eq!(extension_for("application/x-unknown"), "bin");
    }

    #[tokio::test]
    async fn telemetry_stream_records_otel_usage_and_cost() {
        let mut resp = adk_rust::LlmResponse::new(Content::new("model").with_text("hello"));
        resp.usage_metadata = Some(adk_rust::UsageMetadata {
            prompt_token_count: 1_000,
            candidates_token_count: 250,
            total_token_count: 1_250,
            cache_read_input_token_count: Some(200),
            thinking_token_count: Some(50),
            ..Default::default()
        });
        let raw_stream: adk_rust::LlmResponseStream =
            Box::pin(futures::stream::once(async move { Ok(resp) }));
        let span = adk_rust::telemetry::llm_generate_span("gemini", "gemini-3.8-flash", true);
        let mut tracked = track_stream_token_usage(raw_stream, span, "gemini-3.8-flash".into());
        let item = tracked
            .next()
            .await
            .expect("one item")
            .expect("ok response");
        let usage = item.usage_metadata.expect("usage metadata present");
        assert!(usage.cost.unwrap_or(0.0) > 0.0);
        let provider_usage = usage.provider_usage.expect("provider_usage present");
        assert_eq!(provider_usage["gen_ai.request.model"], "gemini-3.8-flash");
        assert_eq!(provider_usage["gen_ai.usage.input_tokens"], 1_000);
        assert_eq!(provider_usage["gen_ai.usage.output_tokens"], 250);
        assert_eq!(provider_usage["gen_ai.usage.total_tokens"], 1_250);
        assert_eq!(provider_usage["gen_ai.usage.cache_read_tokens"], 200);
        assert_eq!(provider_usage["gen_ai.usage.thinking_tokens"], 50);
    }
}
