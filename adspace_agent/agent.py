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
"""The AdSpace Agent main application."""

import os

from dotenv import load_dotenv
from google.adk.agents import Agent
from google.adk.agents.callback_context import CallbackContext
from google.adk.agents.context_cache_config import ContextCacheConfig
from google.adk.apps import App
from google.adk.apps.app import EventsCompactionConfig
from google.adk.auth.auth_schemes import OpenIdConnectWithConfig
from google.adk.models.google_llm import Gemini
from google.adk.skills import list_skills_in_dir
from google.adk.skills import list_skills_in_gcs_dir
from google.adk.skills import load_skill_from_dir
from google.adk.skills import load_skill_from_gcs_dir
from google.adk.skills import Skill
from google.adk.tools import skill_toolset
from google.adk.tools.google_api_tool import BigQueryToolset
from google.adk.tools.google_api_tool import GoogleApiToolset
from google.adk.tools.google_api_tool import YoutubeToolset
from google.adk.tools.google_api_tool.googleapi_to_openapi_converter import (
    GoogleApiToOpenApiConverter,
)
from google.adk.tools.load_artifacts_tool import LoadArtifactsTool
from google.adk.tools.openapi_tool import OpenAPIToolset
from google.adk.tools.preload_memory_tool import PreloadMemoryTool
from google_ads_adk.toolset import GoogleAdsToolset

from .tools.data_analysis import DataAnalysisToolset
from .tools.google_genai import GoogleGenAIToolset
from .tools.utilities import UtilitiesToolset

APP_NAME = "adspace_agent"
MODEL: str = os.environ.get("MODEL", "gemini-3.1-flash-lite-preview")
CLIENT_ID: str = os.environ["CLIENT_ID"]
CLIENT_SECRET: str = os.environ["CLIENT_SECRET"]
GOOGLE_ADS_DEVELOPER_TOKEN: str = os.environ["GOOGLE_ADS_DEVELOPER_TOKEN"]
GOOGLE_ADS_LOGIN_CUSTOMER_ID: str | None = os.environ.get(
    "GOOGLE_ADS_LOGIN_CUSTOMER_ID"
)

GOOGLE_ADS_TOOL_FILTER_ENV = os.environ.get("GOOGLE_ADS_TOOL_FILTER")
GOOGLE_ADS_TOOL_FILTER: list[str] = (
    [t.strip() for t in GOOGLE_ADS_TOOL_FILTER_ENV.split(",") if t.strip()]
    if GOOGLE_ADS_TOOL_FILTER_ENV
    else [
        "googleads_customers_google_ads_search",
        "googleads_google_ads_fields_search",
    ]
)

ALL_GOOGLE_TOOLSETS = [
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

enabled_toolsets_env = os.environ.get("ENABLED_TOOLSETS")
ENABLED_TOOLSETS: list[str] = (
    [t.strip() for t in enabled_toolsets_env.split(",") if t.strip()]
    if enabled_toolsets_env is not None
    else ALL_GOOGLE_TOOLSETS
)


async def auto_save_session_to_memory_callback(
    callback_context: CallbackContext,
) -> None:
    """Saves the session to memory automatically.

    Args:
        callback_context: The context object containing session info.
    """
    await callback_context.add_session_to_memory()


def create_agent() -> Agent:  # noqa: PLR0914
    """Creates and configures the AdSpace Agent.

    Returns:
        Agent: The configured AdSpace Agent.
    """
    _ = load_dotenv()

    bid_manager_toolset = GoogleApiToolset(
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
        api_name="doubleclickbidmanager",
        api_version="v2",
    )

    bigquery_toolset = BigQueryToolset(
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
    )

    campaign_manager_360_toolset = GoogleApiToolset(
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
        api_name="dfareporting",
        api_version="v5",
    )

    # Overwrite the auto-selected scope for dfareporting to fix 401 error
    spec_dict = GoogleApiToOpenApiConverter("dfareporting", "v5").convert()
    campaign_manager_360_toolset._openapi_toolset = OpenAPIToolset(  # noqa: SLF001
        spec_dict=spec_dict,
        spec_str_type="yaml",
        auth_scheme=OpenIdConnectWithConfig(
            authorization_endpoint="https://accounts.google.com/o/oauth2/v2/auth",
            token_endpoint="https://oauth2.googleapis.com/token",  # noqa: S106
            userinfo_endpoint="https://openidconnect.googleapis.com/v1/userinfo",
            revocation_endpoint="https://oauth2.googleapis.com/revoke",
            token_endpoint_auth_methods_supported=[
                "client_secret_post",
                "client_secret_basic",
            ],
            grant_types_supported=["authorization_code"],
            scopes=[
                "https://www.googleapis.com/auth/dfareporting",
                "https://www.googleapis.com/auth/dfatrafficking",
                "https://www.googleapis.com/auth/ddmconversions",
            ],
        ),
    )

    display_video_360_toolset = GoogleApiToolset(
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
        api_name="displayvideo",
        api_version="v4",
        tool_filter=[
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
        ],
    )

    drive_toolset = GoogleApiToolset(
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
        api_name="drive",
        api_version="v3",
        tool_filter=[
            "drive_drives_list",
            "drive_drives_get",
            "drive_files_list",
            "drive_files_get",
            "drive_files_download",
            "drive_files_export",
            "drive_permissions_list",
            "drive_teamdrives_list",
            "drive_teamdrives_get",
        ],
    )

    merchant_center_inventories_toolset = GoogleApiToolset(
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
        api_name="merchantapi",
        api_version="inventories_v1",
    )

    merchant_center_products_toolset = GoogleApiToolset(
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
        api_name="merchantapi",
        api_version="products_v1",
    )

    merchant_center_reports_toolset = GoogleApiToolset(
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
        api_name="merchantapi",
        api_version="reports_v1",
    )

    search_ads_360_toolset = GoogleApiToolset(
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
        api_name="searchads360",
        api_version="v0",
    )

    storage_toolset = GoogleApiToolset(
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
        api_name="storage",
        api_version="v1",
        tool_filter=[
            "storage_buckets_get",
            "storage_buckets_list",
            "storage_folders_get",
            "storage_folders_list",
            "storage_objects_copy",
            "storage_objects_get",
            "storage_objects_list",
        ],
    )

    youtube_toolset = YoutubeToolset(
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
        tool_filter=[
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
        ],
    )

    google_ads_toolset = GoogleAdsToolset(
        developer_token=GOOGLE_ADS_DEVELOPER_TOKEN,
        client_id=CLIENT_ID,
        client_secret=CLIENT_SECRET,
        login_customer_id=GOOGLE_ADS_LOGIN_CUSTOMER_ID,
        tool_filter=GOOGLE_ADS_TOOL_FILTER,
    )

    google_genai_toolset = GoogleGenAIToolset()

    skill_map: dict[str, Skill] = {}

    if os.environ.get("SKILLS_BUCKET_NAME"):
        gcs_skills = list_skills_in_gcs_dir(
            os.environ["SKILLS_BUCKET_NAME"],
            "skills",
        )
        for skill_id in gcs_skills:
            skill_map[skill_id] = load_skill_from_gcs_dir(
                bucket_name=os.environ["SKILLS_BUCKET_NAME"],
                skill_id=skill_id,
                skills_base_path="skills",
            )

    local_skills = list_skills_in_dir("./skills")
    for skill_id in local_skills:
        skill_map[skill_id] = load_skill_from_dir(f"./skills/{skill_id}")

    skills = list(skill_map.values())

    skills_toolset = skill_toolset.SkillToolset(
        skills=skills,
    )

    tools = []
    toolset_map = {
        "bid_manager": bid_manager_toolset,
        "bigquery": bigquery_toolset,
        "campaign_manager_360": campaign_manager_360_toolset,
        "display_video_360": display_video_360_toolset,
        "drive": drive_toolset,
        "google_ads": google_ads_toolset,
        "google_genai": google_genai_toolset,
        "merchant_center_inventories": merchant_center_inventories_toolset,
        "merchant_center_products": merchant_center_products_toolset,
        "merchant_center_reports": merchant_center_reports_toolset,
        "search_ads_360": search_ads_360_toolset,
        "storage": storage_toolset,
        "youtube": youtube_toolset,
    }

    # Add special tools that are not in the map
    tools.extend([
        DataAnalysisToolset(),
        PreloadMemoryTool(),
        UtilitiesToolset(),
        LoadArtifactsTool(),
        skills_toolset,
    ])

    for key, toolset in toolset_map.items():
        if key in ENABLED_TOOLSETS:
            tools.append(toolset)

    return Agent(
        name=APP_NAME,
        model=Gemini(
            model=MODEL,
        ),
        description=(
            "AdSpace Agent is designed to provide a standardized way to "
            "integrate an LLM with Google Ads, Display & Video 360, "
            "Campaign Manager 360, Search Ads 360, YouTube, Google Drive, "
            "and Google Cloud storage to form a more comprehensive campaign "
            "and marketing plan for agencies."
        ),
        instruction=(
            "You are a helpful agent who can answer user questions about "
            "ads, creatives, data science, performance, analytics, and "
            "campaigns. NOTE: Tools for Search Ads 360 and tools for Google "
            "Ads are distinct. DO NOT group tools from one platform with "
            "tools from another platform if asked what tools are available. "
            "When asked for multiple pieces of information or to perform "
            "multiple actions, always try to call functions in parallel if "
            "possible."
        ),
        tools=tools,
        after_agent_callback=auto_save_session_to_memory_callback,
    )


root_agent = create_agent()

app = App(
    name=APP_NAME,
    root_agent=root_agent,
    context_cache_config=ContextCacheConfig(
        min_tokens=2048,  # Minimum tokens to trigger caching
        ttl_seconds=600,  # Store for up to 10 minutes
        cache_intervals=5,  # Refresh after 5 uses
    ),
    events_compaction_config=EventsCompactionConfig(
        compaction_interval=3,  # Trigger compaction every 3 new invocations.
        overlap_size=1,  # Include last invocation from the previous window.
    ),
)
