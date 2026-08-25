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
"""Helper functions for CM360 campaign trafficking execution and entity resolution."""

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
from .utilities import load_raw_dataframe

logger = logging.getLogger(__name__)


def _resolve_placement_ids(
    ads: dict[str, dict[str, Any]],
    profile_id: str,
    advertiser_id: str | None = None,
    campaign_id: str | None = None,
    tool_context: ToolContext | None = None,
) -> None:
    """Fetches existing CM360 placements and updates ad placement assignments with real IDs.

    Args:
        ads: Grouped ads mapping keyed by ad name.
        profile_id: CM360 user profile ID.
        advertiser_id: Optional advertiser ID filter.
        campaign_id: Optional campaign ID filter.
        tool_context: Optional ADK ToolContext.

    Raises:
        ValueError: If fetching placements fails or an assigned placement is not found in CM360.
    """
    logger.info(
        "Fetching existing placements from CM360 for Advertiser ID: %s, Campaign ID: %s",
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
        raise ValueError(
            f"Failed to fetch placements from Campaign Manager 360: {e}"
        ) from e

    placement_name_to_id: dict[str, str] = {}
    for p in existing_placements:
        name = p.get("name")
        real_id = p.get("id")
        if name and real_id:
            placement_name_to_id[name.strip()] = str(real_id)

    for ad_payload in ads.values():
        for assignment in ad_payload.get("placementAssignments", []):
            name = assignment.get("placementId")
            if name in placement_name_to_id:
                assignment["placementId"] = placement_name_to_id[name]
            else:
                raise ValueError(
                    f"Placement '{name}' not found in Campaign Manager 360."
                )


def _resolve_creative_ids(
    ads: dict[str, dict[str, Any]],
    profile_id: str,
    advertiser_id: str | None = None,
    campaign_id: str | None = None,
    tool_context: ToolContext | None = None,
) -> None:
    """Fetches existing CM360 creatives and updates ad creative assignments with real IDs.

    Args:
        ads: Grouped ads mapping keyed by ad name.
        profile_id: CM360 user profile ID.
        advertiser_id: Optional advertiser ID filter.
        campaign_id: Optional campaign ID filter.
        tool_context: Optional ADK ToolContext.

    Raises:
        ValueError: If fetching creatives fails or an assigned creative is not found in CM360.
    """
    logger.info(
        "Fetching existing creatives from CM360 for Advertiser ID: %s, Campaign ID: %s",
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
        raise ValueError(
            f"Failed to fetch creatives from Campaign Manager 360: {e}"
        ) from e

    creative_name_to_id: dict[str, str] = {}
    for c in existing_creatives:
        name = c.get("name")
        real_id = c.get("id")
        if name and real_id:
            creative_name_to_id[name.strip()] = str(real_id)

    for ad_payload in ads.values():
        if ad_payload.get("type") == "AD_SERVING_CLICK_TRACKER":
            continue

        for assignment in ad_payload.get("creativeRotation", {}).get(
            "creativeAssignments", []
        ):
            name = assignment.get("creativeId")
            if name in creative_name_to_id:
                assignment["creativeId"] = creative_name_to_id[name]
            else:
                raise ValueError(
                    f"Creative '{name}' not found in Campaign Manager 360."
                )


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


def _resolve_and_build_operations(
    ads: dict[str, dict[str, Any]],
    event_tags: dict[str, dict[str, Any]],
    profile_id: str,
    advertiser_id: str | None = None,
    campaign_id: str | None = None,
    tool_context: ToolContext | None = None,
) -> list[dict[str, Any]]:
    """Fetches live existing event tags and ads, reconciles them, and builds the operations payload.

    Args:
        ads: Grouped ads mapping keyed by ad name.
        event_tags: Grouped event tags mapping keyed by tag name.
        profile_id: CM360 user profile ID.
        advertiser_id: Optional advertiser ID.
        campaign_id: Optional campaign ID.
        tool_context: Optional ADK ToolContext.

    Returns:
        A list of operation dictionaries (event tags first, then ads) with resolved
        operation types ('dfareporting.*.insert' vs 'dfareporting.*.update') and existing IDs.
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

    existing_event_tags_by_name = {
        str(t.get("name")).strip(): str(t.get("id"))
        for t in existing_tags_list
        if t.get("name") and t.get("id")
    }
    existing_ads_by_name = {
        str(a.get("name")).strip(): a
        for a in existing_ads_list
        if a.get("name") and a.get("id")
    }

    operations: list[dict[str, Any]] = []

    # 1. Build Event Tag operations
    if event_tags:
        for tag_name, raw_payload in event_tags.items():
            tag_payload = dict(raw_payload)
            clean_tag_name = str(tag_name).strip()
            if clean_tag_name in existing_event_tags_by_name:
                tag_id = existing_event_tags_by_name[clean_tag_name]
                tag_payload["id"] = tag_id
                operations.append(
                    {
                        "operation": "dfareporting.eventTags.update",
                        "name": clean_tag_name,
                        "payload": tag_payload,
                    }
                )
            else:
                operations.append(
                    {
                        "operation": "dfareporting.eventTags.insert",
                        "name": clean_tag_name,
                        "payload": tag_payload,
                    }
                )

    # 2. Resolve existing Event Tag IDs in Ad overrides
    for ad_payload in ads.values():
        for override in ad_payload.get("eventTagOverrides", []):
            target_tag = str(override.get("id", "")).strip()
            if target_tag in existing_event_tags_by_name:
                override["id"] = existing_event_tags_by_name[target_tag]

    # 3. Build Ad operations & reconcile placements for existing ads
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
                operations.append(
                    {
                        "operation": "dfareporting.ads.update",
                        "name": clean_ad_name,
                        "payload": ad_payload,
                    }
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


def _execute_event_tag_operation(
    cm360_service: Any,
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

    logger.info("Executing %s for %s", operation_type, tag_name)
    if operation_type == "dfareporting.eventTags.insert":
        request_call = cm360_service.eventTags().insert(
            profileId=profile_id, body=payload
        )
        res = request_call.execute()
    elif operation_type == "dfareporting.eventTags.update":
        request_call = cm360_service.eventTags().update(
            profileId=profile_id, body=payload
        )
        res = request_call.execute()
    else:
        raise ValueError(f"Unsupported event tag operation: {operation_type}")

    real_id = str(res.get("id", payload.get("id", "")))
    event_tag_mappings[tag_name] = real_id

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
    cm360_service: Any,
    profile_id: str,
    event_tag_ops: list[dict[str, Any]],
) -> tuple[list[dict[str, Any]], dict[str, str], set[str]]:
    """Processes all event tag operations and tracks any failures.

    Args:
        cm360_service: Google API Client dfareporting service.
        profile_id: CM360 user profile ID.
        event_tag_ops: List of event tag operations.

    Returns:
        A tuple of (results_list, created_event_tag_mappings, failed_event_tag_names).
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
        existing_event_tag_mappings: Mapping of resolved event tag names to real IDs.
        failed_event_tag_names: Set of event tag names that failed during processing.

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
    """Merges existing placement assignments with any new placements from payload.

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
            "Ad exists but is missing placements. Assigning new placements for %s (ID: %s).",
            ad_name,
            ad_id,
        )


def _execute_ad_operation(
    cm360_service: Any,
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
            "Skipping ad '%s' because associated event tag(s) failed or are missing: %s",
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

    logger.info("Executing %s for %s", operation_type, ad_name)
    try:
        if operation_type == "dfareporting.ads.insert":
            request_call = cm360_service.ads().insert(
                profileId=profile_id, body=payload
            )
            res = request_call.execute()
        elif operation_type == "dfareporting.ads.update":
            request_call = cm360_service.ads().update(
                profileId=profile_id, body=payload
            )
            res = request_call.execute()
        else:
            raise ValueError(f"Unsupported ad operation: {operation_type}")

        ad_id = res.get("id", payload.get("id"))
        return {
            "id": ad_id,
            "name": ad_name,
            "operation": operation_type,
            "status": "SUCCESS",
            "message": f"Ad operation {operation_type} executed successfully with ID: {ad_id}.",
        }
    except Exception as api_err:
        logger.exception(
            "Failed to execute ad operation %s for %s",
            operation_type,
            ad_name,
        )
        return {
            "name": ad_name,
            "operation": operation_type,
            "status": "ERROR",
            "message": str(api_err),
        }


def _process_ad_operations(
    cm360_service: Any,
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


def _build_trafficking_summary_response(
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
            f" operation(s): {success_count} processed, {skipped_count} skipped."
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


async def _update_trafficking_sheet_status(
    tool_context: ToolContext,
    results: list[dict[str, Any]],
) -> None:
    """Updates the trafficking sheet artifact in session with 'Trafficked' status for successful ads.

    Locates the latest trafficking CSV artifact, identifies rows matching successfully
    trafficked ads (matching on Placement Name + Ad Name), updates their Trafficking Status
    to 'Trafficked', and saves the updated file as a new artifact version in the session.

    Args:
        tool_context: Active tool context containing session artifacts.
        results: List of execution result dictionaries from CM360 trafficking push.
    """
    successful_ads = {
        str(r.get("name")).strip()
        for r in results
        if r.get("status") == "SUCCESS"
        and str(r.get("operation", "")).startswith("dfareporting.ads.")
    }
    if not successful_ads:
        logger.info(
            "ℹ️ [traffic_campaigns_in_cm360_tool] - No successful ad operations to update in trafficking sheet."
        )
        return

    df_raw, target_filename = await load_raw_dataframe(tool_context)

    header_row_idx = None
    for idx, row in df_raw.iterrows():
        if idx < 2:  # Skip global metadata rows
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
            "⚠️ [traffic_campaigns_in_cm360_tool] - Could not locate header row in %s to update status.",
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
            "⚠️ [traffic_campaigns_in_cm360_tool] - Required columns ('Trafficking Status', 'Placement Name', or 'Ad Name') not found in %s.",
            target_filename,
        )
        return

    updated_count = 0
    for row_idx in range(header_row_idx + 1, len(df_raw)):
        ad_name_val = df_raw.iat[row_idx, ad_name_col_idx]
        placement_val = df_raw.iat[row_idx, placement_name_col_idx]

        if pd.isna(ad_name_val) or pd.isna(placement_val):
            continue

        ad_name = str(ad_name_val).strip()
        placement_name = str(placement_val).strip()

        if not ad_name or not placement_name:
            continue

        if ad_name in successful_ads:
            df_raw.iat[row_idx, status_col_idx] = "Trafficked"
            updated_count += 1

    csv_buffer = io.StringIO()
    df_raw.to_csv(csv_buffer, index=False, header=False)
    updated_csv_content = csv_buffer.getvalue()

    artifact = types.Part(text=updated_csv_content)
    await tool_context.save_artifact(filename=target_filename, artifact=artifact)
    logger.info(
        "📄 [traffic_campaigns_in_cm360_tool] - Successfully updated %d row(s) to 'Trafficked' in artifact '%s'.",
        updated_count,
        target_filename,
    )
