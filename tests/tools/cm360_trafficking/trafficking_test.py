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
"""Tests for the trafficking module."""

import dataclasses
import json
import typing
from unittest import mock

import anyio
from google.genai import types
import pytest

from adspace_agent.tools.cm360_trafficking import trafficking
from adspace_agent.tools.cm360_trafficking.trafficking import (
    before_traffic_campaigns_in_cm360_tool_callback,
)
from adspace_agent.tools.cm360_trafficking.trafficking import (
    CM360TraffickingParserToolset,
)
from adspace_agent.tools.cm360_trafficking.trafficking import (
    CREDENTIALS_CACHE_KEY,
)
from adspace_agent.tools.cm360_trafficking.trafficking import parse_sheet_tool

parse_trafficking_sheet = parse_sheet_tool

pytestmark = pytest.mark.usefixtures("mock_cm360_api_calls")


@dataclasses.dataclass
class MockSession:
    """Mock session for testing."""

    id: str = "test_session_id"


class MockToolContext:
    """Mock context to simulate ADK Artifacts loader in tests."""

    def __init__(
        self, physical_path: str, session_id: str = "test_session_id"
    ) -> None:
        """Initializes MockToolContext."""
        self.physical_path = physical_path
        self.session = MockSession(session_id)
        self.state: dict[str, typing.Any] = {}

    async def list_artifacts(self) -> list[str]:
        """Lists available artifacts.

        Returns:
            list[str]: The physical paths.
        """
        return [self.physical_path]

    async def load_artifact(self, filename: str) -> types.Part | None:
        """Loads artifact as a Part object.

        Args:
            filename: Artifact filename.

        Returns:
            Part object or None.
        """
        path = anyio.Path(self.physical_path)
        if not await path.exists():
            return None

        if filename.lower().endswith(".csv"):
            text = await path.read_text(encoding="utf-8")
            return types.Part(text=text)

        data = await path.read_bytes()
        return types.Part(
            inline_data=types.Blob(
                mime_type="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                data=data,
            )
        )


def make_test_csv(**overrides: str) -> str:
    """Helper to construct a trafficking sheet CSV string with defaults.

    Args:
        **overrides: Key-value pairs to override defaults.

    Returns:
        str: Generated CSV string.
    """
    defaults = {
        "profile_id": "7023449",
        "advertiser_id": "13641571",
        "campaign_id": "30535365",
        "campaign_name": "Google Ad Spaces Testing - June 2026",
        "status": "New",
        "site_id": "4802860",
        "site": "",
        "placement_id": "",
        "placement_name": "Test~Placement~1 Test",
        "placement_size": "300x250",
        "compatibility": "DISPLAY",
        "payment_source": "PLACEMENT_AGENCY_PAID",
        "pricing_type": "PRICING_TYPE_CPM",
        "pricing_start": "8/29/3000",
        "pricing_end": "9/25/3000",
        "tag_formats": (
            "PLACEMENT_TAG_STANDARD, PLACEMENT_TAG_IFRAME_JAVASCRIPT"
        ),
        "ad_id": "",
        "ad_name": "Test Ad C",
        "ad_start": "8/29/3000",
        "ad_end": "9/25/3000",
        "ad_type": "AD_SERVING_STANDARD_AD",
        "priority": "AD_PRIORITY_01",
        "impression_ratio": "1",
        "creative_id": "999123",
        "creative_name": "TEST_ACG~300x250",
        "click_through_url": "https://click.test.com",
        "ad_dynamic_click_tracker": "TRUE",
        "base_url": "https://www.test.com",
        "final_url": "https://www.test.com?utm_medium=test",
        "placement_status": "PLACEMENT_STATUS_ACTIVE",
    }
    defaults.update(overrides)

    r1 = (
        "Digitas-DFA 609,Profile ID,Advertiser ID,Campaign ID,Campaign Name"
        + "," * 41
        + "\n"
    )
    r2 = (
        f",{defaults['profile_id']},{defaults['advertiser_id']},"
        f"{defaults['campaign_id']},{defaults['campaign_name']}"
        + "," * 41
        + "\n"
    )
    r3 = (
        "," * 9
        + "Placement Type Not in API - renamed to Compatibility"
        + "," * 36
        + "\n"
    )
    r4 = "," * 45 + "\n"
    r5 = (
        "Campaign ID,Campaign Name,Trafficking Status,Site ID,Site,Channel,"
        "Placement ID,Placement Name,Placement Size,Compatibility,"
        "Placement Status,Placement Payment Source,Pricing Schedule Type,"
        "Pricing Schedule Start Date,Pricing Schedule End Date,"
        "Placement Tag Formats,Ad Server,Ad ID,Ad Name,Ad Start Date,"
        "Ad End Date,Ad Type,Delivery Schedule Priority,"
        "Delivery Schedule Impression Ratio,Ad Click Through Url,"
        "Ad Dynamic Click Tracker,Creative ID,Creative Name,"
        "Creative Start Date,Creative End Date,Creative Rotation,"
        "Creative Dimensions,Creative Type,"
        "Creative asset file link,Base URL,Final Trafficking URL,"
        "Brand Safety/Verification,Brand Safety/Verification Measurement Type,"
        "Research Partner,Notes,Campaign Funding,Campaign Quarter,"
        "Creative Detail,Fiscal,Language,Salesforce ID\n"
    )
    r6 = (
        f"{defaults['campaign_id']},{defaults['campaign_name']},"
        f"{defaults['status']},{defaults['site_id']},{defaults['site']},"
        f"display,{defaults['placement_id']},{defaults['placement_name']},"
        f"{defaults['placement_size']},{defaults['compatibility']},"
        f"{defaults['placement_status']},{defaults['payment_source']},"
        f"{defaults['pricing_type']},{defaults['pricing_start']},"
        f'{defaults["pricing_end"]},"{defaults["tag_formats"]}",DCM,'
        f"{defaults['ad_id']},{defaults['ad_name']},{defaults['ad_start']},"
        f"{defaults['ad_end']},{defaults['ad_type']},{defaults['priority']},"
        f"{defaults['impression_ratio']},{defaults['click_through_url']},"
        f"{defaults['ad_dynamic_click_tracker']},{defaults['creative_id']},"
        f"{defaults['creative_name']},"
        "6/10/2026,7/10/2026,100%,300x250,HTML5_BANNER,,"
        f"{defaults['base_url']},{defaults['final_url']},"
        "None,None,CINT,,,,,,,\n"
    )
    return r1 + r2 + r3 + r4 + r5 + r6


@pytest.fixture
def mock_cm360_api_calls(monkeypatch):
    """Mock CM360 API calls to return matching mock data."""
    monkeypatch.setenv("SKILLS_BUCKET_NAME", "test-bucket")

    def mock_list_placements_side_effect(*_args, **_kwargs):
        names = [
            "Test~Placement~1 Test",
            "Test~Placement~2 Test",
            "Test~Placement~3 Test",
            "Test~Placement~4 Test",
            "Test~Placement~5 Test",
            "Summer Placement",
            "Test~Placement~1",
            "Test~Placement~2",
            "Test~Placement~3",
            "Test~Placement~4",
            "Test~Placement~5",
        ]
        return [{"name": n, "id": f"mock_placement_id_{n}"} for n in names]

    def mock_list_creatives_side_effect(*_args, **_kwargs):
        return [
            {"name": "TEST_ACG~300x250", "id": "999123"},
            {"name": "TEST_Shopathon_300x250", "id": "999456"},
            {"name": "sap_elephant", "id": "999789"},
        ]

    def mock_list_event_tags_side_effect(*_args, **_kwargs):
        return []

    def mock_list_ads_side_effect(*_args, **_kwargs):
        return []

    with (
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.helpers.list_cm_placements",
            side_effect=mock_list_placements_side_effect,
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.helpers.list_cm_creatives",
            side_effect=mock_list_creatives_side_effect,
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.helpers.list_cm_event_tags",
            side_effect=mock_list_event_tags_side_effect,
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.helpers.list_cm_ads",
            side_effect=mock_list_ads_side_effect,
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.trafficking.upload_to_gcs",
            return_value="gs://test-bucket/cm360_trafficking/test_session_id/payloads.json",
        ),
    ):
        yield


@pytest.mark.asyncio
async def test_parse_trafficking_sheet_success(tmp_path) -> None:
    """Tests parsing a valid trafficking sheet."""
    csv_content = make_test_csv()
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") == "SUCCESS"
    assert "operations" in result
    assert len(result["operations"]) > 0

    op_types = [op["operation"] for op in result["operations"]]
    assert "dfareporting.creatives.insert" not in op_types
    assert "dfareporting.ads.insert" in op_types

    ad_ops = [
        op
        for op in result["operations"]
        if op["operation"] == "dfareporting.ads.insert"
    ]
    assert len(ad_ops) > 0

    ad_c = next(
        (
            op["payload"]
            for op in ad_ops
            if op["payload"]["name"] == "Test Ad C"
        ),
        None,
    )
    assert ad_c is not None
    rotation = ad_c["creativeRotation"]
    assignments = rotation["creativeAssignments"]
    assert len(assignments) == 1
    assert assignments[0]["creativeId"] == "999123"
    assert (
        assignments[0]["clickThroughUrl"]["customClickThroughUrl"]
        == "https://www.test.com?utm_medium=test"
    )


@pytest.mark.asyncio
async def test_parse_trafficking_sheet_file_not_found() -> None:
    """Tests parsing when the file does not exist."""
    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext("non_existent_file.xlsx"))
    )
    result = json.loads(result_str)

    assert result.get("status") in {"error", "ERROR"}


@pytest.mark.asyncio
async def test_cm360_trafficking_parser_toolset() -> None:
    """Tests that the parser toolset returns the correct tools."""
    toolset = CM360TraffickingParserToolset()
    tools = await toolset.get_tools()
    expected_tool_count = 2
    assert len(tools) == expected_tool_count
    assert tools[0] is toolset.parse_sheet_tool
    assert tools[1] is toolset.traffic_campaigns_in_cm360_tool


@pytest.mark.asyncio
async def test_parse_trafficking_sheet_success_missing_campaign_name(
    tmp_path,
) -> None:
    """Tests that parsing succeeds and Campaign Name is treated as optional."""
    csv_content = make_test_csv(campaign_name="")
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") == "SUCCESS"
    assert len(result["operations"]) > 0


@pytest.mark.asyncio
async def test_parse_trafficking_sheet_success_new_headers(tmp_path) -> None:
    """Tests that parsing succeeds when using the new schedule header names."""
    csv_content = make_test_csv()
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") == "SUCCESS"
    assert len(result["operations"]) > 0

    ad_ops = [
        op
        for op in result["operations"]
        if op["operation"] == "dfareporting.ads.insert"
    ]
    assert len(ad_ops) == 1
    schedule = ad_ops[0]["payload"]["deliverySchedule"]
    assert schedule["priority"] == "AD_PRIORITY_01"
    assert schedule["impressionRatio"] == "1"
    placement_assignments = ad_ops[0]["payload"]["placementAssignments"]
    assert len(placement_assignments) == 1
    assert placement_assignments[0]["active"] is True


def test_before_callback_valid_cached_credentials() -> None:
    """Tests before callback when valid credentials already exist in cache."""
    mock_tool = mock.MagicMock()
    mock_tool.name = "traffic_campaigns_in_cm360_tool"
    mock_context = mock.MagicMock()
    mock_creds_info = {
        "token": "ya29.valid_token",
        "refresh_token": "mock_refresh",
        "client_id": "mock_client",
        "client_secret": "mock_secret",
        "token_uri": "https://oauth2.googleapis.com/token",
    }
    mock_context.state = {CREDENTIALS_CACHE_KEY: mock_creds_info}

    mock_credentials_obj = mock.MagicMock()
    mock_credentials_obj.valid = True
    mock_credentials_obj.to_json.return_value = json.dumps(mock_creds_info)

    with mock.patch(
        "adspace_agent.tools.cm360_trafficking.trafficking.Credentials.from_authorized_user_info",
        return_value=mock_credentials_obj,
    ):
        res = before_traffic_campaigns_in_cm360_tool_callback(
            tool=mock_tool, args={}, tool_context=mock_context
        )
        assert res is None


def test_before_callback_refresh_expired_credentials() -> None:
    """Tests before callback when cached credentials are refreshed."""
    mock_tool = mock.MagicMock()
    mock_tool.name = "parse_sheet_tool"
    mock_context = mock.MagicMock()
    mock_creds_info = {
        "token": "ya29.expired_token",
        "refresh_token": "mock_refresh",
    }
    mock_context.state = {CREDENTIALS_CACHE_KEY: mock_creds_info}

    mock_credentials_obj = mock.MagicMock()
    mock_credentials_obj.valid = False
    mock_credentials_obj.expired = True
    mock_credentials_obj.refresh_token = "mock_refresh"  # ruff: ignore[hardcoded-password-string]
    mock_credentials_obj.to_json.return_value = json.dumps({
        "token": "ya29.refreshed_token"
    })

    def on_refresh(*_args, **_kwargs):
        mock_credentials_obj.valid = True

    mock_credentials_obj.refresh.side_effect = on_refresh

    with mock.patch(
        "adspace_agent.tools.cm360_trafficking.trafficking.Credentials.from_authorized_user_info",
        return_value=mock_credentials_obj,
    ):
        res = before_traffic_campaigns_in_cm360_tool_callback(
            tool=mock_tool, args={}, tool_context=mock_context
        )
        assert res is None
        mock_credentials_obj.refresh.assert_called_once()
        assert mock_context.state[CREDENTIALS_CACHE_KEY] == {
            "token": "ya29.refreshed_token"
        }


def test_before_callback_exchanged_credentials() -> None:
    """Tests before callback when credentials are exchanged successfully."""
    mock_tool = mock.MagicMock()
    mock_tool.name = "traffic_campaigns_in_cm360_tool"
    mock_context = mock.MagicMock()
    mock_context.state = {}

    mock_oauth = mock.MagicMock()
    mock_oauth.access_token = "ya29.exchanged_token"  # ruff: ignore[hardcoded-password-string]
    mock_oauth.refresh_token = "mock_exchanged_refresh"  # ruff: ignore[hardcoded-password-string]

    mock_auth_response = mock.MagicMock()
    mock_auth_response.oauth2 = mock_oauth
    mock_context.get_auth_response.return_value = mock_auth_response

    res = before_traffic_campaigns_in_cm360_tool_callback(
        tool=mock_tool, args={}, tool_context=mock_context
    )
    assert res is None
    assert (
        mock_context.state[CREDENTIALS_CACHE_KEY]["token"]
        == "ya29.exchanged_token"  # ruff: ignore[hardcoded-password-string]
    )
    assert (
        mock_context.state[CREDENTIALS_CACHE_KEY]["refresh_token"]
        == "mock_exchanged_refresh"  # ruff: ignore[hardcoded-password-string]
    )


def test_before_callback_request_credential() -> None:
    """Tests before callback when user authentication is required."""
    mock_tool = mock.MagicMock()
    mock_tool.name = "traffic_campaigns_in_cm360_tool"
    mock_context = mock.MagicMock()
    mock_context.state = {}
    mock_context.get_auth_response.return_value = None

    res = before_traffic_campaigns_in_cm360_tool_callback(
        tool=mock_tool, args={}, tool_context=mock_context
    )
    assert res == {"pending": True, "message": "Awaiting user authentication."}
    mock_context.request_credential.assert_called_once()


def test_before_callback_unrelated_tool() -> None:
    """Tests before callback ignores unrelated tools."""
    mock_tool = mock.MagicMock()
    mock_tool.name = "unrelated_tool"
    mock_context = mock.MagicMock()

    res = before_traffic_campaigns_in_cm360_tool_callback(
        tool=mock_tool, args={}, tool_context=mock_context
    )
    assert res is None


@pytest.mark.asyncio
async def test_traffic_campaigns_in_cm360_tool_execution(monkeypatch) -> None:
    """Tests traffic_campaigns_in_cm360_tool error paths and execution flow."""
    res_no_ctx = json.loads(
        await trafficking.traffic_campaigns_in_cm360_tool(
            typing.cast("typing.Any", None)
        )
    )
    assert res_no_ctx["status"] == "ERROR"

    ctx = MockToolContext("dummy.csv")
    ctx.state = {"key": "val"}
    ctx.session = MockSession(id="")
    res_no_sess = json.loads(
        await trafficking.traffic_campaigns_in_cm360_tool(
            typing.cast("typing.Any", ctx)
        )
    )
    assert res_no_sess["status"] == "ERROR"

    ctx.session = MockSession(id="sess_1")
    monkeypatch.delenv("SKILLS_BUCKET_NAME", raising=False)
    res_no_bucket = json.loads(
        await trafficking.traffic_campaigns_in_cm360_tool(
            typing.cast("typing.Any", ctx)
        )
    )
    assert res_no_bucket["status"] == "ERROR"

    monkeypatch.setenv("SKILLS_BUCKET_NAME", "test-bucket")
    res_no_path = json.loads(
        await trafficking.traffic_campaigns_in_cm360_tool(
            typing.cast("typing.Any", ctx)
        )
    )
    assert res_no_path["status"] == "ERROR"

    ctx.state["parsed_payload_gcs_url"] = (
        "gs://test-bucket/cm360_trafficking/sess_1/payloads.json"
    )
    payload = {
        "profile_id": "123",
        "advertiser_id": "456",
        "campaign_id": "789",
        "campaign_name": "Camp",
        "operations": [
            {"operation": "dfareporting.placements.patch", "payload": {}},
            {"operation": "dfareporting.creatives.patch", "payload": {}},
            {"operation": "dfareporting.eventTags.insert", "payload": {}},
            {"operation": "dfareporting.ads.insert", "payload": {}},
        ],
    }
    with (
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.trafficking.download_from_gcs",
            return_value=json.dumps(payload),
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.trafficking._get_cm360_service",
            return_value=mock.MagicMock(),
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.trafficking._process_placement_operations",
            return_value=[{"status": "SUCCESS", "entity_type": "Placement"}],
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.trafficking._process_creative_operations",
            return_value=[{"status": "SUCCESS", "entity_type": "Creative"}],
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.trafficking._process_event_tag_operations",
            return_value=(
                [{"status": "SUCCESS", "entity_type": "EventTag"}],
                {},
                set(),
            ),
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.trafficking._process_ad_operations",
            return_value=[{"status": "SUCCESS", "entity_type": "Ad"}],
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.trafficking._update_trafficking_sheet_status",
            new_callable=mock.AsyncMock,
        ),
    ):
        res_ok = json.loads(
            await trafficking.traffic_campaigns_in_cm360_tool(
                typing.cast("typing.Any", ctx)
            )
        )
        assert res_ok["status"] == "SUCCESS"
