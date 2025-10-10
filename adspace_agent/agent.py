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
from google.adk.tools.google_api_tool import BigQueryToolset
from google.adk.tools.google_api_tool import GoogleApiToolset
from google.adk.tools.google_api_tool import YoutubeToolset

from adspace_agent.tools.google_ads import GoogleAdsToolset
from adspace_agent.tools.google_genai import GoogleGenAIToolset
from adspace_agent.tools.utilities import UtilitiesToolset

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
    name="adspace_agent",
    model="gemini-2.5-pro",
    description=(
        "AdSpace Agent is designed to provide a standardized way to integrate "
        + "an LLM with Google Ads, YouTube, and Google Cloud to form a more "
        + "comprehensive campaign and marketing plan for agencies."
    ),
    instruction=(
        "You are a helpful agent who can answer user questions about ads, "
        + "creatives, and campaigns."
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
        GoogleAdsToolset(),
        GoogleGenAIToolset(),
        UtilitiesToolset(),
    ],
)
