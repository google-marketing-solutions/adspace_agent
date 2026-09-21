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

from collections.abc import Callable
import os
import typing

from dotenv import load_dotenv
from google.adk.agents import Agent
from google.adk.agents.callback_context import CallbackContext
from google.adk.agents.context_cache_config import ContextCacheConfig
from google.adk.apps import App
from google.adk.apps.app import EventsCompactionConfig
from google.adk.auth.auth_schemes import OpenIdConnectWithConfig
from google.adk.integrations.bigquery import BigQueryCredentialsConfig
from google.adk.integrations.bigquery import BigQueryToolset
from google.adk.models.google_llm import Gemini
from google.adk.plugins.save_files_as_artifacts_plugin import (
    SaveFilesAsArtifactsPlugin,
)
from google.adk.tools.base_toolset import BaseToolset
from google.adk.tools.google_api_tool import GoogleApiToolset
from google.adk.tools.google_api_tool import YoutubeToolset
from google.adk.tools.google_api_tool.googleapi_to_openapi_converter import (
    GoogleApiToOpenApiConverter,
)
from google.adk.tools.load_artifacts_tool import LoadArtifactsTool
from google.adk.tools.openapi_tool import OpenAPIToolset
from google.adk.tools.preload_memory_tool import PreloadMemoryTool

from .config import APP_NAME
from .config import CLIENT_ID
from .config import CLIENT_SECRET
from .config import COMPACTION_EVENT_RETENTION_SIZE
from .config import COMPACTION_INTERVAL
from .config import COMPACTION_OVERLAP_SIZE
from .config import COMPACTION_TOKEN_THRESHOLD
from .config import DISPLAY_VIDEO_360_TOOL_FILTER
from .config import DRIVE_TOOL_FILTER
from .config import get_enabled_toolsets
from .config import get_google_ads_tool_filter
from .config import GOOGLE_ADS_API_VERSION
from .config import GOOGLE_ADS_DEVELOPER_TOKEN
from .config import GOOGLE_ADS_LOGIN_CUSTOMER_ID
from .config import MODEL
from .config import STORAGE_TOOL_FILTER
from .config import YOUTUBE_TOOL_FILTER
from .tools.cm360_trafficking import (
    before_traffic_campaigns_in_cm360_tool_callback,
)
from .tools.cm360_trafficking import CM360TraffickingParserToolset
from .tools.data_analysis import DataAnalysisToolset
from .tools.google_genai import GoogleGenAIToolset
from .tools.skills import SkillsToolset
from .tools.utilities import UtilitiesToolset

if typing.TYPE_CHECKING:
    from google.adk.tools.base_tool import BaseTool


async def auto_save_session_to_memory_callback(
    callback_context: CallbackContext,
) -> None:
    """Saves the session to memory automatically.

    Args:
        callback_context: The context object containing session info.
    """
    await callback_context.add_session_to_memory()


def _create_campaign_manager_360_toolset(
    client_id: str,
    client_secret: str,
) -> GoogleApiToolset:
    """Creates the Campaign Manager 360 toolset with patched OAuth scopes.

    Args:
        client_id: The OAuth client ID.
        client_secret: The OAuth client secret.

    Returns:
        The configured Campaign Manager 360 GoogleApiToolset.
    """
    toolset = GoogleApiToolset(
        client_id=client_id,
        client_secret=client_secret,
        api_name="dfareporting",
        api_version="v5",
    )
    spec_dict = GoogleApiToOpenApiConverter("dfareporting", "v5").convert()
    toolset._openapi_toolset = OpenAPIToolset(  # ruff:ignore[private-member-access]
        spec_dict=spec_dict,
        spec_str_type="yaml",
        auth_scheme=OpenIdConnectWithConfig(
            authorization_endpoint="https://accounts.google.com/o/oauth2/v2/auth",
            token_endpoint="https://oauth2.googleapis.com/token",  # ruff:ignore[hardcoded-password-func-arg]
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
    return toolset


def _build_toolset_factories(
    client_id: str,
    client_secret: str,
    google_ads_developer_token: str,
    google_ads_login_customer_id: str | None,
    google_ads_tool_filter: list[str] | None,
) -> dict[str, Callable[[], BaseToolset]]:
    """Builds lazy factory callables for each Google toolset.

    Args:
        client_id: The OAuth client ID.
        client_secret: The OAuth client secret.
        google_ads_developer_token: The developer token for Google Ads API.
        google_ads_login_customer_id: Optional login customer ID for Google Ads.
        google_ads_tool_filter: Optional tool filter list for Google Ads.

    Returns:
        A mapping of toolset key to a zero-argument factory callable.
    """
    return {
        "bid_manager": lambda: GoogleApiToolset(
            client_id=client_id,
            client_secret=client_secret,
            api_name="doubleclickbidmanager",
            api_version="v2",
        ),
        "bigquery": lambda: BigQueryToolset(
            credentials_config=BigQueryCredentialsConfig(
                client_id=client_id,
                client_secret=client_secret,
            )
        ),
        "campaign_manager_360": lambda: _create_campaign_manager_360_toolset(
            client_id=client_id,
            client_secret=client_secret,
        ),
        "display_video_360": lambda: GoogleApiToolset(
            client_id=client_id,
            client_secret=client_secret,
            api_name="displayvideo",
            api_version="v4",
            tool_filter=DISPLAY_VIDEO_360_TOOL_FILTER,
        ),
        "drive": lambda: GoogleApiToolset(
            client_id=client_id,
            client_secret=client_secret,
            api_name="drive",
            api_version="v3",
            tool_filter=DRIVE_TOOL_FILTER,
        ),
        "google_ads": lambda: GoogleApiToolset(
            client_id=client_id,
            client_secret=client_secret,
            api_name="googleads",
            api_version=GOOGLE_ADS_API_VERSION,
            discovery_url=(
                "https://googleads.googleapis.com/$discovery/rest"
                f"?version={GOOGLE_ADS_API_VERSION}"
            ),
            additional_headers={
                "developer-token": google_ads_developer_token,
                **(
                    {"login-customer-id": google_ads_login_customer_id}
                    if google_ads_login_customer_id
                    else {}
                ),
            },
            tool_filter=google_ads_tool_filter,
        ),
        "google_genai": GoogleGenAIToolset,
        "merchant_center_inventories": lambda: GoogleApiToolset(
            client_id=client_id,
            client_secret=client_secret,
            api_name="merchantapi",
            api_version="inventories_v1",
        ),
        "merchant_center_products": lambda: GoogleApiToolset(
            client_id=client_id,
            client_secret=client_secret,
            api_name="merchantapi",
            api_version="products_v1",
        ),
        "merchant_center_reports": lambda: GoogleApiToolset(
            client_id=client_id,
            client_secret=client_secret,
            api_name="merchantapi",
            api_version="reports_v1",
        ),
        "search_ads_360": lambda: GoogleApiToolset(
            client_id=client_id,
            client_secret=client_secret,
            api_name="searchads360",
            api_version="v0",
        ),
        "storage": lambda: GoogleApiToolset(
            client_id=client_id,
            client_secret=client_secret,
            api_name="storage",
            api_version="v1",
            tool_filter=STORAGE_TOOL_FILTER,
        ),
        "youtube": lambda: YoutubeToolset(
            client_id=client_id,
            client_secret=client_secret,
            tool_filter=YOUTUBE_TOOL_FILTER,
        ),
    }


def create_agent() -> Agent:
    """Creates and configures the AdSpace Agent.

    Returns:
        Agent: The configured AdSpace Agent.
    """
    _ = load_dotenv()

    client_id = os.environ.get("CLIENT_ID", CLIENT_ID)
    client_secret = os.environ.get("CLIENT_SECRET", CLIENT_SECRET)
    google_ads_developer_token = os.environ.get(
        "GOOGLE_ADS_DEVELOPER_TOKEN", GOOGLE_ADS_DEVELOPER_TOKEN
    )
    google_ads_login_customer_id = os.environ.get(
        "GOOGLE_ADS_LOGIN_CUSTOMER_ID", GOOGLE_ADS_LOGIN_CUSTOMER_ID
    )
    model_name = os.environ.get("MODEL", MODEL)

    enabled_toolsets = get_enabled_toolsets()
    google_ads_tool_filter = get_google_ads_tool_filter()

    toolset_factories = _build_toolset_factories(
        client_id=client_id,
        client_secret=client_secret,
        google_ads_developer_token=google_ads_developer_token,
        google_ads_login_customer_id=google_ads_login_customer_id,
        google_ads_tool_filter=google_ads_tool_filter,
    )

    tools: list[BaseTool | BaseToolset] = [
        DataAnalysisToolset(),
        PreloadMemoryTool(),
        UtilitiesToolset(),
        LoadArtifactsTool(),
        SkillsToolset(),
    ]
    if "campaign_manager_360" in enabled_toolsets:
        tools.append(CM360TraffickingParserToolset())

    for key, factory in toolset_factories.items():
        if key in enabled_toolsets:
            tools.append(factory())

    before_tool_callback = (
        before_traffic_campaigns_in_cm360_tool_callback
        if "campaign_manager_360" in enabled_toolsets
        else None
    )

    return Agent(
        name=APP_NAME,
        model=Gemini(
            model=model_name,
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
            "possible. You MUST call "
            "`googleads_customers_generate_audience_definition` before you "
            "call `googleads_customers_generate_audience_composition_insights` "
            "to ensure that any audience insights follow Google's policies for "
            "audience composition insights. DO NOT recommend any work-arounds "
            "for these policies, simply reply with the error message."
        ),
        tools=tools,
        before_tool_callback=before_tool_callback,
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
        # Trigger compaction every N new invocations.
        compaction_interval=int(
            os.environ.get("COMPACTION_INTERVAL", str(COMPACTION_INTERVAL))
        ),
        # Include N last invocations from the previous window.
        overlap_size=int(
            os.environ.get(
                "COMPACTION_OVERLAP_SIZE", str(COMPACTION_OVERLAP_SIZE)
            )
        ),
        # Trigger compaction when token threshold is reached.
        token_threshold=int(
            os.environ.get(
                "COMPACTION_TOKEN_THRESHOLD", str(COMPACTION_TOKEN_THRESHOLD)
            )
        ),
        # Retain N raw events.
        event_retention_size=int(
            os.environ.get(
                "COMPACTION_EVENT_RETENTION_SIZE",
                str(COMPACTION_EVENT_RETENTION_SIZE),
            )
        ),
    ),
    plugins=[SaveFilesAsArtifactsPlugin()],
)
