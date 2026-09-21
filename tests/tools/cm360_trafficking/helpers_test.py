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
"""Tests for the helpers module."""

from unittest import mock

from adspace_agent.tools.cm360_trafficking import helpers


def test_resolve_placement_ids() -> None:
    """Tests resolving placement names to CM360 IDs."""
    ads = {
        "Test Ad": {
            "name": "Test Ad",
            "placementAssignments": [
                {"placementId": "Test~Placement~1 Test", "active": True}
            ],
        }
    }
    with mock.patch(
        "adspace_agent.tools.cm360_trafficking.helpers.list_cm_placements",
        return_value=[
            {
                "name": "Test~Placement~1 Test",
                "id": "mock_placement_id_Test~Placement~1 Test",
            }
        ],
    ):
        helpers._resolve_placement_ids(  # ruff: ignore[private-member-access]
            ads=ads,
            profile_id="7023449",
            advertiser_id="13641571",
            campaign_id="30535365",
            tool_context=None,
        )
    assert (
        ads["Test Ad"]["placementAssignments"][0]["placementId"]
        == "mock_placement_id_Test~Placement~1 Test"
    )


def test_resolve_creative_ids() -> None:
    """Tests resolving creative names to CM360 IDs."""
    ads = {
        "Test Ad": {
            "name": "Test Ad",
            "creativeRotation": {
                "creativeAssignments": [
                    {"creativeId": "TEST_ACG~300x250", "active": True}
                ]
            },
        }
    }
    with mock.patch(
        "adspace_agent.tools.cm360_trafficking.helpers.list_cm_creatives",
        return_value=[{"name": "TEST_ACG~300x250", "id": "999123"}],
    ):
        helpers._resolve_creative_ids(  # ruff: ignore[private-member-access]
            ads=ads,
            profile_id="7023449",
            advertiser_id="13641571",
            campaign_id="30535365",
            tool_context=None,
        )
    assert (
        ads["Test Ad"]["creativeRotation"]["creativeAssignments"][0][
            "creativeId"
        ]
        == "999123"
    )


def test_resolve_and_build_operations_with_existing_entities() -> None:
    """Tests resolving update vs insert operations and existing IDs."""
    ads = {
        "Existing Ad": {
            "name": "Existing Ad",
            "type": "AD_SERVING_STANDARD_AD",
            "placementAssignments": [{"placementId": "mock_placement_2"}],
            "eventTagOverrides": [{"id": "Existing Tag"}, {"id": "New Tag"}],
        },
        "New Ad": {
            "name": "New Ad",
            "type": "AD_SERVING_STANDARD_AD",
            "placementAssignments": [{"placementId": "mock_placement_1"}],
            "eventTagOverrides": [],
        },
    }
    event_tags = {
        "Existing Tag": {
            "name": "Existing Tag",
            "url": "https://example.com/tag",
        },
        "New Tag": {
            "name": "New Tag",
            "url": "https://example.com/new_tag",
        },
    }

    mock_existing_tags = [{"name": "Existing Tag", "id": "tag_999"}]
    mock_existing_ads = [
        {
            "name": "Existing Ad",
            "id": "ad_888",
            "placementAssignments": [{"placementId": "mock_placement_1"}],
        }
    ]

    with (
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.helpers.list_cm_event_tags",
            return_value=mock_existing_tags,
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.helpers.list_cm_ads",
            return_value=mock_existing_ads,
        ),
    ):
        ops = helpers._resolve_and_build_operations(  # ruff: ignore[private-member-access]
            ads=ads,
            event_tags=event_tags,
            profile_id="7023449",
            advertiser_id="13641571",
            campaign_id="30535365",
            tool_context=None,
        )

    expected_ops_count = 4
    assert len(ops) == expected_ops_count

    existing_tag_op = next(op for op in ops if op["name"] == "Existing Tag")
    assert existing_tag_op["operation"] == "dfareporting.eventTags.patch"
    assert existing_tag_op["id"] == "tag_999"
    assert "url" in existing_tag_op["diff_fields"]
    assert existing_tag_op["payload"]["url"] == "https://example.com/tag"

    new_tag_op = next(op for op in ops if op["name"] == "New Tag")
    assert new_tag_op["operation"] == "dfareporting.eventTags.insert"
    assert "id" not in new_tag_op["payload"]

    existing_ad_op = next(op for op in ops if op["name"] == "Existing Ad")
    assert existing_ad_op["operation"] == "dfareporting.ads.patch"
    assert existing_ad_op["id"] == "ad_888"
    placement_ids = [
        p["placementId"]
        for p in existing_ad_op["payload"]["placementAssignments"]
    ]
    assert "mock_placement_1" in placement_ids
    assert "mock_placement_2" in placement_ids
    overrides = existing_ad_op["payload"]["eventTagOverrides"]
    assert overrides[0]["id"] == "tag_999"
    assert overrides[1]["id"] == "New Tag"

    new_ad_op = next(op for op in ops if op["name"] == "New Ad")
    assert new_ad_op["operation"] == "dfareporting.ads.insert"
    assert "id" not in new_ad_op["payload"]


def test_extract_assigned_placement_ids() -> None:
    """Tests _extract_assigned_placement_ids with dict input."""
    ads = {
        "Ad 1": {
            "placementAssignments": [
                {"placementId": "p1"},
                {"placementId": "p2"},
            ]
        },
        "Ad 2": {
            "placementAssignments": [
                {"placementId": "p2"},
                {"placementId": "p3"},
            ]
        },
    }
    extracted_dict = helpers._extract_assigned_placement_ids(  # ruff: ignore[private-member-access]
        ads
    )
    assert extracted_dict == ["p1", "p2", "p3"]


def test_diff_placement_never_diffs_name() -> None:
    """Tests _diff_placement never includes name in diff_fields or payload."""
    sheet_placement = {
        "name": "New Placement Name In Sheet",
        "activeStatus": "PLACEMENT_STATUS_ACTIVE",
        "size": {"width": 300, "height": 250},
        "pricingSchedule": {
            "startDate": "2026-06-01",
            "endDate": "2026-06-30",
        },
    }
    cm_placement = {
        "name": "Old Placement Name In CM360",
        "activeStatus": "PLACEMENT_STATUS_ACTIVE",
        "size": {"width": 300, "height": 250},
        "pricingSchedule": {
            "startDate": "2026-06-01",
            "endDate": "2026-06-30",
        },
    }
    patch_payload, diff_fields = helpers._diff_placement(  # ruff: ignore[private-member-access]
        sheet_placement=sheet_placement,
        cm_placement=cm_placement,
    )
    assert "name" not in diff_fields
    assert "name" not in patch_payload
    assert diff_fields == []
    assert patch_payload == {}


def test_diff_placement_diffs_active_status() -> None:
    """Tests _diff_placement diffs activeStatus directly with enums."""
    sheet_placement = {
        "name": "Placement Inactive in Sheet",
        "activeStatus": "PLACEMENT_STATUS_INACTIVE",
    }
    cm_placement = {
        "name": "Placement Inactive in Sheet",
        "activeStatus": "PLACEMENT_STATUS_ACTIVE",
    }
    patch_payload, diff_fields = helpers._diff_placement(  # ruff: ignore[private-member-access]
        sheet_placement=sheet_placement,
        cm_placement=cm_placement,
    )
    assert "activeStatus" in diff_fields
    assert patch_payload["activeStatus"] == "PLACEMENT_STATUS_INACTIVE"


def test_diff_creative_never_diffs_name() -> None:
    """Tests _diff_creative never includes name in diff_fields or payload."""
    sheet_creative = {
        "name": "Creative Renamed in Sheet",
        "size": {"width": 300, "height": 250},
    }
    cm_creative = {
        "name": "Original Creative Name",
        "size": {"width": 300, "height": 250},
    }
    patch_payload, diff_fields = helpers._diff_creative(  # ruff: ignore[private-member-access]
        sheet_creative=sheet_creative,
        cm_creative=cm_creative,
    )
    assert "name" not in diff_fields
    assert "name" not in patch_payload
    assert diff_fields == []
    assert patch_payload == {}


def test_diff_ad_never_diffs_name() -> None:
    """Tests _diff_ad never includes name in diff_fields or payload."""
    sheet_ad = {
        "name": "Sheet Renamed Ad",
        "startTime": "2026-06-01T00:00:00Z",
        "endTime": "2026-06-30T00:00:00Z",
        "placementAssignments": [{"placementId": "p1"}],
        "eventTagOverrides": [],
    }
    cm_ad = {
        "name": "CM360 Ad Name",
        "startTime": "2026-06-01T00:00:00Z",
        "endTime": "2026-06-30T00:00:00Z",
        "placementAssignments": [{"placementId": "p1"}],
        "eventTagOverrides": [],
    }
    patch_payload, diff_fields = helpers._diff_ad(  # ruff: ignore[private-member-access]
        sheet_ad=sheet_ad,
        cm_ad=cm_ad,
    )
    assert "name" not in diff_fields
    assert "name" not in patch_payload
    assert diff_fields == []
    assert patch_payload == {}


def test_diff_event_tag_never_diffs_name() -> None:
    """Tests _diff_event_tag compares type/url/status and never diffs name."""
    sheet_tag = {
        "name": "Sheet Event Tag Name",
        "type": "IMPRESSION_IMAGE_EVENT_TAG",
        "url": "https://example.com/tag",
        "status": "ENABLED",
    }
    cm_tag_identical = {
        "name": "Different Name In CM360",
        "type": "IMPRESSION_IMAGE_EVENT_TAG",
        "url": "https://example.com/tag",
        "status": "ENABLED",
    }
    patch_payload, diff_fields = helpers._diff_event_tag(  # ruff: ignore[private-member-access]
        sheet_event_tag=sheet_tag,
        cm_event_tag=cm_tag_identical,
    )
    assert "name" not in diff_fields
    assert "name" not in patch_payload
    assert diff_fields == []
    assert patch_payload == {}

    cm_tag_diff = {
        "name": "Different Name In CM360",
        "type": "CLICK_THROUGH_EVENT_TAG",
        "url": "https://example.com/old_url",
        "status": "DISABLED",
    }
    patch_payload, diff_fields = helpers._diff_event_tag(  # ruff: ignore[private-member-access]
        sheet_event_tag=sheet_tag,
        cm_event_tag=cm_tag_diff,
    )
    assert "name" not in diff_fields
    assert "name" not in patch_payload
    assert "type" in diff_fields
    assert "url" in diff_fields
    assert "status" in diff_fields
    assert patch_payload["type"] == "IMPRESSION_IMAGE_EVENT_TAG"
    assert patch_payload["url"] == "https://example.com/tag"
    assert patch_payload["status"] == "ENABLED"


def test_resolve_and_build_operations_skips_identical_event_tags() -> None:
    """Tests _resolve_and_build_operations skips when tag is unchanged."""
    event_tags = {
        "Sync Tag": {
            "name": "Sync Tag",
            "type": "IMPRESSION_IMAGE_EVENT_TAG",
            "url": "https://example.com/tag",
            "status": "ENABLED",
        },
    }
    mock_existing_tags = [
        {
            "name": "Sync Tag",
            "id": "tag_123",
            "type": "IMPRESSION_IMAGE_EVENT_TAG",
            "url": "https://example.com/tag",
            "status": "ENABLED",
        }
    ]
    with (
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.helpers.list_cm_event_tags",
            return_value=mock_existing_tags,
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.helpers.list_cm_ads",
            return_value=[],
        ),
    ):
        ops = helpers._resolve_and_build_operations(  # ruff: ignore[private-member-access]
            ads={},
            event_tags=event_tags,
            profile_id="7023449",
            advertiser_id="13641571",
            campaign_id="30535365",
            tool_context=None,
        )
    assert len(ops) == 0


def test_execute_event_tag_patch_operation() -> None:
    """Tests _execute_event_tag_operation executes patch call."""
    mock_service = mock.MagicMock()
    mock_patch_call = mock.MagicMock()
    mock_patch_call.execute.return_value = {
        "id": "tag_123",
        "name": "Test Tag",
    }
    mock_service.eventTags.return_value.patch.return_value = mock_patch_call

    mappings: dict[str, str] = {}
    op = {
        "operation": "dfareporting.eventTags.patch",
        "id": "tag_123",
        "name": "Test Tag",
        "payload": {"url": "https://example.com/new"},
    }
    result = helpers._execute_event_tag_operation(  # ruff: ignore[private-member-access]
        cm360_service=mock_service,
        profile_id="12345",
        op=op,
        event_tag_mappings=mappings,
    )
    assert result["status"] == "SUCCESS"
    assert result["id"] == "tag_123"
    assert mappings["Test Tag"] == "tag_123"
    mock_service.eventTags.return_value.patch.assert_called_once_with(
        profileId="12345",
        id="tag_123",
        body={"url": "https://example.com/new"},
    )


def test_diff_placement_aligns_pricing_periods() -> None:
    """Tests _diff_placement aligns pricingPeriods when flight dates change."""
    sheet_placement = {
        "name": "Placement A",
        "pricingSchedule": {
            "startDate": "2026-09-07",
            "endDate": "2026-09-30",
        },
    }
    cm_placement = {
        "name": "Placement A",
        "pricingSchedule": {
            "startDate": "2026-06-10",
            "endDate": "2026-07-10",
            "pricingPeriods": [
                {
                    "startDate": "2026-06-10",
                    "endDate": "2026-07-10",
                    "units": "1000",
                    "rateOrCostNanos": "5000000000",
                }
            ],
        },
    }
    patch_payload, diff_fields = helpers._diff_placement(  # ruff: ignore[private-member-access]
        sheet_placement=sheet_placement,
        cm_placement=cm_placement,
    )
    assert "pricingSchedule.startDate" in diff_fields
    assert "pricingSchedule.endDate" in diff_fields
    assert patch_payload["pricingSchedule"]["startDate"] == "2026-09-07"
    assert patch_payload["pricingSchedule"]["endDate"] == "2026-09-30"
    assert patch_payload["pricingSchedule"]["pricingPeriods"] == [
        {
            "startDate": "2026-09-07",
            "endDate": "2026-09-30",
            "units": "1000",
            "rateOrCostNanos": "5000000000",
        }
    ]
