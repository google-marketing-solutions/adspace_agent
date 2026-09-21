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

DISPLAY_VIDEO_360_TOOL_FILTER: list[str] = [
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
    "displayvideo_advertisers_campaigns_list_assigned_targeting_o",
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
    "displayvideo_advertisers_insertion_orders_list_assigned_targ",
    "displayvideo_advertisers_invoices_list",
    "displayvideo_advertisers_invoices_lookup_invoice_currency",
    "displayvideo_advertisers_line_items_bulk_edit_assigned_targe",
    "displayvideo_advertisers_line_items_bulk_update",
    "displayvideo_advertisers_line_items_list",
    "displayvideo_advertisers_line_items_create",
    "displayvideo_advertisers_line_items_get",
    "displayvideo_advertisers_line_items_patch",
    "displayvideo_advertisers_line_items_duplicate",
    "displayvideo_advertisers_line_items_generate_default",
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


def get_google_ads_tool_filter() -> list[str] | None:
    """Resolves the Google Ads tool filter from the environment.

    Returns:
        A list of allowed tool names, or None if all tools are enabled via
        '*' or 'all'.
    """
    raw = os.environ.get("GOOGLE_ADS_TOOL_FILTER")
    if raw is None:
        return list(DEFAULT_GOOGLE_ADS_TOOL_FILTER)
    parsed = [t.strip() for t in raw.split(",") if t.strip()]
    if not parsed:
        return list(DEFAULT_GOOGLE_ADS_TOOL_FILTER)
    lowered = [t.lower() for t in parsed]
    if lowered in (["*"], ["all"]):
        return None
    if lowered == ["none"]:
        return []
    return parsed


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
