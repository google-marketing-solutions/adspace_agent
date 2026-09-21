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
"""Tests for the actions module."""

from unittest import mock

from google.oauth2.credentials import Credentials
import pandas as pd
import pytest

from adspace_agent.tools.cm360_trafficking import actions
from adspace_agent.tools.cm360_trafficking import trafficking
from adspace_agent.tools.cm360_trafficking.trafficking import (
    CREDENTIALS_CACHE_KEY,
)


def test_group_placements_status_mapping() -> None:
    """Tests valid Placement Status maps in grouped placements."""
    df = pd.DataFrame([
        {
            "Placement Name": "Test Placement 1",
            "Placement Status": "PLACEMENT_STATUS_INACTIVE",
            "Compatibility": "DISPLAY",
            "Site ID": "123",
            "Pricing Schedule Type": "PRICING_TYPE_CPM",
            "Pricing Schedule Start Date": "8/29/3000",
            "Pricing Schedule End Date": "9/25/3000",
            "Placement Payment Source": "PLACEMENT_AGENCY_PAID",
            "Placement Tag Formats": "PLACEMENT_TAG_STANDARD",
            "Placement Size": "300x250",
        }
    ])
    placements = actions._group_placements(  # ruff: ignore[private-member-access]
        df, advertiser_id="123", campaign_id="456"
    )
    assert "Test Placement 1" in placements
    assert (
        placements["Test Placement 1"]["activeStatus"]
        == "PLACEMENT_STATUS_INACTIVE"
    )


def test_get_cm360_service_success_from_dict() -> None:
    """Tests _get_cm360_service creates service with credentials dict."""
    mock_context = mock.MagicMock()
    mock_context.state = {
        CREDENTIALS_CACHE_KEY: {
            "token": "ya29.mock_token",
            "refresh_token": "mock_refresh",
            "client_id": "mock_client",
            "client_secret": "mock_secret",
            "token_uri": "https://oauth2.googleapis.com/token",
        }
    }

    with mock.patch(
        "adspace_agent.tools.cm360_trafficking.actions.build"
    ) as mock_build:
        mock_build.return_value = mock.MagicMock()
        service = trafficking._get_cm360_service(  # ruff: ignore[private-member-access]
            mock_context
        )
        assert service == mock_build.return_value
        mock_build.assert_called_once()
        args, kwargs = mock_build.call_args
        assert args == ("dfareporting", "v5")
        assert (
            kwargs["credentials"].token == "ya29.mock_token"  # ruff: ignore[hardcoded-password-string]
        )


def test_get_cm360_service_success_from_credentials_object() -> None:
    """Tests _get_cm360_service uses credentials object present in state."""
    mock_credentials = mock.MagicMock(spec=Credentials)
    mock_context = mock.MagicMock()
    mock_context.state = {CREDENTIALS_CACHE_KEY: mock_credentials}

    with mock.patch(
        "adspace_agent.tools.cm360_trafficking.actions.build"
    ) as mock_build:
        mock_build.return_value = mock.MagicMock()
        service = trafficking._get_cm360_service(  # ruff: ignore[private-member-access]
            mock_context
        )
        assert service == mock_build.return_value
        mock_build.assert_called_once_with(
            "dfareporting", "v5", credentials=mock_credentials
        )


def test_get_cm360_service_missing_tool_context() -> None:
    """Tests _get_cm360_service raises ValueError when tool_context is None."""
    with pytest.raises(
        ValueError,
        match=r"Tool context and state are required to get CM360 service\.",
    ):
        trafficking._get_cm360_service(None)  # ruff: ignore[private-member-access]


def test_get_cm360_service_missing_state() -> None:
    """Tests _get_cm360_service raises ValueError when state is None."""
    mock_context = mock.MagicMock()
    mock_context.state = None
    with pytest.raises(
        ValueError,
        match=r"Tool context and state are required to get CM360 service\.",
    ):
        trafficking._get_cm360_service(mock_context)  # ruff: ignore[private-member-access]


def test_get_cm360_service_missing_credentials_key() -> None:
    """Tests _get_cm360_service raises ValueError when cache key missing."""
    mock_context = mock.MagicMock()
    mock_context.state = {}
    with pytest.raises(
        ValueError, match="Credentials not found in tool context state"
    ):
        trafficking._get_cm360_service(mock_context)  # ruff: ignore[private-member-access]


def test_cm360_actions_grouping_and_list_helpers() -> None:
    """Tests grouping and listing helpers in actions."""
    df = pd.DataFrame([
        {
            "Placement Name": "P1",
            "Site": "SiteA",
            "Placement Start Date": "2026-01-01",
            "Placement End Date": "2026-02-01",
            "Placement Size": "300x250",
            "Placement Type": "DISPLAY",
            "Placement Status": "PLACEMENT_STATUS_ACTIVE",
            "Creative Name": "C1",
            "Creative Dimensions": "300x250",
            "Creative Type": "HTML5_BANNER",
            "Creative Rotation": "100%",
            "Event Tag Names": "ET1",
            "Event Tag Types": "IMPRESSION",
            "Event Tag Urls": "https://et1.com",
            "Event Tag Status": "ENABLED",
        },
        {
            "Placement Name": "P1",
            "Site": "SiteA",
            "Placement Size": "300x250",
            "Creative Name": "",
            "Event Tag Names": "none",
        },
    ])
    placements = actions._group_placements(  # ruff: ignore[private-member-access]
        df, advertiser_id="1", campaign_id="2", campaign_name="C"
    )
    assert "P1" in placements
    creatives = actions._group_creatives(df, advertiser_id="1")  # ruff: ignore[private-member-access]
    assert "C1" in creatives
    event_tags = actions._group_event_tags(  # ruff: ignore[private-member-access]
        df, advertiser_id="1", campaign_id="2"
    )
    assert "ET1" in event_tags

    mock_svc = mock.MagicMock()
    mock_svc.placements().list().execute.return_value = {
        "placements": [{"id": "p1"}]
    }
    mock_svc.creatives().list().execute.return_value = {
        "creatives": [{"id": "c1"}]
    }
    mock_svc.eventTags().list().execute.return_value = {
        "eventTags": [{"id": "et1"}]
    }
    mock_svc.ads().list().execute.return_value = {"ads": [{"id": "a1"}]}

    with mock.patch(
        "adspace_agent.tools.cm360_trafficking.actions._get_cm360_service",
        return_value=mock_svc,
    ):
        assert (
            len(
                actions.list_cm_placements(
                    "123", advertiser_ids=["1"], campaign_ids=["2"]
                )
            )
            == 1
        )
        assert (
            len(
                actions.list_cm_creatives(
                    "123", advertiser_id="1", campaign_id="2"
                )
            )
            == 1
        )
        assert (
            len(
                actions.list_cm_event_tags(
                    "123", advertiser_id="1", campaign_id="2"
                )
            )
            == 1
        )
        assert (
            len(
                actions.list_cm_ads(
                    "123",
                    advertiser_id="1",
                    campaign_ids=["2"],
                    placement_ids=["3"],
                )
            )
            == 1
        )
