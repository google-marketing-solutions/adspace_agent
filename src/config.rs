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

//! Configuration settings and toolset filters for the AdSpace Agent.
//!
//! Everything here is resolved from environment variables (optionally loaded
//! from a `.env` file by the binary entry points) so that the agent can be
//! configured identically on a laptop, in Cloud Run, or on Agent Engine.

use std::env;

use thiserror::Error;

/// The application / root agent name.
pub const APP_NAME: &str = "adspace_agent";

/// Default Gemini model.
pub const DEFAULT_MODEL: &str = "gemini-4-hydrogen";
/// Default video generation model.
pub const DEFAULT_VIDEO_MODEL: &str = "gemini-omni-1.2-flash";
/// Default image generation model.
pub const DEFAULT_IMAGE_MODEL: &str = "gemini-nano-banana-2.1";

/// Preset agent LLM models exposed in the Studio settings selector.
pub const PRESET_MODELS: &[&str] = &[
    "gemini-4-hydrogen",
    "gemini-4-carbon",
    "gemini-4-argon",
    "gemini-3.8-flash",
    "gemini-3.5-flash-lite",
];

/// Preset video generation models exposed in the Studio settings selector.
pub const PRESET_VIDEO_MODELS: &[&str] = &[
    "gemini-omni-1.2-flash",
    "gemini-omni-1.1-flash",
    "veo-3.1-fast-generate-001",
    "veo-3.1-generate-001",
];

/// Preset image generation models exposed in the Studio settings selector.
pub const PRESET_IMAGE_MODELS: &[&str] = &[
    "gemini-nano-banana-2.1",
    "gemini-3-pro-image",
    "gemini-3.1-flash-image",
    "gemini-3.1-flash-lite-image",
];

/// Default events-compaction interval (in invocations).
pub const DEFAULT_COMPACTION_INTERVAL: u64 = 3;
/// Default events-compaction overlap size (in invocations).
pub const DEFAULT_COMPACTION_OVERLAP_SIZE: u64 = 1;
/// Default token threshold for intra-invocation compaction. With adk-rust
/// 2.3.0 that compaction only runs on the turn resumed after an OAuth grant,
/// once before the agent starts; normal chat turns (`/api/run_sse`) do not
/// use it, because adk-server's request runners have no setting for it.
pub const DEFAULT_COMPACTION_TOKEN_THRESHOLD: u64 = 80_000;
/// Default number of raw events retained after intra-invocation compaction
/// (same scope as [`DEFAULT_COMPACTION_TOKEN_THRESHOLD`]).
pub const DEFAULT_COMPACTION_EVENT_RETENTION_SIZE: u64 = 6;
/// Default fast model used for background sliding-window / intra-invocation summarization.
pub const DEFAULT_SUMMARIZER_MODEL: &str = "gemini-4-hydrogen";

/// Google Ads API version used for the discovery document.
pub const GOOGLE_ADS_API_VERSION: &str = "v26";

/// Google Ads tools enabled when `GOOGLE_ADS_TOOL_FILTER` is unset.
pub const DEFAULT_GOOGLE_ADS_TOOL_FILTER: &[&str] = &[
    "googleads_customers_google_ads_search",
    "googleads_google_ads_fields_search",
    "googleads_customers_generate_audience_definition",
    "googleads_customers_generate_audience_composition_insights",
];

/// Every Google toolset the agent knows how to build.
pub const ALL_GOOGLE_TOOLSETS: &[&str] = &[
    "bid_manager",
    "bigquery",
    "campaign_manager_360",
    "display_video_360",
    "drive",
    "google_ads",
    "google_genai",
    "merchant_center_inventories",
    "merchant_center_products",
    "merchant_center_reports",
    "search_ads_360",
    "storage",
    "youtube",
];

/// Display & Video 360 tools exposed to the agent.
pub const DISPLAY_VIDEO_360_TOOL_FILTER: &[&str] = &[
    "displayvideo_advertisers_audit",
    "displayvideo_advertisers_list",
    "displayvideo_advertisers_get",
    "displayvideo_advertisers_edit_assigned_targeting_options",
    "displayvideo_advertisers_ad_assets_bulk_create",
    "displayvideo_advertisers_ad_assets_list",
    "displayvideo_advertisers_ad_assets_create",
    "displayvideo_advertisers_ad_assets_get",
    "displayvideo_advertisers_ad_assets_upload",
    "displayvideo_advertisers_ad_group_ads_get",
    "displayvideo_advertisers_ad_group_ads_list",
    "displayvideo_advertisers_ad_groups_bulk_list_assigned_target",
    "displayvideo_advertisers_ad_groups_get",
    "displayvideo_advertisers_ad_groups_list",
    "displayvideo_advertisers_assets_upload",
    "displayvideo_advertisers_campaigns_list",
    "displayvideo_advertisers_campaigns_create",
    "displayvideo_advertisers_campaigns_get",
    "displayvideo_advertisers_campaigns_patch",
    "displayvideo_advertisers_channels_list",
    "displayvideo_advertisers_channels_create",
    "displayvideo_advertisers_channels_get",
    "displayvideo_advertisers_channels_patch",
    "displayvideo_advertisers_channels_sites_bulk_edit",
    "displayvideo_advertisers_channels_sites_create",
    "displayvideo_advertisers_channels_sites_list",
    "displayvideo_advertisers_channels_sites_replace",
    "displayvideo_advertisers_creatives_list",
    "displayvideo_advertisers_creatives_create",
    "displayvideo_advertisers_creatives_get",
    "displayvideo_advertisers_creatives_patch",
    "displayvideo_advertisers_insertion_orders_list",
    "displayvideo_advertisers_insertion_orders_create",
    "displayvideo_advertisers_insertion_orders_get",
    "displayvideo_advertisers_insertion_orders_patch",
    "displayvideo_advertisers_invoices_list",
    "displayvideo_advertisers_invoices_lookup_invoice_currency",
    "displayvideo_advertisers_line_items_bulk_edit_assigned_targe",
    "displayvideo_advertisers_line_items_bulk_list_assigned_targe",
    "displayvideo_advertisers_line_items_bulk_update",
    "displayvideo_advertisers_line_items_list",
    "displayvideo_advertisers_line_items_create",
    "displayvideo_advertisers_line_items_get",
    "displayvideo_advertisers_line_items_patch",
    "displayvideo_advertisers_line_items_duplicate",
    "displayvideo_advertisers_location_lists_list",
    "displayvideo_advertisers_location_lists_create",
    "displayvideo_advertisers_location_lists_get",
    "displayvideo_advertisers_location_lists_patch",
    "displayvideo_advertisers_negative_keyword_lists_list",
    "displayvideo_advertisers_negative_keyword_lists_create",
    "displayvideo_advertisers_negative_keyword_lists_get",
    "displayvideo_advertisers_negative_keyword_lists_patch",
    "displayvideo_combined_audiences_get",
    "displayvideo_combined_audiences_list",
    "displayvideo_custom_bidding_algorithms_list",
    "displayvideo_custom_bidding_algorithms_create",
    "displayvideo_custom_bidding_algorithms_get",
    "displayvideo_custom_bidding_algorithms_patch",
    "displayvideo_custom_bidding_algorithms_upload_rules",
    "displayvideo_custom_bidding_algorithms_upload_script",
    "displayvideo_custom_bidding_algorithms_rules_list",
    "displayvideo_custom_bidding_algorithms_rules_create",
    "displayvideo_custom_bidding_algorithms_rules_get",
    "displayvideo_custom_bidding_algorithms_scripts_list",
    "displayvideo_custom_bidding_algorithms_scripts_create",
    "displayvideo_custom_bidding_algorithms_scripts_get",
    "displayvideo_custom_lists_get",
    "displayvideo_custom_lists_list",
    "displayvideo_first_party_and_partner_audiences_list",
    "displayvideo_first_party_and_partner_audiences_create",
    "displayvideo_first_party_and_partner_audiences_edit_customer",
    "displayvideo_first_party_and_partner_audiences_get",
    "displayvideo_first_party_and_partner_audiences_patch",
    "displayvideo_floodlight_groups_get",
    "displayvideo_floodlight_groups_patch",
    "displayvideo_floodlight_groups_floodlight_activities_get",
    "displayvideo_floodlight_groups_floodlight_activities_list",
    "displayvideo_google_audiences_get",
    "displayvideo_google_audiences_list",
    "displayvideo_guaranteed_orders_list",
    "displayvideo_guaranteed_orders_create",
    "displayvideo_guaranteed_orders_edit_guaranteed_order_read_ac",
    "displayvideo_guaranteed_orders_get",
    "displayvideo_guaranteed_orders_patch",
    "displayvideo_inventory_source_groups_list",
    "displayvideo_inventory_source_groups_create",
    "displayvideo_inventory_source_groups_get",
    "displayvideo_inventory_source_groups_patch",
    "displayvideo_inventory_sources_list",
    "displayvideo_inventory_sources_create",
    "displayvideo_inventory_sources_get",
    "displayvideo_inventory_sources_patch",
    "displayvideo_media_download",
    "displayvideo_media_upload",
    "displayvideo_partners_edit_assigned_targeting_options",
    "displayvideo_partners_get",
    "displayvideo_partners_list",
    "displayvideo_partners_channels_list",
    "displayvideo_partners_channels_create",
    "displayvideo_partners_channels_get",
    "displayvideo_partners_channels_patch",
    "displayvideo_partners_channels_sites_bulk_edit",
    "displayvideo_partners_channels_sites_create",
    "displayvideo_partners_channels_sites_list",
    "displayvideo_sdfdownloadtasks_create",
    "displayvideo_sdfdownloadtasks_operations_get",
    "displayvideo_sdfuploadtasks_operations_get",
    "displayvideo_targeting_types_targeting_options_get",
    "displayvideo_targeting_types_targeting_options_list",
    "displayvideo_targeting_types_targeting_options_search",
    "displayvideo_users_bulk_edit_assigned_user_roles",
    "displayvideo_users_list",
    "displayvideo_users_get",
];

/// Google Drive tools exposed to the agent (read-only subset).
pub const DRIVE_TOOL_FILTER: &[&str] = &[
    "drive_drives_list",
    "drive_drives_get",
    "drive_files_list",
    "drive_files_get",
    "drive_files_download",
    "drive_files_export",
    "drive_permissions_list",
    "drive_teamdrives_list",
    "drive_teamdrives_get",
];

/// Cloud Storage tools exposed to the agent (read-only subset).
pub const STORAGE_TOOL_FILTER: &[&str] = &[
    "storage_buckets_get",
    "storage_buckets_list",
    "storage_folders_get",
    "storage_folders_list",
    "storage_objects_copy",
    "storage_objects_get",
    "storage_objects_list",
];

/// YouTube Data API tools exposed to the agent (read-only subset).
pub const YOUTUBE_TOOL_FILTER: &[&str] = &[
    "youtube_activities_list",
    "youtube_captions_list",
    "youtube_captions_download",
    "youtube_channels_list",
    "youtube_comment_threads_list",
    "youtube_comments_list",
    "youtube_live_broadcasts_list",
    "youtube_live_chat_messages_list",
    "youtube_live_streams_list",
    "youtube_search_list",
    "youtube_subscriptions_list",
    "youtube_super_chat_events_list",
    "youtube_video_categories_list",
    "youtube_videos_list",
    "youtube_videos_get_rating",
    "youtube_youtube_v3_live_chat_messages_stream",
];

/// Google toolsets whose tools are loaded on demand (`enable_toolsets`)
/// when `LAZY_TOOLSETS` is unset: keeps the high-frequency core (`google_ads`
/// and `bigquery`) eager while deferring secondary platforms until needed.
pub const DEFAULT_LAZY_TOOLSETS: &[&str] = &[
    "bid_manager",
    "campaign_manager_360",
    "display_video_360",
    "drive",
    "google_genai",
    "merchant_center_inventories",
    "merchant_center_products",
    "merchant_center_reports",
    "search_ads_360",
    "storage",
    "youtube",
];

/// Whether `name` is a general-purpose data store (a warehouse, a file store)
/// rather than an ads platform. The agent instruction tells the model to use
/// these only when the user asks for them, and never as a place to look for a
/// platform's data.
pub fn is_general_purpose_store(name: &str) -> bool {
    matches!(name, "bigquery" | "drive" | "storage")
}

/// Suffix of the write tier of a deferred toolset (`campaign_manager_360`
/// holds the read tools, `campaign_manager_360_write` the create / update /
/// patch / delete tools). See [`crate::google_api::GoogleApiToolset::split_writes`].
pub const LAZY_WRITE_TIER_SUFFIX: &str = "_write";

/// Short descriptions of the Google toolsets, shown to the model by
/// `enable_toolsets` for deferred toolsets.
pub fn toolset_summary(name: &str) -> &'static str {
    match name {
        "bid_manager" => "Bid Manager API (DV360 reporting queries and reports).",
        "bigquery" => "BigQuery datasets, tables, schemas and SQL queries.",
        "campaign_manager_360" => {
            "Campaign Manager 360 (dfareporting v5): campaigns, placements, ads, creatives, event tags, sites, advertisers, reports and more."
        }
        "display_video_360" => {
            "Display & Video 360: advertisers, campaigns, insertion orders, line items, creatives, audiences, targeting, SDF tasks."
        }
        "drive" => "Google Drive: list, get, download and export files; shared drives.",
        "google_ads" => {
            "Google Ads: GAQL search, field metadata, audience/keyword/reach insights and planning."
        }
        "google_genai" => {
            "Gemini media tools: ask about YouTube videos, generate images and videos."
        }
        "merchant_center_inventories" => "Merchant Center local and regional product inventories.",
        "merchant_center_products" => "Merchant Center products and product inputs.",
        "merchant_center_reports" => "Merchant Center reporting (reports.search).",
        "search_ads_360" => "Search Ads 360: search queries, custom columns, accessible customers.",
        "storage" => "Google Cloud Storage buckets, folders and objects (read-only).",
        "youtube" => "YouTube Data API: videos, channels, captions, comments, search, live chat.",
        _ => "Google API tools.",
    }
}

/// Summaries shown to the model for the two tiers of a deferred toolset
/// that has been split with [`LAZY_WRITE_TIER_SUFFIX`]: `(reads, writes)`.
pub fn tiered_toolset_summaries(name: &str) -> (String, String) {
    let base = toolset_summary(name);
    let reads = format!(
        "{base} Read operations only (list, get, search, query, run reports); \
         create/update/delete tools are in `{name}{LAZY_WRITE_TIER_SUFFIX}`."
    );
    let writes = format!(
        "Create, update, patch and delete operations of `{name}` ({}). \
         Enable only when the user asks to create or change something; \
         reading never needs it.",
        base.split(':').next().unwrap_or(base).trim_end_matches('.')
    );
    (reads, writes)
}

/// Resolves which toolsets are deferred from a raw `LAZY_TOOLSETS` value
/// (`none` disables deferral, `all` defers every enabled toolset).
pub fn parse_lazy_toolsets(raw: Option<&str>, enabled: &[String]) -> Vec<String> {
    let default = || {
        DEFAULT_LAZY_TOOLSETS
            .iter()
            .map(|s| s.to_string())
            .filter(|s| enabled.contains(s))
            .collect()
    };
    let Some(raw) = raw else {
        return default();
    };
    let parsed = split_csv_env(raw);
    if parsed.is_empty() {
        return default();
    }
    if is_none_marker(&parsed) {
        return Vec::new();
    }
    if is_wildcard(&parsed) {
        return enabled.to_vec();
    }
    parsed.into_iter().filter(|s| enabled.contains(s)).collect()
}

/// Errors raised while resolving configuration from the environment.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ConfigError {
    /// `ENABLED_TOOLSETS` names a toolset the agent does not know.
    #[error("Unknown toolset(s) in ENABLED_TOOLSETS: {unknown}. Valid toolsets are: {valid}")]
    UnknownToolsets {
        /// Comma-separated unknown names (sorted).
        unknown: String,
        /// Comma-separated valid names.
        valid: String,
    },
}

/// Splits a comma-separated environment value, trimming and dropping blanks.
fn split_csv_env(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Returns true when `parsed` is exactly one of the wildcard markers (`*`/`all`).
fn is_wildcard(parsed: &[String]) -> bool {
    parsed.len() == 1 && matches!(parsed[0].to_lowercase().as_str(), "*" | "all")
}

/// Returns true when `parsed` is exactly the `none` marker.
fn is_none_marker(parsed: &[String]) -> bool {
    parsed.len() == 1 && parsed[0].eq_ignore_ascii_case("none")
}

/// Resolves the Google Ads tool filter from a raw `GOOGLE_ADS_TOOL_FILTER` value.
///
/// Returns `None` when all tools are enabled via `*` or `all`, an empty list
/// for `none`, the default filter when the variable is unset or blank, and
/// otherwise the parsed list of tool names.
pub fn parse_google_ads_tool_filter(raw: Option<&str>) -> Option<Vec<String>> {
    let default = || {
        Some(
            DEFAULT_GOOGLE_ADS_TOOL_FILTER
                .iter()
                .map(|s| s.to_string())
                .collect(),
        )
    };
    let Some(raw) = raw else {
        return default();
    };
    let parsed = split_csv_env(raw);
    if parsed.is_empty() {
        return default();
    }
    if is_wildcard(&parsed) {
        return None;
    }
    if is_none_marker(&parsed) {
        return Some(Vec::new());
    }
    Some(parsed)
}

/// Resolves the Google Ads tool filter from the environment.
pub fn get_google_ads_tool_filter() -> Option<Vec<String>> {
    parse_google_ads_tool_filter(env::var("GOOGLE_ADS_TOOL_FILTER").ok().as_deref())
}

/// Resolves and validates the enabled toolsets from a raw `ENABLED_TOOLSETS` value.
///
/// # Errors
///
/// Returns [`ConfigError::UnknownToolsets`] when unknown names are configured.
pub fn parse_enabled_toolsets(raw: Option<&str>) -> Result<Vec<String>, ConfigError> {
    let all = || {
        ALL_GOOGLE_TOOLSETS
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    };
    let Some(raw) = raw else {
        return Ok(all());
    };
    let parsed = split_csv_env(raw);
    if parsed.is_empty() || is_wildcard(&parsed) {
        return Ok(all());
    }
    if is_none_marker(&parsed) {
        return Ok(Vec::new());
    }
    let mut unknown: Vec<&str> = parsed
        .iter()
        .map(String::as_str)
        .filter(|name| !ALL_GOOGLE_TOOLSETS.contains(name))
        .collect();
    if !unknown.is_empty() {
        unknown.sort_unstable();
        unknown.dedup();
        let err = ConfigError::UnknownToolsets {
            unknown: unknown.join(", "),
            valid: ALL_GOOGLE_TOOLSETS.join(", "),
        };
        tracing::warn!("{err}");
        return Err(err);
    }
    Ok(parsed)
}

/// Resolves and validates the enabled Google toolsets from the environment.
///
/// # Errors
///
/// Returns [`ConfigError::UnknownToolsets`] when unknown names are configured.
pub fn get_enabled_toolsets() -> Result<Vec<String>, ConfigError> {
    parse_enabled_toolsets(env::var("ENABLED_TOOLSETS").ok().as_deref())
}

/// Reads an environment variable, treating unset *and* empty values as absent.
pub fn env_opt(name: &str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// Reads an environment variable with a default.
pub fn env_or(name: &str, default: &str) -> String {
    env_opt(name).unwrap_or_else(|| default.to_string())
}

/// Reads an unsigned integer environment variable with a default.
///
/// Unparseable values fall back to the default with a warning.
pub fn env_u64_or(name: &str, default: u64) -> u64 {
    match env_opt(name) {
        Some(raw) => raw.parse().unwrap_or_else(|_| {
            tracing::warn!("Ignoring invalid {name}={raw:?}; using default {default}");
            default
        }),
        None => default,
    }
}

/// Parses a boolean-ish configuration value.
///
/// Accepts the usual spellings case-insensitively: `1`/`true`/`yes`/`on`/`y`/`t`
/// are `Some(true)`, `0`/`false`/`no`/`off`/`n`/`f` are `Some(false)`; anything
/// else is `None` so callers can fall back to a default with a warning instead
/// of silently flipping a flag.
pub fn parse_bool(raw: &str) -> Option<bool> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "y" | "t" => Some(true),
        "0" | "false" | "no" | "off" | "n" | "f" => Some(false),
        _ => None,
    }
}

/// Reads a boolean environment variable with a default (see [`parse_bool`]).
///
/// Unrecognised values fall back to the default with a warning.
pub fn env_bool_or(name: &str, default: bool) -> bool {
    match env_opt(name) {
        Some(raw) => parse_bool(&raw).unwrap_or_else(|| {
            tracing::warn!("Ignoring invalid {name}={raw:?}; using default {default}");
            default
        }),
        None => default,
    }
}

/// Whether `GOOGLE_GENAI_USE_VERTEXAI` (or `GOOGLE_GENAI_USE_ENTERPRISE`) is
/// truthy (defaults to `true`; accepts `1`/`true`/`yes`/`on` like the Python SDK).
pub fn use_vertex_ai() -> bool {
    if env_opt("GOOGLE_GENAI_USE_ENTERPRISE").is_some() {
        return env_bool_or("GOOGLE_GENAI_USE_ENTERPRISE", true);
    }
    env_bool_or("GOOGLE_GENAI_USE_VERTEXAI", true)
}

/// Events-compaction settings (mirrors ADK Python's `EventsCompactionConfig`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionSettings {
    /// Trigger compaction every N new invocations.
    pub compaction_interval: u64,
    /// Include N last invocations from the previous window.
    pub overlap_size: u64,
    /// Token threshold for intra-invocation compaction (OAuth-resume turn only).
    pub token_threshold: u64,
    /// Raw events kept by intra-invocation compaction (OAuth-resume turn only).
    pub event_retention_size: u64,
}

impl CompactionSettings {
    /// Resolves the compaction settings from the environment.
    pub fn from_env() -> Self {
        Self {
            compaction_interval: env_u64_or("COMPACTION_INTERVAL", DEFAULT_COMPACTION_INTERVAL),
            overlap_size: env_u64_or("COMPACTION_OVERLAP_SIZE", DEFAULT_COMPACTION_OVERLAP_SIZE),
            token_threshold: env_u64_or(
                "COMPACTION_TOKEN_THRESHOLD",
                DEFAULT_COMPACTION_TOKEN_THRESHOLD,
            ),
            event_retention_size: env_u64_or(
                "COMPACTION_EVENT_RETENTION_SIZE",
                DEFAULT_COMPACTION_EVENT_RETENTION_SIZE,
            ),
        }
    }
}

/// Reasoning-effort mode resolved from `THINKING_LEVEL` / `THINKING_BUDGET`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ThinkingMode {
    /// Low-latency default: `ThinkingLevel::Low` on Gemini 3/4 models and
    /// `thinking_budget: 512` on `gemini-2.5-flash` (non-lite).
    #[default]
    AutoFast,
    /// Defer entirely to the model's server-side default (`thinking_config: None`).
    ModelDefault,
    /// Explicit Gemini 3/4 level (`minimal`, `low`, `medium`, `high`).
    Level(adk_gemini::ThinkingLevel),
    /// Explicit Gemini 2.5 token budget (`0` = off, `-1` = dynamic, `>0` = cap).
    Budget(i32),
}

/// Thinking configuration resolved from `THINKING_LEVEL`, `THINKING_BUDGET`,
/// and `INCLUDE_THOUGHTS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ThinkingSettings {
    /// Reasoning-effort mode.
    pub mode: ThinkingMode,
    /// Optional override for whether synthesized thought summaries are streamed.
    pub include_thoughts: Option<bool>,
}

impl ThinkingSettings {
    /// Resolves thinking settings from the environment.
    pub fn from_env() -> Self {
        Self::parse(
            env_opt("THINKING_LEVEL").as_deref(),
            env_opt("THINKING_BUDGET").as_deref(),
            env_opt("INCLUDE_THOUGHTS").as_deref(),
        )
    }

    /// Parses raw environment values into [`ThinkingSettings`].
    pub fn parse(
        level_raw: Option<&str>,
        budget_raw: Option<&str>,
        include_thoughts_raw: Option<&str>,
    ) -> Self {
        let include_thoughts = include_thoughts_raw.and_then(|raw| {
            let parsed = parse_bool(raw);
            if parsed.is_none() {
                tracing::warn!("Ignoring invalid INCLUDE_THOUGHTS={raw:?}");
            }
            parsed
        });

        // Explicit budget wins when `THINKING_LEVEL` is unset so Gemini 2.5
        // callers can set `THINKING_BUDGET` directly without mutual-exclusion errors.
        if let Some(raw) = budget_raw {
            if level_raw.is_some() {
                tracing::warn!("THINKING_BUDGET={raw:?} is ignored because THINKING_LEVEL is set");
            } else {
                match raw.trim().parse::<i32>() {
                    // `-1` is dynamic; anything lower means nothing.
                    Ok(budget) if budget >= -1 => {
                        return Self {
                            mode: ThinkingMode::Budget(budget),
                            include_thoughts,
                        };
                    }
                    _ => {
                        tracing::warn!(
                            "Ignoring invalid THINKING_BUDGET={raw:?} (expected -1 or a \
                             non-negative token count); using default"
                        );
                    }
                }
            }
        }

        let mode = match level_raw.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
            None | Some("") | Some("auto-fast") | Some("fast") => ThinkingMode::AutoFast,
            Some("auto") | Some("default") | Some("dynamic") => ThinkingMode::ModelDefault,
            Some("minimal") | Some("min") => {
                ThinkingMode::Level(adk_gemini::ThinkingLevel::Minimal)
            }
            Some("low") => ThinkingMode::Level(adk_gemini::ThinkingLevel::Low),
            Some("medium") | Some("med") => ThinkingMode::Level(adk_gemini::ThinkingLevel::Medium),
            Some("high") => ThinkingMode::Level(adk_gemini::ThinkingLevel::High),
            Some("0") | Some("off") | Some("none") | Some("false") => ThinkingMode::Budget(0),
            Some(other) => {
                tracing::warn!("Ignoring invalid THINKING_LEVEL={other:?}; using fast default");
                ThinkingMode::AutoFast
            }
        };

        Self {
            mode,
            include_thoughts,
        }
    }

    /// Builds the model-appropriate [`adk_gemini::ThinkingConfig`] for `model_name`,
    /// guaranteeing `thinking_level` and `thinking_budget` are never both set.
    pub fn thinking_config_for_model(
        &self,
        model_name: &str,
    ) -> Option<adk_gemini::ThinkingConfig> {
        use adk_gemini::{ThinkingConfig, ThinkingLevel};

        let m = model_name.trim().to_ascii_lowercase();
        if m.contains("image")
            || m.contains("banana")
            || m.contains("imagen")
            || m.contains("veo")
            || m.contains("omni")
        {
            return None;
        }
        let is_gemini_3_or_newer =
            m.contains("gemini-3") || m.contains("gemini-4") || m.contains("argon");
        let is_gemini_25 = m.contains("gemini-2.5");

        let mut cfg = match self.mode {
            ThinkingMode::ModelDefault => None,
            ThinkingMode::AutoFast => {
                if is_gemini_3_or_newer {
                    Some(ThinkingConfig::new().with_thinking_level(ThinkingLevel::Low))
                } else if is_gemini_25 && m.contains("flash") && !m.contains("lite") {
                    Some(ThinkingConfig::new().with_thinking_budget(512))
                } else {
                    None
                }
            }
            ThinkingMode::Level(level) => {
                if is_gemini_25 {
                    let budget = match level {
                        ThinkingLevel::Minimal => 0,
                        ThinkingLevel::Low => 512,
                        ThinkingLevel::Medium => 2048,
                        ThinkingLevel::High => 8192,
                    };
                    Some(ThinkingConfig::new().with_thinking_budget(budget))
                } else {
                    // Gemini 3.1 Pro rejects Minimal; use Low on non-Flash models,
                    // as the budget branch below does.
                    let level = if level == ThinkingLevel::Minimal && !m.contains("flash") {
                        ThinkingLevel::Low
                    } else {
                        level
                    };
                    Some(ThinkingConfig::new().with_thinking_level(level))
                }
            }
            ThinkingMode::Budget(budget) => {
                if is_gemini_3_or_newer {
                    let level = if budget < 0 {
                        None
                    } else if budget == 0 {
                        // Gemini 3.1 Pro rejects Minimal; use Low on non-Flash Gemini 3/4.
                        Some(if m.contains("flash") {
                            ThinkingLevel::Minimal
                        } else {
                            ThinkingLevel::Low
                        })
                    } else if budget <= 1024 {
                        Some(ThinkingLevel::Low)
                    } else if budget <= 4096 {
                        Some(ThinkingLevel::Medium)
                    } else {
                        Some(ThinkingLevel::High)
                    };
                    level.map(|lvl| ThinkingConfig::new().with_thinking_level(lvl))
                } else {
                    Some(ThinkingConfig::new().with_thinking_budget(budget))
                }
            }
        };

        if let Some(inc) = self.include_thoughts {
            cfg = Some(cfg.unwrap_or_default().with_thoughts_included(inc));
        }
        cfg
    }
}

/// Fully resolved agent settings.
#[derive(Debug, Clone)]
pub struct Settings {
    /// Gemini model name (`MODEL`).
    pub model: String,
    /// Veo video model name (`VIDEO_MODEL`).
    pub video_model: String,
    /// Image model name (`IMAGE_MODEL`).
    pub image_model: String,
    /// Google Cloud project (`GOOGLE_CLOUD_PROJECT`).
    pub google_cloud_project: Option<String>,
    /// Google Cloud location (`GOOGLE_CLOUD_LOCATION`, defaults to `global`).
    pub google_cloud_location: String,
    /// Whether to use Vertex AI for Gemini (`GOOGLE_GENAI_USE_VERTEXAI`).
    pub use_vertex_ai: bool,
    /// Gemini Developer API key (`GOOGLE_API_KEY`), used when not on Vertex AI.
    pub google_api_key: Option<String>,
    /// OAuth client ID (`CLIENT_ID`).
    pub client_id: String,
    /// OAuth client secret (`CLIENT_SECRET`).
    pub client_secret: String,
    /// Google Ads developer token (`GOOGLE_ADS_DEVELOPER_TOKEN`).
    pub google_ads_developer_token: String,
    /// Google Ads login customer ID (`GOOGLE_ADS_LOGIN_CUSTOMER_ID`).
    pub google_ads_login_customer_id: Option<String>,
    /// Enabled Google toolsets (`ENABLED_TOOLSETS`).
    pub enabled_toolsets: Vec<String>,
    /// Google Ads tool filter (`GOOGLE_ADS_TOOL_FILTER`); `None` means all.
    pub google_ads_tool_filter: Option<Vec<String>>,
    /// Toolsets loaded on demand through `enable_toolsets` (`LAZY_TOOLSETS`).
    pub lazy_toolsets: Vec<String>,
    /// Split each deferred toolset into a read tier and a `_write` tier
    /// that are enabled separately (`LAZY_SPLIT_WRITES`, default `true`).
    pub lazy_split_writes: bool,
    /// GCS bucket holding skills and CM360 payloads (`SKILLS_BUCKET_NAME`).
    pub skills_bucket_name: Option<String>,
    /// Local skills directory (`LOCAL_SKILLS_DIR`, defaults to `./skills`).
    pub local_skills_dir: String,
    /// Events-compaction settings.
    pub compaction: CompactionSettings,
}

impl Settings {
    /// Resolves all settings from the environment.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when `ENABLED_TOOLSETS` is invalid.
    pub fn from_env() -> Result<Self, ConfigError> {
        let enabled_toolsets = get_enabled_toolsets()?;
        let lazy_toolsets =
            parse_lazy_toolsets(env::var("LAZY_TOOLSETS").ok().as_deref(), &enabled_toolsets);
        Ok(Self {
            model: env_or("MODEL", DEFAULT_MODEL),
            video_model: env_or("VIDEO_MODEL", DEFAULT_VIDEO_MODEL),
            image_model: env_or("IMAGE_MODEL", DEFAULT_IMAGE_MODEL),
            google_cloud_project: env_opt("GOOGLE_PROJECT_ID")
                .or_else(|| env_opt("GOOGLE_CLOUD_PROJECT")),
            google_cloud_location: env_opt("GOOGLE_MODEL_LOCATION")
                .unwrap_or_else(|| env_or("GOOGLE_CLOUD_LOCATION", "global")),
            use_vertex_ai: use_vertex_ai(),
            google_api_key: env_opt("GOOGLE_API_KEY"),
            client_id: env_or("CLIENT_ID", ""),
            client_secret: env_or("CLIENT_SECRET", ""),
            google_ads_developer_token: env_or("GOOGLE_ADS_DEVELOPER_TOKEN", ""),
            google_ads_login_customer_id: env_opt("GOOGLE_ADS_LOGIN_CUSTOMER_ID"),
            enabled_toolsets,
            google_ads_tool_filter: get_google_ads_tool_filter(),
            lazy_toolsets,
            lazy_split_writes: env_bool_or("LAZY_SPLIT_WRITES", true),
            skills_bucket_name: env_opt("SKILLS_BUCKET_NAME"),
            local_skills_dir: env_or("LOCAL_SKILLS_DIR", "./skills"),
            compaction: CompactionSettings::from_env(),
        })
    }

    /// Returns true when the named toolset is enabled.
    pub fn is_toolset_enabled(&self, name: &str) -> bool {
        self.enabled_toolsets.iter().any(|t| t == name)
    }
}

/// Snapshot of the currently active model identifiers.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ActiveModels {
    /// Active ADK LLM model identifier.
    pub model: String,
    /// Active video generation model identifier.
    pub video_model: String,
    /// Active image generation model identifier.
    pub image_model: String,
}

impl Default for ActiveModels {
    fn default() -> Self {
        Self {
            model: DEFAULT_MODEL.to_string(),
            video_model: DEFAULT_VIDEO_MODEL.to_string(),
            image_model: DEFAULT_IMAGE_MODEL.to_string(),
        }
    }
}

/// Validates a model identifier string.
pub fn validate_model_name(raw: &str) -> anyhow::Result<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        anyhow::bail!("model identifier cannot be empty");
    }
    if trimmed.len() > 128 {
        anyhow::bail!("model identifier cannot exceed 128 characters");
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':'))
    {
        anyhow::bail!("model identifier '{trimmed}' contains invalid characters");
    }
    Ok(trimmed.to_string())
}

/// Runtime hook allowing the HTTP server and tools to read or hot-swap the
/// active agent, video, and image models.
pub trait ModelController: Send + Sync {
    /// Returns the currently active model identifiers.
    fn active_models(&self) -> ActiveModels;

    /// Updates any supplied model identifiers and returns the updated snapshot.
    fn update_models(
        &self,
        model: Option<&str>,
        video_model: Option<&str>,
        image_model: Option<&str>,
    ) -> anyhow::Result<ActiveModels>;
}

/// In-memory [`ModelController`] used when no live LLM factory is attached.
pub struct SimpleModelController {
    state: std::sync::RwLock<ActiveModels>,
}

impl SimpleModelController {
    /// Creates a controller with the given initial model names.
    pub fn new(
        model: impl Into<String>,
        video_model: impl Into<String>,
        image_model: impl Into<String>,
    ) -> Self {
        Self {
            state: std::sync::RwLock::new(ActiveModels {
                model: model.into(),
                video_model: video_model.into(),
                image_model: image_model.into(),
            }),
        }
    }
}

impl Default for SimpleModelController {
    fn default() -> Self {
        Self {
            state: std::sync::RwLock::new(ActiveModels::default()),
        }
    }
}

impl ModelController for SimpleModelController {
    fn active_models(&self) -> ActiveModels {
        self.state.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn update_models(
        &self,
        model: Option<&str>,
        video_model: Option<&str>,
        image_model: Option<&str>,
    ) -> anyhow::Result<ActiveModels> {
        let mut guard = self.state.write().unwrap_or_else(|e| e.into_inner());
        if let Some(m) = model {
            guard.model = validate_model_name(m)?;
        }
        if let Some(v) = video_model {
            guard.video_model = validate_model_name(v)?;
        }
        if let Some(i) = image_model {
            guard.image_model = validate_model_name(i)?;
        }
        Ok(guard.clone())
    }
}

/// Per-million-token USD pricing rates for an LLM model.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ModelPricing {
    /// USD per 1M uncached input tokens (prompts <= 200k tokens).
    pub input_per_million: f64,
    /// USD per 1M uncached input tokens when prompt > 200k tokens.
    pub input_long_per_million: f64,
    /// USD per 1M cached input tokens (prompts <= 200k tokens).
    pub cached_input_per_million: f64,
    /// USD per 1M cached input tokens when prompt > 200k tokens.
    pub cached_input_long_per_million: f64,
    /// USD per 1M output/completion tokens (including thinking tokens, prompts <= 200k).
    pub output_per_million: f64,
    /// USD per 1M output/completion tokens when prompt > 200k tokens.
    pub output_long_per_million: f64,
    /// USD per 1M audio input tokens.
    pub audio_input_per_million: f64,
}

/// Returns the per-million-token Paid Tier Standard pricing table for `model`
/// (from <https://aistudio.google.com/docs/pricing>).
pub fn model_pricing(model: &str) -> ModelPricing {
    let m = model.trim().to_ascii_lowercase();
    if m.contains("hydrogen") {
        // Gemini 4 Hydrogen (Paid Tier Standard): $1.50 in, $0.075 cached, $7.50 out
        ModelPricing {
            input_per_million: 1.50,
            input_long_per_million: 1.50,
            cached_input_per_million: 0.075,
            cached_input_long_per_million: 0.075,
            output_per_million: 7.50,
            output_long_per_million: 7.50,
            audio_input_per_million: 1.50,
        }
    } else if m.contains("gemini-4") || m.contains("argon") || m.contains("carbon") {
        // Gemini 4 Carbon and Argon (Paid Tier Standard), introductory rates
        // through November 30, 2026: $2.00 in, $0.10 cached, $10.00 out. The
        // pricing page lists $4.00 / $0.20 / $20.00 from December 1, 2026.
        ModelPricing {
            input_per_million: 2.00,
            input_long_per_million: 2.00,
            cached_input_per_million: 0.10,
            cached_input_long_per_million: 0.10,
            output_per_million: 10.00,
            output_long_per_million: 10.00,
            audio_input_per_million: 2.00,
        }
    } else if m.contains("gemini-3.5-flash-lite") {
        // Gemini 3.5 Flash-Lite (Paid Tier Standard): $0.30 in, $0.03 cached, $2.50 out
        ModelPricing {
            input_per_million: 0.30,
            input_long_per_million: 0.30,
            cached_input_per_million: 0.03,
            cached_input_long_per_million: 0.03,
            output_per_million: 2.50,
            output_long_per_million: 2.50,
            audio_input_per_million: 0.30,
        }
    } else if m.contains("gemini-3.1-flash-lite") {
        // Gemini 3.1 Flash-Lite (Paid Tier Standard): $0.25 in ($0.50 audio), $0.025 cached, $1.50 out
        ModelPricing {
            input_per_million: 0.25,
            input_long_per_million: 0.25,
            cached_input_per_million: 0.025,
            cached_input_long_per_million: 0.025,
            output_per_million: 1.50,
            output_long_per_million: 1.50,
            audio_input_per_million: 0.50,
        }
    } else if m.contains("flash-lite") || m.contains("gemini-2.0-flash") {
        // Gemini 2.5 Flash-Lite (Paid Tier Standard): $0.10 in ($0.30 audio), $0.01 cached, $0.40 out
        ModelPricing {
            input_per_million: 0.10,
            input_long_per_million: 0.10,
            cached_input_per_million: 0.01,
            cached_input_long_per_million: 0.01,
            output_per_million: 0.40,
            output_long_per_million: 0.40,
            audio_input_per_million: 0.30,
        }
    } else if (m.contains("gemini-3.8") || m.contains("gemini-3.6")) && m.contains("flash") {
        // Gemini 3.8 Flash & Gemini 3.6 Flash (Paid Tier Standard): $0.75 in, $0.075 cached, $3.75 out
        ModelPricing {
            input_per_million: 0.75,
            input_long_per_million: 0.75,
            cached_input_per_million: 0.075,
            cached_input_long_per_million: 0.075,
            output_per_million: 3.75,
            output_long_per_million: 3.75,
            audio_input_per_million: 0.75,
        }
    } else if m.contains("gemini-3") && m.contains("flash") {
        // Gemini 3 Flash Preview (Paid Tier Standard): $0.50 in ($1.00 audio), $0.05 cached, $3.00 out
        ModelPricing {
            input_per_million: 0.50,
            input_long_per_million: 0.50,
            cached_input_per_million: 0.05,
            cached_input_long_per_million: 0.05,
            output_per_million: 3.00,
            output_long_per_million: 3.00,
            audio_input_per_million: 1.00,
        }
    } else if m.contains("flash") {
        // Gemini 2.5 Flash (Paid Tier Standard): $0.30 in ($1.00 audio), $0.03 cached, $2.50 out
        ModelPricing {
            input_per_million: 0.30,
            input_long_per_million: 0.30,
            cached_input_per_million: 0.03,
            cached_input_long_per_million: 0.03,
            output_per_million: 2.50,
            output_long_per_million: 2.50,
            audio_input_per_million: 1.00,
        }
    } else if m.contains("gemini-3") {
        // Gemini 3.1 Pro Preview (Paid Tier Standard): $2.00/$4.00 in, $0.20/$0.40 cached, $12.00/$18.00 out
        ModelPricing {
            input_per_million: 2.00,
            input_long_per_million: 4.00,
            cached_input_per_million: 0.20,
            cached_input_long_per_million: 0.40,
            output_per_million: 12.00,
            output_long_per_million: 18.00,
            audio_input_per_million: 2.00,
        }
    } else {
        // Gemini 2.5 Pro (Paid Tier Standard): $1.25/$2.50 in, $0.125/$0.25 cached, $10.00/$15.00 out
        ModelPricing {
            input_per_million: 1.25,
            input_long_per_million: 2.50,
            cached_input_per_million: 0.125,
            cached_input_long_per_million: 0.25,
            output_per_million: 10.00,
            output_long_per_million: 15.00,
            audio_input_per_million: 1.25,
        }
    }
}

/// Pricing table keyed by model name for every preset LLM model plus any
/// `extra` names (typically the currently active model), ready to be embedded
/// in the `/auth/models` payload so the Studio UI never carries its own copy
/// of the rates.
///
/// Keys are lower-cased and trimmed to match how the UI looks models up.
pub fn model_pricing_table<'a>(
    extra: impl IntoIterator<Item = &'a str>,
) -> std::collections::BTreeMap<String, ModelPricing> {
    PRESET_MODELS
        .iter()
        .copied()
        .chain(extra)
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .map(|name| {
            let pricing = model_pricing(&name);
            (name, pricing)
        })
        .collect()
}

/// Estimates the USD cost of a single LLM call from its [`adk_rust::UsageMetadata`].
pub fn estimate_usage_cost_usd(model: &str, usage: &adk_rust::UsageMetadata) -> f64 {
    let p = model_pricing(model);
    let prompt_tokens = i64::from(usage.prompt_token_count.max(0));
    let is_long = prompt_tokens > 200_000;
    let input_rate = if is_long {
        p.input_long_per_million
    } else {
        p.input_per_million
    };
    let cached_rate = if is_long {
        p.cached_input_long_per_million
    } else {
        p.cached_input_per_million
    };
    let output_rate = if is_long {
        p.output_long_per_million
    } else {
        p.output_per_million
    };

    let cached_tokens =
        i64::from(usage.cache_read_input_token_count.unwrap_or(0).max(0)).min(prompt_tokens);
    let remaining_after_cache = (prompt_tokens - cached_tokens).max(0);
    let audio_in_tokens =
        i64::from(usage.audio_input_token_count.unwrap_or(0).max(0)).min(remaining_after_cache);
    let uncached_text_in_tokens = (remaining_after_cache - audio_in_tokens).max(0);
    let output_tokens = i64::from(usage.candidates_token_count.max(0));

    let input_cost = (uncached_text_in_tokens as f64 / 1_000_000.0) * input_rate;
    let cached_cost = (cached_tokens as f64 / 1_000_000.0) * cached_rate;
    let audio_cost = (audio_in_tokens as f64 / 1_000_000.0) * p.audio_input_per_million;
    let output_cost = (output_tokens as f64 / 1_000_000.0) * output_rate;

    input_cost + cached_cost + audio_cost + output_cost
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn google_ads_tool_filter_variants() {
        assert_eq!(
            parse_google_ads_tool_filter(None),
            Some(strs(DEFAULT_GOOGLE_ADS_TOOL_FILTER))
        );
        assert_eq!(
            parse_google_ads_tool_filter(Some(" , ")),
            Some(strs(DEFAULT_GOOGLE_ADS_TOOL_FILTER))
        );
        assert_eq!(parse_google_ads_tool_filter(Some("*")), None);
        assert_eq!(parse_google_ads_tool_filter(Some("ALL")), None);
        assert_eq!(parse_google_ads_tool_filter(Some("none")), Some(vec![]));
        assert_eq!(
            parse_google_ads_tool_filter(Some(" tool_a , tool_b ")),
            Some(strs(&["tool_a", "tool_b"]))
        );
    }

    #[test]
    fn enabled_toolsets_variants() {
        assert_eq!(parse_enabled_toolsets(None), Ok(strs(ALL_GOOGLE_TOOLSETS)));
        assert_eq!(
            parse_enabled_toolsets(Some("   ")),
            Ok(strs(ALL_GOOGLE_TOOLSETS))
        );
        assert_eq!(
            parse_enabled_toolsets(Some("all")),
            Ok(strs(ALL_GOOGLE_TOOLSETS))
        );
        assert_eq!(
            parse_enabled_toolsets(Some("*")),
            Ok(strs(ALL_GOOGLE_TOOLSETS))
        );
        assert_eq!(parse_enabled_toolsets(Some("none")), Ok(vec![]));
        assert_eq!(
            parse_enabled_toolsets(Some("youtube, google_ads")),
            Ok(strs(&["youtube", "google_ads"]))
        );
        let err = parse_enabled_toolsets(Some("google-ads,dv360")).unwrap_err();
        assert!(
            err.to_string()
                .starts_with("Unknown toolset(s) in ENABLED_TOOLSETS: dv360, google-ads."),
            "{err}"
        );
    }

    #[test]
    fn lazy_toolsets_variants() {
        let enabled = strs(&[
            "google_ads",
            "campaign_manager_360",
            "display_video_360",
            "bigquery",
        ]);
        assert_eq!(
            parse_lazy_toolsets(None, &enabled),
            strs(&["campaign_manager_360", "display_video_360"])
        );
        assert_eq!(
            parse_lazy_toolsets(Some("none"), &enabled),
            Vec::<String>::new()
        );
        assert_eq!(
            parse_lazy_toolsets(Some("all"), &enabled),
            strs(&[
                "google_ads",
                "campaign_manager_360",
                "display_video_360",
                "bigquery",
            ])
        );
        assert_eq!(
            parse_lazy_toolsets(Some("google_ads, nope"), &enabled),
            strs(&["google_ads"])
        );
        // Full default toolset roster keeps only google_ads and bigquery eager.
        let all_enabled = strs(ALL_GOOGLE_TOOLSETS);
        let default_lazy = parse_lazy_toolsets(None, &all_enabled);
        assert!(!default_lazy.contains(&"google_ads".to_string()));
        assert!(!default_lazy.contains(&"bigquery".to_string()));
        assert!(default_lazy.contains(&"drive".to_string()));
        assert!(default_lazy.contains(&"youtube".to_string()));
        assert!(default_lazy.contains(&"google_genai".to_string()));
    }

    #[test]
    fn thinking_settings_defaults_and_overrides() {
        let default_cfg = ThinkingSettings::parse(None, None, None);
        assert_eq!(default_cfg.mode, ThinkingMode::AutoFast);

        let hydrogen = default_cfg
            .thinking_config_for_model("gemini-4-hydrogen")
            .expect("gemini 4 hydrogen gets low thinking by default");
        assert_eq!(
            hydrogen.thinking_level,
            Some(adk_gemini::ThinkingLevel::Low)
        );
        assert_eq!(hydrogen.thinking_budget, None);
        hydrogen.validate().unwrap();

        let flash25 = default_cfg
            .thinking_config_for_model("gemini-2.5-flash")
            .expect("gemini 2.5 flash gets fast budget");
        assert_eq!(flash25.thinking_budget, Some(512));
        assert_eq!(flash25.thinking_level, None);
        flash25.validate().unwrap();

        for media_model in [
            "gemini-nano-banana-2.1",
            "gemini-3.1-flash-image",
            "gemini-omni-1.2-flash",
        ] {
            assert!(
                default_cfg.thinking_config_for_model(media_model).is_none(),
                "media models never receive thinking_config ({media_model})"
            );
        }

        let auto_cfg = ThinkingSettings::parse(Some("auto"), None, None);
        assert!(
            auto_cfg
                .thinking_config_for_model("gemini-4-hydrogen")
                .is_none()
        );

        // The Gemini 4 presets, including the default, get low thinking by
        // default; all three list `low` as a supported level.
        for model in ["gemini-4-hydrogen", "gemini-4-carbon", "gemini-4-argon"] {
            let cfg = default_cfg
                .thinking_config_for_model(model)
                .unwrap_or_else(|| panic!("{model} gets a thinking config"));
            assert_eq!(cfg.thinking_level, Some(adk_gemini::ThinkingLevel::Low));
        }
        assert!(PRESET_MODELS.contains(&DEFAULT_MODEL));

        // An explicit `minimal` is downgraded to Low where Minimal is not
        // supported (non-Flash Gemini 3/4), and kept on Flash.
        let minimal_cfg = ThinkingSettings::parse(Some("minimal"), None, None);
        assert_eq!(
            minimal_cfg
                .thinking_config_for_model("gemini-4-argon")
                .unwrap()
                .thinking_level,
            Some(adk_gemini::ThinkingLevel::Low)
        );
        assert_eq!(
            minimal_cfg
                .thinking_config_for_model("gemini-3.1-pro")
                .unwrap()
                .thinking_level,
            Some(adk_gemini::ThinkingLevel::Low)
        );
        assert_eq!(
            minimal_cfg
                .thinking_config_for_model("gemini-3.8-flash")
                .unwrap()
                .thinking_level,
            Some(adk_gemini::ThinkingLevel::Minimal)
        );
        assert_eq!(
            minimal_cfg
                .thinking_config_for_model("gemini-2.5-flash")
                .unwrap()
                .thinking_budget,
            Some(0)
        );

        // A budget below -1 is ignored, and a budget is ignored when a level is set.
        assert_eq!(
            ThinkingSettings::parse(None, Some("-5"), None).mode,
            ThinkingMode::AutoFast
        );
        assert_eq!(
            ThinkingSettings::parse(None, Some("-1"), None).mode,
            ThinkingMode::Budget(-1)
        );
        assert_eq!(
            ThinkingSettings::parse(Some("high"), Some("1024"), None).mode,
            ThinkingMode::Level(adk_gemini::ThinkingLevel::High)
        );

        let high_cfg = ThinkingSettings::parse(Some("high"), None, Some("true"));
        let argon = high_cfg
            .thinking_config_for_model("gemini-4-argon")
            .unwrap();
        assert_eq!(argon.thinking_level, Some(adk_gemini::ThinkingLevel::High));
        assert_eq!(argon.include_thoughts, Some(true));
        argon.validate().unwrap();
    }

    #[test]
    fn parses_boolean_flags_leniently() {
        for truthy in ["1", "true", "TRUE", " Yes ", "on", "y", "t"] {
            assert_eq!(parse_bool(truthy), Some(true), "{truthy:?}");
        }
        for falsy in ["0", "false", "False", "no", "OFF", "n", "f"] {
            assert_eq!(parse_bool(falsy), Some(false), "{falsy:?}");
        }
        for invalid in ["", "maybe", "2", "enabled"] {
            assert_eq!(parse_bool(invalid), None, "{invalid:?}");
        }
    }

    #[test]
    fn pricing_table_covers_presets_and_extra_models() {
        let table = model_pricing_table(["Custom-Model-X ", ""]);
        for preset in PRESET_MODELS {
            assert_eq!(
                table.get(*preset).copied(),
                Some(model_pricing(preset)),
                "preset {preset} present with the server rates"
            );
        }
        assert_eq!(
            table.get("custom-model-x").copied(),
            Some(model_pricing("custom-model-x")),
            "extra names are normalised and priced"
        );
        assert!(!table.contains_key(""), "blank names are skipped");
    }

    #[test]
    fn tool_filters_are_unique_and_within_gemini_name_limit() {
        for filter in [
            DISPLAY_VIDEO_360_TOOL_FILTER,
            DRIVE_TOOL_FILTER,
            STORAGE_TOOL_FILTER,
            YOUTUBE_TOOL_FILTER,
            DEFAULT_GOOGLE_ADS_TOOL_FILTER,
        ] {
            let mut sorted = filter.to_vec();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(sorted.len(), filter.len(), "duplicate names in filter");
            assert!(
                filter.iter().all(|n| n.len() <= 60),
                "tool names are truncated to 60 chars"
            );
        }
    }

    #[test]
    fn estimates_token_usage_cost_across_model_families() {
        let usage = adk_rust::UsageMetadata {
            prompt_token_count: 100_000,
            candidates_token_count: 10_000,
            total_token_count: 110_000,
            cache_read_input_token_count: Some(40_000),
            thinking_token_count: Some(2_000),
            ..Default::default()
        };
        // gemini-2.5-flash: 60k uncached * $0.30/1M ($0.018) + 40k cached * $0.03/1M ($0.0012) + 10k out * $2.50/1M ($0.025) = $0.0442
        let flash_cost = estimate_usage_cost_usd("gemini-2.5-flash", &usage);
        assert!((flash_cost - 0.0442).abs() < 1e-9, "got {flash_cost}");

        // gemini-3.5-flash-lite: 60k * $0.30/1M ($0.018) + 40k * $0.03/1M ($0.0012) + 10k * $2.50/1M ($0.025) = $0.0442
        let lite_cost = estimate_usage_cost_usd("gemini-3.5-flash-lite", &usage);
        assert!((lite_cost - 0.0442).abs() < 1e-9, "got {lite_cost}");

        // gemini-3.8-flash: 60k * $0.75/1M ($0.045) + 40k * $0.075/1M ($0.003) + 10k * $3.75/1M ($0.0375) = $0.0855
        let flash38_cost = estimate_usage_cost_usd("gemini-3.8-flash", &usage);
        assert!((flash38_cost - 0.0855).abs() < 1e-9, "got {flash38_cost}");

        // gemini-4-argon: 60k * $2.00/1M ($0.12) + 40k * $0.10/1M ($0.004) + 10k * $10.00/1M ($0.10) = $0.224
        let argon_cost = estimate_usage_cost_usd("gemini-4-argon", &usage);
        assert!((argon_cost - 0.224).abs() < 1e-9, "got {argon_cost}");

        // gemini-4-carbon shares Argon's rates.
        let carbon_cost = estimate_usage_cost_usd("gemini-4-carbon", &usage);
        assert!((carbon_cost - 0.224).abs() < 1e-9, "got {carbon_cost}");

        // gemini-4-hydrogen: 60k * $1.50/1M ($0.09) + 40k * $0.075/1M ($0.003) + 10k * $7.50/1M ($0.075) = $0.168
        let hydrogen_cost = estimate_usage_cost_usd("gemini-4-hydrogen", &usage);
        assert!((hydrogen_cost - 0.168).abs() < 1e-9, "got {hydrogen_cost}");
    }
}
