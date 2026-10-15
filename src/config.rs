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
pub const DEFAULT_MODEL: &str = "gemini-3.8-flash";
/// Default Veo video generation model.
pub const DEFAULT_VIDEO_MODEL: &str = "veo-3.1-fast-generate-001";
/// Default image generation model.
pub const DEFAULT_IMAGE_MODEL: &str = "gemini-3.1-flash-image";

/// Default events-compaction interval (in invocations).
pub const DEFAULT_COMPACTION_INTERVAL: u64 = 3;
/// Default events-compaction overlap size (in invocations).
pub const DEFAULT_COMPACTION_OVERLAP_SIZE: u64 = 1;
/// Default events-compaction token threshold.
pub const DEFAULT_COMPACTION_TOKEN_THRESHOLD: u64 = 500_000;
/// Default number of raw events retained after compaction.
pub const DEFAULT_COMPACTION_EVENT_RETENTION_SIZE: u64 = 3;

/// Google Ads API version used for the discovery document.
pub const GOOGLE_ADS_API_VERSION: &str = "v25";

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
/// when `LAZY_TOOLSETS` is unset: the ones large enough to dominate the
/// per-request token budget.
pub const DEFAULT_LAZY_TOOLSETS: &[&str] = &[
    "campaign_manager_360",
    "display_video_360",
    "merchant_center_products",
];

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

/// Resolves which toolsets are deferred from a raw `LAZY_TOOLSETS` value
/// (`none` disables deferral, `all` defers every Discovery toolset).
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
        return enabled
            .iter()
            .filter(|s| !matches!(s.as_str(), "bigquery" | "google_genai"))
            .cloned()
            .collect();
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

/// Whether `GOOGLE_GENAI_USE_VERTEXAI` is truthy (defaults to `true`).
pub fn use_vertex_ai() -> bool {
    env_or("GOOGLE_GENAI_USE_VERTEXAI", "True").to_uppercase() == "TRUE"
}

/// Events-compaction settings (mirrors ADK Python's `EventsCompactionConfig`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionSettings {
    /// Trigger compaction every N new invocations.
    pub compaction_interval: u64,
    /// Include N last invocations from the previous window.
    pub overlap_size: u64,
    /// Trigger compaction when the token threshold is reached.
    pub token_threshold: u64,
    /// Retain N raw events.
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
            google_cloud_project: env_opt("GOOGLE_CLOUD_PROJECT"),
            google_cloud_location: env_or("GOOGLE_CLOUD_LOCATION", "global"),
            use_vertex_ai: use_vertex_ai(),
            google_api_key: env_opt("GOOGLE_API_KEY"),
            client_id: env_or("CLIENT_ID", ""),
            client_secret: env_or("CLIENT_SECRET", ""),
            google_ads_developer_token: env_or("GOOGLE_ADS_DEVELOPER_TOKEN", ""),
            google_ads_login_customer_id: env_opt("GOOGLE_ADS_LOGIN_CUSTOMER_ID"),
            enabled_toolsets,
            google_ads_tool_filter: get_google_ads_tool_filter(),
            lazy_toolsets,
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
            strs(&["google_ads", "campaign_manager_360", "display_video_360"])
        );
        assert_eq!(
            parse_lazy_toolsets(Some("google_ads, nope"), &enabled),
            strs(&["google_ads"])
        );
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
}
