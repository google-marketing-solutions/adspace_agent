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
"""Helper functions for CM360 campaign trafficking and entity resolution."""

import io
import json
import logging
from typing import Any

from google.adk.tools.tool_context import ToolContext
from google.genai import types
import pandas as pd

from .cm360_actions import list_cm_ads
from .cm360_actions import list_cm_creatives
from .cm360_actions import list_cm_event_tags
from .cm360_actions import list_cm_placements
from .utilities import format_date
from .utilities import GLOBAL_METADATA_ROWS_COUNT
from .utilities import load_raw_dataframe
from .utilities import normalize_iso_datetime

logger = logging.getLogger(__name__)


def _resolve_placement_ids(
    ads: dict[str, dict[str, Any]],
    profile_id: str,
    advertiser_id: str | None = None,
    campaign_id: str | None = None,
    tool_context: ToolContext | None = None,
) -> dict[str, dict[str, Any]]:
    """Fetches existing CM360 placements and updates ad assignments.

    Args:
        ads: Grouped ads mapping keyed by ad name.
        profile_id: CM360 user profile ID.
        advertiser_id: Optional advertiser ID filter.
        campaign_id: Optional campaign ID filter.
        tool_context: Optional ADK ToolContext.

    Returns:
        Dictionary mapping placement names to CM360 placement resources.

    Raises:
        ValueError: If fetching placements fails or placement not found.
    """
    logger.info(
        "Fetching existing placements from CM360 for Advertiser ID: %s,"
        " Campaign ID: %s",
        advertiser_id,
        campaign_id,
    )
    try:
        existing_placements = list_cm_placements(
            profile_id=profile_id,
            advertiser_ids=[advertiser_id] if advertiser_id else None,
            campaign_ids=[campaign_id] if campaign_id else None,
            tool_context=tool_context,
        )
    except Exception as e:
        msg = f"Failed to fetch placements from Campaign Manager 360: {e}"
        raise ValueError(msg) from e

    placement_name_to_id: dict[str, str] = {}
    existing_placements_by_name: dict[str, dict[str, Any]] = {}
    for p in existing_placements:
        name = p.get("name")
        real_id = p.get("id")
        if name and real_id:
            clean_name = name.strip()
            placement_name_to_id[clean_name] = str(real_id)
            existing_placements_by_name[clean_name] = p

    for ad_payload in ads.values():
        for assignment in ad_payload.get("placementAssignments", []):
            name = assignment.get("placementId")
            if name in placement_name_to_id:
                assignment["placementId"] = placement_name_to_id[name]
            else:
                msg = f"Placement '{name}' not found in Campaign Manager 360."
                raise ValueError(msg)

    return existing_placements_by_name


def _resolve_creative_ids(
    ads: dict[str, dict[str, Any]],
    profile_id: str,
    advertiser_id: str | None = None,
    campaign_id: str | None = None,
    tool_context: ToolContext | None = None,
) -> dict[str, dict[str, Any]]:
    """Fetches existing CM360 creatives and updates ad assignments.

    Args:
        ads: Grouped ads mapping keyed by ad name.
        profile_id: CM360 user profile ID.
        advertiser_id: Optional advertiser ID filter.
        campaign_id: Optional campaign ID filter.
        tool_context: Optional ADK ToolContext.

    Returns:
        Dictionary mapping creative names to existing CM360 creative resources.

    Raises:
        ValueError: If fetching creatives fails or creative not found.
    """
    logger.info(
        "Fetching existing creatives from CM360 for Advertiser ID: %s,"
        " Campaign ID: %s",
        advertiser_id,
        campaign_id,
    )
    try:
        existing_creatives = list_cm_creatives(
            profile_id=profile_id,
            advertiser_id=advertiser_id,
            campaign_id=campaign_id,
            tool_context=tool_context,
        )
    except Exception as e:
        msg = f"Failed to fetch creatives from Campaign Manager 360: {e}"
        raise ValueError(msg) from e

    creative_name_to_id: dict[str, str] = {}
    existing_creatives_by_name: dict[str, dict[str, Any]] = {}
    for c in existing_creatives:
        name = c.get("name")
        real_id = c.get("id")
        if name and real_id:
            clean_name = name.strip()
            creative_name_to_id[clean_name] = str(real_id)
            existing_creatives_by_name[clean_name] = c

    for ad_payload in ads.values():
        """if ad_payload.get("type") == "AD_SERVING_CLICK_TRACKER":
        continue"""

        for assignment in ad_payload.get("creativeRotation", {}).get(
            "creativeAssignments", []
        ):
            name = assignment.get("creativeId")
            if name in creative_name_to_id:
                assignment["creativeId"] = creative_name_to_id[name]
            else:
                msg = f"Creative '{name}' not found in Campaign Manager 360."
                raise ValueError(msg)

    return existing_creatives_by_name


def _extract_assigned_placement_ids(
    ads: dict[str, dict[str, Any]],
) -> list[str]:
    """Extracts unique placement IDs assigned across ads.

    Args:
        ads: Grouped ads mapping keyed by ad name.

    Returns:
        List of distinct placement ID strings.
    """
    assigned_ids: list[str] = []
    for ad_payload in ads.values():
        for assignment in ad_payload.get("placementAssignments", []):
            p_id = assignment.get("placementId")
            if p_id and str(p_id) not in assigned_ids:
                assigned_ids.append(str(p_id))
    return assigned_ids


def _diff_placement(  # ruff: ignore[too-many-locals]
    sheet_placement: dict[str, Any],
    cm_placement: dict[str, Any],
) -> tuple[dict[str, Any], list[str]]:
    """Diffs sheet placement fields against existing CM360 placement.

    Compares only fields present in the sheet: pricingSchedule (startDate,
    endDate), activeStatus, and size. Note: 'name' is intentionally excluded
    from diff comparisons because placement names serve as the primary matching
    key in sheet reconciliation and cannot be renamed via spreadsheet diffing.

    Args:
        sheet_placement: Grouped placement dictionary from sheet.
        cm_placement: Existing placement resource from CM360.

    Returns:
        A tuple of (patch_payload, diff_fields) where patch_payload contains
        only the changed attributes.
    """
    patch_payload: dict[str, Any] = {}
    diff_fields: list[str] = []

    # 1. Diff pricingSchedule dates
    sheet_pricing = sheet_placement.get("pricingSchedule", {})
    cm_pricing = cm_placement.get("pricingSchedule", {})

    sheet_start = format_date(sheet_pricing.get("startDate"))
    cm_start = format_date(cm_pricing.get("startDate"))
    sheet_end = format_date(sheet_pricing.get("endDate"))
    cm_end = format_date(cm_pricing.get("endDate"))

    pricing_patch: dict[str, Any] = {}
    dates_changed = False
    if sheet_start and sheet_start != cm_start:
        pricing_patch["startDate"] = sheet_start
        diff_fields.append("pricingSchedule.startDate")
        dates_changed = True
    if sheet_end and sheet_end != cm_end:
        pricing_patch["endDate"] = sheet_end
        diff_fields.append("pricingSchedule.endDate")
        dates_changed = True

    if dates_changed:
        new_start = sheet_start or cm_start
        new_end = sheet_end or cm_end
        cm_periods = cm_pricing.get("pricingPeriods", [])
        if cm_periods:
            updated_periods = []
            for period in cm_periods:
                p = dict(period)
                p["startDate"] = new_start
                p["endDate"] = new_end
                updated_periods.append(p)
            pricing_patch["pricingPeriods"] = updated_periods
        else:
            pricing_patch["pricingPeriods"] = [
                {
                    "startDate": new_start,
                    "endDate": new_end,
                }
            ]

    if pricing_patch:
        patch_payload["pricingSchedule"] = pricing_patch

    # 2. Diff activeStatus
    sheet_status = str(sheet_placement.get("activeStatus", "")).strip()
    cm_status = str(cm_placement.get("activeStatus", "")).strip()
    if sheet_status and sheet_status != cm_status:
        patch_payload["activeStatus"] = sheet_status
        diff_fields.append("activeStatus")

    # 3. Diff size
    sheet_size = sheet_placement.get("size", {})
    cm_size = cm_placement.get("size", {})
    s_w = int(sheet_size.get("width", 0))
    s_h = int(sheet_size.get("height", 0))
    cm_w = int(cm_size.get("width", 0))
    cm_h = int(cm_size.get("height", 0))

    if (s_w, s_h) != (0, 0) and (s_w != cm_w or s_h != cm_h):
        patch_payload["size"] = {"width": s_w, "height": s_h}
        diff_fields.append("size")

    return patch_payload, diff_fields


def _diff_creative(
    sheet_creative: dict[str, Any],
    cm_creative: dict[str, Any],
) -> tuple[dict[str, Any], list[str]]:
    """Diffs sheet creative fields against existing CM360 creative.

    Compares dimensions (size width and height). Note: 'name' is intentionally
    excluded from diff comparisons because creative names serve as the primary
    matching key in sheet reconciliation and cannot be renamed via spreadsheet
    diffing.

    Args:
        sheet_creative: Grouped creative dictionary from sheet.
        cm_creative: Existing creative resource from CM360.

    Returns:
        A tuple of (patch_payload, diff_fields) where patch_payload contains
        only the changed attributes.
    """
    patch_payload: dict[str, Any] = {}
    diff_fields: list[str] = []

    # Diff size
    sheet_size = sheet_creative.get("size", {})
    cm_size = cm_creative.get("size", {})
    s_w = int(sheet_size.get("width", 0))
    s_h = int(sheet_size.get("height", 0))
    cm_w = int(cm_size.get("width", 0))
    cm_h = int(cm_size.get("height", 0))

    if (s_w, s_h) != (0, 0) and (s_w != cm_w or s_h != cm_h):
        patch_payload["size"] = {"width": s_w, "height": s_h}
        diff_fields.append("size")

    return patch_payload, diff_fields


def _diff_ad(  # ruff: ignore[complex-structure, too-many-branches, too-many-locals, too-many-statements]
    sheet_ad: dict[str, Any],
    cm_ad: dict[str, Any],
) -> tuple[dict[str, Any], list[str]]:
    """Diffs sheet ad fields against existing CM360 ad.

    Compares only fields present in the sheet: startTime, endTime,
    deliverySchedule, dynamicClickTracker, clickThroughUrl,
    placementAssignments, creativeRotation, and eventTagOverrides. Note: 'name'
    is intentionally excluded from diff comparisons because ad names serve as
    the primary matching key in sheet reconciliation and cannot be renamed via
    spreadsheet diffing. Similarly, 'type' (e.g. AD_SERVING_STANDARD_AD vs
    AD_SERVING_CLICK_TRACKER) is immutable in CM360 after creation and cannot
    be modified on existing ads.

    Args:
        sheet_ad: Grouped ad dictionary from sheet (with reconciled IDs).
        cm_ad: Existing ad resource from CM360.

    Returns:
        A tuple of (patch_payload, diff_fields) where patch_payload contains
        only the changed attributes.
    """
    patch_payload: dict[str, Any] = {}
    diff_fields: list[str] = []

    # Note: 'type' is intentionally not diffed because an ad's type is
    # immutable in CM360 after creation and cannot be changed via patch.

    # 1. Diff ad dates
    sheet_start = normalize_iso_datetime(sheet_ad.get("startTime"))
    cm_start = normalize_iso_datetime(cm_ad.get("startTime"))
    if sheet_start and sheet_start != cm_start:
        patch_payload["startTime"] = sheet_start
        diff_fields.append("startTime")

    sheet_end = normalize_iso_datetime(sheet_ad.get("endTime"))
    cm_end = normalize_iso_datetime(cm_ad.get("endTime"))
    if sheet_end and sheet_end != cm_end:
        patch_payload["endTime"] = sheet_end
        diff_fields.append("endTime")

    # 2. Diff deliverySchedule (for standard/tracking ads)
    sheet_ds = sheet_ad.get("deliverySchedule")
    if sheet_ds:
        cm_ds = cm_ad.get("deliverySchedule", {})
        s_priority = str(sheet_ds.get("priority", "")).strip()
        c_priority = str(cm_ds.get("priority", "")).strip()
        s_ratio = str(sheet_ds.get("impressionRatio", "")).strip()
        c_ratio = str(cm_ds.get("impressionRatio", "")).strip()

        if (s_priority and s_priority != c_priority) or (
            s_ratio and s_ratio != c_ratio
        ):
            patch_payload["deliverySchedule"] = sheet_ds
            diff_fields.append("deliverySchedule")

    # 3. Diff dynamicClickTracker (for click tracker ads)
    if "dynamicClickTracker" in sheet_ad:
        s_dct = bool(sheet_ad.get("dynamicClickTracker"))
        c_dct = bool(cm_ad.get("dynamicClickTracker"))
        if s_dct != c_dct:
            patch_payload["dynamicClickTracker"] = s_dct
            diff_fields.append("dynamicClickTracker")

    # 4. Diff clickThroughUrl (for click tracker ads)
    if "clickThroughUrl" in sheet_ad:
        s_url = (
            sheet_ad.get("clickThroughUrl", {}).get("customClickThroughUrl", "").strip()
        )
        c_url = (
            cm_ad.get("clickThroughUrl", {}).get("customClickThroughUrl", "").strip()
        )
        if s_url and s_url != c_url:
            patch_payload["clickThroughUrl"] = sheet_ad["clickThroughUrl"]
            diff_fields.append("ad_level_clickThroughUrl")

    # 5. Diff placementAssignments
    s_placements = {
        str(pa.get("placementId", "")).strip()
        for pa in sheet_ad.get("placementAssignments", [])
        if pa.get("placementId")
    }
    c_placements = {
        str(pa.get("placementId", "")).strip()
        for pa in cm_ad.get("placementAssignments", [])
        if pa.get("placementId")
    }
    if s_placements and s_placements != c_placements:
        patch_payload["placementAssignments"] = sheet_ad["placementAssignments"]
        diff_fields.append("placementAssignments")

    # 6. Diff creativeRotation (only for other ads)
    s_ad_type = sheet_ad.get("type")
    if s_ad_type != "AD_SERVING_CLICK_TRACKER":
        s_assignments = sheet_ad.get("creativeRotation", {}).get(
            "creativeAssignments", []
        )
        if s_assignments:
            c_assignments = cm_ad.get("creativeRotation", {}).get(
                "creativeAssignments", []
            )
            s_ca_map = {
                str(ca.get("creativeId", "")).strip(): ca
                for ca in s_assignments
                if ca.get("creativeId")
            }
            c_ca_map = {
                str(ca.get("creativeId", "")).strip(): ca
                for ca in c_assignments
                if ca.get("creativeId")
            }

            rotation_changed = False
            creative_rotation_url_changed = False
            if set(s_ca_map.keys()) != set(c_ca_map.keys()):
                rotation_changed = True

            for cid in set(s_ca_map.keys()) & set(c_ca_map.keys()):
                s_ca = s_ca_map[cid]
                c_ca = c_ca_map[cid]
                s_weight = s_ca.get("weight")
                c_weight = c_ca.get("weight")
                # When there is 1 creative in the rotation
                # weight is not assigned in CM and it's None,
                # add this or this would always lead to unnecessary diffs.
                if len(c_ca_map) > 1 and int(s_weight) != int(c_weight):
                    rotation_changed = True

                s_url = (
                    s_ca.get("clickThroughUrl", {})
                    .get("customClickThroughUrl", "")
                    .strip()
                )
                c_url = (
                    c_ca.get("clickThroughUrl", {})
                    .get("customClickThroughUrl", "")
                    .strip()
                )
                if s_url and s_url != c_url:
                    creative_rotation_url_changed = True

            if rotation_changed or creative_rotation_url_changed:
                patch_payload["creativeRotation"] = sheet_ad["creativeRotation"]
                if rotation_changed:
                    diff_fields.append("creativeRotation")
                if (
                    creative_rotation_url_changed
                    and "clickThroughUrl" not in diff_fields
                ):
                    diff_fields.append("creative_level_clickThroughUrl")

    # 7. Diff eventTagOverrides
    s_overrides = {
        str(o.get("id", "")).strip(): o.get("enabled")
        for o in sheet_ad.get("eventTagOverrides", [])
        if o.get("id")
    }
    c_overrides = {
        str(o.get("id", "")).strip(): o.get("enabled")
        for o in cm_ad.get("eventTagOverrides", [])
        if o.get("id")
    }
    if s_overrides != c_overrides:
        patch_payload["eventTagOverrides"] = sheet_ad["eventTagOverrides"]
        diff_fields.append("eventTagOverrides")

    return patch_payload, diff_fields


def _diff_event_tag(
    sheet_event_tag: dict[str, Any],
    cm_event_tag: dict[str, Any],
) -> tuple[dict[str, Any], list[str]]:
    """Diffs sheet event tag fields against existing CM360 event tag.

    Compares only fields present in the sheet: type, url, and status.
    Note: 'name' is intentionally excluded from diff comparisons because
    entity names serve as the primary matching key in sheet reconciliation
    and cannot be renamed via spreadsheet diffing.

    Args:
        sheet_event_tag: Grouped event tag dictionary from sheet.
        cm_event_tag: Existing event tag resource from CM360.

    Returns:
        A tuple of (patch_payload, diff_fields) where patch_payload contains
        only the changed attributes.
    """
    patch_payload: dict[str, Any] = {}
    diff_fields: list[str] = []

    # 1. Diff type
    sheet_type = str(sheet_event_tag.get("type", "")).strip().upper()
    cm_type = str(cm_event_tag.get("type", "")).strip().upper()
    if sheet_type and sheet_type != cm_type:
        patch_payload["type"] = sheet_type
        diff_fields.append("type")

    # 2. Diff url
    sheet_url = str(sheet_event_tag.get("url", "")).strip()
    cm_url = str(cm_event_tag.get("url", "")).strip()
    if sheet_url and sheet_url != cm_url:
        patch_payload["url"] = sheet_url
        diff_fields.append("url")

    # 3. Diff status
    sheet_status = str(sheet_event_tag.get("status", "")).strip().upper()
    cm_status = str(cm_event_tag.get("status", "")).strip().upper()
    if sheet_status and sheet_status != cm_status:
        patch_payload["status"] = sheet_status
        diff_fields.append("status")

    return patch_payload, diff_fields


def _resolve_and_build_operations(  # ruff: ignore[complex-structure, too-many-arguments, too-many-branches, too-many-locals, too-many-positional-arguments, too-many-statements]
    ads: dict[str, dict[str, Any]],
    event_tags: dict[str, dict[str, Any]],
    profile_id: str,
    advertiser_id: str | None = None,
    campaign_id: str | None = None,
    tool_context: ToolContext | None = None,
    placements: dict[str, dict[str, Any]] | None = None,
    creatives: dict[str, dict[str, Any]] | None = None,
    existing_placements: dict[str, dict[str, Any]] | None = None,
    existing_creatives: dict[str, dict[str, Any]] | None = None,
) -> list[dict[str, Any]]:
    """Fetches live existing event tags and ads, and reconciles operations.

    Args:
        ads: Grouped ads mapping keyed by ad name.
        event_tags: Grouped event tags mapping keyed by tag name.
        profile_id: CM360 user profile ID.
        advertiser_id: Optional advertiser ID.
        campaign_id: Optional campaign ID.
        tool_context: Optional ADK ToolContext.
        placements: Optional grouped placements mapping keyed by placement name.
        creatives: Optional grouped creatives mapping keyed by creative name.
        existing_placements: Optional existing CM360 placements mapped by name.
        existing_creatives: Optional existing CM360 creatives mapped by name.

    Returns:
        A list of operation dictionaries (placements, creatives, event tags,
        then ads) with resolved operation types and existing IDs.
    """
    logger.info("Fetching existing event tags and ads from CM360 for reconciliation...")
    assigned_placement_ids = _extract_assigned_placement_ids(ads)
    existing_tags_list = list_cm_event_tags(
        profile_id=profile_id,
        advertiser_id=advertiser_id,
        campaign_id=campaign_id,
        tool_context=tool_context,
    )
    existing_ads_list = list_cm_ads(
        profile_id=profile_id,
        advertiser_id=advertiser_id,
        campaign_ids=[campaign_id] if campaign_id else None,
        placement_ids=assigned_placement_ids or None,
        tool_context=tool_context,
    )

    existing_event_tags_by_name: dict[str, dict[str, Any]] = {
        str(t.get("name")).strip(): t
        for t in existing_tags_list
        if t.get("name") and t.get("id")
    }
    existing_ads_by_name = {
        str(a.get("name")).strip(): a
        for a in existing_ads_list
        if a.get("name") and a.get("id")
    }

    operations: list[dict[str, Any]] = []

    # 1. Build Placement PATCH operations (diffs only)
    if placements and existing_placements:
        for placement_name, sheet_payload in placements.items():
            clean_name = str(placement_name).strip()
            if clean_name in existing_placements:
                cm_placement = existing_placements[clean_name]
                patch_payload, diff_fields = _diff_placement(
                    sheet_placement=sheet_payload,
                    cm_placement=cm_placement,
                )
                if diff_fields:
                    placement_id = str(cm_placement.get("id"))
                    operations.append(
                        {
                            "operation": "dfareporting.placements.patch",
                            "id": placement_id,
                            "name": clean_name,
                            "diff_fields": diff_fields,
                            "payload": patch_payload,
                        }
                    )

    # 2. Build Creative PATCH operations (diffs only)
    if creatives and existing_creatives:
        for creative_name, sheet_payload in creatives.items():
            clean_name = str(creative_name).strip()
            if clean_name in existing_creatives:
                cm_creative = existing_creatives[clean_name]
                patch_payload, diff_fields = _diff_creative(
                    sheet_creative=sheet_payload,
                    cm_creative=cm_creative,
                )
                if diff_fields:
                    creative_id = str(cm_creative.get("id"))
                    operations.append(
                        {
                            "operation": "dfareporting.creatives.patch",
                            "id": creative_id,
                            "name": clean_name,
                            "diff_fields": diff_fields,
                            "payload": patch_payload,
                        }
                    )

    # 3. Build Event Tag operations: INSERT for new tags, PATCH for diffs
    if event_tags:
        for tag_name, raw_payload in event_tags.items():
            tag_payload = dict(raw_payload)
            clean_tag_name = str(tag_name).strip()
            if clean_tag_name in existing_event_tags_by_name:
                cm_tag = existing_event_tags_by_name[clean_tag_name]
                tag_id = str(cm_tag.get("id"))
                tag_payload["id"] = tag_id
                patch_payload, diff_fields = _diff_event_tag(
                    sheet_event_tag=tag_payload,
                    cm_event_tag=cm_tag,
                )
                if diff_fields:
                    operations.append(
                        {
                            "operation": "dfareporting.eventTags.patch",
                            "id": tag_id,
                            "name": clean_tag_name,
                            "diff_fields": diff_fields,
                            "payload": patch_payload,
                        }
                    )
                else:
                    logger.info(
                        "Event tag '%s' (ID: %s) already matches CM360 state."
                        " Skipping patch.",
                        clean_tag_name,
                        tag_id,
                    )
            else:
                operations.append(
                    {
                        "operation": "dfareporting.eventTags.insert",
                        "name": clean_tag_name,
                        "payload": tag_payload,
                    }
                )

    # 4. Resolve existing Event Tag IDs in Ad overrides
    for ad_payload in ads.values():
        for override in ad_payload.get("eventTagOverrides", []):
            target_tag = str(override.get("id", "")).strip()
            if target_tag in existing_event_tags_by_name:
                override["id"] = str(existing_event_tags_by_name[target_tag].get("id"))

    # 5. Build Ad operations: INSERT for new ads, PATCH for diffs
    if ads:
        for ad_name, raw_payload in ads.items():
            ad_payload = dict(raw_payload)
            clean_ad_name = str(ad_name).strip()
            payload_expected_placement_ids = [
                str(assignment.get("placementId")).strip()
                for assignment in ad_payload.get("placementAssignments", [])
                if assignment.get("placementId")
            ]

            if clean_ad_name in existing_ads_by_name:
                existing_ad = existing_ads_by_name[clean_ad_name]
                ad_id = str(existing_ad.get("id"))
                ad_payload["id"] = ad_id
                _reconcile_missing_ad_placements(
                    payload=ad_payload,
                    existing_ad=existing_ad,
                    payload_expected_placement_ids=payload_expected_placement_ids,
                    ad_name=clean_ad_name,
                    ad_id=ad_id,
                )
                patch_payload, diff_fields = _diff_ad(
                    sheet_ad=ad_payload,
                    cm_ad=existing_ad,
                )
                if diff_fields:
                    operations.append(
                        {
                            "operation": "dfareporting.ads.patch",
                            "id": ad_id,
                            "name": clean_ad_name,
                            "diff_fields": diff_fields,
                            "payload": patch_payload,
                        }
                    )
                else:
                    logger.info(
                        "Ad '%s' (ID: %s) already matches CM360 state."
                        " Skipping patch.",
                        clean_ad_name,
                        ad_id,
                    )
            else:
                operations.append(
                    {
                        "operation": "dfareporting.ads.insert",
                        "name": clean_ad_name,
                        "payload": ad_payload,
                    }
                )

    return operations


def _execute_placement_operation(
    cm360_service: Any,  # ruff: ignore[any-type]
    profile_id: str,
    op: dict[str, Any],
) -> dict[str, Any]:
    """Executes a single placement patch operation.

    Args:
        cm360_service: Google API Client dfareporting service.
        profile_id: CM360 user profile ID.
        op: Placement operation dictionary.

    Returns:
        A dictionary containing the execution result record.

    Raises:
        ValueError: If the placement operation type is unsupported.
    """
    operation_type = op.get("operation", "")
    payload = dict(op.get("payload", {}))
    placement_name = str(op.get("name", "")).strip()
    placement_id = str(op.get("id", "")).strip()

    if operation_type != "dfareporting.placements.patch":
        msg = f"Unsupported placement operation: {operation_type}"
        raise ValueError(msg)

    logger.info(
        "Executing %s for %s (ID: %s)",
        operation_type,
        placement_name,
        placement_id,
    )
    try:
        request_call = cm360_service.placements().patch(
            profileId=profile_id, id=placement_id, body=payload
        )
        res = request_call.execute()
    except Exception as api_err:
        logger.exception("Failed to execute placement patch for %s", placement_name)
        return {
            "id": placement_id,
            "name": placement_name,
            "operation": operation_type,
            "status": "ERROR",
            "message": f"Execution failed: {api_err}",
        }
    else:
        real_id = str(res.get("id", placement_id))
        logger.info(
            "Successfully executed %s for %s (ID: %s)",
            operation_type,
            placement_name,
            real_id,
        )
        return {
            "id": real_id,
            "name": placement_name,
            "operation": operation_type,
            "status": "SUCCESS",
            "message": (
                f"Placement '{placement_name}' patched successfully with"
                f" ID: {real_id}."
            ),
        }


def _process_placement_operations(
    cm360_service: Any,  # ruff: ignore[any-type]
    profile_id: str,
    placement_ops: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    """Processes all placement patch operations and collects results.

    Args:
        cm360_service: Google API Client dfareporting service.
        profile_id: CM360 user profile ID.
        placement_ops: List of placement operations.

    Returns:
        List of execution result dictionaries.
    """
    results: list[dict[str, Any]] = []
    for op in placement_ops:
        result_record = _execute_placement_operation(
            cm360_service=cm360_service,
            profile_id=profile_id,
            op=op,
        )
        results.append(result_record)
    return results


def _execute_creative_operation(
    cm360_service: Any,  # ruff: ignore[any-type]
    profile_id: str,
    op: dict[str, Any],
) -> dict[str, Any]:
    """Executes a single creative patch operation.

    Args:
        cm360_service: Google API Client dfareporting service.
        profile_id: CM360 user profile ID.
        op: Creative operation dictionary.

    Returns:
        A dictionary containing the execution result record.

    Raises:
        ValueError: If the creative operation type is unsupported.
    """
    operation_type = op.get("operation", "")
    payload = dict(op.get("payload", {}))
    creative_name = str(op.get("name", "")).strip()
    creative_id = str(op.get("id", "")).strip()

    if operation_type != "dfareporting.creatives.patch":
        msg = f"Unsupported creative operation: {operation_type}"
        raise ValueError(msg)

    logger.info(
        "Executing %s for %s (ID: %s)",
        operation_type,
        creative_name,
        creative_id,
    )
    try:
        request_call = cm360_service.creatives().patch(
            profileId=profile_id, id=creative_id, body=payload
        )
        res = request_call.execute()
    except Exception as api_err:
        logger.exception("Failed to execute creative patch for %s", creative_name)
        return {
            "id": creative_id,
            "name": creative_name,
            "operation": operation_type,
            "status": "ERROR",
            "message": f"Execution failed: {api_err}",
        }
    else:
        real_id = str(res.get("id", creative_id))
        logger.info(
            "Successfully executed %s for %s (ID: %s)",
            operation_type,
            creative_name,
            real_id,
        )
        return {
            "id": real_id,
            "name": creative_name,
            "operation": operation_type,
            "status": "SUCCESS",
            "message": (
                f"Creative '{creative_name}' patched successfully with"
                f" ID: {real_id}."
            ),
        }


def _process_creative_operations(
    cm360_service: Any,  # ruff: ignore[any-type]
    profile_id: str,
    creative_ops: list[dict[str, Any]],
) -> list[dict[str, Any]]:
    """Processes all creative patch operations and collects results.

    Args:
        cm360_service: Google API Client dfareporting service.
        profile_id: CM360 user profile ID.
        creative_ops: List of creative operations.

    Returns:
        List of execution result dictionaries.
    """
    results: list[dict[str, Any]] = []
    for op in creative_ops:
        result_record = _execute_creative_operation(
            cm360_service=cm360_service,
            profile_id=profile_id,
            op=op,
        )
        results.append(result_record)
    return results


def _execute_event_tag_operation(
    cm360_service: Any,  # ruff: ignore[any-type]
    profile_id: str,
    op: dict[str, Any],
    event_tag_mappings: dict[str, str],
) -> dict[str, Any]:
    """Executes a single event tag insert or update operation.

    Args:
        cm360_service: Google API Client dfareporting service.
        profile_id: CM360 user profile ID.
        op: Operation dictionary containing operation type, name, and payload.
        event_tag_mappings: Mutable mapping from tag name to tag ID.

    Returns:
        A dictionary containing the execution result record.

    Raises:
        ValueError: If the operation type is unsupported.
    """
    payload = dict(op.get("payload", {}))
    tag_name = str(op.get("name", "")).strip()
    operation_type = op.get("operation", "")

    tag_id = str(op.get("id", payload.get("id", "")))
    logger.info("Executing %s for %s", operation_type, tag_name)
    if operation_type == "dfareporting.eventTags.insert":
        request_call = cm360_service.eventTags().insert(
            profileId=profile_id, body=payload
        )
        res = request_call.execute()
    elif operation_type == "dfareporting.eventTags.patch":
        request_call = cm360_service.eventTags().patch(
            profileId=profile_id, id=tag_id, body=payload
        )
        res = request_call.execute()
    elif operation_type == "dfareporting.eventTags.update":
        request_call = cm360_service.eventTags().update(
            profileId=profile_id, body=payload
        )
        res = request_call.execute()
    else:
        msg = f"Unsupported event tag operation: {operation_type}"
        raise ValueError(msg)

    real_id = str(res.get("id", payload.get("id", "")))
    event_tag_mappings[tag_name] = real_id
    logger.info(
        "Successfully executed %s for %s (ID: %s)",
        operation_type,
        tag_name,
        real_id,
    )

    return {
        "id": real_id,
        "name": tag_name,
        "operation": operation_type,
        "status": "SUCCESS",
        "message": (
            f"Event tag '{tag_name}' operation {operation_type} executed"
            f" successfully with ID: {real_id}."
        ),
    }


def _process_event_tag_operations(
    cm360_service: Any,  # ruff: ignore[any-type]
    profile_id: str,
    event_tag_ops: list[dict[str, Any]],
) -> tuple[list[dict[str, Any]], dict[str, str], set[str]]:
    """Processes all event tag operations and tracks any failures.

    Args:
        cm360_service: Google API Client dfareporting service.
        profile_id: CM360 user profile ID.
        event_tag_ops: List of event tag operations.

    Returns:
        A tuple of (results_list, created_event_tag_mappings,
        failed_event_tag_names).
    """
    results: list[dict[str, Any]] = []
    created_event_tag_mappings: dict[str, str] = {}
    failed_event_tag_names: set[str] = set()

    for op in event_tag_ops:
        tag_name = str(op.get("name", "")).strip()
        operation_type = op.get("operation", "")
        try:
            result_record = _execute_event_tag_operation(
                cm360_service=cm360_service,
                profile_id=profile_id,
                op=op,
                event_tag_mappings=created_event_tag_mappings,
            )
            results.append(result_record)
        except Exception as api_err:
            logger.exception(
                "Failed to execute tag operation %s for %s",
                operation_type,
                tag_name,
            )
            failed_event_tag_names.add(tag_name)
            results.append(
                {
                    "name": tag_name,
                    "operation": operation_type,
                    "status": "ERROR",
                    "message": str(api_err),
                }
            )

    return results, created_event_tag_mappings, failed_event_tag_names


def _validate_and_resolve_ad_event_tags(
    payload: dict[str, Any],
    existing_event_tag_mappings: dict[str, str],
    failed_event_tag_names: set[str],
) -> tuple[bool, list[str]]:
    """Validates ad event tag dependencies and resolves tag names to real IDs.

    Args:
        payload: Mutable ad payload dictionary containing eventTagOverrides.
        existing_event_tag_mappings: Mapping of event tag names to real IDs.
        failed_event_tag_names: Set of event tag names that failed.

    Returns:
        A tuple of (is_valid, missing_or_failed_tags).
    """
    payload_event_tag_overrides = payload.get("eventTagOverrides", [])
    missing_or_failed_tags: list[str] = []

    for override in payload_event_tag_overrides:
        target_tag = str(override.get("id", "")).strip()
        if target_tag in failed_event_tag_names or (
            target_tag not in existing_event_tag_mappings and not target_tag.isdigit()
        ):
            missing_or_failed_tags.append(target_tag)

    if missing_or_failed_tags:
        return False, missing_or_failed_tags

    for override in payload_event_tag_overrides:
        target_tag = str(override.get("id", "")).strip()
        if target_tag in existing_event_tag_mappings:
            override["id"] = existing_event_tag_mappings[target_tag]

    return True, []


def _reconcile_missing_ad_placements(
    payload: dict[str, Any],
    existing_ad: dict[str, Any],
    payload_expected_placement_ids: list[str],
    ad_name: str,
    ad_id: str,
) -> None:
    """Merges existing placement assignments with payload placements.

    Args:
        payload: Mutable ad payload dictionary.
        existing_ad: Existing ad resource from CM360.
        payload_expected_placement_ids: List of placement IDs from payload.
        ad_name: Ad name for logging.
        ad_id: Ad ID for logging.
    """
    existing_actual_placement_ids = {
        str(pa.get("placementId")).strip()
        for pa in existing_ad.get("placementAssignments", [])
        if pa.get("placementId")
    }
    unassigned_placement_ids = [
        p
        for p in payload_expected_placement_ids
        if p not in existing_actual_placement_ids
    ]

    merged_assignments = list(existing_ad.get("placementAssignments", []))
    seen_new_ids: set[str] = set()
    for assignment in payload.get("placementAssignments", []):
        p_id = str(assignment.get("placementId", "")).strip()
        if p_id in unassigned_placement_ids and p_id not in seen_new_ids:
            merged_assignments.append(assignment)
            seen_new_ids.add(p_id)

    payload["placementAssignments"] = merged_assignments

    if unassigned_placement_ids:
        logger.info(
            "Ad exists but is missing placements. Assigning new placements for"
            " %s (ID: %s).",
            ad_name,
            ad_id,
        )


def _execute_ad_operation(
    cm360_service: Any,  # ruff: ignore[any-type]
    profile_id: str,
    op: dict[str, Any],
    event_tag_mappings: dict[str, str],
    failed_event_tag_names: set[str],
) -> dict[str, Any]:
    """Validates dependencies and executes a single ad operation.

    Args:
        cm360_service: Google API Client dfareporting service.
        profile_id: CM360 user profile ID.
        op: Ad operation dictionary.
        event_tag_mappings: Mapping from tag name to real ID.
        failed_event_tag_names: Set of event tag names that failed.

    Returns:
        A dictionary containing the execution result record.

    Raises:
        ValueError: If the ad operation type is unsupported.
    """
    operation_type = op.get("operation", "")
    payload = dict(op.get("payload", {}))
    ad_name = str(op.get("name", "")).strip()

    # Validate associated event tags
    is_valid, missing_tags = _validate_and_resolve_ad_event_tags(
        payload=payload,
        existing_event_tag_mappings=event_tag_mappings,
        failed_event_tag_names=failed_event_tag_names,
    )
    if not is_valid:
        logger.warning(
            "Skipping ad '%s' because associated event tag(s) failed or are"
            " missing: %s",
            ad_name,
            missing_tags,
        )
        return {
            "name": ad_name,
            "operation": operation_type,
            "status": "ERROR",
            "message": (
                "Ad operation aborted: Dependent event tag(s) failed or"
                f" missing: {', '.join(missing_tags)}."
            ),
        }

    if operation_type not in {
        "dfareporting.ads.insert",
        "dfareporting.ads.patch",
        "dfareporting.ads.update",
    }:
        msg = f"Unsupported ad operation: {operation_type}"
        raise ValueError(msg)

    ad_id = str(op.get("id", payload.get("id", "")))
    logger.info("Executing %s for %s", operation_type, ad_name)
    if operation_type == "dfareporting.ads.insert":
        request_call = cm360_service.ads().insert(profileId=profile_id, body=payload)
    elif operation_type == "dfareporting.ads.patch":
        request_call = cm360_service.ads().patch(
            profileId=profile_id, id=ad_id, body=payload
        )
    else:
        request_call = cm360_service.ads().update(profileId=profile_id, body=payload)
    try:
        res = request_call.execute()
    except Exception as api_err:
        logger.exception(
            "Failed to execute ad operation %s for %s",
            operation_type,
            ad_name,
        )
        return {
            "id": ad_id,
            "name": ad_name,
            "operation": operation_type,
            "status": "ERROR",
            "message": f"Execution failed: {api_err}",
        }
    else:
        real_ad_id = str(res.get("id", ad_id))
        logger.info(
            "Successfully executed %s for %s (ID: %s)",
            operation_type,
            ad_name,
            real_ad_id,
        )
        return {
            "id": real_ad_id,
            "name": ad_name,
            "operation": operation_type,
            "status": "SUCCESS",
            "message": (
                f"Ad operation {operation_type} executed successfully with"
                f" ID: {real_ad_id}."
            ),
        }


def _process_ad_operations(
    cm360_service: Any,  # ruff: ignore[any-type]
    profile_id: str,
    ad_ops: list[dict[str, Any]],
    event_tag_mappings: dict[str, str],
    failed_event_tag_names: set[str],
) -> list[dict[str, Any]]:
    """Processes all ad operations and collects their execution results.

    Args:
        cm360_service: Google API Client dfareporting service.
        profile_id: CM360 user profile ID.
        ad_ops: List of ad operations.
        event_tag_mappings: Mapping from tag name to real ID.
        failed_event_tag_names: Set of event tag names that failed.

    Returns:
        List of ad execution result dictionaries.
    """
    results: list[dict[str, Any]] = []
    for op in ad_ops:
        result_record = _execute_ad_operation(
            cm360_service=cm360_service,
            profile_id=profile_id,
            op=op,
            event_tag_mappings=event_tag_mappings,
            failed_event_tag_names=failed_event_tag_names,
        )
        results.append(result_record)
    return results


def _build_trafficking_summary_response(  # ruff: ignore[too-many-arguments, too-many-positional-arguments]
    results: list[dict[str, Any]],
    total_operations_count: int,
    profile_id: str | None = None,
    advertiser_id: str | None = None,
    campaign_id: str | None = None,
    campaign_name: str | None = None,
) -> str:
    """Builds a JSON summary string from trafficking operation results.

    Args:
        results: List of execution result dictionaries.
        total_operations_count: Total number of operations scheduled.
        profile_id: Optional CM360 user profile ID.
        advertiser_id: Optional advertiser ID.
        campaign_id: Optional campaign ID.
        campaign_name: Optional campaign name.

    Returns:
        A JSON-formatted string summarizing status and results.
    """
    success_count = sum(1 for r in results if r.get("status") == "SUCCESS")
    skipped_count = sum(1 for r in results if r.get("status") == "SKIPPED")
    error_count = sum(1 for r in results if r.get("status") == "ERROR")

    clean_results: list[dict[str, Any]] = []
    for r in results:
        record: dict[str, Any] = {}
        if r.get("id"):
            record["id"] = r.get("id")
        record["name"] = r.get("name")
        record["operation"] = r.get("operation")
        record["status"] = r.get("status")
        if r.get("status") == "ERROR" and r.get("message"):
            record["message"] = r.get("message")
        clean_results.append(record)

    response_payload: dict[str, Any] = {
        "status": (
            "SUCCESS"
            if error_count == 0
            else (
                "ERROR" if (success_count + skipped_count == 0) else "PARTIAL_SUCCESS"
            )
        ),
        "message": (
            f"Successfully executed trafficking for {total_operations_count}"
            f" operation(s): {success_count} processed,"
            f" {skipped_count} skipped."
            if error_count == 0
            else (
                f"Trafficking execution encountered {error_count} error(s) "
                f"({success_count} processed, {skipped_count} skipped). "
                "Please try again to reprocess missing entities."
            )
        ),
    }

    if profile_id:
        response_payload["profile_id"] = profile_id
    if advertiser_id:
        response_payload["advertiser_id"] = advertiser_id
    if campaign_id:
        response_payload["campaign_id"] = campaign_id
    if campaign_name:
        response_payload["campaign_name"] = campaign_name

    response_payload["results"] = clean_results

    return json.dumps(response_payload, indent=2)


async def _update_trafficking_sheet_status(  # ruff: ignore[complex-structure, too-many-branches, too-many-locals, too-many-statements]
    tool_context: ToolContext,
    results: list[dict[str, Any]],
) -> None:
    """Updates the trafficking sheet artifact with 'Trafficked' status.

    Locates the latest trafficking CSV artifact, identifies rows matching
    successfully trafficked ads (matching on Placement Name + Ad Name),
    updates their Trafficking Status to 'Trafficked', and saves the updated
    file as a new artifact version in the session.

    Args:
        tool_context: Active tool context containing session artifacts.
        results: List of execution results from CM360 trafficking push.
    """
    successful_ads = {
        str(r.get("name")).strip()
        for r in results
        if r.get("status") == "SUCCESS"
        and str(r.get("operation", "")).startswith("dfareporting.ads.")
    }
    successful_placements = {
        str(r.get("name")).strip()
        for r in results
        if r.get("status") == "SUCCESS"
        and str(r.get("operation", "")).startswith("dfareporting.placements.")
    }
    if not successful_ads and not successful_placements:
        logger.info(
            "ℹ️ [traffic_campaigns_in_cm360_tool] - No successful ad or"  # ruff: ignore[ambiguous-unicode-character-string]
            " placement operations to update in trafficking sheet."
        )
        return

    df_raw, target_filename = await load_raw_dataframe(tool_context)

    header_row_idx: int | None = None
    for idx, row in df_raw.iterrows():
        if not isinstance(idx, int) or idx < GLOBAL_METADATA_ROWS_COUNT:
            continue
        row_values = [str(v).strip() for v in row.to_numpy() if not pd.isna(v)]
        if (
            "Trafficking Status" in row_values
            or "Placement Name" in row_values
            or "Ad Name" in row_values
        ):
            header_row_idx = idx
            break

    if header_row_idx is None:
        for idx, row in df_raw.iterrows():
            if not isinstance(idx, int):
                continue
            row_values = [str(v).strip() for v in row.to_numpy() if not pd.isna(v)]
            if (
                "Trafficking Status" in row_values
                or "Placement Name" in row_values
                or "Ad Name" in row_values
            ):
                header_row_idx = idx
                break

    if header_row_idx is None:
        logger.warning(
            "⚠️ [traffic_campaigns_in_cm360_tool] - Could not locate header"
            " row in %s to update status.",
            target_filename,
        )
        return

    header_row = df_raw.iloc[header_row_idx]
    status_col_idx: int | None = None
    ad_name_col_idx: int | None = None
    placement_name_col_idx: int | None = None

    for col_idx, col_name in enumerate(header_row):
        col_str = str(col_name).strip().lower()
        if col_str == "trafficking status":
            status_col_idx = col_idx
        elif col_str == "ad name":
            ad_name_col_idx = col_idx
        elif col_str == "placement name":
            placement_name_col_idx = col_idx

    if (
        status_col_idx is None
        or ad_name_col_idx is None
        or placement_name_col_idx is None
    ):
        logger.warning(
            "⚠️ [traffic_campaigns_in_cm360_tool] - Required columns"
            " ('Trafficking Status', 'Placement Name', or 'Ad Name') not"
            " found in %s.",
            target_filename,
        )
        return

    updated_count = 0
    for row_idx in range(header_row_idx + 1, len(df_raw)):
        ad_name_val = df_raw.iloc[row_idx, ad_name_col_idx]
        placement_val = df_raw.iloc[row_idx, placement_name_col_idx]

        if pd.isna(ad_name_val) or pd.isna(placement_val):
            continue

        ad_name = str(ad_name_val).strip()
        placement_name = str(placement_val).strip()

        if not ad_name or not placement_name:
            continue

        if ad_name in successful_ads or placement_name in successful_placements:
            df_raw.iloc[row_idx, status_col_idx] = "Trafficked"
            updated_count += 1

    csv_buffer = io.StringIO()
    df_raw.to_csv(csv_buffer, index=False, header=False)
    updated_csv_content = csv_buffer.getvalue()

    artifact = types.Part(text=updated_csv_content)
    await tool_context.save_artifact(filename=target_filename, artifact=artifact)
    logger.info(
        "📄 [traffic_campaigns_in_cm360_tool] - Successfully updated %d"
        " row(s) to 'Trafficked' in artifact '%s'.",
        updated_count,
        target_filename,
    )
