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
"""Campaign Manager 360 actions for grouping, API listings, and operations building."""

import logging
from typing import Any

from google.adk.tools.tool_context import ToolContext
from google.oauth2.credentials import Credentials
from googleapiclient.discovery import build
import pandas as pd

from .utilities import extract_site_identifier
from .utilities import format_date
from .utilities import format_date_time
from .utilities import parse_size
from .utilities import parse_weight

logger = logging.getLogger(__name__)

SCOPES = [
    "https://www.googleapis.com/auth/dfatrafficking",
    "https://www.googleapis.com/auth/dfareporting",
    "https://www.googleapis.com/auth/ddmconversions",
]

CREDENTIALS_CACHE_KEY: str = "CREDENTIALS_CACHE_KEY"


def _group_placements(
    df: pd.DataFrame,
    advertiser_id: str | None = None,
    campaign_id: str | None = None,
    campaign_name: str | None = None,
) -> dict[str, dict[str, Any]]:
    """Groups rows into unique placements keyed by placement name.

    Args:
        df: The parsed pandas DataFrame.
        advertiser_id: Dynamically parsed Advertiser ID.
        campaign_id: Dynamically parsed Campaign ID.
        campaign_name: Dynamically parsed Campaign Name.

    Returns:
        A dictionary mapping placement names to their CM360 payloads.
    """
    placements = {}
    for _, row in df.iterrows():
        placement_name = str(row["Placement Name"]).strip()
        site_identifier = extract_site_identifier(row)

        placement_start = None
        if "Placement Start Date" in row and not pd.isna(row["Placement Start Date"]):
            placement_start = row["Placement Start Date"]

        placement_end = None
        if "Placement End Date" in row and not pd.isna(row["Placement End Date"]):
            placement_end = row["Placement End Date"]

        placement_size = row["Placement Size"]

        compatibility = ""
        if "Placement Type" in row and not pd.isna(row["Placement Type"]):
            compatibility = str(row["Placement Type"]).strip().upper()

        site_id = f"{site_identifier}"

        payment_source = ""

        pricing_type = ""

        active_status = ""
        if (
            "Placement Status" in row
            and not pd.isna(row["Placement Status"])
            and str(row["Placement Status"]).strip() != ""
        ):
            active_status = str(row["Placement Status"]).strip().upper()

        tag_formats = []

        if placement_name not in placements:
            payload = {
                "name": placement_name,
                "campaignId": campaign_id,
                "siteId": site_id,
                "activeStatus": active_status,
                "size": parse_size(placement_size),
                "compatibility": compatibility,
                "paymentSource": payment_source,
                "pricingSchedule": {
                    "startDate": format_date(placement_start),
                    "endDate": format_date(placement_end),
                    "pricingType": pricing_type,
                },
                "tagFormats": tag_formats,
            }
            placements[placement_name] = payload
    return placements


def _parse_row_event_tags(row: pd.Series) -> list[dict[str, Any]]:
    """Parses comma-separated event tags from a single row.

    Args:
        row: A pandas Series representing a row in the trafficking sheet.

    Returns:
        A list of parsed event tag dictionaries.
    """
    if "Event Tag Names" not in row or pd.isna(row["Event Tag Names"]):
        return []

    raw_names = str(row["Event Tag Names"]).strip()
    if not raw_names or raw_names.lower() in {"none", "nan"}:
        return []

    names = [n.strip() for n in raw_names.split(",") if n.strip()]
    if not names:
        return []

    raw_types = str(row.get("Event Tag Types", "")).strip()
    types = [t.strip() for t in raw_types.split(",") if t.strip()]

    raw_urls = str(row.get("Event Tag Urls", "")).strip()
    urls = [u.strip() for u in raw_urls.split(",") if u.strip()]

    raw_statuses = str(row.get("Event Tag Status", "")).strip()
    statuses = (
        [s.strip() for s in raw_statuses.split(",") if s.strip()]
        if raw_statuses and raw_statuses.lower() not in {"none", "nan"}
        else []
    )

    tags = []
    for i, name in enumerate(names):
        tag_type = types[i].upper() if i < len(types) else ""
        url = urls[i] if i < len(urls) else ""
        status = statuses[i].upper() if i < len(statuses) else "ENABLED"
        tags.append(
            {
                "name": name,
                "type": tag_type,
                "url": url,
                "status": status,
            }
        )
    return tags


def _group_event_tags(
    df: pd.DataFrame,
    advertiser_id: str | None = None,
    campaign_id: str | None = None,
) -> dict[str, dict[str, Any]]:
    """Groups rows into unique event tags keyed by event tag name.

    Args:
        df: The parsed pandas DataFrame.
        advertiser_id: Dynamically parsed Advertiser ID.
        campaign_id: Dynamically parsed Campaign ID.

    Returns:
        A dictionary mapping unique event tag names to their CM360 payloads.
    """
    event_tags = {}
    for _, row in df.iterrows():
        row_tags = _parse_row_event_tags(row)
        for tag in row_tags:
            tag_name = tag["name"]
            if tag_name not in event_tags:
                payload = {
                    "name": tag_name,
                    "advertiserId": advertiser_id,
                    "campaignId": campaign_id,
                    "type": tag["type"],
                    "url": tag["url"],
                    "status": tag["status"],
                }
                event_tags[tag_name] = payload
    return event_tags


def _group_ads(
    df: pd.DataFrame,
    advertiser_id: str | None = None,
    campaign_id: str | None = None,
    campaign_name: str | None = None,
) -> dict[str, dict[str, Any]]:
    """Groups rows into unique ads with their associations keyed by unique ad name.

    Args:
        df: The parsed pandas DataFrame.
        advertiser_id: Dynamically parsed Advertiser ID.
        campaign_id: Dynamically parsed Campaign ID.
        campaign_name: Dynamically parsed Campaign Name.

    Returns:
        A dictionary mapping unique ad names to their payloads.
    """
    ads = {}
    for _, row in df.iterrows():
        placement_name = str(row["Placement Name"]).strip()
        ad_name = str(row["Ad Name"]).strip()
        ad_start = row["Ad Start Date"]
        ad_end = row["Ad End Date"]
        creative_id_val = str(row["Creative Name"]).strip()

        creative_url = None
        if (
            "Final Trafficking URL" in row
            and not pd.isna(row["Final Trafficking URL"])
            and str(row["Final Trafficking URL"]).strip() != ""
        ):
            creative_url = str(row["Final Trafficking URL"]).strip()

        priority = "AD_PRIORITY_01"
        if "Delivery Schedule Priority" in row and not pd.isna(
            row["Delivery Schedule Priority"]
        ):
            priority = str(row["Delivery Schedule Priority"]).strip()

        impression_ratio = "1"
        if "Delivery Schedule Impression Ratio" in row and not pd.isna(
            row["Delivery Schedule Impression Ratio"]
        ):
            val = row["Delivery Schedule Impression Ratio"]
            if isinstance(val, (int, float)):
                impression_ratio = str(int(val))
            else:
                impression_ratio = str(val).strip()

        ad_type_val = None
        if (
            "Ad Type" in row
            and not pd.isna(row["Ad Type"])
            and str(row["Ad Type"]).strip() != ""
        ):
            ad_type_val = str(row["Ad Type"]).strip().upper()

        if ad_name not in ads:
            ad_payload = {
                "name": ad_name,
                "active": True,
                "advertiserId": advertiser_id,
                "campaignId": campaign_id,
                "type": ad_type_val,
                "startTime": format_date_time(ad_start),
                "endTime": format_date_time(ad_end),
                "placementAssignments": [],
                "eventTagOverrides": [],
            }
            if ad_type_val == "AD_SERVING_CLICK_TRACKER":
                val = row.get("Ad Dynamic Click Tracker")
                val_str = str(val).strip().upper() if not pd.isna(val) else ""
                ad_payload["dynamicClickTracker"] = val_str == "TRUE"
                if creative_url:
                    ad_payload["clickThroughUrl"] = {
                        "customClickThroughUrl": creative_url,
                    }
            else:
                ad_payload["creativeRotation"] = {
                    "creativeAssignments": [],
                }
                if ad_type_val in (
                    "AD_SERVING_STANDARD_AD",
                    "AD_SERVING_TRACKING",
                ):
                    ad_payload["deliverySchedule"] = {
                        "priority": priority,
                        "impressionRatio": impression_ratio,
                    }
            ads[ad_name] = ad_payload

        # Associate Placement by placement name
        ad_placements = [p["placementId"] for p in ads[ad_name]["placementAssignments"]]
        if placement_name not in ad_placements:
            ads[ad_name]["placementAssignments"].append(
                {
                    "placementId": placement_name,
                    "active": True,
                }
            )

        # Associate Creative by creative name (only for non-click-tracker ads)
        if (
            ad_type_val != "AD_SERVING_CLICK_TRACKER"
            and "creativeRotation" in ads[ad_name]
        ):
            ad_creatives = [
                c["creativeId"]
                for c in ads[ad_name]["creativeRotation"]["creativeAssignments"]
            ]
            if creative_id_val not in ad_creatives:
                rotation_val = row.get("Creative Rotation")
                weight_val = (
                    parse_weight(rotation_val) if not pd.isna(rotation_val) else 100
                )
                weight_val = max(1, weight_val)

                creative_assignment = {
                    "creativeId": creative_id_val,
                    "active": True,
                    "weight": weight_val,
                }
                if creative_url:
                    creative_assignment["clickThroughUrl"] = {
                        "customClickThroughUrl": creative_url,
                    }
                ads[ad_name]["creativeRotation"]["creativeAssignments"].append(
                    creative_assignment
                )

        # Associate Event Tags with Ad overrides
        row_tags = _parse_row_event_tags(row)
        existing_override_ids = {
            o["id"] for o in ads[ad_name]["eventTagOverrides"] if "id" in o
        }
        for tag in row_tags:
            tag_name = tag["name"]
            if tag_name not in existing_override_ids:
                ads[ad_name]["eventTagOverrides"].append(
                    {
                        "id": tag_name,
                        "enabled": True,
                    }
                )
                existing_override_ids.add(tag_name)

    return ads


def build_operations_list(
    ads: dict[str, dict[str, Any]],
    event_tags: dict[str, dict[str, Any]] | None = None,
) -> list[dict[str, Any]]:
    """Builds the final operations payload list for Event Tags and Ads.

    Args:
        ads: Grouped ads mapping keyed by unique ad name.
        event_tags: Optional grouped event tags mapping keyed by unique tag name.

    Returns:
        A list of operation dictionaries with event tags first, followed by ads.
    """
    operations = []
    if event_tags:
        for tag_name, tag_payload in event_tags.items():
            operations.append(
                {
                    "operation": "dfareporting.eventTags.insert",
                    "name": tag_name,
                    "payload": tag_payload,
                }
            )
    if ads:
        for ad_name, ad_payload in ads.items():
            operations.append(
                {
                    "operation": "dfareporting.ads.insert",
                    "name": ad_name,
                    "payload": ad_payload,
                }
            )
    return operations


def _get_cm360_service(tool_context: ToolContext | None = None) -> Any:
    """Builds dfareporting service using credentials stored in tool context state.

    Args:
        tool_context: Optional ADK ToolContext containing credentials in state.

    Returns:
        Google API Client service for CM360 (dfareporting v5).

    Raises:
        ValueError: If tool_context, state, or credentials are missing.
    """
    logger.info("Initializing CM360 dfareporting service client...")
    if not tool_context or tool_context.state is None:
        raise ValueError("Tool context and state are required to get CM360 service.")

    cached_token_info = tool_context.state.get(CREDENTIALS_CACHE_KEY)
    if not cached_token_info:
        raise ValueError(
            f"Credentials not found in tool context state (key: {CREDENTIALS_CACHE_KEY})."
        )

    if isinstance(cached_token_info, Credentials):
        credentials = cached_token_info
    else:
        credentials = Credentials.from_authorized_user_info(cached_token_info, SCOPES)

    logger.info(
        "Valid credentials found in tool_context. Building CM360 service client..."
    )

    service = build("dfareporting", "v5", credentials=credentials)
    return service


def list_cm_placements(
    profile_id: str,
    advertiser_ids: list[str] | None = None,
    campaign_ids: list[str] | None = None,
    tool_context: ToolContext | None = None,
) -> list[dict[str, Any]]:
    """Lists active placements from CM360 with pagination and filters.

    Args:
        profile_id: CM360 user profile ID.
        advertiser_ids: Optional list of advertiser IDs to filter by.
        campaign_ids: Optional list of campaign IDs to filter by.
        tool_context: Optional ADK ToolContext.

    Returns:
        A list of placement resource dictionaries.
    """
    logger.info(
        "Listing active CM360 placements for Profile=%s (Filters: AdvertiserIds=%s,"
        " CampaignIds=%s)...",
        profile_id,
        advertiser_ids,
        campaign_ids,
    )
    service = _get_cm360_service(tool_context)
    placements = []
    page_token = None

    while True:
        kwargs: dict[str, Any] = {
            "profileId": profile_id,
            "pageToken": page_token,
            "maxResults": 1000,
            "activeStatus": ["PLACEMENT_STATUS_ACTIVE"],
            "fields": "nextPageToken,placements(id,name)",
        }
        if advertiser_ids:
            kwargs["advertiserIds"] = advertiser_ids
        if campaign_ids:
            kwargs["campaignIds"] = campaign_ids

        request = service.placements().list(**kwargs)
        response = request.execute()

        page_placements = response.get("placements", [])
        placements.extend(page_placements)
        page_token = response.get("nextPageToken")

        if not page_token:
            logger.info("No more pages (nextPageToken is empty).")
            break

    logger.info(
        "Successfully fetched %d placements in total.",
        len(placements),
    )
    return placements


def list_cm_creatives(
    profile_id: str,
    advertiser_id: str | None = None,
    campaign_id: str | None = None,
    tool_context: ToolContext | None = None,
) -> list[dict[str, Any]]:
    """Lists active creatives from CM360 with pagination and filters.

    Args:
        profile_id: CM360 user profile ID.
        advertiser_id: Optional advertiser ID to filter by.
        campaign_id: Optional campaign ID to filter by.
        tool_context: Optional ADK ToolContext.

    Returns:
        A list of creative resource dictionaries.
    """
    logger.info(
        "Listing active CM360 creatives for Profile=%s (Filters: AdvertiserId=%s,"
        " CampaignId=%s)...",
        profile_id,
        advertiser_id,
        campaign_id,
    )
    service = _get_cm360_service(tool_context)
    creatives = []
    page_token = None

    while True:
        kwargs: dict[str, Any] = {
            "profileId": profile_id,
            "pageToken": page_token,
            "maxResults": 1000,
            "active": True,
            "archived": False,
            "fields": "nextPageToken,creatives(id,name)",
        }
        if advertiser_id:
            kwargs["advertiserId"] = advertiser_id
        if campaign_id:
            kwargs["campaignId"] = campaign_id

        request = service.creatives().list(**kwargs)
        response = request.execute()

        page_creatives = response.get("creatives", [])
        creatives.extend(page_creatives)
        page_token = response.get("nextPageToken")

        if not page_token:
            logger.info("No more pages (nextPageToken is empty).")
            break

    logger.info(
        "Successfully fetched %d creatives in total.",
        len(creatives),
    )
    return creatives


def list_cm_event_tags(
    profile_id: str,
    advertiser_id: str | None = None,
    campaign_id: str | None = None,
    tool_context: ToolContext | None = None,
) -> list[dict[str, Any]]:
    """Lists active event tags from CM360 with filters.

    Args:
        profile_id: CM360 user profile ID.
        advertiser_id: Optional advertiser ID to filter by.
        campaign_id: Optional campaign ID to filter by.
        tool_context: Optional ADK ToolContext.

    Returns:
        A list of event tag resource dictionaries.
    """
    logger.info(
        "Listing active CM360 event tags for Profile=%s (Filters: AdvertiserId=%s,"
        " CampaignId=%s)...",
        profile_id,
        advertiser_id,
        campaign_id,
    )
    service = _get_cm360_service(tool_context)
    kwargs: dict[str, Any] = {
        "profileId": profile_id,
        "fields": "eventTags(id,name,url)",
    }
    if advertiser_id:
        kwargs["advertiserId"] = advertiser_id
    if campaign_id:
        kwargs["campaignId"] = campaign_id

    request = service.eventTags().list(**kwargs)
    response = request.execute()

    event_tags = response.get("eventTags", [])
    logger.info(
        "Successfully fetched %d event tags in total.",
        len(event_tags),
    )
    return event_tags


def list_cm_ads(
    profile_id: str,
    advertiser_id: str | None = None,
    campaign_ids: list[str] | None = None,
    placement_ids: list[str] | None = None,
    tool_context: ToolContext | None = None,
) -> list[dict[str, Any]]:
    """Lists active ads from CM360 with pagination and filters.

    Args:
        profile_id: CM360 user profile ID.
        advertiser_id: Optional advertiser ID to filter by.
        campaign_ids: Optional list of campaign IDs to filter by.
        placement_ids: Optional list of placement IDs to filter by.
        tool_context: Optional ADK ToolContext.

    Returns:
        A list of ad resource dictionaries.
    """
    logger.info(
        "Listing active CM360 ads for Profile=%s (Filters: AdvertiserId=%s,"
        " CampaignIds=%s, PlacementIds=%s)...",
        profile_id,
        advertiser_id,
        campaign_ids,
        placement_ids,
    )
    service = _get_cm360_service(tool_context)
    ads = []
    page_token = None

    while True:
        kwargs: dict[str, Any] = {
            "profileId": profile_id,
            "pageToken": page_token,
            "maxResults": 1000,
            "archived": False,
            "fields": "nextPageToken,ads(id,name,placementAssignments)",
        }
        if advertiser_id:
            kwargs["advertiserId"] = advertiser_id
        if campaign_ids:
            kwargs["campaignIds"] = campaign_ids
        if placement_ids:
            kwargs["placementIds"] = placement_ids

        request = service.ads().list(**kwargs)
        logger.info("Sending API request to list ads...")
        response = request.execute()

        page_ads = response.get("ads", [])
        ads.extend(page_ads)
        page_token = response.get("nextPageToken")

        if not page_token:
            logger.info("No more pages (nextPageToken is empty).")
            break

    logger.info(
        "Successfully fetched %d ads in total.",
        len(ads),
    )
    return ads
