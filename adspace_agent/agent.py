# Copyright 2025 Google LLC
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

from google.adk.agents import Agent
from google.adk.artifacts import InMemoryArtifactService
from google.adk.memory import InMemoryMemoryService
from google.adk.models.google_llm import Gemini
from google.adk.sessions import InMemorySessionService
from google.adk.tools.google_api_tool import BigQueryToolset
from google.adk.tools.google_api_tool import GoogleApiToolset
from google.adk.tools.google_api_tool import YoutubeToolset

from adspace_agent.tools.data_analysis import DataAnalysisToolset
from adspace_agent.tools.google_ads import GoogleAdsToolset
from adspace_agent.tools.google_genai import GoogleGenAIToolset
from adspace_agent.tools.utilities import UtilitiesToolset

APP_NAME = "adspace_agent"
MODEL = "gemini-3-pro-preview"

session_service = InMemorySessionService()
memory_service = InMemoryMemoryService()
artifact_service = InMemoryArtifactService()

bid_manager_toolset = GoogleApiToolset(
    client_id=os.environ["CLIENT_ID"],
    client_secret=os.environ["CLIENT_SECRET"],
    api_name="doubleclickbidmanager",
    api_version="v2",
)

bigquery_toolset = BigQueryToolset(
    client_id=os.environ["CLIENT_ID"],
    client_secret=os.environ["CLIENT_SECRET"],
)

campaign_manager_360_toolset = GoogleApiToolset(
    client_id=os.environ["CLIENT_ID"],
    client_secret=os.environ["CLIENT_SECRET"],
    api_name="dfareporting",
    api_version="v5",
)

display_video_360_toolset = GoogleApiToolset(
    client_id=os.environ["CLIENT_ID"],
    client_secret=os.environ["CLIENT_SECRET"],
    api_name="displayvideo",
    api_version="v4",
    tool_filter=[
        "displayvideo_advertisers_audit",
        "displayvideo_advertisers_list",
        # "displayvideo_advertisers_create",
        "displayvideo_advertisers_get",
        # "displayvideo_advertisers_delete",
        # "displayvideo_advertisers_patch",
        "displayvideo_advertisers_edit_assigned_targeting_options",
        # "displayvideo_advertisers_ad_assets_bulk_create",
        "displayvideo_advertisers_ad_assets_list",
        # "displayvideo_advertisers_ad_assets_create",
        "displayvideo_advertisers_ad_assets_get",
        # "displayvideo_advertisers_ad_assets_upload",
        "displayvideo_advertisers_ad_group_ads_get",
        "displayvideo_advertisers_ad_group_ads_list",
        "displayvideo_advertisers_ad_groups_bulk_list_assigned_target",
        "displayvideo_advertisers_ad_groups_get",
        "displayvideo_advertisers_ad_groups_list",
        # "displayvideo_advertisers_assets_upload",
        "displayvideo_advertisers_campaigns_list",
        # "displayvideo_advertisers_campaigns_create",
        "displayvideo_advertisers_campaigns_get",
        # "displayvideo_advertisers_campaigns_delete",
        # "displayvideo_advertisers_campaigns_patch",
        "displayvideo_advertisers_campaigns_list_assigned_targeting_o",
        "displayvideo_advertisers_channels_list",
        # "displayvideo_advertisers_channels_create",
        "displayvideo_advertisers_channels_get",
        # "displayvideo_advertisers_channels_patch",
        # "displayvideo_advertisers_channels_sites_bulk_edit",
        # "displayvideo_advertisers_channels_sites_create",
        # "displayvideo_advertisers_channels_sites_delete",
        "displayvideo_advertisers_channels_sites_list",
        # "displayvideo_advertisers_channels_sites_replace",
        "displayvideo_advertisers_creatives_list",
        # "displayvideo_advertisers_creatives_create",
        "displayvideo_advertisers_creatives_get",
        # "displayvideo_advertisers_creatives_delete",
        # "displayvideo_advertisers_creatives_patch",
        "displayvideo_advertisers_insertion_orders_list",
        # "displayvideo_advertisers_insertion_orders_create",
        "displayvideo_advertisers_insertion_orders_get",
        # "displayvideo_advertisers_insertion_orders_delete",
        # "displayvideo_advertisers_insertion_orders_patch",
        "displayvideo_advertisers_insertion_orders_list_assigned_targ",
        "displayvideo_advertisers_invoices_list",
        "displayvideo_advertisers_invoices_lookup_invoice_currency",
        "displayvideo_advertisers_line_items_bulk_edit_assigned_targe",
        # "displayvideo_advertisers_line_items_bulk_update",
        "displayvideo_advertisers_line_items_list",
        # "displayvideo_advertisers_line_items_create",
        "displayvideo_advertisers_line_items_get",
        # "displayvideo_advertisers_line_items_delete",
        # "displayvideo_advertisers_line_items_patch",
        # "displayvideo_advertisers_line_items_duplicate",
        "displayvideo_advertisers_line_items_generate_default",
        "displayvideo_advertisers_location_lists_list",
        # "displayvideo_advertisers_location_lists_create",
        "displayvideo_advertisers_location_lists_get",
        # "displayvideo_advertisers_location_lists_patch",
        "displayvideo_advertisers_location_lists_assigned_locations_b",
        "displayvideo_advertisers_location_lists_assigned_locations_l",
        # "displayvideo_advertisers_location_lists_assigned_locations_c",
        # "displayvideo_advertisers_location_lists_assigned_locations_d",
        "displayvideo_advertisers_negative_keyword_lists_list",
        # "displayvideo_advertisers_negative_keyword_lists_create",
        "displayvideo_advertisers_negative_keyword_lists_get",
        # "displayvideo_advertisers_negative_keyword_lists_delete",
        # "displayvideo_advertisers_negative_keyword_lists_patch",
        "displayvideo_combined_audiences_get",
        "displayvideo_combined_audiences_list",
        "displayvideo_custom_bidding_algorithms_list",
        # "displayvideo_custom_bidding_algorithms_create",
        "displayvideo_custom_bidding_algorithms_get",
        # "displayvideo_custom_bidding_algorithms_patch",
        "displayvideo_custom_bidding_algorithms_upload_rules",
        "displayvideo_custom_bidding_algorithms_upload_script",
        "displayvideo_custom_bidding_algorithms_rules_list",
        # "displayvideo_custom_bidding_algorithms_rules_create",
        "displayvideo_custom_bidding_algorithms_rules_get",
        "displayvideo_custom_bidding_algorithms_scripts_list",
        # "displayvideo_custom_bidding_algorithms_scripts_create",
        "displayvideo_custom_bidding_algorithms_scripts_get",
        "displayvideo_custom_lists_get",
        "displayvideo_custom_lists_list",
        "displayvideo_first_party_and_partner_audiences_list",
        # "displayvideo_first_party_and_partner_audiences_create",
        # "displayvideo_first_party_and_partner_audiences_edit_customer",
        "displayvideo_first_party_and_partner_audiences_get",
        # "displayvideo_first_party_and_partner_audiences_patch",
        "displayvideo_floodlight_groups_get",
        # "displayvideo_floodlight_groups_patch",
        "displayvideo_floodlight_groups_floodlight_activities_get",
        "displayvideo_floodlight_groups_floodlight_activities_list",
        "displayvideo_google_audiences_get",
        "displayvideo_google_audiences_list",
        "displayvideo_guaranteed_orders_list",
        # "displayvideo_guaranteed_orders_create",
        "displayvideo_guaranteed_orders_edit_guaranteed_order_read_ac",
        "displayvideo_guaranteed_orders_get",
        # "displayvideo_guaranteed_orders_patch",
        "displayvideo_inventory_source_groups_list",
        # "displayvideo_inventory_source_groups_create",
        "displayvideo_inventory_source_groups_get",
        # "displayvideo_inventory_source_groups_delete",
        # "displayvideo_inventory_source_groups_patch",
        "displayvideo_inventory_sources_list",
        # "displayvideo_inventory_sources_create",
        # "displayvideo_inventory_sources_edit_inventory_source_read_wr",
        "displayvideo_inventory_sources_get",
        # "displayvideo_inventory_sources_patch",
        "displayvideo_media_download",
        # "displayvideo_media_upload",
        "displayvideo_partners_edit_assigned_targeting_options",
        "displayvideo_partners_get",
        "displayvideo_partners_list",
        "displayvideo_partners_channels_list",
        # "displayvideo_partners_channels_create",
        "displayvideo_partners_channels_get",
        # "displayvideo_partners_channels_patch",
        "displayvideo_partners_channels_sites_bulk_edit",
        "displayvideo_partners_channels_sites_create",
        # "displayvideo_partners_channels_sites_delete",
        "displayvideo_partners_channels_sites_list",
        # "displayvideo_partners_channels_sites_replace",
        "displayvideo_sdfdownloadtasks_create",
        "displayvideo_sdfdownloadtasks_operations_get",
        "displayvideo_sdfuploadtasks_operations_get",
        "displayvideo_targeting_types_targeting_options_get",
        "displayvideo_targeting_types_targeting_options_list",
        "displayvideo_targeting_types_targeting_options_search",
        "displayvideo_users_bulk_edit_assigned_user_roles",
        "displayvideo_users_list",
        # "displayvideo_users_create",
        "displayvideo_users_get",
        # "displayvideo_users_delete",
        # "displayvideo_users_patch",
    ],
)

drive_toolset = GoogleApiToolset(
    client_id=os.environ["CLIENT_ID"],
    client_secret=os.environ["CLIENT_SECRET"],
    api_name="drive",
    api_version="v3",
    tool_filter=[
        "drive_changes_list",
        "drive_comments_list",
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
    client_id=os.environ["CLIENT_ID"],
    client_secret=os.environ["CLIENT_SECRET"],
    api_name="merchantapi",
    api_version="inventories_v1",
)

merchant_center_products_toolset = GoogleApiToolset(
    client_id=os.environ["CLIENT_ID"],
    client_secret=os.environ["CLIENT_SECRET"],
    api_name="merchantapi",
    api_version="products_v1",
)

merchant_center_reports_toolset = GoogleApiToolset(
    client_id=os.environ["CLIENT_ID"],
    client_secret=os.environ["CLIENT_SECRET"],
    api_name="merchantapi",
    api_version="reports_v1",
)

search_ads_360_toolset = GoogleApiToolset(
    client_id=os.environ["CLIENT_ID"],
    client_secret=os.environ["CLIENT_SECRET"],
    api_name="searchads360",
    api_version="v0",
)

storage_toolset = GoogleApiToolset(
    client_id=os.environ["CLIENT_ID"],
    client_secret=os.environ["CLIENT_SECRET"],
    api_name="storage",
    api_version="v1",
    tool_filter=[
        "storage_buckets_get",
        "storage_buckets_list",
        "storage_folders_get",
        "storage_folders_list",
        "storage_managed_folders_list",
        "storage_object_access_controls_get",
        "storage_object_access_controls_list",
        "storage_objects_copy",
        "storage_objects_get",
        "storage_objects_list",
    ],
)

youtube_toolset = YoutubeToolset(
    client_id=os.environ["CLIENT_ID"],
    client_secret=os.environ["CLIENT_SECRET"],
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
        "youtube_members_list",
        "youtube_memberships_levels_list",
        "youtube_playlist_images_list",
        "youtube_playlist_items_list",
        "youtube_playlist_items_insert",
        "youtube_playlists_list",
        "youtube_search_list",
        "youtube_subscriptions_list",
        "youtube_super_chat_events_list",
        "youtube_video_categories_list",
        "youtube_videos_list",
        "youtube_videos_get_rating",
        "youtube_youtube_v3_live_chat_messages_stream",
    ],
)

root_agent = Agent(
    name=APP_NAME,
    model=Gemini(
        model=MODEL,
    ),
    description=(
        "AdSpace Agent is designed to provide a standardized way to integrate "
        + "an LLM with Google Ads, YouTube, and Google Cloud to form a more "
        + "comprehensive campaign and marketing plan for agencies."
    ),
    instruction=(
        "You are a helpful agent who can answer user questions about ads, "
        + "creatives, data science, performance, analytics, and campaigns."
    ),
    tools=[
        bid_manager_toolset,
        bigquery_toolset,
        campaign_manager_360_toolset,
        display_video_360_toolset,
        drive_toolset,
        merchant_center_inventories_toolset,
        merchant_center_products_toolset,
        merchant_center_reports_toolset,
        search_ads_360_toolset,
        storage_toolset,
        youtube_toolset,
        # MCPToolset(
        #     connection_params=StdioConnectionParams(
        #         server_params=StdioServerParameters(
        #             command="uvx",
        #             args=[
        #                 "git+https://github.com/googleads/google-ads-mcp.git",
        #             ],
        #             env={
        #                 "GOOGLE_APPLICATION_CREDENTIALS": os.environ[
        #                     "GOOGLE_APPLICATION_CREDENTIALS"
        #                 ],
        #                 "GOOGLE_PROJECT_ID": os.environ["GOOGLE_PROJECT_ID"],
        #                 "GOOGLE_ADS_DEVELOPER_TOKEN": os.environ[
        #                     "GOOGLE_ADS_DEVELOPER_TOKEN"
        #                 ],
        #                 "GOOGLE_ADS_LOGIN_CUSTOMER_ID": os.environ[
        #                     "GOOGLE_ADS_LOGIN_CUSTOMER_ID"
        #                 ],
        #             },
        #         )
        #     )
        # ),
        DataAnalysisToolset(),
        GoogleAdsToolset(),
        GoogleGenAIToolset(),
        UtilitiesToolset(),
    ],
)
