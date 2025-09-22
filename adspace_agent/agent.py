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
from typing import cast

from google.adk.agents import Agent
from google.adk.tools.bigquery import BigQueryCredentialsConfig
from google.adk.tools.bigquery import BigQueryToolset
from google.adk.tools.bigquery.config import BigQueryToolConfig
from google.adk.tools.bigquery.config import WriteMode
from google.adk.tools.google_api_tool import YoutubeToolset
import google.auth
from google.auth.credentials import Credentials

from .tools.google_ads import GoogleAdsToolset

tool_config = BigQueryToolConfig(write_mode=WriteMode.BLOCKED)

credentials, project_id = google.auth.default()

credentials_config = BigQueryCredentialsConfig(
    credentials=cast(Credentials, credentials)
)

youtube_toolset = YoutubeToolset(
    client_id=os.environ["CLIENT_ID"],
    client_secret=os.environ["CLIENT_SECRET"],
)

bigquery_toolset = BigQueryToolset(
    credentials_config=credentials_config, bigquery_tool_config=tool_config
)

google_ads_toolset = GoogleAdsToolset()

root_agent = Agent(
    name="adspace_agent",
    model="gemini-2.5-flash",
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
        youtube_toolset,
        bigquery_toolset,
        google_ads_toolset,
        # google_search,
    ],
)
