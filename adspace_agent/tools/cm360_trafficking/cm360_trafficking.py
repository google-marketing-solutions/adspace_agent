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
"""Tool for parsing campaign sheets and executing CM360 trafficking."""

import json
import logging
import os
from typing import Any, override

from fastapi.openapi.models import OAuth2
from fastapi.openapi.models import OAuthFlowAuthorizationCode
from fastapi.openapi.models import OAuthFlows
from google.auth.transport.requests import Request
from google.oauth2.credentials import Credentials

# Workaround for mTLS issues in Python 3.11+ with google-auth
# Error: Context has already been used to create a Connection, it cannot
# be mutated again.
# Disable mTLS client certificate adapter to avoid pyOpenSSL SSLContext
# mutation conflicts.
os.environ["GOOGLE_API_USE_CLIENT_CERTIFICATE"] = "false"

try:
    import urllib3.contrib.pyopenssl

    urllib3.contrib.pyopenssl.extract_from_urllib3()
except (AttributeError, ImportError, TypeError):
    pass

from google.adk.agents.callback_context import CallbackContext
from google.adk.agents.readonly_context import ReadonlyContext
from google.adk.auth.auth_credential import AuthCredential
from google.adk.auth.auth_credential import AuthCredentialTypes
from google.adk.auth.auth_credential import OAuth2Auth
from google.adk.auth.auth_tool import AuthConfig
from google.adk.tools import FunctionTool
from google.adk.tools.base_tool import BaseTool
from google.adk.tools.base_toolset import BaseToolset
from google.adk.tools.tool_context import ToolContext

from adspace_agent.tools.cm360_trafficking.cm360_actions import (
    _get_cm360_service,
)
from adspace_agent.tools.cm360_trafficking.cm360_actions import _group_ads
from adspace_agent.tools.cm360_trafficking.cm360_actions import _group_creatives
from adspace_agent.tools.cm360_trafficking.cm360_actions import (
    _group_event_tags,
)
from adspace_agent.tools.cm360_trafficking.cm360_actions import (
    _group_placements,
)
from adspace_agent.tools.cm360_trafficking.trafficking_helpers import (
    _build_trafficking_summary_response,
)
from adspace_agent.tools.cm360_trafficking.trafficking_helpers import (
    _process_ad_operations,
)
from adspace_agent.tools.cm360_trafficking.trafficking_helpers import (
    _process_creative_operations,
)
from adspace_agent.tools.cm360_trafficking.trafficking_helpers import (
    _process_event_tag_operations,
)
from adspace_agent.tools.cm360_trafficking.trafficking_helpers import (
    _process_placement_operations,
)
from adspace_agent.tools.cm360_trafficking.trafficking_helpers import (
    _resolve_and_build_operations,
)
from adspace_agent.tools.cm360_trafficking.trafficking_helpers import (
    _resolve_creative_ids,
)
from adspace_agent.tools.cm360_trafficking.trafficking_helpers import (
    _resolve_placement_ids,
)
from adspace_agent.tools.cm360_trafficking.trafficking_helpers import (
    _update_trafficking_sheet_status,
)
from adspace_agent.tools.cm360_trafficking.utilities import download_from_gcs
from adspace_agent.tools.cm360_trafficking.utilities import load_raw_dataframe
from adspace_agent.tools.cm360_trafficking.utilities import prepare_data
from adspace_agent.tools.cm360_trafficking.utilities import upload_to_gcs
from adspace_agent.tools.cm360_trafficking.validations import validate_sheet

logger = logging.getLogger(__name__)

# Configure default logging
if not logging.getLogger().handlers:
    logging.basicConfig(
        level=logging.INFO, format="%(asctime)s [%(levelname)s] %(message)s"
    )

SCOPES = [
    "https://www.googleapis.com/auth/dfatrafficking",
    "https://www.googleapis.com/auth/dfareporting",
    "https://www.googleapis.com/auth/ddmconversions",
]

CREDENTIALS_CACHE_KEY: str = "CREDENTIALS_CACHE_KEY"


auth_scheme = OAuth2(
    flows=OAuthFlows(
        authorizationCode=OAuthFlowAuthorizationCode(
            authorizationUrl="https://accounts.google.com/o/oauth2/auth",
            tokenUrl="https://oauth2.googleapis.com/token",
            scopes={
                "https://www.googleapis.com/auth/dfatrafficking": (
                    "View/Edit CM360 entities"
                ),
                "https://www.googleapis.com/auth/dfareporting": (
                    "Execute reporting"
                ),
                "https://www.googleapis.com/auth/ddmconversions": (
                    "Insert conversions"
                ),
            },
        )
    )
)

auth_credential = AuthCredential(
    auth_type=AuthCredentialTypes.OAUTH2,
    oauth2=OAuth2Auth(
        client_id=os.getenv("CLIENT_ID"),
        client_secret=os.getenv("CLIENT_SECRET"),
    ),
)


async def parse_sheet_tool(  # ruff: ignore[too-many-locals]
    tool_context: ToolContext,
) -> str:
    """Parses a CM360 trafficking sheet and uploads payload to GCS.

    Loads the campaign trafficking CSV sheet from session artifacts, validates
    all required fields, groups placements, event tags, and ads, resolves
    existing CM360 placement and creative IDs, generates create/update
    operations, and uploads the resolved JSON payload to GCS.

    Args:
        tool_context: Active tool context containing session artifacts.

    Returns:
        A JSON-formatted string summarizing the parsing and upload results,
        including status, campaign identifiers, and generated operations.

    Raises:
        ValueError: If session ID or GCS bucket name is missing.
    """
    logger.info("🚀 Starting parse_sheet_tool...")
    try:  # ruff: ignore[too-many-statements-in-try-clause]
        # Resolve session ID
        if (
            not tool_context
            or not getattr(tool_context, "session", None)
            or not getattr(tool_context.session, "id", None)
        ):
            msg = (
                "Session ID is required and must be present in the tool"
                " context."
            )
            raise ValueError(msg)  # ruff: ignore[raise-within-try]
        session_id = tool_context.session.id

        # Resolve bucket name
        bucket_name = os.getenv("SKILLS_BUCKET_NAME")
        if not bucket_name:
            msg = (
                "GCS bucket name not configured in environment variables"
                " (SKILLS_BUCKET_NAME)."
            )
            raise ValueError(msg)  # ruff: ignore[raise-within-try]

        # 1. Load the raw DataFrame
        df_raw, _ = await load_raw_dataframe(tool_context)

        # 2. Extract metadata and clean data rows
        df, profile_id, advertiser_id, campaign_id, campaign_name = (
            prepare_data(df_raw)
        )

        logger.info(
            "🆔 [parse_sheet_tool] - Extracted IDs: Profile=%s, Advertiser=%s,"
            " Campaign=%s, Campaign Name=%s",
            profile_id,
            advertiser_id,
            campaign_id,
            campaign_name,
        )

        # 3. Run validations
        validation_errors = validate_sheet(
            df=df,
            profile_id=profile_id,
            advertiser_id=advertiser_id,
            campaign_id=campaign_id,
        )
        if validation_errors:
            logger.info(
                "❌ [parse_sheet_tool] - Validation failed: %s",
                validation_errors,
            )
            return json.dumps(
                {
                    "status": "error",
                    "message": (
                        "[parse_sheet_tool] - Validation failed for the"
                        " trafficking sheet."
                    ),
                    "validation_errors": validation_errors,
                },
                indent=2,
            )

        # 4. Group placements, creatives, event tags, and ads
        placements = _group_placements(
            df, advertiser_id, campaign_id, campaign_name
        )
        creatives = _group_creatives(df, advertiser_id)
        event_tags = _group_event_tags(df, advertiser_id, campaign_id)
        ads = _group_ads(df, advertiser_id, campaign_id, campaign_name)

        # 5. Fetch existing placements and creatives from CM360 and resolve IDs
        existing_placements = _resolve_placement_ids(
            ads=ads,
            profile_id=profile_id,
            advertiser_id=advertiser_id,
            campaign_id=campaign_id,
            tool_context=tool_context,
        )
        existing_creatives = _resolve_creative_ids(
            ads=ads,
            profile_id=profile_id,
            advertiser_id=advertiser_id,
            campaign_id=campaign_id,
            tool_context=tool_context,
        )

        logger.info(
            "📊 [parse_sheet_tool] - Grouped: %d placements, %d creatives,"
            " %d event tags, %d ads.",
            len(placements),
            len(creatives),
            len(event_tags),
            len(ads),
        )

        # 6. Fetch existing event tags and ads, reconcile
        # placements/overrides, and build operations
        operations = _resolve_and_build_operations(
            ads=ads,
            event_tags=event_tags,
            profile_id=profile_id,
            advertiser_id=advertiser_id,
            campaign_id=campaign_id,
            tool_context=tool_context,
            placements=placements,
            creatives=creatives,
            existing_placements=existing_placements,
            existing_creatives=existing_creatives,
        )
        logger.info(
            "✨ [parse_sheet_tool] - Successfully generated %d operations.",
            len(operations),
        )

        # 7. Store parsed payload in GCS
        object_path = f"cm360_trafficking/{session_id}/payloads.json"
        parsed_data = {
            "status": "success",
            "profile_id": profile_id,
            "advertiser_id": advertiser_id,
            "campaign_id": campaign_id,
            "campaign_name": campaign_name,
            "operations": operations,
        }
        parsed_data_str = json.dumps(parsed_data, indent=2)

        parsed_payload_gcs_url = upload_to_gcs(
            bucket_name=bucket_name,
            object_path=object_path,
            content=parsed_data_str,
        )

        tool_context.state["parsed_payload_gcs_url"] = parsed_payload_gcs_url

        sheet_entities = {
            "placements": placements,
            "ads": ads,
            "creatives": creatives,
            "event_tags": event_tags,
        }

        return json.dumps(
            {
                "status": "SUCCESS",
                "message": (
                    "The trafficking sheet has been successfully parsed."
                ),
                "parsed_payload_gcs_url": parsed_payload_gcs_url,
                "profile_id": profile_id,
                "advertiser_id": advertiser_id,
                "campaign_id": campaign_id,
                "campaign_name": campaign_name,
                "sheet_entities": sheet_entities,
                "operations": operations,
            },
            indent=2,
        )
    except Exception as e:
        logger.exception(
            "❌ [parse_sheet_tool] - Error parsing the tsheet to generate"
            " payloads."
        )
        return json.dumps(
            {
                "status": "ERROR",
                "message": (
                    "[parse_sheet_tool] - Error parsing the tsheet to generate"
                    f" payloads: {e}"
                ),
            },
            indent=2,
        )


async def traffic_campaigns_in_cm360_tool(  # ruff: ignore[too-many-locals]
    tool_context: ToolContext,
) -> str:
    """Pushes resolved campaign trafficking payloads from GCS to CM360.

    Downloads the resolved payloads JSON from Google Cloud Storage, executes
    the create and update operations sequentially for event tags and ads
    against the Campaign Manager 360 API, updates the status of successfully
    trafficked rows to 'Trafficked' in the session CSV artifact, and returns
    an execution summary.

    Args:
        tool_context: Active tool context containing session state and
            credentials.

    Returns:
        A JSON-formatted string summarizing the push execution results,
        including per-entity status, assigned CM360 IDs, and error messages.

    Raises:
        ValueError: If tool context, session, or payload GCS URL is missing.
    """
    logger.info("🚀 Starting traffic_campaigns_in_cm360_tool...")
    try:  # ruff: ignore[too-many-statements-in-try-clause]
        # Retrieve GCS URL and resolve bucket name
        if not tool_context or not tool_context.state:
            msg = "Tool context and state are missing from tool_context."
            raise ValueError(msg)  # ruff: ignore[raise-within-try]

        if not getattr(tool_context, "session", None) or not getattr(
            tool_context.session, "id", None
        ):
            msg = (
                "Session ID is required and must be present in the tool"
                " context."
            )
            raise ValueError(msg)  # ruff: ignore[raise-within-try]

        parsed_payload_gcs_url = tool_context.state.get(
            "parsed_payload_gcs_url"
        )
        gcs_object_path = None
        if parsed_payload_gcs_url and parsed_payload_gcs_url.startswith(
            "gs://"
        ):
            parts = parsed_payload_gcs_url[5:].split("/", 1)
            if len(parts) == 2:  # ruff: ignore[magic-value-comparison]
                _, gcs_object_path = parts

        bucket_name = os.getenv("SKILLS_BUCKET_NAME")
        if not bucket_name:
            msg = (
                "GCS bucket name could not be determined from environment"
                f" or parsed_payload_gcs_url: {parsed_payload_gcs_url}"
            )
            raise ValueError(msg)  # ruff: ignore[raise-within-try]

        if not gcs_object_path:
            msg = (
                f"CM360 payloads not found in GCS: {parsed_payload_gcs_url}."
                " Please try parsing the sheet again."
            )
            raise ValueError(msg)  # ruff: ignore[raise-within-try]

        logger.info(
            "📥 [traffic_campaigns_in_cm360_tool] - Downloading payloads"
            " from GCS: %s...",
            parsed_payload_gcs_url,
        )

        # Download content from GCS using SDK
        content_str = download_from_gcs(
            bucket_name=bucket_name, object_path=gcs_object_path
        )
        payload_data = json.loads(content_str)

        profile_id = payload_data.get("profile_id")
        advertiser_id = payload_data.get("advertiser_id")
        campaign_id = payload_data.get("campaign_id")
        campaign_name = payload_data.get("campaign_name")
        operations = payload_data.get("operations", [])

        logger.info(
            "🚀 [traffic_campaigns_in_cm360_tool] - Pushing %d operations to"
            " CM360 for profile ID: %s (Advertiser ID: %s, Campaign ID: %s,"
            " Campaign Name: %s)",
            len(operations),
            profile_id,
            advertiser_id,
            campaign_id,
            campaign_name,
        )

        cm360_service = _get_cm360_service(tool_context)

        # 1. Filter operations into Placements, Creatives, Event Tags, and Ads
        placement_ops = [
            op
            for op in operations
            if op.get("operation") == "dfareporting.placements.patch"
        ]
        creative_ops = [
            op
            for op in operations
            if op.get("operation") == "dfareporting.creatives.patch"
        ]
        event_tag_ops = [
            op
            for op in operations
            if op.get("operation")
            in {
                "dfareporting.eventTags.insert",
                "dfareporting.eventTags.patch",
                "dfareporting.eventTags.update",
            }
        ]
        ad_ops = [
            op
            for op in operations
            if op.get("operation")
            in {
                "dfareporting.ads.insert",
                "dfareporting.ads.patch",
                "dfareporting.ads.update",
            }
        ]

        # 2. Process Placement PATCH operations
        placement_results = _process_placement_operations(
            cm360_service=cm360_service,
            profile_id=profile_id,
            placement_ops=placement_ops,
        )

        # 3. Process Creative PATCH operations
        creative_results = _process_creative_operations(
            cm360_service=cm360_service,
            profile_id=profile_id,
            creative_ops=creative_ops,
        )

        # 4. Process Event Tag creations and updates
        tag_results, event_tag_mappings, failed_event_tags = (
            _process_event_tag_operations(
                cm360_service=cm360_service,
                profile_id=profile_id,
                event_tag_ops=event_tag_ops,
            )
        )

        # 5. Process Ads with event tag dependency validation
        ad_results = _process_ad_operations(
            cm360_service=cm360_service,
            profile_id=profile_id,
            ad_ops=ad_ops,
            event_tag_mappings=event_tag_mappings,
            failed_event_tag_names=failed_event_tags,
        )

        # 6. Build summary response
        all_results = (
            placement_results + creative_results + tag_results + ad_results
        )

        # 6. Update trafficking sheet artifact status to 'Trafficked'
        await _update_trafficking_sheet_status(
            tool_context=tool_context,
            results=all_results,
        )

        return _build_trafficking_summary_response(
            results=all_results,
            total_operations_count=len(operations),
            profile_id=profile_id,
            advertiser_id=advertiser_id,
            campaign_id=campaign_id,
            campaign_name=campaign_name,
        )

    except Exception as e:
        logger.exception(
            "❌ [traffic_campaigns_in_cm360_tool] - Error executing CM360"
            " trafficking push."
        )
        return json.dumps(
            {
                "status": "ERROR",
                "message": (
                    "[traffic_campaigns_in_cm360_tool] - Error executing CM360"
                    f" trafficking push: {e}"
                ),
            },
            indent=2,
        )


def before_traffic_campaigns_in_cm360_tool_callback(
    tool: BaseTool,
    args: dict[str, Any],  # ruff: ignore[unused-function-argument]
    tool_context: CallbackContext,
) -> dict[str, Any] | None:
    """Callback before CM360 trafficking tools to handle OAuth auth.

    Args:
        tool: The tool being invoked.
        args: Arguments passed to the tool.
        tool_context: Callback context containing session state and auth.

    Returns:
        A dict with pending status if auth is requested, or None if ready.
    """
    logger.info(
        "before_traffic_campaigns_in_cm360_tool_callback invoked for tool: %s",
        tool.name,
    )

    if tool.name in {"traffic_campaigns_in_cm360_tool", "parse_sheet_tool"}:
        creds = None
        cached_token_info = tool_context.state.get(CREDENTIALS_CACHE_KEY)

        if cached_token_info:
            logger.info("Found cached token info in tool state.")
            try:  # ruff: ignore[too-many-statements-in-try-clause]
                creds = Credentials.from_authorized_user_info(
                    cached_token_info, SCOPES
                )

                if not creds.valid and creds.expired and creds.refresh_token:
                    logger.info(
                        "Cached credentials expired; attempting to refresh"
                        " token."
                    )
                    creds.refresh(Request())
                    tool_context.state[CREDENTIALS_CACHE_KEY] = json.loads(
                        creds.to_json()
                    )
                    logger.info(
                        "Successfully refreshed credentials and updated cache."
                    )

                elif not creds.valid:
                    logger.warning(
                        "Cached credentials are invalid and could not be"
                        " refreshed. Clearing cache."
                    )
                    creds = None
                    tool_context.state[CREDENTIALS_CACHE_KEY] = None

            except Exception:
                logger.exception(
                    "Failed to load or refresh cached credentials."
                    " Clearing cache."
                )
                creds = None
                tool_context.state[CREDENTIALS_CACHE_KEY] = None
        else:
            logger.info("No cached credentials found.")

        if creds and creds.valid:
            logger.info("Using valid cached credentials.")
            tool_context.state[CREDENTIALS_CACHE_KEY] = json.loads(
                creds.to_json()
            )
            return None

        logger.info(
            "Valid credentials not available in cache. Fetching"
            " authentication response."
        )
        exchanged_credential = tool_context.get_auth_response(
            AuthConfig(
                auth_scheme=auth_scheme,
                raw_auth_credential=auth_credential,
            )
        )

        if (
            exchanged_credential is not None
            and exchanged_credential.oauth2 is not None
            and auth_scheme.flows is not None
            and auth_scheme.flows.authorizationCode is not None
        ):
            logger.info("Successfully exchanged credentials via tool context.")
            oauth: OAuth2Auth = exchanged_credential.oauth2

            client_id = os.getenv("CLIENT_ID")
            client_secret = os.getenv("CLIENT_SECRET")

            token_uri = auth_scheme.flows.authorizationCode.tokenUrl

            tool_context.state[CREDENTIALS_CACHE_KEY] = {
                "client_id": client_id,
                "client_secret": client_secret,
                "token": oauth.access_token,
                "token_uri": token_uri,
                "refresh_token": oauth.refresh_token,
            }
            logger.info("Saved exchanged credentials to cache.")

            return None

        logger.info(
            "No exchanged credential available. Requesting user authentication."
        )
        tool_context.request_credential(
            AuthConfig(
                auth_scheme=auth_scheme,
                raw_auth_credential=auth_credential,
            )
        )

        return {"pending": True, "message": "Awaiting user authentication."}

    return None


class CM360TraffickingParserToolset(BaseToolset):
    """Toolset for parsing trafficking sheets and executing trafficking."""

    def __init__(self) -> None:
        """Initializes the CM360TraffickingParserToolset."""
        super().__init__()
        self.parse_sheet_tool = FunctionTool(func=parse_sheet_tool)
        self.traffic_campaigns_in_cm360_tool = FunctionTool(
            func=traffic_campaigns_in_cm360_tool
        )

    @override
    async def get_tools(
        self,
        readonly_context: ReadonlyContext | None = None,
    ) -> list[BaseTool]:
        """Returns the list of tools provided by this toolset.

        Args:
            readonly_context: Optional readonly agent context.

        Returns:
            A list containing the parse and traffic FunctionTools.
        """
        return [self.parse_sheet_tool, self.traffic_campaigns_in_cm360_tool]
