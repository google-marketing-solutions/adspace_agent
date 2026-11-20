# Copyright 2026 Google LLC
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     https://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.
"""Configuration settings and toolset filters for the AdSpace Agent."""

import logging
import os
import re

from dotenv import load_dotenv

_ = load_dotenv()

logger = logging.getLogger(__name__)

APP_NAME = "adspace_agent"

MODEL: str = os.environ.get("MODEL", "gemini-3.8-flash")

COMPACTION_INTERVAL: int = int(os.environ.get("COMPACTION_INTERVAL", "3"))
COMPACTION_OVERLAP_SIZE: int = int(
    os.environ.get("COMPACTION_OVERLAP_SIZE", "1")
)
COMPACTION_TOKEN_THRESHOLD: int = int(
    os.environ.get("COMPACTION_TOKEN_THRESHOLD", "500000")
)
COMPACTION_EVENT_RETENTION_SIZE: int = int(
    os.environ.get("COMPACTION_EVENT_RETENTION_SIZE", "3")
)

CLIENT_ID: str = os.environ.get("CLIENT_ID", "")
CLIENT_SECRET: str = os.environ.get("CLIENT_SECRET", "")

GOOGLE_ADS_API_VERSION = "v25"
GOOGLE_ADS_DEVELOPER_TOKEN: str = os.environ.get(
    "GOOGLE_ADS_DEVELOPER_TOKEN", ""
)
GOOGLE_ADS_LOGIN_CUSTOMER_ID: str | None = os.environ.get(
    "GOOGLE_ADS_LOGIN_CUSTOMER_ID"
)

DEFAULT_GOOGLE_ADS_TOOL_FILTER: list[str] = [
    "googleads_customers_google_ads_search",
    "googleads_google_ads_fields_search",
    "googleads_customers_generate_audience_definition",
    "googleads_customers_generate_audience_composition_insights",
]

ALL_GOOGLE_TOOLSETS: list[str] = [
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
]

BID_MANAGER_TOOL_FILTER: list[str] = [
    "doubleclickbidmanager_queries_get",
    "doubleclickbidmanager_queries_list",
    "doubleclickbidmanager_queries_reports_get",
    "doubleclickbidmanager_queries_reports_list",
]

CAMPAIGN_MANAGER_360_TOOL_FILTER: list[str] = [
    "dfareporting_account_active_ad_summaries_get",
    "dfareporting_account_permission_groups_get",
    "dfareporting_account_permission_groups_list",
    "dfareporting_account_permissions_get",
    "dfareporting_account_permissions_list",
    "dfareporting_account_user_profiles_get",
    "dfareporting_account_user_profiles_list",
    "dfareporting_accounts_get",
    "dfareporting_accounts_list",
    "dfareporting_ads_get",
    "dfareporting_ads_list",
    "dfareporting_advertiser_groups_get",
    "dfareporting_advertiser_groups_list",
    "dfareporting_advertiser_invoices_list",
    "dfareporting_advertiser_landing_pages_get",
    "dfareporting_advertiser_landing_pages_list",
    "dfareporting_advertisers_get",
    "dfareporting_advertisers_list",
    "dfareporting_billing_assignments_list",
    "dfareporting_billing_profiles_get",
    "dfareporting_billing_profiles_list",
    "dfareporting_billing_rates_list",
    "dfareporting_browsers_list",
    "dfareporting_campaign_creative_associations_list",
    "dfareporting_campaigns_get",
    "dfareporting_campaigns_list",
    "dfareporting_change_logs_get",
    "dfareporting_change_logs_list",
    "dfareporting_cities_list",
    "dfareporting_connection_types_get",
    "dfareporting_connection_types_list",
    "dfareporting_content_categories_get",
    "dfareporting_content_categories_list",
    "dfareporting_countries_get",
    "dfareporting_countries_list",
    "dfareporting_creative_field_values_get",
    "dfareporting_creative_field_values_list",
    "dfareporting_creative_fields_get",
    "dfareporting_creative_fields_list",
    "dfareporting_creative_groups_get",
    "dfareporting_creative_groups_list",
    "dfareporting_creatives_get",
    "dfareporting_creatives_list",
    "dfareporting_dimension_values_query",
    "dfareporting_directory_sites_get",
    "dfareporting_directory_sites_list",
    "dfareporting_dynamic_feeds_get",
    "dfareporting_dynamic_profiles_generate_code",
    "dfareporting_dynamic_profiles_get",
    "dfareporting_dynamic_targeting_keys_list",
    "dfareporting_event_tags_get",
    "dfareporting_event_tags_list",
    "dfareporting_files_get",
    "dfareporting_files_list",
    "dfareporting_floodlight_activities_get",
    "dfareporting_floodlight_activities_list",
    "dfareporting_floodlight_activity_groups_get",
    "dfareporting_floodlight_activity_groups_list",
    "dfareporting_floodlight_configurations_get",
    "dfareporting_floodlight_configurations_list",
    "dfareporting_languages_list",
    "dfareporting_metros_list",
    "dfareporting_mobile_apps_get",
    "dfareporting_mobile_apps_list",
    "dfareporting_mobile_carriers_get",
    "dfareporting_mobile_carriers_list",
    "dfareporting_operating_system_versions_get",
    "dfareporting_operating_system_versions_list",
    "dfareporting_operating_systems_get",
    "dfareporting_operating_systems_list",
    "dfareporting_placement_groups_get",
    "dfareporting_placement_groups_list",
    "dfareporting_placement_strategies_get",
    "dfareporting_placement_strategies_list",
    "dfareporting_placements_get",
    "dfareporting_placements_list",
    "dfareporting_platform_types_get",
    "dfareporting_platform_types_list",
    "dfareporting_postal_codes_get",
    "dfareporting_postal_codes_list",
    "dfareporting_regions_list",
    "dfareporting_remarketing_list_shares_get",
    "dfareporting_remarketing_lists_get",
    "dfareporting_remarketing_lists_list",
    "dfareporting_report_data_query",
    "dfareporting_reports_compatible_fields_query",
    "dfareporting_reports_files_get",
    "dfareporting_reports_files_list",
    "dfareporting_reports_get",
    "dfareporting_reports_list",
    "dfareporting_sites_get",
    "dfareporting_sites_list",
    "dfareporting_sizes_get",
    "dfareporting_sizes_list",
    "dfareporting_studio_creatives_get",
    "dfareporting_subaccounts_get",
    "dfareporting_subaccounts_list",
    "dfareporting_targetable_remarketing_lists_get",
    "dfareporting_targetable_remarketing_lists_list",
    "dfareporting_targeting_templates_get",
    "dfareporting_targeting_templates_list",
    "dfareporting_tv_campaign_details_get",
    "dfareporting_tv_campaign_summaries_list",
    "dfareporting_user_profiles_get",
    "dfareporting_user_profiles_list",
    "dfareporting_user_role_permission_groups_get",
    "dfareporting_user_role_permission_groups_list",
    "dfareporting_user_role_permissions_get",
    "dfareporting_user_role_permissions_list",
    "dfareporting_user_roles_get",
    "dfareporting_user_roles_list",
    "dfareporting_video_formats_get",
    "dfareporting_video_formats_list",
]

DISPLAY_VIDEO_360_TOOL_FILTER: list[str] = [
    "displayvideo_advertisers_ad_assets_get",
    "displayvideo_advertisers_ad_assets_list",
    "displayvideo_advertisers_ad_group_ads_get",
    "displayvideo_advertisers_ad_group_ads_list",
    "displayvideo_advertisers_ad_groups_bulk_list_assigned_target",
    "displayvideo_advertisers_ad_groups_get",
    "displayvideo_advertisers_ad_groups_list",
    "displayvideo_advertisers_audit",
    "displayvideo_advertisers_campaigns_get",
    "displayvideo_advertisers_campaigns_list",
    "displayvideo_advertisers_channels_get",
    "displayvideo_advertisers_channels_list",
    "displayvideo_advertisers_channels_sites_list",
    "displayvideo_advertisers_creatives_get",
    "displayvideo_advertisers_creatives_list",
    "displayvideo_advertisers_get",
    "displayvideo_advertisers_insertion_orders_get",
    "displayvideo_advertisers_insertion_orders_list",
    "displayvideo_advertisers_invoices_list",
    "displayvideo_advertisers_invoices_lookup_invoice_currency",
    "displayvideo_advertisers_line_items_get",
    "displayvideo_advertisers_line_items_list",
    "displayvideo_advertisers_list",
    "displayvideo_advertisers_location_lists_get",
    "displayvideo_advertisers_location_lists_list",
    "displayvideo_advertisers_negative_keyword_lists_get",
    "displayvideo_advertisers_negative_keyword_lists_list",
    "displayvideo_combined_audiences_get",
    "displayvideo_combined_audiences_list",
    "displayvideo_custom_bidding_algorithms_get",
    "displayvideo_custom_bidding_algorithms_list",
    "displayvideo_custom_bidding_algorithms_rules_get",
    "displayvideo_custom_bidding_algorithms_rules_list",
    "displayvideo_custom_bidding_algorithms_scripts_get",
    "displayvideo_custom_bidding_algorithms_scripts_list",
    "displayvideo_custom_lists_get",
    "displayvideo_custom_lists_list",
    "displayvideo_first_party_and_partner_audiences_get",
    "displayvideo_first_party_and_partner_audiences_list",
    "displayvideo_floodlight_groups_floodlight_activities_get",
    "displayvideo_floodlight_groups_floodlight_activities_list",
    "displayvideo_floodlight_groups_get",
    "displayvideo_google_audiences_get",
    "displayvideo_google_audiences_list",
    "displayvideo_guaranteed_orders_get",
    "displayvideo_guaranteed_orders_list",
    "displayvideo_inventory_source_groups_get",
    "displayvideo_inventory_source_groups_list",
    "displayvideo_inventory_sources_get",
    "displayvideo_inventory_sources_list",
    "displayvideo_media_download",
    "displayvideo_partners_channels_get",
    "displayvideo_partners_channels_list",
    "displayvideo_partners_channels_sites_list",
    "displayvideo_partners_get",
    "displayvideo_partners_list",
    "displayvideo_sdfdownloadtasks_operations_get",
    "displayvideo_sdfuploadtasks_operations_get",
    "displayvideo_targeting_types_targeting_options_get",
    "displayvideo_targeting_types_targeting_options_list",
    "displayvideo_targeting_types_targeting_options_search",
    "displayvideo_users_get",
    "displayvideo_users_list",
]

MERCHANT_CENTER_INVENTORIES_TOOL_FILTER: list[str] = [
    "merchantapi_accounts_products_local_inventories_list",
    "merchantapi_accounts_products_regional_inventories_list",
]

MERCHANT_CENTER_PRODUCTS_TOOL_FILTER: list[str] = [
    "merchantapi_accounts_products_get",
    "merchantapi_accounts_products_list",
]

MERCHANT_CENTER_REPORTS_TOOL_FILTER: list[str] = [
    "merchantapi_accounts_reports_search",
]

SEARCH_ADS_360_TOOL_FILTER: list[str] = [
    "searchads360_customers_custom_columns_get",
    "searchads360_customers_custom_columns_list",
    "searchads360_customers_list_accessible_customers",
    "searchads360_customers_search_ads360_search",
    "searchads360_search_ads360_fields_get",
    "searchads360_search_ads360_fields_search",
]

DRIVE_TOOL_FILTER: list[str] = [
    "drive_drives_list",
    "drive_drives_get",
    "drive_files_list",
    "drive_files_get",
    "drive_files_download",
    "drive_files_export",
    "drive_permissions_list",
    "drive_teamdrives_list",
    "drive_teamdrives_get",
]

STORAGE_TOOL_FILTER: list[str] = [
    "storage_buckets_get",
    "storage_buckets_list",
    "storage_folders_get",
    "storage_folders_list",
    "storage_objects_copy",
    "storage_objects_get",
    "storage_objects_list",
]

YOUTUBE_TOOL_FILTER: list[str] = [
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
]


_RESOURCE_PREFIX_PATTERN: re.Pattern[str] = re.compile(
    r"^(?:advertisers|customers|accounts|partners|userProfiles)/",
    re.IGNORECASE,
)


def resolve_tool_filter(
    env_var: str, default_filter: list[str]
) -> list[str] | None:
    """Resolves a toolset filter from an environment variable.

    Args:
        env_var: The environment variable name to read.
        default_filter: The default list of allowed tool names when the env
            var is unset or blank.

    Returns:
        A list of allowed tool names, or None if all tools are enabled via
        '*' or 'all'.
    """
    raw = os.environ.get(env_var)
    if raw is None:
        return list(default_filter)
    parsed = [t.strip() for t in raw.split(",") if t.strip()]
    if not parsed:
        return list(default_filter)
    lowered = [t.lower() for t in parsed]
    if lowered in (["*"], ["all"]):
        return None
    if lowered == ["none"]:
        return []
    return parsed


def get_google_ads_tool_filter() -> list[str] | None:
    """Resolves the Google Ads tool filter from the environment.

    Returns:
        A list of allowed tool names, or None if all tools are enabled via
        '*' or 'all'.
    """
    return resolve_tool_filter(
        "GOOGLE_ADS_TOOL_FILTER", DEFAULT_GOOGLE_ADS_TOOL_FILTER
    )


def get_toolset_filters() -> dict[str, list[str] | None]:
    """Resolves tool filters for all marketing Google API toolsets.

    Returns:
        A mapping from toolset key to resolved tool filter list or None.
    """
    return {
        "bid_manager": resolve_tool_filter(
            "BID_MANAGER_TOOL_FILTER", BID_MANAGER_TOOL_FILTER
        ),
        "campaign_manager_360": resolve_tool_filter(
            "CAMPAIGN_MANAGER_360_TOOL_FILTER",
            CAMPAIGN_MANAGER_360_TOOL_FILTER,
        ),
        "display_video_360": resolve_tool_filter(
            "DISPLAY_VIDEO_360_TOOL_FILTER", DISPLAY_VIDEO_360_TOOL_FILTER
        ),
        "google_ads": get_google_ads_tool_filter(),
        "merchant_center_inventories": resolve_tool_filter(
            "MERCHANT_CENTER_INVENTORIES_TOOL_FILTER",
            MERCHANT_CENTER_INVENTORIES_TOOL_FILTER,
        ),
        "merchant_center_products": resolve_tool_filter(
            "MERCHANT_CENTER_PRODUCTS_TOOL_FILTER",
            MERCHANT_CENTER_PRODUCTS_TOOL_FILTER,
        ),
        "merchant_center_reports": resolve_tool_filter(
            "MERCHANT_CENTER_REPORTS_TOOL_FILTER",
            MERCHANT_CENTER_REPORTS_TOOL_FILTER,
        ),
        "search_ads_360": resolve_tool_filter(
            "SEARCH_ADS_360_TOOL_FILTER", SEARCH_ADS_360_TOOL_FILTER
        ),
    }


def normalize_account_id(raw_id: str) -> str:
    """Normalizes an account or customer ID for allowlist comparisons.

    Args:
        raw_id: The raw account ID or resource-prefixed identifier.

    Returns:
        The normalized account ID with whitespace, known resource prefixes,
        and hyphens removed.
    """
    stripped = _RESOURCE_PREFIX_PATTERN.sub("", raw_id.strip())
    return stripped.replace("-", "")


def get_allowed_mutate_account_ids() -> set[str] | None:
    """Resolves the optional account allowlist for mutating marketing tools.

    Returns:
        A set of normalized allowed account IDs, an empty set when mutations
        are disallowed via 'none', or None when unrestricted.
    """
    raw = os.environ.get("ALLOWED_MUTATE_ACCOUNT_IDS")
    if raw is None:
        return None
    parsed = [t.strip() for t in raw.split(",") if t.strip()]
    if not parsed:
        return None
    lowered = [t.lower() for t in parsed]
    if lowered in (["*"], ["all"]):
        return None
    if lowered == ["none"]:
        return set()
    return {
        normalized
        for item in parsed
        if (normalized := normalize_account_id(item))
    }


def get_enabled_toolsets() -> list[str]:
    """Resolves and validates enabled Google toolsets from the environment.

    Returns:
        A list of enabled toolset names.

    Raises:
        ValueError: If unknown toolset names are configured in
            ENABLED_TOOLSETS.
    """
    raw = os.environ.get("ENABLED_TOOLSETS")
    if raw is None:
        return list(ALL_GOOGLE_TOOLSETS)
    parsed = [t.strip() for t in raw.split(",") if t.strip()]
    if not parsed:
        return list(ALL_GOOGLE_TOOLSETS)
    lowered = [t.lower() for t in parsed]
    if lowered in (["*"], ["all"]):
        return list(ALL_GOOGLE_TOOLSETS)
    if lowered == ["none"]:
        return []
    unknown = sorted(set(parsed) - set(ALL_GOOGLE_TOOLSETS))
    if unknown:
        valid_str = ", ".join(ALL_GOOGLE_TOOLSETS)
        unknown_str = ", ".join(unknown)
        msg = (
            f"Unknown toolset(s) in ENABLED_TOOLSETS: {unknown_str}. "
            f"Valid toolsets are: {valid_str}"
        )
        logger.warning(msg)
        raise ValueError(msg)
    return parsed


GOOGLE_ADS_TOOL_FILTER_ENV: str | None = os.environ.get(
    "GOOGLE_ADS_TOOL_FILTER"
)
GOOGLE_ADS_TOOL_FILTER: list[str] | None = get_google_ads_tool_filter()

ENABLED_TOOLSETS_ENV: str | None = os.environ.get("ENABLED_TOOLSETS")
ENABLED_TOOLSETS: list[str] = get_enabled_toolsets()
