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
"""Tests for the CM360 trafficking parser tool."""

import dataclasses
import datetime
import json
import typing
from unittest import mock

import anyio
from google.genai import types
from google.oauth2.credentials import Credentials
import pandas as pd
import pytest

from adspace_agent.tools.cm360_trafficking import cm360_actions
from adspace_agent.tools.cm360_trafficking import cm360_trafficking
from adspace_agent.tools.cm360_trafficking import trafficking_helpers
from adspace_agent.tools.cm360_trafficking import validations
from adspace_agent.tools.cm360_trafficking.cm360_trafficking import (
    before_traffic_campaigns_in_cm360_tool_callback,
)
from adspace_agent.tools.cm360_trafficking.cm360_trafficking import (
    CM360TraffickingParserToolset,
)
from adspace_agent.tools.cm360_trafficking.cm360_trafficking import (
    CREDENTIALS_CACHE_KEY,
)
from adspace_agent.tools.cm360_trafficking.cm360_trafficking import (
    parse_sheet_tool,
)

parse_trafficking_sheet = parse_sheet_tool


@dataclasses.dataclass
class MockSession:
    """Mock session for testing."""

    id: str = "test_session_id"


@pytest.fixture(autouse=True)
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
            "adspace_agent.tools.cm360_trafficking.trafficking_helpers.list_cm_placements",
            side_effect=mock_list_placements_side_effect,
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.trafficking_helpers.list_cm_creatives",
            side_effect=mock_list_creatives_side_effect,
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.trafficking_helpers.list_cm_event_tags",
            side_effect=mock_list_event_tags_side_effect,
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.trafficking_helpers.list_cm_ads",
            side_effect=mock_list_ads_side_effect,
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.cm360_trafficking.upload_to_gcs",
            return_value="gs://test-bucket/cm360_trafficking/test_session_id/payloads.json",
        ),
    ):
        yield


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


def test_validate_placement_name() -> None:
    """Tests validations.validate_placement_name helper."""
    row_missing = pd.Series({"Trafficking Status": "New", "Placement Name": ""})
    err = validations.validate_placement_name(row_missing, 1)
    assert err is not None
    assert err["field"] == "Placement Name"
    assert "Placement Name is required" in err["error"]

    row_long = pd.Series({
        "Trafficking Status": "New",
        "Placement Name": "A" * 513,
    })
    err = validations.validate_placement_name(row_long, 1)
    assert err is not None
    assert "must be less than or equal to 512 characters" in err["error"]

    row_valid = pd.Series({
        "Trafficking Status": "New",
        "Placement Name": "Valid Name",
    })
    assert validations.validate_placement_name(row_valid, 1) is None


@pytest.mark.asyncio
@pytest.mark.parametrize(
    ("field", "override"),
    [
        ("Profile ID", {"profile_id": ""}),
        ("Advertiser ID", {"advertiser_id": ""}),
        ("Campaign ID", {"campaign_id": ""}),
    ],
)
async def test_validation_fail_missing_required_ids(
    tmp_path, field: str, override: dict[str, str]
) -> None:
    """Tests that validation fails when required campaign IDs are missing."""
    csv_content = make_test_csv(**override)
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") in {"error", "ERROR"}
    assert field in result.get("message", "")


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


def test_validate_placement_id() -> None:
    """Tests validations.validate_placement_id helper."""
    row_update_missing = pd.Series({
        "Trafficking Status": "Update",
        "Placement ID": "",
    })
    assert validations.validate_placement_id(row_update_missing, 1) is not None
    row_update_valid = pd.Series({
        "Trafficking Status": "Update",
        "Placement ID": "12345",
    })
    assert validations.validate_placement_id(row_update_valid, 1) is None
    row_new_missing = pd.Series({
        "Trafficking Status": "New",
        "Placement ID": "",
    })
    assert validations.validate_placement_id(row_new_missing, 1) is None


def test_validate_site_id() -> None:
    """Tests validations.validate_site_id helper."""
    row_missing = pd.Series({"Site ID": ""})
    assert validations.validate_site_id(row_missing, 1) is not None
    row_valid = pd.Series({"Site ID": "12345"})
    assert validations.validate_site_id(row_valid, 1) is None


def test_validate_payment_source() -> None:
    """Tests validations.validate_payment_source helper."""
    row_missing = pd.Series({
        "Trafficking Status": "New",
        "Placement Payment Source": "",
    })
    assert validations.validate_payment_source(row_missing, 1) is not None
    row_valid = pd.Series({
        "Trafficking Status": "New",
        "Placement Payment Source": "PLACEMENT_AGENCY_PAID",
    })
    assert validations.validate_payment_source(row_valid, 1) is None


def test_validate_pricing_schedule() -> None:
    """Tests validations.validate_pricing_schedule helper."""
    row_missing = pd.Series({
        "Trafficking Status": "New",
        "Pricing Schedule Start Date": "",
        "Pricing Schedule End Date": "9/25/3000",
        "Pricing Schedule Type": "PRICING_TYPE_CPM",
    })
    assert validations.validate_pricing_schedule(row_missing, 1) is not None
    row_valid = pd.Series({
        "Trafficking Status": "New",
        "Pricing Schedule Start Date": "8/29/3000",
        "Pricing Schedule End Date": "9/25/3000",
        "Pricing Schedule Type": "PRICING_TYPE_CPM",
    })
    assert validations.validate_pricing_schedule(row_valid, 1) is None


def test_validate_tag_formats() -> None:
    """Tests validations.validate_tag_formats helper."""
    row_missing = pd.Series({
        "Trafficking Status": "New",
        "Placement Tag Formats": "",
    })
    assert validations.validate_tag_formats(row_missing, 1) is not None
    row_invalid = pd.Series({
        "Trafficking Status": "New",
        "Placement Tag Formats": "PLACEMENT_TAG_INVALID",
    })
    assert validations.validate_tag_formats(row_invalid, 1) is not None
    row_valid = pd.Series({
        "Trafficking Status": "New",
        "Placement Tag Formats": "PLACEMENT_TAG_STANDARD",
    })
    assert validations.validate_tag_formats(row_valid, 1) is None


def test_validate_ad_id() -> None:
    """Tests validations.validate_ad_id helper."""
    row_missing = pd.Series({"Trafficking Status": "Update", "Ad ID": ""})
    assert validations.validate_ad_id(row_missing, 1) is not None
    row_valid = pd.Series({"Trafficking Status": "Update", "Ad ID": "12345"})
    assert validations.validate_ad_id(row_valid, 1) is None


def test_validate_compatibility() -> None:
    """Tests validations.validate_compatibility helper."""
    row_missing = pd.Series({"Trafficking Status": "New", "Compatibility": ""})
    err = validations.validate_compatibility(row_missing, 1)
    assert err is not None
    assert "Compatibility is required" in err["error"]

    row_forbidden = pd.Series({
        "Trafficking Status": "New",
        "Compatibility": "APP",
    })
    err = validations.validate_compatibility(row_forbidden, 1)
    assert err is not None
    assert "APP and APP_INTERSTITIAL are no longer allowed" in err["error"]

    row_invalid = pd.Series({
        "Trafficking Status": "New",
        "Compatibility": "INVALID",
    })
    err = validations.validate_compatibility(row_invalid, 1)
    assert err is not None
    assert "Invalid compatibility value" in err["error"]

    row_valid = pd.Series({
        "Trafficking Status": "New",
        "Compatibility": "DISPLAY",
    })
    assert validations.validate_compatibility(row_valid, 1) is None

    row_valid_audio = pd.Series({
        "Trafficking Status": "New",
        "Compatibility": "IN_STREAM_AUDIO",
    })
    assert validations.validate_compatibility(row_valid_audio, 1) is None


def test_validate_placement_size() -> None:
    """Tests validations.validate_placement_size helper."""
    row_missing = pd.Series({
        "Trafficking Status": "New",
        "Placement Size": "",
    })
    err = validations.validate_placement_size(row_missing, 1)
    assert err is not None
    assert "Placement Size is required" in err["error"]

    row_valid = pd.Series({
        "Trafficking Status": "New",
        "Placement Size": "300x250",
    })
    assert validations.validate_placement_size(row_valid, 1) is None


@pytest.mark.asyncio
async def test_validation_fail_missing_ad_name_on_new(
    tmp_path,
) -> None:
    """Tests that validation fails when New Ad Name is missing."""
    csv_content = make_test_csv(ad_name="")
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") == "error"
    assert "Validation failed" in result.get("message", "")
    assert "validation_errors" in result
    errors = result["validation_errors"]
    assert len(errors) == 1
    expected_row = 6
    assert errors[0]["row"] == expected_row
    assert errors[0]["field"] == "Ad Name"
    assert "Ad Name is required" in errors[0]["error"]


@pytest.mark.asyncio
async def test_validation_fail_long_ad_name_on_new(
    tmp_path,
) -> None:
    """Tests validation fails when New Ad Name exceeds 256 characters."""
    long_ad_name = "B" * 257
    csv_content = make_test_csv(ad_name=long_ad_name)
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") == "error"
    assert "Validation failed" in result.get("message", "")
    assert "validation_errors" in result
    errors = result["validation_errors"]
    assert len(errors) == 1
    expected_row = 6
    assert errors[0]["row"] == expected_row
    assert errors[0]["field"] == "Ad Name"
    assert (
        "Ad Name must be less than or equal to 256 characters"
        in errors[0]["error"]
    )


@pytest.mark.asyncio
async def test_validation_pass_ad_start_time_today(
    tmp_path,
) -> None:
    """Tests that validation passes when Ad Start Date is today."""
    today = datetime.date.today()  # ruff: ignore[call-date-today]
    today_date = today.strftime("%Y-%m-%d")
    csv_content = make_test_csv(ad_start=today_date, ad_end="8/29/3000")
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") == "SUCCESS"
    assert (
        "validation_errors" not in result
        or len(result.get("validation_errors", [])) == 0
    )


@pytest.mark.asyncio
@pytest.mark.parametrize(
    ("status", "ad_start", "ad_end", "expected_field", "expected_err"),
    [
        (
            "New",
            (
                datetime.date.today() - datetime.timedelta(days=1)  # ruff: ignore[call-date-today]
            ).strftime("%Y-%m-%d"),
            "8/29/3000",
            "Ad Start Date",
            "cannot be in the past",
        ),
        (
            "Update",
            (
                datetime.date.today() - datetime.timedelta(days=1)  # ruff: ignore[call-date-today]
            ).strftime("%Y-%m-%d"),
            "8/29/3000",
            "Ad Start Date",
            "cannot be in the past",
        ),
        (
            "New",
            "8/29/3000",
            "8/28/3000",
            "Ad End Date",
            "must be later than Ad Start Date",
        ),
        (
            "Update",
            "8/29/3000",
            "8/28/3000",
            "Ad End Date",
            "must be later than Ad Start Date",
        ),
        (
            "New",
            "invalid-date",
            "8/29/3000",
            "Ad Start Date",
            "is not a valid date",
        ),
    ],
)
async def test_validation_fail_ad_dates(  # ruff: ignore[too-many-arguments, too-many-positional-arguments]
    tmp_path,
    status: str,
    ad_start: str,
    ad_end: str,
    expected_field: str,
    expected_err: str,
) -> None:
    """Tests that ad date validation fails on invalid dates or past dates."""
    csv_content = make_test_csv(
        status=status,
        ad_start=ad_start,
        ad_end=ad_end,
        ad_id="9999" if status == "Update" else "",
        placement_id="8888" if status == "Update" else "",
    )
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") == "error"
    assert "Validation failed" in result.get("message", "")
    assert "validation_errors" in result
    errors = result["validation_errors"]
    assert len(errors) == 1
    assert errors[0]["field"] == expected_field
    assert expected_err in errors[0]["error"]


def test_validate_placement_assignment() -> None:
    """Tests validations.validate_placement_assignment helper."""
    row_missing = pd.Series({"Placement ID": "", "Placement Name": ""})
    err = validations.validate_placement_assignment(row_missing, 1)
    assert err is not None
    assert err["field"] == "Placement ID"
    assert "Placement ID is required" in err["error"]

    row_valid_name = pd.Series({"Placement Name": "Test Placement"})
    assert validations.validate_placement_assignment(row_valid_name, 1) is None

    row_valid_id = pd.Series({"Placement ID": "12345"})
    assert validations.validate_placement_assignment(row_valid_id, 1) is None


@pytest.mark.asyncio
async def test_validation_fail_missing_ad_type_on_new(
    tmp_path,
) -> None:
    """Tests that validation fails when Ad Type is missing on New status."""
    csv_content = make_test_csv(ad_type="")
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") == "error"
    assert "Validation failed" in result.get("message", "")
    assert "validation_errors" in result
    errors = result["validation_errors"]
    ad_type_error = next(e for e in errors if e["field"] == "Ad Type")
    assert "Ad Type is required" in ad_type_error["error"]


@pytest.mark.asyncio
async def test_validation_fail_forbidden_ad_type_on_new(
    tmp_path,
) -> None:
    """Tests validation fails when AD_SERVING_DEFAULT_AD is used."""
    csv_content = make_test_csv(ad_type="AD_SERVING_DEFAULT_AD")
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") == "error"
    assert "Validation failed" in result.get("message", "")
    assert "validation_errors" in result
    errors = result["validation_errors"]
    ad_type_error = next(e for e in errors if e["field"] == "Ad Type")
    assert "cannot be created directly" in ad_type_error["error"]


@pytest.mark.asyncio
@pytest.mark.parametrize("status", ["New", "Update"])
async def test_validation_fail_missing_delivery_schedule(
    tmp_path, status: str
) -> None:
    """Tests validation fails when standard ad is missing delivery schedule."""
    csv_content = make_test_csv(
        status=status,
        ad_type="AD_SERVING_STANDARD_AD",
        priority="",
        ad_id="9999" if status == "Update" else "",
        placement_id="8888" if status == "Update" else "",
    )
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") == "error"
    assert "Validation failed" in result.get("message", "")
    assert "validation_errors" in result
    errors = result["validation_errors"]
    schedule_error = next(
        e for e in errors if e["field"] == "Delivery Schedule"
    )
    assert (
        "required when Ad Type is AD_SERVING_STANDARD_AD"
        in schedule_error["error"]
    )


@pytest.mark.asyncio
async def test_validation_fail_missing_creative_identifier(
    tmp_path,
) -> None:
    """Tests that validation fails when creative name is missing."""
    csv_content = make_test_csv(creative_id="", creative_name="")
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") == "error"
    assert "Validation failed" in result.get("message", "")
    assert "validation_errors" in result
    errors = result["validation_errors"]
    creative_error = next(e for e in errors if e["field"] == "Creative Name")
    assert "Creative Name is required" in creative_error["error"]


@pytest.mark.asyncio
async def test_validation_fail_missing_click_through_url(
    tmp_path,
) -> None:
    """Tests validation fails when Final URL is missing for creative."""
    csv_content = make_test_csv(final_url="")
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)

    assert result.get("status") == "error"
    assert "Validation failed" in result.get("message", "")
    assert "validation_errors" in result
    errors = result["validation_errors"]
    url_error = next(e for e in errors if e["field"] == "Final Trafficking URL")
    assert "Final Trafficking URL is required" in url_error["error"]


def test_validate_placement_status() -> None:
    """Tests validations.validate_placement_status helper."""
    row_invalid = pd.Series({"Placement Status": "PLACEMENT_STATUS_INVALID"})
    err = validations.validate_placement_status(row_invalid, 1)
    assert err is not None
    assert err["field"] == "Placement Status"
    assert "Invalid placement status 'PLACEMENT_STATUS_INVALID'" in err["error"]

    row_valid = pd.Series({"Placement Status": "PLACEMENT_STATUS_ACTIVE"})
    assert validations.validate_placement_status(row_valid, 1) is None

    row_perm_archived = pd.Series({
        "Placement Status": "PLACEMENT_STATUS_PERMANENTLY_ARCHIVED"
    })
    assert validations.validate_placement_status(row_perm_archived, 1) is None


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
    placements = cm360_actions._group_placements(  # ruff: ignore[private-member-access]
        df, advertiser_id="123", campaign_id="456"
    )
    assert "Test Placement 1" in placements
    assert (
        placements["Test Placement 1"]["activeStatus"]
        == "PLACEMENT_STATUS_INACTIVE"
    )


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
    trafficking_helpers._resolve_placement_ids(  # ruff: ignore[private-member-access]
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
    trafficking_helpers._resolve_creative_ids(  # ruff: ignore[private-member-access]
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
        "adspace_agent.tools.cm360_trafficking.cm360_actions.build"
    ) as mock_build:
        mock_build.return_value = mock.MagicMock()
        service = cm360_trafficking._get_cm360_service(  # ruff: ignore[private-member-access]
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
        "adspace_agent.tools.cm360_trafficking.cm360_actions.build"
    ) as mock_build:
        mock_build.return_value = mock.MagicMock()
        service = cm360_trafficking._get_cm360_service(  # ruff: ignore[private-member-access]
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
        cm360_trafficking._get_cm360_service(None)  # ruff: ignore[private-member-access]


def test_get_cm360_service_missing_state() -> None:
    """Tests _get_cm360_service raises ValueError when state is None."""
    mock_context = mock.MagicMock()
    mock_context.state = None
    with pytest.raises(
        ValueError,
        match=r"Tool context and state are required to get CM360 service\.",
    ):
        cm360_trafficking._get_cm360_service(mock_context)  # ruff: ignore[private-member-access]


def test_get_cm360_service_missing_credentials_key() -> None:
    """Tests _get_cm360_service raises ValueError when cache key missing."""
    mock_context = mock.MagicMock()
    mock_context.state = {}
    with pytest.raises(
        ValueError, match="Credentials not found in tool context state"
    ):
        cm360_trafficking._get_cm360_service(mock_context)  # ruff: ignore[private-member-access]


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
        "adspace_agent.tools.cm360_trafficking.cm360_trafficking.Credentials.from_authorized_user_info",
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
        "adspace_agent.tools.cm360_trafficking.cm360_trafficking.Credentials.from_authorized_user_info",
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
@pytest.mark.parametrize(
    ("status", "tracker_val", "expected_bool"),
    [
        ("New", "TRUE", True),
        ("Update", "FALSE", False),
    ],
)
async def test_validation_click_tracker_success(
    tmp_path,
    status: str,
    tracker_val: str,
    expected_bool: bool,  # ruff: ignore[boolean-type-hint-positional-argument]
) -> None:
    """Tests that AD_SERVING_CLICK_TRACKER passes on New and Update statuses."""
    csv_content = make_test_csv(
        status=status,
        ad_id="123456" if status == "Update" else "",
        placement_id="789101" if status == "Update" else "",
        ad_type="AD_SERVING_CLICK_TRACKER",
        ad_dynamic_click_tracker=tracker_val,
    )
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)
    assert result.get("status") == "SUCCESS"

    ad_ops = [
        op
        for op in result["operations"]
        if op["operation"] == "dfareporting.ads.insert"
    ]
    assert len(ad_ops) == 1
    assert ad_ops[0]["payload"]["dynamicClickTracker"] is expected_bool
    assert "deliverySchedule" not in ad_ops[0]["payload"]


@pytest.mark.asyncio
@pytest.mark.parametrize("status", ["New", "Update"])
async def test_validation_click_tracker_fail_missing_value(
    tmp_path, status: str
) -> None:
    """Tests validation fails when click tracker is missing on New or Update."""
    csv_content = make_test_csv(
        status=status,
        ad_id="123456" if status == "Update" else "",
        placement_id="789101" if status == "Update" else "",
        ad_type="AD_SERVING_CLICK_TRACKER",
        ad_dynamic_click_tracker="",
    )
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)
    assert result.get("status") == "error"
    assert "Validation failed" in result.get("message", "")
    errors = result["validation_errors"]
    click_tracker_error = next(
        e for e in errors if e["field"] == "Ad Dynamic Click Tracker"
    )
    assert (
        "Ad Dynamic Click Tracker is required" in click_tracker_error["error"]
    )


@pytest.mark.asyncio
async def test_validation_click_tracker_fail_invalid_boolean(
    tmp_path,
) -> None:
    """Tests validation fails when click tracker is not boolean string."""
    csv_content = make_test_csv(
        ad_type="AD_SERVING_CLICK_TRACKER",
        ad_dynamic_click_tracker="1",
    )
    file_path = tmp_path / "test_sheet.csv"
    file_path.write_text(csv_content, encoding="utf-8")

    result_str = await parse_trafficking_sheet(
        typing.cast("typing.Any", MockToolContext(str(file_path)))
    )
    result = json.loads(result_str)
    assert result.get("status") == "error"
    assert "Validation failed" in result.get("message", "")
    errors = result["validation_errors"]
    click_tracker_error = next(
        e for e in errors if e["field"] == "Ad Dynamic Click Tracker"
    )
    assert "Must be 'TRUE' or 'FALSE'" in click_tracker_error["error"]


def test_validate_profile_id() -> None:
    """Tests validations.validate_profile_id with various inputs."""
    # Test with Profile ID in row
    row = pd.Series({"Profile ID": "12345"})
    assert validations.validate_profile_id(row, 1) is None

    # Test with Profile ID from parameter fallback
    row_empty = pd.Series({})
    assert (
        validations.validate_profile_id(row_empty, 1, profile_id="12345")
        is None
    )

    # Test with missing Profile ID
    err = validations.validate_profile_id(row_empty, 1)
    assert err is not None
    assert err["field"] == "Profile ID"
    assert err["error"] == "Profile ID is required."


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
            "adspace_agent.tools.cm360_trafficking.trafficking_helpers.list_cm_event_tags",
            return_value=mock_existing_tags,
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.trafficking_helpers.list_cm_ads",
            return_value=mock_existing_ads,
        ),
    ):
        ops = trafficking_helpers._resolve_and_build_operations(  # ruff: ignore[private-member-access]
            ads=ads,
            event_tags=event_tags,
            profile_id="7023449",
            advertiser_id="13641571",
            campaign_id="30535365",
            tool_context=None,
        )

    # 2 event tag operations + 2 ad operations
    expected_ops_count = 4
    assert len(ops) == expected_ops_count

    # Event tags
    existing_tag_op = next(op for op in ops if op["name"] == "Existing Tag")
    assert existing_tag_op["operation"] == "dfareporting.eventTags.patch"
    assert existing_tag_op["id"] == "tag_999"
    assert "url" in existing_tag_op["diff_fields"]
    assert existing_tag_op["payload"]["url"] == "https://example.com/tag"

    new_tag_op = next(op for op in ops if op["name"] == "New Tag")
    assert new_tag_op["operation"] == "dfareporting.eventTags.insert"
    assert "id" not in new_tag_op["payload"]

    # Ads
    existing_ad_op = next(op for op in ops if op["name"] == "Existing Ad")
    assert existing_ad_op["operation"] == "dfareporting.ads.patch"
    assert existing_ad_op["id"] == "ad_888"
    # Placements from CM360 and sheet are merged together
    placement_ids = [
        p["placementId"]
        for p in existing_ad_op["payload"]["placementAssignments"]
    ]
    assert "mock_placement_1" in placement_ids
    assert "mock_placement_2" in placement_ids
    # Event tag override resolved for existing tag
    overrides = existing_ad_op["payload"]["eventTagOverrides"]
    assert overrides[0]["id"] == "tag_999"
    assert overrides[1]["id"] == "New Tag"  # Kept as name for push resolution

    new_ad_op = next(op for op in ops if op["name"] == "New Ad")
    assert new_ad_op["operation"] == "dfareporting.ads.insert"
    assert "id" not in new_ad_op["payload"]


def test_extract_assigned_placement_ids() -> None:
    """Tests _extract_assigned_placement_ids with dict input."""
    # Test with dict of ads
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
    extracted_dict = trafficking_helpers._extract_assigned_placement_ids(  # ruff: ignore[private-member-access]
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
    patch_payload, diff_fields = trafficking_helpers._diff_placement(  # ruff: ignore[private-member-access]
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
    patch_payload, diff_fields = trafficking_helpers._diff_placement(  # ruff: ignore[private-member-access]
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
    patch_payload, diff_fields = trafficking_helpers._diff_creative(  # ruff: ignore[private-member-access]
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
    patch_payload, diff_fields = trafficking_helpers._diff_ad(  # ruff: ignore[private-member-access]
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
    patch_payload, diff_fields = trafficking_helpers._diff_event_tag(  # ruff: ignore[private-member-access]
        sheet_event_tag=sheet_tag,
        cm_event_tag=cm_tag_identical,
    )
    assert "name" not in diff_fields
    assert "name" not in patch_payload
    assert diff_fields == []
    assert patch_payload == {}

    # Test when configuration fields differ
    cm_tag_diff = {
        "name": "Different Name In CM360",
        "type": "CLICK_THROUGH_EVENT_TAG",
        "url": "https://example.com/old_url",
        "status": "DISABLED",
    }
    patch_payload, diff_fields = trafficking_helpers._diff_event_tag(  # ruff: ignore[private-member-access]
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
            "adspace_agent.tools.cm360_trafficking.trafficking_helpers.list_cm_event_tags",
            return_value=mock_existing_tags,
        ),
        mock.patch(
            "adspace_agent.tools.cm360_trafficking.trafficking_helpers.list_cm_ads",
            return_value=[],
        ),
    ):
        ops = trafficking_helpers._resolve_and_build_operations(  # ruff: ignore[private-member-access]
            ads={},
            event_tags=event_tags,
            profile_id="7023449",
            advertiser_id="13641571",
            campaign_id="30535365",
            tool_context=None,
        )
    # Event tag was identical, so no operations should be generated
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
    result = trafficking_helpers._execute_event_tag_operation(  # ruff: ignore[private-member-access]
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
    patch_payload, diff_fields = trafficking_helpers._diff_placement(  # ruff: ignore[private-member-access]
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
